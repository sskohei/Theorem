use std::collections::BTreeSet;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::agents::adapter::{AgentImageInput, PermissionDecision};
use crate::application::AppState;
use crate::application::science_service::{
    PersistedPlotArtifact, reconcile_solution_json_with_science_outputs,
};
use crate::database::library_repo::NewAttachmentRecord;
use crate::database::science_repo::NewVerificationResult;
use crate::database::{ai_repo, library_repo, science_repo, study_repo};
use crate::document::detect_supported_format;
use crate::domain::{
    AgentProfile, AgentStatus, ChatMessage, Conversation, GenerateSolutionRequest, Problem,
    RecognizeProblemsRequest, RecognizeProblemsResult, SaveAgentProfileRequest,
    SaveSolutionEditRequest, SchemaValidationIssue, SendChatMessageRequest, SendChatMessageResult,
    Solution, SolutionOperationResult, ToolExecutionRecord, UpsertProblemRequest,
};
use crate::error::{AppError, AppResult};
use crate::pdf::compiler::{CompilePdfMetadata, PdfCompileFailure, compile_solution_to_pdf};
use crate::prompts::{
    build_recognition_prompt, build_solution_prompt, extract_json_value,
    format_untrusted_references, recognition_schema_json, solution_schema_json,
    validate_recognition_json, validate_solution_json,
};
use crate::security::{keyring, redact_secrets};
use crate::storage::sha256_hex;

impl AppState {
    // ========================================================================
    // Agent Profiles, Detection & OS Keychain
    // ========================================================================

    pub async fn list_agent_profiles(&self) -> AppResult<Vec<AgentProfile>> {
        let db = self.db.read().await;
        ai_repo::list_agent_profiles(db.pool()).await
    }

    pub async fn save_agent_profile(
        &self,
        req: &SaveAgentProfileRequest,
    ) -> AppResult<AgentProfile> {
        let db = self.db.read().await;
        ai_repo::save_agent_profile(db.pool(), req).await
    }

    pub async fn delete_agent_profile(&self, id: &str) -> AppResult<()> {
        let db = self.db.read().await;
        ai_repo::delete_agent_profile(db.pool(), id).await
    }

    pub async fn check_agent_status(&self, profile_id: &str) -> AppResult<AgentStatus> {
        let profile = {
            let db = self.db.read().await;
            ai_repo::get_agent_profile(db.pool(), profile_id).await?
        };
        self.agents
            .check_availability(&profile)
            .await
            .map_err(|e| AppError::Agent(e.to_string()))
    }

    pub async fn detect_all_agents(&self) -> AppResult<Vec<AgentStatus>> {
        let profiles = self.list_agent_profiles().await?;
        let mut results = Vec::with_capacity(profiles.len());
        for profile in &profiles {
            if let Ok(status) = self.agents.check_availability(profile).await {
                results.push(status);
            }
        }
        Ok(results)
    }

    pub async fn set_agent_api_key(
        &self,
        profile_id: &str,
        api_key: &str,
    ) -> AppResult<AgentProfile> {
        let db = self.db.read().await;
        let profile = ai_repo::get_agent_profile(db.pool(), profile_id).await?;
        let cred_ref = format!("agent:{}:api_key", profile.id);
        keyring::store_secret(&cred_ref, api_key)?;
        let updated =
            ai_repo::set_agent_credential_ref(db.pool(), profile_id, Some(&cred_ref)).await?;
        self.logger.log_info(
            "agent_api_key_saved",
            &format!("profile_id={} stored_in_keyring=true", profile.id),
        );
        Ok(updated)
    }

    pub async fn delete_agent_api_key(&self, profile_id: &str) -> AppResult<AgentProfile> {
        let db = self.db.read().await;
        let cred_ref = ai_repo::get_agent_credential_ref(db.pool(), profile_id).await?;
        if let Some(cref) = cred_ref {
            keyring::delete_secret(&cref)?;
        }
        ai_repo::set_agent_credential_ref(db.pool(), profile_id, None).await
    }

    pub async fn cancel_agent_run(&self, run_id: &str) -> AppResult<()> {
        self.agents
            .cancel_run(run_id)
            .await
            .map_err(|e| AppError::Agent(e.to_string()))
    }

    pub async fn respond_agent_permission(
        &self,
        run_id: &str,
        request_id: &str,
        decision: PermissionDecision,
    ) -> AppResult<()> {
        self.agents
            .respond_permission(run_id, request_id, decision)
            .await
            .map_err(|e| AppError::Agent(e.to_string()))
    }

    pub(crate) async fn resolve_profile_for_purpose(
        &self,
        purpose: &str,
        override_profile_id: Option<&str>,
    ) -> AppResult<(AgentProfile, Option<String>)> {
        let settings = self.storage.load_settings()?;
        let assigned_id = override_profile_id
            .map(ToString::to_string)
            .or_else(|| match purpose {
                "extraction" => settings.agent_assignments.extraction_profile_id.clone(),
                "solution" => settings.agent_assignments.solution_profile_id.clone(),
                "chat" => settings.agent_assignments.chat_profile_id.clone(),
                "verification" => settings.agent_assignments.verification_profile_id.clone(),
                "revision" => settings.agent_assignments.revision_profile_id.clone(),
                _ => None,
            });

        let db = self.db.read().await;
        let profile = if let Some(pid) = assigned_id {
            ai_repo::get_agent_profile(db.pool(), &pid).await?
        } else {
            let all = ai_repo::list_agent_profiles(db.pool()).await?;
            all.into_iter()
                .find(|p| p.is_default && p.is_enabled)
                .ok_or_else(|| {
                    AppError::Agent(
                        "有効な AI エージェントプロファイルが設定されていません。設定画面から選択してください。"
                            .into(),
                    )
                })?
        };

        // Explicit opt-in ONLY: never read or pass API keys unless auth_mode == "api_key_keyring"
        let explicit_api_key = if profile.auth_mode == "api_key_keyring" {
            let cred_ref = ai_repo::get_agent_credential_ref(db.pool(), &profile.id).await?;
            match cred_ref {
                Some(cref) => keyring::read_secret(&cref)?,
                None => None,
            }
        } else {
            None
        };

        Ok((profile, explicit_api_key))
    }

    // ========================================================================
    // Problems & Problem Recognition
    // ========================================================================

    pub async fn list_problems(&self, document_id: &str) -> AppResult<Vec<Problem>> {
        let db = self.db.read().await;
        ai_repo::list_problems(db.pool(), document_id).await
    }

    pub async fn upsert_problem(&self, input: &UpsertProblemRequest) -> AppResult<Problem> {
        let db = self.db.read().await;
        ai_repo::upsert_problem(db.pool(), input).await
    }

    pub async fn delete_problem(&self, problem_id: &str) -> AppResult<()> {
        let db = self.db.read().await;
        ai_repo::delete_problem(db.pool(), problem_id).await
    }

    pub async fn split_problem(
        &self,
        problem_id: &str,
        first_statement: &str,
        second_statement: &str,
        second_subproblem_number: Option<&str>,
    ) -> AppResult<Vec<Problem>> {
        let db = self.db.read().await;
        ai_repo::split_problem(
            db.pool(),
            problem_id,
            first_statement,
            second_statement,
            second_subproblem_number,
        )
        .await
    }

    pub async fn merge_problems(
        &self,
        primary_id: &str,
        secondary_id: &str,
    ) -> AppResult<Vec<Problem>> {
        let db = self.db.read().await;
        ai_repo::merge_problems(db.pool(), primary_id, secondary_id).await
    }

    pub async fn recognize_problems(
        &self,
        req: &RecognizeProblemsRequest,
    ) -> AppResult<RecognizeProblemsResult> {
        let (doc_detail, subject_category) = {
            let db = self.db.read().await;
            let detail = library_repo::get_document_detail(db.pool(), &req.document_id).await?;
            let subjects = library_repo::list_subjects(db.pool()).await?;
            let cat = subjects
                .into_iter()
                .find(|s| s.id == detail.document.subject_id)
                .map(|s| s.category)
                .unwrap_or_else(|| "mathematics".to_string());
            (detail, cat)
        };

        let page_range = match (req.page_range_start, req.page_range_end) {
            (Some(start), Some(end))
                if start >= 1
                    && end >= start
                    && doc_detail
                        .document
                        .page_count
                        .is_none_or(|count| end <= count) =>
            {
                Some((start, end))
            }
            (None, None) => None,
            _ => {
                return Err(AppError::Validation(
                    "PDF のページ範囲が正しくありません".into(),
                ));
            }
        };
        let crop = req.cropped_region_png_base64.as_deref().map(str::trim);
        let mut images = Vec::new();
        let mut extracted_texts = Vec::new();
        let mut input_pages = BTreeSet::new();
        let valid_page = |number| {
            number >= 1
                && doc_detail
                    .document
                    .page_count
                    .is_none_or(|count| number <= count)
                && page_range.is_none_or(|(start, end)| (start..=end).contains(&number))
        };

        if let Some(crop) = crop {
            let Some((page, end)) = page_range else {
                return Err(AppError::Validation(
                    "切り出し元のページ番号を指定してください".into(),
                ));
            };
            if page != end || crop.is_empty() {
                return Err(AppError::Validation(
                    "切り出し画像またはページ番号が正しくありません".into(),
                ));
            }
            let bytes = STANDARD.decode(crop).map_err(|_| {
                AppError::Validation("切り出し PNG のデコードに失敗しました".into())
            })?;
            images.push(AgentImageInput {
                filename: format!("page-{page:03}-crop.png"),
                mime_type: "image/png".to_string(),
                bytes,
            });
            input_pages.insert(page);
        } else if !req.page_images.is_empty() {
            for p in &req.page_images {
                let png = p.png_base64.trim();
                let text = p
                    .extracted_text
                    .as_deref()
                    .map(str::trim)
                    .filter(|txt| !txt.is_empty());
                if !valid_page(p.page_number) || !input_pages.insert(p.page_number) {
                    return Err(AppError::Validation(
                        "認識対象のページ番号が範囲外か重複しています".into(),
                    ));
                }
                if png.is_empty() && text.is_none() {
                    return Err(AppError::Validation(format!(
                        "ページ {} に認識対象の画像またはテキストがありません",
                        p.page_number
                    )));
                }
                if !png.is_empty() {
                    let bytes = STANDARD.decode(png).map_err(|_| {
                        AppError::Validation(format!(
                            "ページ {} の PNG デコードに失敗しました",
                            p.page_number
                        ))
                    })?;
                    images.push(AgentImageInput {
                        filename: format!("page-{:03}.png", p.page_number),
                        mime_type: "image/png".to_string(),
                        bytes,
                    });
                }
                if let Some(text) = text {
                    extracted_texts.push((p.page_number, text.to_string()));
                }
            }
        } else if let Some(ref primary_id) = doc_detail.document.primary_attachment_id
            && let Some(att) = doc_detail.attachments.iter().find(|a| &a.id == primary_id)
            && att.mime_type.starts_with("image/")
        {
            let bytes = self.storage.read_bytes(&att.relative_path)?;
            if !bytes.is_empty() {
                images.push(AgentImageInput {
                    filename: att.original_filename.clone(),
                    mime_type: att.mime_type.clone(),
                    bytes,
                });
                input_pages.insert(1);
            }
        }

        if input_pages.is_empty() {
            return Err(AppError::Validation(
                "認識対象の画像またはページテキストを指定してください".into(),
            ));
        }
        if let Some((start, end)) = page_range
            && input_pages.len() as i64 != end - start + 1
        {
            return Err(AppError::Validation(
                "指定範囲の一部のページに認識入力がありません".into(),
            ));
        }

        let (profile, explicit_api_key) = self
            .resolve_profile_for_purpose("extraction", req.agent_profile_id.as_deref())
            .await?;
        let actual_range = input_pages
            .first()
            .copied()
            .zip(input_pages.last().copied());
        let prompt_text = build_recognition_prompt(
            &doc_detail.document.title,
            &subject_category,
            &req.recognition_mode,
            actual_range,
            &extracted_texts,
            crop.is_some(),
        );

        let run_id = Uuid::new_v4().to_string();
        let chosen_model = req
            .model_name
            .clone()
            .or_else(|| profile.default_model.clone());
        {
            let db = self.db.read().await;
            ai_repo::create_agent_run(
                db.pool(),
                &run_id,
                None,
                &profile.id,
                "extraction",
                Some(&req.document_id),
                None,
                chosen_model.as_deref(),
            )
            .await?;
        }

        let outcome = self
            .agents
            .run_isolated_prompt(
                &profile,
                explicit_api_key,
                &run_id,
                prompt_text,
                images,
                chosen_model,
                Some(recognition_schema_json()),
                None,
            )
            .await;

        let outcome = match outcome {
            Ok(o) => o,
            Err(err) => {
                let db = self.db.read().await;
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

        if outcome.status != "completed" {
            return Err(AppError::Agent(outcome.error_message.unwrap_or_else(
                || format!("問題認識が完了しませんでした ({})", outcome.status),
            )));
        }

        let mut parsed = match extract_json_value(&outcome.final_text) {
            Ok(v) => v,
            Err(parse_err) => {
                return Ok(RecognizeProblemsResult {
                    problems: self.list_problems(&req.document_id).await?,
                    schema_issues: vec![SchemaValidationIssue {
                        path: "$".to_string(),
                        message: parse_err,
                    }],
                    raw_output: Some(outcome.final_text),
                    run_id,
                });
            }
        };

        // Assign sequential fallback labels (`第1問`, `第2問`, ...) when an unnumbered
        // screenshot/crop returns an empty `problem_number` (never modifies problem statements).
        if let Some(arr) = parsed.get_mut("problems").and_then(Value::as_array_mut) {
            for (idx, item) in arr.iter_mut().enumerate() {
                if let Some(obj) = item.as_object_mut() {
                    let is_blank = obj
                        .get("problem_number")
                        .and_then(Value::as_str)
                        .is_none_or(|s| s.trim().is_empty());
                    if is_blank {
                        obj.insert(
                            "problem_number".to_string(),
                            Value::String(format!("第{}問", idx + 1)),
                        );
                    }
                }
            }
        }

        let schema_issues = validate_recognition_json(&parsed);
        if !schema_issues.is_empty() {
            return Ok(RecognizeProblemsResult {
                problems: self.list_problems(&req.document_id).await?,
                schema_issues,
                raw_output: Some(outcome.final_text),
                run_id,
            });
        }

        let mut items: Vec<UpsertProblemRequest> = parsed
            .get("problems")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .map(|p| UpsertProblemRequest {
                        id: None,
                        document_id: req.document_id.clone(),
                        problem_number: p
                            .get("problem_number")
                            .and_then(Value::as_str)
                            .unwrap_or("第1問")
                            .to_string(),
                        subproblem_number: p
                            .get("subproblem_number")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        subject_category: p
                            .get("subject_category")
                            .and_then(Value::as_str)
                            .unwrap_or(&subject_category)
                            .to_string(),
                        field: p
                            .get("field")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        statement_markdown: p
                            .get("statement_markdown")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        given_conditions: json_str_vec(p.get("given_conditions")),
                        target_objective: p
                            .get("target_objective")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        source_pages: json_i64_vec(p.get("source_pages")),
                        figure_refs: json_str_vec(p.get("figure_refs")),
                        uncertainties: json_str_vec(p.get("uncertainties")),
                        is_user_verified: false,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // A single supplied page establishes provenance even if the model omitted or
        // misnumbered it. For multiple pages, require provenance within the input.
        if input_pages.len() == 1 {
            for item in &mut items {
                item.source_pages = input_pages.iter().copied().collect();
            }
        } else {
            items.retain(|item| {
                item.source_pages
                    .iter()
                    .all(|page| input_pages.contains(page))
                    && (!item.source_pages.is_empty() || page_range.is_none())
            });
        }

        let saved_problems = {
            let db = self.db.read().await;
            ai_repo::add_recognized_problems(db.pool(), &req.document_id, &items).await?
        };
        self.logger.log_info(
            "problems_recognized",
            &format!(
                "document_id={} count={}",
                req.document_id,
                saved_problems.len()
            ),
        );

        Ok(RecognizeProblemsResult {
            problems: saved_problems,
            schema_issues: Vec::new(),
            raw_output: None,
            run_id,
        })
    }

    // ========================================================================
    // Solutions, Typst PDF Compilation, & Non-Destructive Versioning
    // ========================================================================

    pub async fn list_solutions(
        &self,
        document_id: &str,
        problem_id: Option<&str>,
    ) -> AppResult<Vec<Solution>> {
        let db = self.db.read().await;
        ai_repo::list_solutions(db.pool(), document_id, problem_id).await
    }

    pub async fn generate_solution(
        &self,
        req: &GenerateSolutionRequest,
    ) -> AppResult<SolutionOperationResult> {
        let (doc_detail, all_problems) = {
            let db = self.db.read().await;
            let detail = library_repo::get_document_detail(db.pool(), &req.document_id).await?;
            let probs = ai_repo::list_problems(db.pool(), &req.document_id).await?;
            (detail, probs)
        };

        let target_problems: Vec<Problem> = match req.target_scope.as_str() {
            "single_problem" => {
                let pid = req
                    .problem_id
                    .as_deref()
                    .ok_or_else(|| AppError::Validation("対象の問題を選択してください".into()))?;
                all_problems.into_iter().filter(|p| p.id == pid).collect()
            }
            "page_range" => {
                let s = req.page_range_start.unwrap_or(1);
                let e = req.page_range_end.unwrap_or(i64::MAX);
                all_problems
                    .into_iter()
                    .filter(|p| {
                        p.source_pages.is_empty()
                            || p.source_pages.iter().any(|pg| *pg >= s && *pg <= e)
                    })
                    .collect()
            }
            _ => all_problems,
        };

        if target_problems.is_empty() {
            return Err(AppError::Validation(
                "解答を生成する対象問題がありません。先に「問題認識」を実行するか問題を手動追加してください。"
                    .into(),
            ));
        }

        let settings = self.storage.load_settings()?;
        let effective_purpose = if req.mode == "verified"
            && settings.agent_assignments.verification_profile_id.is_some()
        {
            "verification"
        } else {
            "solution"
        };
        let (profile, explicit_api_key) = self
            .resolve_profile_for_purpose(effective_purpose, req.agent_profile_id.as_deref())
            .await?;

        let selected_refs = {
            let db = self.db.read().await;
            study_repo::get_reference_materials_by_ids(db.pool(), &req.reference_material_ids)
                .await?
        };

        let mut attempt_images: Vec<AgentImageInput> = Vec::new();
        for img in &req.user_attempt_images {
            let bytes = STANDARD.decode(img.base64_data.trim()).map_err(|_| {
                AppError::Validation("答案画像の Base64 デコードに失敗しました".into())
            })?;
            let fmt = detect_supported_format(&bytes, &img.filename)?;
            attempt_images.push(AgentImageInput {
                filename: img.filename.clone(),
                mime_type: fmt.mime_type().to_string(),
                bytes,
            });
        }

        let prompt_text = build_solution_prompt(
            &doc_detail.document.title,
            &target_problems,
            &req.mode,
            &settings.prompts,
            req.repair_feedback.as_deref(),
            &selected_refs,
            req.user_attempt_markdown.as_deref(),
        );

        let run_id = Uuid::new_v4().to_string();
        let chosen_model = req
            .model_name
            .clone()
            .or_else(|| profile.default_model.clone());
        let scoped_problem_id = if req.target_scope == "single_problem" {
            req.problem_id.clone()
        } else {
            None
        };

        {
            let db = self.db.read().await;
            ai_repo::create_agent_run(
                db.pool(),
                &run_id,
                None,
                &profile.id,
                if req.mode == "verified" {
                    "verification"
                } else {
                    "solution"
                },
                Some(&req.document_id),
                scoped_problem_id.as_deref(),
                chosen_model.as_deref(),
            )
            .await?;
        }

        let outcome = self
            .agents
            .run_isolated_prompt(
                &profile,
                explicit_api_key,
                &run_id,
                prompt_text,
                attempt_images,
                chosen_model.clone(),
                Some(solution_schema_json()),
                Some(&settings.science_mcp),
            )
            .await;

        let outcome = match outcome {
            Ok(o) => o,
            Err(err) => {
                let db = self.db.read().await;
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

        let (saved_tools, persisted_plots) = self
            .persist_run_tools_and_plots(
                &run_id,
                &req.document_id,
                &outcome.tool_executions,
                &outcome.collected_artifacts,
            )
            .await?;

        if outcome.status != "completed" {
            return Err(AppError::Agent(outcome.error_message.unwrap_or_else(
                || format!("解答生成が完了しませんでした ({})", outcome.status),
            )));
        }

        let prob_label = target_problems.first().map(|p| match &p.subproblem_number {
            Some(sub) => format!("{} {}", p.problem_number, sub),
            None => p.problem_number.clone(),
        });
        let prob_stmt = target_problems
            .first()
            .map(|p| p.statement_markdown.clone());

        self.validate_compile_and_persist_solution(
            &req.document_id,
            &doc_detail.document.title,
            scoped_problem_id.as_deref(),
            req.parent_solution_id.as_deref(),
            &req.mode,
            &outcome.final_text,
            false,
            Some(&run_id),
            chosen_model.as_deref(),
            req.repair_feedback
                .as_deref()
                .map(|_| "エラー修正後の再生成"),
            prob_label.as_deref(),
            prob_stmt.as_deref(),
            &saved_tools,
            &persisted_plots,
        )
        .await
    }

    pub async fn save_solution_edit(
        &self,
        req: &SaveSolutionEditRequest,
    ) -> AppResult<SolutionOperationResult> {
        let (doc_title, prob_label, prob_stmt) = {
            let db = self.db.read().await;
            let detail = library_repo::get_document_detail(db.pool(), &req.document_id).await?;
            let (lbl, stmt) = if let Some(ref pid) = req.problem_id {
                let p = ai_repo::get_problem(db.pool(), pid).await?;
                let l = match p.subproblem_number {
                    Some(sub) => format!("{} {}", p.problem_number, sub),
                    None => p.problem_number,
                };
                (Some(l), Some(p.statement_markdown))
            } else {
                (None, None)
            };
            (detail.document.title, lbl, stmt)
        };

        self.validate_compile_and_persist_solution(
            &req.document_id,
            &doc_title,
            req.problem_id.as_deref(),
            req.parent_solution_id.as_deref(),
            &req.mode,
            &req.structured_json,
            true,
            None,
            None,
            req.change_summary
                .as_deref()
                .or(Some("解答エディタで手動編集")),
            prob_label.as_deref(),
            prob_stmt.as_deref(),
            &[],
            &[],
        )
        .await
    }

    pub async fn recompile_solution_pdf(
        &self,
        solution_id: &str,
    ) -> AppResult<SolutionOperationResult> {
        let (sol, doc_title, prob_label, prob_stmt) = {
            let db = self.db.read().await;
            let s = ai_repo::get_solution(db.pool(), solution_id).await?;
            let detail = library_repo::get_document_detail(db.pool(), &s.document_id).await?;
            let (lbl, stmt) = if let Some(ref pid) = s.problem_id {
                match ai_repo::get_problem(db.pool(), pid).await {
                    Ok(p) => {
                        let l = match p.subproblem_number {
                            Some(sub) => format!("{} {}", p.problem_number, sub),
                            None => p.problem_number,
                        };
                        (Some(l), Some(p.statement_markdown))
                    }
                    Err(_) => (None, None),
                }
            } else {
                (None, None)
            };
            (s, detail.document.title, lbl, stmt)
        };

        let val = match extract_json_value(&sol.structured_json) {
            Ok(v) => v,
            Err(e) => {
                return Ok(SolutionOperationResult {
                    solution: sol,
                    schema_issues: vec![SchemaValidationIssue {
                        path: "$".to_string(),
                        message: e,
                    }],
                    latex_errors: Vec::new(),
                    pdf_error: None,
                    raw_output: None,
                });
            }
        };
        let schema_issues = validate_solution_json(&val);
        if !schema_issues.is_empty() {
            return Ok(SolutionOperationResult {
                solution: sol,
                schema_issues,
                latex_errors: Vec::new(),
                pdf_error: None,
                raw_output: None,
            });
        }

        let settings = self.storage.load_settings()?;
        let pdf_artifacts = self
            .load_pdf_artifacts_for_solution(&val, Some(&sol.id))
            .await;
        match compile_solution_to_pdf(
            &val,
            &settings.pdf,
            &CompilePdfMetadata {
                document_title: &doc_title,
                problem_label: prob_label.as_deref(),
                problem_statement: prob_stmt.as_deref(),
                version_number: sol.version_number,
                model_name: sol.model_name.as_deref(),
                generated_at: &sol.created_at,
            },
            pdf_artifacts,
        )
        .await
        {
            Ok(compiled) => {
                let (typ_id, pdf_id) = self
                    .persist_compiled_solution_files(
                        &sol.document_id,
                        sol.version_number,
                        &compiled.typst_source,
                        &compiled.pdf_bytes,
                        compiled.page_count,
                    )
                    .await?;
                let updated = {
                    let db = self.db.read().await;
                    ai_repo::update_solution_pdf_attachments(
                        db.pool(),
                        &sol.id,
                        Some(&typ_id),
                        Some(&pdf_id),
                        if sol.validation_status == "user_edited" {
                            "user_edited"
                        } else {
                            "valid"
                        },
                    )
                    .await?
                };
                self.on_solution_pdf_updated(&sol.document_id).await;
                Ok(SolutionOperationResult {
                    solution: updated,
                    schema_issues: Vec::new(),
                    latex_errors: Vec::new(),
                    pdf_error: None,
                    raw_output: None,
                })
            }
            Err(PdfCompileFailure::Latex(latex_errors)) => Ok(SolutionOperationResult {
                solution: sol,
                schema_issues: Vec::new(),
                latex_errors,
                pdf_error: None,
                raw_output: None,
            }),
            Err(PdfCompileFailure::Compile(msg)) => Ok(SolutionOperationResult {
                solution: sol,
                schema_issues: Vec::new(),
                latex_errors: Vec::new(),
                pdf_error: Some(msg),
                raw_output: None,
            }),
        }
    }

    pub async fn restore_solution_version(
        &self,
        solution_id: &str,
    ) -> AppResult<SolutionOperationResult> {
        let orig = {
            let db = self.db.read().await;
            ai_repo::get_solution(db.pool(), solution_id).await?
        };
        let req = SaveSolutionEditRequest {
            document_id: orig.document_id,
            problem_id: orig.problem_id,
            parent_solution_id: Some(orig.id),
            mode: orig.mode,
            structured_json: orig.structured_json,
            change_summary: Some(format!("v{} から非破壊復元", orig.version_number)),
        };
        self.save_solution_edit(&req).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn validate_compile_and_persist_solution(
        &self,
        document_id: &str,
        document_title: &str,
        problem_id: Option<&str>,
        parent_solution_id: Option<&str>,
        mode: &str,
        raw_json_text: &str,
        is_user_edit: bool,
        agent_run_id: Option<&str>,
        model_name: Option<&str>,
        change_summary: Option<&str>,
        prob_label: Option<&str>,
        prob_stmt: Option<&str>,
        saved_tools: &[ToolExecutionRecord],
        persisted_plots: &[PersistedPlotArtifact],
    ) -> AppResult<SolutionOperationResult> {
        let mut parsed = match extract_json_value(raw_json_text) {
            Ok(v) => v,
            Err(parse_err) => {
                // Persist invalid version so user can inspect/fix in Solution Editor without losing AI output
                let db = self.db.read().await;
                let saved = ai_repo::create_solution_version(
                    db.pool(),
                    &ai_repo::NewSolutionVersion {
                        document_id,
                        problem_id,
                        parent_solution_id,
                        mode,
                        structured_json: raw_json_text,
                        validation_status: "invalid",
                        typst_source_attachment_id: None,
                        pdf_attachment_id: None,
                        agent_run_id,
                        model_name,
                        change_summary,
                    },
                )
                .await?;
                return Ok(SolutionOperationResult {
                    solution: saved,
                    schema_issues: vec![SchemaValidationIssue {
                        path: "$".to_string(),
                        message: parse_err,
                    }],
                    latex_errors: Vec::new(),
                    pdf_error: None,
                    raw_output: Some(raw_json_text.to_string()),
                });
            }
        };

        // Reconcile AI JSON with real Science MCP tool verifications & persisted plot paths,
        // strictly downgrading any AI-spoofed `source_kind: "science_mcp"` to `"ai_self_check"`.
        let extracted_verifications = reconcile_solution_json_with_science_outputs(
            &mut parsed,
            is_user_edit,
            saved_tools,
            persisted_plots,
        );

        let pretty_json =
            serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| raw_json_text.to_string());
        let schema_issues = validate_solution_json(&parsed);
        if !schema_issues.is_empty() {
            let db = self.db.read().await;
            let saved = ai_repo::create_solution_version(
                db.pool(),
                &ai_repo::NewSolutionVersion {
                    document_id,
                    problem_id,
                    parent_solution_id,
                    mode,
                    structured_json: &pretty_json,
                    validation_status: "invalid",
                    typst_source_attachment_id: None,
                    pdf_attachment_id: None,
                    agent_run_id,
                    model_name,
                    change_summary,
                },
            )
            .await?;
            return Ok(SolutionOperationResult {
                solution: saved,
                schema_issues,
                latex_errors: Vec::new(),
                pdf_error: None,
                raw_output: Some(pretty_json),
            });
        }

        // Determine next version number for PDF header metadata
        let next_ver = {
            let db = self.db.read().await;
            let existing = ai_repo::list_solutions(db.pool(), document_id, problem_id).await?;
            existing
                .iter()
                .filter(|s| s.problem_id.as_deref() == problem_id)
                .map(|s| s.version_number)
                .max()
                .unwrap_or(0)
                + 1
        };

        let settings = self.storage.load_settings()?;
        let now_str = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
        let pdf_artifacts = self.load_pdf_artifacts_for_solution(&parsed, None).await;

        match compile_solution_to_pdf(
            &parsed,
            &settings.pdf,
            &CompilePdfMetadata {
                document_title,
                problem_label: prob_label,
                problem_statement: prob_stmt,
                version_number: next_ver,
                model_name,
                generated_at: &now_str,
            },
            pdf_artifacts,
        )
        .await
        {
            Ok(compiled) => {
                let (typ_id, pdf_id) = self
                    .persist_compiled_solution_files(
                        document_id,
                        next_ver,
                        &compiled.typst_source,
                        &compiled.pdf_bytes,
                        compiled.page_count,
                    )
                    .await?;

                let db = self.db.read().await;
                let saved = ai_repo::create_solution_version(
                    db.pool(),
                    &ai_repo::NewSolutionVersion {
                        document_id,
                        problem_id,
                        parent_solution_id,
                        mode,
                        structured_json: &pretty_json,
                        validation_status: if is_user_edit { "user_edited" } else { "valid" },
                        typst_source_attachment_id: Some(&typ_id),
                        pdf_attachment_id: Some(&pdf_id),
                        agent_run_id,
                        model_name,
                        change_summary,
                    },
                )
                .await?;

                for v in &extracted_verifications {
                    let _ = science_repo::insert_verification_result(
                        db.pool(),
                        &NewVerificationResult {
                            solution_id: &saved.id,
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
                for plot in persisted_plots {
                    let _ = science_repo::insert_solution_artifact(
                        db.pool(),
                        &saved.id,
                        &plot.attachment.id,
                        "plot",
                        plot.caption.as_deref(),
                        plot.source_tool_execution_id.as_deref(),
                    )
                    .await;
                }
                if let Some(rid) = agent_run_id {
                    let _ = sqlx::query(
                        "UPDATE tool_executions SET solution_id = ?1 WHERE agent_run_id = ?2",
                    )
                    .bind(&saved.id)
                    .bind(rid)
                    .execute(db.pool())
                    .await;
                }

                drop(db);
                self.logger.log_info(
                    "solution_pdf_compiled",
                    &format!(
                        "solution_id={} version={} pages={}",
                        saved.id, saved.version_number, compiled.page_count
                    ),
                );
                self.on_solution_pdf_updated(document_id).await;

                Ok(SolutionOperationResult {
                    solution: saved,
                    schema_issues: Vec::new(),
                    latex_errors: Vec::new(),
                    pdf_error: None,
                    raw_output: None,
                })
            }
            Err(PdfCompileFailure::Latex(latex_errors)) => {
                let db = self.db.read().await;
                let saved = ai_repo::create_solution_version(
                    db.pool(),
                    &ai_repo::NewSolutionVersion {
                        document_id,
                        problem_id,
                        parent_solution_id,
                        mode,
                        structured_json: &pretty_json,
                        validation_status: "invalid",
                        typst_source_attachment_id: None,
                        pdf_attachment_id: None,
                        agent_run_id,
                        model_name,
                        change_summary,
                    },
                )
                .await?;
                Ok(SolutionOperationResult {
                    solution: saved,
                    schema_issues: Vec::new(),
                    latex_errors,
                    pdf_error: None,
                    raw_output: None,
                })
            }
            Err(PdfCompileFailure::Compile(msg)) => {
                let db = self.db.read().await;
                let saved = ai_repo::create_solution_version(
                    db.pool(),
                    &ai_repo::NewSolutionVersion {
                        document_id,
                        problem_id,
                        parent_solution_id,
                        mode,
                        structured_json: &pretty_json,
                        validation_status: "invalid",
                        typst_source_attachment_id: None,
                        pdf_attachment_id: None,
                        agent_run_id,
                        model_name,
                        change_summary,
                    },
                )
                .await?;
                Ok(SolutionOperationResult {
                    solution: saved,
                    schema_issues: Vec::new(),
                    latex_errors: Vec::new(),
                    pdf_error: Some(msg),
                    raw_output: None,
                })
            }
        }
    }

    async fn persist_compiled_solution_files(
        &self,
        document_id: &str,
        version_number: i64,
        typst_source: &str,
        pdf_bytes: &[u8],
        page_count: usize,
    ) -> AppResult<(String, String)> {
        let typ_id = Uuid::new_v4().to_string();
        let typ_rel = format!("sources/{typ_id}.typ");
        let pdf_id = Uuid::new_v4().to_string();
        let pdf_rel = format!("attachments/solutions/{pdf_id}.pdf");

        self.storage
            .write_atomic(&typ_rel, typst_source.as_bytes())?;
        if let Err(err) = self.storage.write_atomic(&pdf_rel, pdf_bytes) {
            let _ = self.storage.delete_if_exists(&typ_rel);
            return Err(err);
        }

        let db = self.db.read().await;
        let mut tx = db.pool().begin().await?;
        let typ_res = library_repo::insert_attachment_conn(
            &mut tx,
            &NewAttachmentRecord {
                id: typ_id.clone(),
                document_id: Some(document_id.to_string()),
                kind: "source_typst".to_string(),
                original_filename: format!("solution_v{version_number}.typ"),
                relative_path: typ_rel.clone(),
                mime_type: "text/plain; charset=utf-8".to_string(),
                byte_size: typst_source.len() as i64,
                sha256: sha256_hex(typst_source.as_bytes()),
                width: None,
                height: None,
                page_count: None,
            },
        )
        .await;

        let pdf_res = library_repo::insert_attachment_conn(
            &mut tx,
            &NewAttachmentRecord {
                id: pdf_id.clone(),
                document_id: Some(document_id.to_string()),
                kind: "solution_pdf".to_string(),
                original_filename: format!("解答_v{version_number}.pdf"),
                relative_path: pdf_rel.clone(),
                mime_type: "application/pdf".to_string(),
                byte_size: pdf_bytes.len() as i64,
                sha256: sha256_hex(pdf_bytes),
                width: None,
                height: None,
                page_count: Some(page_count as i64),
            },
        )
        .await;

        if let Err(err) = typ_res.and(pdf_res) {
            let _ = self.storage.delete_if_exists(&typ_rel);
            let _ = self.storage.delete_if_exists(&pdf_rel);
            return Err(err);
        }

        tx.commit().await?;
        Ok((typ_id, pdf_id))
    }

    // ========================================================================
    // AI Chat, Conversations, History & Session Resume / Replay Fallback
    // ========================================================================

    pub async fn list_conversations(&self, document_id: &str) -> AppResult<Vec<Conversation>> {
        let db = self.db.read().await;
        ai_repo::list_conversations(db.pool(), document_id).await
    }

    pub async fn create_conversation(
        &self,
        document_id: &str,
        problem_id: Option<&str>,
        solution_id: Option<&str>,
        title: &str,
    ) -> AppResult<Conversation> {
        let db = self.db.read().await;
        ai_repo::create_conversation(db.pool(), document_id, problem_id, solution_id, title).await
    }

    pub async fn delete_conversation(&self, conversation_id: &str) -> AppResult<()> {
        self.agents
            .close_conversation_sessions(conversation_id)
            .await;
        let db = self.db.read().await;
        ai_repo::delete_conversation(db.pool(), conversation_id).await
    }

    pub async fn list_messages(&self, conversation_id: &str) -> AppResult<Vec<ChatMessage>> {
        let db = self.db.read().await;
        ai_repo::list_messages(db.pool(), conversation_id).await
    }

    pub async fn send_chat_message(
        &self,
        req: &SendChatMessageRequest,
    ) -> AppResult<SendChatMessageResult> {
        if req.content_markdown.trim().is_empty() && req.attached_images.is_empty() {
            return Err(AppError::Validation(
                "メッセージまたは画像を入力してください".into(),
            ));
        }

        let run_id = match req.run_id.as_deref() {
            Some(id) => Uuid::parse_str(id)
                .ok()
                .filter(|uuid| uuid.to_string() == id && !uuid.is_nil())
                .ok_or_else(|| AppError::Validation("チャット実行 ID が正しくありません".into()))?
                .to_string(),
            None => Uuid::new_v4().to_string(),
        };
        {
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
        }

        let (profile, explicit_api_key) = self
            .resolve_profile_for_purpose("chat", req.agent_profile_id.as_deref())
            .await?;

        // Save chat images safely into managed storage (never mutating external paths)
        let mut attachment_ids: Vec<String> = Vec::new();
        let mut agent_images: Vec<AgentImageInput> = Vec::new();
        for img in &req.attached_images {
            let bytes = STANDARD.decode(img.base64_data.trim()).map_err(|_| {
                AppError::Validation("チャット添付画像のデコードに失敗しました".into())
            })?;
            let fmt = detect_supported_format(&bytes, &img.filename)?;
            let att_id = Uuid::new_v4().to_string();
            let rel = format!("attachments/originals/{att_id}.{}", fmt.extension());
            self.storage.write_atomic(&rel, &bytes)?;

            let db = self.db.read().await;
            let mut tx = db.pool().begin().await?;
            library_repo::insert_attachment_conn(
                &mut tx,
                &NewAttachmentRecord {
                    id: att_id.clone(),
                    document_id: Some(req.document_id.clone()),
                    kind: "chat_image".to_string(),
                    original_filename: img.filename.clone(),
                    relative_path: rel,
                    mime_type: fmt.mime_type().to_string(),
                    byte_size: bytes.len() as i64,
                    sha256: sha256_hex(&bytes),
                    width: None,
                    height: None,
                    page_count: None,
                },
            )
            .await?;
            tx.commit().await?;

            attachment_ids.push(att_id);
            agent_images.push(AgentImageInput {
                filename: img.filename.clone(),
                mime_type: fmt.mime_type().to_string(),
                bytes,
            });
        }

        let chosen_model = req
            .model_name
            .clone()
            .or_else(|| profile.default_model.clone());

        // Fetch history, resumable session, and optional selective context before inserting new messages
        let (prior_messages, stored_session, context_block) = {
            let db = self.db.read().await;
            let msgs = ai_repo::list_messages(db.pool(), &req.conversation_id).await?;
            let sess =
                ai_repo::find_resumable_session(db.pool(), &req.conversation_id, &profile.id)
                    .await?;

            let mut ctx = String::new();
            if req.include_problem_context {
                if let Some(ref pid) = req.referenced_problem_id {
                    if let Ok(p) = ai_repo::get_problem(db.pool(), pid).await {
                        ctx.push_str(&format!(
                            "[参照問題 {}]\n{}\n\n",
                            p.problem_number, p.statement_markdown
                        ));
                    }
                } else if let Ok(probs) = ai_repo::list_problems(db.pool(), &req.document_id).await
                    && !probs.is_empty()
                {
                    ctx.push_str("[対象問題一覧]\n");
                    for p in probs.iter().take(5) {
                        ctx.push_str(&format!(
                            "- {}: {}\n",
                            p.problem_number,
                            p.statement_markdown.chars().take(400).collect::<String>()
                        ));
                    }
                    ctx.push('\n');
                }
            }
            if req.include_solution_context
                && let Ok(sols) = ai_repo::list_solutions(
                    db.pool(),
                    &req.document_id,
                    req.referenced_problem_id.as_deref(),
                )
                .await
                && let Some(cur) = sols.into_iter().find(|s| s.is_current)
            {
                let snippet: String = cur.structured_json.chars().take(2500).collect();
                ctx.push_str(&format!(
                    "[現在の解答 v{}]\n```json\n{}\n```\n\n",
                    cur.version_number, snippet
                ));
            }
            if !req.reference_material_ids.is_empty()
                && let Ok(refs) = study_repo::get_reference_materials_by_ids(
                    db.pool(),
                    &req.reference_material_ids,
                )
                .await
            {
                ctx.push_str(&format_untrusted_references(&refs));
                ctx.push('\n');
            }
            (msgs, sess, ctx)
        };

        let (user_msg, assistant_msg) = {
            let db = self.db.read().await;
            ai_repo::create_agent_run(
                db.pool(),
                &run_id,
                stored_session.as_ref().map(|s| s.id.as_str()),
                &profile.id,
                "chat",
                Some(&req.document_id),
                req.referenced_problem_id.as_deref(),
                chosen_model.as_deref(),
            )
            .await?;

            let u_msg = ai_repo::insert_message(
                db.pool(),
                &req.conversation_id,
                "user",
                req.content_markdown.trim(),
                "completed",
                None,
                req.referenced_problem_id.as_deref(),
                &attachment_ids,
            )
            .await?;

            let a_msg = ai_repo::insert_message(
                db.pool(),
                &req.conversation_id,
                "assistant",
                "",
                "streaming",
                Some(&run_id),
                req.referenced_problem_id.as_deref(),
                &[],
            )
            .await?;
            (u_msg, a_msg)
        };

        // If no resumable session exists for this agent (e.g. first turn, crashed, or switched agent),
        // replay minimal recent SQLite conversation history cleanly.
        let mut prompt_text = String::new();
        prompt_text.push_str(
            "あなたは大学数学・大学物理の学習を支援する対話 AI です。数式は標準 LaTeX（$...$ / $$...$$）で記述し、不要なシェル実行は行わないでください。\n\n",
        );
        if !context_block.is_empty() {
            prompt_text.push_str(&context_block);
        }
        if stored_session.is_none() && !prior_messages.is_empty() {
            prompt_text.push_str("[これまでの会話履歴（SQLite 引き継ぎ）]\n");
            let start = prior_messages.len().saturating_sub(10);
            for m in &prior_messages[start..] {
                let role_lbl = if m.role == "user" {
                    "ユーザー"
                } else {
                    "AI"
                };
                let body: String = m.content_markdown.chars().take(800).collect();
                prompt_text.push_str(&format!("{role_lbl}: {body}\n"));
            }
            prompt_text.push('\n');
        }
        prompt_text.push_str("[今回の質問]\n");
        prompt_text.push_str(req.content_markdown.trim());

        let settings = self.storage.load_settings()?;
        let outcome = self
            .agents
            .run_chat_turn(
                &profile,
                explicit_api_key,
                &req.conversation_id,
                stored_session
                    .as_ref()
                    .and_then(|s| s.external_session_id.clone()),
                &run_id,
                &assistant_msg.id,
                prompt_text,
                agent_images,
                chosen_model,
                Some(&settings.science_mcp),
            )
            .await;

        match outcome {
            Ok(out) => {
                let _ = self
                    .persist_run_tools_and_plots(
                        &run_id,
                        &req.document_id,
                        &out.tool_executions,
                        &out.collected_artifacts,
                    )
                    .await;
                let db = self.db.read().await;
                let final_body = if out.final_text.trim().is_empty() && out.status == "failed" {
                    out.error_message
                        .clone()
                        .unwrap_or_else(|| "応答生成中にエラーが発生しました。".to_string())
                } else {
                    out.final_text.clone()
                };
                ai_repo::update_message_content(
                    db.pool(),
                    &assistant_msg.id,
                    &final_body,
                    &out.status,
                )
                .await?;
                let session_status = if out.error_code.as_deref() == Some("PROCESS_CRASHED") {
                    "crashed"
                } else {
                    "active"
                };
                ai_repo::upsert_agent_session(
                    db.pool(),
                    &out.internal_session_id,
                    &profile.id,
                    Some(&req.conversation_id),
                    out.external_session_id.as_deref(),
                    &out.workspace_rel_path,
                    session_status,
                    out.resumable,
                )
                .await?;
                ai_repo::finish_agent_run(
                    db.pool(),
                    &run_id,
                    &out.status,
                    out.usage_json.as_deref(),
                    out.error_code.as_deref(),
                    out.error_message.as_deref(),
                )
                .await?;

                let mut final_assistant = assistant_msg;
                final_assistant.content_markdown = final_body;
                final_assistant.status = out.status;
                final_assistant.updated_at = Utc::now().to_rfc3339();

                Ok(SendChatMessageResult {
                    run_id,
                    user_message: user_msg,
                    assistant_message: final_assistant,
                    resumed_external_session: out.resumed_external_session,
                })
            }
            Err(err) => {
                let err_text = redact_secrets(&err.to_string());
                let db = self.db.read().await;
                let _ = ai_repo::update_message_content(
                    db.pool(),
                    &assistant_msg.id,
                    &err_text,
                    "failed",
                )
                .await;
                let _ = ai_repo::finish_agent_run(
                    db.pool(),
                    &run_id,
                    "failed",
                    None,
                    Some(err.error_code()),
                    Some(&err_text),
                )
                .await;
                Err(AppError::Agent(err_text))
            }
        }
    }
}

fn json_str_vec(val: Option<&Value>) -> Vec<String> {
    val.and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn json_i64_vec(val: Option<&Value>) -> Vec<i64> {
    val.and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}
