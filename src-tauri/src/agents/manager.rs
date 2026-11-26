use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::agents::adapter::{
    AgentAdapter, AgentError, AgentImageInput, AgentInput, PermissionDecision, SessionConfig,
};
use crate::agents::providers::acp::AcpAdapter;
use crate::agents::providers::claude::ClaudeCodeAdapter;
use crate::agents::providers::codex::CodexAdapter;
use crate::agents::providers::mock::MockAdapter;
use crate::agents::sandbox::IsolatedWorkspace;
use crate::domain::{
    AgentEventEnvelope, AgentEventPayload, AgentProfile, AgentStatus, ScienceMcpSettings,
};
use crate::science_mcp::{SCIENCE_MCP_SERVER_NAME, ScopedMcpServerConfig, build_scoped_mcp_config};

pub type EventEmitter = Arc<dyn Fn(&AgentEventEnvelope) + Send + Sync>;

struct ActiveSessionEntry {
    adapter: Arc<dyn AgentAdapter>,
    internal_session_id: String,
    external_session_id: Option<String>,
    workspace_root: PathBuf,
    workspace_rel_path: String,
    resumable: bool,
}

struct ActiveRunEntry {
    adapter: Arc<dyn AgentAdapter>,
    internal_session_id: String,
}

#[derive(Debug, Clone)]
pub struct CapturedToolExecution {
    pub call_id: String,
    pub server_name: String,
    pub tool_name: String,
    pub arguments: Value,
    pub output: Option<Value>,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub error_message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CollectedWorkspaceArtifact {
    pub workspace_rel_path: String,
    pub filename: String,
    pub mime_type: String,
    pub caption: Option<String>,
    pub source_call_id: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct RunPromptOutcome {
    pub internal_session_id: String,
    pub external_session_id: Option<String>,
    pub workspace_rel_path: String,
    pub resumed_external_session: bool,
    pub resumable: bool,
    pub final_text: String,
    pub status: String,
    pub usage_json: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub rate_limited: bool,
    pub auth_required: bool,
    pub tool_executions: Vec<CapturedToolExecution>,
    pub collected_artifacts: Vec<CollectedWorkspaceArtifact>,
}

/// Coordinates all AI agent adapters, isolated workspaces, session reuse/resume,
/// active run cancellation, and interactive permission responses.
pub struct AgentManager {
    workspaces_root: PathBuf,
    /// Keyed by `"{conversation_id}:{profile_id}"` for live chat session reuse.
    live_sessions: Arc<Mutex<HashMap<String, ActiveSessionEntry>>>,
    /// Keyed by `run_id` for cancel & permission routing.
    active_runs: Arc<Mutex<HashMap<String, ActiveRunEntry>>>,
    emitter: Arc<Mutex<Option<EventEmitter>>>,
}

impl AgentManager {
    pub fn new(workspaces_root: PathBuf) -> Self {
        Self {
            workspaces_root,
            live_sessions: Arc::new(Mutex::new(HashMap::new())),
            active_runs: Arc::new(Mutex::new(HashMap::new())),
            emitter: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn set_emitter(&self, emitter: EventEmitter) {
        *self.emitter.lock().await = Some(emitter);
    }

    pub fn build_adapter(profile: &AgentProfile) -> Arc<dyn AgentAdapter> {
        match profile.provider_kind.as_str() {
            "codex" => Arc::new(CodexAdapter::new(
                profile.executable_path.clone(),
                profile.args.clone(),
            )),
            "antigravity" => Arc::new(AcpAdapter::antigravity(
                profile.executable_path.clone(),
                profile.args.clone(),
            )),
            "custom_acp" => Arc::new(AcpAdapter::custom(
                profile.executable_path.clone(),
                profile.args.clone(),
            )),
            "claude_code" => Arc::new(ClaudeCodeAdapter::new(
                profile.executable_path.clone(),
                profile.auth_mode.clone(),
            )),
            _ => Arc::new(MockAdapter::new()),
        }
    }

    pub async fn check_availability(
        &self,
        profile: &AgentProfile,
    ) -> Result<AgentStatus, AgentError> {
        let adapter = Self::build_adapter(profile);
        adapter.check_availability().await
    }

    /// Runs a one-shot isolated prompt (used for problem recognition & solution generation).
    /// Always enforces `deny_shell_by_default = true`, attaches scoped Science MCP if enabled,
    /// harvests generated plot artifacts BEFORE cleaning up the session workspace.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_isolated_prompt(
        &self,
        profile: &AgentProfile,
        explicit_api_key: Option<String>,
        run_id: &str,
        prompt_text: String,
        images: Vec<AgentImageInput>,
        model_override: Option<String>,
        output_schema: Option<serde_json::Value>,
        science_mcp_settings: Option<&ScienceMcpSettings>,
    ) -> Result<RunPromptOutcome, AgentError> {
        let adapter = Self::build_adapter(profile);
        let caps = adapter.capabilities().await?;
        if !images.is_empty() && !caps.supports_image_input {
            return Err(AgentError::UnsupportedCapability(
                "画像入力（画像対応エージェントへ切り替えてください）".to_string(),
            ));
        }

        let internal_sid = Uuid::new_v4().to_string();
        let ws = IsolatedWorkspace::ensure(&self.workspaces_root, &internal_sid)
            .map_err(|e| AgentError::Transport(format!("ワークスペース作成失敗: {e}")))?;

        let mcp_servers = build_session_mcp_servers(
            caps.supports_mcp_scoped_config,
            science_mcp_settings,
            &ws.artifacts_dir,
        );

        let session = adapter
            .create_session(SessionConfig {
                internal_session_id: internal_sid.clone(),
                workspace_root: ws.root.clone(),
                resume_external_session_id: None,
                model_name: model_override
                    .clone()
                    .or_else(|| profile.default_model.clone()),
                explicit_api_key,
                deny_shell_by_default: true,
                mcp_servers,
            })
            .await?;

        self.active_runs.lock().await.insert(
            run_id.to_string(),
            ActiveRunEntry {
                adapter: Arc::clone(&adapter),
                internal_session_id: session.session_id.clone(),
            },
        );

        let result = self
            .consume_prompt(
                &adapter,
                &session.session_id,
                session.external_session_id.clone(),
                &ws.root,
                ws.relative_path.clone(),
                session.resumed,
                session.resumable,
                AgentInput {
                    run_id: run_id.to_string(),
                    message_id: None,
                    prompt_text,
                    images,
                    model_override: model_override.or_else(|| profile.default_model.clone()),
                    output_schema,
                },
            )
            .await;

        self.active_runs.lock().await.remove(run_id);
        let _ = adapter.close_session(&session.session_id).await;
        ws.cleanup();
        result
    }

    /// Runs an interactive chat turn with session reuse/resume, scoped Science MCP,
    /// live streaming emission, and interactive permission forwarding.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_chat_turn(
        &self,
        profile: &AgentProfile,
        explicit_api_key: Option<String>,
        conversation_id: &str,
        stored_external_session_id: Option<String>,
        run_id: &str,
        message_id: &str,
        prompt_text: String,
        images: Vec<AgentImageInput>,
        model_override: Option<String>,
        science_mcp_settings: Option<&ScienceMcpSettings>,
    ) -> Result<RunPromptOutcome, AgentError> {
        let session_key = format!("{conversation_id}:{}", profile.id);

        // Reuse existing live session if present; otherwise create/resume one
        let (adapter, internal_sid, external_sid, ws_root, ws_rel, resumed, resumable) = {
            let guard = self.live_sessions.lock().await;
            if let Some(existing) = guard.get(&session_key) {
                (
                    Arc::clone(&existing.adapter),
                    existing.internal_session_id.clone(),
                    existing.external_session_id.clone(),
                    existing.workspace_root.clone(),
                    existing.workspace_rel_path.clone(),
                    true,
                    existing.resumable,
                )
            } else {
                drop(guard);
                let adapter = Self::build_adapter(profile);
                let caps = adapter.capabilities().await?;
                if !images.is_empty() && !caps.supports_image_input {
                    return Err(AgentError::UnsupportedCapability(
                        "画像入力（画像対応エージェントへ切り替えてください）".to_string(),
                    ));
                }
                let internal_sid = Uuid::new_v4().to_string();
                let ws = IsolatedWorkspace::ensure(&self.workspaces_root, &internal_sid)
                    .map_err(|e| AgentError::Transport(format!("ワークスペース作成失敗: {e}")))?;

                let mcp_servers = build_session_mcp_servers(
                    caps.supports_mcp_scoped_config,
                    science_mcp_settings,
                    &ws.artifacts_dir,
                );

                let session = adapter
                    .create_session(SessionConfig {
                        internal_session_id: internal_sid.clone(),
                        workspace_root: ws.root.clone(),
                        resume_external_session_id: stored_external_session_id,
                        model_name: model_override
                            .clone()
                            .or_else(|| profile.default_model.clone()),
                        explicit_api_key,
                        deny_shell_by_default: false,
                        mcp_servers,
                    })
                    .await?;

                let entry = ActiveSessionEntry {
                    adapter: Arc::clone(&adapter),
                    internal_session_id: session.session_id.clone(),
                    external_session_id: session.external_session_id.clone(),
                    workspace_root: ws.root.clone(),
                    workspace_rel_path: ws.relative_path.clone(),
                    resumable: session.resumable,
                };
                self.live_sessions
                    .lock()
                    .await
                    .insert(session_key.clone(), entry);

                (
                    adapter,
                    session.session_id,
                    session.external_session_id,
                    ws.root,
                    ws.relative_path,
                    session.resumed,
                    session.resumable,
                )
            }
        };

        self.active_runs.lock().await.insert(
            run_id.to_string(),
            ActiveRunEntry {
                adapter: Arc::clone(&adapter),
                internal_session_id: internal_sid.clone(),
            },
        );

        let result = self
            .consume_prompt(
                &adapter,
                &internal_sid,
                external_sid,
                &ws_root,
                ws_rel,
                resumed,
                resumable,
                AgentInput {
                    run_id: run_id.to_string(),
                    message_id: Some(message_id.to_string()),
                    prompt_text,
                    images,
                    model_override: model_override.or_else(|| profile.default_model.clone()),
                    output_schema: None,
                },
            )
            .await;

        self.active_runs.lock().await.remove(run_id);

        // If the underlying harness crashed, evict live session so next turn recovers cleanly
        if let Ok(ref out) = result
            && out.error_code.as_deref() == Some("PROCESS_CRASHED")
        {
            if let Some(dead) = self.live_sessions.lock().await.remove(&session_key) {
                let _ = dead.adapter.close_session(&dead.internal_session_id).await;
            }
        } else if result.is_err()
            && let Some(dead) = self.live_sessions.lock().await.remove(&session_key)
        {
            let _ = dead.adapter.close_session(&dead.internal_session_id).await;
        }

        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn consume_prompt(
        &self,
        adapter: &Arc<dyn AgentAdapter>,
        internal_session_id: &str,
        external_session_id: Option<String>,
        workspace_root: &Path,
        workspace_rel_path: String,
        resumed_external_session: bool,
        resumable: bool,
        input: AgentInput,
    ) -> Result<RunPromptOutcome, AgentError> {
        let mut stream = adapter.prompt(internal_session_id, input).await?;
        let emitter_opt = self.emitter.lock().await.clone();

        let mut text = String::new();
        let mut status = "completed".to_string();
        let mut usage_json: Option<String> = None;
        let mut error_code: Option<String> = None;
        let mut error_message: Option<String> = None;
        let mut rate_limited = false;
        let mut auth_required = false;

        let mut tool_calls_order: Vec<String> = Vec::new();
        let mut tool_calls_map: HashMap<String, CapturedToolExecution> = HashMap::new();
        let mut artifact_hints: Vec<(String, String, Option<String>, Option<String>)> = Vec::new();

        while let Some(env) = stream.next().await {
            if let Some(ref emit) = emitter_opt {
                emit(&env);
            }
            match &env.event {
                AgentEventPayload::TextDelta { text: delta } => {
                    text.push_str(delta);
                }
                AgentEventPayload::ToolCallStarted {
                    call_id,
                    server,
                    tool_name,
                    arguments,
                } => {
                    if !tool_calls_map.contains_key(call_id) {
                        tool_calls_order.push(call_id.clone());
                    }
                    tool_calls_map.insert(
                        call_id.clone(),
                        CapturedToolExecution {
                            call_id: call_id.clone(),
                            server_name: server
                                .clone()
                                .unwrap_or_else(|| SCIENCE_MCP_SERVER_NAME.to_string()),
                            tool_name: tool_name.clone(),
                            arguments: arguments.clone(),
                            output: None,
                            status: "running".to_string(),
                            duration_ms: None,
                            error_message: None,
                            started_at: env.timestamp.clone(),
                            finished_at: None,
                        },
                    );
                }
                AgentEventPayload::ToolCallCompleted {
                    call_id,
                    succeeded,
                    output,
                } => {
                    let entry = tool_calls_map.entry(call_id.clone()).or_insert_with(|| {
                        tool_calls_order.push(call_id.clone());
                        CapturedToolExecution {
                            call_id: call_id.clone(),
                            server_name: SCIENCE_MCP_SERVER_NAME.to_string(),
                            tool_name: output
                                .get("tool")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown_tool")
                                .to_string(),
                            arguments: serde_json::json!({}),
                            output: None,
                            status: "running".to_string(),
                            duration_ms: None,
                            error_message: None,
                            started_at: env.timestamp.clone(),
                            finished_at: None,
                        }
                    });
                    let err_code = output.pointer("/error/code").and_then(Value::as_str);
                    let err_msg = output
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    entry.status = if err_code == Some("TIMEOUT") {
                        "timeout".to_string()
                    } else if *succeeded {
                        "succeeded".to_string()
                    } else {
                        "failed".to_string()
                    };
                    entry.duration_ms = output.get("duration_ms").and_then(Value::as_i64);
                    entry.error_message = err_msg;
                    entry.output = Some(output.clone());
                    entry.finished_at = Some(env.timestamp.clone());

                    if let Some(arts) = output.get("artifacts").and_then(Value::as_array) {
                        for art in arts {
                            if let Some(rel) = art.get("relative_path").and_then(Value::as_str) {
                                let mime = art
                                    .get("mime_type")
                                    .and_then(Value::as_str)
                                    .unwrap_or("image/svg+xml")
                                    .to_string();
                                let cap = art
                                    .get("caption")
                                    .and_then(Value::as_str)
                                    .map(ToString::to_string);
                                artifact_hints.push((
                                    rel.to_string(),
                                    mime,
                                    cap,
                                    Some(call_id.clone()),
                                ));
                            }
                        }
                    }
                }
                AgentEventPayload::ArtifactCreated {
                    relative_path,
                    mime_type,
                    caption,
                } => {
                    if !artifact_hints.iter().any(|(r, _, _, _)| r == relative_path) {
                        artifact_hints.push((
                            relative_path.clone(),
                            mime_type.clone(),
                            caption.clone(),
                            None,
                        ));
                    }
                }
                AgentEventPayload::UsageUpdated {
                    input_tokens,
                    output_tokens,
                    raw_label,
                } => {
                    usage_json = Some(
                        serde_json::json!({
                            "inputTokens": input_tokens,
                            "outputTokens": output_tokens,
                            "rawLabel": raw_label
                        })
                        .to_string(),
                    );
                }
                AgentEventPayload::MessageCompleted { final_text } => {
                    if let Some(ft) = final_text
                        && text.is_empty()
                    {
                        text = ft.clone();
                    }
                    status = "completed".to_string();
                }
                AgentEventPayload::Cancelled { .. } => {
                    status = "cancelled".to_string();
                }
                AgentEventPayload::Failed {
                    code,
                    message,
                    rate_limited: rl,
                    auth_required: ar,
                } => {
                    status = "failed".to_string();
                    error_code = Some(code.clone());
                    error_message = Some(message.clone());
                    rate_limited = *rl;
                    auth_required = *ar;
                }
                _ => {}
            }
        }

        let tool_executions: Vec<CapturedToolExecution> = tool_calls_order
            .into_iter()
            .filter_map(|id| tool_calls_map.remove(&id))
            .map(|mut item| {
                if item.status == "running" {
                    item.status = if status == "cancelled" {
                        "cancelled".to_string()
                    } else {
                        "failed".to_string()
                    };
                    item.finished_at = Some(Utc::now().to_rfc3339());
                }
                item
            })
            .collect();

        let collected_artifacts =
            collect_safe_workspace_artifacts(&workspace_root.join("artifacts"), &artifact_hints);

        Ok(RunPromptOutcome {
            internal_session_id: internal_session_id.to_string(),
            external_session_id,
            workspace_rel_path,
            resumed_external_session,
            resumable,
            final_text: text,
            status,
            usage_json,
            error_code,
            error_message,
            rate_limited,
            auth_required,
            tool_executions,
            collected_artifacts,
        })
    }

    pub async fn cancel_run(&self, run_id: &str) -> Result<(), AgentError> {
        let entry = self
            .active_runs
            .lock()
            .await
            .get(run_id)
            .map(|e| (Arc::clone(&e.adapter), e.internal_session_id.clone()));
        if let Some((adapter, sid)) = entry {
            adapter.cancel(&sid).await?;
        }
        Ok(())
    }

    pub async fn respond_permission(
        &self,
        run_id: &str,
        request_id: &str,
        decision: PermissionDecision,
    ) -> Result<(), AgentError> {
        let entry = self
            .active_runs
            .lock()
            .await
            .get(run_id)
            .map(|e| (Arc::clone(&e.adapter), e.internal_session_id.clone()));
        if let Some((adapter, sid)) = entry {
            adapter
                .respond_permission(&sid, request_id, decision)
                .await?;
        }
        Ok(())
    }

    pub async fn close_conversation_sessions(&self, conversation_id: &str) {
        let prefix = format!("{conversation_id}:");
        let to_close: Vec<ActiveSessionEntry> = {
            let mut guard = self.live_sessions.lock().await;
            let keys: Vec<String> = guard
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .cloned()
                .collect();
            keys.into_iter().filter_map(|k| guard.remove(&k)).collect()
        };
        for entry in to_close {
            let _ = entry
                .adapter
                .close_session(&entry.internal_session_id)
                .await;
        }
    }
}

fn build_session_mcp_servers(
    supports_mcp_scoped_config: bool,
    settings: Option<&ScienceMcpSettings>,
    artifacts_dir: &Path,
) -> Vec<ScopedMcpServerConfig> {
    if !supports_mcp_scoped_config {
        return Vec::new();
    }
    let Some(mcp_cfg) = settings else {
        return Vec::new();
    };
    match build_scoped_mcp_config(mcp_cfg, artifacts_dir) {
        Some(spec) => vec![spec],
        None => Vec::new(),
    }
}

/// Safely reads generated SVG/PNG plot artifacts confined strictly inside `artifacts_dir`.
/// Rejects symlinks, traversal paths, oversized files (>15 MB), and invalid formats.
pub fn collect_safe_workspace_artifacts(
    artifacts_dir: &Path,
    hints: &[(String, String, Option<String>, Option<String>)],
) -> Vec<CollectedWorkspaceArtifact> {
    const MAX_ARTIFACT_BYTES: u64 = 15 * 1024 * 1024;
    let Ok(canonical_dir) = std::fs::canonicalize(artifacts_dir) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&canonical_dir) else {
        return Vec::new();
    };

    let mut collected = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(sym_meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if sym_meta.file_type().is_symlink() || !sym_meta.is_file() {
            continue;
        }
        if sym_meta.len() == 0 || sym_meta.len() > MAX_ARTIFACT_BYTES {
            continue;
        }
        let Ok(canon_file) = std::fs::canonicalize(&path) else {
            continue;
        };
        if !canon_file.starts_with(&canonical_dir) {
            continue;
        }
        let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if ext != "svg" && ext != "png" {
            continue;
        }
        let Ok(bytes) = std::fs::read(&canon_file) else {
            continue;
        };
        let mime_type = if ext == "svg" {
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
            if !head.contains("<svg") {
                continue;
            }
            "image/svg+xml".to_string()
        } else {
            if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                continue;
            }
            "image/png".to_string()
        };

        let ws_rel = format!("artifacts/{fname}");
        let hint = hints
            .iter()
            .find(|(r, _, _, _)| r == &ws_rel || r.ends_with(&format!("/{fname}")));
        collected.push(CollectedWorkspaceArtifact {
            workspace_rel_path: ws_rel,
            filename: fname.to_string(),
            mime_type,
            caption: hint.and_then(|(_, _, c, _)| c.clone()),
            source_call_id: hint.and_then(|(_, _, _, cid)| cid.clone()),
            bytes,
        });
    }
    collected
}
