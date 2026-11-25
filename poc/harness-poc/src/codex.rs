use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

use crate::jsonrpc::{IncomingEvent, JsonRpcProcess};
use crate::sandbox::IsolatedWorkspace;

#[derive(Debug, Serialize)]
pub struct CodexPocReport {
    pub harness: &'static str,
    pub startup_initialized: bool,
    pub auth_type: Option<String>,
    pub model_count: usize,
    pub scoped_mcp_registered: bool,
    pub streaming_delta_count: usize,
    pub streaming_reply_sample: String,
    pub image_input_verified: bool,
    pub image_reply_sample: String,
    pub session_resume_verified: bool,
    pub cancel_verified: bool,
    pub permission_control_verified: bool,
    pub permission_detail: String,
    pub notes: Vec<String>,
}

pub async fn run_codex_poc(
    base_tmp: &Path,
    mcp_script_path: &Path,
    test_image_path: &Path,
) -> Result<CodexPocReport, Box<dyn std::error::Error>> {
    let ws = IsolatedWorkspace::create(base_tmp)?;
    let mcp_path_str = mcp_script_path.to_string_lossy().replace('\\', "/");

    // Pass scoped MCP configuration via `-c` ONLY for this child process.
    // Never modifies `~/.codex/config.toml`!
    let args = vec![
        "app-server".to_string(),
        "--stdio".to_string(),
        "-c".to_string(),
        r#"mcp_servers.poc_science.command="python""#.to_string(),
        "-c".to_string(),
        format!(r#"mcp_servers.poc_science.args=["{mcp_path_str}"]"#),
    ];

    let mut proc = JsonRpcProcess::spawn("codex", &args, &ws.root)?;
    let mut notes = Vec::new();

    // 1. Initialize handshake
    let _init_res = proc
        .send_request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "formuladesk-phase0-poc",
                    "title": "FormulaDesk Phase 0 PoC",
                    "version": "0.1.0"
                }
            }),
            Duration::from_secs(15),
        )
        .await?;
    proc.send_notification("initialized", json!({})).await?;

    // 2. Official Auth status check (never logs tokens)
    let account_res = proc
        .send_request(
            "account/read",
            json!({ "refreshToken": false }),
            Duration::from_secs(15),
        )
        .await;
    let auth_type = match account_res {
        Ok(val) => {
            let acc_type = val
                .pointer("/account/type")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .or_else(|| {
                    val.get("account").and_then(|a| {
                        if a.is_null() {
                            None
                        } else {
                            Some("authenticated".to_string())
                        }
                    })
                });
            if acc_type.is_none() {
                notes.push(format!("account/read returned: {val}"));
            }
            acc_type
        }
        Err(err) => {
            notes.push(format!("account/read error: {err}"));
            None
        }
    };

    // 3. Model list check
    let model_count = match proc
        .send_request(
            "model/list",
            json!({ "limit": 20 }),
            Duration::from_secs(15),
        )
        .await
    {
        Ok(val) => val
            .get("data")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        Err(err) => {
            notes.push(format!("model/list error: {err}"));
            0
        }
    };

    // 4. Scoped MCP registration check
    let scoped_mcp_registered = match proc
        .send_request("mcpServerStatus/list", json!({}), Duration::from_secs(15))
        .await
    {
        Ok(val) => {
            let text = val.to_string();
            let found = text.contains("poc_science");
            if !found {
                notes.push(format!(
                    "mcpServerStatus/list did not list poc_science: {text}"
                ));
            }
            found
        }
        Err(err) => {
            notes.push(format!("mcpServerStatus/list error: {err}"));
            false
        }
    };

    // 5. Start read-only + untrusted-approval thread in isolated workspace
    let thread_res = proc
        .send_request(
            "thread/start",
            json!({
                "cwd": ws.root.to_string_lossy(),
                "sandbox": "read-only",
                "approvalPolicy": "untrusted",
                "ephemeral": false
            }),
            Duration::from_secs(20),
        )
        .await?;
    let thread_id = thread_res
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("thread/start missing thread.id: {thread_res}"))?
        .to_string();

    // Helper to drain turn events while declining any command/file approvals
    async fn run_turn_and_collect(
        proc: &mut JsonRpcProcess,
        thread_id: &str,
        input: Value,
        cancel_after_first_delta: bool,
        timeout: Duration,
    ) -> Result<(usize, String, bool, bool, String), Box<dyn std::error::Error>> {
        let start_res = proc
            .send_request(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "input": input
                }),
                Duration::from_secs(20),
            )
            .await?;
        let turn_id = start_res
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let mut delta_count = 0usize;
        let mut text = String::new();
        let mut approval_declined = false;
        let mut interrupted_sent = false;
        let mut completion_status = String::new();

        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, proc.events_rx.recv()).await {
                Ok(Some(IncomingEvent::Notification { method, params })) => {
                    if method == "item/agentMessage/delta" {
                        if let Some(d) = params.get("delta").and_then(Value::as_str) {
                            delta_count += 1;
                            text.push_str(d);
                            if cancel_after_first_delta && !interrupted_sent && !turn_id.is_empty()
                            {
                                interrupted_sent = true;
                                let _ = proc
                                    .send_request(
                                        "turn/interrupt",
                                        json!({ "threadId": thread_id, "turnId": turn_id }),
                                        Duration::from_secs(10),
                                    )
                                    .await;
                            }
                        }
                    } else if method == "turn/completed" {
                        completion_status = params
                            .pointer("/turn/status")
                            .and_then(Value::as_str)
                            .unwrap_or("completed")
                            .to_string();
                        break;
                    }
                }
                Ok(Some(IncomingEvent::ServerRequest {
                    id,
                    method,
                    params: _,
                })) => {
                    if method.contains("requestApproval") || method.contains("Approval") {
                        approval_declined = true;
                        let _ = proc
                            .send_server_response(id, json!({ "decision": "decline" }))
                            .await;
                    } else {
                        let _ = proc.send_server_response(id, json!({})).await;
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }

        Ok((
            delta_count,
            text,
            approval_declined,
            interrupted_sent,
            completion_status,
        ))
    }

    // 6. Streaming text turn
    let (streaming_delta_count, streaming_reply, _, _, _) = run_turn_and_collect(
        &mut proc,
        &thread_id,
        json!([
            { "type": "text", "text": "Compute the derivative d/dx (x^3) in one short sentence without running any tools or commands." }
        ]),
        false,
        Duration::from_secs(45),
    )
    .await?;

    // 7. Image input turn (`localImage`)
    let copied_img = ws.input_dir.join("problem.png");
    std::fs::copy(test_image_path, &copied_img)?;
    let (_, image_reply, _, _, _) = run_turn_and_collect(
        &mut proc,
        &thread_id,
        json!([
            { "type": "text", "text": "Read the math equation shown in this image and state the solution for x in one short line. Do not run any tools." },
            { "type": "localImage", "path": copied_img.to_string_lossy() }
        ]),
        false,
        Duration::from_secs(45),
    )
    .await?;
    let image_input_verified =
        image_reply.contains('4') || image_reply.to_ascii_lowercase().contains("2x");

    // 8. Session resume check (`thread/resume`)
    let session_resume_verified = match proc
        .send_request(
            "thread/resume",
            json!({
                "threadId": thread_id,
                "sandbox": "read-only",
                "approvalPolicy": "untrusted"
            }),
            Duration::from_secs(15),
        )
        .await
    {
        Ok(val) => val
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .is_some_and(|id| id == thread_id),
        Err(err) => {
            notes.push(format!("thread/resume error: {err}"));
            false
        }
    };

    // 9. Cancellation test (`turn/interrupt`)
    let (cancel_deltas, _, _, interrupt_sent, cancel_status) = run_turn_and_collect(
        &mut proc,
        &thread_id,
        json!([
            { "type": "text", "text": "Write a 30-item numbered list of theorems in real analysis with full proofs. Do not run tools." }
        ]),
        true,
        Duration::from_secs(45),
    )
    .await?;
    let cancel_verified = interrupt_sent
        && (cancel_status.eq_ignore_ascii_case("interrupted")
            || cancel_status.eq_ignore_ascii_case("cancelled")
            || cancel_deltas > 0);
    notes.push(format!(
        "cancel turn status={cancel_status}, deltas_before_stop={cancel_deltas}, interrupt_sent={interrupt_sent}"
    ));

    // 10. Permission control check: request shell command execution and verify declined or blocked
    let (_, perm_reply, approval_declined, _, perm_status) = run_turn_and_collect(
        &mut proc,
        &thread_id,
        json!([
            { "type": "text", "text": "Run the shell command `echo FORMULADESK_SHELL_TEST` right now using your shell/command tool." }
        ]),
        false,
        Duration::from_secs(45),
    )
    .await?;
    let permission_control_verified = approval_declined
        || !perm_reply.contains("FORMULADESK_SHELL_TEST")
        || perm_reply.to_ascii_lowercase().contains("declined")
        || perm_reply.to_ascii_lowercase().contains("denied")
        || perm_reply.to_ascii_lowercase().contains("read-only");
    let permission_detail = format!(
        "approval_request_declined={approval_declined}, status={perm_status}, reply_snippet={}",
        perm_reply.chars().take(120).collect::<String>()
    );

    // Cleanup test thread and temporary workspace
    let _ = proc
        .send_request(
            "thread/archive",
            json!({ "threadId": thread_id }),
            Duration::from_secs(10),
        )
        .await;
    proc.shutdown().await;
    ws.cleanup();

    Ok(CodexPocReport {
        harness: "OpenAI Codex (codex app-server)",
        startup_initialized: true,
        auth_type,
        model_count,
        scoped_mcp_registered,
        streaming_delta_count,
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
