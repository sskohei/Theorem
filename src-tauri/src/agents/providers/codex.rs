use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
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
use crate::domain::{AgentCapabilities, AgentEventPayload, AgentModelInfo, AgentStatus};

struct CodexLiveSession {
    process: Arc<Mutex<Option<JsonRpcProcess>>>,
    thread_id: String,
    workspace_root: PathBuf,
    active_turn_id: Arc<Mutex<Option<String>>>,
    pending_permissions: Arc<Mutex<HashMap<String, oneshot::Sender<PermissionDecision>>>>,
    deny_shell_by_default: bool,
}

pub struct CodexAdapter {
    manual_path: Option<String>,
    extra_args: Vec<String>,
    sessions: Arc<Mutex<HashMap<String, Arc<CodexLiveSession>>>>,
}

impl CodexAdapter {
    pub fn new(manual_path: Option<String>, extra_args: Vec<String>) -> Self {
        Self {
            manual_path,
            extra_args,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn default_capabilities() -> AgentCapabilities {
        AgentCapabilities {
            supports_image_input: true,
            supports_pdf_input: false,
            supports_streaming: true,
            supports_session_resume: true,
            supports_model_listing: true,
            supports_auth_status_check: true,
            supports_mcp_scoped_config: true,
            supports_permission_hooks: true,
            supports_sandbox_restriction: true,
            supports_cancel: true,
        }
    }

    fn spawn_args(&self) -> Vec<String> {
        let mut args = vec!["app-server".to_string(), "--stdio".to_string()];
        args.extend(self.extra_args.iter().cloned());
        args
    }

    async fn initialize_process(
        binary: &Path,
        args: &[String],
        cwd: &Path,
        explicit_api_key: Option<&str>,
    ) -> Result<JsonRpcProcess, AgentError> {
        let explicit_env: Vec<(&str, &str)> = match explicit_api_key {
            Some(k) if !k.trim().is_empty() => vec![("OPENAI_API_KEY", k.trim())],
            _ => Vec::new(),
        };
        let proc = JsonRpcProcess::spawn(binary, args, cwd, &explicit_env)
            .map_err(|e| AgentError::Transport(e.to_string()))?;

        proc.send_request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "theorem",
                    "title": "Theorem",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
            Duration::from_secs(15),
        )
        .await
        .map_err(|e| AgentError::Transport(format!("Codex initialize エラー: {e}")))?;

        proc.send_notification("initialized", json!({}))
            .await
            .map_err(|e| AgentError::Transport(format!("Codex initialized 通知エラー: {e}")))?;

        Ok(proc)
    }
}

#[async_trait]
impl AgentAdapter for CodexAdapter {
    async fn check_availability(&self) -> Result<AgentStatus, AgentError> {
        let caps = Self::default_capabilities();
        let auth_guide =
            "ターミナルで `codex login` を実行して公式認証してください（`codex login status` で確認可能）。"
                .to_string();
        let compliance_note =
            "公式 `codex app-server` を無改変で利用し、サブスクリプション利用時は OPENAI_API_KEY を子プロセスへ引き継ぎません。"
                .to_string();

        let Some(binary) = resolve_cli_binary(self.manual_path.as_deref(), "codex") else {
            return Ok(AgentStatus {
                provider_kind: "codex".to_string(),
                installed: false,
                resolved_path: None,
                version: None,
                auth_status: "unknown".to_string(),
                auth_method: None,
                verification_status: "unverified".to_string(),
                available_models: None,
                capabilities: caps,
                auth_guide,
                compliance_note,
                detail_note: Some(
                    "codex CLI が見つかりませんでした。手動パス指定も可能です。".into(),
                ),
            });
        };

        let version = probe_cli_version(&binary).await;
        let tmp_cwd = std::env::temp_dir();
        let args = self.spawn_args();

        let Ok(proc) = Self::initialize_process(&binary, &args, &tmp_cwd, None).await else {
            return Ok(AgentStatus {
                provider_kind: "codex".to_string(),
                installed: true,
                resolved_path: Some(binary.to_string_lossy().into_owned()),
                version,
                auth_status: "unknown".to_string(),
                auth_method: None,
                verification_status: "unverified".to_string(),
                available_models: None,
                capabilities: caps,
                auth_guide,
                compliance_note,
                detail_note: Some(
                    "codex app-server の初期化プローブに失敗しました。バージョンを確認してください。"
                        .into(),
                ),
            });
        };

        let (auth_status, auth_method) = match proc
            .send_request(
                "account/read",
                json!({ "refreshToken": false }),
                Duration::from_secs(10),
            )
            .await
        {
            Ok(val) => {
                if let Some(acc_type) = val.pointer("/account/type").and_then(Value::as_str) {
                    ("authenticated".to_string(), Some(acc_type.to_string()))
                } else if val.get("account").is_some_and(|a| !a.is_null()) {
                    ("authenticated".to_string(), Some("official".to_string()))
                } else {
                    ("unauthenticated".to_string(), None)
                }
            }
            Err(_) => ("unknown".to_string(), None),
        };

        let available_models = match proc
            .send_request(
                "model/list",
                json!({ "limit": 30 }),
                Duration::from_secs(10),
            )
            .await
        {
            Ok(val) => val.get("data").and_then(Value::as_array).map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let id = m
                            .get("id")
                            .or_else(|| m.get("model"))
                            .and_then(Value::as_str)?;
                        let label = m.get("displayName").and_then(Value::as_str).unwrap_or(id);
                        Some(AgentModelInfo {
                            id: id.to_string(),
                            label: label.to_string(),
                        })
                    })
                    .collect::<Vec<_>>()
            }),
            Err(_) => None,
        };

        proc.shutdown().await;

        Ok(AgentStatus {
            provider_kind: "codex".to_string(),
            installed: true,
            resolved_path: Some(binary.to_string_lossy().into_owned()),
            version,
            auth_status,
            auth_method,
            verification_status: "verified_live".to_string(),
            available_models,
            capabilities: caps,
            auth_guide,
            compliance_note,
            detail_note: None,
        })
    }

    async fn capabilities(&self) -> Result<AgentCapabilities, AgentError> {
        Ok(Self::default_capabilities())
    }

    async fn create_session(&self, config: SessionConfig) -> Result<AgentSession, AgentError> {
        let binary = resolve_cli_binary(self.manual_path.as_deref(), "codex")
            .ok_or_else(|| AgentError::NotInstalled("codex".into()))?;
        let args = self.spawn_args();
        let proc = Self::initialize_process(
            &binary,
            &args,
            &config.workspace_root,
            config.explicit_api_key.as_deref(),
        )
        .await?;

        let mut resumed = false;
        let thread_id = if let Some(ref ext_id) = config.resume_external_session_id {
            match proc
                .send_request(
                    "thread/resume",
                    json!({
                        "threadId": ext_id,
                        "sandbox": "read-only",
                        "approvalPolicy": "untrusted"
                    }),
                    Duration::from_secs(15),
                )
                .await
            {
                Ok(val) => {
                    resumed = true;
                    val.pointer("/thread/id")
                        .and_then(Value::as_str)
                        .unwrap_or(ext_id)
                        .to_string()
                }
                Err(_) => {
                    let started = proc
                        .send_request(
                            "thread/start",
                            json!({
                                "cwd": config.workspace_root.to_string_lossy(),
                                "sandbox": "read-only",
                                "approvalPolicy": "untrusted",
                                "ephemeral": false
                            }),
                            Duration::from_secs(20),
                        )
                        .await
                        .map_err(|e| map_rpc_error("thread/start", &e.to_string()))?;
                    started
                        .pointer("/thread/id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            AgentError::Transport(
                                "Codex thread/start が thread.id を返しませんでした".into(),
                            )
                        })?
                        .to_string()
                }
            }
        } else {
            let started = proc
                .send_request(
                    "thread/start",
                    json!({
                        "cwd": config.workspace_root.to_string_lossy(),
                        "sandbox": "read-only",
                        "approvalPolicy": "untrusted",
                        "ephemeral": false
                    }),
                    Duration::from_secs(20),
                )
                .await
                .map_err(|e| map_rpc_error("thread/start", &e.to_string()))?;
            started
                .pointer("/thread/id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AgentError::Transport(
                        "Codex thread/start が thread.id を返しませんでした".into(),
                    )
                })?
                .to_string()
        };

        let live = Arc::new(CodexLiveSession {
            process: Arc::new(Mutex::new(Some(proc))),
            thread_id: thread_id.clone(),
            workspace_root: config.workspace_root.clone(),
            active_turn_id: Arc::new(Mutex::new(None)),
            pending_permissions: Arc::new(Mutex::new(HashMap::new())),
            deny_shell_by_default: config.deny_shell_by_default,
        });
        self.sessions
            .lock()
            .await
            .insert(config.internal_session_id.clone(), live);

        Ok(AgentSession {
            session_id: config.internal_session_id,
            external_session_id: Some(thread_id),
            resumed,
            resumable: true,
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

        // Build turn/start input items (copying images safely inside session workspace/input/)
        let mut items = vec![json!({ "type": "text", "text": input.prompt_text })];
        let input_dir = live.workspace_root.join("input");
        let _ = std::fs::create_dir_all(&input_dir);
        for (idx, img) in input.images.iter().enumerate() {
            let ext = match img.mime_type.as_str() {
                "image/jpeg" => "jpg",
                "image/webp" => "webp",
                _ => "png",
            };
            let target = input_dir.join(format!("turn-{}-{idx}.{ext}", Uuid::new_v4()));
            std::fs::write(&target, &img.bytes)
                .map_err(|e| AgentError::Transport(format!("入力画像の書き出し失敗: {e}")))?;
            items.push(json!({
                "type": "localImage",
                "path": target.to_string_lossy()
            }));
        }

        let mut start_params = json!({
            "threadId": live.thread_id,
            "input": items
        });
        if let Some(ref model) = input.model_override
            && !model.trim().is_empty()
        {
            start_params["model"] = Value::String(model.trim().to_string());
        }
        if let Some(ref schema) = input.output_schema {
            start_params["outputSchema"] = to_openai_strict_schema(schema);
        }

        let (client, events_rx) = {
            let guard = live.process.lock().await;
            let proc = guard
                .as_ref()
                .ok_or_else(|| AgentError::Transport("Codex プロセスが終了しています".into()))?;
            (proc.client.clone(), Arc::clone(&proc.events_rx))
        };

        let start_res = client
            .send_request("turn/start", start_params, Duration::from_secs(20))
            .await
            .map_err(|e| map_rpc_error("turn/start", &e.to_string()))?;

        let turn_id = start_res
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        *live.active_turn_id.lock().await = Some(turn_id);

        let (tx, rx) = mpsc::unbounded_channel();
        let sess_id_owned = session_id.to_string();
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

            let mut full_text = String::new();
            let mut rx_guard = events_rx.lock().await;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(240);

            loop {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    let _ = tx.send(make_envelope(
                        &run_id,
                        &sess_id_owned,
                        message_id.as_deref(),
                        AgentEventPayload::Failed {
                            code: "TIMEOUT".to_string(),
                            message: "Codex 応答がタイムアウトしました".to_string(),
                            rate_limited: false,
                            auth_required: false,
                        },
                    ));
                    break;
                }

                match tokio::time::timeout(remaining, rx_guard.recv()).await {
                    Ok(Some(IncomingEvent::Notification { method, params })) => {
                        if method == "item/agentMessage/delta" {
                            if let Some(d) = params.get("delta").and_then(Value::as_str) {
                                full_text.push_str(d);
                                let _ = tx.send(make_envelope(
                                    &run_id,
                                    &sess_id_owned,
                                    message_id.as_deref(),
                                    AgentEventPayload::TextDelta {
                                        text: d.to_string(),
                                    },
                                ));
                            }
                        } else if method == "thread/tokenUsage/updated" {
                            let input_t = params
                                .pointer("/tokenUsage/last/inputTokens")
                                .and_then(Value::as_u64);
                            let output_t = params
                                .pointer("/tokenUsage/last/outputTokens")
                                .and_then(Value::as_u64);
                            if input_t.is_some() || output_t.is_some() {
                                let _ = tx.send(make_envelope(
                                    &run_id,
                                    &sess_id_owned,
                                    message_id.as_deref(),
                                    AgentEventPayload::UsageUpdated {
                                        input_tokens: input_t,
                                        output_tokens: output_t,
                                        raw_label: None,
                                    },
                                ));
                            }
                        } else if method == "turn/completed" {
                            let status = params
                                .pointer("/turn/status")
                                .and_then(Value::as_str)
                                .unwrap_or("completed");
                            if status.eq_ignore_ascii_case("interrupted")
                                || status.eq_ignore_ascii_case("cancelled")
                            {
                                let _ = tx.send(make_envelope(
                                    &run_id,
                                    &sess_id_owned,
                                    message_id.as_deref(),
                                    AgentEventPayload::Cancelled {
                                        reason: Some("ユーザーにより中断されました".to_string()),
                                    },
                                ));
                            } else if status.eq_ignore_ascii_case("failed") {
                                let err_msg = params
                                    .pointer("/turn/error/message")
                                    .and_then(Value::as_str)
                                    .unwrap_or("Codex ターン実行に失敗しました")
                                    .to_string();
                                let rl = is_rate_limit_text(&err_msg);
                                let auth = is_auth_error_text(&err_msg);
                                let _ = tx.send(make_envelope(
                                    &run_id,
                                    &sess_id_owned,
                                    message_id.as_deref(),
                                    AgentEventPayload::Failed {
                                        code: if rl {
                                            "RATE_LIMITED".to_string()
                                        } else if auth {
                                            "AUTH_REQUIRED".to_string()
                                        } else {
                                            "TURN_FAILED".to_string()
                                        },
                                        message: err_msg,
                                        rate_limited: rl,
                                        auth_required: auth,
                                    },
                                ));
                            } else {
                                let _ = tx.send(make_envelope(
                                    &run_id,
                                    &sess_id_owned,
                                    message_id.as_deref(),
                                    AgentEventPayload::MessageCompleted {
                                        final_text: Some(full_text.clone()),
                                    },
                                ));
                            }
                            break;
                        }
                    }
                    Ok(Some(IncomingEvent::ServerRequest { id, method, params })) => {
                        if method.contains("Approval") || method.contains("requestApproval") {
                            let req_id = Uuid::new_v4().to_string();
                            let detail = params
                                .get("command")
                                .or_else(|| params.get("reason"))
                                .map(ToString::to_string)
                                .unwrap_or_else(|| method.clone());

                            let _ = tx.send(make_envelope(
                                &run_id,
                                &sess_id_owned,
                                message_id.as_deref(),
                                AgentEventPayload::PermissionRequested {
                                    request_id: req_id.clone(),
                                    action: method.clone(),
                                    target: params
                                        .get("cwd")
                                        .and_then(Value::as_str)
                                        .map(ToString::to_string),
                                    detail,
                                },
                            ));

                            let decision = if live.deny_shell_by_default {
                                // Enforce default-deny immediately in non-interactive runs
                                PermissionDecision::Deny
                            } else {
                                let (ptx, prx) = oneshot::channel();
                                live.pending_permissions
                                    .lock()
                                    .await
                                    .insert(req_id.clone(), ptx);
                                match tokio::time::timeout(Duration::from_secs(60), prx).await {
                                    Ok(Ok(dec)) => dec,
                                    _ => {
                                        live.pending_permissions.lock().await.remove(&req_id);
                                        PermissionDecision::Deny
                                    }
                                }
                            };

                            let codex_decision = match decision {
                                PermissionDecision::AllowOnce => "accept",
                                PermissionDecision::Deny => "decline",
                            };
                            let _ = client
                                .send_server_response(id, json!({ "decision": codex_decision }))
                                .await;
                        } else {
                            let _ = client.send_server_response(id, json!({})).await;
                        }
                    }
                    Ok(None) => {
                        let _ = tx.send(make_envelope(
                            &run_id,
                            &sess_id_owned,
                            message_id.as_deref(),
                            AgentEventPayload::Failed {
                                code: "PROCESS_CRASHED".to_string(),
                                message: "Codex プロセスが予期せず終了しました".to_string(),
                                rate_limited: false,
                                auth_required: false,
                            },
                        ));
                        break;
                    }
                    Err(_) => break,
                }
            }

            *live.active_turn_id.lock().await = None;
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

        // Deny any pending permissions so the turn loop unblocks cleanly
        for (_, ptx) in live.pending_permissions.lock().await.drain() {
            let _ = ptx.send(PermissionDecision::Deny);
        }

        let turn_id = live.active_turn_id.lock().await.clone();
        if let Some(tid) = turn_id {
            let client_opt = live.process.lock().await.as_ref().map(|p| p.client.clone());
            if let Some(client) = client_opt {
                let _ = client
                    .send_request(
                        "turn/interrupt",
                        json!({ "threadId": live.thread_id, "turnId": tid }),
                        Duration::from_secs(10),
                    )
                    .await;
            }
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

fn is_rate_limit_text(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("rate limit") || lower.contains("429") || lower.contains("quota")
}

fn is_auth_error_text(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("unauthorized")
        || lower.contains("not logged in")
        || lower.contains("authentication")
        || lower.contains("401")
}

fn map_rpc_error(stage: &str, msg: &str) -> AgentError {
    if is_rate_limit_text(msg) {
        AgentError::RateLimited(format!("{stage}: {msg}"))
    } else if is_auth_error_text(msg) {
        AgentError::AuthRequired(format!("{stage}: {msg}"))
    } else {
        AgentError::Transport(format!("{stage}: {msg}"))
    }
}

/// Converts a standard JSON Schema (Draft 2020-12) into the strict subset accepted by
/// OpenAI Codex `outputSchema` (`response_format` strict mode):
/// - Strips unsupported validation/metadata keywords (`$schema`, `$id`, `default`, `minLength`, etc.)
/// - Converts `const` into a 1-element `enum`
/// - Converts nullable object schemas (`type: ["object", "null"]` with `properties`) into `anyOf`
/// - Ensures every object node sets `additionalProperties: false` and lists EVERY property key in `required`
fn to_openai_strict_schema(value: &Value) -> Value {
    let Some(obj) = value.as_object() else {
        return value.clone();
    };

    // Detect nullable object: `type: ["object", "null"]` with `properties`
    let is_nullable_object = obj
        .get("type")
        .and_then(Value::as_array)
        .is_some_and(|arr| {
            arr.iter().any(|v| v.as_str() == Some("object"))
                && arr.iter().any(|v| v.as_str() == Some("null"))
        })
        && obj.contains_key("properties");

    if is_nullable_object {
        let mut inner = obj.clone();
        inner.insert("type".to_string(), Value::String("object".to_string()));
        let strict_inner = to_openai_strict_schema(&Value::Object(inner));
        return json!({
            "anyOf": [
                strict_inner,
                { "type": "null" }
            ]
        });
    }

    let mut out = serde_json::Map::new();
    const STRIP_KEYS: &[&str] = &[
        "$schema",
        "$id",
        "default",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
        "minimum",
        "maximum",
        "pattern",
        "format",
    ];

    for (k, v) in obj {
        if STRIP_KEYS.contains(&k.as_str()) {
            continue;
        }
        if k == "const" {
            out.insert("enum".to_string(), Value::Array(vec![v.clone()]));
            continue;
        }
        if k == "properties"
            && let Some(props) = v.as_object()
        {
            let mut strict_props = serde_json::Map::new();
            for (pk, pv) in props {
                strict_props.insert(pk.clone(), to_openai_strict_schema(pv));
            }
            out.insert("properties".to_string(), Value::Object(strict_props));
            continue;
        }
        if k == "items" {
            out.insert("items".to_string(), to_openai_strict_schema(v));
            continue;
        }
        if k == "anyOf"
            && let Some(variants) = v.as_array()
        {
            out.insert(
                "anyOf".to_string(),
                Value::Array(variants.iter().map(to_openai_strict_schema).collect()),
            );
            continue;
        }
        out.insert(k.clone(), v.clone());
    }

    if out.get("type").and_then(Value::as_str) == Some("object")
        && let Some(props) = out.get("properties").and_then(Value::as_object)
    {
        let mut keys: Vec<String> = props.keys().cloned().collect();
        keys.sort();
        out.insert(
            "required".to_string(),
            Value::Array(keys.into_iter().map(Value::String).collect()),
        );
        out.insert("additionalProperties".to_string(), Value::Bool(false));
    }

    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::{recognition_schema_json, solution_schema_json};

    fn assert_openai_strict_valid(node: &Value) {
        let Some(obj) = node.as_object() else {
            return;
        };
        assert!(!obj.contains_key("$schema"));
        assert!(!obj.contains_key("const"));
        assert!(!obj.contains_key("default"));
        if obj.get("type").and_then(Value::as_str) == Some("object")
            && let Some(props) = obj.get("properties").and_then(Value::as_object)
        {
            assert_eq!(
                obj.get("additionalProperties").and_then(Value::as_bool),
                Some(false)
            );
            let req = obj
                .get("required")
                .and_then(Value::as_array)
                .expect("required array");
            for key in props.keys() {
                assert!(
                    req.iter().any(|v| v.as_str() == Some(key.as_str())),
                    "missing required key: {key}"
                );
            }
            for child in props.values() {
                assert_openai_strict_valid(child);
            }
        }
        if let Some(items) = obj.get("items") {
            assert_openai_strict_valid(items);
        }
        if let Some(any_of) = obj.get("anyOf").and_then(Value::as_array) {
            for variant in any_of {
                assert_openai_strict_valid(variant);
            }
        }
    }

    #[test]
    fn test_to_openai_strict_schema_normalizes_project_schemas() {
        let rec = to_openai_strict_schema(&recognition_schema_json());
        assert_openai_strict_valid(&rec);
        let sol = to_openai_strict_schema(&solution_schema_json());
        assert_openai_strict_valid(&sol);
    }
}
