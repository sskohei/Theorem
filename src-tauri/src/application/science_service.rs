use std::collections::HashMap;

use serde_json::{Value, json};
use uuid::Uuid;

use crate::agents::manager::{
    CapturedToolExecution, CollectedWorkspaceArtifact, collect_safe_workspace_artifacts,
};
use crate::agents::sandbox::IsolatedWorkspace;
use crate::application::AppState;
use crate::database::library_repo::NewAttachmentRecord;
use crate::database::science_repo::{NewToolExecution, NewVerificationResult};
use crate::database::{ai_repo, library_repo, science_repo};
use crate::domain::{
    AttachmentInfo, RunScienceToolRequest, RunScienceToolResult, SaveSolutionEditRequest,
    ScienceMcpStatus, SolutionArtifactItem, SolutionOperationResult, ToolExecutionRecord,
    VerificationResultRecord,
};
use crate::error::{AppError, AppResult};
use crate::pdf::compiler::PdfArtifactInput;
use crate::prompts::extract_json_value;
use crate::science_mcp::{
    SCIENCE_MCP_SERVER_NAME, call_science_mcp_tool, check_science_mcp_status,
};
use crate::security::normalize_relative_path;
use crate::storage::sha256_hex;

#[derive(Debug, Clone)]
pub struct PersistedPlotArtifact {
    pub workspace_rel_path: String,
    pub attachment: AttachmentInfo,
    pub caption: Option<String>,
    pub source_tool_execution_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExtractedVerificationEntry {
    pub tool_execution_id: Option<String>,
    pub source_kind: String,
    pub target_description: String,
    pub assumptions: Vec<String>,
    pub method: String,
    pub status: String,
    pub note: Option<String>,
    pub detail_json: Option<String>,
}

impl AppState {
    // ========================================================================
    // Phase 3: Science MCP Status, Direct Execution, Tool & Verification History
    // ========================================================================

    pub async fn check_science_mcp_status(&self) -> AppResult<ScienceMcpStatus> {
        let settings = self.storage.load_settings()?;
        let probe_dir = self.storage.resolve("tmp/science-mcp-probe")?;
        let status = check_science_mcp_status(&settings.science_mcp, &probe_dir).await;
        Ok(status)
    }

    pub async fn list_document_tool_executions(
        &self,
        document_id: &str,
    ) -> AppResult<Vec<ToolExecutionRecord>> {
        let db = self.db.read().await;
        science_repo::list_tool_executions_for_document(db.pool(), document_id).await
    }

    pub async fn list_solution_verifications(
        &self,
        solution_id: &str,
    ) -> AppResult<Vec<VerificationResultRecord>> {
        let db = self.db.read().await;
        science_repo::list_verification_results_for_solution(db.pool(), solution_id).await
    }

    pub async fn list_solution_artifacts(
        &self,
        solution_id: &str,
    ) -> AppResult<Vec<SolutionArtifactItem>> {
        let db = self.db.read().await;
        science_repo::list_solution_artifacts(db.pool(), solution_id).await
    }

    /// Runs a single Science MCP tool directly in a temporary isolated session workspace,
    /// persists execution + verification + generated plot artifacts into SQLite/storage,
    /// and optionally attaches generated plots to a solution PDF.
    pub async fn run_science_tool(
        &self,
        req: &RunScienceToolRequest,
    ) -> AppResult<RunScienceToolResult> {
        if req.tool_name.trim().is_empty() {
            return Err(AppError::Validation(
                "実行するツール名を指定してください".into(),
            ));
        }

        let settings = self.storage.load_settings()?;
        let ws_id = format!("direct-{}", Uuid::new_v4());
        let ws = IsolatedWorkspace::ensure(&self.storage.root().join("workspaces"), &ws_id)
            .map_err(|e| AppError::Storage(format!("一時ワークスペース作成失敗: {e}")))?;

        let started_at = chrono::Utc::now().to_rfc3339();
        let args_json = serde_json::to_string(&req.arguments)
            .map_err(|e| AppError::Validation(format!("ツール引数の JSON 変換失敗: {e}")))?;

        let call_res = call_science_mcp_tool(
            &settings.science_mcp,
            &ws.artifacts_dir,
            req.tool_name.trim(),
            req.arguments.clone(),
        )
        .await;

        let finished_at = chrono::Utc::now().to_rfc3339();

        let (result_val, status, duration_ms, error_message, hints) = match call_res {
            Ok(val) => {
                let ok = val.get("ok").and_then(Value::as_bool).unwrap_or(true);
                let err_code = val.pointer("/error/code").and_then(Value::as_str);
                let err_msg = val
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                let st = if err_code == Some("TIMEOUT") {
                    "timeout"
                } else if ok {
                    "succeeded"
                } else {
                    "failed"
                };
                let dur = val.get("duration_ms").and_then(Value::as_i64);
                let mut h = Vec::new();
                if let Some(arr) = val.get("artifacts").and_then(Value::as_array) {
                    for art in arr {
                        if let Some(rel) = art.get("relative_path").and_then(Value::as_str) {
                            h.push((
                                rel.to_string(),
                                art.get("mime_type")
                                    .and_then(Value::as_str)
                                    .unwrap_or("image/svg+xml")
                                    .to_string(),
                                art.get("caption")
                                    .and_then(Value::as_str)
                                    .map(ToString::to_string),
                                None,
                            ));
                        }
                    }
                }
                (Some(val), st.to_string(), dur, err_msg, h)
            }
            Err(err) => (
                None,
                "failed".to_string(),
                None,
                Some(err.to_string()),
                Vec::new(),
            ),
        };

        let collected = collect_safe_workspace_artifacts(&ws.artifacts_dir, &hints);
        ws.cleanup();

        let result_json = result_val.as_ref().map(Value::to_string);
        let execution = {
            let db = self.db.read().await;
            science_repo::insert_tool_execution(
                db.pool(),
                &NewToolExecution {
                    id: None,
                    agent_run_id: None,
                    document_id: Some(&req.document_id),
                    solution_id: req.solution_id.as_deref(),
                    server_name: SCIENCE_MCP_SERVER_NAME,
                    tool_name: req.tool_name.trim(),
                    arguments_json: &args_json,
                    result_json: result_json.as_deref(),
                    status: &status,
                    duration_ms,
                    error_message: error_message.as_deref(),
                    created_at: Some(&started_at),
                    finished_at: Some(&finished_at),
                },
            )
            .await?
        };

        let mut persisted_plots: Vec<PersistedPlotArtifact> = Vec::new();
        for item in &collected {
            let att = self.persist_plot_attachment(&req.document_id, item).await?;
            persisted_plots.push(PersistedPlotArtifact {
                workspace_rel_path: item.workspace_rel_path.clone(),
                attachment: att,
                caption: item.caption.clone(),
                source_tool_execution_id: Some(execution.id.clone()),
            });
        }

        let mut verification_rec: Option<VerificationResultRecord> = None;
        if let Some(ref sol_id) = req.solution_id {
            if let Some(ref val) = result_val
                && let Some(entry) =
                    extract_science_mcp_verification(val, &execution.id, &execution.tool_name)
            {
                let db = self.db.read().await;
                let saved_v = science_repo::insert_verification_result(
                    db.pool(),
                    &NewVerificationResult {
                        solution_id: sol_id,
                        tool_execution_id: Some(&execution.id),
                        source_kind: "science_mcp",
                        target_description: &entry.target_description,
                        assumptions: &entry.assumptions,
                        method: &entry.method,
                        status: &entry.status,
                        detail_json: entry.detail_json.as_deref(),
                    },
                )
                .await?;
                verification_rec = Some(saved_v);
            }

            if !persisted_plots.is_empty() {
                let db = self.db.read().await;
                for plot in &persisted_plots {
                    let _ = science_repo::insert_solution_artifact(
                        db.pool(),
                        sol_id,
                        &plot.attachment.id,
                        "plot",
                        plot.caption.as_deref(),
                        Some(&execution.id),
                    )
                    .await;
                }
            }
        }

        let mut updated_solution: Option<SolutionOperationResult> = None;
        if req.attach_to_solution_pdf
            && let Some(ref sol_id) = req.solution_id
            && (!persisted_plots.is_empty() || verification_rec.is_some())
        {
            updated_solution = Some(
                self.embed_tool_outputs_into_solution(
                    sol_id,
                    &persisted_plots,
                    verification_rec.as_ref(),
                )
                .await?,
            );
        }

        let created_artifacts = persisted_plots.into_iter().map(|p| p.attachment).collect();

        Ok(RunScienceToolResult {
            execution,
            verification: verification_rec,
            created_artifacts,
            updated_solution,
        })
    }

    /// Non-destructively attaches a saved plot/diagram attachment to a solution and recompiles PDF.
    pub async fn attach_artifact_to_solution(
        &self,
        solution_id: &str,
        attachment_id: &str,
        caption: Option<&str>,
    ) -> AppResult<SolutionOperationResult> {
        let (sol, att) = {
            let db = self.db.read().await;
            let s = ai_repo::get_solution(db.pool(), solution_id).await?;
            let a = library_repo::get_attachment(db.pool(), attachment_id).await?;
            (s, a)
        };

        let plot = PersistedPlotArtifact {
            workspace_rel_path: att.relative_path.clone(),
            attachment: att,
            caption: caption.map(ToString::to_string),
            source_tool_execution_id: None,
        };
        self.embed_tool_outputs_into_solution(&sol.id, &[plot], None)
            .await
    }

    async fn embed_tool_outputs_into_solution(
        &self,
        solution_id: &str,
        plots: &[PersistedPlotArtifact],
        verification: Option<&VerificationResultRecord>,
    ) -> AppResult<SolutionOperationResult> {
        let sol = {
            let db = self.db.read().await;
            ai_repo::get_solution(db.pool(), solution_id).await?
        };
        let mut parsed = extract_json_value(&sol.structured_json)
            .map_err(|e| AppError::Validation(format!("解答 JSON の解析に失敗しました: {e}")))?;

        if let Some(obj) = parsed.as_object_mut() {
            if !plots.is_empty() {
                let arr = obj
                    .entry("artifacts".to_string())
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Some(list) = arr.as_array_mut() {
                    for p in plots {
                        let exists = list.iter().any(|item| {
                            item.get("relative_path").and_then(Value::as_str)
                                == Some(p.attachment.relative_path.as_str())
                        });
                        if !exists {
                            list.push(json!({
                                "relative_path": p.attachment.relative_path,
                                "caption": p.caption.clone().unwrap_or_else(|| "生成グラフ".to_string()),
                                "artifact_type": "plot"
                            }));
                        }
                    }
                }
            }
            if let Some(v) = verification {
                let arr = obj
                    .entry("verification".to_string())
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Some(list) = arr.as_array_mut() {
                    list.push(json!({
                        "source_kind": "science_mcp",
                        "target": v.target_description,
                        "method": v.method,
                        "status": v.status,
                        "note": if v.assumptions.is_empty() {
                            Value::Null
                        } else {
                            Value::String(format!("仮定: {}", v.assumptions.join(", ")))
                        }
                    }));
                }
            }
        }

        let new_json = serde_json::to_string_pretty(&parsed)
            .map_err(|e| AppError::Validation(e.to_string()))?;
        let res = self
            .save_solution_edit(&SaveSolutionEditRequest {
                document_id: sol.document_id,
                problem_id: sol.problem_id,
                parent_solution_id: Some(sol.id.clone()),
                mode: sol.mode,
                structured_json: new_json,
                change_summary: Some(
                    "Science MCP 計算検証・グラフを解答 PDF へ埋め込み".to_string(),
                ),
            })
            .await?;

        // Copy / register solution_artifacts & verification_results on the newly created version
        let db = self.db.read().await;
        for p in plots {
            let _ = science_repo::insert_solution_artifact(
                db.pool(),
                &res.solution.id,
                &p.attachment.id,
                "plot",
                p.caption.as_deref(),
                p.source_tool_execution_id.as_deref(),
            )
            .await;
        }
        if let Some(v) = verification {
            let _ = science_repo::insert_verification_result(
                db.pool(),
                &NewVerificationResult {
                    solution_id: &res.solution.id,
                    tool_execution_id: v.tool_execution_id.as_deref(),
                    source_kind: &v.source_kind,
                    target_description: &v.target_description,
                    assumptions: &v.assumptions,
                    method: &v.method,
                    status: &v.status,
                    detail_json: v.detail_json.as_deref(),
                },
            )
            .await;
        }

        Ok(res)
    }

    // ========================================================================
    // Internal Helpers Used by ai_service.rs
    // ========================================================================

    pub(crate) async fn persist_plot_attachment(
        &self,
        document_id: &str,
        item: &CollectedWorkspaceArtifact,
    ) -> AppResult<AttachmentInfo> {
        let ext = if item.mime_type == "image/png" {
            "png"
        } else {
            "svg"
        };
        let att_id = Uuid::new_v4().to_string();
        let rel_path = format!("attachments/plots/{att_id}.{ext}");
        self.storage.write_atomic(&rel_path, &item.bytes)?;

        let rec = NewAttachmentRecord {
            id: att_id.clone(),
            document_id: Some(document_id.to_string()),
            kind: "plot".to_string(),
            original_filename: item.filename.clone(),
            relative_path: rel_path.clone(),
            mime_type: item.mime_type.clone(),
            byte_size: item.bytes.len() as i64,
            sha256: sha256_hex(&item.bytes),
            width: None,
            height: None,
            page_count: None,
        };

        let db = self.db.read().await;
        let mut tx = db.pool().begin().await?;
        let res = library_repo::insert_attachment_conn(&mut tx, &rec).await;
        if let Err(err) = res {
            let _ = self.storage.delete_if_exists(&rel_path);
            return Err(err);
        }
        tx.commit().await?;
        library_repo::get_attachment(db.pool(), &att_id).await
    }

    pub(crate) async fn persist_run_tools_and_plots(
        &self,
        agent_run_id: &str,
        document_id: &str,
        captured_tools: &[CapturedToolExecution],
        collected_artifacts: &[CollectedWorkspaceArtifact],
    ) -> AppResult<(Vec<ToolExecutionRecord>, Vec<PersistedPlotArtifact>)> {
        let mut call_id_to_db_id: HashMap<String, String> = HashMap::new();
        let mut saved_tools: Vec<ToolExecutionRecord> = Vec::new();

        {
            let db = self.db.read().await;
            for tool in captured_tools {
                let args_json =
                    serde_json::to_string(&tool.arguments).unwrap_or_else(|_| "{}".to_string());
                let res_json = tool.output.as_ref().map(Value::to_string);
                let rec = science_repo::insert_tool_execution(
                    db.pool(),
                    &NewToolExecution {
                        id: None,
                        agent_run_id: Some(agent_run_id),
                        document_id: Some(document_id),
                        solution_id: None,
                        server_name: &tool.server_name,
                        tool_name: &tool.tool_name,
                        arguments_json: &args_json,
                        result_json: res_json.as_deref(),
                        status: &tool.status,
                        duration_ms: tool.duration_ms,
                        error_message: tool.error_message.as_deref(),
                        created_at: Some(&tool.started_at),
                        finished_at: tool.finished_at.as_deref(),
                    },
                )
                .await?;
                call_id_to_db_id.insert(tool.call_id.clone(), rec.id.clone());
                saved_tools.push(rec);
            }
        }

        let mut persisted_plots: Vec<PersistedPlotArtifact> = Vec::new();
        for art in collected_artifacts {
            let att = self.persist_plot_attachment(document_id, art).await?;
            let source_exec_id = art
                .source_call_id
                .as_ref()
                .and_then(|cid| call_id_to_db_id.get(cid))
                .cloned();
            persisted_plots.push(PersistedPlotArtifact {
                workspace_rel_path: art.workspace_rel_path.clone(),
                attachment: att,
                caption: art.caption.clone(),
                source_tool_execution_id: source_exec_id,
            });
        }

        Ok((saved_tools, persisted_plots))
    }

    /// Loads strictly validated image artifacts (`attachments/plots/` or `attachments/diagrams/`)
    /// referenced by the solution JSON or `solution_artifacts` table for sandboxed Typst compilation.
    pub(crate) async fn load_pdf_artifacts_for_solution(
        &self,
        parsed_json: &Value,
        solution_id: Option<&str>,
    ) -> Vec<PdfArtifactInput> {
        let mut ordered_specs: Vec<(String, Option<String>, String)> = Vec::new();

        if let Some(arr) = parsed_json.get("artifacts").and_then(Value::as_array) {
            for item in arr {
                if let Some(rel) = item.get("relative_path").and_then(Value::as_str) {
                    let cap = item
                        .get("caption")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    let atype = item
                        .get("artifact_type")
                        .and_then(Value::as_str)
                        .unwrap_or("plot")
                        .to_string();
                    ordered_specs.push((rel.to_string(), cap, atype));
                }
            }
        }

        if let Some(sid) = solution_id
            && let Ok(db_arts) = self.list_solution_artifacts(sid).await
        {
            for item in db_arts {
                if !ordered_specs
                    .iter()
                    .any(|(r, _, _)| r == &item.relative_path)
                {
                    ordered_specs.push((item.relative_path, item.caption, item.artifact_type));
                }
            }
        }

        let mut inputs = Vec::new();
        for (rel_raw, caption, artifact_type) in ordered_specs {
            let Ok(norm) = normalize_relative_path(&rel_raw) else {
                continue;
            };
            if !norm.starts_with("attachments/plots/") && !norm.starts_with("attachments/diagrams/")
            {
                continue;
            }
            let ext = if norm.to_ascii_lowercase().ends_with(".svg") {
                "svg"
            } else if norm.to_ascii_lowercase().ends_with(".png") {
                "png"
            } else {
                continue;
            };
            let Ok(bytes) = self.storage.read_bytes(&norm) else {
                continue;
            };
            let virtual_path = format!("artifacts/fig_{}.{ext}", inputs.len());
            inputs.push(PdfArtifactInput {
                relative_path: norm,
                virtual_path,
                caption,
                artifact_type,
                bytes,
            });
        }
        inputs
    }
}

/// Extracts a verified `science_mcp` verification record from a real tool execution output envelope.
pub fn extract_science_mcp_verification(
    output: &Value,
    tool_execution_id: &str,
    tool_name: &str,
) -> Option<ExtractedVerificationEntry> {
    if let Some(v) = output.get("verification").and_then(Value::as_object) {
        let raw_status = v.get("status").and_then(Value::as_str).unwrap_or("error");
        let status = match raw_status {
            "passed" | "failed" | "inconclusive" | "not_run" | "error" => raw_status,
            _ => "error",
        };
        let target = v
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or(tool_name)
            .to_string();
        let method = v
            .get("method")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("science_mcp.{tool_name}"));
        let assumptions: Vec<String> = v
            .get("assumptions")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let detail_json = v.get("detail").map(Value::to_string);
        let note = if assumptions.is_empty() {
            None
        } else {
            Some(format!("仮定: {}", assumptions.join(", ")))
        };
        return Some(ExtractedVerificationEntry {
            tool_execution_id: Some(tool_execution_id.to_string()),
            source_kind: "science_mcp".to_string(),
            target_description: target,
            assumptions,
            method,
            status: status.to_string(),
            note,
            detail_json,
        });
    }
    None
}

/// Sanitizes untrusted AI solution JSON BEFORE PDF compilation and DB persistence:
/// 1. Downgrades any AI-claimed `source_kind: "science_mcp"` in raw AI JSON to `"ai_self_check"`
///    (prevents LLMs from spoofing real Science MCP verification results).
/// 2. Appends authentic verification entries backed by real `tool_executions` records.
/// 3. Rewrites workspace plot paths (`artifacts/<uuid>.svg`) to persisted managed storage paths
///    (`attachments/plots/<uuid>.svg`) and rejects unverified/traversal artifact paths.
pub fn reconcile_solution_json_with_science_outputs(
    parsed: &mut Value,
    is_user_edit: bool,
    saved_tools: &[ToolExecutionRecord],
    persisted_plots: &[PersistedPlotArtifact],
) -> Vec<ExtractedVerificationEntry> {
    let mut db_verifications: Vec<ExtractedVerificationEntry> = Vec::new();

    // 1. Collect authentic verifications from real executed Science MCP tool calls
    for tool in saved_tools {
        if tool.server_name != SCIENCE_MCP_SERVER_NAME {
            continue;
        }
        if let Some(ref raw_res) = tool.result_json
            && let Ok(val) = serde_json::from_str::<Value>(raw_res)
            && let Some(entry) = extract_science_mcp_verification(&val, &tool.id, &tool.tool_name)
        {
            db_verifications.push(entry);
        } else if tool.status == "timeout" || tool.status == "failed" {
            db_verifications.push(ExtractedVerificationEntry {
                tool_execution_id: Some(tool.id.clone()),
                source_kind: "science_mcp".to_string(),
                target_description: format!("{} 実行", tool.tool_name),
                assumptions: Vec::new(),
                method: format!("science_mcp.{}", tool.tool_name),
                status: "error".to_string(),
                note: tool.error_message.clone(),
                detail_json: tool
                    .error_message
                    .as_ref()
                    .map(|m| json!({ "error": m }).to_string()),
            });
        }
    }

    let Some(obj) = parsed.as_object_mut() else {
        return db_verifications;
    };

    // 2. Sanitize AI-authored `verification` entries & merge authentic Science MCP entries
    let mut sanitized_json_verifs: Vec<Value> = Vec::new();
    for entry in &db_verifications {
        sanitized_json_verifs.push(json!({
            "source_kind": "science_mcp",
            "target": entry.target_description,
            "method": entry.method,
            "status": entry.status,
            "note": entry.note
        }));
    }

    if let Some(existing) = obj.get("verification").and_then(Value::as_array) {
        for item in existing {
            let Some(m) = item.as_object() else {
                continue;
            };
            let raw_src = m
                .get("source_kind")
                .and_then(Value::as_str)
                .unwrap_or("ai_self_check");
            // On AI generation (not user edit), NEVER trust AI JSON claiming "science_mcp"
            let safe_src = if !is_user_edit && raw_src == "science_mcp" {
                "ai_self_check"
            } else if matches!(raw_src, "science_mcp" | "ai_self_check" | "user_check") {
                raw_src
            } else {
                "ai_self_check"
            };
            let target = m
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("検算")
                .to_string();
            let method = m
                .get("method")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            let status = m
                .get("status")
                .and_then(Value::as_str)
                .filter(|s| {
                    matches!(
                        *s,
                        "passed" | "failed" | "inconclusive" | "not_run" | "error"
                    )
                })
                .unwrap_or("not_run")
                .to_string();
            let note = m
                .get("note")
                .and_then(Value::as_str)
                .map(ToString::to_string);

            // Avoid duplicating an authentic science_mcp entry already merged on user edit
            if is_user_edit
                && safe_src == "science_mcp"
                && sanitized_json_verifs.iter().any(|v| {
                    v.get("source_kind").and_then(Value::as_str) == Some("science_mcp")
                        && v.get("target").and_then(Value::as_str) == Some(target.as_str())
                })
            {
                continue;
            }

            sanitized_json_verifs.push(json!({
                "source_kind": safe_src,
                "target": target,
                "method": method,
                "status": status,
                "note": note
            }));
            db_verifications.push(ExtractedVerificationEntry {
                tool_execution_id: None,
                source_kind: safe_src.to_string(),
                target_description: target,
                assumptions: Vec::new(),
                method: method.unwrap_or_else(|| "AI 自己確認".to_string()),
                status,
                note: note.clone(),
                detail_json: note.map(|n| json!({ "note": n }).to_string()),
            });
        }
    }
    obj.insert(
        "verification".to_string(),
        Value::Array(sanitized_json_verifs),
    );

    // 3. Reconcile `artifacts` entries: remap workspace paths -> persisted `attachments/plots/` paths
    let mut reconciled_artifacts: Vec<Value> = Vec::new();
    let mut seen_rel: Vec<String> = Vec::new();

    if let Some(existing_arts) = obj.get("artifacts").and_then(Value::as_array) {
        for item in existing_arts {
            let Some(raw_rel) = item.get("relative_path").and_then(Value::as_str) else {
                continue;
            };
            let caption = item
                .get("caption")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            let atype = item
                .get("artifact_type")
                .and_then(Value::as_str)
                .filter(|t| matches!(*t, "plot" | "diagram" | "crop" | "table"))
                .unwrap_or("plot");

            // Remap session workspace path (`artifacts/<uuid>.svg`) to persisted storage path
            if let Some(matched) = persisted_plots.iter().find(|p| {
                p.workspace_rel_path == raw_rel
                    || raw_rel.ends_with(&format!("/{}", p.attachment.original_filename))
                    || raw_rel == p.attachment.original_filename
            }) {
                let target_rel = matched.attachment.relative_path.clone();
                if !seen_rel.contains(&target_rel) {
                    seen_rel.push(target_rel.clone());
                    reconciled_artifacts.push(json!({
                        "relative_path": target_rel,
                        "caption": caption.or_else(|| matched.caption.clone()),
                        "artifact_type": atype
                    }));
                }
            } else if let Ok(norm) = normalize_relative_path(raw_rel)
                && (norm.starts_with("attachments/plots/")
                    || norm.starts_with("attachments/diagrams/"))
                && !seen_rel.contains(&norm)
            {
                seen_rel.push(norm.clone());
                reconciled_artifacts.push(json!({
                    "relative_path": norm,
                    "caption": caption,
                    "artifact_type": atype
                }));
            }
        }
    }

    // Ensure every plot generated by Science MCP during this run is attached to the solution
    for p in persisted_plots {
        if !seen_rel.contains(&p.attachment.relative_path) {
            seen_rel.push(p.attachment.relative_path.clone());
            reconciled_artifacts.push(json!({
                "relative_path": p.attachment.relative_path,
                "caption": p.caption.clone().unwrap_or_else(|| "計算ツール生成グラフ".to_string()),
                "artifact_type": "plot"
            }));
        }
    }
    obj.insert("artifacts".to_string(), Value::Array(reconciled_artifacts));

    db_verifications
}
