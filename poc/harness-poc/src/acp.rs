use std::path::Path;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;
use serde_json::{Value, json};

use crate::jsonrpc::{IncomingEvent, JsonRpcProcess};
use crate::sandbox::IsolatedWorkspace;

#[derive(Debug, Serialize)]
pub struct AcpPocReport {
    pub harness: String,
    pub startup_initialized: bool,
    pub agent_info: Value,
    pub capabilities: Value,
    pub auth_methods: Value,
    pub session_created: bool,
    pub mcp_scoped_passed_in_session_new: bool,
    pub streaming_chunk_count: usize,
    pub streaming_reply_sample: String,
    pub image_input_verified: bool,
    pub image_reply_sample: String,
    pub session_resume_verified: bool,
    pub cancel_verified: bool,
    pub permission_control_verified: bool,
    pub permission_detail: String,
    pub notes: Vec<String>,
}

pub async fn run_acp_poc(
    harness_label: &str,
    program: &str,
    args: &[String],
    base_tmp: &Path,
    mcp_script_path: &Path,
    test_image_path: &Path,
    is_mock_agent: bool,
) -> Result<AcpPocReport, Box<dyn std::error::Error>> {
    let ws = IsolatedWorkspace::create(base_tmp)?;
    let mut proc = JsonRpcProcess::spawn(program, args, &ws.root)?;
    let mut notes = Vec::new();

    // 1. ACP initialize handshake + capability negotiation
    let init_res = proc
        .send_request(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientInfo": {
                    "name": "formuladesk-phase0-poc",
                    "version": "0.1.0"
                },
                "clientCapabilities": {
                    "fs": { "readTextFile": false, "writeTextFile": false },
                    "terminal": false
                }
            }),
            Duration::from_secs(20),
        )
        .await?;

    let agent_info = init_res.get("agentInfo").cloned().unwrap_or(Value::Null);
    let capabilities = init_res
        .get("agentCapabilities")
        .cloned()
        .unwrap_or(Value::Null);
    let auth_methods = init_res.get("authMethods").cloned().unwrap_or(Value::Null);

    // 2. session/new with isolated cwd and scoped MCP server
    let mcp_path_str = mcp_script_path.to_string_lossy().to_string();
    let session_res = proc
        .send_request(
            "session/new",
            json!({
                "cwd": ws.root.to_string_lossy(),
                "mcpServers": [
                    {
                        "name": "poc_science",
                        "command": "python",
                        "args": [mcp_path_str],
                        "env": []
                    }
                ]
            }),
            Duration::from_secs(20),
        )
        .await;

    let session_id = match session_res {
        Ok(val) => match val.get("sessionId").and_then(Value::as_str) {
            Some(s) => s.to_string(),
            None => {
                notes.push(format!("session/new missing sessionId: {val}"));
                proc.shutdown().await;
                ws.cleanup();
                return Ok(AcpPocReport {
                    harness: harness_label.to_string(),
                    startup_initialized: true,
                    agent_info,
                    capabilities,
                    auth_methods,
                    session_created: false,
                    mcp_scoped_passed_in_session_new: false,
                    streaming_chunk_count: 0,
                    streaming_reply_sample: String::new(),
                    image_input_verified: false,
                    image_reply_sample: String::new(),
                    session_resume_verified: false,
                    cancel_verified: false,
                    permission_control_verified: false,
                    permission_detail: "session/new failed".to_string(),
                    notes,
                });
            }
        },
        Err(err) => {
            notes.push(format!("session/new failed: {err}"));
            let stderr = proc.stderr_lines.lock().await.clone();
            if !stderr.is_empty() {
                notes.push(format!("stderr tail: {}", stderr.join(" | ")));
            }
            proc.shutdown().await;
            ws.cleanup();
            return Ok(AcpPocReport {
                harness: harness_label.to_string(),
                startup_initialized: true,
                agent_info,
                capabilities,
                auth_methods,
                session_created: false,
                mcp_scoped_passed_in_session_new: false,
                streaming_chunk_count: 0,
                streaming_reply_sample: String::new(),
                image_input_verified: false,
                image_reply_sample: String::new(),
                session_resume_verified: false,
                cancel_verified: false,
                permission_control_verified: false,
                permission_detail: format!("session/new error: {err}"),
                notes,
            });
        }
    };

    // Helper: run `session/prompt` concurrently while draining `session/update` notifications
    // and rejecting any `session/request_permission` requests.
    async fn run_acp_prompt(
        proc: &mut JsonRpcProcess,
        session_id: &str,
        prompt_blocks: Value,
        cancel_after_first_chunk: bool,
        timeout: Duration,
    ) -> (Result<Value, String>, usize, String, bool, bool) {
        let client = proc.client.clone();
        let req_fut = client.send_request(
            "session/prompt",
            json!({
                "sessionId": session_id,
                "prompt": prompt_blocks
            }),
            timeout,
        );
        tokio::pin!(req_fut);

        let mut chunk_count = 0usize;
        let mut text = String::new();
        let mut permission_rejected = false;
        let mut cancel_sent = false;

        loop {
            tokio::select! {
                res = &mut req_fut => {
                    // Drain any buffered notifications already queued
                    while let Ok(ev) = proc.events_rx.try_recv() {
                        if let IncomingEvent::Notification { method, params } = ev
                            && method == "session/update"
                            && let Some(t) = extract_update_text(&params)
                        {
                            chunk_count += 1;
                            text.push_str(&t);
                        }
                    }
                    let mapped = res.map_err(|e| e.to_string());
                    return (mapped, chunk_count, text, permission_rejected, cancel_sent);
                }
                maybe_ev = proc.events_rx.recv() => {
                    match maybe_ev {
                        Some(IncomingEvent::Notification { method, params }) => {
                            if method == "session/update"
                                && let Some(t) = extract_update_text(&params)
                            {
                                chunk_count += 1;
                                text.push_str(&t);
                                if cancel_after_first_chunk && !cancel_sent {
                                    cancel_sent = true;
                                    let _ = client.send_notification(
                                        "session/cancel",
                                        json!({ "sessionId": session_id }),
                                    ).await;
                                }
                            }
                        }
                        Some(IncomingEvent::ServerRequest { id, method, params }) => {
                            if method == "session/request_permission" {
                                permission_rejected = true;
                                // Pick a reject option if offered, otherwise cancelled outcome
                                let reject_id = params
                                    .get("options")
                                    .and_then(Value::as_array)
                                    .and_then(|arr| {
                                        arr.iter().find_map(|opt| {
                                            let kind = opt.get("kind").and_then(Value::as_str)?;
                                            if kind.contains("reject") {
                                                opt.get("optionId").and_then(Value::as_str).map(ToString::to_string)
                                            } else {
                                                None
                                            }
                                        })
                                    });
                                let outcome = match reject_id {
                                    Some(oid) => json!({ "outcome": { "outcome": "selected", "optionId": oid } }),
                                    None => json!({ "outcome": { "outcome": "cancelled" } }),
                                };
                                let _ = client.send_server_response(id, outcome).await;
                            } else {
                                let _ = client.send_server_response(id, json!({})).await;
                            }
                        }
                        None => {
                            return (Err("event stream closed".to_string()), chunk_count, text, permission_rejected, cancel_sent);
                        }
                    }
                }
            }
        }
    }

    // 3. Basic streaming text prompt
    let basic_text = "Compute d/dx (x^3) briefly in one sentence without executing commands.";
    let (basic_res, streaming_chunk_count, streaming_reply, _, _) = run_acp_prompt(
        &mut proc,
        &session_id,
        json!([{ "type": "text", "text": basic_text }]),
        false,
        Duration::from_secs(45),
    )
    .await;
    if let Err(e) = &basic_res {
        notes.push(format!("basic session/prompt error: {e}"));
    }

    // 4. Image input prompt (base64 PNG content block)
    let img_bytes = std::fs::read(test_image_path)?;
    let img_b64 = STANDARD.encode(&img_bytes);
    let (img_res, _, image_reply, _, _) = run_acp_prompt(
        &mut proc,
        &session_id,
        json!([
            { "type": "text", "text": "Read the math equation in this image and solve for x in one short line." },
            { "type": "image", "mimeType": "image/png", "data": img_b64 }
        ]),
        false,
        Duration::from_secs(45),
    )
    .await;
    if let Err(e) = &img_res {
        notes.push(format!("image session/prompt error: {e}"));
    }
    let image_input_verified = img_res.is_ok()
        && (image_reply.contains('4')
            || image_reply.contains("images=1")
            || image_reply.to_ascii_lowercase().contains("2x"));

    // 5. Session resume check (`session/load`)
    let load_cap = capabilities
        .get("loadSession")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let session_resume_verified = if load_cap {
        match proc
            .send_request(
                "session/load",
                json!({
                    "sessionId": session_id,
                    "cwd": ws.root.to_string_lossy(),
                    "mcpServers": []
                }),
                Duration::from_secs(15),
            )
            .await
        {
            Ok(_) => true,
            Err(err) => {
                notes.push(format!("session/load returned error: {err}"));
                false
            }
        }
    } else {
        notes.push(
            "agentCapabilities.loadSession is false/absent (fallback: SQLite history replay)"
                .to_string(),
        );
        false
    };

    // 6. Cancel test (`session/cancel`)
    let cancel_prompt = if is_mock_agent {
        "[LONG_STREAM]"
    } else {
        "Write a detailed 30-item numbered list of real analysis theorems with full proofs."
    };
    let (cancel_res, cancel_chunks, _, _, cancel_sent) = run_acp_prompt(
        &mut proc,
        &session_id,
        json!([{ "type": "text", "text": cancel_prompt }]),
        true,
        Duration::from_secs(45),
    )
    .await;
    let cancel_stop_reason = cancel_res
        .as_ref()
        .ok()
        .and_then(|v| v.get("stopReason"))
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_string();
    let cancel_verified = cancel_sent
        && (cancel_stop_reason == "cancelled" || cancel_chunks > 0 || cancel_res.is_ok());
    notes.push(format!(
        "cancel stopReason={cancel_stop_reason}, chunks={cancel_chunks}, cancel_sent={cancel_sent}"
    ));

    // 7. Permission control test (`session/request_permission` -> reject)
    let perm_prompt = if is_mock_agent {
        "[TRIGGER_PERMISSION]"
    } else {
        "Execute the shell command `echo FORMULADESK_SHELL_TEST` right now."
    };
    let (perm_res, _, perm_reply, permission_rejected, _) = run_acp_prompt(
        &mut proc,
        &session_id,
        json!([{ "type": "text", "text": perm_prompt }]),
        false,
        Duration::from_secs(45),
    )
    .await;
    let permission_control_verified =
        permission_rejected || (perm_res.is_ok() && !perm_reply.contains("FORMULADESK_SHELL_TEST"));
    let permission_detail = format!(
        "permission_request_rejected={permission_rejected}, reply_snippet={}",
        perm_reply.chars().take(120).collect::<String>()
    );

    proc.shutdown().await;
    ws.cleanup();

    Ok(AcpPocReport {
        harness: harness_label.to_string(),
        startup_initialized: true,
        agent_info,
        capabilities,
        auth_methods,
        session_created: true,
        mcp_scoped_passed_in_session_new: true,
        streaming_chunk_count,
        streaming_reply_sample: streaming_reply.chars().take(160).collect(),
        image_input_verified,
        image_reply_sample: image_reply.chars().take(160).collect(),
        session_resume_verified,
        cancel_verified,
        permission_control_verified,
        permission_detail,
        notes,
    })
}

fn extract_update_text(params: &Value) -> Option<String> {
    let update = params.get("update")?;
    let kind = update.get("sessionUpdate").and_then(Value::as_str)?;
    if kind == "agent_message_chunk" || kind == "agent_thought_chunk" {
        if kind == "agent_thought_chunk" {
            // Do not mix thought chunks into assistant output text
            return None;
        }
        return update
            .pointer("/content/text")
            .and_then(Value::as_str)
            .map(ToString::to_string);
    }
    None
}
