use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Mutex, mpsc};

use crate::agents::adapter::{
    AgentAdapter, AgentError, AgentEventStream, AgentInput, AgentSession, PermissionDecision,
    SessionConfig, make_envelope,
};
use crate::agents::sandbox::{
    ProcessJobGuard, apply_sanitized_env, probe_cli_version, resolve_cli_binary,
};
use crate::domain::{AgentCapabilities, AgentEventPayload, AgentStatus};

struct ClaudeSessionState {
    workspace_root: PathBuf,
    api_key: String,
    model_name: Option<String>,
    mcp_config_path: Option<PathBuf>,
    active_kill_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

/// Anthropic Claude Code adapter with strict Terms-of-Service compliance:
/// - NEVER extracts, relays, or reuses third-party Claude subscription OAuth tokens.
/// - Disables execution under `official_harness` until confirmed permitted by official terms.
/// - Allows execution ONLY when the user explicitly opts into `api_key_keyring` (OS Keychain).
pub struct ClaudeCodeAdapter {
    manual_path: Option<String>,
    auth_mode: String,
    sessions: Arc<Mutex<HashMap<String, Arc<ClaudeSessionState>>>>,
}

impl ClaudeCodeAdapter {
    pub fn new(manual_path: Option<String>, auth_mode: String) -> Self {
        Self {
            manual_path,
            auth_mode,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn default_capabilities(&self) -> AgentCapabilities {
        let allowed = self.auth_mode == "api_key_keyring";
        AgentCapabilities {
            supports_image_input: false,
            supports_pdf_input: false,
            supports_streaming: allowed,
            supports_session_resume: false,
            supports_model_listing: false,
            supports_auth_status_check: false,
            supports_mcp_scoped_config: allowed,
            supports_permission_hooks: false,
            supports_sandbox_restriction: allowed,
            supports_cancel: allowed,
        }
    }
}

#[async_trait]
impl AgentAdapter for ClaudeCodeAdapter {
    async fn check_availability(&self) -> Result<AgentStatus, AgentError> {
        let binary = resolve_cli_binary(self.manual_path.as_deref(), "claude");
        let version = match &binary {
            Some(p) => probe_cli_version(p).await,
            None => None,
        };
        let installed = binary.is_some();

        Ok(AgentStatus {
            provider_kind: "claude_code".to_string(),
            installed,
            resolved_path: binary.map(|p| p.to_string_lossy().into_owned()),
            version,
            auth_status: if self.auth_mode == "api_key_keyring" {
                "unknown".to_string()
            } else {
                "terms_restricted".to_string()
            },
            auth_method: if self.auth_mode == "api_key_keyring" {
                Some("api_key_keyring".to_string())
            } else {
                None
            },
            // Honesty rule: never mark unverified Claude Code as verified_live.
            verification_status: "unverified".to_string(),
            available_models: None,
            capabilities: self.default_capabilities(),
            auth_guide: "Anthropic の利用規約により、サブスクリプション OAuth のサードパーティ流用は無効化されています。利用する場合は設定画面の「モデル設定」から明示同意のうえ API キー（OS キーチェーン保存）を設定してください。".to_string(),
            compliance_note: "許可なく従量課金へ切り替えません。実行時は `--permission-mode plan` および任意シェル禁止で隔離実行します。".to_string(),
            detail_note: if !installed {
                Some("本環境には claude CLI が未インストールのため未検証です。".into())
            } else if self.auth_mode != "api_key_keyring" {
                Some("規約遵守のため official_harness モードでの実行は無効化されています。".into())
            } else {
                Some("API キー明示利用モード（実機未検証）。".into())
            },
        })
    }

    async fn capabilities(&self) -> Result<AgentCapabilities, AgentError> {
        Ok(self.default_capabilities())
    }

    async fn create_session(&self, config: SessionConfig) -> Result<AgentSession, AgentError> {
        if self.auth_mode != "api_key_keyring" {
            return Err(AgentError::PolicyBlocked(
                "Claude Code のサブスクリプション OAuth 流用は利用規約上の制約により無効化されています。明示同意のもと API キー方式を選択するか、Codex / Antigravity をご利用ください。"
                    .into(),
            ));
        }

        let api_key = config.explicit_api_key.filter(|k| !k.trim().is_empty()).ok_or_else(|| {
            AgentError::AuthRequired(
                "Claude Code の API キーが OS キーチェーンに登録されていません。設定画面の「モデル設定」から登録してください。"
                    .into(),
            )
        })?;

        let _binary = resolve_cli_binary(self.manual_path.as_deref(), "claude")
            .ok_or_else(|| AgentError::NotInstalled("claude".into()))?;

        // Write session-scoped MCP config ONLY inside isolated workspace/context (never touches ~/.claude.json)
        let mcp_config_path = if !config.mcp_servers.is_empty() {
            let mut mcp_map = serde_json::Map::new();
            for srv in &config.mcp_servers {
                let mut env_obj = serde_json::Map::new();
                for (k, v) in &srv.env {
                    env_obj.insert(k.clone(), Value::String(v.clone()));
                }
                mcp_map.insert(
                    srv.name.clone(),
                    serde_json::json!({
                        "command": srv.command,
                        "args": srv.args,
                        "env": Value::Object(env_obj)
                    }),
                );
            }
            let ctx_dir = config.workspace_root.join("context");
            let _ = std::fs::create_dir_all(&ctx_dir);
            let cfg_path = ctx_dir.join("mcp-config.json");
            let body = serde_json::json!({ "mcpServers": Value::Object(mcp_map) });
            std::fs::write(
                &cfg_path,
                serde_json::to_string_pretty(&body).unwrap_or_default(),
            )
            .map_err(|e| AgentError::Transport(format!("MCP 設定書き出し失敗: {e}")))?;
            Some(cfg_path)
        } else {
            None
        };

        let state = Arc::new(ClaudeSessionState {
            workspace_root: config.workspace_root,
            api_key,
            model_name: config.model_name,
            mcp_config_path,
            active_kill_tx: Arc::new(Mutex::new(None)),
        });
        self.sessions
            .lock()
            .await
            .insert(config.internal_session_id.clone(), state);

        Ok(AgentSession {
            session_id: config.internal_session_id,
            external_session_id: None,
            resumed: false,
            resumable: false,
        })
    }

    async fn prompt(
        &self,
        session_id: &str,
        input: AgentInput,
    ) -> Result<AgentEventStream, AgentError> {
        let sess = self
            .sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let binary = resolve_cli_binary(self.manual_path.as_deref(), "claude")
            .ok_or_else(|| AgentError::NotInstalled("claude".into()))?;

        let mut cmd = Command::new(&binary);
        cmd.arg("-p")
            .arg(&input.prompt_text)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .arg("--permission-mode")
            .arg("plan")
            .arg("--disallowedTools")
            .arg("Bash,Write,Edit")
            .current_dir(&sess.workspace_root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        if let Some(ref mcp_path) = sess.mcp_config_path {
            cmd.arg("--mcp-config")
                .arg(mcp_path)
                .arg("--strict-mcp-config");
        }

        if let Some(model) = input
            .model_override
            .as_deref()
            .or(sess.model_name.as_deref())
            .filter(|m| !m.trim().is_empty())
        {
            cmd.arg("--model").arg(model.trim());
        }

        apply_sanitized_env(&mut cmd, &[("ANTHROPIC_API_KEY", &sess.api_key)]);

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| AgentError::Transport(format!("claude 起動失敗: {e}")))?;
        let job_guard = ProcessJobGuard::new().ok();
        if let (Some(guard), Some(pid)) = (&job_guard, child.id()) {
            let _ = guard.assign_pid(pid);
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentError::Transport("claude stdout を取得できませんでした".into()))?;

        let (kill_tx, mut kill_rx) = tokio::sync::oneshot::channel();
        *sess.active_kill_tx.lock().await = Some(kill_tx);

        let (tx, rx) = mpsc::unbounded_channel();
        let sess_id_owned = session_id.to_string();
        let run_id = input.run_id.clone();
        let message_id = input.message_id.clone();

        tokio::spawn(async move {
            let _guard = job_guard;
            let _ = tx.send(make_envelope(
                &run_id,
                &sess_id_owned,
                message_id.as_deref(),
                AgentEventPayload::MessageStarted {
                    role: "assistant".to_string(),
                },
            ));

            let mut reader = BufReader::new(stdout).lines();
            let mut full_text = String::new();
            let mut cancelled = false;

            loop {
                tokio::select! {
                    _ = &mut kill_rx => {
                        cancelled = true;
                        let _ = child.kill().await;
                        break;
                    }
                    line = reader.next_line() => {
                        match line {
                            Ok(Some(l)) => {
                                if let Ok(v) = serde_json::from_str::<Value>(l.trim())
                                    && let Some(chunk) = extract_claude_stream_text(&v)
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
                            Ok(None) | Err(_) => break,
                        }
                    }
                }
            }

            let status = child.wait().await;
            *sess.active_kill_tx.lock().await = None;

            if cancelled {
                let _ = tx.send(make_envelope(
                    &run_id,
                    &sess_id_owned,
                    message_id.as_deref(),
                    AgentEventPayload::Cancelled {
                        reason: Some("ユーザーにより中断されました".to_string()),
                    },
                ));
            } else if status.map(|s| s.success()).unwrap_or(false) {
                let _ = tx.send(make_envelope(
                    &run_id,
                    &sess_id_owned,
                    message_id.as_deref(),
                    AgentEventPayload::MessageCompleted {
                        final_text: Some(full_text),
                    },
                ));
            } else {
                let _ = tx.send(make_envelope(
                    &run_id,
                    &sess_id_owned,
                    message_id.as_deref(),
                    AgentEventPayload::Failed {
                        code: "CLAUDE_EXEC_ERROR".to_string(),
                        message: "Claude Code プロセスがエラー終了しました".to_string(),
                        rate_limited: false,
                        auth_required: false,
                    },
                ));
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
        _session_id: &str,
        _request_id: &str,
        _decision: PermissionDecision,
    ) -> Result<(), AgentError> {
        Ok(())
    }

    async fn cancel(&self, session_id: &str) -> Result<(), AgentError> {
        if let Some(sess) = self.sessions.lock().await.get(session_id).cloned()
            && let Some(tx) = sess.active_kill_tx.lock().await.take()
        {
            let _ = tx.send(());
        }
        Ok(())
    }

    async fn close_session(&self, session_id: &str) -> Result<(), AgentError> {
        self.cancel(session_id).await?;
        self.sessions.lock().await.remove(session_id);
        Ok(())
    }
}

fn extract_claude_stream_text(v: &Value) -> Option<String> {
    if v.get("type").and_then(Value::as_str) == Some("assistant")
        && let Some(arr) = v.pointer("/message/content").and_then(Value::as_array)
    {
        let joined: String = arr
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(Value::as_str) == Some("text") {
                    b.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .collect();
        if !joined.is_empty() {
            return Some(joined);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_claude_blocks_subscription_oauth_mode() {
        let adapter = ClaudeCodeAdapter::new(None, "official_harness".to_string());
        let status = adapter.check_availability().await.expect("status");
        assert_eq!(status.verification_status, "unverified");
        assert_eq!(status.auth_status, "terms_restricted");

        let res = adapter
            .create_session(SessionConfig {
                internal_session_id: "s-1".to_string(),
                workspace_root: PathBuf::from("."),
                resume_external_session_id: None,
                model_name: None,
                explicit_api_key: None,
                deny_shell_by_default: true,
                mcp_servers: Vec::new(),
            })
            .await;
        assert!(matches!(res, Err(AgentError::PolicyBlocked(_))));
    }
}
