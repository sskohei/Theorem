use std::collections::HashMap;
use std::path::PathBuf;
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
use crate::science_mcp::{SCIENCE_MCP_SERVER_NAME, ScopedMcpServerConfig};

struct MockSessionState {
    external_id: String,
    workspace_root: PathBuf,
    mcp_servers: Vec<ScopedMcpServerConfig>,
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
            workspace_root: config.workspace_root,
            mcp_servers: config.mcp_servers,
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

            // Detect structured problem recognition vs partial revision vs solution generation vs normal chat
            let response_text = if prompt_text.contains("解答部分修正エンジン") {
                // Extract the current structured JSON from the revision prompt and apply a clean deterministic partial edit
                let base_json_opt = prompt_text
                    .split("【現行の構造化解答 JSON（修正前）】")
                    .nth(1)
                    .and_then(|rest| rest.split("【ユーザーからの部分修正指示】").next())
                    .and_then(|chunk| crate::prompts::extract_json_value(chunk).ok());
                let instruction_snip = prompt_text
                    .split("【ユーザーからの部分修正指示】")
                    .nth(1)
                    .map(str::trim)
                    .unwrap_or("途中式を補強");

                let mut proposed = base_json_opt.unwrap_or_else(|| {
                    json!({
                        "schema_version": 1,
                        "subject": "mathematics",
                        "field": "calculus",
                        "title": "部分修正提案解答",
                        "mode": "detailed",
                        "steps": [{ "type": "explanation", "text": "元のステップ" }],
                        "answer": { "text": "結果", "latex": "I = \\frac{\\sqrt{\\pi}}{2}" }
                    })
                });
                if let Some(obj) = proposed.as_object_mut() {
                    let steps = obj
                        .entry("steps")
                        .or_insert_with(|| json!([]))
                        .as_array_mut();
                    if let Some(arr) = steps {
                        arr.push(json!({
                            "type": "explanation",
                            "heading": "部分修正による補足",
                            "text": format!("修正指示（{}）に基づき、ヤコビアンおよび積分領域の厳密性を補足した。", instruction_snip),
                            "latex": "\\left|\\det \\frac{\\partial(x,y)}{\\partial(r,\\theta)}\\right| = r \\ge 0",
                            "display": true
                        }));
                    }
                    if let Some(ans) = obj.get_mut("answer").and_then(|v| v.as_object_mut()) {
                        ans.insert(
                            "text".to_string(),
                            json!("部分修正済み：極座標変換のヤコビアン確認を含め厳密に得られた。"),
                        );
                    }
                }
                proposed.to_string()
            } else if prompt_text.contains("問題文認識エンジン") {
                if prompt_text.contains("[TEST_INVALID_JSON]") {
                    "not valid json".to_string()
                } else if prompt_text.contains("手書き答案認識モード") {
                    json!({
                        "schema_version": 1,
                        "problems": [
                            {
                                "problem_number": "答案1",
                                "subproblem_number": null,
                                "subject_category": "mathematics",
                                "field": "calculus",
                                "statement_markdown": "【問題】$\\int_0^1 x^2\\,dx$ を求めよ。\n\n【手書き答案書き起こし】$\\int_0^1 x^2\\,dx = \\left[\\frac{x^2}{2}\\right]_0^1 = \\frac{1}{2}$",
                                "given_conditions": ["手書き答案を未修正のまま書き起こし"],
                                "target_objective": "ユーザー答案の添削対象",
                                "source_pages": [1],
                                "figure_refs": [],
                                "uncertainties": ["3行目の指数文字がやや薄い（2 または 3 を要確認）"]
                            }
                        ]
                    })
                    .to_string()
                } else {
                    let pages = if cfg!(test) && prompt_text.contains("[TEST_REPEATED_PROBLEM]") {
                        [1, 2]
                            .into_iter()
                            .filter(|page| {
                                prompt_text.contains(&format!("[TEST_PAGE_{page}]"))
                                    || prompt_text.contains(&format!("{page}〜{page} ページ"))
                            })
                            .collect::<Vec<_>>()
                    } else {
                        vec![1]
                    };
                    let problems: Vec<_> = pages.into_iter().map(|page| json!({
                        "problem_number": "第1問",
                        "subproblem_number": "(1)",
                        "subject_category": "mathematics",
                        "field": "calculus",
                        "statement_markdown": "広義積分 $I = \\int_0^{\\infty} e^{-x^2}\\,dx$ の値を求めよ。",
                        "given_conditions": ["$x \\in [0, \\infty)$"],
                        "target_objective": "ガウス積分 $I$ の厳密値",
                        "source_pages": [page],
                        "figure_refs": [],
                        "uncertainties": if image_count == 0 {
                            vec!["添付画像なしのため補助テキストのみで認識しました".to_string()]
                        } else {
                            Vec::<String>::new()
                        }
                    })).collect();
                    json!({"schema_version": 1, "problems": problems}).to_string()
                }
            } else if prompt_text.contains("Theorem Solution Schema v1") {
                if prompt_text.contains("【生成モード】review") {
                    json!({
                        "schema_version": 1,
                        "subject": "mathematics",
                        "field": "calculus",
                        "title": "ユーザー答案の添削と正しい導出",
                        "mode": "review",
                        "assumptions": ["ユーザー答案の誤りを特定し、正しい導出を提示する"],
                        "steps": [
                            {
                                "type": "explanation",
                                "heading": "1. ユーザー答案の診断",
                                "text": "積分区間の設定は正しいが、原始関数を求める際にべき乗の次数を 1 増やす操作が抜けており $x^2/2$ となっている点が誤りです。"
                            },
                            {
                                "type": "equation",
                                "heading": "2. 正しい導出",
                                "text": "正しくは $\\int x^n\\,dx = \\frac{x^{n+1}}{n+1}$ を適用する。",
                                "latex": "\\int_0^1 x^2\\,dx = \\left[ \\frac{x^3}{3} \\right]_0^1 = \\frac{1}{3}",
                                "display": true
                            }
                        ],
                        "answer": {
                            "text": "誤答 $\\frac{1}{2}$ を修正し、正答は $\\frac{1}{3}$。",
                            "latex": "\\frac{1}{3}",
                            "unit": null
                        },
                        "verification": [
                            {
                                "source_kind": "ai_self_check",
                                "target": "原始関数の微分検算",
                                "method": "d/dx (x^3/3) = x^2",
                                "status": "passed",
                                "note": "検算一致"
                            }
                        ],
                        "artifacts": [],
                        "warnings": ["指数法則の適用ミスに注意してください。"]
                    })
                    .to_string()
                } else if prompt_text.contains("【生成モード】beginner") {
                    json!({
                        "schema_version": 1,
                        "subject": "mathematics",
                        "field": "calculus",
                        "title": "初学者向けガウス積分ステップ解説",
                        "mode": "beginner",
                        "assumptions": ["極座標の微小面積要素 dA = r dr dθ を確認する"],
                        "steps": [
                            {
                                "type": "explanation",
                                "heading": "1. なぜ 2 乗するのか（直感的イメージ）",
                                "text": "1 変数のままでは初等関数で不定積分が表せないため、2 変数の円対称性を使って極座標で解きます。"
                            },
                            {
                                "type": "equation",
                                "latex": "I^2 = \\int_0^{\\pi/2} d\\theta \\int_0^{\\infty} r e^{-r^2}\\,dr = \\frac{\\pi}{4}",
                                "display": true
                            }
                        ],
                        "answer": { "text": "正の平方根をとると次を得る。", "latex": "I = \\frac{\\sqrt{\\pi}}{2}" },
                        "verification": [],
                        "artifacts": [],
                        "warnings": ["ヤコビアン r を掛け忘れるミスが多いので注意してください。"]
                    })
                    .to_string()
                } else if prompt_text.contains("【生成モード】concise") {
                    json!({
                        "schema_version": 1,
                        "subject": "mathematics",
                        "field": "calculus",
                        "title": "ガウス積分（簡潔模範解答）",
                        "mode": "concise",
                        "steps": [
                            {
                                "type": "equation",
                                "latex": "I^2 = \\int_0^{\\pi/2} d\\theta \\int_0^{\\infty} r e^{-r^2}\\,dr = \\frac{\\pi}{4} \\implies I = \\frac{\\sqrt{\\pi}}{2}",
                                "display": true
                            }
                        ],
                        "answer": { "latex": "I = \\frac{\\sqrt{\\pi}}{2}" },
                        "verification": [],
                        "artifacts": [],
                        "warnings": []
                    })
                    .to_string()
                } else if prompt_text.contains("[TEST_BAD_LATEX]") {
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
                } else if prompt_text.contains("[TEST_SPOOF_SCIENCE_MCP]") {
                    // Intentionally spoof `source_kind: "science_mcp"` WITHOUT invoking any tool;
                    // Rust core MUST downgrade this to `ai_self_check`!
                    json!({
                        "schema_version": 1,
                        "subject": "mathematics",
                        "field": "calculus",
                        "title": "検証ソース詐称防御テスト",
                        "mode": "verified",
                        "steps": [
                            { "type": "equation", "latex": "x^2 - 1 = (x-1)(x+1)", "display": true }
                        ],
                        "answer": { "latex": "x = \\pm 1" },
                        "verification": [
                            {
                                "source_kind": "science_mcp",
                                "target": "ツール未実行の偽装検証",
                                "method": "AI 自己申告",
                                "status": "passed",
                                "note": "ツール実行なしで science_mcp を名乗るケース"
                            }
                        ],
                        "artifacts": [],
                        "warnings": []
                    })
                    .to_string()
                } else if prompt_text.contains("【生成モード】verified")
                    || prompt_text.contains("subject=physics")
                    || prompt_text.contains("[TEST_SCIENCE_MCP]")
                {
                    let has_science_mcp = sess
                        .mcp_servers
                        .iter()
                        .any(|s| s.name == SCIENCE_MCP_SERVER_NAME);

                    let mut artifact_rel_opt: Option<String> = None;
                    if has_science_mcp {
                        // 1. Emit check_dimensions tool call
                        let call_dim = format!("mock-call-dim-{}", Uuid::new_v4());
                        let _ = tx.send(make_envelope(
                            &run_id,
                            &sess_id_owned,
                            message_id.as_deref(),
                            AgentEventPayload::ToolCallStarted {
                                call_id: call_dim.clone(),
                                server: Some(SCIENCE_MCP_SERVER_NAME.to_string()),
                                tool_name: "check_dimensions".to_string(),
                                arguments: json!({
                                    "lhs_expression": "m * a",
                                    "rhs_expression": "k * x",
                                    "unit_map": { "m": "kg", "a": "m/s**2", "k": "N/m", "x": "m" }
                                }),
                            },
                        ));
                        let _ = tx.send(make_envelope(
                            &run_id,
                            &sess_id_owned,
                            message_id.as_deref(),
                            AgentEventPayload::ToolCallUpdated {
                                call_id: call_dim.clone(),
                                status: "running".to_string(),
                                progress: Some("Pint 次元解析を実行中".to_string()),
                            },
                        ));
                        let _ = tx.send(make_envelope(
                            &run_id,
                            &sess_id_owned,
                            message_id.as_deref(),
                            AgentEventPayload::ToolCallCompleted {
                                call_id: call_dim,
                                succeeded: true,
                                output: json!({
                                    "ok": true,
                                    "tool": "check_dimensions",
                                    "result": {
                                        "consistent": true,
                                        "status": "passed",
                                        "lhs_dimension": "[M][L][T]^-2",
                                        "rhs_dimension": "[M][L][T]^-2"
                                    },
                                    "verification": {
                                        "status": "passed",
                                        "target": "運動方程式の次元整合性: m * a = k * x",
                                        "assumptions": ["[m] = kg", "[a] = m/s**2", "[k] = N/m", "[x] = m"],
                                        "method": "pint.dimensionality 一致検証"
                                    },
                                    "artifacts": [],
                                    "warnings": [],
                                    "duration_ms": 18
                                }),
                            },
                        ));

                        // 2. Emit plot_function tool call and write real vector SVG into workspace artifacts/
                        let plot_filename = format!("{}.svg", Uuid::new_v4());
                        let plot_rel = format!("artifacts/{plot_filename}");
                        let artifacts_dir = sess.workspace_root.join("artifacts");
                        let _ = std::fs::create_dir_all(&artifacts_dir);
                        let svg_content = mock_damped_oscillation_svg();
                        if std::fs::write(
                            artifacts_dir.join(&plot_filename),
                            svg_content.as_bytes(),
                        )
                        .is_ok()
                        {
                            artifact_rel_opt = Some(plot_rel.clone());
                            let call_plot = format!("mock-call-plot-{}", Uuid::new_v4());
                            let _ = tx.send(make_envelope(
                                &run_id,
                                &sess_id_owned,
                                message_id.as_deref(),
                                AgentEventPayload::ToolCallStarted {
                                    call_id: call_plot.clone(),
                                    server: Some(SCIENCE_MCP_SERVER_NAME.to_string()),
                                    tool_name: "plot_function".to_string(),
                                    arguments: json!({
                                        "expressions": ["exp(-t)*(cos(2*t) + sin(2*t)/2)"],
                                        "variable": "t",
                                        "x_range": [0, 6],
                                        "output_format": "svg"
                                    }),
                                },
                            ));
                            let _ = tx.send(make_envelope(
                                &run_id,
                                &sess_id_owned,
                                message_id.as_deref(),
                                AgentEventPayload::ToolCallCompleted {
                                    call_id: call_plot,
                                    succeeded: true,
                                    output: json!({
                                        "ok": true,
                                        "tool": "plot_function",
                                        "result": {
                                            "variable": "t",
                                            "x_range": [0, 6],
                                            "artifact_path": plot_rel,
                                            "format": "svg"
                                        },
                                        "verification": {
                                            "status": "passed",
                                            "target": "減衰振動の関数グラフ生成 (SVG)",
                                            "assumptions": ["t in [0, 6]"],
                                            "method": "Matplotlib Agg 非対話描画"
                                        },
                                        "artifacts": [
                                            {
                                                "relative_path": plot_rel,
                                                "filename": plot_filename,
                                                "mime_type": "image/svg+xml",
                                                "format": "svg",
                                                "caption": "減衰振動の変位 x(t) グラフ"
                                            }
                                        ],
                                        "warnings": [],
                                        "duration_ms": 45
                                    }),
                                },
                            ));
                            let _ = tx.send(make_envelope(
                                &run_id,
                                &sess_id_owned,
                                message_id.as_deref(),
                                AgentEventPayload::ArtifactCreated {
                                    relative_path: plot_rel,
                                    mime_type: "image/svg+xml".to_string(),
                                    caption: Some("減衰振動の変位 x(t) グラフ".to_string()),
                                },
                            ));
                        }
                    }

                    let artifacts_val = match artifact_rel_opt {
                        Some(rel) => json!([
                            {
                                "relative_path": rel,
                                "caption": "減衰振動の変位 x(t) グラフ",
                                "artifact_type": "plot"
                            }
                        ]),
                        None => json!([]),
                    };

                    json!({
                        "schema_version": 1,
                        "subject": "physics",
                        "field": "classical_mechanics",
                        "title": "減衰調和振動子の運動方程式と応答グラフ",
                        "mode": "verified",
                        "assumptions": ["質量 m > 0、減衰係数・ばね定数は正の定数とする"],
                        "coordinate_system": "つり合い位置を原点とし変位 x(t) を正方向にとる",
                        "given_quantities": [
                            { "symbol": "m", "description": "物体の質量", "unit": "kg" },
                            { "symbol": "k", "description": "ばね定数", "unit": "N/m" },
                            { "symbol": "t", "description": "時刻", "unit": "s" }
                        ],
                        "principles": ["ニュートンの運動方程式", "線形定数係数 2 階常微分方程式"],
                        "steps": [
                            {
                                "type": "explanation",
                                "heading": "1. 運動方程式の立式",
                                "text": "減衰項を含む運動方程式を整理する。"
                            },
                            {
                                "type": "equation",
                                "latex": "\\frac{d^2 x}{dt^2} + 2 \\frac{dx}{dt} + 5 x = 0",
                                "display": true
                            },
                            {
                                "type": "explanation",
                                "heading": "2. 初期条件 x(0) = 1, x'(0) = 0 の適用",
                                "text": "特性方程式 r^2 + 2r + 5 = 0 の根 r = -1 \\pm 2i より一般解を得て初期条件を代入する。"
                            },
                            {
                                "type": "equation",
                                "latex": "x(t) = e^{-t} \\left( \\cos(2t) + \\frac{1}{2} \\sin(2t) \\right)",
                                "display": true
                            }
                        ],
                        "answer": {
                            "text": "初期条件を満たす減衰振動の変位は次式で与えられる。",
                            "latex": "x(t) = e^{-t} \\left( \\cos(2t) + \\frac{1}{2} \\sin(2t) \\right)",
                            "unit": "m"
                        },
                        "dimensional_check": {
                            "note": "運動方程式の両辺は力 [M][L][T]^{-2} の次元で一致する。",
                            "latex": "[m a] = [k x] = \\mathrm{M L T^{-2}}"
                        },
                        "limiting_cases": ["t \\to \\infty で x(t) \\to 0 に収束する"],
                        "verification": [
                            {
                                "source_kind": "ai_self_check",
                                "target": "初期条件 x(0)=1 の確認",
                                "method": "t=0 代入確認",
                                "status": "passed",
                                "note": "e^0 (cos 0 + 0) = 1"
                            }
                        ],
                        "artifacts": artifacts_val,
                        "warnings": []
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
                if prompt_text.contains("[TEST_TOOL_CALL]") {
                    let call_id = format!("mock-chat-tool-{}", Uuid::new_v4());
                    let _ = tx.send(make_envelope(
                        &run_id,
                        &sess_id_owned,
                        message_id.as_deref(),
                        AgentEventPayload::ToolCallStarted {
                            call_id: call_id.clone(),
                            server: Some(SCIENCE_MCP_SERVER_NAME.to_string()),
                            tool_name: "differentiate".to_string(),
                            arguments: json!({ "expression": "x**3", "variable": "x", "order": 1 }),
                        },
                    ));
                    let _ = tx.send(make_envelope(
                        &run_id,
                        &sess_id_owned,
                        message_id.as_deref(),
                        AgentEventPayload::ToolCallCompleted {
                            call_id,
                            succeeded: true,
                            output: json!({
                                "ok": true,
                                "tool": "differentiate",
                                "result": { "expression": "3*x**2", "latex": "3 x^{2}" },
                                "verification": {
                                    "status": "passed",
                                    "target": "1 階微分 d/dx (x**3)",
                                    "assumptions": [],
                                    "method": "sympy.diff(expr, x, 1)"
                                },
                                "artifacts": [],
                                "warnings": [],
                                "duration_ms": 12
                            }),
                        },
                    ));
                }
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

fn mock_damped_oscillation_svg() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="480" height="240" viewBox="0 0 480 240">
  <rect width="480" height="240" fill="#ffffff"/>
  <line x1="40" y1="120" x2="460" y2="120" stroke="#94a3b8" stroke-width="1"/>
  <line x1="40" y1="20" x2="40" y2="220" stroke="#94a3b8" stroke-width="1"/>
  <polyline fill="none" stroke="#2563eb" stroke-width="2.2"
    points="40,40 75,78 110,148 145,146 180,108 215,109 250,125 285,124 320,118 355,119 390,120 440,120"/>
  <text x="240" y="22" font-size="12" text-anchor="middle" fill="#1e293b">x(t) = exp(-t)(cos 2t + 0.5 sin 2t)</text>
</svg>"##
}
