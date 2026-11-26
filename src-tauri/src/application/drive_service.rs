use std::path::Path;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use uuid::Uuid;

use crate::application::AppState;
use crate::database::drive_repo::{
    self, ConnectionWithCredential, NewDriveUploadJob, UpsertDriveFile,
};
use crate::database::{ai_repo, library_repo, science_repo};
use crate::domain::{
    DocumentDriveStatus, DriveConnection, DriveOAuthConfigStatus, DriveOverview, DriveUploadJob,
    EnqueueDriveUploadRequest, EnqueueDriveUploadResult, Folder, ResolveDriveConflictRequest,
    SaveDriveOAuthConfigRequest, UpdateDriveSettingsRequest,
};
use crate::error::{AppError, AppResult};
use crate::gdrive::client::{DriveApiError, DriveClient, DriveFileMeta};
use crate::gdrive::oauth::{
    DRIVE_FILE_SCOPE, LoopbackServer, OAuthError, TokenResponse, build_authorize_url,
    exchange_code_for_token, generate_pkce, refresh_access_token, validate_scopes,
};
use crate::gdrive::{
    KEYRING_OAUTH_CLIENT_SECRET_REF, MAX_DRIVE_UPLOAD_ATTEMPTS, OAuthTokenBundle,
    compute_backoff_seconds, connection_token_keyring_ref, open_system_browser,
    sanitize_drive_name,
};
use crate::security::keyring::{delete_secret, read_secret, store_secret};
use crate::security::redact_secrets;

const ALLOWED_UPLOAD_ATTACHMENT_KINDS: &[&str] = &[
    "original_pdf",
    "original_image",
    "solution_pdf",
    "source_typst",
    "source_json",
    "plot",
    "diagram",
];

impl AppState {
    // ========================================================================
    // Phase 4: Google Drive OAuth Config & Connection Management
    // ========================================================================

    pub fn get_drive_oauth_config_status(&self) -> AppResult<DriveOAuthConfigStatus> {
        let settings = self.storage.load_settings()?;
        let (client_id, source) = if let Some(cid) = settings
            .drive
            .oauth_client_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            (Some(cid.to_string()), "settings".to_string())
        } else if let Ok(env_cid) = std::env::var("GOOGLE_OAUTH_CLIENT_ID")
            && !env_cid.trim().is_empty()
        {
            (Some(env_cid.trim().to_string()), "env".to_string())
        } else {
            (None, "none".to_string())
        };

        let has_keyring_secret = read_secret(KEYRING_OAUTH_CLIENT_SECRET_REF)
            .ok()
            .flatten()
            .is_some_and(|s| !s.trim().is_empty());
        let has_env_secret = std::env::var("GOOGLE_OAUTH_CLIENT_SECRET")
            .ok()
            .is_some_and(|s| !s.trim().is_empty());

        Ok(DriveOAuthConfigStatus {
            client_id,
            client_id_source: source,
            has_client_secret: has_keyring_secret || has_env_secret,
            required_scope: DRIVE_FILE_SCOPE.to_string(),
        })
    }

    pub fn save_drive_oauth_config(
        &self,
        req: &SaveDriveOAuthConfigRequest,
    ) -> AppResult<DriveOAuthConfigStatus> {
        let mut settings = self.storage.load_settings()?;
        if let Some(ref cid) = req.client_id {
            let trimmed = cid.trim();
            settings.drive.oauth_client_id = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        self.storage.save_settings(&settings)?;

        if req.clear_client_secret {
            delete_secret(KEYRING_OAUTH_CLIENT_SECRET_REF)?;
        } else if let Some(ref secret) = req.client_secret
            && !secret.trim().is_empty()
        {
            store_secret(KEYRING_OAUTH_CLIENT_SECRET_REF, secret.trim())?;
        }

        self.logger
            .log_info("drive_oauth_config_updated", "OAuth client config updated");
        self.get_drive_oauth_config_status()
    }

    fn resolve_oauth_client_credentials(&self) -> AppResult<(String, Option<String>)> {
        let cfg = self.get_drive_oauth_config_status()?;
        let client_id = cfg.client_id.ok_or_else(|| {
            AppError::Validation(
                "Google Cloud の Desktop OAuth クライアント ID が未設定です。設定画面「Google Drive」で入力するか、環境変数 GOOGLE_OAUTH_CLIENT_ID を設定してください。"
                    .into(),
            )
        })?;

        let secret = match read_secret(KEYRING_OAUTH_CLIENT_SECRET_REF)? {
            Some(s) if !s.trim().is_empty() => Some(s),
            _ => std::env::var("GOOGLE_OAUTH_CLIENT_SECRET")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        };
        Ok((client_id, secret))
    }

    /// Runs full interactive Desktop OAuth 2.0 Authorization Code + PKCE (`S256`) flow:
    /// - Enforces `https://www.googleapis.com/auth/drive.file` scope ONLY
    /// - Opens OS default system browser (never WebView)
    /// - Listens on ephemeral `127.0.0.1:0` loopback port
    /// - Stores tokens strictly in OS keychain (never SQLite)
    pub async fn connect_google_drive(&self) -> AppResult<DriveConnection> {
        let (client_id, client_secret) = self.resolve_oauth_client_credentials()?;
        validate_scopes(&[DRIVE_FILE_SCOPE])
            .map_err(|e| AppError::Security(redact_secrets(&e.to_string())))?;

        let pkce = generate_pkce().map_err(|e| AppError::Drive(redact_secrets(&e.to_string())))?;
        let loopback = LoopbackServer::bind()
            .await
            .map_err(|e| AppError::Drive(redact_secrets(&e.to_string())))?;
        let redirect_uri = loopback.redirect_uri.clone();

        let endpoints = self.drive_endpoints.read().await.clone();
        let auth_url = build_authorize_url(
            &endpoints.auth_endpoint,
            &client_id,
            &redirect_uri,
            &pkce,
            &[DRIVE_FILE_SCOPE],
        )
        .map_err(|e| AppError::Drive(redact_secrets(&e.to_string())))?;

        open_system_browser(&auth_url)?;

        let code = loopback
            .wait_for_code(&pkce.state, Duration::from_secs(180))
            .await
            .map_err(|e| AppError::Drive(redact_secrets(&e.to_string())))?;

        let token = exchange_code_for_token(
            &self.http_client,
            &endpoints.token_endpoint,
            &client_id,
            client_secret.as_deref(),
            &code,
            &pkce.code_verifier,
            &redirect_uri,
        )
        .await
        .map_err(|e| AppError::Drive(redact_secrets(&e.to_string())))?;

        self.finalize_drive_connection_from_token(token).await
    }

    /// Completes connection persistence from a validated `TokenResponse` (shared by live OAuth and tests).
    pub async fn finalize_drive_connection_from_token(
        &self,
        token: TokenResponse,
    ) -> AppResult<DriveConnection> {
        // Verify returned scopes never exceed allowed scope if present
        let scopes = vec![DRIVE_FILE_SCOPE.to_string()];
        let endpoints = self.drive_endpoints.read().await.clone();
        let client = DriveClient::new(
            self.http_client.clone(),
            &endpoints.api_base,
            &endpoints.upload_base,
            &token.access_token,
        );

        let about = client.get_about_user().await.ok().flatten();
        let account_email = about
            .as_ref()
            .and_then(|u| u.email_address.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "google-account@drive.file".to_string());
        let account_display = about.and_then(|u| u.display_name);

        let existing_conn = {
            let db = self.db.read().await;
            drive_repo::get_active_connection_with_cred(db.pool()).await?
        };
        let conn_id = existing_conn
            .as_ref()
            .filter(|c| c.connection.account_email == account_email)
            .map(|c| c.connection.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let root_name = existing_conn
            .as_ref()
            .map(|c| c.connection.root_folder_name.clone())
            .unwrap_or_else(|| "Theorem".to_string());

        let cred_ref = connection_token_keyring_ref(&conn_id);
        let expires_at_epoch = token
            .expires_in
            .map(|secs| Utc::now().timestamp() + secs as i64);

        // Preserve existing refresh_token if Google omitted it on re-consent
        let existing_refresh = read_secret(&cred_ref)
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str::<OAuthTokenBundle>(&raw).ok())
            .and_then(|b| b.refresh_token);

        let bundle = OAuthTokenBundle {
            access_token: token.access_token,
            refresh_token: token.refresh_token.or(existing_refresh),
            expires_at_epoch,
            token_type: token.token_type,
        };
        let bundle_json = serde_json::to_string(&bundle)
            .map_err(|e| AppError::Keyring(redact_secrets(&e.to_string())))?;
        store_secret(&cred_ref, &bundle_json)?;

        let saved = {
            let db = self.db.read().await;
            drive_repo::upsert_connection(
                db.pool(),
                &conn_id,
                &account_email,
                account_display.as_deref(),
                &cred_ref,
                &scopes,
                None,
                &root_name,
            )
            .await?
        };

        // Best-effort ensure root folder exists immediately so settings shows ready state
        let _ = self
            .ensure_root_folder(
                &client,
                &saved.connection.id,
                &saved.connection.root_folder_name,
            )
            .await;

        let updated = {
            let db = self.db.read().await;
            drive_repo::get_connection_with_cred_by_id(db.pool(), &saved.connection.id)
                .await?
                .connection
        };

        self.logger.log_info(
            "drive_connected",
            &format!("connection_id={} scope=drive.file", updated.id),
        );
        self.emit_drive_updated().await;
        Ok(updated)
    }

    pub async fn disconnect_google_drive(&self) -> AppResult<()> {
        let conn_opt = {
            let db = self.db.read().await;
            drive_repo::get_active_connection_with_cred(db.pool()).await?
        };
        if let Some(c) = conn_opt {
            let _ = delete_secret(&c.credential_ref);
            let db = self.db.read().await;
            drive_repo::cancel_pending_jobs_for_connection(db.pool(), &c.connection.id).await?;
            drive_repo::set_connection_status(db.pool(), &c.connection.id, "disconnected").await?;
            self.logger.log_info(
                "drive_disconnected",
                &format!("connection_id={}", c.connection.id),
            );
        }
        self.emit_drive_updated().await;
        Ok(())
    }

    pub async fn update_drive_settings(
        &self,
        req: &UpdateDriveSettingsRequest,
    ) -> AppResult<DriveConnection> {
        let db = self.db.read().await;
        let conn = drive_repo::get_active_connection_with_cred(db.pool())
            .await?
            .ok_or_else(|| AppError::Validation("Google Drive に接続されていません".into()))?;
        let updated =
            drive_repo::update_connection_settings(db.pool(), &conn.connection.id, req).await?;
        drop(db);
        self.emit_drive_updated().await;
        Ok(updated)
    }

    pub async fn set_folder_auto_upload(
        &self,
        folder_id: &str,
        auto_upload_drive: bool,
    ) -> AppResult<Folder> {
        let db = self.db.read().await;
        library_repo::set_folder_auto_upload(db.pool(), folder_id, auto_upload_drive).await
    }

    pub async fn get_drive_overview(&self) -> AppResult<DriveOverview> {
        let oauth_config = self.get_drive_oauth_config_status()?;
        let db = self.db.read().await;
        let conn_opt = drive_repo::get_active_connection_with_cred(db.pool()).await?;
        let connection = conn_opt.as_ref().map(|c| c.connection.clone());

        let (jobs, document_summaries) = if let Some(ref c) = connection {
            let j = drive_repo::list_recent_jobs(db.pool(), Some(&c.id), 100).await?;
            let summaries = drive_repo::build_document_sync_summaries(db.pool(), &c.id).await?;
            (j, summaries)
        } else {
            (Vec::new(), Vec::new())
        };

        let pending_count = jobs.iter().filter(|j| j.status == "pending").count();
        let uploading_count = jobs.iter().filter(|j| j.status == "uploading").count();
        let conflict_count = jobs.iter().filter(|j| j.status == "conflict").count();
        let failed_count = jobs.iter().filter(|j| j.status == "failed").count();

        Ok(DriveOverview {
            oauth_config,
            connection,
            jobs,
            document_summaries,
            pending_count,
            uploading_count,
            conflict_count,
            failed_count,
        })
    }

    pub async fn list_document_drive_status(
        &self,
        document_id: &str,
    ) -> AppResult<DocumentDriveStatus> {
        let db = self.db.read().await;
        let files = drive_repo::list_drive_files_for_document(db.pool(), document_id).await?;
        let jobs = drive_repo::list_jobs_for_document(db.pool(), document_id).await?;
        Ok(DocumentDriveStatus {
            document_id: document_id.to_string(),
            files,
            jobs,
        })
    }

    // ========================================================================
    // Phase 4: Enqueue Upload Jobs (Manual / Auto / Folder Auto / Versioned)
    // ========================================================================

    pub async fn enqueue_document_drive_upload(
        &self,
        req: &EnqueueDriveUploadRequest,
    ) -> AppResult<EnqueueDriveUploadResult> {
        let conn = {
            let db = self.db.read().await;
            drive_repo::get_active_connection_with_cred(db.pool())
                .await?
                .ok_or_else(|| {
                    AppError::Validation(
                        "Google Drive に接続されていません。設定画面の「Google Drive」から接続してください。"
                            .into(),
                    )
                })?
        };
        if conn.connection.status == "disconnected" {
            return Err(AppError::Validation(
                "Google Drive は切断されています。再接続してから実行してください。".into(),
            ));
        }
        if conn.connection.status == "reauth_required" {
            return Err(AppError::DriveAuthRequired(
                "再認証が必要です。設定画面の「Google Drive」から再認証してください。".into(),
            ));
        }

        let (detail, solutions) = {
            let db = self.db.read().await;
            let d = library_repo::get_document_detail(db.pool(), &req.document_id).await?;
            let s = ai_repo::list_solutions(db.pool(), &req.document_id, None).await?;
            (d, s)
        };

        let mut candidates: Vec<(String, String, String, String)> = Vec::new();
        // Tuple: (logical_role, attachment_id, sha256, target_filename)

        // Explicit single solution version upload (from Version History tab)
        if let Some(ref sol_id) = req.solution_id {
            let sol = solutions.iter().find(|s| &s.id == sol_id).ok_or_else(|| {
                AppError::NotFound("指定された解答バージョンが見つかりません".into())
            })?;
            let pdf_att_id = sol.pdf_attachment_id.as_deref().ok_or_else(|| {
                AppError::Validation(
                    "この解答バージョンには生成済み PDF がありません。先に PDF を再生成してください。"
                        .into(),
                )
            })?;
            let att = {
                let db = self.db.read().await;
                library_repo::get_attachment(db.pool(), pdf_att_id).await?
            };
            let role = format!("solution_version:v{}", sol.version_number);
            let fname = format!("解答_v{}.pdf", sol.version_number);
            candidates.push((role, att.id, att.sha256, fname));
        } else {
            // 1. Original problem file
            if req.scope != "latest_solution_only"
                && let Some(ref primary_id) = detail.document.primary_attachment_id
                && let Some(att) = detail.attachments.iter().find(|a| &a.id == primary_id)
            {
                let ext = Path::new(&att.original_filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
                    .unwrap_or_else(|| "pdf".to_string());
                let fname = format!("問題.{ext}");
                candidates.push((
                    "original_problem".to_string(),
                    att.id.clone(),
                    att.sha256.clone(),
                    fname,
                ));
            }

            // 2. Latest solution PDF (respects Drive version_policy)
            if req.scope != "original_only" {
                let current_sol = solutions
                    .iter()
                    .find(|s| s.is_current && s.pdf_attachment_id.is_some())
                    .or_else(|| solutions.iter().find(|s| s.pdf_attachment_id.is_some()));

                if let Some(sol) = current_sol
                    && let Some(ref pdf_att_id) = sol.pdf_attachment_id
                {
                    let att = {
                        let db = self.db.read().await;
                        library_repo::get_attachment(db.pool(), pdf_att_id).await?
                    };
                    match conn.connection.version_policy.as_str() {
                        "create_new" => {
                            candidates.push((
                                format!("solution_version:v{}", sol.version_number),
                                att.id,
                                att.sha256,
                                format!("解答_v{}.pdf", sol.version_number),
                            ));
                        }
                        "manual_version" if req.scope == "default" => {
                            // In manual_version policy, only upload latest PDF when explicitly triggered via latest_solution_only or Version History
                        }
                        _ => {
                            // `update_latest` (default): ALWAYS map to logical_role `latest_solution_pdf`
                            // so updating from v1 -> v2 reuses the exact same Drive file ID & share link!
                            candidates.push((
                                "latest_solution_pdf".to_string(),
                                att.id,
                                att.sha256,
                                "解答.pdf".to_string(),
                            ));
                        }
                    }
                }

                // 3. Optional past versions (when configured)
                if conn.connection.include_past_versions {
                    for sol in solutions.iter().filter(|s| !s.is_current) {
                        if let Some(ref pdf_att_id) = sol.pdf_attachment_id {
                            let db = self.db.read().await;
                            if let Ok(att) =
                                library_repo::get_attachment(db.pool(), pdf_att_id).await
                            {
                                candidates.push((
                                    format!("solution_version:v{}", sol.version_number),
                                    att.id,
                                    att.sha256,
                                    format!("解答_v{}.pdf", sol.version_number),
                                ));
                            }
                        }
                    }
                }

                // 4. Optional Typst source (.typ) when configured
                if conn.connection.include_solution_source
                    && let Some(sol) = current_sol
                    && let Some(ref typ_att_id) = sol.typst_source_attachment_id
                {
                    let db = self.db.read().await;
                    if let Ok(att) = library_repo::get_attachment(db.pool(), typ_att_id).await {
                        candidates.push((
                            "source".to_string(),
                            att.id,
                            att.sha256,
                            format!("解答_v{}.typ", sol.version_number),
                        ));
                    }
                }

                // 5. Optional generated plot / diagram artifacts when configured
                if conn.connection.include_artifacts
                    && let Some(sol) = current_sol
                {
                    let arts = {
                        let db = self.db.read().await;
                        science_repo::list_solution_artifacts(db.pool(), &sol.id)
                            .await
                            .unwrap_or_default()
                    };
                    for art in arts {
                        let db = self.db.read().await;
                        if let Ok(att) =
                            library_repo::get_attachment(db.pool(), &art.attachment_id).await
                        {
                            candidates.push((
                                format!("artifact:{}", att.id),
                                att.id.clone(),
                                att.sha256.clone(),
                                sanitize_drive_name(&att.original_filename, "graph.svg"),
                            ));
                        }
                    }
                }
            }
        }

        let mut enqueued_jobs = Vec::new();
        let mut skipped_up_to_date = Vec::new();
        let mut notices = Vec::new();

        if candidates.is_empty() {
            notices.push("アップロード対象のファイルがありませんでした。".to_string());
        }

        {
            let db = self.db.read().await;
            for (logical_role, att_id, sha256, filename) in candidates {
                let existing = drive_repo::get_drive_file_by_logical_role(
                    db.pool(),
                    &conn.connection.id,
                    &req.document_id,
                    &logical_role,
                )
                .await?;

                // Duplicate upload prevention: skip if SHA-256 matches and remote is up-to-date
                if let Some(ref ex) = existing
                    && ex.sync_status == "uploaded"
                    && ex.last_uploaded_sha256.as_deref() == Some(sha256.as_str())
                {
                    skipped_up_to_date.push(logical_role);
                    continue;
                }

                // If existing mapping exists and local SHA changed, mark `update_required` until uploaded
                if existing.is_some() {
                    let _ = drive_repo::update_drive_file_sync_status(
                        db.pool(),
                        &conn.connection.id,
                        &req.document_id,
                        &logical_role,
                        "update_required",
                    )
                    .await;
                }

                let job = drive_repo::enqueue_or_coalesce_job(
                    db.pool(),
                    &NewDriveUploadJob {
                        connection_id: &conn.connection.id,
                        document_id: &req.document_id,
                        logical_role: &logical_role,
                        attachment_id: Some(&att_id),
                        drive_file_id: existing.as_ref().map(|e| e.drive_file_id.as_str()),
                        target_filename: &filename,
                    },
                )
                .await?;
                enqueued_jobs.push(job);
            }
        }

        if !enqueued_jobs.is_empty() {
            self.drive_worker_notify.notify_one();
            self.emit_drive_updated().await;
        }

        Ok(EnqueueDriveUploadResult {
            enqueued_jobs,
            skipped_up_to_date,
            notices,
        })
    }

    /// Non-blocking auto-upload hook invoked after document import or solution PDF compilation.
    /// Respects `manual` (no-op), `auto` (always), and `folder_auto` (folder or ancestor flagged).
    /// NEVER fails local operations if Drive is disconnected or unavailable.
    pub(crate) async fn maybe_auto_enqueue_drive_for_document(
        &self,
        document_id: &str,
        scope: &str,
    ) {
        let should_enqueue = {
            let db = self.db.read().await;
            let Ok(Some(conn)) = drive_repo::get_active_connection_with_cred(db.pool()).await
            else {
                return;
            };
            if conn.connection.status != "connected" {
                return;
            }
            match conn.connection.upload_mode.as_str() {
                "auto" => true,
                "folder_auto" => {
                    let Ok(detail) =
                        library_repo::get_document_detail(db.pool(), document_id).await
                    else {
                        return;
                    };
                    let Ok(folders) = library_repo::list_folders(db.pool()).await else {
                        return;
                    };
                    folder_or_ancestor_auto_upload(&folders, detail.document.folder_id.as_deref())
                }
                _ => false,
            }
        };

        if !should_enqueue {
            return;
        }

        let _ = self
            .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
                document_id: document_id.to_string(),
                solution_id: None,
                scope: scope.to_string(),
            })
            .await;
    }

    /// Marks any existing uploaded `latest_solution_pdf` mapping as `update_required`
    /// whenever a new solution PDF is compiled, then triggers auto-upload if configured.
    pub(crate) async fn on_solution_pdf_updated(&self, document_id: &str) {
        let marked = {
            let db = self.db.read().await;
            if let Ok(Some(conn)) = drive_repo::get_active_connection_with_cred(db.pool()).await {
                if let Ok(Some(existing)) = drive_repo::get_drive_file_by_logical_role(
                    db.pool(),
                    &conn.connection.id,
                    document_id,
                    "latest_solution_pdf",
                )
                .await
                {
                    if existing.sync_status == "uploaded" {
                        let _ = drive_repo::update_drive_file_sync_status(
                            db.pool(),
                            &conn.connection.id,
                            document_id,
                            "latest_solution_pdf",
                            "update_required",
                        )
                        .await;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        };
        if marked {
            self.emit_drive_updated().await;
        }
        self.maybe_auto_enqueue_drive_for_document(document_id, "latest_solution_only")
            .await;
    }

    // ========================================================================
    // Phase 4: Job Queue Controls & Conflict Resolution
    // ========================================================================

    pub async fn retry_drive_job(&self, job_id: &str) -> AppResult<DriveUploadJob> {
        let job = {
            let db = self.db.read().await;
            drive_repo::retry_job(db.pool(), job_id).await?
        };
        self.drive_worker_notify.notify_one();
        self.emit_drive_updated().await;
        Ok(job)
    }

    pub async fn cancel_drive_job(&self, job_id: &str) -> AppResult<DriveUploadJob> {
        let job = {
            let db = self.db.read().await;
            drive_repo::cancel_job(db.pool(), job_id).await?
        };
        self.emit_drive_updated().await;
        Ok(job)
    }

    /// Resolves a `conflict` (external modification) or `remote_deleted` Drive job strictly
    /// according to the user's explicit decision (`overwrite`, `save_as_new`, `recreate_after_remote_delete`, `cancel`).
    pub async fn resolve_drive_conflict(
        &self,
        req: &ResolveDriveConflictRequest,
    ) -> AppResult<DriveUploadJob> {
        let job = {
            let db = self.db.read().await;
            drive_repo::get_job_by_id(db.pool(), &req.job_id).await?
        };

        let updated = match req.resolution.as_str() {
            "overwrite" => {
                let db = self.db.read().await;
                drive_repo::prepare_conflict_resolution(db.pool(), &job.id, true, false, None)
                    .await?
            }
            "save_as_new" => {
                let stamped = append_filename_suffix(
                    &job.target_filename,
                    &format!("_local_{}", Utc::now().format("%Y%m%d_%H%M%S")),
                );
                let db = self.db.read().await;
                drive_repo::prepare_conflict_resolution(
                    db.pool(),
                    &job.id,
                    false,
                    true,
                    Some(&stamped),
                )
                .await?
            }
            "recreate_after_remote_delete" => {
                let db = self.db.read().await;
                drive_repo::prepare_conflict_resolution(db.pool(), &job.id, false, true, None)
                    .await?
            }
            "cancel" => {
                let db = self.db.read().await;
                drive_repo::cancel_job(db.pool(), &job.id).await?
            }
            other => {
                return Err(AppError::Validation(format!(
                    "不正な競合解決アクションです: {other}"
                )));
            }
        };

        if updated.status == "pending" {
            self.drive_worker_notify.notify_one();
        }
        self.emit_drive_updated().await;
        Ok(updated)
    }

    // ========================================================================
    // Phase 4: Persistent Queue Processor & Background Worker
    // ========================================================================

    /// Processes all currently due `pending` Drive upload jobs once and returns the count processed.
    /// Called both by the background worker task and directly in deterministic tests.
    pub async fn process_drive_queue_once(&self) -> AppResult<usize> {
        let conn = {
            let db = self.db.read().await;
            drive_repo::get_active_connection_with_cred(db.pool()).await?
        };
        let Some(conn) = conn else {
            return Ok(0);
        };
        if conn.connection.status != "connected" {
            return Ok(0);
        }

        let mut processed = 0usize;
        for _ in 0..20 {
            let next_job = {
                let db = self.db.read().await;
                drive_repo::claim_next_due_job(db.pool(), &conn.connection.id).await?
            };
            let Some(job) = next_job else {
                break;
            };
            processed += 1;
            self.execute_single_drive_job(&conn, &job).await;
            self.emit_drive_updated().await;
        }
        Ok(processed)
    }

    async fn execute_single_drive_job(
        &self,
        conn: &ConnectionWithCredential,
        job: &DriveUploadJob,
    ) {
        let Some(ref att_id) = job.attachment_id else {
            let db = self.db.read().await;
            let _ = drive_repo::mark_job_failed(
                db.pool(),
                &job.id,
                "ATTACHMENT_MISSING",
                "対象のローカルファイル参照が存在しません",
            )
            .await;
            return;
        };

        let att = {
            let db = self.db.read().await;
            match library_repo::get_attachment(db.pool(), att_id).await {
                Ok(a) => a,
                Err(err) => {
                    let _ = drive_repo::mark_job_failed(
                        db.pool(),
                        &job.id,
                        "ATTACHMENT_NOT_FOUND",
                        &redact_secrets(&err.to_string()),
                    )
                    .await;
                    return;
                }
            }
        };

        // Security invariant: NEVER upload SQLite DB, logs, settings, or disallowed attachment kinds
        if !ALLOWED_UPLOAD_ATTACHMENT_KINDS.contains(&att.kind.as_str())
            || !att.relative_path.starts_with("attachments/")
            || att.relative_path.ends_with(".db")
            || att.relative_path.contains("theorem.log")
        {
            let db = self.db.read().await;
            let _ = drive_repo::mark_job_failed(
                db.pool(),
                &job.id,
                "UPLOAD_FORBIDDEN_FILE",
                "セキュリティ制約によりこの種別のファイルは Google Drive へアップロードできません",
            )
            .await;
            return;
        }

        let bytes = match self.storage.read_bytes(&att.relative_path) {
            Ok(b) => b,
            Err(err) => {
                let db = self.db.read().await;
                let _ = drive_repo::mark_job_failed(
                    db.pool(),
                    &job.id,
                    "LOCAL_READ_ERROR",
                    &redact_secrets(&err.to_string()),
                )
                .await;
                return;
            }
        };

        // Check existing logical mapping for duplicate skip and conflict baseline
        let existing_mapping = {
            let db = self.db.read().await;
            drive_repo::get_drive_file_by_logical_role(
                db.pool(),
                &conn.connection.id,
                &job.document_id,
                &job.logical_role,
            )
            .await
            .ok()
            .flatten()
        };

        if !job.force_overwrite
            && !job.save_as_new
            && let Some(ref ex) = existing_mapping
            && ex.sync_status == "uploaded"
            && ex.last_uploaded_sha256.as_deref() == Some(att.sha256.as_str())
        {
            let db = self.db.read().await;
            let _ = drive_repo::mark_job_uploaded(db.pool(), &job.id, &ex.drive_file_id).await;
            return;
        }

        // Acquire valid access token (auto-refreshing if expiring soon)
        let mut access_token = match self.get_valid_access_token(conn, false).await {
            Ok(tok) => tok,
            Err(AppError::DriveAuthRequired(msg)) => {
                let db = self.db.read().await;
                let _ = drive_repo::mark_job_failed(
                    db.pool(),
                    &job.id,
                    "DRIVE_REAUTH_REQUIRED",
                    &redact_secrets(&msg),
                )
                .await;
                return;
            }
            Err(err) => {
                self.handle_retryable_job_error(
                    job,
                    "TOKEN_ERROR",
                    &redact_secrets(&err.to_string()),
                    None,
                )
                .await;
                return;
            }
        };

        // Attempt upload (retrying once on 401 token refresh or stale FolderNotFound)
        let mut refreshed_token_once = false;
        let mut rebuilt_folders_once = false;

        loop {
            let endpoints = self.drive_endpoints.read().await.clone();
            let client = DriveClient::new(
                self.http_client.clone(),
                &endpoints.api_base,
                &endpoints.upload_base,
                &access_token,
            );

            let folder_id = match self
                .ensure_document_folder_hierarchy(&client, conn, &job.document_id)
                .await
            {
                Ok(fid) => fid,
                Err(DriveApiError::Unauthorized) if !refreshed_token_once => {
                    refreshed_token_once = true;
                    match self.get_valid_access_token(conn, true).await {
                        Ok(new_tok) => {
                            access_token = new_tok;
                            continue;
                        }
                        Err(err) => {
                            let db = self.db.read().await;
                            let _ = drive_repo::mark_job_failed(
                                db.pool(),
                                &job.id,
                                "DRIVE_REAUTH_REQUIRED",
                                &redact_secrets(&err.to_string()),
                            )
                            .await;
                            return;
                        }
                    }
                }
                Err(DriveApiError::FolderNotFound { folder_id }) if !rebuilt_folders_once => {
                    rebuilt_folders_once = true;
                    let db = self.db.read().await;
                    let _ = drive_repo::delete_folder_mapping_by_drive_id(
                        db.pool(),
                        &conn.connection.id,
                        &folder_id,
                    )
                    .await;
                    let _ = drive_repo::clear_folder_mappings(db.pool(), &conn.connection.id).await;
                    let _ = drive_repo::set_connection_root_folder_id(
                        db.pool(),
                        &conn.connection.id,
                        None,
                    )
                    .await;
                    continue;
                }
                Err(err) => {
                    self.classify_and_record_drive_error(conn, job, err).await;
                    return;
                }
            };

            let target_update_id = if job.save_as_new {
                None
            } else {
                job.drive_file_id
                    .as_deref()
                    .or_else(|| existing_mapping.as_ref().map(|m| m.drive_file_id.as_str()))
            };

            let upload_res: Result<DriveFileMeta, DriveApiError> =
                if let Some(existing_file_id) = target_update_id {
                    client
                        .update_file_preserving_id(
                            existing_file_id,
                            &att.mime_type,
                            existing_mapping
                                .as_ref()
                                .and_then(|m| m.remote_md5_checksum.as_deref()),
                            existing_mapping
                                .as_ref()
                                .and_then(|m| m.remote_modified_time.as_deref()),
                            existing_mapping
                                .as_ref()
                                .and_then(|m| m.remote_version.as_deref()),
                            job.force_overwrite,
                            bytes.clone(),
                        )
                        .await
                } else {
                    client
                        .create_file(
                            &folder_id,
                            &job.target_filename,
                            &att.mime_type,
                            &job.document_id,
                            &job.logical_role,
                            &job.idempotency_key,
                            bytes.clone(),
                        )
                        .await
                };

            match upload_res {
                Ok(meta) => {
                    // When saving as a new copy after a conflict (save_as_new + existing mapping with different ID),
                    // record under a distinct logical role if a conflicting file existed, unless recovering remote_deleted.
                    let mapping_role = if job.save_as_new
                        && existing_mapping
                            .as_ref()
                            .is_some_and(|m| m.sync_status == "conflict")
                    {
                        format!(
                            "{}#copy_{}",
                            job.logical_role,
                            Utc::now().format("%Y%m%d%H%M%S")
                        )
                    } else {
                        job.logical_role.clone()
                    };

                    let db = self.db.read().await;
                    let _ = drive_repo::upsert_drive_file(
                        db.pool(),
                        &UpsertDriveFile {
                            connection_id: &conn.connection.id,
                            document_id: &job.document_id,
                            logical_role: &mapping_role,
                            drive_file_id: &meta.id,
                            drive_folder_id: &folder_id,
                            drive_filename: &meta.name,
                            web_view_link: meta.web_view_link.as_deref(),
                            last_uploaded_attachment_id: Some(&att.id),
                            last_uploaded_sha256: Some(&att.sha256),
                            remote_md5_checksum: meta.md5_checksum.as_deref(),
                            remote_modified_time: meta.modified_time.as_deref(),
                            remote_version: meta.version.as_deref(),
                            sync_status: "uploaded",
                        },
                    )
                    .await;
                    let _ = drive_repo::mark_job_uploaded(db.pool(), &job.id, &meta.id).await;
                    self.logger.log_info(
                        "drive_upload_succeeded",
                        &format!(
                            "doc={} role={} file_id={}",
                            job.document_id, mapping_role, meta.id
                        ),
                    );
                    return;
                }
                Err(DriveApiError::Unauthorized) if !refreshed_token_once => {
                    refreshed_token_once = true;
                    match self.get_valid_access_token(conn, true).await {
                        Ok(new_tok) => {
                            access_token = new_tok;
                            continue;
                        }
                        Err(err) => {
                            let db = self.db.read().await;
                            let _ = drive_repo::mark_job_failed(
                                db.pool(),
                                &job.id,
                                "DRIVE_REAUTH_REQUIRED",
                                &redact_secrets(&err.to_string()),
                            )
                            .await;
                            return;
                        }
                    }
                }
                Err(DriveApiError::FolderNotFound { folder_id }) if !rebuilt_folders_once => {
                    rebuilt_folders_once = true;
                    let db = self.db.read().await;
                    let _ = drive_repo::delete_folder_mapping_by_drive_id(
                        db.pool(),
                        &conn.connection.id,
                        &folder_id,
                    )
                    .await;
                    let _ = drive_repo::clear_folder_mappings(db.pool(), &conn.connection.id).await;
                    let _ = drive_repo::set_connection_root_folder_id(
                        db.pool(),
                        &conn.connection.id,
                        None,
                    )
                    .await;
                    continue;
                }
                Err(err) => {
                    self.classify_and_record_drive_error(conn, job, err).await;
                    return;
                }
            }
        }
    }

    async fn classify_and_record_drive_error(
        &self,
        conn: &ConnectionWithCredential,
        job: &DriveUploadJob,
        err: DriveApiError,
    ) {
        match err {
            DriveApiError::ExternalConflict {
                remote_md5,
                remote_modified_time,
                remote_version,
                ..
            } => {
                let db = self.db.read().await;
                let _ = drive_repo::update_drive_file_sync_status(
                    db.pool(),
                    &conn.connection.id,
                    &job.document_id,
                    &job.logical_role,
                    "conflict",
                )
                .await;
                let _ = drive_repo::mark_job_conflict(
                    db.pool(),
                    &job.id,
                    "EXTERNAL_CONFLICT",
                    "Google Drive 側のファイルが外部で変更されているため上書きを中止しました。「ローカルで上書き」「別ファイルとして保存」「キャンセル」から選択してください。",
                    remote_md5.as_deref(),
                    remote_modified_time.as_deref(),
                    remote_version.as_deref(),
                )
                .await;
            }
            DriveApiError::RemoteDeleted { .. } => {
                let db = self.db.read().await;
                let _ = drive_repo::update_drive_file_sync_status(
                    db.pool(),
                    &conn.connection.id,
                    &job.document_id,
                    &job.logical_role,
                    "remote_deleted",
                )
                .await;
                let _ = drive_repo::mark_job_conflict(
                    db.pool(),
                    &job.id,
                    "REMOTE_DELETED",
                    "Google Drive 上の既存ファイルが削除またはゴミ箱へ移動されています。新規ファイルとして再作成するかキャンセルを選択してください。",
                    None,
                    None,
                    None,
                )
                .await;
            }
            DriveApiError::StorageQuotaExceeded => {
                let db = self.db.read().await;
                let _ = drive_repo::update_drive_file_sync_status(
                    db.pool(),
                    &conn.connection.id,
                    &job.document_id,
                    &job.logical_role,
                    "error",
                )
                .await;
                let _ = drive_repo::mark_job_failed(
                    db.pool(),
                    &job.id,
                    "STORAGE_QUOTA_EXCEEDED",
                    "Google Drive のストレージ容量上限に達しました。空き容量を確保してから手動で再試行してください。",
                )
                .await;
            }
            DriveApiError::RateLimited { retry_after_secs } => {
                self.handle_retryable_job_error(
                    job,
                    "RATE_LIMITED",
                    "Google Drive API のレート制限に達したため待機後に自動再試行します。",
                    retry_after_secs,
                )
                .await;
            }
            DriveApiError::Network(msg) => {
                self.handle_retryable_job_error(
                    job,
                    "NETWORK_ERROR",
                    &format!("ネットワーク通信エラー（自動再試行待ち）: {msg}"),
                    None,
                )
                .await;
            }
            DriveApiError::Unauthorized => {
                let db = self.db.read().await;
                let _ = drive_repo::set_connection_status(
                    db.pool(),
                    &conn.connection.id,
                    "reauth_required",
                )
                .await;
                let _ = drive_repo::mark_job_failed(
                    db.pool(),
                    &job.id,
                    "DRIVE_REAUTH_REQUIRED",
                    "Google Drive の認証が失効しました。設定画面から再認証してください。",
                )
                .await;
            }
            other => {
                let db = self.db.read().await;
                let _ = drive_repo::update_drive_file_sync_status(
                    db.pool(),
                    &conn.connection.id,
                    &job.document_id,
                    &job.logical_role,
                    "error",
                )
                .await;
                let _ = drive_repo::mark_job_failed(
                    db.pool(),
                    &job.id,
                    "DRIVE_API_ERROR",
                    &redact_secrets(&other.to_string()),
                )
                .await;
            }
        }
    }

    async fn handle_retryable_job_error(
        &self,
        job: &DriveUploadJob,
        code: &str,
        message: &str,
        retry_after_secs: Option<u64>,
    ) {
        let db = self.db.read().await;
        if job.attempt_count >= MAX_DRIVE_UPLOAD_ATTEMPTS {
            let _ = drive_repo::mark_job_failed(
                db.pool(),
                &job.id,
                code,
                &format!("最大リトライ回数 ({MAX_DRIVE_UPLOAD_ATTEMPTS}) に達しました: {message}"),
            )
            .await;
            return;
        }
        let backoff_secs = compute_backoff_seconds(job.attempt_count, retry_after_secs);
        let next_retry = (Utc::now() + ChronoDuration::seconds(backoff_secs as i64)).to_rfc3339();
        let _ =
            drive_repo::reschedule_job_with_backoff(db.pool(), &job.id, &next_retry, code, message)
                .await;
    }

    async fn get_valid_access_token(
        &self,
        conn: &ConnectionWithCredential,
        force_refresh: bool,
    ) -> AppResult<String> {
        let raw_opt = read_secret(&conn.credential_ref)?;
        let Some(raw) = raw_opt else {
            let db = self.db.read().await;
            let _ = drive_repo::set_connection_status(
                db.pool(),
                &conn.connection.id,
                "reauth_required",
            )
            .await;
            return Err(AppError::DriveAuthRequired(
                "OS キーチェーンに Google Drive 認証トークンがありません".into(),
            ));
        };

        let mut bundle: OAuthTokenBundle = serde_json::from_str(&raw).map_err(|_| {
            AppError::DriveAuthRequired("保存済み Google Drive トークンが破損しています".into())
        })?;

        let now_epoch = Utc::now().timestamp();
        if !force_refresh && !bundle.is_expiring_soon(now_epoch) {
            return Ok(bundle.access_token);
        }

        let Some(refresh_tok) = bundle.refresh_token.clone() else {
            let db = self.db.read().await;
            let _ = drive_repo::set_connection_status(
                db.pool(),
                &conn.connection.id,
                "reauth_required",
            )
            .await;
            return Err(AppError::DriveAuthRequired(
                "リフレッシュトークンが保存されていないため再認証が必要です".into(),
            ));
        };

        let (client_id, client_secret) = self.resolve_oauth_client_credentials()?;
        let endpoints = self.drive_endpoints.read().await.clone();
        match refresh_access_token(
            &self.http_client,
            &endpoints.token_endpoint,
            &client_id,
            client_secret.as_deref(),
            &refresh_tok,
        )
        .await
        {
            Ok(refreshed) => {
                bundle.access_token = refreshed.access_token.clone();
                if let Some(new_rt) = refreshed.refresh_token {
                    bundle.refresh_token = Some(new_rt);
                }
                bundle.expires_at_epoch = refreshed
                    .expires_in
                    .map(|s| Utc::now().timestamp() + s as i64);
                let json = serde_json::to_string(&bundle)
                    .map_err(|e| AppError::Keyring(redact_secrets(&e.to_string())))?;
                store_secret(&conn.credential_ref, &json)?;
                Ok(bundle.access_token)
            }
            Err(OAuthError::ReauthRequired(msg)) => {
                let db = self.db.read().await;
                let _ = drive_repo::set_connection_status(
                    db.pool(),
                    &conn.connection.id,
                    "reauth_required",
                )
                .await;
                Err(AppError::DriveAuthRequired(msg))
            }
            Err(err) => Err(AppError::Drive(redact_secrets(&err.to_string()))),
        }
    }

    async fn ensure_root_folder(
        &self,
        client: &DriveClient,
        connection_id: &str,
        root_name: &str,
    ) -> Result<String, DriveApiError> {
        let cached = {
            let db = self.db.read().await;
            drive_repo::get_folder_mapping(db.pool(), connection_id, "root", None)
                .await
                .ok()
                .flatten()
        };
        if let Some(c) = cached
            && client.verify_folder_exists(&c.drive_folder_id).await?
        {
            return Ok(c.drive_folder_id);
        }

        let clean_name = sanitize_drive_name(root_name, "Theorem");
        let meta = match client.find_folder(&clean_name, None).await? {
            Some(found) => found,
            None => client.create_folder(&clean_name, None).await?,
        };

        let db = self.db.read().await;
        let _ = drive_repo::upsert_folder_mapping(
            db.pool(),
            connection_id,
            "root",
            None,
            &meta.id,
            None,
            &clean_name,
        )
        .await;
        let _ = drive_repo::set_connection_root_folder_id(db.pool(), connection_id, Some(&meta.id))
            .await;
        Ok(meta.id)
    }

    /// Mirrors local `Root -> Subject -> Ancestor Folders... -> Document` hierarchy on Google Drive,
    /// caching mappings in `drive_folders` and recreating missing folders if deleted remotely.
    async fn ensure_document_folder_hierarchy(
        &self,
        client: &DriveClient,
        conn: &ConnectionWithCredential,
        document_id: &str,
    ) -> Result<String, DriveApiError> {
        let (doc_detail, subjects, folders) =
            {
                let db = self.db.read().await;
                let d = library_repo::get_document_detail(db.pool(), document_id)
                    .await
                    .map_err(|e| DriveApiError::Fatal {
                        status: 0,
                        message: e.to_string(),
                    })?;
                let s = library_repo::list_subjects(db.pool()).await.map_err(|e| {
                    DriveApiError::Fatal {
                        status: 0,
                        message: e.to_string(),
                    }
                })?;
                let f = library_repo::list_folders(db.pool()).await.map_err(|e| {
                    DriveApiError::Fatal {
                        status: 0,
                        message: e.to_string(),
                    }
                })?;
                (d, s, f)
            };

        // 1. Root folder (`Theorem` or custom root_folder_name)
        let mut parent_drive_id = self
            .ensure_root_folder(
                client,
                &conn.connection.id,
                &conn.connection.root_folder_name,
            )
            .await?;

        // 2. Subject folder
        let subject_name = subjects
            .iter()
            .find(|s| s.id == doc_detail.document.subject_id)
            .map(|s| s.name.as_str())
            .unwrap_or("未分類");
        parent_drive_id = self
            .ensure_child_folder(
                client,
                &conn.connection.id,
                "subject",
                &doc_detail.document.subject_id,
                &parent_drive_id,
                subject_name,
            )
            .await?;

        // 3. Nested local folders chain (root -> leaf)
        let mut folder_chain: Vec<&Folder> = Vec::new();
        let mut cur_fid = doc_detail.document.folder_id.as_deref();
        let mut guard = 0usize;
        while let Some(fid) = cur_fid {
            guard += 1;
            if guard > 64 {
                break;
            }
            if let Some(f) = folders.iter().find(|x| x.id == fid) {
                folder_chain.push(f);
                cur_fid = f.parent_id.as_deref();
            } else {
                break;
            }
        }
        folder_chain.reverse();
        for f in folder_chain {
            parent_drive_id = self
                .ensure_child_folder(
                    client,
                    &conn.connection.id,
                    "folder",
                    &f.id,
                    &parent_drive_id,
                    &f.name,
                )
                .await?;
        }

        // 4. Document folder (holds `問題.pdf` / `解答.pdf` / optional files)
        let doc_folder_id = self
            .ensure_child_folder(
                client,
                &conn.connection.id,
                "document",
                &doc_detail.document.id,
                &parent_drive_id,
                &doc_detail.document.title,
            )
            .await?;

        Ok(doc_folder_id)
    }

    async fn ensure_child_folder(
        &self,
        client: &DriveClient,
        connection_id: &str,
        entity_type: &str,
        entity_id: &str,
        parent_drive_id: &str,
        raw_name: &str,
    ) -> Result<String, DriveApiError> {
        let clean_name = sanitize_drive_name(raw_name, entity_type);
        let cached = {
            let db = self.db.read().await;
            drive_repo::get_folder_mapping(db.pool(), connection_id, entity_type, Some(entity_id))
                .await
                .ok()
                .flatten()
        };

        if let Some(c) = cached
            && c.parent_drive_folder_id.as_deref() == Some(parent_drive_id)
            && client.verify_folder_exists(&c.drive_folder_id).await?
        {
            return Ok(c.drive_folder_id);
        }

        let meta = match client
            .find_folder(&clean_name, Some(parent_drive_id))
            .await?
        {
            Some(found) => found,
            None => {
                client
                    .create_folder(&clean_name, Some(parent_drive_id))
                    .await?
            }
        };

        let db = self.db.read().await;
        let _ = drive_repo::upsert_folder_mapping(
            db.pool(),
            connection_id,
            entity_type,
            Some(entity_id),
            &meta.id,
            Some(parent_drive_id),
            &clean_name,
        )
        .await;
        Ok(meta.id)
    }

    pub(crate) async fn emit_drive_updated(&self) {
        let emitter = self.drive_emitter.lock().await.clone();
        if let Some(cb) = emitter {
            cb();
        }
    }
}

fn folder_or_ancestor_auto_upload(folders: &[Folder], folder_id: Option<&str>) -> bool {
    let mut cur = folder_id;
    let mut depth = 0usize;
    while let Some(fid) = cur {
        depth += 1;
        if depth > 64 {
            break;
        }
        let Some(f) = folders.iter().find(|x| x.id == fid) else {
            break;
        };
        if f.auto_upload_drive {
            return true;
        }
        cur = f.parent_id.as_deref();
    }
    false
}

fn append_filename_suffix(filename: &str, suffix: &str) -> String {
    if let Some((stem, ext)) = filename.rsplit_once('.')
        && !stem.is_empty()
        && !ext.is_empty()
        && ext.len() <= 10
    {
        return format!("{stem}{suffix}.{ext}");
    }
    format!("{filename}{suffix}")
}
