use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::agents::jsonrpc::JsonRpcProcess;
use crate::agents::sandbox::{probe_cli_version, resolve_cli_binary};
use crate::domain::{ScienceMcpSettings, ScienceMcpStatus, ScienceMcpToolInfo};
use crate::error::{AppError, AppResult};

pub const SCIENCE_MCP_SERVER_NAME: &str = "science_mcp";

/// Scoped MCP server launch configuration passed ONLY to a single session/child process.
/// NEVER modifies user global MCP config files (`~/.codex/config.toml`, `~/.claude.json`, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScopedMcpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Resolves the `uv` executable via manual setting, PATH, or well-known directories.
pub fn resolve_uv_binary(manual_path: Option<&str>) -> Option<PathBuf> {
    resolve_cli_binary(manual_path, "uv")
}

/// Resolves the local `science-mcp/` package directory containing `pyproject.toml`.
pub fn resolve_science_mcp_dir(manual_dir: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = manual_dir {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if p.join("pyproject.toml").is_file() {
                return Some(p);
            }
        }
    }

    if let Ok(env_dir) = std::env::var("THEOREM_SCIENCE_MCP_DIR") {
        let p = PathBuf::from(env_dir.trim());
        if p.join("pyproject.toml").is_file() {
            return Some(p);
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("science-mcp"));
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join("science-mcp"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut cur = exe.parent().map(Path::to_path_buf);
        for _ in 0..5 {
            if let Some(dir) = cur {
                candidates.push(dir.join("science-mcp"));
                cur = dir.parent().map(Path::to_path_buf);
            } else {
                break;
            }
        }
    }
    // Development workspace fallback relative to `src-tauri`
    let manifest_parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    candidates.push(manifest_parent.join("science-mcp"));

    for cand in candidates {
        if cand.join("pyproject.toml").is_file() {
            let resolved = std::fs::canonicalize(&cand).unwrap_or(cand);
            return Some(strip_windows_verbatim_prefix(resolved));
        }
    }
    None
}

fn strip_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    path
}

fn resolve_venv_python(server_dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    let cand = server_dir.join(".venv").join("Scripts").join("python.exe");
    #[cfg(not(windows))]
    let cand = server_dir.join(".venv").join("bin").join("python");

    if cand.is_file() { Some(cand) } else { None }
}

/// Builds a session-scoped `science_mcp` launch config confining plot output strictly
/// to `artifacts_dir` with `MPLBACKEND=Agg` and configured timeout/format.
pub fn build_scoped_mcp_config(
    settings: &ScienceMcpSettings,
    artifacts_dir: &Path,
) -> Option<ScopedMcpServerConfig> {
    if !settings.enabled {
        return None;
    }
    let server_dir = resolve_science_mcp_dir(settings.server_dir.as_deref())?;
    let timeout_sec = settings.timeout_seconds.clamp(2, 60);
    let default_fmt = if settings.default_plot_format.eq_ignore_ascii_case("png") {
        "png"
    } else {
        "svg"
    };
    let artifacts_str = artifacts_dir.to_string_lossy().into_owned();
    let env = vec![
        ("MPLBACKEND".to_string(), "Agg".to_string()),
        (
            "SCIENCE_MCP_ARTIFACTS_DIR".to_string(),
            artifacts_str.clone(),
        ),
        (
            "SCIENCE_MCP_TIMEOUT_SEC".to_string(),
            timeout_sec.to_string(),
        ),
        (
            "SCIENCE_MCP_DEFAULT_FORMAT".to_string(),
            default_fmt.to_string(),
        ),
    ];

    let has_manual_uv = settings
        .uv_path
        .as_deref()
        .is_some_and(|p| !p.trim().is_empty());

    // Prefer the pre-synced venv Python for fast offline startup when uv_path is not manually forced
    if !has_manual_uv && let Some(venv_py) = resolve_venv_python(&server_dir) {
        return Some(ScopedMcpServerConfig {
            name: SCIENCE_MCP_SERVER_NAME.to_string(),
            command: venv_py.to_string_lossy().into_owned(),
            args: vec![
                "-m".to_string(),
                "science_mcp.server".to_string(),
                "--artifacts-dir".to_string(),
                artifacts_str,
                "--timeout".to_string(),
                timeout_sec.to_string(),
                "--default-format".to_string(),
                default_fmt.to_string(),
            ],
            env,
        });
    }

    let uv_bin = resolve_uv_binary(settings.uv_path.as_deref())?;
    let mut args = vec![
        "--directory".to_string(),
        server_dir.to_string_lossy().into_owned(),
        "run".to_string(),
    ];
    if resolve_venv_python(&server_dir).is_some() {
        args.push("--no-sync".to_string());
    }
    args.extend([
        "science-mcp".to_string(),
        "--artifacts-dir".to_string(),
        artifacts_str,
        "--timeout".to_string(),
        timeout_sec.to_string(),
        "--default-format".to_string(),
        default_fmt.to_string(),
    ]);

    Some(ScopedMcpServerConfig {
        name: SCIENCE_MCP_SERVER_NAME.to_string(),
        command: uv_bin.to_string_lossy().into_owned(),
        args,
        env,
    })
}

async fn spawn_mcp_client(cfg: &ScopedMcpServerConfig, cwd: &Path) -> AppResult<JsonRpcProcess> {
    let env_refs: Vec<(&str, &str)> = cfg
        .env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let proc = JsonRpcProcess::spawn(Path::new(&cfg.command), &cfg.args, cwd, &env_refs)
        .map_err(|e| AppError::Agent(format!("Science MCP Server の起動に失敗しました: {e}")))?;

    proc.send_request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "theorem",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
        Duration::from_secs(15),
    )
    .await
    .map_err(|e| AppError::Agent(format!("Science MCP initialize ハンドシェイク失敗: {e}")))?;

    proc.send_notification("notifications/initialized", json!({}))
        .await
        .map_err(|e| AppError::Agent(format!("Science MCP initialized 通知失敗: {e}")))?;

    Ok(proc)
}

/// Probes `uv`, `science-mcp/` directory, and runs a live MCP `initialize` + `tools/list` check.
pub async fn check_science_mcp_status(
    settings: &ScienceMcpSettings,
    probe_artifacts_dir: &Path,
) -> ScienceMcpStatus {
    let security_note = "汎用 Python 実行ツールを持たず、AST ホワイトリスト数式パーサー・ワーカー隔離タイムアウト・固定 artifacts/ UUID 出力で保護されています。ユーザーのグローバル MCP 設定は変更しません。".to_string();

    let uv_bin = resolve_uv_binary(settings.uv_path.as_deref());
    let uv_version = match &uv_bin {
        Some(p) => probe_cli_version(p).await,
        None => None,
    };
    let server_dir = resolve_science_mcp_dir(settings.server_dir.as_deref());

    if !settings.enabled {
        return ScienceMcpStatus {
            enabled: false,
            uv_available: uv_bin.is_some(),
            uv_path: uv_bin.map(|p| p.to_string_lossy().into_owned()),
            uv_version,
            server_dir: server_dir.map(|p| p.to_string_lossy().into_owned()),
            server_ready: false,
            tool_count: 0,
            tools: Vec::new(),
            security_note,
            detail_note: Some("設定により Science MCP Server は無効化されています。".into()),
        };
    }

    let Some(cfg) = build_scoped_mcp_config(settings, probe_artifacts_dir) else {
        return ScienceMcpStatus {
            enabled: true,
            uv_available: uv_bin.is_some(),
            uv_path: uv_bin.map(|p| p.to_string_lossy().into_owned()),
            uv_version,
            server_dir: server_dir.map(|p| p.to_string_lossy().into_owned()),
            server_ready: false,
            tool_count: 0,
            tools: Vec::new(),
            security_note,
            detail_note: Some(
                "science-mcp パッケージまたは uv / Python 仮想環境が見つかりませんでした。`science-mcp` で `uv sync` を実行してください。"
                    .into(),
            ),
        };
    };

    let cwd = server_dir.clone().unwrap_or_else(std::env::temp_dir);
    match spawn_mcp_client(&cfg, &cwd).await {
        Ok(proc) => {
            let tools_res = proc
                .send_request("tools/list", json!({}), Duration::from_secs(15))
                .await;
            proc.shutdown().await;

            match tools_res {
                Ok(val) => {
                    let tools: Vec<ScienceMcpToolInfo> = val
                        .get("tools")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|item| {
                                    let name = item.get("name").and_then(Value::as_str)?;
                                    let desc = item
                                        .get("description")
                                        .and_then(Value::as_str)
                                        .unwrap_or("")
                                        .trim();
                                    Some(ScienceMcpToolInfo {
                                        name: name.to_string(),
                                        description: desc.to_string(),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    ScienceMcpStatus {
                        enabled: true,
                        uv_available: uv_bin.is_some(),
                        uv_path: uv_bin.map(|p| p.to_string_lossy().into_owned()),
                        uv_version,
                        server_dir: server_dir.map(|p| p.to_string_lossy().into_owned()),
                        server_ready: !tools.is_empty(),
                        tool_count: tools.len(),
                        tools,
                        security_note,
                        detail_note: None,
                    }
                }
                Err(err) => ScienceMcpStatus {
                    enabled: true,
                    uv_available: uv_bin.is_some(),
                    uv_path: uv_bin.map(|p| p.to_string_lossy().into_owned()),
                    uv_version,
                    server_dir: server_dir.map(|p| p.to_string_lossy().into_owned()),
                    server_ready: false,
                    tool_count: 0,
                    tools: Vec::new(),
                    security_note,
                    detail_note: Some(format!("tools/list 取得に失敗しました: {err}")),
                },
            }
        }
        Err(err) => ScienceMcpStatus {
            enabled: true,
            uv_available: uv_bin.is_some(),
            uv_path: uv_bin.map(|p| p.to_string_lossy().into_owned()),
            uv_version,
            server_dir: server_dir.map(|p| p.to_string_lossy().into_owned()),
            server_ready: false,
            tool_count: 0,
            tools: Vec::new(),
            security_note,
            detail_note: Some(err.to_string()),
        },
    }
}

/// Calls a single Science MCP tool directly over MCP stdio JSON-RPC with isolated artifacts output.
pub async fn call_science_mcp_tool(
    settings: &ScienceMcpSettings,
    artifacts_dir: &Path,
    tool_name: &str,
    arguments: Value,
) -> AppResult<Value> {
    if !settings.enabled {
        return Err(AppError::Validation(
            "Science MCP Server が設定で無効化されています。設定画面の「計算ツール」から有効にしてください。"
                .into(),
        ));
    }
    let cfg = build_scoped_mcp_config(settings, artifacts_dir).ok_or_else(|| {
        AppError::Agent(
            "Science MCP Server が利用できません。`science-mcp` 環境および `uv` パスを確認してください。"
                .into(),
        )
    })?;
    let cwd =
        resolve_science_mcp_dir(settings.server_dir.as_deref()).unwrap_or_else(std::env::temp_dir);

    let proc = spawn_mcp_client(&cfg, &cwd).await?;
    let rpc_timeout = Duration::from_secs(u64::from(settings.timeout_seconds.clamp(2, 60)) + 10);
    let raw_res = proc
        .send_request(
            "tools/call",
            json!({
                "name": tool_name,
                "arguments": arguments
            }),
            rpc_timeout,
        )
        .await;
    proc.shutdown().await;

    let val = raw_res.map_err(|e| AppError::Agent(format!("Science MCP 呼び出し失敗: {e}")))?;
    Ok(extract_mcp_tool_envelope(&val))
}

/// Normalizes MCP `tools/call` responses (both `structuredContent` and JSON `content[0].text`)
/// into the standardized Science MCP response envelope.
pub fn extract_mcp_tool_envelope(raw: &Value) -> Value {
    if let Some(sc) = raw.get("structuredContent") {
        if sc.get("ok").is_some() {
            return sc.clone();
        }
        if let Some(inner) = sc.get("result")
            && inner.get("ok").is_some()
        {
            return inner.clone();
        }
    }
    if let Some(arr) = raw.get("content").and_then(Value::as_array) {
        for block in arr {
            if let Some(text) = block.get("text").and_then(Value::as_str)
                && let Ok(parsed) = serde_json::from_str::<Value>(text)
            {
                return parsed;
            }
        }
    }
    raw.clone()
}
