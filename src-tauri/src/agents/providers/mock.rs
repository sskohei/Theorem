use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::json;
use tokio::sync::{Mutex, mpsc, oneshot};
use uuid::Uuid;

use crate::agents::adapter::{
    AgentAdapter, AgentError, AgentEventStream, AgentInput, AgentSession, PermissionDecision,
    SessionConfig, make_envelope,
};
use crate::domain::{AgentCapabilities, AgentEventPayload, AgentModelInfo, AgentStatus};

struct MockSessionState {
    external_id: String,
    cancelled: Arc<Mutex<bool>>,
    pending_permissions: Arc<Mutex<HashMap<String, oneshot::Sender<PermissionDecision>>>>,
}

/// Deterministic in-process MockAdapter for CI / automated tests and offline development.
/// Clearly labeled `verified_mock` so it is NEVER confused with a real external AI harness.
pub struct MockAdapter {
    sessions: Arc<Mutex<HashMap<String, Arc<MockSessionState>>>>,
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAdapter {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn caps() -> AgentCapabilities {
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
}

#[async_trait]
impl AgentAdapter for MockAdapter {
    async fn check_availability(&self) -> Result<AgentStatus, AgentError> {
        Ok(AgentStatus {
            provider_kind: "mock".to_string(),
            installed: true,
            resolved_path: Some("in-process://mock-adapter".to_string()),
            version: Some("mock-1.0.0".to_string()),
            auth_status: "authenticated".to_string(),
            auth_method: Some("mock_local".to_string()),
            verification_status: "verified_mock".to_string(),
            available_models: Some(vec![
                AgentModelInfo {
                    id: "mock-math-pro".to_string(),
                    label: "Mock Math/Physics Pro (テスト用)".to_string(),
                },
                AgentModelInfo {
                    id: "mock-fast".to_string(),
                    label: "Mock Fast (テスト用)".to_string(),
                },
            ]),
            capabilities: Self::caps(),
            auth_guide: "テスト・オフライン検証用のモックアダプターです（認証不要）。".to_string(),
            compliance_note:
                "外部通信を行いません。モック動作は実 AI ハーネス接続の確認とは明確に区別されます。"
                    .to_string(),
            detail_note: Some("CI 自動テストおよび UI 検証用モック。".into()),
        })
    }

    async fn capabilities(&self) -> Result<AgentCapabilities, AgentError> {
        Ok(Self::caps())
    }

    async fn create_session(&self, config: SessionConfig) -> Result<AgentSession, AgentError> {
        let resumed = config.resume_external_session_id.is_some();
        let ext_id = config
            .resume_external_session_id
            .unwrap_or_else(|| format!("mock-ext-{}", Uuid::new_v4()));

        let state = Arc::new(MockSessionState {
            external_id: ext_id.clone(),
            cancelled: Arc::new(Mutex::new(false)),
            pending_permissions: Arc::new(Mutex::new(HashMap::new())),
        });
        self.sessions
            .lock()
            .await
            .insert(config.internal_session_id.clone(), state);

        Ok(AgentSession {
            session_id: config.internal_session_id,
            external_session_id: Some(ext_id),
            resumed,
            resumable: true,
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

        *sess.cancelled.lock().await = false;

        let (tx, rx) = mpsc::unbounded_channel();
        let sess_id_owned = session_id.to_string();
        let run_id = input.run_id.clone();
        let message_id = input.message_id.clone();
        let prompt_text = input.prompt_text.clone();
        let image_count = input.images.len();

        tokio::spawn(async move {
            let _ = tx.send(make_envelope(
                &run_id,
                &sess_id_owned,
                message_id.as_deref(),
                AgentEventPayload::SessionStarted {
                    external_session_id: Some(sess.external_id.clone()),
                    resumed: false,
                },
            ));
            let _ = tx.send(make_envelope(
                &run_id,
                &sess_id_owned,
                message_id.as_deref(),
                AgentEventPayload::MessageStarted {
                    role: "assistant".to_string(),
                },
            ));

            if prompt_text.contains("[TEST_RATE_LIMIT]") {
                let _ = tx.send(make_envelope(
                    &run_id,
                    &sess_id_owned,
                    message_id.as_deref(),
                    AgentEventPayload::Failed {
                        code: "RATE_LIMITED".to_string(),
                        message: "利用上限（429 Rate Limit）に達しました。しばらく待ってから再試行してください。".to_string(),
                        rate_limited: true,
                        auth_required: false,
                    },
                ));
                return;
            }

            if prompt_text.contains("[TEST_AUTH_ERROR]") {
                let _ = tx.send(make_envelope(
                    &run_id,
                    &sess_id_owned,
                    message_id.as_deref(),
                    AgentEventPayload::Failed {
                        code: "AUTH_REQUIRED".to_string(),
                        message: "公式ハーネスの認証が必要です。".to_string(),
                        rate_limited: false,
                        auth_required: true,
                    },
                ));
                return;
            }

            if prompt_text.contains("[TEST_PERMISSION]") {
                let req_id = Uuid::new_v4().to_string();
                let (ptx, prx) = oneshot::channel();
                sess.pending_permissions
                    .lock()
                    .await
                    .insert(req_id.clone(), ptx);
                let _ = tx.send(make_envelope(
                    &run_id,
                    &sess_id_owned,
                    message_id.as_deref(),
                    AgentEventPayload::PermissionRequested {
                        request_id: req_id.clone(),
                        action: "shell_command".to_string(),
                        target: Some("echo test".to_string()),
                        detail: "任意シェルコマンド実行の許可要求（デフォルト拒否推奨）"
                            .to_string(),
                    },
                ));
                let decision = match tokio::time::timeout(Duration::from_secs(30), prx).await {
                    Ok(Ok(d)) => d,
                    _ => {
                        sess.pending_permissions.lock().await.remove(&req_id);
                        PermissionDecision::Deny
                    }
                };
                let reply = match decision {
                    PermissionDecision::AllowOnce => "ユーザーが一回許可しました（モック実行）。",
                    PermissionDecision::Deny => {
                        "セキュリティポリシーによりコマンド実行は拒否されました。"
                    }
                };
                let _ = tx.send(make_envelope(
                    &run_id,
                    &sess_id_owned,
                    message_id.as_deref(),
                    AgentEventPayload::TextDelta {
                        text: reply.to_string(),
                    },
                ));
                let _ = tx.send(make_envelope(
                    &run_id,
                    &sess_id_owned,
                    message_id.as_deref(),
                    AgentEventPayload::MessageCompleted {
                        final_text: Some(reply.to_string()),
                    },
                ));
                return;
            }

            // Detect structured problem recognition vs solution generation vs normal chat
            let response_text = if prompt_text.contains("問題文認識エンジン") {
                if prompt_text.contains("[TEST_INVALID_JSON]") {
                    "not valid json".to_string()
                } else {
                    json!({
                        "schema_version": 1,
                        "problems": [
                            {
                                "problem_number": "第1問",
                                "subproblem_number": "(1)",
                                "subject_category": "mathematics",
                                "field": "calculus",
                                "statement_markdown": "広義積分 $I = \\int_0^{\\infty} e^{-x^2}\\,dx$ の値を求めよ。",
                                "given_conditions": ["$x \\in [0, \\infty)$"],
                                "target_objective": "ガウス積分 $I$ の厳密値",
                                "source_pages": [1],
                                "figure_refs": [],
                                "uncertainties": if image_count == 0 {
                                    vec!["添付画像なしのため補助テキストのみで認識しました".to_string()]
                                } else {
                                    Vec::<String>::new()
                                }
                            }
                        ]
                    })
                    .to_string()
                }
            } else if prompt_text.contains("Theorem Solution Schema v1") {
                if prompt_text.contains("[TEST_BAD_LATEX]") {
                    json!({
                        "schema_version": 1,
                        "subject": "mathematics",
                        "field": "calculus",
                        "title": "LaTeX エラー検証用解答",
                        "mode": "standard",
                        "steps": [
                            { "type": "equation", "latex": "\\totallyUnknownCustomMacro{x}", "display": true }
                        ],
                        "answer": { "latex": "x = 1" }
                    })
                    .to_string()
                } else {
                    json!({
                        "schema_version": 1,
                        "subject": "mathematics",
                        "field": "calculus",
                        "title": "ガウス積分の計算と収束の証明",
                        "mode": "detailed",
                        "assumptions": ["$x$ は実数とする"],
                        "definitions": [
                            {
                                "name": "広義積分",
                                "latex": "I = \\lim_{R \\to \\infty} \\int_0^R e^{-x^2}\\,dx",
                                "explanation": "区間 $[0, \\infty)$ 上の非負連続関数に対する極限として定義する。"
                            }
                        ],
                        "theorems": [
                            {
                                "name": "トネリ・フビニの定理",
                                "conditions": "被積分関数 $e^{-(x^2+y^2)} > 0$ は非負可測",
                                "application": "重積分の順序交換および極座標変換を正当化する。"
                            }
                        ],
                        "domain_and_boundary": "積分領域は第1象限 $D = \\{(x,y) \\mid x \\ge 0, y \\ge 0\\}$",
                        "steps": [
                            {
                                "type": "explanation",
                                "heading": "1. 二重積分への帰着",
                                "text": "ガウス積分 $I$ の 2 乗を考え、第 1 象限上の二重積分として表す。"
                            },
                            {
                                "type": "equation",
                                "latex": "I^2 = \\left(\\int_0^{\\infty} e^{-x^2}\\,dx\\right)\\left(\\int_0^{\\infty} e^{-y^2}\\,dy\\right) = \\iint_{x\\ge 0, y\\ge 0} e^{-(x^2+y^2)}\\,dx\\,dy",
                                "display": true
                            },
                            {
                                "type": "explanation",
                                "heading": "2. 極座標変換",
                                "text": "極座標 $x = r\\cos\\theta, y = r\\sin\\theta$（ヤコビアン $r$）を用いる。"
                            },
                            {
                                "type": "equation",
                                "latex": "I^2 = \\int_0^{\\pi/2} d\\theta \\int_0^{\\infty} r e^{-r^2}\\,dr = \\frac{\\pi}{2} \\left[ -\\frac{1}{2} e^{-r^2} \\right]_0^{\\infty} = \\frac{\\pi}{4}",
                                "display": true
                            }
                        ],
                        "proof": [
                            {
                                "kind": "conclusion",
                                "text": "被積分関数は正であるため $I > 0$ であり、正の平方根をとることで次を得る。",
                                "latex": "I = \\frac{\\sqrt{\\pi}}{2}"
                            }
                        ],
                        "answer": {
                            "text": "ガウス積分の値は次の通りである。",
                            "latex": "I = \\int_0^{\\infty} e^{-x^2}\\,dx = \\frac{\\sqrt{\\pi}}{2}"
                        },
                        "verification": [
                            {
                                "source_kind": "ai_self_check",
                                "target": "符号・数値オーダー確認",
                                "method": "被積分関数の正値性と近似値確認",
                                "status": "passed",
                                "note": "sqrt(pi)/2 ≈ 0.8862 > 0"
                            }
                        ],
                        "artifacts": [],
                        "warnings": []
                    })
                    .to_string()
                }
            } else {
                "ご質問ありがとうございます（Mock 応答）。\n\n数式例: $\\int_0^\\infty e^{-x^2}\\,dx = \\frac{\\sqrt{\\pi}}{2}$\n\n極座標変換 $x = r\\cos\\theta,\\; y = r\\sin\\theta$ を使うとヤコビアン $r$ が現れるため、$r e^{-r^2}$ が初等的に積分できます。".to_string()
            };

            let slow = prompt_text.contains("[TEST_SLOW]");
            let chunks: Vec<String> = if slow {
                (0..30)
                    .map(|i| {
                        format!("ステップ {i}: $\\sum_{{k=1}}^{{{i}}} k$ の計算を進めています。\n")
                    })
                    .collect()
            } else {
                let chars: Vec<char> = response_text.chars().collect();
                chars
                    .chunks(32)
                    .map(|slice| slice.iter().collect::<String>())
                    .collect()
            };

            let mut accumulated = String::new();
            for chunk in chunks {
                if *sess.cancelled.lock().await {
                    let _ = tx.send(make_envelope(
                        &run_id,
                        &sess_id_owned,
                        message_id.as_deref(),
                        AgentEventPayload::Cancelled {
                            reason: Some("ユーザーにより中断されました".to_string()),
                        },
                    ));
                    return;
                }
                accumulated.push_str(&chunk);
                let _ = tx.send(make_envelope(
                    &run_id,
                    &sess_id_owned,
                    message_id.as_deref(),
                    AgentEventPayload::TextDelta { text: chunk },
                ));
                if slow {
                    tokio::time::sleep(Duration::from_millis(60)).await;
                }
            }

            let _ = tx.send(make_envelope(
                &run_id,
                &sess_id_owned,
                message_id.as_deref(),
                AgentEventPayload::UsageUpdated {
                    input_tokens: Some(120),
                    output_tokens: Some(accumulated.chars().count() as u64 / 3),
                    raw_label: Some("mock-usage".to_string()),
                },
            ));
            let _ = tx.send(make_envelope(
                &run_id,
                &sess_id_owned,
                message_id.as_deref(),
                AgentEventPayload::MessageCompleted {
                    final_text: Some(accumulated),
                },
            ));
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
        let sess = self
            .sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;
        if let Some(tx) = sess.pending_permissions.lock().await.remove(request_id) {
            let _ = tx.send(decision);
        }
        Ok(())
    }

    async fn cancel(&self, session_id: &str) -> Result<(), AgentError> {
        let sess = self
            .sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;
        *sess.cancelled.lock().await = true;
        for (_, tx) in sess.pending_permissions.lock().await.drain() {
            let _ = tx.send(PermissionDecision::Deny);
        }
        Ok(())
    }

    async fn close_session(&self, session_id: &str) -> Result<(), AgentError> {
        self.sessions.lock().await.remove(session_id);
        Ok(())
    }
}
