use serde_json::Value;
use uuid::Uuid;

use crate::application::AppState;
use crate::database::{ai_repo, library_repo, study_repo};
use crate::domain::{
    AgentComparisonEntry, CompareAgentsRequest, CompareAgentsResult,
    CreateReferenceMaterialRequest, DecideSolutionRevisionRequest, DecideSolutionRevisionResult,
    FullTextSearchHit, FullTextSearchRequest, GenerateSolutionRequest,
    ProposeSolutionRevisionRequest, ReferenceMaterial, SaveSolutionEditRequest,
    SendChatMessageResult, SolutionRevisionProposal, SolutionVersionDiff,
};
use crate::error::{AppError, AppResult};
use crate::prompts::diff::{apply_approved_hunks, build_version_diff, compute_solution_diff};
use crate::prompts::{
    build_revision_prompt, extract_json_value, solution_schema_json, validate_solution_json,
};
use crate::security::redact_secrets;

impl AppState {
    // ========================================================================
    // Phase 5: Reference Materials (Untrusted Context Library)
    // ========================================================================

    pub async fn list_reference_materials(
        &self,
        document_id: Option<&str>,
        subject_id: Option<&str>,
    ) -> AppResult<Vec<ReferenceMaterial>> {
        let db = self.db.read().await;
        study_repo::list_reference_materials(db.pool(), document_id, subject_id).await
    }

    pub async fn create_reference_material(
        &self,
        req: &CreateReferenceMaterialRequest,
    ) -> AppResult<ReferenceMaterial> {
        let db = self.db.read().await;
        let created = study_repo::create_reference_material(db.pool(), req, None).await?;
        self.logger.log_info(
            "reference_material_created",
            &format!("ref_id={} kind={}", created.id, created.kind),
        );
        Ok(created)
    }

    pub async fn delete_reference_material(&self, id: &str) -> AppResult<()> {
        let db = self.db.read().await;
        study_repo::delete_reference_material(db.pool(), id).await
    }

    // ========================================================================
    // Phase 5: SQLite FTS5 Cross-Entity Full-Text Search
    // ========================================================================

    pub async fn full_text_search(
        &self,
        req: &FullTextSearchRequest,
    ) -> AppResult<Vec<FullTextSearchHit>> {
        let db = self.db.read().await;
        study_repo::full_text_search(db.pool(), req).await
    }

    pub async fn rebuild_search_index(&self) -> AppResult<usize> {
        let db = self.db.read().await;
        study_repo::rebuild_search_index(db.pool()).await
    }

    // ========================================================================
    // Phase 5: Structured Solution Version Diff
    // ========================================================================

    pub async fn compare_solution_versions(
        &self,
        base_solution_id: &str,
        target_solution_id: &str,
    ) -> AppResult<SolutionVersionDiff> {
        let (base_sol, target_sol) = {
            let db = self.db.read().await;
            let b = ai_repo::get_solution(db.pool(), base_solution_id).await?;
            let t = ai_repo::get_solution(db.pool(), target_solution_id).await?;
            (b, t)
        };
        let base_val: Value = serde_json::from_str(&base_sol.structured_json)
            .map_err(|e| AppError::Validation(format!("比較元 JSON 解析エラー: {e}")))?;
        let target_val: Value = serde_json::from_str(&target_sol.structured_json)
            .map_err(|e| AppError::Validation(format!("比較先 JSON 解析エラー: {e}")))?;

        Ok(build_version_diff(
            &base_sol.id,
            base_sol.version_number,
            &base_val,
            &target_sol.id,
            target_sol.version_number,
            &target_val,
        ))
    }

    // ========================================================================
    // Phase 5: Chat-Driven Partial Solution Revision & Approval Workflow
    // ========================================================================

    /// Proposes a strictly scoped partial revision to `base_solution_id` from a user chat instruction.
    /// NEVER auto-applies changes: persists a `pending` diff proposal on the assistant chat message
    /// so the user must explicitly inspect and approve (or reject) hunks.
    pub async fn propose_solution_revision(
        &self,
        req: &ProposeSolutionRevisionRequest,
    ) -> AppResult<SendChatMessageResult> {
        let instruction = req.instruction.trim();
        if instruction.is_empty() {
            return Err(AppError::Validation(
                "解答への修正指示を入力してください".into(),
            ));
        }

        let (doc_detail, base_solution, target_problem, references) = {
            let db = self.db.read().await;
            let belongs_to_document: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1 AND document_id = ?2)",
            )
            .bind(&req.conversation_id)
            .bind(&req.document_id)
            .fetch_one(db.pool())
            .await?;
            if !belongs_to_document {
                return Err(AppError::Validation(
                    "会話が対象ドキュメントに属していません".into(),
                ));
            }
            let detail = library_repo::get_document_detail(db.pool(), &req.document_id).await?;
            let base = ai_repo::get_solution(db.pool(), &req.base_solution_id).await?;
            if base.document_id != req.document_id {
                return Err(AppError::Validation(
                    "指定された解答バージョンは現在のドキュメントに属していません".into(),
                ));
            }
            let prob = match base.problem_id.as_deref() {
                Some(pid) => ai_repo::get_problem(db.pool(), pid).await.ok(),
                None => None,
            };
            let refs =
                study_repo::get_reference_materials_by_ids(db.pool(), &req.reference_material_ids)
                    .await?;
            (detail, base, prob, refs)
        };

        let base_val: Value = serde_json::from_str(&base_solution.structured_json)
            .map_err(|e| AppError::Validation(format!("現行解答 JSON 解析エラー: {e}")))?;

        let settings = self.storage.load_settings()?;
        let (profile, explicit_api_key) = self
            .resolve_profile_for_purpose("revision", req.agent_profile_id.as_deref())
            .await?;

        let prompt_text = build_revision_prompt(
            &doc_detail.document.title,
            &base_solution,
            target_problem.as_ref(),
            instruction,
            &references,
        );

        let run_id = Uuid::new_v4().to_string();
        let chosen_model = req
            .model_name
            .clone()
            .or_else(|| profile.default_model.clone());

        let (user_msg, assistant_msg) = {
            let db = self.db.read().await;
            ai_repo::create_agent_run(
                db.pool(),
                &run_id,
                None,
                &profile.id,
                "revision",
                Some(&req.document_id),
                base_solution.problem_id.as_deref(),
                chosen_model.as_deref(),
            )
            .await?;

            let u = ai_repo::insert_message(
                db.pool(),
                &req.conversation_id,
                "user",
                &format!(
                    "【解答部分修正依頼 (v{})】\n{}",
                    base_solution.version_number, instruction
                ),
                "completed",
                None,
                base_solution.problem_id.as_deref(),
                &[],
            )
            .await?;

            let a = ai_repo::insert_message(
                db.pool(),
                &req.conversation_id,
                "assistant",
                "",
                "streaming",
                Some(&run_id),
                base_solution.problem_id.as_deref(),
                &[],
            )
            .await?;
            (u, a)
        };

        let outcome = self
            .agents
            .run_isolated_prompt(
                &profile,
                explicit_api_key,
                &run_id,
                prompt_text,
                Vec::new(),
                chosen_model.clone(),
                Some(solution_schema_json()),
                Some(&settings.science_mcp),
            )
            .await;

        let outcome = match outcome {
            Ok(o) => o,
            Err(err) => {
                let db = self.db.read().await;
                let msg_err = format!("修正案の生成に失敗しました: {err}");
                let _ = ai_repo::update_message_content(
                    db.pool(),
                    &assistant_msg.id,
                    &msg_err,
                    "failed",
                )
                .await;
                let _ = ai_repo::finish_agent_run(
                    db.pool(),
                    &run_id,
                    "failed",
                    None,
                    Some(err.error_code()),
                    Some(&redact_secrets(&err.to_string())),
                )
                .await;
                return Err(AppError::Agent(err.to_string()));
            }
        };

        {
            let db = self.db.read().await;
            let _ = ai_repo::finish_agent_run(
                db.pool(),
                &run_id,
                &outcome.status,
                outcome.usage_json.as_deref(),
                outcome.error_code.as_deref(),
                outcome.error_message.as_deref(),
            )
            .await;
        }

        let mut proposed_val = extract_json_value(&outcome.final_text).map_err(|parse_err| {
            AppError::Validation(format!(
                "AI が返却した修正案 JSON を解析できませんでした: {parse_err}"
            ))
        })?;

        // Scope lock: strictly preserve target problem_id & schema_version so other problems are never mutated
        if let Some(obj) = proposed_val.as_object_mut() {
            obj.insert("schema_version".to_string(), Value::from(1));
            if let Some(ref pid) = base_solution.problem_id {
                obj.insert("problem_id".to_string(), Value::from(pid.clone()));
            }
        }

        let schema_issues = validate_solution_json(&proposed_val);
        if !schema_issues.is_empty() {
            let summary = schema_issues
                .iter()
                .map(|i| format!("{}: {}", i.path, i.message))
                .collect::<Vec<_>>()
                .join("; ");
            let db = self.db.read().await;
            let _ = ai_repo::update_message_content(
                db.pool(),
                &assistant_msg.id,
                &format!("修正案が Schema v1 検証を満たしませんでした: {summary}"),
                "failed",
            )
            .await;
            return Err(AppError::Validation(format!(
                "修正案が Schema v1 検証を満たしませんでした: {summary}"
            )));
        }

        let hunks = compute_solution_diff(&base_val, &proposed_val);
        let proposed_pretty = serde_json::to_string_pretty(&proposed_val)
            .unwrap_or_else(|_| proposed_val.to_string());

        let proposal = SolutionRevisionProposal {
            base_solution_id: base_solution.id.clone(),
            base_version_number: base_solution.version_number,
            target_problem_id: base_solution.problem_id.clone(),
            summary: format!(
                "v{} に対する修正案（{} 箇所の変更）：{}",
                base_solution.version_number,
                hunks.len(),
                instruction.chars().take(80).collect::<String>()
            ),
            proposed_json: proposed_pretty,
            hunks,
            applied_solution_id: None,
            applied_version_number: None,
        };

        let assistant_text = if proposal.hunks.is_empty() {
            format!(
                "v{} の現行解答と差分はありませんでした（変更提案なし）。",
                base_solution.version_number
            )
        } else {
            format!(
                "v{} に対する **{} 件の差分提案** を作成しました。下の差分カードで内容を確認し、承認した項目のみ新しいバージョンとして適用してください（承認まで既存解答は変更されません）。",
                base_solution.version_number,
                proposal.hunks.len()
            )
        };

        let saved_assistant = {
            let db = self.db.read().await;
            ai_repo::update_message_content(
                db.pool(),
                &assistant_msg.id,
                &assistant_text,
                "completed",
            )
            .await?;
            ai_repo::update_message_revision(db.pool(), &assistant_msg.id, &proposal, "pending")
                .await?
        };

        self.logger.log_info(
            "solution_revision_proposed",
            &format!(
                "document_id={} base_solution_id={} hunks={}",
                req.document_id,
                base_solution.id,
                proposal.hunks.len()
            ),
        );

        Ok(SendChatMessageResult {
            user_message: user_msg,
            assistant_message: saved_assistant,
            run_id,
            resumed_external_session: false,
        })
    }

    /// Approves (full or partial hunk selection) or rejects a pending chat revision proposal.
    /// Rejecting leaves all persisted solutions 100% untouched; approving creates a non-destructive
    /// new version (`vN+1`) strictly within the base solution's problem scope and recompiles PDF.
    pub async fn decide_solution_revision(
        &self,
        req: &DecideSolutionRevisionRequest,
    ) -> AppResult<DecideSolutionRevisionResult> {
        let decision = req.decision.trim();
        if !matches!(decision, "approve" | "reject") {
            return Err(AppError::Validation(format!(
                "decision は 'approve' または 'reject' を指定してください: {decision}"
            )));
        }

        let msg = {
            let db = self.db.read().await;
            ai_repo::get_message(db.pool(), &req.message_id).await?
        };
        let mut proposal = msg.revision_patch.clone().ok_or_else(|| {
            AppError::Validation("このメッセージには解答修正提案が含まれていません".into())
        })?;

        if msg.revision_status.as_deref() != Some("pending") {
            return Err(AppError::Validation(format!(
                "この修正提案はすでに処理済みです（現在状態: {}）",
                msg.revision_status.as_deref().unwrap_or("unknown")
            )));
        }

        if decision == "reject" {
            let updated_msg = {
                let db = self.db.read().await;
                ai_repo::update_message_revision(db.pool(), &msg.id, &proposal, "rejected").await?
            };
            self.logger.log_info(
                "solution_revision_rejected",
                &format!(
                    "message_id={} base_solution_id={}",
                    msg.id, proposal.base_solution_id
                ),
            );
            return Ok(DecideSolutionRevisionResult {
                message: updated_msg,
                new_solution: None,
            });
        }

        // Approve flow: validate scope and apply ONLY approved hunks onto base_solution
        let base_solution = {
            let db = self.db.read().await;
            ai_repo::get_solution(db.pool(), &proposal.base_solution_id).await?
        };
        if base_solution.problem_id != proposal.target_problem_id {
            return Err(AppError::Validation(
                "修正対象の問題スコープが一致しないため適用を中止しました".into(),
            ));
        }

        if let Some(ref ids) = req.approved_hunk_ids
            && ids.is_empty()
        {
            return Err(AppError::Validation(
                "適用する差分項目を 1 つ以上選択してください（すべて不要な場合は「拒否」を選択してください）".into(),
            ));
        }

        let base_val: Value = serde_json::from_str(&base_solution.structured_json)
            .map_err(|e| AppError::Validation(format!("現行解答 JSON 解析エラー: {e}")))?;
        let proposed_val: Value = serde_json::from_str(&proposal.proposed_json)
            .map_err(|e| AppError::Validation(format!("提案 JSON 解析エラー: {e}")))?;

        let merged_val =
            apply_approved_hunks(&base_val, &proposed_val, req.approved_hunk_ids.as_deref());
        let merged_json = serde_json::to_string_pretty(&merged_val)
            .map_err(|e| AppError::Database(e.to_string()))?;

        let approved_count = req
            .approved_hunk_ids
            .as_ref()
            .map(Vec::len)
            .unwrap_or(proposal.hunks.len());

        // Non-destructive new version creation + Typst PDF recompile + Drive auto-update hook
        let new_sol_res = self
            .save_solution_edit(&SaveSolutionEditRequest {
                document_id: base_solution.document_id.clone(),
                problem_id: base_solution.problem_id.clone(),
                parent_solution_id: Some(base_solution.id.clone()),
                mode: base_solution.mode.clone(),
                structured_json: merged_json,
                change_summary: Some(format!(
                    "チャット差分承認 ({approved_count}/{} 箇所適用): {}",
                    proposal.hunks.len(),
                    proposal.summary
                )),
            })
            .await?;

        proposal.applied_solution_id = Some(new_sol_res.solution.id.clone());
        proposal.applied_version_number = Some(new_sol_res.solution.version_number);

        let updated_msg = {
            let db = self.db.read().await;
            ai_repo::update_message_revision(db.pool(), &msg.id, &proposal, "approved").await?
        };

        self.logger.log_info(
            "solution_revision_approved",
            &format!(
                "message_id={} base={} new_solution={} new_version=v{}",
                msg.id,
                base_solution.id,
                new_sol_res.solution.id,
                new_sol_res.solution.version_number
            ),
        );

        Ok(DecideSolutionRevisionResult {
            message: updated_msg,
            new_solution: Some(new_sol_res),
        })
    }

    // ========================================================================
    // Phase 5: User-Triggered Multi-Agent Solution Comparison
    // ========================================================================

    /// Generates solutions using 2..=4 user-selected enabled & compliant AI profiles
    /// and computes a structured diff between the first two successful solutions.
    /// Strictly user-triggered (never autonomous loops); profiles blocked by provider
    /// terms (e.g. Claude Code `official_harness`) are honestly reported as errors.
    pub async fn compare_agents_generate_solutions(
        &self,
        req: &CompareAgentsRequest,
    ) -> AppResult<CompareAgentsResult> {
        if req.profile_ids.len() < 2 {
            return Err(AppError::Validation(
                "複数 AI 比較には 2 つ以上のエージェントプロファイルを選択してください".into(),
            ));
        }
        if req.profile_ids.len() > 4 {
            return Err(AppError::Validation(
                "同時比較できるエージェントは最大 4 つまでです".into(),
            ));
        }

        let mut entries: Vec<AgentComparisonEntry> = Vec::with_capacity(req.profile_ids.len());
        let mut successful_solutions: Vec<(String, i64, String)> = Vec::new();

        for pid in &req.profile_ids {
            let profile = {
                let db = self.db.read().await;
                ai_repo::get_agent_profile(db.pool(), pid).await
            };
            let profile = match profile {
                Ok(p) => p,
                Err(e) => {
                    entries.push(AgentComparisonEntry {
                        profile_id: pid.clone(),
                        profile_name: "不明なプロファイル".to_string(),
                        provider_kind: "unknown".to_string(),
                        model_name: None,
                        solution_result: None,
                        error: Some(e.to_string()),
                    });
                    continue;
                }
            };

            // Explicit terms/compliance gate before invoking any provider
            if profile.provider_kind == "claude_code" && profile.auth_mode == "official_harness" {
                entries.push(AgentComparisonEntry {
                    profile_id: profile.id,
                    profile_name: profile.name,
                    provider_kind: profile.provider_kind,
                    model_name: profile.default_model,
                    solution_result: None,
                    error: Some(
                        "利用規約保護のため Claude Code の official_harness 比較は無効です（明示的同意による api_key_keyring のみ利用可能）。"
                            .to_string(),
                    ),
                });
                continue;
            }

            let gen_req = GenerateSolutionRequest {
                document_id: req.document_id.clone(),
                problem_id: req.problem_id.clone(),
                agent_profile_id: Some(profile.id.clone()),
                model_name: profile.default_model.clone(),
                mode: req.mode.clone(),
                target_scope: if req.problem_id.is_some() {
                    "single_problem".to_string()
                } else {
                    "all_problems".to_string()
                },
                page_range_start: None,
                page_range_end: None,
                repair_feedback: None,
                parent_solution_id: None,
                reference_material_ids: req.reference_material_ids.clone(),
                user_attempt_markdown: req.user_attempt_markdown.clone(),
                user_attempt_images: Vec::new(),
            };

            match self.generate_solution(&gen_req).await {
                Ok(sol_res) => {
                    successful_solutions.push((
                        sol_res.solution.id.clone(),
                        sol_res.solution.version_number,
                        sol_res.solution.structured_json.clone(),
                    ));
                    entries.push(AgentComparisonEntry {
                        profile_id: profile.id,
                        profile_name: profile.name,
                        provider_kind: profile.provider_kind,
                        model_name: sol_res.solution.model_name.clone(),
                        solution_result: Some(sol_res),
                        error: None,
                    });
                }
                Err(err) => {
                    entries.push(AgentComparisonEntry {
                        profile_id: profile.id,
                        profile_name: profile.name,
                        provider_kind: profile.provider_kind,
                        model_name: profile.default_model,
                        solution_result: None,
                        error: Some(err.to_string()),
                    });
                }
            }
        }

        let diff_between_first_two = if successful_solutions.len() >= 2 {
            let (id_a, ver_a, json_a) = &successful_solutions[0];
            let (id_b, ver_b, json_b) = &successful_solutions[1];
            if let (Ok(va), Ok(vb)) = (
                serde_json::from_str::<Value>(json_a),
                serde_json::from_str::<Value>(json_b),
            ) {
                Some(build_version_diff(id_a, *ver_a, &va, id_b, *ver_b, &vb))
            } else {
                None
            }
        } else {
            None
        };

        Ok(CompareAgentsResult {
            entries,
            diff_between_first_two,
        })
    }
}
