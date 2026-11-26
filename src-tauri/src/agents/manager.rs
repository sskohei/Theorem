use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use futures_util::StreamExt;
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
use crate::domain::{AgentEventEnvelope, AgentEventPayload, AgentProfile, AgentStatus};

pub type EventEmitter = Arc<dyn Fn(&AgentEventEnvelope) + Send + Sync>;

struct ActiveSessionEntry {
    adapter: Arc<dyn AgentAdapter>,
    internal_session_id: String,
    external_session_id: Option<String>,
    workspace_rel_path: String,
    resumable: bool,
}

struct ActiveRunEntry {
    adapter: Arc<dyn AgentAdapter>,
    internal_session_id: String,
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
    /// Always enforces `deny_shell_by_default = true` and cleans up session process on completion.
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

    /// Runs an interactive chat turn with session reuse/resume, live streaming emission,
    /// and interactive permission forwarding.
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
    ) -> Result<RunPromptOutcome, AgentError> {
        let session_key = format!("{conversation_id}:{}", profile.id);

        // Reuse existing live session if present; otherwise create/resume one
        let (adapter, internal_sid, external_sid, ws_rel, resumed, resumable) = {
            let guard = self.live_sessions.lock().await;
            if let Some(existing) = guard.get(&session_key) {
                (
                    Arc::clone(&existing.adapter),
                    existing.internal_session_id.clone(),
                    existing.external_session_id.clone(),
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
                    })
                    .await?;

                let entry = ActiveSessionEntry {
                    adapter: Arc::clone(&adapter),
                    internal_session_id: session.session_id.clone(),
                    external_session_id: session.external_session_id.clone(),
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

        while let Some(env) = stream.next().await {
            if let Some(ref emit) = emitter_opt {
                emit(&env);
            }
            match &env.event {
                AgentEventPayload::TextDelta { text: delta } => {
                    text.push_str(delta);
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
