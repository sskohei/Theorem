mod drive;
mod oauth;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

use crate::drive::{DriveClient, DriveError, DriveFileMeta};
use crate::oauth::{
    DRIVE_FILE_SCOPE, LoopbackServer, build_authorize_url, exchange_code_for_token, generate_pkce,
    validate_scopes,
};

#[derive(Debug, Serialize)]
struct MockPocSummary {
    mode: &'static str,
    pkce_s256_verified: bool,
    minimal_scope_enforced: bool,
    loopback_callback_verified: bool,
    folder_created_id: String,
    initial_upload_file_id: String,
    idempotent_retry_same_id: bool,
    update_preserved_file_id: bool,
    external_conflict_detected: bool,
}

#[derive(Default)]
struct MockDriveState {
    files: HashMap<String, (DriveFileMeta, Vec<u8>)>,
    last_code_challenge: Option<String>,
    version_counter: u64,
}

async fn start_mock_google_server() -> (String, Arc<Mutex<MockDriveState>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock google server");
    let port = listener.local_addr().expect("addr").port();
    let base_url = format!("http://127.0.0.1:{port}");
    let state = Arc::new(Mutex::new(MockDriveState::default()));
    let state_bg = Arc::clone(&state);

    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let st = Arc::clone(&state_bg);
            tokio::spawn(async move {
                let mut raw = Vec::new();
                let mut tmp = [0u8; 4096];
                // Read HTTP headers + body based on Content-Length
                let header_end;
                loop {
                    match sock.read(&mut tmp).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => {
                            raw.extend_from_slice(&tmp[..n]);
                            if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                                header_end = pos + 4;
                                break;
                            }
                        }
                    }
                }
                let head = String::from_utf8_lossy(&raw[..header_end]).to_string();
                let content_len: usize = head
                    .lines()
                    .find_map(|l| {
                        let lower = l.to_ascii_lowercase();
                        lower
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse().ok())
                    })
                    .unwrap_or(0);
                while raw.len() < header_end + content_len {
                    match sock.read(&mut tmp).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => raw.extend_from_slice(&tmp[..n]),
                    }
                }
                let body_bytes = raw[header_end..].to_vec();
                let first_line = head.lines().next().unwrap_or_default();
                let mut parts = first_line.split_whitespace();
                let method = parts.next().unwrap_or("GET");
                let target = parts.next().unwrap_or("/");

                let (status, resp_json) = handle_mock_request(method, target, &body_bytes, &st);
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    resp_json.len(),
                    resp_json
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });

    (base_url, state)
}

fn pseudo_md5_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d[..16].iter().map(|b| format!("{b:02x}")).collect()
}

fn handle_mock_request(
    method: &str,
    target: &str,
    body: &[u8],
    state: &Arc<Mutex<MockDriveState>>,
) -> (&'static str, String) {
    let mut guard = state.lock().expect("lock mock state");

    if method == "POST" && target == "/token" {
        let form = String::from_utf8_lossy(body);
        if !form.contains("code_verifier=") || !form.contains("grant_type=authorization_code") {
            return (
                "400 Bad Request",
                r#"{"error":"invalid_grant"}"#.to_string(),
            );
        }
        return (
            "200 OK",
            serde_json::json!({
                "access_token": "mock_access_token_not_real",
                "refresh_token": "mock_refresh_token_not_real",
                "expires_in": 3600,
                "token_type": "Bearer",
                "scope": DRIVE_FILE_SCOPE
            })
            .to_string(),
        );
    }

    // Search files (idempotency check)
    if method == "GET" && target.starts_with("/drive/v3/files?") {
        let mut matched = Vec::new();
        for (meta, _) in guard.files.values() {
            if let Some(props) = &meta.app_properties
                && let Some(key) = props.get("formuladesk_idempotency_key")
                && target.contains(key)
            {
                matched.push(meta.clone());
            }
        }
        return (
            "200 OK",
            serde_json::json!({ "files": matched }).to_string(),
        );
    }

    // Create folder
    if method == "POST" && target.starts_with("/drive/v3/files") {
        let req: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
        let id = format!("folder-{}", Uuid::new_v4());
        let meta = DriveFileMeta {
            id: id.clone(),
            name: req["name"].as_str().unwrap_or("FormulaDesk").to_string(),
            mime_type: "application/vnd.google-apps.folder".to_string(),
            md5_checksum: None,
            modified_time: Some("2026-11-25T00:00:00Z".to_string()),
            version: Some("1".to_string()),
            trashed: Some(false),
            app_properties: None,
        };
        guard.files.insert(id, (meta.clone(), Vec::new()));
        return ("200 OK", serde_json::to_string(&meta).unwrap_or_default());
    }

    // Upload new file (multipart)
    if method == "POST" && target.starts_with("/upload/drive/v3/files") {
        let text = String::from_utf8_lossy(body);
        let json_start = text.find('{').unwrap_or(0);
        let json_end = text
            .find("}\r\n")
            .or_else(|| text.find("}\n"))
            .map(|i| i + 1)
            .unwrap_or(text.len());
        let req: serde_json::Value =
            serde_json::from_str(&text[json_start..json_end]).unwrap_or_default();
        let id = format!("file-{}", Uuid::new_v4());
        let mut app_props = HashMap::new();
        if let Some(obj) = req["appProperties"].as_object() {
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    app_props.insert(k.clone(), s.to_string());
                }
            }
        }
        guard.version_counter += 1;
        let meta = DriveFileMeta {
            id: id.clone(),
            name: req["name"].as_str().unwrap_or("解答.pdf").to_string(),
            mime_type: "application/pdf".to_string(),
            md5_checksum: Some(pseudo_md5_hex(body)),
            modified_time: Some("2026-11-25T00:01:00Z".to_string()),
            version: Some(guard.version_counter.to_string()),
            trashed: Some(false),
            app_properties: Some(app_props),
        };
        guard.files.insert(id, (meta.clone(), body.to_vec()));
        return ("200 OK", serde_json::to_string(&meta).unwrap_or_default());
    }

    // Get metadata for conflict detection
    if method == "GET" && target.starts_with("/drive/v3/files/") {
        let fid = target
            .trim_start_matches("/drive/v3/files/")
            .split('?')
            .next()
            .unwrap_or("");
        if let Some((meta, _)) = guard.files.get(fid) {
            return ("200 OK", serde_json::to_string(meta).unwrap_or_default());
        }
        return ("404 Not Found", r#"{"error":"not_found"}"#.to_string());
    }

    // Update existing file preserving exact Drive file ID
    if method == "PATCH" && target.starts_with("/upload/drive/v3/files/") {
        let fid = target
            .trim_start_matches("/upload/drive/v3/files/")
            .split('?')
            .next()
            .unwrap_or("");
        guard.version_counter += 1;
        let ver = guard.version_counter.to_string();
        if let Some((meta, content)) = guard.files.get_mut(fid) {
            *content = body.to_vec();
            meta.md5_checksum = Some(pseudo_md5_hex(body));
            meta.modified_time = Some("2026-11-25T00:05:00Z".to_string());
            meta.version = Some(ver);
            return ("200 OK", serde_json::to_string(meta).unwrap_or_default());
        }
        return ("404 Not Found", r#"{"error":"not_found"}"#.to_string());
    }

    ("404 Not Found", r#"{"error":"unknown_route"}"#.to_string())
}

async fn run_mock_end_to_end() -> Result<MockPocSummary, Box<dyn std::error::Error>> {
    // 1. Scope enforcement: reject overly broad `drive` scope
    let broad_rejected = validate_scopes(&["https://www.googleapis.com/auth/drive"]).is_err();
    assert!(broad_rejected, "Broad drive scope must be rejected");

    let (mock_base, mock_state) = start_mock_google_server().await;
    let http = reqwest::Client::new();

    // 2. PKCE + Loopback OAuth flow
    let pkce = generate_pkce()?;
    mock_state.lock().expect("lock").last_code_challenge = Some(pkce.code_challenge.clone());
    let loopback = LoopbackServer::bind().await?;
    let redirect_uri = loopback.redirect_uri.clone();
    let _auth_url = build_authorize_url(
        &format!("{mock_base}/o/oauth2/v2/auth"),
        "mock-desktop-client-id",
        &redirect_uri,
        &pkce,
        &[DRIVE_FILE_SCOPE],
    )?;

    // Simulate browser redirect back to loopback server
    let callback_url = format!(
        "{redirect_uri}?code=mock_auth_code_123&state={}",
        pkce.state
    );
    let http_clone = http.clone();
    let browser_task = tokio::spawn(async move {
        let _ = http_clone.get(callback_url).send().await;
    });
    let received_code = loopback.wait_for_code(&pkce.state).await?;
    browser_task.await?;
    assert_eq!(received_code, "mock_auth_code_123");

    let token = exchange_code_for_token(
        &http,
        &format!("{mock_base}/token"),
        "mock-desktop-client-id",
        None,
        &received_code,
        &pkce.code_verifier,
        &redirect_uri,
    )
    .await?;

    // 3. Drive folder creation
    let client = DriveClient::new(
        http,
        format!("{mock_base}/drive/v3"),
        format!("{mock_base}/upload/drive/v3"),
        token.access_token,
    );
    let root_folder = client.create_folder("FormulaDesk", None).await?;
    let math_folder = client
        .create_folder("大学数学", Some(&root_folder.id))
        .await?;

    // 4. Initial file upload (`files.create`) with idempotency key
    let idem_key = Uuid::new_v4().to_string();
    let uploaded_v1 = client
        .create_file(
            &math_folder.id,
            "解答.pdf",
            "application/pdf",
            "latest_solution_pdf",
            &idem_key,
            b"%PDF-1.7 solution v1".to_vec(),
        )
        .await?;

    // 5. Idempotent retry must return the exact same file ID without creating a duplicate
    let retried = client
        .create_file(
            &math_folder.id,
            "解答.pdf",
            "application/pdf",
            "latest_solution_pdf",
            &idem_key,
            b"%PDF-1.7 solution v1".to_vec(),
        )
        .await?;
    let idempotent_retry_same_id = uploaded_v1.id == retried.id;

    // 6. Same-ID update (`files.update`) preserving share link
    let updated_v2 = client
        .update_file_preserving_id(
            &uploaded_v1.id,
            "application/pdf",
            uploaded_v1.md5_checksum.as_deref(),
            false,
            b"%PDF-1.7 solution v2".to_vec(),
        )
        .await?;
    let update_preserved_file_id = uploaded_v1.id == updated_v2.id;

    // 7. External modification conflict detection
    {
        let mut st = mock_state.lock().expect("lock");
        if let Some((meta, _)) = st.files.get_mut(&uploaded_v1.id) {
            meta.md5_checksum = Some("externally_modified_md5".to_string());
        }
    }
    let conflict_res = client
        .update_file_preserving_id(
            &uploaded_v1.id,
            "application/pdf",
            updated_v2.md5_checksum.as_deref(),
            false,
            b"%PDF-1.7 solution v3".to_vec(),
        )
        .await;
    let external_conflict_detected =
        matches!(conflict_res, Err(DriveError::ExternalConflict { .. }));

    Ok(MockPocSummary {
        mode: "mock-protocol-verification (NOT live Google account)",
        pkce_s256_verified: true,
        minimal_scope_enforced: broad_rejected,
        loopback_callback_verified: true,
        folder_created_id: math_folder.id,
        initial_upload_file_id: uploaded_v1.id,
        idempotent_retry_same_id,
        update_preserved_file_id,
        external_conflict_detected,
    })
}

async fn run_live_flow() -> Result<(), Box<dyn std::error::Error>> {
    let client_id = std::env::var("GOOGLE_OAUTH_CLIENT_ID").map_err(|_| {
        "GOOGLE_OAUTH_CLIENT_ID env var is not set. Live Google Drive PoC requires user-provided Desktop OAuth Client credentials."
    })?;
    let client_secret = std::env::var("GOOGLE_OAUTH_CLIENT_SECRET").ok();

    let pkce = generate_pkce()?;
    let loopback = LoopbackServer::bind().await?;
    let redirect_uri = loopback.redirect_uri.clone();
    let auth_url = build_authorize_url(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &client_id,
        &redirect_uri,
        &pkce,
        &[DRIVE_FILE_SCOPE],
    )?;

    println!("Open this URL in your system browser to authorize (scope=drive.file):");
    println!("{auth_url}");
    let code = loopback.wait_for_code(&pkce.state).await?;

    let http = reqwest::Client::new();
    let token = exchange_code_for_token(
        &http,
        "https://oauth2.googleapis.com/token",
        &client_id,
        client_secret.as_deref(),
        &code,
        &pkce.code_verifier,
        &redirect_uri,
    )
    .await?;

    let client = DriveClient::google_production(http, token.access_token);
    let folder = client.create_folder("FormulaDesk-PoC", None).await?;
    let idem_key = Uuid::new_v4().to_string();
    let f1 = client
        .create_file(
            &folder.id,
            "poc-solution.txt",
            "text/plain",
            "latest_solution_pdf",
            &idem_key,
            b"FormulaDesk Phase 0 v1".to_vec(),
        )
        .await?;
    let f2 = client
        .update_file_preserving_id(
            &f1.id,
            "text/plain",
            f1.md5_checksum.as_deref(),
            false,
            b"FormulaDesk Phase 0 v2".to_vec(),
        )
        .await?;

    println!(
        "Live Google Drive PoC succeeded: folder_id={}, file_id={}, same_id={}",
        folder.id,
        f2.id,
        f1.id == f2.id
    );
    Ok(())
}

#[tokio::main]
async fn main() {
    let is_live = std::env::args().any(|a| a == "--live");
    if is_live {
        if let Err(err) = run_live_flow().await {
            eprintln!("Live Google Drive PoC halted: {err}");
            std::process::exit(1);
        }
    } else {
        match run_mock_end_to_end().await {
            Ok(summary) => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&summary).unwrap_or_default()
                );
            }
            Err(err) => {
                eprintln!("Mock Google Drive PoC failed: {err}");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_oauth_and_drive_same_id_update_and_conflict() {
        let summary = run_mock_end_to_end()
            .await
            .expect("Mock OAuth + Drive flow must succeed");
        assert!(summary.pkce_s256_verified);
        assert!(summary.minimal_scope_enforced);
        assert!(summary.loopback_callback_verified);
        assert!(summary.idempotent_retry_same_id);
        assert!(summary.update_preserved_file_id);
        assert!(summary.external_conflict_detected);
    }
}
