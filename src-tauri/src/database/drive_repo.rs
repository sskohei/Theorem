use std::collections::HashMap;

use chrono::Utc;
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use crate::domain::{
    DriveConnection, DriveDocumentSyncSummary, DriveFileMapping, DriveUploadJob,
    UpdateDriveSettingsRequest,
};
use crate::error::{AppError, AppResult};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

/// Resets any `uploading` Drive jobs left over from a crash or unexpected app termination
/// back to `pending` so they safely recover using `idempotency_key` (`appProperties`).
pub async fn recover_interrupted_drive_jobs(pool: &Pool<Sqlite>) -> AppResult<()> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE drive_upload_jobs
         SET status = 'pending',
             next_retry_at = NULL,
             last_error_code = COALESCE(last_error_code, 'APP_RESTART_RECOVERY'),
             last_error_message = COALESCE(last_error_message, '前回終了時にアップロード途中だったため安全に再試行します'),
             updated_at = ?1
         WHERE status = 'uploading'",
    )
    .bind(&ts)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Connections
// ============================================================================

#[derive(Debug, Clone)]
pub struct ConnectionWithCredential {
    pub connection: DriveConnection,
    pub credential_ref: String,
}

pub async fn get_active_connection_with_cred(
    pool: &Pool<Sqlite>,
) -> AppResult<Option<ConnectionWithCredential>> {
    let row = sqlx::query(
        "SELECT id, account_email, account_display_name, credential_ref, scopes_json,
                root_folder_drive_id, root_folder_name, upload_mode, version_policy,
                include_past_versions, include_solution_source, include_artifacts,
                status, created_at, updated_at
         FROM drive_connections
         ORDER BY CASE status WHEN 'connected' THEN 0 WHEN 'reauth_required' THEN 1 ELSE 2 END,
                  updated_at DESC
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| row_to_connection_with_cred(&r)))
}

pub async fn get_connection_with_cred_by_id(
    pool: &Pool<Sqlite>,
    connection_id: &str,
) -> AppResult<ConnectionWithCredential> {
    let row = sqlx::query(
        "SELECT id, account_email, account_display_name, credential_ref, scopes_json,
                root_folder_drive_id, root_folder_name, upload_mode, version_policy,
                include_past_versions, include_solution_source, include_artifacts,
                status, created_at, updated_at
         FROM drive_connections
         WHERE id = ?1",
    )
    .bind(connection_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Google Drive 接続情報が見つかりません".into()))?;

    Ok(row_to_connection_with_cred(&row))
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_connection(
    pool: &Pool<Sqlite>,
    id: &str,
    account_email: &str,
    account_display_name: Option<&str>,
    credential_ref: &str,
    scopes: &[String],
    root_folder_drive_id: Option<&str>,
    root_folder_name: &str,
) -> AppResult<ConnectionWithCredential> {
    let ts = now_iso();
    let scopes_json =
        serde_json::to_string(scopes).map_err(|e| AppError::Database(e.to_string()))?;

    let existing_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM drive_connections WHERE account_email = ?1")
            .bind(account_email)
            .fetch_optional(pool)
            .await?;

    if let Some(eid) = existing_id {
        sqlx::query(
            "UPDATE drive_connections
             SET account_display_name = COALESCE(?1, account_display_name),
                 credential_ref = ?2,
                 scopes_json = ?3,
                 root_folder_drive_id = COALESCE(?4, root_folder_drive_id),
                 status = 'connected',
                 updated_at = ?5
             WHERE id = ?6",
        )
        .bind(account_display_name)
        .bind(credential_ref)
        .bind(&scopes_json)
        .bind(root_folder_drive_id)
        .bind(&ts)
        .bind(&eid)
        .execute(pool)
        .await?;
        get_connection_with_cred_by_id(pool, &eid).await
    } else {
        sqlx::query(
            "INSERT INTO drive_connections
             (id, account_email, account_display_name, credential_ref, scopes_json,
              root_folder_drive_id, root_folder_name, upload_mode, version_policy,
              include_past_versions, include_solution_source, include_artifacts,
              status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'manual', 'update_latest', 0, 0, 0, 'connected', ?8, ?9)",
        )
        .bind(id)
        .bind(account_email)
        .bind(account_display_name)
        .bind(credential_ref)
        .bind(&scopes_json)
        .bind(root_folder_drive_id)
        .bind(root_folder_name)
        .bind(&ts)
        .bind(&ts)
        .execute(pool)
        .await?;
        get_connection_with_cred_by_id(pool, id).await
    }
}

pub async fn set_connection_status(
    pool: &Pool<Sqlite>,
    connection_id: &str,
    status: &str,
) -> AppResult<()> {
    if !matches!(status, "connected" | "reauth_required" | "disconnected") {
        return Err(AppError::Validation(format!(
            "不正な接続ステータスです: {status}"
        )));
    }
    let ts = now_iso();
    sqlx::query("UPDATE drive_connections SET status = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(status)
        .bind(&ts)
        .bind(connection_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_connection_root_folder_id(
    pool: &Pool<Sqlite>,
    connection_id: &str,
    root_folder_drive_id: Option<&str>,
) -> AppResult<()> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE drive_connections SET root_folder_drive_id = ?1, updated_at = ?2 WHERE id = ?3",
    )
    .bind(root_folder_drive_id)
    .bind(&ts)
    .bind(connection_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_connection_settings(
    pool: &Pool<Sqlite>,
    connection_id: &str,
    req: &UpdateDriveSettingsRequest,
) -> AppResult<DriveConnection> {
    let cur = get_connection_with_cred_by_id(pool, connection_id)
        .await?
        .connection;

    let new_root_name = match &req.root_folder_name {
        Some(n) => {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                return Err(AppError::Validation(
                    "保存先ルートフォルダ名を入力してください".into(),
                ));
            }
            trimmed.to_string()
        }
        None => cur.root_folder_name.clone(),
    };
    let root_name_changed = new_root_name != cur.root_folder_name;

    let new_upload_mode = match &req.upload_mode {
        Some(m) => {
            if !matches!(m.as_str(), "manual" | "auto" | "folder_auto") {
                return Err(AppError::Validation(format!(
                    "不正なアップロードモードです: {m}"
                )));
            }
            m.clone()
        }
        None => cur.upload_mode.clone(),
    };

    let new_version_policy = match &req.version_policy {
        Some(v) => {
            if !matches!(
                v.as_str(),
                "update_latest" | "create_new" | "manual_version"
            ) {
                return Err(AppError::Validation(format!(
                    "不正なバージョン管理ポリシーです: {v}"
                )));
            }
            v.clone()
        }
        None => cur.version_policy.clone(),
    };

    let inc_past = req
        .include_past_versions
        .unwrap_or(cur.include_past_versions);
    let inc_src = req
        .include_solution_source
        .unwrap_or(cur.include_solution_source);
    let inc_art = req.include_artifacts.unwrap_or(cur.include_artifacts);
    let ts = now_iso();

    sqlx::query(
        "UPDATE drive_connections
         SET root_folder_name = ?1,
             root_folder_drive_id = CASE WHEN ?2 = 1 THEN NULL ELSE root_folder_drive_id END,
             upload_mode = ?3,
             version_policy = ?4,
             include_past_versions = ?5,
             include_solution_source = ?6,
             include_artifacts = ?7,
             updated_at = ?8
         WHERE id = ?9",
    )
    .bind(&new_root_name)
    .bind(if root_name_changed { 1 } else { 0 })
    .bind(&new_upload_mode)
    .bind(&new_version_policy)
    .bind(if inc_past { 1 } else { 0 })
    .bind(if inc_src { 1 } else { 0 })
    .bind(if inc_art { 1 } else { 0 })
    .bind(&ts)
    .bind(connection_id)
    .execute(pool)
    .await?;

    // If root folder name changed, clear cached folder hierarchy so next upload creates/resolves under new root
    if root_name_changed {
        clear_folder_mappings(pool, connection_id).await?;
    }

    Ok(get_connection_with_cred_by_id(pool, connection_id)
        .await?
        .connection)
}

fn row_to_connection_with_cred(r: &sqlx::sqlite::SqliteRow) -> ConnectionWithCredential {
    let scopes_raw: String = r.get("scopes_json");
    let scopes = serde_json::from_str::<Vec<String>>(&scopes_raw).unwrap_or_default();
    ConnectionWithCredential {
        credential_ref: r.get("credential_ref"),
        connection: DriveConnection {
            id: r.get("id"),
            account_email: r.get("account_email"),
            account_display_name: r.get("account_display_name"),
            scopes,
            root_folder_drive_id: r.get("root_folder_drive_id"),
            root_folder_name: r.get("root_folder_name"),
            upload_mode: r.get("upload_mode"),
            version_policy: r.get("version_policy"),
            include_past_versions: r.get::<i64, _>("include_past_versions") != 0,
            include_solution_source: r.get::<i64, _>("include_solution_source") != 0,
            include_artifacts: r.get::<i64, _>("include_artifacts") != 0,
            status: r.get("status"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        },
    }
}

// ============================================================================
// Folder Mappings
// ============================================================================

#[derive(Debug, Clone)]
pub struct DriveFolderRecord {
    pub id: String,
    pub connection_id: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub drive_folder_id: String,
    pub parent_drive_folder_id: Option<String>,
    pub folder_name: String,
}

pub async fn get_folder_mapping(
    pool: &Pool<Sqlite>,
    connection_id: &str,
    entity_type: &str,
    entity_id: Option<&str>,
) -> AppResult<Option<DriveFolderRecord>> {
    let row = sqlx::query(
        "SELECT id, connection_id, entity_type, entity_id, drive_folder_id,
                parent_drive_folder_id, folder_name
         FROM drive_folders
         WHERE connection_id = ?1 AND entity_type = ?2 AND COALESCE(entity_id, '') = COALESCE(?3, '')",
    )
    .bind(connection_id)
    .bind(entity_type)
    .bind(entity_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| DriveFolderRecord {
        id: r.get("id"),
        connection_id: r.get("connection_id"),
        entity_type: r.get("entity_type"),
        entity_id: r.get("entity_id"),
        drive_folder_id: r.get("drive_folder_id"),
        parent_drive_folder_id: r.get("parent_drive_folder_id"),
        folder_name: r.get("folder_name"),
    }))
}

pub async fn upsert_folder_mapping(
    pool: &Pool<Sqlite>,
    connection_id: &str,
    entity_type: &str,
    entity_id: Option<&str>,
    drive_folder_id: &str,
    parent_drive_folder_id: Option<&str>,
    folder_name: &str,
) -> AppResult<()> {
    let ts = now_iso();
    if let Some(existing) = get_folder_mapping(pool, connection_id, entity_type, entity_id).await? {
        sqlx::query(
            "UPDATE drive_folders
             SET drive_folder_id = ?1, parent_drive_folder_id = ?2, folder_name = ?3, updated_at = ?4
             WHERE id = ?5",
        )
        .bind(drive_folder_id)
        .bind(parent_drive_folder_id)
        .bind(folder_name)
        .bind(&ts)
        .bind(&existing.id)
        .execute(pool)
        .await?;
    } else {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO drive_folders
             (id, connection_id, entity_type, entity_id, drive_folder_id, parent_drive_folder_id, folder_name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(&id)
        .bind(connection_id)
        .bind(entity_type)
        .bind(entity_id)
        .bind(drive_folder_id)
        .bind(parent_drive_folder_id)
        .bind(folder_name)
        .bind(&ts)
        .bind(&ts)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn delete_folder_mapping_by_drive_id(
    pool: &Pool<Sqlite>,
    connection_id: &str,
    drive_folder_id: &str,
) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM drive_folders WHERE connection_id = ?1 AND (drive_folder_id = ?2 OR parent_drive_folder_id = ?2)",
    )
    .bind(connection_id)
    .bind(drive_folder_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear_folder_mappings(pool: &Pool<Sqlite>, connection_id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM drive_folders WHERE connection_id = ?1")
        .bind(connection_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Drive Files (Logical Role Mapping -> Same Drive ID Updates)
// ============================================================================

pub async fn get_drive_file_by_logical_role(
    pool: &Pool<Sqlite>,
    connection_id: &str,
    document_id: &str,
    logical_role: &str,
) -> AppResult<Option<DriveFileMapping>> {
    let row = sqlx::query(
        "SELECT id, connection_id, document_id, logical_role, drive_file_id, drive_folder_id,
                drive_filename, web_view_link, last_uploaded_attachment_id, last_uploaded_sha256,
                remote_md5_checksum, remote_modified_time, remote_version, sync_status,
                created_at, updated_at
         FROM drive_files
         WHERE connection_id = ?1 AND document_id = ?2 AND logical_role = ?3",
    )
    .bind(connection_id)
    .bind(document_id)
    .bind(logical_role)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| row_to_drive_file(&r)))
}

pub async fn list_drive_files_for_document(
    pool: &Pool<Sqlite>,
    document_id: &str,
) -> AppResult<Vec<DriveFileMapping>> {
    let rows = sqlx::query(
        "SELECT id, connection_id, document_id, logical_role, drive_file_id, drive_folder_id,
                drive_filename, web_view_link, last_uploaded_attachment_id, last_uploaded_sha256,
                remote_md5_checksum, remote_modified_time, remote_version, sync_status,
                created_at, updated_at
         FROM drive_files
         WHERE document_id = ?1
         ORDER BY updated_at DESC",
    )
    .bind(document_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_drive_file).collect())
}

pub async fn list_all_drive_files(
    pool: &Pool<Sqlite>,
    connection_id: &str,
) -> AppResult<Vec<DriveFileMapping>> {
    let rows = sqlx::query(
        "SELECT id, connection_id, document_id, logical_role, drive_file_id, drive_folder_id,
                drive_filename, web_view_link, last_uploaded_attachment_id, last_uploaded_sha256,
                remote_md5_checksum, remote_modified_time, remote_version, sync_status,
                created_at, updated_at
         FROM drive_files
         WHERE connection_id = ?1
         ORDER BY updated_at DESC",
    )
    .bind(connection_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_drive_file).collect())
}

pub struct UpsertDriveFile<'a> {
    pub connection_id: &'a str,
    pub document_id: &'a str,
    pub logical_role: &'a str,
    pub drive_file_id: &'a str,
    pub drive_folder_id: &'a str,
    pub drive_filename: &'a str,
    pub web_view_link: Option<&'a str>,
    pub last_uploaded_attachment_id: Option<&'a str>,
    pub last_uploaded_sha256: Option<&'a str>,
    pub remote_md5_checksum: Option<&'a str>,
    pub remote_modified_time: Option<&'a str>,
    pub remote_version: Option<&'a str>,
    pub sync_status: &'a str,
}

pub async fn upsert_drive_file(
    pool: &Pool<Sqlite>,
    rec: &UpsertDriveFile<'_>,
) -> AppResult<DriveFileMapping> {
    let ts = now_iso();
    if let Some(existing) =
        get_drive_file_by_logical_role(pool, rec.connection_id, rec.document_id, rec.logical_role)
            .await?
    {
        sqlx::query(
            "UPDATE drive_files
             SET drive_file_id = ?1,
                 drive_folder_id = ?2,
                 drive_filename = ?3,
                 web_view_link = COALESCE(?4, web_view_link),
                 last_uploaded_attachment_id = ?5,
                 last_uploaded_sha256 = ?6,
                 remote_md5_checksum = ?7,
                 remote_modified_time = ?8,
                 remote_version = ?9,
                 sync_status = ?10,
                 updated_at = ?11
             WHERE id = ?12",
        )
        .bind(rec.drive_file_id)
        .bind(rec.drive_folder_id)
        .bind(rec.drive_filename)
        .bind(rec.web_view_link)
        .bind(rec.last_uploaded_attachment_id)
        .bind(rec.last_uploaded_sha256)
        .bind(rec.remote_md5_checksum)
        .bind(rec.remote_modified_time)
        .bind(rec.remote_version)
        .bind(rec.sync_status)
        .bind(&ts)
        .bind(&existing.id)
        .execute(pool)
        .await?;
    } else {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO drive_files
             (id, connection_id, document_id, logical_role, drive_file_id, drive_folder_id,
              drive_filename, web_view_link, last_uploaded_attachment_id, last_uploaded_sha256,
              remote_md5_checksum, remote_modified_time, remote_version, sync_status,
              created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        )
        .bind(&id)
        .bind(rec.connection_id)
        .bind(rec.document_id)
        .bind(rec.logical_role)
        .bind(rec.drive_file_id)
        .bind(rec.drive_folder_id)
        .bind(rec.drive_filename)
        .bind(rec.web_view_link)
        .bind(rec.last_uploaded_attachment_id)
        .bind(rec.last_uploaded_sha256)
        .bind(rec.remote_md5_checksum)
        .bind(rec.remote_modified_time)
        .bind(rec.remote_version)
        .bind(rec.sync_status)
        .bind(&ts)
        .bind(&ts)
        .execute(pool)
        .await?;
    }

    get_drive_file_by_logical_role(pool, rec.connection_id, rec.document_id, rec.logical_role)
        .await?
        .ok_or_else(|| AppError::Database("drive_files 保存後の読み込みに失敗しました".into()))
}

pub async fn update_drive_file_sync_status(
    pool: &Pool<Sqlite>,
    connection_id: &str,
    document_id: &str,
    logical_role: &str,
    sync_status: &str,
) -> AppResult<()> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE drive_files
         SET sync_status = ?1, updated_at = ?2
         WHERE connection_id = ?3 AND document_id = ?4 AND logical_role = ?5",
    )
    .bind(sync_status)
    .bind(&ts)
    .bind(connection_id)
    .bind(document_id)
    .bind(logical_role)
    .execute(pool)
    .await?;
    Ok(())
}

fn row_to_drive_file(r: &sqlx::sqlite::SqliteRow) -> DriveFileMapping {
    DriveFileMapping {
        id: r.get("id"),
        connection_id: r.get("connection_id"),
        document_id: r.get("document_id"),
        logical_role: r.get("logical_role"),
        drive_file_id: r.get("drive_file_id"),
        drive_folder_id: r.get("drive_folder_id"),
        drive_filename: r.get("drive_filename"),
        web_view_link: r.get("web_view_link"),
        last_uploaded_attachment_id: r.get("last_uploaded_attachment_id"),
        last_uploaded_sha256: r.get("last_uploaded_sha256"),
        remote_md5_checksum: r.get("remote_md5_checksum"),
        remote_modified_time: r.get("remote_modified_time"),
        remote_version: r.get("remote_version"),
        sync_status: r.get("sync_status"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

// ============================================================================
// Persistent Upload Job Queue
// ============================================================================

pub struct NewDriveUploadJob<'a> {
    pub connection_id: &'a str,
    pub document_id: &'a str,
    pub logical_role: &'a str,
    pub attachment_id: Option<&'a str>,
    pub drive_file_id: Option<&'a str>,
    pub target_filename: &'a str,
}

/// Enqueues an upload job with active-job deduplication via `uq_drive_jobs_active`:
/// - If a `pending` job already exists for `(connection_id, document_id, logical_role)`,
///   updates its `attachment_id` & `target_filename` in place (avoiding duplicate jobs).
/// - If an `uploading` job is currently running and the local attachment changed,
///   marks the running job as `update_required` so the worker re-enqueues the newer version.
pub async fn enqueue_or_coalesce_job(
    pool: &Pool<Sqlite>,
    req: &NewDriveUploadJob<'_>,
) -> AppResult<DriveUploadJob> {
    let ts = now_iso();
    let active = sqlx::query(
        "SELECT id, connection_id, document_id, logical_role, attachment_id, drive_file_id,
                target_filename, idempotency_key, force_overwrite, save_as_new, status,
                attempt_count, next_retry_at, last_error_code, last_error_message,
                conflict_remote_md5, conflict_remote_modified_time, conflict_remote_version,
                created_at, updated_at
         FROM drive_upload_jobs
         WHERE connection_id = ?1 AND document_id = ?2 AND logical_role = ?3
           AND status IN ('pending', 'uploading')
         LIMIT 1",
    )
    .bind(req.connection_id)
    .bind(req.document_id)
    .bind(req.logical_role)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = active {
        let existing = row_to_drive_job(&row);
        if existing.status == "pending" {
            sqlx::query(
                "UPDATE drive_upload_jobs
                 SET attachment_id = ?1,
                     drive_file_id = COALESCE(?2, drive_file_id),
                     target_filename = ?3,
                     next_retry_at = NULL,
                     updated_at = ?4
                 WHERE id = ?5",
            )
            .bind(req.attachment_id)
            .bind(req.drive_file_id)
            .bind(req.target_filename)
            .bind(&ts)
            .bind(&existing.id)
            .execute(pool)
            .await?;
            return get_job_by_id(pool, &existing.id).await;
        }
        // Currently uploading: if local attachment changed mid-flight, flag `update_required`
        if existing.attachment_id.as_deref() != req.attachment_id {
            sqlx::query(
                "UPDATE drive_upload_jobs
                 SET status = 'update_required',
                     attachment_id = ?1,
                     target_filename = ?2,
                     updated_at = ?3
                 WHERE id = ?4",
            )
            .bind(req.attachment_id)
            .bind(req.target_filename)
            .bind(&ts)
            .bind(&existing.id)
            .execute(pool)
            .await?;
            return get_job_by_id(pool, &existing.id).await;
        }
        return Ok(existing);
    }

    let id = Uuid::new_v4().to_string();
    let idempotency_key = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO drive_upload_jobs
         (id, connection_id, document_id, logical_role, attachment_id, drive_file_id,
          target_filename, idempotency_key, force_overwrite, save_as_new, status,
          attempt_count, next_retry_at, last_error_code, last_error_message,
          conflict_remote_md5, conflict_remote_modified_time, conflict_remote_version,
          created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 0, 'pending', 0, NULL, NULL, NULL, NULL, NULL, NULL, ?9, ?10)",
    )
    .bind(&id)
    .bind(req.connection_id)
    .bind(req.document_id)
    .bind(req.logical_role)
    .bind(req.attachment_id)
    .bind(req.drive_file_id)
    .bind(req.target_filename)
    .bind(&idempotency_key)
    .bind(&ts)
    .bind(&ts)
    .execute(pool)
    .await?;

    get_job_by_id(pool, &id).await
}

pub async fn get_job_by_id(pool: &Pool<Sqlite>, job_id: &str) -> AppResult<DriveUploadJob> {
    let row = sqlx::query(
        "SELECT id, connection_id, document_id, logical_role, attachment_id, drive_file_id,
                target_filename, idempotency_key, force_overwrite, save_as_new, status,
                attempt_count, next_retry_at, last_error_code, last_error_message,
                conflict_remote_md5, conflict_remote_modified_time, conflict_remote_version,
                created_at, updated_at
         FROM drive_upload_jobs
         WHERE id = ?1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Drive アップロードジョブが見つかりません".into()))?;
    Ok(row_to_drive_job(&row))
}

pub async fn list_recent_jobs(
    pool: &Pool<Sqlite>,
    connection_id: Option<&str>,
    limit: i64,
) -> AppResult<Vec<DriveUploadJob>> {
    let rows = if let Some(cid) = connection_id {
        sqlx::query(
            "SELECT id, connection_id, document_id, logical_role, attachment_id, drive_file_id,
                    target_filename, idempotency_key, force_overwrite, save_as_new, status,
                    attempt_count, next_retry_at, last_error_code, last_error_message,
                    conflict_remote_md5, conflict_remote_modified_time, conflict_remote_version,
                    created_at, updated_at
             FROM drive_upload_jobs
             WHERE connection_id = ?1
             ORDER BY updated_at DESC
             LIMIT ?2",
        )
        .bind(cid)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT id, connection_id, document_id, logical_role, attachment_id, drive_file_id,
                    target_filename, idempotency_key, force_overwrite, save_as_new, status,
                    attempt_count, next_retry_at, last_error_code, last_error_message,
                    conflict_remote_md5, conflict_remote_modified_time, conflict_remote_version,
                    created_at, updated_at
             FROM drive_upload_jobs
             ORDER BY updated_at DESC
             LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?
    };
    Ok(rows.iter().map(row_to_drive_job).collect())
}

pub async fn list_jobs_for_document(
    pool: &Pool<Sqlite>,
    document_id: &str,
) -> AppResult<Vec<DriveUploadJob>> {
    let rows = sqlx::query(
        "SELECT id, connection_id, document_id, logical_role, attachment_id, drive_file_id,
                target_filename, idempotency_key, force_overwrite, save_as_new, status,
                attempt_count, next_retry_at, last_error_code, last_error_message,
                conflict_remote_md5, conflict_remote_modified_time, conflict_remote_version,
                created_at, updated_at
         FROM drive_upload_jobs
         WHERE document_id = ?1
         ORDER BY updated_at DESC
         LIMIT 50",
    )
    .bind(document_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_drive_job).collect())
}

/// Atomically claims the next due `pending` job (whose `next_retry_at` is NULL or `<= now`),
/// incrementing `attempt_count` and setting `status = 'uploading'`.
pub async fn claim_next_due_job(
    pool: &Pool<Sqlite>,
    connection_id: &str,
) -> AppResult<Option<DriveUploadJob>> {
    let now = now_iso();
    let mut tx = pool.begin().await?;
    let next_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM drive_upload_jobs
         WHERE connection_id = ?1
           AND status = 'pending'
           AND (next_retry_at IS NULL OR next_retry_at <= ?2)
         ORDER BY created_at ASC
         LIMIT 1",
    )
    .bind(connection_id)
    .bind(&now)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(job_id) = next_id else {
        tx.commit().await?;
        return Ok(None);
    };

    sqlx::query(
        "UPDATE drive_upload_jobs
         SET status = 'uploading',
             attempt_count = attempt_count + 1,
             next_retry_at = NULL,
             updated_at = ?1
         WHERE id = ?2 AND status = 'pending'",
    )
    .bind(&now)
    .bind(&job_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Some(get_job_by_id(pool, &job_id).await?))
}

pub async fn mark_job_uploaded(
    pool: &Pool<Sqlite>,
    job_id: &str,
    drive_file_id: &str,
) -> AppResult<DriveUploadJob> {
    let cur = get_job_by_id(pool, job_id).await?;
    let ts = now_iso();
    // If local file was updated while this job was uploading, transition to pending for latest version
    if cur.status == "update_required" {
        sqlx::query(
            "UPDATE drive_upload_jobs
             SET status = 'pending',
                 drive_file_id = ?1,
                 next_retry_at = NULL,
                 last_error_code = NULL,
                 last_error_message = NULL,
                 updated_at = ?2
             WHERE id = ?3",
        )
        .bind(drive_file_id)
        .bind(&ts)
        .bind(job_id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE drive_upload_jobs
             SET status = 'uploaded',
                 drive_file_id = ?1,
                 next_retry_at = NULL,
                 last_error_code = NULL,
                 last_error_message = NULL,
                 conflict_remote_md5 = NULL,
                 conflict_remote_modified_time = NULL,
                 conflict_remote_version = NULL,
                 updated_at = ?2
             WHERE id = ?3",
        )
        .bind(drive_file_id)
        .bind(&ts)
        .bind(job_id)
        .execute(pool)
        .await?;
    }
    get_job_by_id(pool, job_id).await
}

pub async fn reschedule_job_with_backoff(
    pool: &Pool<Sqlite>,
    job_id: &str,
    next_retry_at: &str,
    error_code: &str,
    error_message: &str,
) -> AppResult<DriveUploadJob> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE drive_upload_jobs
         SET status = 'pending',
             next_retry_at = ?1,
             last_error_code = ?2,
             last_error_message = ?3,
             updated_at = ?4
         WHERE id = ?5",
    )
    .bind(next_retry_at)
    .bind(error_code)
    .bind(error_message)
    .bind(&ts)
    .bind(job_id)
    .execute(pool)
    .await?;
    get_job_by_id(pool, job_id).await
}

pub async fn mark_job_conflict(
    pool: &Pool<Sqlite>,
    job_id: &str,
    error_code: &str,
    error_message: &str,
    remote_md5: Option<&str>,
    remote_modified_time: Option<&str>,
    remote_version: Option<&str>,
) -> AppResult<DriveUploadJob> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE drive_upload_jobs
         SET status = 'conflict',
             next_retry_at = NULL,
             last_error_code = ?1,
             last_error_message = ?2,
             conflict_remote_md5 = ?3,
             conflict_remote_modified_time = ?4,
             conflict_remote_version = ?5,
             updated_at = ?6
         WHERE id = ?7",
    )
    .bind(error_code)
    .bind(error_message)
    .bind(remote_md5)
    .bind(remote_modified_time)
    .bind(remote_version)
    .bind(&ts)
    .bind(job_id)
    .execute(pool)
    .await?;
    get_job_by_id(pool, job_id).await
}

pub async fn mark_job_failed(
    pool: &Pool<Sqlite>,
    job_id: &str,
    error_code: &str,
    error_message: &str,
) -> AppResult<DriveUploadJob> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE drive_upload_jobs
         SET status = 'failed',
             next_retry_at = NULL,
             last_error_code = ?1,
             last_error_message = ?2,
             updated_at = ?3
         WHERE id = ?4",
    )
    .bind(error_code)
    .bind(error_message)
    .bind(&ts)
    .bind(job_id)
    .execute(pool)
    .await?;
    get_job_by_id(pool, job_id).await
}

pub async fn cancel_job(pool: &Pool<Sqlite>, job_id: &str) -> AppResult<DriveUploadJob> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE drive_upload_jobs
         SET status = 'cancelled', next_retry_at = NULL, updated_at = ?1
         WHERE id = ?2 AND status IN ('pending', 'conflict', 'failed', 'update_required')",
    )
    .bind(&ts)
    .bind(job_id)
    .execute(pool)
    .await?;
    get_job_by_id(pool, job_id).await
}

pub async fn cancel_pending_jobs_for_connection(
    pool: &Pool<Sqlite>,
    connection_id: &str,
) -> AppResult<()> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE drive_upload_jobs
         SET status = 'cancelled', next_retry_at = NULL, updated_at = ?1
         WHERE connection_id = ?2 AND status IN ('pending', 'uploading')",
    )
    .bind(&ts)
    .bind(connection_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn retry_job(pool: &Pool<Sqlite>, job_id: &str) -> AppResult<DriveUploadJob> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE drive_upload_jobs
         SET status = 'pending',
             attempt_count = 0,
             next_retry_at = NULL,
             last_error_code = NULL,
             last_error_message = NULL,
             updated_at = ?1
         WHERE id = ?2 AND status IN ('failed', 'cancelled', 'update_required', 'pending')",
    )
    .bind(&ts)
    .bind(job_id)
    .execute(pool)
    .await?;
    get_job_by_id(pool, job_id).await
}

pub async fn prepare_conflict_resolution(
    pool: &Pool<Sqlite>,
    job_id: &str,
    force_overwrite: bool,
    save_as_new: bool,
    new_filename: Option<&str>,
) -> AppResult<DriveUploadJob> {
    let ts = now_iso();
    let new_idem = if save_as_new {
        Some(Uuid::new_v4().to_string())
    } else {
        None
    };

    sqlx::query(
        "UPDATE drive_upload_jobs
         SET status = 'pending',
             force_overwrite = ?1,
             save_as_new = ?2,
             drive_file_id = CASE WHEN ?2 = 1 THEN NULL ELSE drive_file_id END,
             target_filename = COALESCE(?3, target_filename),
             idempotency_key = COALESCE(?4, idempotency_key),
             attempt_count = 0,
             next_retry_at = NULL,
             last_error_code = NULL,
             last_error_message = NULL,
             conflict_remote_md5 = NULL,
             conflict_remote_modified_time = NULL,
             conflict_remote_version = NULL,
             updated_at = ?5
         WHERE id = ?6",
    )
    .bind(if force_overwrite { 1 } else { 0 })
    .bind(if save_as_new { 1 } else { 0 })
    .bind(new_filename)
    .bind(new_idem.as_deref())
    .bind(&ts)
    .bind(job_id)
    .execute(pool)
    .await?;

    get_job_by_id(pool, job_id).await
}

/// Computes per-document aggregate Drive sync statuses for fast UI badge rendering.
pub async fn build_document_sync_summaries(
    pool: &Pool<Sqlite>,
    connection_id: &str,
) -> AppResult<Vec<DriveDocumentSyncSummary>> {
    let files = list_all_drive_files(pool, connection_id).await?;
    let jobs = list_recent_jobs(pool, Some(connection_id), 500).await?;

    let mut map: HashMap<String, DriveDocumentSyncSummary> = HashMap::new();

    for f in files {
        let Some(doc_id) = f.document_id else {
            continue;
        };
        let entry = map
            .entry(doc_id.clone())
            .or_insert_with(|| DriveDocumentSyncSummary {
                document_id: doc_id,
                aggregate_status: "none".to_string(),
                uploaded_count: 0,
                pending_count: 0,
                conflict_count: 0,
                failed_count: 0,
            });
        match f.sync_status.as_str() {
            "uploaded" => entry.uploaded_count += 1,
            "conflict" | "remote_deleted" => entry.conflict_count += 1,
            "error" => entry.failed_count += 1,
            "update_required" => entry.pending_count += 1,
            _ => {}
        }
    }

    for j in jobs {
        let entry = map
            .entry(j.document_id.clone())
            .or_insert_with(|| DriveDocumentSyncSummary {
                document_id: j.document_id.clone(),
                aggregate_status: "none".to_string(),
                uploaded_count: 0,
                pending_count: 0,
                conflict_count: 0,
                failed_count: 0,
            });
        match j.status.as_str() {
            "uploading" => {
                entry.pending_count += 1;
                entry.aggregate_status = "uploading".to_string();
            }
            "pending" | "update_required" => entry.pending_count += 1,
            "conflict" => entry.conflict_count += 1,
            "failed" => entry.failed_count += 1,
            _ => {}
        }
    }

    let mut out: Vec<DriveDocumentSyncSummary> = map
        .into_values()
        .map(|mut s| {
            if s.conflict_count > 0 {
                s.aggregate_status = "conflict".to_string();
            } else if s.aggregate_status == "uploading" {
                // keep uploading
            } else if s.pending_count > 0 {
                s.aggregate_status = "pending".to_string();
            } else if s.failed_count > 0 {
                s.aggregate_status = "failed".to_string();
            } else if s.uploaded_count > 0 {
                s.aggregate_status = "uploaded".to_string();
            }
            s
        })
        .collect();
    out.sort_by(|a, b| a.document_id.cmp(&b.document_id));
    Ok(out)
}

fn row_to_drive_job(r: &sqlx::sqlite::SqliteRow) -> DriveUploadJob {
    DriveUploadJob {
        id: r.get("id"),
        connection_id: r.get("connection_id"),
        document_id: r.get("document_id"),
        logical_role: r.get("logical_role"),
        attachment_id: r.get("attachment_id"),
        drive_file_id: r.get("drive_file_id"),
        target_filename: r.get("target_filename"),
        idempotency_key: r.get("idempotency_key"),
        force_overwrite: r.get::<i64, _>("force_overwrite") != 0,
        save_as_new: r.get::<i64, _>("save_as_new") != 0,
        status: r.get("status"),
        attempt_count: r.get("attempt_count"),
        next_retry_at: r.get("next_retry_at"),
        last_error_code: r.get("last_error_code"),
        last_error_message: r.get("last_error_message"),
        conflict_remote_md5: r.get("conflict_remote_md5"),
        conflict_remote_modified_time: r.get("conflict_remote_modified_time"),
        conflict_remote_version: r.get("conflict_remote_version"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}
