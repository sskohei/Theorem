use std::collections::HashMap;

use reqwest::multipart;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DriveError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Google Drive API error {status}: {body}")]
    Api { status: u16, body: String },
    #[error(
        "External modification conflict detected on Drive file {file_id}: expected md5={expected_md5:?}, remote md5={remote_md5:?}"
    )]
    ExternalConflict {
        file_id: String,
        expected_md5: Option<String>,
        remote_md5: Option<String>,
    },
    #[error("JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),
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

    pub fn google_production(http: reqwest::Client, access_token: impl Into<String>) -> Self {
        Self::new(
            http,
            "https://www.googleapis.com/drive/v3",
            "https://www.googleapis.com/upload/drive/v3",
            access_token,
        )
    }

    pub async fn create_folder(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<DriveFileMeta, DriveError> {
        let mut body = serde_json::json!({
            "name": name,
            "mimeType": "application/vnd.google-apps.folder",
        });
        if let Some(pid) = parent_id {
            body["parents"] = serde_json::json!([pid]);
        }
        let url = format!(
            "{}/files?fields=id,name,mimeType,modifiedTime,version",
            self.api_base
        );
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?;
        parse_json_or_err(resp).await
    }

    pub async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<DriveFileMeta>, DriveError> {
        let query = format!(
            "appProperties has {{ key='formuladesk_idempotency_key' and value='{idempotency_key}' }} and trashed=false"
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
                    "files(id,name,mimeType,md5Checksum,modifiedTime,version,appProperties)",
                ),
            ])
            .send()
            .await?;
        let list: FileListResponse = parse_json_or_err(resp).await?;
        Ok(list.files.into_iter().next())
    }

    /// Initial upload (`files.create`) with `appProperties` idempotency key so interrupted
    /// requests never create duplicate Drive files on retry.
    pub async fn create_file(
        &self,
        folder_id: &str,
        filename: &str,
        mime_type: &str,
        logical_role: &str,
        idempotency_key: &str,
        content: Vec<u8>,
    ) -> Result<DriveFileMeta, DriveError> {
        if let Some(existing) = self.find_by_idempotency_key(idempotency_key).await? {
            return Ok(existing);
        }

        let metadata = serde_json::json!({
            "name": filename,
            "parents": [folder_id],
            "appProperties": {
                "formuladesk_idempotency_key": idempotency_key,
                "formuladesk_logical_role": logical_role,
            }
        });
        let meta_part = multipart::Part::text(serde_json::to_string(&metadata)?)
            .mime_str("application/json; charset=UTF-8")?;
        let media_part = multipart::Part::bytes(content).mime_str(mime_type)?;
        let form = multipart::Form::new()
            .part("metadata", meta_part)
            .part("file", media_part);

        let url = format!(
            "{}/files?uploadType=multipart&fields=id,name,mimeType,md5Checksum,modifiedTime,version,appProperties",
            self.upload_base
        );
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.access_token)
            .multipart(form)
            .send()
            .await?;
        parse_json_or_err(resp).await
    }

    pub async fn get_file_meta(&self, file_id: &str) -> Result<DriveFileMeta, DriveError> {
        let url = format!(
            "{}/files/{file_id}?fields=id,name,mimeType,md5Checksum,modifiedTime,version,trashed,appProperties",
            self.api_base
        );
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        parse_json_or_err(resp).await
    }

    /// Updates an existing Drive file (`files.update`) while preserving the exact same `file_id`
    /// (and therefore preserving existing share links). Pre-checks remote `md5Checksum` to detect
    /// external edits before overwriting unless `force_overwrite` is true.
    pub async fn update_file_preserving_id(
        &self,
        file_id: &str,
        mime_type: &str,
        expected_remote_md5: Option<&str>,
        force_overwrite: bool,
        new_content: Vec<u8>,
    ) -> Result<DriveFileMeta, DriveError> {
        if !force_overwrite && let Some(expected) = expected_remote_md5 {
            let current = self.get_file_meta(file_id).await?;
            if current.md5_checksum.as_deref() != Some(expected) {
                return Err(DriveError::ExternalConflict {
                    file_id: file_id.to_string(),
                    expected_md5: Some(expected.to_string()),
                    remote_md5: current.md5_checksum,
                });
            }
        }

        let url = format!(
            "{}/files/{file_id}?uploadType=media&fields=id,name,mimeType,md5Checksum,modifiedTime,version,appProperties",
            self.upload_base
        );
        let resp = self
            .http
            .patch(url)
            .bearer_auth(&self.access_token)
            .header(reqwest::header::CONTENT_TYPE, mime_type)
            .body(new_content)
            .send()
            .await?;
        parse_json_or_err(resp).await
    }
}

async fn parse_json_or_err<T: for<'de> Deserialize<'de>>(
    resp: reqwest::Response,
) -> Result<T, DriveError> {
    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(DriveError::Api { status, body });
    }
    Ok(resp.json::<T>().await?)
}
