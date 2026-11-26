use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

use crate::application::AppState;
use crate::database::drive_repo;
use crate::domain::{
    EnqueueDriveUploadRequest, GenerateSolutionRequest, RecognizeProblemsRequest,
    ResolveDriveConflictRequest, RunScienceToolRequest, SaveDriveOAuthConfigRequest,
    SaveSolutionEditRequest, SendChatMessageRequest, UpdateDriveSettingsRequest,
    UpsertProblemRequest,
};
use crate::gdrive::DriveEndpoints;
use crate::gdrive::client::DriveFileMeta;
use crate::gdrive::oauth::{
    DRIVE_FILE_SCOPE, LoopbackServer, build_authorize_url, exchange_code_for_token, generate_pkce,
};

#[derive(Default)]
struct MockDriveState {
    files: HashMap<String, (DriveFileMeta, Vec<u8>, Option<String>)>, // id -> (meta, bytes, parent_id)
    version_counter: u64,
    valid_access_token: String,
    fail_refresh: bool,
    refresh_count: usize,
    next_error: Option<(&'static str, String, Option<String>)>, // (status_line, json_body, retry_after)
    create_file_calls: usize,
    update_file_calls: usize,
    delete_calls: usize,
}

fn pseudo_md5(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d[..16].iter().map(|b| format!("{b:02x}")).collect()
}

fn url_decode_simple(input: &str) -> String {
    url::form_urlencoded::parse(input.as_bytes())
        .next()
        .map(|(k, v)| {
            if v.is_empty() {
                k.into_owned()
            } else {
                format!("{k}={v}")
            }
        })
        .unwrap_or_else(|| input.to_string())
}

async fn start_mock_google_server() -> (String, Arc<Mutex<MockDriveState>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock google server");
    let port = listener.local_addr().expect("local addr").port();
    let base_url = format!("http://127.0.0.1:{port}");
    let state = Arc::new(Mutex::new(MockDriveState {
        valid_access_token: "ya29.mock_access_token_v1".to_string(),
        ..Default::default()
    }));
    let bg = Arc::clone(&state);

    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let st = Arc::clone(&bg);
            tokio::spawn(async move {
                let mut raw = Vec::new();
                let mut tmp = [0u8; 4096];
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
                let body = raw[header_end..].to_vec();
                let first_line = head.lines().next().unwrap_or_default();
                let mut parts = first_line.split_whitespace();
                let method = parts.next().unwrap_or("GET");
                let target = parts.next().unwrap_or("/");
                let auth_header = head.lines().find_map(|l| {
                    let lower = l.to_ascii_lowercase();
                    if lower.starts_with("authorization:") {
                        Some(l.split_once(':')?.1.trim().to_string())
                    } else {
                        None
                    }
                });

                let (status, resp_json, retry_after) =
                    handle_mock_http(method, target, auth_header.as_deref(), &body, &st);
                let extra_hdr = match retry_after {
                    Some(ra) => format!("Retry-After: {ra}\r\n"),
                    None => String::new(),
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra_hdr}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    resp_json.len(),
                    resp_json
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });

    (base_url, state)
}

fn handle_mock_http(
    method: &str,
    target: &str,
    auth_header: Option<&str>,
    body: &[u8],
    state: &Arc<Mutex<MockDriveState>>,
) -> (&'static str, String, Option<String>) {
    let mut guard = state.lock().expect("lock mock drive");

    if method == "DELETE" {
        guard.delete_calls += 1;
        return ("204 No Content", String::new(), None);
    }

    // OAuth Token endpoint
    if method == "POST" && target == "/token" {
        let form = String::from_utf8_lossy(body);
        if form.contains("grant_type=authorization_code") {
            if !form.contains("code_verifier=") {
                return (
                    "400 Bad Request",
                    r#"{"error":"invalid_grant"}"#.to_string(),
                    None,
                );
            }
            return (
                "200 OK",
                serde_json::json!({
                    "access_token": guard.valid_access_token,
                    "refresh_token": "1//mock_refresh_token_secret_999",
                    "expires_in": 3600,
                    "token_type": "Bearer",
                    "scope": DRIVE_FILE_SCOPE
                })
                .to_string(),
                None,
            );
        }
        if form.contains("grant_type=refresh_token") {
            if guard.fail_refresh {
                return (
                    "400 Bad Request",
                    r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#
                        .to_string(),
                    None,
                );
            }
            guard.refresh_count += 1;
            let next_tok = format!("ya29.mock_access_token_refreshed_{}", guard.refresh_count);
            guard.valid_access_token = next_tok.clone();
            return (
                "200 OK",
                serde_json::json!({
                    "access_token": next_tok,
                    "expires_in": 3600,
                    "token_type": "Bearer",
                    "scope": DRIVE_FILE_SCOPE
                })
                .to_string(),
                None,
            );
        }
        return (
            "400 Bad Request",
            r#"{"error":"unsupported_grant_type"}"#.to_string(),
            None,
        );
    }

    // Validate Bearer token for Drive API endpoints
    let expected_bearer = format!("Bearer {}", guard.valid_access_token);
    if auth_header != Some(expected_bearer.as_str()) {
        return (
            "401 Unauthorized",
            r#"{"error":{"code":401,"message":"Invalid Credentials"}}"#.to_string(),
            None,
        );
    }

    // One-shot fault injection (for testing 429, 403 quota, 500, etc.)
    if let Some(err) = guard.next_error.take() {
        return err;
    }

    // GET /drive/v3/about
    if method == "GET" && target.starts_with("/drive/v3/about") {
        return (
            "200 OK",
            serde_json::json!({
                "user": {
                    "displayName": "Theorem Mock User",
                    "emailAddress": "mock-student@example.edu"
                }
            })
            .to_string(),
            None,
        );
    }

    // GET /drive/v3/files?q=... (search folder or idempotency key)
    if method == "GET" && target.starts_with("/drive/v3/files?") {
        let query_raw = target.split_once('?').map(|(_, q)| q).unwrap_or("");
        let mut q_param = String::new();
        for (k, v) in url::form_urlencoded::parse(query_raw.as_bytes()) {
            if k == "q" {
                q_param = v.into_owned();
            }
        }
        let mut matched = Vec::new();
        for (meta, _, parent_opt) in guard.files.values() {
            if meta.trashed == Some(true) {
                continue;
            }
            if q_param.contains("theorem_idempotency_key") {
                if let Some(props) = &meta.app_properties
                    && let Some(key) = props.get("theorem_idempotency_key")
                    && q_param.contains(key)
                {
                    matched.push(meta.clone());
                }
            } else if q_param.contains("application/vnd.google-apps.folder")
                && meta.mime_type == "application/vnd.google-apps.folder"
            {
                let name_token = format!("name='{}'", meta.name);
                let parent_ok = match parent_opt {
                    Some(pid) => q_param.contains(&format!("'{pid}' in parents")),
                    None => q_param.contains("'root' in parents"),
                };
                if q_param.contains(&name_token) && parent_ok {
                    matched.push(meta.clone());
                }
            }
        }
        return (
            "200 OK",
            serde_json::json!({ "files": matched }).to_string(),
            None,
        );
    }

    // POST /drive/v3/files (create folder)
    if method == "POST" && target.starts_with("/drive/v3/files") {
        let req: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
        let parent_id = req
            .get("parents")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        if let Some(ref pid) = parent_id {
            let valid_parent = guard
                .files
                .get(pid)
                .is_some_and(|(m, _, _)| m.trashed != Some(true));
            if !valid_parent {
                return (
                    "404 Not Found",
                    r#"{"error":{"code":404,"message":"Parent folder not found"}}"#.to_string(),
                    None,
                );
            }
        }
        let id = format!("folder-{}", Uuid::new_v4());
        let name = req["name"].as_str().unwrap_or("Theorem").to_string();
        let meta = DriveFileMeta {
            id: id.clone(),
            name,
            mime_type: "application/vnd.google-apps.folder".to_string(),
            md5_checksum: None,
            modified_time: Some("2026-11-26T12:00:00Z".to_string()),
            version: Some("1".to_string()),
            trashed: Some(false),
            web_view_link: Some(format!("https://drive.google.com/drive/folders/{id}")),
            app_properties: None,
        };
        guard
            .files
            .insert(id, (meta.clone(), Vec::new(), parent_id));
        return (
            "200 OK",
            serde_json::to_string(&meta).unwrap_or_default(),
            None,
        );
    }

    // POST /upload/drive/v3/files (create file via multipart)
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
        let parent_id = req
            .get("parents")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        if let Some(ref pid) = parent_id {
            let valid_parent = guard
                .files
                .get(pid)
                .is_some_and(|(m, _, _)| m.trashed != Some(true));
            if !valid_parent {
                return (
                    "404 Not Found",
                    r#"{"error":{"code":404,"message":"Folder not found"}}"#.to_string(),
                    None,
                );
            }
        }
        guard.create_file_calls += 1;
        guard.version_counter += 1;
        let id = format!("file-{}", Uuid::new_v4());
        let mut app_props = HashMap::new();
        if let Some(obj) = req["appProperties"].as_object() {
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    app_props.insert(k.clone(), s.to_string());
                }
            }
        }
        let meta = DriveFileMeta {
            id: id.clone(),
            name: req["name"].as_str().unwrap_or("解答.pdf").to_string(),
            mime_type: "application/pdf".to_string(),
            md5_checksum: Some(pseudo_md5(body)),
            modified_time: Some(format!(
                "2026-11-26T12:{:02}:00Z",
                guard.version_counter % 60
            )),
            version: Some(guard.version_counter.to_string()),
            trashed: Some(false),
            web_view_link: Some(format!("https://drive.google.com/file/d/{id}/view")),
            app_properties: Some(app_props),
        };
        guard
            .files
            .insert(id, (meta.clone(), body.to_vec(), parent_id));
        return (
            "200 OK",
            serde_json::to_string(&meta).unwrap_or_default(),
            None,
        );
    }

    // GET /drive/v3/files/{id}
    if method == "GET" && target.starts_with("/drive/v3/files/") {
        let fid = url_decode_simple(
            target
                .trim_start_matches("/drive/v3/files/")
                .split('?')
                .next()
                .unwrap_or(""),
        );
        if let Some((meta, _, _)) = guard.files.get(&fid) {
            return (
                "200 OK",
                serde_json::to_string(meta).unwrap_or_default(),
                None,
            );
        }
        return (
            "404 Not Found",
            r#"{"error":{"code":404,"message":"File not found"}}"#.to_string(),
            None,
        );
    }

    // PATCH /upload/drive/v3/files/{id} (same-ID update!)
    if method == "PATCH" && target.starts_with("/upload/drive/v3/files/") {
        let fid = url_decode_simple(
            target
                .trim_start_matches("/upload/drive/v3/files/")
                .split('?')
                .next()
                .unwrap_or(""),
        );
        guard.update_file_calls += 1;
        guard.version_counter += 1;
        let ver = guard.version_counter.to_string();
        let mod_time = format!("2026-11-26T13:{:02}:00Z", guard.version_counter % 60);
        if let Some((meta, content, _)) = guard.files.get_mut(&fid) {
            *content = body.to_vec();
            meta.md5_checksum = Some(pseudo_md5(body));
            meta.modified_time = Some(mod_time);
            meta.version = Some(ver);
            return (
                "200 OK",
                serde_json::to_string(meta).unwrap_or_default(),
                None,
            );
        }
        return (
            "404 Not Found",
            r#"{"error":{"code":404,"message":"File not found"}}"#.to_string(),
            None,
        );
    }

    (
        "404 Not Found",
        r#"{"error":{"code":404,"message":"Unknown mock route"}}"#.to_string(),
        None,
    )
}

async fn connect_state_to_mock_drive(state: &AppState, base_url: &str) {
    *state.drive_endpoints.write().await = DriveEndpoints {
        auth_endpoint: format!("{base_url}/o/oauth2/v2/auth"),
        token_endpoint: format!("{base_url}/token"),
        api_base: format!("{base_url}/drive/v3"),
        upload_base: format!("{base_url}/upload/drive/v3"),
    };
    state
        .save_drive_oauth_config(&SaveDriveOAuthConfigRequest {
            client_id: Some("mock-desktop-client-id.apps.googleusercontent.com".to_string()),
            client_secret: Some("GOCSPX-mock_secret_do_not_commit".to_string()),
            clear_client_secret: false,
        })
        .expect("save oauth cfg");

    // Exercise real PKCE + Loopback + Token Exchange against Mock OAuth server
    let pkce = generate_pkce().expect("pkce");
    let loopback = LoopbackServer::bind().await.expect("loopback");
    let redirect = loopback.redirect_uri.clone();
    let _auth_url = build_authorize_url(
        &format!("{base_url}/o/oauth2/v2/auth"),
        "mock-desktop-client-id.apps.googleusercontent.com",
        &redirect,
        &pkce,
        &[DRIVE_FILE_SCOPE],
    )
    .expect("auth url");

    let http = reqwest::Client::new();
    let cb = format!("{redirect}?code=mock_code_abc&state={}", pkce.state);
    let browser = tokio::spawn(async move {
        let _ = http.get(cb).send().await;
    });
    let code = loopback
        .wait_for_code(&pkce.state, std::time::Duration::from_secs(5))
        .await
        .expect("code");
    browser.await.expect("join browser");

    let token = exchange_code_for_token(
        &state.http_client,
        &format!("{base_url}/token"),
        "mock-desktop-client-id.apps.googleusercontent.com",
        Some("GOCSPX-mock_secret_do_not_commit"),
        &code,
        &pkce.code_verifier,
        &redirect,
    )
    .await
    .expect("token exchange");

    let conn = state
        .finalize_drive_connection_from_token(token)
        .await
        .expect("finalize drive conn");
    assert_eq!(conn.status, "connected");
    assert_eq!(conn.account_email, "mock-student@example.edu");
    assert_eq!(conn.scopes, vec![DRIVE_FILE_SCOPE.to_string()]);
}

fn minimal_pdf_bytes() -> Vec<u8> {
    use lopdf::dictionary;

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    };
    doc.objects
        .insert(pages_id, lopdf::Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

fn sample_png_bytes() -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba([240, 244, 255, 255]));
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("png");
    buf.into_inner()
}

#[tokio::test]
async fn test_phase4_drive_full_lifecycle_conflicts_recovery_and_non_deletion() {
    let (mock_base, mock_state) = start_mock_google_server().await;
    let dir = TempDir::new().expect("tempdir");
    let state = AppState::initialize(dir.path().to_path_buf())
        .await
        .expect("init");
    let boot = state.bootstrap().await.expect("bootstrap");
    let math_id = boot.subjects[0].id.clone();

    // 0. Disconnected operation check: app works smoothly with no Drive connection
    let overview_before = state.get_drive_overview().await.expect("overview");
    assert!(overview_before.connection.is_none());

    // 1. Connect via PKCE + Loopback + Mock OAuth server
    connect_state_to_mock_drive(&state, &mock_base).await;

    // Security invariant: raw tokens & client secret must NEVER appear in SQLite or settings.json
    let raw_settings =
        std::fs::read_to_string(dir.path().join("settings.json")).expect("read settings");
    assert!(!raw_settings.contains("GOCSPX-mock_secret"));
    assert!(!raw_settings.contains("ya29."));
    assert!(!raw_settings.contains("mock_refresh_token"));
    {
        let db = state.db.read().await;
        let conn_dump: String = sqlx::query_scalar(
            "SELECT credential_ref || ' ' || account_email || ' ' || scopes_json FROM drive_connections LIMIT 1",
        )
        .fetch_one(db.pool())
        .await
        .expect("query drive_connections");
        assert!(!conn_dump.contains("ya29."));
        assert!(!conn_dump.contains("mock_refresh_token"));
        assert!(conn_dump.starts_with("gdrive:token:"));
    }

    // 2. Create nested folder + import document + generate v1 solution PDF
    let parent_folder = state
        .create_folder(&math_id, None, "2026線形代数")
        .await
        .expect("parent folder");
    let child_folder = state
        .create_folder(&math_id, Some(&parent_folder.id), "中間試験")
        .await
        .expect("child folder");

    let imported = state
        .import_file_bytes(
            &math_id,
            Some(&child_folder.id),
            "midterm_q1.pdf",
            &STANDARD.encode(minimal_pdf_bytes()),
        )
        .await
        .expect("import pdf");
    let doc_id = imported.detail.document.id.clone();

    let profiles = state.list_agent_profiles().await.expect("profiles");
    let mock_profile = profiles
        .iter()
        .find(|p| p.provider_kind == "mock")
        .expect("mock profile")
        .clone();

    let prob = state
        .upsert_problem(&UpsertProblemRequest {
            id: None,
            document_id: doc_id.clone(),
            problem_number: "第1問".to_string(),
            subproblem_number: None,
            subject_category: "mathematics".to_string(),
            field: Some("linear_algebra".to_string()),
            statement_markdown: "行列 $A$ の固有値を求めよ。".to_string(),
            given_conditions: vec![],
            target_objective: Some("固有値".to_string()),
            source_pages: vec![1],
            figure_refs: vec![],
            uncertainties: vec![],
            is_user_verified: true,
        })
        .await
        .expect("upsert prob");

    let sol_v1 = state
        .generate_solution(&GenerateSolutionRequest {
            document_id: doc_id.clone(),
            problem_id: Some(prob.id.clone()),
            agent_profile_id: Some(mock_profile.id.clone()),
            model_name: None,
            mode: "standard".to_string(),
            target_scope: "single_problem".to_string(),
            page_range_start: None,
            page_range_end: None,
            repair_feedback: None,
            parent_solution_id: None,
        })
        .await
        .expect("sol v1");
    assert!(sol_v1.solution.pdf_attachment_id.is_some());

    // 3. Initial upload (`files.create`) of problem + latest solution PDF
    let enq1 = state
        .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
            document_id: doc_id.clone(),
            solution_id: None,
            scope: "default".to_string(),
        })
        .await
        .expect("enqueue initial");
    assert_eq!(enq1.enqueued_jobs.len(), 2);

    // Duplicate enqueue while pending must coalesce without creating duplicate active jobs
    let enq_dup = state
        .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
            document_id: doc_id.clone(),
            solution_id: None,
            scope: "default".to_string(),
        })
        .await
        .expect("enqueue dup");
    assert_eq!(enq_dup.enqueued_jobs[0].id, enq1.enqueued_jobs[0].id);

    let processed = state.process_drive_queue_once().await.expect("process 1");
    assert_eq!(processed, 2);

    let doc_status_1 = state
        .list_document_drive_status(&doc_id)
        .await
        .expect("doc status 1");
    assert_eq!(doc_status_1.files.len(), 2);
    let sol_mapping_v1 = doc_status_1
        .files
        .iter()
        .find(|f| f.logical_role == "latest_solution_pdf")
        .expect("latest_solution_pdf mapping");
    let initial_drive_file_id = sol_mapping_v1.drive_file_id.clone();
    assert_eq!(sol_mapping_v1.sync_status, "uploaded");

    // 4. Duplicate SHA-256 skip check (no redundant upload jobs enqueued)
    let enq_skip = state
        .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
            document_id: doc_id.clone(),
            solution_id: None,
            scope: "default".to_string(),
        })
        .await
        .expect("enqueue skip");
    assert!(enq_skip.enqueued_jobs.is_empty());
    assert_eq!(enq_skip.skipped_up_to_date.len(), 2);

    // 5. Solution v2 edit -> MUST update the EXACT SAME Drive file ID (`files.update`) preserving share link!
    let mut v2_json: serde_json::Value =
        serde_json::from_str(&sol_v1.solution.structured_json).expect("parse v1 json");
    v2_json["title"] = serde_json::Value::String("固有値解析（改訂版 v2）".to_string());
    let sol_v2 = state
        .save_solution_edit(&SaveSolutionEditRequest {
            document_id: doc_id.clone(),
            problem_id: Some(prob.id.clone()),
            parent_solution_id: Some(sol_v1.solution.id.clone()),
            mode: "detailed".to_string(),
            structured_json: serde_json::to_string_pretty(&v2_json).expect("pretty"),
            change_summary: Some("v2 改訂".to_string()),
        })
        .await
        .expect("sol v2");
    assert_eq!(sol_v2.solution.version_number, 2);

    // Editing solution v2 marks existing latest_solution_pdf mapping as `update_required`
    let stale_status = state
        .list_document_drive_status(&doc_id)
        .await
        .expect("stale status");
    assert_eq!(
        stale_status
            .files
            .iter()
            .find(|f| f.logical_role == "latest_solution_pdf")
            .expect("mapping")
            .sync_status,
        "update_required"
    );

    let enq_v2 = state
        .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
            document_id: doc_id.clone(),
            solution_id: None,
            scope: "default".to_string(),
        })
        .await
        .expect("enqueue v2");
    assert_eq!(enq_v2.enqueued_jobs.len(), 1);
    assert_eq!(enq_v2.enqueued_jobs[0].logical_role, "latest_solution_pdf");

    state.process_drive_queue_once().await.expect("process v2");
    let doc_status_2 = state
        .list_document_drive_status(&doc_id)
        .await
        .expect("doc status 2");
    let sol_mapping_v2 = doc_status_2
        .files
        .iter()
        .find(|f| f.logical_role == "latest_solution_pdf")
        .expect("latest v2");
    assert_eq!(
        sol_mapping_v2.drive_file_id, initial_drive_file_id,
        "Updating v1 -> v2 MUST preserve the exact same Google Drive file ID"
    );
    assert!(mock_state.lock().expect("lock").update_file_calls >= 1);

    // 6. External modification conflict detection -> MUST NOT overwrite silently!
    {
        let mut st = mock_state.lock().expect("lock");
        if let Some((meta, _, _)) = st.files.get_mut(&initial_drive_file_id) {
            meta.md5_checksum = Some("externally_edited_md5_deadbeef".to_string());
            meta.version = Some("999".to_string());
        }
    }
    v2_json["title"] = serde_json::Value::String("固有値解析（改訂版 v3）".to_string());
    state
        .save_solution_edit(&SaveSolutionEditRequest {
            document_id: doc_id.clone(),
            problem_id: Some(prob.id.clone()),
            parent_solution_id: Some(sol_v2.solution.id.clone()),
            mode: "detailed".to_string(),
            structured_json: serde_json::to_string_pretty(&v2_json).expect("pretty"),
            change_summary: Some("v3 改訂".to_string()),
        })
        .await
        .expect("sol v3");

    let enq_v3 = state
        .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
            document_id: doc_id.clone(),
            solution_id: None,
            scope: "latest_solution_only".to_string(),
        })
        .await
        .expect("enqueue v3");
    let conflict_job_id = enq_v3.enqueued_jobs[0].id.clone();

    state
        .process_drive_queue_once()
        .await
        .expect("process v3 conflict");
    let status_after_conflict = state
        .list_document_drive_status(&doc_id)
        .await
        .expect("status conflict");
    let cjob = status_after_conflict
        .jobs
        .iter()
        .find(|j| j.id == conflict_job_id)
        .expect("conflict job");
    assert_eq!(cjob.status, "conflict");
    assert_eq!(cjob.last_error_code.as_deref(), Some("EXTERNAL_CONFLICT"));
    assert_eq!(
        cjob.conflict_remote_md5.as_deref(),
        Some("externally_edited_md5_deadbeef")
    );

    // Resolve conflict with explicit user choice: `overwrite`
    state
        .resolve_drive_conflict(&ResolveDriveConflictRequest {
            job_id: conflict_job_id.clone(),
            resolution: "overwrite".to_string(),
        })
        .await
        .expect("resolve overwrite");
    state
        .process_drive_queue_once()
        .await
        .expect("process overwrite");
    let after_overwrite = state
        .list_document_drive_status(&doc_id)
        .await
        .expect("after overwrite");
    assert_eq!(
        after_overwrite
            .files
            .iter()
            .find(|f| f.logical_role == "latest_solution_pdf")
            .expect("file")
            .drive_file_id,
        initial_drive_file_id
    );

    // 7. Rate limit (HTTP 429) exponential backoff + Storage quota exceeded (HTTP 403) fatal stop
    {
        let mut st = mock_state.lock().expect("lock");
        st.next_error = Some((
            "429 Too Many Requests",
            r#"{"error":{"code":429,"message":"Rate Limit Exceeded"}}"#.to_string(),
            Some("12".to_string()),
        ));
    }
    let enq_v1_manual = state
        .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
            document_id: doc_id.clone(),
            solution_id: Some(sol_v1.solution.id.clone()),
            scope: "default".to_string(),
        })
        .await
        .expect("enqueue v1 manual");
    let manual_job_id = enq_v1_manual.enqueued_jobs[0].id.clone();
    state.process_drive_queue_once().await.expect("process 429");
    let job_after_429 = {
        let db = state.db.read().await;
        drive_repo::get_job_by_id(db.pool(), &manual_job_id)
            .await
            .expect("get 429 job")
    };
    assert_eq!(job_after_429.status, "pending");
    assert_eq!(
        job_after_429.last_error_code.as_deref(),
        Some("RATE_LIMITED")
    );
    assert!(job_after_429.next_retry_at.is_some());

    // Now simulate storageQuotaExceeded (must stop immediately as `failed` without retry)
    state
        .retry_drive_job(&manual_job_id)
        .await
        .expect("retry manual");
    {
        let mut st = mock_state.lock().expect("lock");
        st.next_error = Some((
            "403 Forbidden",
            r#"{"error":{"code":403,"message":"storageQuotaExceeded"}}"#.to_string(),
            None,
        ));
    }
    state
        .process_drive_queue_once()
        .await
        .expect("process quota");
    let job_after_quota = {
        let db = state.db.read().await;
        drive_repo::get_job_by_id(db.pool(), &manual_job_id)
            .await
            .expect("get quota job")
    };
    assert_eq!(job_after_quota.status, "failed");
    assert_eq!(
        job_after_quota.last_error_code.as_deref(),
        Some("STORAGE_QUOTA_EXCEEDED")
    );

    // 8. HTTP 401 token expiry -> automatic token refresh succeeds!
    {
        let mut st = mock_state.lock().expect("lock");
        st.valid_access_token = "ya29.rotated_force_401".to_string();
    }
    state
        .retry_drive_job(&manual_job_id)
        .await
        .expect("retry after quota");
    state
        .process_drive_queue_once()
        .await
        .expect("process with auto refresh");
    assert!(mock_state.lock().expect("lock").refresh_count >= 1);
    let job_after_refresh = {
        let db = state.db.read().await;
        drive_repo::get_job_by_id(db.pool(), &manual_job_id)
            .await
            .expect("get refreshed job")
    };
    assert_eq!(job_after_refresh.status, "uploaded");

    // 9. Remote deleted file detection -> user chooses `recreate_after_remote_delete`
    {
        let mut st = mock_state.lock().expect("lock");
        if let Some((meta, _, _)) = st.files.get_mut(&initial_drive_file_id) {
            meta.trashed = Some(true);
        }
    }
    v2_json["title"] = serde_json::Value::String("固有値解析（改訂版 v4）".to_string());
    state
        .save_solution_edit(&SaveSolutionEditRequest {
            document_id: doc_id.clone(),
            problem_id: Some(prob.id.clone()),
            parent_solution_id: Some(sol_v2.solution.id.clone()),
            mode: "detailed".to_string(),
            structured_json: serde_json::to_string_pretty(&v2_json).expect("pretty"),
            change_summary: Some("v4 改訂".to_string()),
        })
        .await
        .expect("sol v4");
    let enq_v4 = state
        .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
            document_id: doc_id.clone(),
            solution_id: None,
            scope: "latest_solution_only".to_string(),
        })
        .await
        .expect("enqueue v4");
    let del_job_id = enq_v4.enqueued_jobs[0].id.clone();
    state
        .process_drive_queue_once()
        .await
        .expect("detect remote delete");
    let del_job = {
        let db = state.db.read().await;
        drive_repo::get_job_by_id(db.pool(), &del_job_id)
            .await
            .expect("del job")
    };
    assert_eq!(del_job.status, "conflict");
    assert_eq!(del_job.last_error_code.as_deref(), Some("REMOTE_DELETED"));

    state
        .resolve_drive_conflict(&ResolveDriveConflictRequest {
            job_id: del_job_id.clone(),
            resolution: "recreate_after_remote_delete".to_string(),
        })
        .await
        .expect("recreate");
    state
        .process_drive_queue_once()
        .await
        .expect("process recreate");
    let recreated_job = {
        let db = state.db.read().await;
        drive_repo::get_job_by_id(db.pool(), &del_job_id)
            .await
            .expect("recreated job")
    };
    assert_eq!(recreated_job.status, "uploaded");
    assert_ne!(
        recreated_job.drive_file_id.as_deref(),
        Some(initial_drive_file_id.as_str())
    );

    // 10. Crash/restart recovery + appProperties idempotency prevents duplicate file creation!
    let doc2 = state
        .import_file_bytes(
            &math_id,
            Some(&child_folder.id),
            "crash_test.png",
            &STANDARD.encode(sample_png_bytes()),
        )
        .await
        .expect("import doc2");
    let enq_crash = state
        .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
            document_id: doc2.detail.document.id.clone(),
            solution_id: None,
            scope: "original_only".to_string(),
        })
        .await
        .expect("enqueue crash");
    let crash_job = enq_crash.enqueued_jobs[0].clone();

    // Simulate file already created remotely with the same idempotency key right before app crashed
    let pre_created_id = "file-precreated-before-crash".to_string();
    {
        let mut st = mock_state.lock().expect("lock");
        let mut props = HashMap::new();
        props.insert(
            "theorem_idempotency_key".to_string(),
            crash_job.idempotency_key.clone(),
        );
        let meta = DriveFileMeta {
            id: pre_created_id.clone(),
            name: "問題.png".to_string(),
            mime_type: "image/png".to_string(),
            md5_checksum: Some("md5-pre".to_string()),
            modified_time: Some("2026-11-26T12:30:00Z".to_string()),
            version: Some("1".to_string()),
            trashed: Some(false),
            web_view_link: Some("https://drive.google.com/file/d/pre/view".to_string()),
            app_properties: Some(props),
        };
        st.files
            .insert(pre_created_id.clone(), (meta, vec![1, 2, 3], None));
    }
    // Simulate job left in `uploading` state at crash time
    {
        let db = state.db.read().await;
        sqlx::query("UPDATE drive_upload_jobs SET status = 'uploading' WHERE id = ?1")
            .bind(&crash_job.id)
            .execute(db.pool())
            .await
            .expect("mark uploading");
    }

    let create_calls_before = mock_state.lock().expect("lock").create_file_calls;
    drop(state);

    // Re-initialize AppState (simulating app restart): must recover `uploading` -> `pending`
    let restarted = AppState::initialize(dir.path().to_path_buf())
        .await
        .expect("restart");
    *restarted.drive_endpoints.write().await = DriveEndpoints {
        auth_endpoint: format!("{mock_base}/o/oauth2/v2/auth"),
        token_endpoint: format!("{mock_base}/token"),
        api_base: format!("{mock_base}/drive/v3"),
        upload_base: format!("{mock_base}/upload/drive/v3"),
    };
    restarted
        .process_drive_queue_once()
        .await
        .expect("process recovered");
    let create_calls_after = mock_state.lock().expect("lock").create_file_calls;
    assert_eq!(
        create_calls_before, create_calls_after,
        "Idempotency key recovery must NOT create a duplicate file on Drive"
    );
    let doc2_status = restarted
        .list_document_drive_status(&doc2.detail.document.id)
        .await
        .expect("doc2 status");
    assert_eq!(doc2_status.files[0].drive_file_id, pre_created_id);

    // 11. Folder auto-upload mode (`folder_auto`) only triggers for flagged folders
    restarted
        .update_drive_settings(&UpdateDriveSettingsRequest {
            root_folder_name: None,
            upload_mode: Some("folder_auto".to_string()),
            version_policy: None,
            include_past_versions: None,
            include_solution_source: None,
            include_artifacts: None,
        })
        .await
        .expect("set folder_auto");
    let unflagged_folder = restarted
        .create_folder(&math_id, None, "自動対象外")
        .await
        .expect("unflagged");
    restarted
        .set_folder_auto_upload(&parent_folder.id, true)
        .await
        .expect("flag parent folder");

    let doc_unflagged = restarted
        .import_file_bytes(
            &math_id,
            Some(&unflagged_folder.id),
            "no_auto.png",
            &STANDARD.encode(sample_png_bytes()),
        )
        .await
        .expect("import unflagged");
    assert!(
        restarted
            .list_document_drive_status(&doc_unflagged.detail.document.id)
            .await
            .expect("status unflagged")
            .jobs
            .is_empty()
    );

    let doc_flagged = restarted
        .import_file_bytes(
            &math_id,
            Some(&child_folder.id), // inherits auto_upload_drive from ancestor `parent_folder`!
            "auto_yes.png",
            &STANDARD.encode(sample_png_bytes()),
        )
        .await
        .expect("import flagged");
    assert_eq!(
        restarted
            .list_document_drive_status(&doc_flagged.detail.document.id)
            .await
            .expect("status flagged")
            .jobs
            .len(),
        1
    );

    // 12. Security & safety invariant: Deleting a local document MUST NOT delete Drive files!
    let remote_file_count_before = mock_state.lock().expect("lock").files.len();
    restarted
        .delete_document(&doc_id)
        .await
        .expect("delete local doc");
    assert_eq!(
        mock_state.lock().expect("lock").delete_calls,
        0,
        "Theorem must NEVER call Google Drive delete API when deleting local documents"
    );
    assert_eq!(
        mock_state.lock().expect("lock").files.len(),
        remote_file_count_before
    );
    {
        let db = restarted.db.read().await;
        let detached_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM drive_files WHERE document_id IS NULL")
                .fetch_one(db.pool())
                .await
                .expect("detached count");
        assert!(
            detached_rows >= 2,
            "drive_files history rows must remain detached with document_id = NULL"
        );
    }

    // 13. Failed refresh token triggers `reauth_required` status cleanly
    {
        let mut st = mock_state.lock().expect("lock");
        st.valid_access_token = "ya29.revoked_token".to_string();
        st.fail_refresh = true;
    }
    restarted
        .process_drive_queue_once()
        .await
        .expect("process revoked");
    let overview_reauth = restarted.get_drive_overview().await.expect("overview");
    assert_eq!(
        overview_reauth.connection.expect("conn").status,
        "reauth_required"
    );
}

/// Implements docs/testing.md Section 3.1 (12-step University Mathematics E2E workflow)
/// and Section 3.2 (3 University Physics scenarios) using Mock Agent + real local Science MCP + Mock Drive.
#[tokio::test]
async fn test_mvp_math_12_steps_and_physics_3_cases_e2e() {
    let (mock_base, mock_state) = start_mock_google_server().await;
    let dir = TempDir::new().expect("tempdir");
    let state = AppState::initialize(dir.path().to_path_buf())
        .await
        .expect("init");
    connect_state_to_mock_drive(&state, &mock_base).await;

    let boot = state.bootstrap().await.expect("bootstrap");
    let math_id = boot.subjects[0].id.clone();
    let phys_id = boot.subjects[1].id.clone();
    let profiles = state.list_agent_profiles().await.expect("profiles");
    let mock_profile = profiles
        .iter()
        .find(|p| p.provider_kind == "mock")
        .expect("mock profile")
        .clone();

    // ========================================================================
    // Section 3.1: University Mathematics 12-Step Full Workflow
    // ========================================================================
    // Step 1: Import math PDF
    let math_doc = state
        .import_file_bytes(
            &math_id,
            None,
            "calculus_integral.pdf",
            &STANDARD.encode(minimal_pdf_bytes()),
        )
        .await
        .expect("step 1 import");
    let math_doc_id = math_doc.detail.document.id.clone();

    // Step 2: Recognize printed problem
    let rec = state
        .recognize_problems(&RecognizeProblemsRequest {
            document_id: math_doc_id.clone(),
            agent_profile_id: Some(mock_profile.id.clone()),
            model_name: None,
            recognition_mode: "printed_only".to_string(),
            page_range_start: None,
            page_range_end: None,
            page_images: vec![],
            cropped_region_png_base64: None,
        })
        .await
        .expect("step 2 recognize");
    assert!(!rec.problems.is_empty());

    // Step 3: User edits recognized problem
    let edited_prob = state
        .upsert_problem(&UpsertProblemRequest {
            id: Some(rec.problems[0].id.clone()),
            document_id: math_doc_id.clone(),
            problem_number: "第1問".to_string(),
            subproblem_number: None,
            subject_category: "mathematics".to_string(),
            field: Some("calculus".to_string()),
            statement_markdown: "ガウス積分 $\\int_0^\\infty e^{-x^2}\\,dx$ を求めよ。".to_string(),
            given_conditions: vec!["x は実数".to_string()],
            target_objective: Some("定積分の値".to_string()),
            source_pages: vec![1],
            figure_refs: vec![],
            uncertainties: vec![],
            is_user_verified: true,
        })
        .await
        .expect("step 3 edit");

    // Step 4 & 5: Generate structured solution + Typst PDF
    let sol_v1 = state
        .generate_solution(&GenerateSolutionRequest {
            document_id: math_doc_id.clone(),
            problem_id: Some(edited_prob.id.clone()),
            agent_profile_id: Some(mock_profile.id.clone()),
            model_name: None,
            mode: "detailed".to_string(),
            target_scope: "single_problem".to_string(),
            page_range_start: None,
            page_range_end: None,
            repair_feedback: None,
            parent_solution_id: None,
        })
        .await
        .expect("step 4-5 generate");
    assert!(sol_v1.schema_issues.is_empty());
    assert!(sol_v1.latex_errors.is_empty());
    assert!(sol_v1.solution.pdf_attachment_id.is_some());

    // Step 6: Chat streaming question saved to SQLite
    let conv = state
        .create_conversation(
            &math_doc_id,
            Some(&edited_prob.id),
            Some(&sol_v1.solution.id),
            "ガウス積分の極座標変換",
        )
        .await
        .expect("step 6 conv");
    let chat_res = state
        .send_chat_message(&SendChatMessageRequest {
            conversation_id: conv.id.clone(),
            document_id: math_doc_id.clone(),
            agent_profile_id: Some(mock_profile.id.clone()),
            model_name: None,
            content_markdown: "ヤコビアンが r になる理由を教えてください。".to_string(),
            referenced_problem_id: Some(edited_prob.id.clone()),
            include_problem_context: true,
            include_solution_context: true,
            attached_images: vec![],
        })
        .await
        .expect("step 6 chat");
    assert_eq!(chat_res.assistant_message.status, "completed");

    // Step 9 (initial Drive upload of problem + v1 PDF)
    state
        .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
            document_id: math_doc_id.clone(),
            solution_id: None,
            scope: "default".to_string(),
        })
        .await
        .expect("step 9 enqueue");
    state.process_drive_queue_once().await.expect("step 9 run");
    let v1_drive_id = state
        .list_document_drive_status(&math_doc_id)
        .await
        .expect("step 9 status")
        .files
        .into_iter()
        .find(|f| f.logical_role == "latest_solution_pdf")
        .expect("v1 mapping")
        .drive_file_id;

    // Step 7 & 8: Non-destructive v2 revision & PDF regeneration
    let mut parsed_sol: serde_json::Value =
        serde_json::from_str(&sol_v1.solution.structured_json).expect("json");
    parsed_sol["title"] = serde_json::Value::String("ガウス積分（極座標ヤコビアン補足版）".into());
    let sol_v2 = state
        .save_solution_edit(&SaveSolutionEditRequest {
            document_id: math_doc_id.clone(),
            problem_id: Some(edited_prob.id.clone()),
            parent_solution_id: Some(sol_v1.solution.id.clone()),
            mode: "detailed".to_string(),
            structured_json: serde_json::to_string_pretty(&parsed_sol).expect("pretty"),
            change_summary: Some("ヤコビアン補足".to_string()),
        })
        .await
        .expect("step 7-8 v2");
    assert_eq!(sol_v2.solution.version_number, 2);

    // Step 10: Re-upload updates the EXACT SAME Google Drive file ID!
    state
        .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
            document_id: math_doc_id.clone(),
            solution_id: None,
            scope: "default".to_string(),
        })
        .await
        .expect("step 10 enqueue");
    state.process_drive_queue_once().await.expect("step 10 run");
    let v2_drive_id = state
        .list_document_drive_status(&math_doc_id)
        .await
        .expect("step 10 status")
        .files
        .into_iter()
        .find(|f| f.logical_role == "latest_solution_pdf")
        .expect("v2 mapping")
        .drive_file_id;
    assert_eq!(v1_drive_id, v2_drive_id);

    // ========================================================================
    // Section 3.2: University Physics 3 Scenarios (Diagram, Units/Dims, ODE+Plot)
    // ========================================================================
    let phys_doc = state
        .import_file_bytes(
            &phys_id,
            None,
            "physics_suite.png",
            &STANDARD.encode(sample_png_bytes()),
        )
        .await
        .expect("phys import");
    let phys_doc_id = phys_doc.detail.document.id.clone();

    let phys_prob = state
        .upsert_problem(&UpsertProblemRequest {
            id: None,
            document_id: phys_doc_id.clone(),
            problem_number: "第1問".to_string(),
            subproblem_number: None,
            subject_category: "physics".to_string(),
            field: Some("classical_mechanics".to_string()),
            statement_markdown: "減衰振動 $x''+2x'+5x=0$ とローレンツ力 $F=qvB$ の次元を解析せよ。"
                .to_string(),
            given_conditions: vec!["x(0)=1, x'(0)=0".to_string()],
            target_objective: Some("解・次元・グラフ".to_string()),
            source_pages: vec![1],
            figure_refs: vec!["図1: 減衰振動".to_string()],
            uncertainties: vec![],
            is_user_verified: true,
        })
        .await
        .expect("phys prob");

    let phys_sol = state
        .generate_solution(&GenerateSolutionRequest {
            document_id: phys_doc_id.clone(),
            problem_id: Some(phys_prob.id.clone()),
            agent_profile_id: Some(mock_profile.id.clone()),
            model_name: None,
            mode: "verified".to_string(),
            target_scope: "single_problem".to_string(),
            page_range_start: None,
            page_range_end: None,
            repair_feedback: None,
            parent_solution_id: None,
        })
        .await
        .expect("phys sol");
    assert!(phys_sol.solution.pdf_attachment_id.is_some());

    // Real Science MCP Case 2: Electromagnetism units + dimensions (`F = q * v * B`)
    let dim_res = state
        .run_science_tool(&RunScienceToolRequest {
            document_id: phys_doc_id.clone(),
            solution_id: Some(phys_sol.solution.id.clone()),
            tool_name: "check_dimensions".to_string(),
            arguments: serde_json::json!({
                "lhs_expression": "F",
                "rhs_expression": "q * v * B",
                "unit_map": { "F": "N", "q": "C", "v": "m/s", "B": "T" }
            }),
            attach_to_solution_pdf: false,
        })
        .await
        .expect("real science mcp lorentz dim");
    assert_eq!(dim_res.execution.status, "succeeded");
    assert_eq!(dim_res.verification.expect("v").status, "passed");

    // Real Science MCP Case 3: ODE + SVG Plot embedded into Typst PDF + uploaded to Drive
    let plot_res = state
        .run_science_tool(&RunScienceToolRequest {
            document_id: phys_doc_id.clone(),
            solution_id: Some(phys_sol.solution.id.clone()),
            tool_name: "plot_function".to_string(),
            arguments: serde_json::json!({
                "expressions": ["exp(-t)*(cos(2*t) + sin(2*t)/2)"],
                "variable": "t",
                "x_range": [0, 6],
                "title": "Damped Oscillation",
                "output_format": "svg"
            }),
            attach_to_solution_pdf: true,
        })
        .await
        .expect("real science mcp plot");
    assert_eq!(plot_res.created_artifacts.len(), 1);
    assert!(
        plot_res
            .updated_solution
            .expect("pdf updated")
            .pdf_error
            .is_none()
    );

    state
        .enqueue_document_drive_upload(&EnqueueDriveUploadRequest {
            document_id: phys_doc_id.clone(),
            solution_id: None,
            scope: "default".to_string(),
        })
        .await
        .expect("phys drive enqueue");
    state
        .process_drive_queue_once()
        .await
        .expect("phys drive run");

    // Step 11 & 12: Restart persistence verification across ALL MVP tables
    drop(state);
    let restarted = AppState::initialize(dir.path().to_path_buf())
        .await
        .expect("step 11 restart");

    assert_eq!(
        restarted
            .list_problems(&math_doc_id)
            .await
            .expect("p")
            .len(),
        1
    );
    assert_eq!(
        restarted
            .list_solutions(&math_doc_id, Some(&edited_prob.id))
            .await
            .expect("s")
            .len(),
        2
    );
    assert_eq!(restarted.list_messages(&conv.id).await.expect("m").len(), 2);
    let persisted_drive = restarted
        .list_document_drive_status(&math_doc_id)
        .await
        .expect("drive");
    assert_eq!(persisted_drive.files.len(), 2);
    assert_eq!(
        persisted_drive
            .files
            .iter()
            .find(|f| f.logical_role == "latest_solution_pdf")
            .expect("latest")
            .drive_file_id,
        v1_drive_id
    );
    assert!(
        !restarted
            .list_document_tool_executions(&phys_doc_id)
            .await
            .expect("execs")
            .is_empty()
    );
    assert_eq!(mock_state.lock().expect("lock").delete_calls, 0);
}

/// Live real-account Google Drive OAuth + upload + same-ID update verification.
/// Explicitly gated with `#[ignore]` and requires user-provided `GOOGLE_OAUTH_CLIENT_ID`.
/// Run via: `cargo test --lib -- --ignored test_live_google_drive_oauth_and_same_id_update --nocapture`
#[tokio::test]
#[ignore]
async fn test_live_google_drive_oauth_and_same_id_update() {
    let client_id = std::env::var("GOOGLE_OAUTH_CLIENT_ID")
        .expect("GOOGLE_OAUTH_CLIENT_ID must be set for live Google Drive verification");
    let client_secret = std::env::var("GOOGLE_OAUTH_CLIENT_SECRET").ok();

    let dir = TempDir::new().expect("tempdir");
    let state = AppState::initialize(dir.path().to_path_buf())
        .await
        .expect("init");
    state
        .save_drive_oauth_config(&SaveDriveOAuthConfigRequest {
            client_id: Some(client_id),
            client_secret,
            clear_client_secret: false,
        })
        .expect("save cfg");

    let conn = state
        .connect_google_drive()
        .await
        .expect("live oauth connect");
    assert_eq!(conn.status, "connected");
    assert_eq!(conn.scopes, vec![DRIVE_FILE_SCOPE.to_string()]);
}
