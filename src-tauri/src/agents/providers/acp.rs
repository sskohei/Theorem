use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, oneshot};
use uuid::Uuid;

use crate::agents::adapter::{
    AgentAdapter, AgentError, AgentEventStream, AgentInput, AgentSession, PermissionDecision,
    SessionConfig, make_envelope,
};
use crate::agents::jsonrpc::{IncomingEvent, JsonRpcProcess};
use crate::agents::sandbox::{probe_cli_version, resolve_cli_binary};
use crate::domain::{AgentCapabilities, AgentEventPayload, AgentStatus};
use crate::science_mcp::{ScopedMcpServerConfig, extract_mcp_tool_envelope};

struct AcpLiveSession {
    process: Arc<Mutex<Option<JsonRpcProcess>>>,
    external_session_id: String,
    pending_permissions: Arc<Mutex<HashMap<String, oneshot::Sender<PermissionDecision>>>>,
    deny_shell_by_default: bool,
    cancelled_flag: Arc<Mutex<bool>>,
}

/// Shared ACP (Agent Client Protocol) adapter used by both Google Antigravity (`agy acp`)
/// and user-configured Custom ACP agents.
pub struct AcpAdapter {
    provider_kind: String,
    default_binary: String,
    manual_path: Option<String>,
    base_args: Vec<String>,
    sessions: Arc<Mutex<HashMap<String, Arc<AcpLiveSession>>>>,
}

impl AcpAdapter {
    pub fn antigravity(manual_path: Option<String>, extra_args: Vec<String>) -> Self {
        let mut base_args = vec![
            "acp".to_string(),
            "--stdio".to_string(),
            "--sandbox".to_string(),
            "--disable-telemetry".to_string(),
        ];
        base_args.extend(extra_args);
        Self {
            provider_kind: "antigravity".to_string(),
            default_binary: "agy".to_string(),
            manual_path,
            base_args,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn custom(manual_path: Option<String>, args: Vec<String>) -> Self {
        Self {
            provider_kind: "custom_acp".to_string(),
            default_binary: String::new(),
            manual_path,
            base_args: args,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn resolve_binary(&self) -> Option<PathBuf> {
        if self.default_binary.is_empty() {
            let manual = self.manual_path.as_deref()?.trim();
            if manual.is_empty() {
                return None;
            }
            let p = PathBuf::from(manual);
            if p.is_file() { Some(p) } else { None }
        } else {
            resolve_cli_binary(self.manual_path.as_deref(), &self.default_binary)
        }
    }

    fn parse_capabilities(&self, init_res: &Value) -> AgentCapabilities {
        let caps = init_res.get("agentCapabilities").unwrap_or(&Value::Null);
        let load_session = caps
            .get("loadSession")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let image_cap = caps
            .pointer("/promptCapabilities/image")
            .and_then(Value::as_bool)
            .unwrap_or(self.provider_kind == "antigravity");
        let is_antigravity = self.provider_kind == "antigravity";
        let has_sandbox_flag = is_antigravity
            || self
                .base_args
                .iter()
                .any(|a| a.contains("sandbox") || a.contains("read-only"));

        AgentCapabilities {
            supports_image_input: image_cap,
            supports_pdf_input: false,
            supports_streaming: true,
            supports_session_resume: load_session,
            supports_model_listing: false,
            supports_auth_status_check: init_res
                .get("authMethods")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty()),
            supports_mcp_scoped_config: true,
            supports_permission_hooks: true,
            supports_sandbox_restriction: has_sandbox_flag,
            supports_cancel: true,
        }
    }

    async fn initialize_acp(
        binary: &Path,
        args: &[String],
        cwd: &Path,
    ) -> Result<(JsonRpcProcess, Value), AgentError> {
        let proc = JsonRpcProcess::spawn(binary, args, cwd, &[])
            .map_err(|e| AgentError::Transport(e.to_string()))?;

        let init_res = proc
            .send_request(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientInfo": {
                        "name": "theorem",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "clientCapabilities": {
                        "fs": { "readTextFile": false, "writeTextFile": false },
                        "terminal": false
                    }
                }),
                Duration::from_secs(20),
            )
            .await
            .map_err(|e| AgentError::Transport(format!("ACP initialize エラー: {e}")))?;

        Ok((proc, init_res))
    }
}

#[async_trait]
impl AgentAdapter for AcpAdapter {
    async fn check_availability(&self) -> Result<AgentStatus, AgentError> {
        let is_agy = self.provider_kind == "antigravity";
        let auth_guide = if is_agy {
            "ターミナルで `agy acp --login` 等を実行して公式認証を行ってください。".to_string()
        } else {
            "カスタム ACP エージェントの公式認証手順に従ってください。".to_string()
        };
        let compliance_note = if is_agy {
            "公式 `agy acp --stdio --sandbox --disable-telemetry` を対話的クライアントとしてのみ利用します。"
                .to_string()
        } else {
            "initialize 応答の Capability に基づき対応機能を動的判定し、安全制御を満たさない操作を無効化します。"
                .to_string()
        };

        let Some(binary) = self.resolve_binary() else {
            return Ok(AgentStatus {
                provider_kind: self.provider_kind.clone(),
                installed: false,
                resolved_path: None,
                version: None,
                auth_status: "unknown".to_string(),
                auth_method: None,
                verification_status: "unverified".to_string(),
                available_models: None,
                capabilities: AgentCapabilities::default(),
                auth_guide,
                compliance_note,
                detail_note: Some(
                    "実行ファイルが見つかりませんでした。設定画面で手動パスを指定できます。".into(),
                ),
            });
        };

        let version = probe_cli_version(&binary).await;
        let tmp_cwd = std::env::temp_dir();

        let Ok((proc, init_res)) = Self::initialize_acp(&binary, &self.base_args, &tmp_cwd).await
        else {
            return Ok(AgentStatus {
                provider_kind: self.provider_kind.clone(),
                installed: true,
                resolved_path: Some(binary.to_string_lossy().into_owned()),
                version,
                auth_status: "unknown".to_string(),
                auth_method: None,
                verification_status: "unverified".to_string(),
                available_models: None,
                capabilities: AgentCapabilities::default(),
                auth_guide,
                compliance_note,
                detail_note: Some("ACP initialize ハンドシェイクに失敗しました。".into()),
            });
        };

        let caps = self.parse_capabilities(&init_res);
        let auth_methods: Vec<String> = init_res
            .get("authMethods")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(Value::as_str).map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default();

        proc.shutdown().await;

        Ok(AgentStatus {
            provider_kind: self.provider_kind.clone(),
            installed: true,
            resolved_path: Some(binary.to_string_lossy().into_owned()),
            version,
            // ACP `initialize` advertises authMethods but does not always report active login state;
            // report `unknown` honestly rather than fabricating authenticated status.
            auth_status: "unknown".to_string(),
            auth_method: if auth_methods.is_empty() {
                None
            } else {
                Some(auth_methods.join(", "))
            },
            verification_status: if is_agy {
                "verified_live".to_string()
            } else {
                "unverified".to_string()
            },
            available_models: None,
            capabilities: caps,
            auth_guide,
            compliance_note,
            detail_note: Some(
                "ACP は公式モデル一覧・認証状態の事前取得 API を持たないため「未取得」表示となります。"
                    .into(),
            ),
        })
    }

    async fn capabilities(&self) -> Result<AgentCapabilities, AgentError> {
        let status = self.check_availability().await?;
        Ok(status.capabilities)
    }

    async fn create_session(&self, config: SessionConfig) -> Result<AgentSession, AgentError> {
        let binary = self.resolve_binary().ok_or_else(|| {
            AgentError::NotInstalled(if self.default_binary.is_empty() {
                "custom_acp".to_string()
            } else {
                self.default_binary.clone()
            })
        })?;

        let (proc, init_res) =
            Self::initialize_acp(&binary, &self.base_args, &config.workspace_root).await?;
        let caps = self.parse_capabilities(&init_res);

        // Enforce security policy: custom ACP agents without sandbox/permission guarantees are blocked
        if !caps.supports_sandbox_restriction && !caps.supports_permission_hooks {
            proc.shutdown().await;
            return Err(AgentError::PolicyBlocked(
                "このカスタム ACP エージェントは必須のサンドボックス/権限フックを提供しないため無効化されました"
                    .into(),
            ));
        }

        let mcp_servers_json = build_acp_mcp_servers_json(&config.mcp_servers);

        let mut resumed = false;
        let ext_session_id = if let Some(ref ext_id) = config.resume_external_session_id
            && caps.supports_session_resume
        {
            match proc
                .send_request(
                    "session/load",
                    json!({
                        "sessionId": ext_id,
                        "cwd": config.workspace_root.to_string_lossy(),
                        "mcpServers": mcp_servers_json.clone()
                    }),
                    Duration::from_secs(15),
                )
                .await
            {
                Ok(_) => {
                    resumed = true;
                    ext_id.clone()
                }
                Err(_) => {
                    let created = proc
                        .send_request(
                            "session/new",
                            json!({
                                "cwd": config.workspace_root.to_string_lossy(),
                                "mcpServers": mcp_servers_json.clone()
                            }),
                            Duration::from_secs(20),
                        )
                        .await
                        .map_err(|e| AgentError::Transport(format!("ACP session/new 失敗: {e}")))?;
                    created
                        .get("sessionId")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            AgentError::Transport(
                                "ACP session/new に sessionId がありません".into(),
                            )
                        })?
                        .to_string()
                }
            }
        } else {
            let created = proc
                .send_request(
                    "session/new",
                    json!({
                        "cwd": config.workspace_root.to_string_lossy(),
                        "mcpServers": mcp_servers_json
                    }),
                    Duration::from_secs(20),
                )
                .await
                .map_err(|e| AgentError::Transport(format!("ACP session/new 失敗: {e}")))?;
            created
                .get("sessionId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AgentError::Transport("ACP session/new に sessionId がありません".into())
                })?
                .to_string()
        };

        let live = Arc::new(AcpLiveSession {
            process: Arc::new(Mutex::new(Some(proc))),
            external_session_id: ext_session_id.clone(),
            pending_permissions: Arc::new(Mutex::new(HashMap::new())),
            deny_shell_by_default: config.deny_shell_by_default,
            cancelled_flag: Arc::new(Mutex::new(false)),
        });
        self.sessions
            .lock()
            .await
            .insert(config.internal_session_id.clone(), live);

        Ok(AgentSession {
            session_id: config.internal_session_id,
            external_session_id: Some(ext_session_id),
            resumed,
            resumable: caps.supports_session_resume,
        })
    }

    async fn prompt(
        &self,
        session_id: &str,
        input: AgentInput,
    ) -> Result<AgentEventStream, AgentError> {
        let live = self
            .sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let (client, events_rx) = {
            let guard = live.process.lock().await;
            let proc = guard
                .as_ref()
                .ok_or_else(|| AgentError::Transport("ACP プロセスが終了しています".into()))?;
            (proc.client.clone(), Arc::clone(&proc.events_rx))
        };

        // Drain stale post-cancel residual notifications BEFORE starting a new turn
        // (Phase 0 mitigation for Antigravity ACP residual buffer chunks).
        {
            let mut rx_guard = events_rx.lock().await;
            while rx_guard.try_recv().is_ok() {}
            *live.cancelled_flag.lock().await = false;
        }

        let mut blocks = vec![json!({ "type": "text", "text": input.prompt_text })];
        for img in &input.images {
            blocks.push(json!({
                "type": "image",
                "mimeType": img.mime_type,
                "data": STANDARD.encode(&img.bytes)
            }));
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let sess_id_owned = session_id.to_string();
        let ext_sid = live.external_session_id.clone();
        let run_id = input.run_id.clone();
        let message_id = input.message_id.clone();

        tokio::spawn(async move {
            let _ = tx.send(make_envelope(
                &run_id,
                &sess_id_owned,
                message_id.as_deref(),
                AgentEventPayload::MessageStarted {
                    role: "assistant".to_string(),
                },
            ));

            let req_fut = client.send_request(
                "session/prompt",
                json!({
                    "sessionId": ext_sid,
                    "prompt": blocks
                }),
                Duration::from_secs(240),
            );
            tokio::pin!(req_fut);

            let mut rx_guard = events_rx.lock().await;
            let mut full_text = String::new();

            loop {
                tokio::select! {
                    res = &mut req_fut => {
                        // Drain remaining buffered notifications for this completed turn
                        while let Ok(ev) = rx_guard.try_recv() {
                            if let IncomingEvent::Notification { method, params } = ev
                                && method == "session/update"
                                && let Some(chunk) = extract_acp_update_text(&params)
                            {
                                full_text.push_str(&chunk);
                                let _ = tx.send(make_envelope(
                                    &run_id,
                                    &sess_id_owned,
                                    message_id.as_deref(),
                                    AgentEventPayload::TextDelta { text: chunk },
                                ));
                            }
                        }

                        let was_cancelled = *live.cancelled_flag.lock().await;
                        match res {
                            Ok(val) => {
                                let stop_reason = val
                                    .get("stopReason")
                                    .and_then(Value::as_str)
                                    .unwrap_or("end_turn");
                                if was_cancelled || stop_reason == "cancelled" {
                                    let _ = tx.send(make_envelope(
                                        &run_id,
                                        &sess_id_owned,
                                        message_id.as_deref(),
                                        AgentEventPayload::Cancelled {
                                            reason: Some("ユーザーにより中断されました".to_string()),
                                        },
                                    ));
                                } else {
                                    let _ = tx.send(make_envelope(
                                        &run_id,
                                        &sess_id_owned,
                                        message_id.as_deref(),
                                        AgentEventPayload::MessageCompleted {
                                            final_text: Some(full_text),
                                        },
                                    ));
                                }
                            }
                            Err(err) => {
                                let msg = err.to_string();
                                let lower = msg.to_ascii_lowercase();
                                let _ = tx.send(make_envelope(
                                    &run_id,
                                    &sess_id_owned,
                                    message_id.as_deref(),
                                    AgentEventPayload::Failed {
                                        code: "ACP_PROMPT_ERROR".to_string(),
                                        message: msg,
                                        rate_limited: lower.contains("429") || lower.contains("quota") || lower.contains("rate"),
                                        auth_required: lower.contains("401") || lower.contains("auth") || lower.contains("login"),
                                    },
                                ));
                            }
                        }
                        break;
                    }
                    maybe_ev = rx_guard.recv() => {
                        match maybe_ev {
                            Some(IncomingEvent::Notification { method, params }) => {
                                if method == "session/update" {
                                    if let Some(chunk) = extract_acp_update_text(&params) {
                                        full_text.push_str(&chunk);
                                        let _ = tx.send(make_envelope(
                                            &run_id,
                                            &sess_id_owned,
                                            message_id.as_deref(),
                                            AgentEventPayload::TextDelta { text: chunk },
                                        ));
                                    } else {
                                        for payload in extract_acp_tool_events(&params) {
                                            let _ = tx.send(make_envelope(
                                                &run_id,
                                                &sess_id_owned,
                                                message_id.as_deref(),
                                                payload,
                                            ));
                                        }
                                    }
                                }
                            }
                            Some(IncomingEvent::ServerRequest { id, method, params }) => {
                                if method == "session/request_permission" {
                                    let req_id = Uuid::new_v4().to_string();
                                    let title = params
                                        .pointer("/toolCall/title")
                                        .and_then(Value::as_str)
                                        .unwrap_or("外部操作の実行要求")
                                        .to_string();
                                    let _ = tx.send(make_envelope(
                                        &run_id,
                                        &sess_id_owned,
                                        message_id.as_deref(),
                                        AgentEventPayload::PermissionRequested {
                                            request_id: req_id.clone(),
                                            action: "session/request_permission".to_string(),
                                            target: None,
                                            detail: title,
                                        },
                                    ));

                                    let decision = if live.deny_shell_by_default {
                                        PermissionDecision::Deny
                                    } else {
                                        let (ptx, prx) = oneshot::channel();
                                        live.pending_permissions.lock().await.insert(req_id.clone(), ptx);
                                        match tokio::time::timeout(Duration::from_secs(60), prx).await {
                                            Ok(Ok(dec)) => dec,
                                            _ => {
                                                live.pending_permissions.lock().await.remove(&req_id);
                                                PermissionDecision::Deny
                                            }
                                        }
                                    };

                                    let outcome = build_acp_permission_outcome(&params, decision);
                                    let _ = client.send_server_response(id, outcome).await;
                                } else {
                                    let _ = client.send_server_response(id, json!({})).await;
                                }
                            }
                            None => {
                                let _ = tx.send(make_envelope(
                                    &run_id,
                                    &sess_id_owned,
                                    message_id.as_deref(),
                                    AgentEventPayload::Failed {
                                        code: "PROCESS_CRASHED".to_string(),
                                        message: "ACP プロセスが予期せず終了しました".to_string(),
                                        rate_limited: false,
                                        auth_required: false,
                                    },
                                ));
                                break;
                            }
                        }
                    }
                }
            }
        });

        let stream =
            futures_util::stream::unfold(
                rx,
                |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
            );
        Ok(stream.boxed())
    }

    async fn respond_permission(
        &self,
        session_id: &str,
        request_id: &str,
        decision: PermissionDecision,
    ) -> Result<(), AgentError> {
        let live = self
            .sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;
        if let Some(tx) = live.pending_permissions.lock().await.remove(request_id) {
            let _ = tx.send(decision);
        }
        Ok(())
    }

    async fn cancel(&self, session_id: &str) -> Result<(), AgentError> {
        let live = self
            .sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        *live.cancelled_flag.lock().await = true;
        for (_, ptx) in live.pending_permissions.lock().await.drain() {
            let _ = ptx.send(PermissionDecision::Deny);
        }

        let client_opt = live.process.lock().await.as_ref().map(|p| p.client.clone());
        if let Some(client) = client_opt {
            let _ = client
                .send_notification(
                    "session/cancel",
                    json!({ "sessionId": live.external_session_id }),
                )
                .await;
        }
        Ok(())
    }

    async fn close_session(&self, session_id: &str) -> Result<(), AgentError> {
        if let Some(live) = self.sessions.lock().await.remove(session_id)
            && let Some(proc) = live.process.lock().await.take()
        {
            proc.shutdown().await;
        }
        Ok(())
    }
}

fn build_acp_mcp_servers_json(servers: &[ScopedMcpServerConfig]) -> Value {
    let arr: Vec<Value> = servers
        .iter()
        .map(|srv| {
            let env_arr: Vec<Value> = srv
                .env
                .iter()
                .map(|(k, v)| json!({ "name": k, "value": v }))
                .collect();
            json!({
                "name": srv.name,
                "command": srv.command,
                "args": srv.args,
                "env": env_arr
            })
        })
        .collect();
    Value::Array(arr)
}

/// Extracts ONLY public `agent_message_chunk` text (never exposes hidden `agent_thought_chunk`).
fn extract_acp_update_text(params: &Value) -> Option<String> {
    let update = params.get("update")?;
    let kind = update.get("sessionUpdate").and_then(Value::as_str)?;
    if kind != "agent_message_chunk" {
        return None;
    }
    update
        .pointer("/content/text")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

/// Normalizes ACP `tool_call` / `tool_call_update` notifications into unified `AgentEventPayload`.
fn extract_acp_tool_events(params: &Value) -> Vec<AgentEventPayload> {
    let Some(update) = params.get("update") else {
        return Vec::new();
    };
    let Some(kind) = update.get("sessionUpdate").and_then(Value::as_str) else {
        return Vec::new();
    };

    let call_id = update
        .get("toolCallId")
        .or_else(|| update.get("id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let mut out = Vec::new();
    if kind == "tool_call" {
        let tool_name = update
            .get("title")
            .or_else(|| update.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("mcp_tool")
            .to_string();
        let arguments = update
            .get("rawInput")
            .or_else(|| update.get("arguments"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        out.push(AgentEventPayload::ToolCallStarted {
            call_id,
            server: update
                .get("server")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            tool_name,
            arguments,
        });
    } else if kind == "tool_call_update" {
        let status = update
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("in_progress");
        if status == "completed" || status == "failed" {
            let raw_out = update
                .get("rawOutput")
                .or_else(|| update.get("content"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            let normalized = extract_mcp_tool_envelope(&raw_out);
            let succeeded = status == "completed"
                && normalized
                    .get("ok")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
            out.push(AgentEventPayload::ToolCallCompleted {
                call_id,
                succeeded,
                output: normalized.clone(),
            });
            if let Some(arts) = normalized.get("artifacts").and_then(Value::as_array) {
                for art in arts {
                    if let Some(rel) = art.get("relative_path").and_then(Value::as_str) {
                        out.push(AgentEventPayload::ArtifactCreated {
                            relative_path: rel.to_string(),
                            mime_type: art
                                .get("mime_type")
                                .and_then(Value::as_str)
                                .unwrap_or("image/svg+xml")
                                .to_string(),
                            caption: art
                                .get("caption")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                        });
                    }
                }
            }
        } else {
            out.push(AgentEventPayload::ToolCallUpdated {
                call_id,
                status: status.to_string(),
                progress: update
                    .get("title")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            });
        }
    }
    out
}

fn build_acp_permission_outcome(params: &Value, decision: PermissionDecision) -> Value {
    let options = params.get("options").and_then(Value::as_array);
    let wanted = match decision {
        PermissionDecision::AllowOnce => "allow",
        PermissionDecision::Deny => "reject",
    };
    let chosen = options.and_then(|arr| {
        arr.iter().find_map(|opt| {
            let kind = opt.get("kind").and_then(Value::as_str)?;
            if kind.contains(wanted) {
                opt.get("optionId")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            } else {
                None
            }
        })
    });
    match chosen {
        Some(oid) => json!({ "outcome": { "outcome": "selected", "optionId": oid } }),
        None => json!({ "outcome": { "outcome": "cancelled" } }),
    }
}
