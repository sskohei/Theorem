use std::path::PathBuf;
use std::pin::Pin;

use async_trait::async_trait;
use chrono::Utc;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{AgentCapabilities, AgentEventEnvelope, AgentEventPayload, AgentStatus};

pub type AgentEventStream = Pin<Box<dyn Stream<Item = AgentEventEnvelope> + Send>>;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("エージェント実行ファイルが見つかりません: {0}")]
    NotInstalled(String),

    #[error("認証が必要です: {0}")]
    AuthRequired(String),

    #[error("利用規約または安全制約によりこの実行方式は無効化されています: {0}")]
    PolicyBlocked(String),

    #[error("利用上限（レート制限）に達しました: {0}")]
    RateLimited(String),

    #[error("このエージェントは必要な機能（{0}）に対応していません")]
    UnsupportedCapability(String),

    #[error("セッションが見つかりません: {0}")]
    SessionNotFound(String),

    #[error("処理がキャンセルされました: {0}")]
    Cancelled(String),

    #[error("通信タイムアウトが発生しました: {0}")]
    Timeout(String),

    #[error("プロセス通信エラー: {0}")]
    Transport(String),
}

impl AgentError {
    pub fn error_code(&self) -> &'static str {
        match self {
            AgentError::NotInstalled(_) => "AGENT_NOT_INSTALLED",
            AgentError::AuthRequired(_) => "AUTH_REQUIRED",
            AgentError::PolicyBlocked(_) => "POLICY_BLOCKED",
            AgentError::RateLimited(_) => "RATE_LIMITED",
            AgentError::UnsupportedCapability(_) => "UNSUPPORTED_CAPABILITY",
            AgentError::SessionNotFound(_) => "SESSION_NOT_FOUND",
            AgentError::Cancelled(_) => "CANCELLED",
            AgentError::Timeout(_) => "TIMEOUT",
            AgentError::Transport(_) => "TRANSPORT_ERROR",
        }
    }

    pub fn is_rate_limited(&self) -> bool {
        matches!(self, AgentError::RateLimited(_))
    }

    pub fn is_auth_required(&self) -> bool {
        matches!(self, AgentError::AuthRequired(_))
    }
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub internal_session_id: String,
    pub workspace_root: PathBuf,
    pub resume_external_session_id: Option<String>,
    pub model_name: Option<String>,
    /// Explicitly opted-in API key from OS Keychain (ONLY when auth_mode == "api_key_keyring").
    pub explicit_api_key: Option<String>,
    pub deny_shell_by_default: bool,
}

#[derive(Debug, Clone)]
pub struct AgentSession {
    pub session_id: String,
    pub external_session_id: Option<String>,
    pub resumed: bool,
    pub resumable: bool,
}

#[derive(Debug, Clone)]
pub struct AgentImageInput {
    pub filename: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct AgentInput {
    pub run_id: String,
    pub message_id: Option<String>,
    pub prompt_text: String,
    pub images: Vec<AgentImageInput>,
    pub model_override: Option<String>,
    /// Optional JSON Schema to constrain structured output (supported by Codex `outputSchema`).
    pub output_schema: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    AllowOnce,
    Deny,
}

pub fn make_envelope(
    run_id: &str,
    session_id: &str,
    message_id: Option<&str>,
    event: AgentEventPayload,
) -> AgentEventEnvelope {
    AgentEventEnvelope {
        event_id: Uuid::new_v4().to_string(),
        run_id: run_id.to_string(),
        session_id: session_id.to_string(),
        message_id: message_id.map(ToString::to_string),
        timestamp: Utc::now().to_rfc3339(),
        event,
    }
}

/// Common provider-agnostic AI harness interface (docs/agent-integration.md Section 3).
/// All provider-specific protocol details live strictly inside `agents/providers/`.
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    async fn check_availability(&self) -> Result<AgentStatus, AgentError>;

    async fn capabilities(&self) -> Result<AgentCapabilities, AgentError>;

    async fn create_session(&self, config: SessionConfig) -> Result<AgentSession, AgentError>;

    async fn prompt(
        &self,
        session_id: &str,
        input: AgentInput,
    ) -> Result<AgentEventStream, AgentError>;

    async fn respond_permission(
        &self,
        session_id: &str,
        request_id: &str,
        decision: PermissionDecision,
    ) -> Result<(), AgentError>;

    async fn cancel(&self, session_id: &str) -> Result<(), AgentError>;

    async fn close_session(&self, session_id: &str) -> Result<(), AgentError>;
}
