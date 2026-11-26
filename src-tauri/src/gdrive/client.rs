use std::collections::HashMap;

use reqwest::multipart;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::security::redact_secrets;

/// Safe, export-only Google Drive API v3 client.
///
/// SECURITY INVARIANT:
/// This client intentionally provides ZERO delete methods (`files.delete` / `trashed: true`)
/// so deleting local problems or solutions can NEVER delete files on Google Drive.
#[derive(Debug, Error)]
pub enum DriveApiError {
    #[error("ネットワーク通信エラー: {0}")]
    Network(String),
    #[error("認証期限切れまたは無効なトークンです (HTTP 401)")]
    Unauthorized,
    #[error("Google Drive API レート制限に達しました (Retry-After: {retry_after_secs:?}s)")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error(
        "Google Drive の保存容量上限に達しています。不要なファイルを整理して再試行してください。"
    )]
    StorageQuotaExceeded,
    #[error(
        "Google Drive 上でファイルが外部変更されたため上書きを中止しました (remote_md5={remote_md5:?}, remote_version={remote_version:?})"
    )]
    ExternalConflict {
        file_id: String,
        expected_md5: Option<String>,
        remote_md5: Option<String>,
        remote_modified_time: Option<String>,
        remote_version: Option<String>,
    },
    #[error("Google Drive 上の既存ファイルが削除またはゴミ箱へ移動されています")]
    RemoteDeleted { file_id: String },
    #[error("Google Drive 上の保存先フォルダが見つからないか削除されています")]
    FolderNotFound { folder_id: String },
    #[error("Google Drive API エラー ({status}): {message}")]
    Fatal { status: u16, message: String },
    #[error("JSON シリアライズ/デシリアライズ失敗: {0}")]
    Json(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveAboutUser {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub email_address: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AboutResponse {
    #[serde(default)]
    user: Option<DriveAboutUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFileMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub md5_checksum: Option<String>,
    #[serde(default)]
    pub modified_time: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub trashed: Option<bool>,
    #[serde(default)]
    pub web_view_link: Option<String>,
    #[serde(default)]
    pub app_properties: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct FileListResponse {
    #[serde(default)]
    files: Vec<DriveFileMeta>,
}

pub struct DriveClient {
    http: reqwest::Client,
    api_base: String,
    upload_base: String,
    access_token: String,
}

impl DriveClient {
    pub fn new(
        http: reqwest::Client,
        api_base: impl Into<String>,
        upload_base: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Self {
        Self {
            http,
            api_base: api_base.into().trim_end_matches('/').to_string(),
            upload_base: upload_base.into().trim_end_matches('/').to_string(),
            access_token: access_token.into(),
        }
    }

    /// Fetches connected Google account metadata via `drive/v3/about` (allowed under `drive.file`).
    pub async fn get_about_user(&self) -> Result<Option<DriveAboutUser>, DriveApiError> {
        let url = format!(
            "{}/about?fields=user(displayName,emailAddress)",
            self.api_base
        );
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(map_network_err)?;
        let parsed: AboutResponse = parse_json_response(resp, None).await?;
        Ok(parsed.user)
    }

    /// Verifies that a cached folder still exists and is not trashed.
    pub async fn verify_folder_exists(&self, folder_id: &str) -> Result<bool, DriveApiError> {
        let url = format!(
            "{}/files/{folder_id}?fields=id,mimeType,trashed",
            self.api_base
        );
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(map_network_err)?;
        if resp.status().as_u16() == 404 {
            return Ok(false);
        }
        let meta: DriveFileMeta = parse_json_response(resp, Some(folder_id)).await?;
        Ok(meta.trashed != Some(true) && meta.mime_type == "application/vnd.google-apps.folder")
    }

    /// Searches for an existing non-trashed app-created folder with the given name & parent.
    pub async fn find_folder(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<Option<DriveFileMeta>, DriveApiError> {
        let escaped_name = escape_drive_query_literal(name);
        let query = match parent_id {
            Some(pid) => format!(
                "mimeType='application/vnd.google-apps.folder' and name='{escaped_name}' and '{}' in parents and trashed=false",
                escape_drive_query_literal(pid)
            ),
            None => format!(
                "mimeType='application/vnd.google-apps.folder' and name='{escaped_name}' and 'root' in parents and trashed=false"
            ),
        };
        let url = format!("{}/files", self.api_base);
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.access_token)
            .query(&[
                ("q", query.as_str()),
                (
                    "fields",
                    "files(id,name,mimeType,modifiedTime,version,trashed)",
                ),
                ("pageSize", "1"),
            ])
            .send()
            .await
            .map_err(map_network_err)?;
        let list: FileListResponse = parse_json_response(resp, parent_id).await?;
        Ok(list.files.into_iter().next())
    }

    pub async fn create_folder(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<DriveFileMeta, DriveApiError> {
        let mut body = serde_json::json!({
            "name": name,
            "mimeType": "application/vnd.google-apps.folder",
        });
        if let Some(pid) = parent_id {
            body["parents"] = serde_json::json!([pid]);
        }
        let url = format!(
            "{}/files?fields=id,name,mimeType,modifiedTime,version,trashed,webViewLink",
            self.api_base
        );
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await
            .map_err(map_network_err)?;
        if resp.status().as_u16() == 404
            && let Some(pid) = parent_id
        {
            return Err(DriveApiError::FolderNotFound {
                folder_id: pid.to_string(),
            });
        }
        parse_json_response(resp, parent_id).await
    }

    /// Queries Drive by our `appProperties.theorem_idempotency_key` to prevent duplicate file creation
    /// when a network drop or app crash happened right after `files.create` succeeded remotely.
    pub async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<DriveFileMeta>, DriveApiError> {
        let escaped = escape_drive_query_literal(idempotency_key);
        let query = format!(
            "appProperties has {{ key='theorem_idempotency_key' and value='{escaped}' }} and trashed=false"
        );
        let url = format!("{}/files", self.api_base);
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.access_token)
            .query(&[
                ("q", query.as_str()),
                (
                    "fields",
                    "files(id,name,mimeType,md5Checksum,modifiedTime,version,trashed,webViewLink,appProperties)",
                ),
                ("pageSize", "1"),
            ])
            .send()
            .await
            .map_err(map_network_err)?;
        let list: FileListResponse = parse_json_response(resp, None).await?;
        Ok(list.files.into_iter().next())
    }

    /// Initial upload (`files.create`) with `appProperties` idempotency key.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_file(
        &self,
        folder_id: &str,
        filename: &str,
        mime_type: &str,
        document_id: &str,
        logical_role: &str,
        idempotency_key: &str,
        content: Vec<u8>,
    ) -> Result<DriveFileMeta, DriveApiError> {
        // Idempotency recovery: reuse already-created remote file if previous attempt succeeded before ack
        if let Some(existing) = self.find_by_idempotency_key(idempotency_key).await? {
            return Ok(existing);
        }

        let metadata = serde_json::json!({
            "name": filename,
            "parents": [folder_id],
            "appProperties": {
                "theorem_idempotency_key": idempotency_key,
                "theorem_logical_role": logical_role,
                "theorem_document_id": document_id,
            }
        });
        let meta_str =
            serde_json::to_string(&metadata).map_err(|e| DriveApiError::Json(e.to_string()))?;
        let meta_part = multipart::Part::text(meta_str)
            .mime_str("application/json; charset=UTF-8")
            .map_err(|e| DriveApiError::Fatal {
                status: 0,
                message: e.to_string(),
            })?;
        let media_part = multipart::Part::bytes(content)
            .mime_str(mime_type)
            .map_err(|e| DriveApiError::Fatal {
                status: 0,
                message: e.to_string(),
            })?;
        let form = multipart::Form::new()
            .part("metadata", meta_part)
            .part("file", media_part);

        let url = format!(
            "{}/files?uploadType=multipart&fields=id,name,mimeType,md5Checksum,modifiedTime,version,trashed,webViewLink,appProperties",
            self.upload_base
        );
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.access_token)
            .multipart(form)
            .send()
            .await
            .map_err(map_network_err)?;
        if resp.status().as_u16() == 404 {
            return Err(DriveApiError::FolderNotFound {
                folder_id: folder_id.to_string(),
            });
        }
        parse_json_response(resp, None).await
    }

    pub async fn get_file_meta(&self, file_id: &str) -> Result<DriveFileMeta, DriveApiError> {
        let url = format!(
            "{}/files/{file_id}?fields=id,name,mimeType,md5Checksum,modifiedTime,version,trashed,webViewLink,appProperties",
            self.api_base
        );
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(map_network_err)?;
        if resp.status().as_u16() == 404 {
            return Err(DriveApiError::RemoteDeleted {
                file_id: file_id.to_string(),
            });
        }
        let meta: DriveFileMeta = parse_json_response(resp, Some(file_id)).await?;
        if meta.trashed == Some(true) {
            return Err(DriveApiError::RemoteDeleted {
                file_id: file_id.to_string(),
            });
        }
        Ok(meta)
    }

    /// Updates an existing Drive file (`files.update`) preserving the exact same `file_id`
    /// (so existing share links never break).
    ///
    /// Pre-fetches remote metadata (`md5Checksum`, `modifiedTime`, `version`, `trashed`) to detect
    /// external edits or remote deletion BEFORE overwriting, unless `force_overwrite` is true.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_file_preserving_id(
        &self,
        file_id: &str,
        mime_type: &str,
        expected_remote_md5: Option<&str>,
        expected_remote_modified_time: Option<&str>,
        expected_remote_version: Option<&str>,
        force_overwrite: bool,
        new_content: Vec<u8>,
    ) -> Result<DriveFileMeta, DriveApiError> {
        let current = self.get_file_meta(file_id).await?;
        if !force_overwrite {
            let md5_changed = expected_remote_md5.is_some()
                && current.md5_checksum.as_deref() != expected_remote_md5;
            let time_changed = expected_remote_md5.is_none()
                && expected_remote_modified_time.is_some()
                && current.modified_time.as_deref() != expected_remote_modified_time;
            let version_changed = expected_remote_md5.is_none()
                && expected_remote_modified_time.is_none()
                && expected_remote_version.is_some()
                && current.version.as_deref() != expected_remote_version;

            if md5_changed || time_changed || version_changed {
                return Err(DriveApiError::ExternalConflict {
                    file_id: file_id.to_string(),
                    expected_md5: expected_remote_md5.map(ToString::to_string),
                    remote_md5: current.md5_checksum,
                    remote_modified_time: current.modified_time,
                    remote_version: current.version,
                });
            }
        }

        let url = format!(
            "{}/files/{file_id}?uploadType=media&fields=id,name,mimeType,md5Checksum,modifiedTime,version,trashed,webViewLink,appProperties",
            self.upload_base
        );
        let resp = self
            .http
            .patch(url)
            .bearer_auth(&self.access_token)
            .header(reqwest::header::CONTENT_TYPE, mime_type)
            .body(new_content)
            .send()
            .await
            .map_err(map_network_err)?;
        if resp.status().as_u16() == 404 {
            return Err(DriveApiError::RemoteDeleted {
                file_id: file_id.to_string(),
            });
        }
        parse_json_response(resp, Some(file_id)).await
    }
}

fn escape_drive_query_literal(input: &str) -> String {
    input.replace('\\', r"\\").replace('\'', r"\'")
}

fn map_network_err(err: reqwest::Error) -> DriveApiError {
    DriveApiError::Network(redact_secrets(&err.to_string()))
}

async fn parse_json_response<T: for<'de> Deserialize<'de>>(
    resp: reqwest::Response,
    target_id: Option<&str>,
) -> Result<T, DriveApiError> {
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok());

    if resp.status().is_success() {
        return resp
            .json::<T>()
            .await
            .map_err(|e| DriveApiError::Json(redact_secrets(&e.to_string())));
    }

    let raw_body = resp.text().await.unwrap_or_default();
    let safe_body = redact_secrets(&raw_body);

    if status == 401 {
        return Err(DriveApiError::Unauthorized);
    }
    if status == 429
        || (status == 403
            && (safe_body.contains("rateLimitExceeded")
                || safe_body.contains("userRateLimitExceeded")))
    {
        return Err(DriveApiError::RateLimited {
            retry_after_secs: retry_after,
        });
    }
    if status == 403 && safe_body.contains("storageQuotaExceeded") {
        return Err(DriveApiError::StorageQuotaExceeded);
    }
    if status == 404
        && let Some(fid) = target_id
    {
        return Err(DriveApiError::RemoteDeleted {
            file_id: fid.to_string(),
        });
    }
    if status >= 500 {
        return Err(DriveApiError::Network(format!(
            "Google Drive サーバー一時エラー ({status}): {safe_body}"
        )));
    }
    Err(DriveApiError::Fatal {
        status,
        message: safe_body,
    })
}
