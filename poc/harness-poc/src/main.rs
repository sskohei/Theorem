mod acp;
mod codex;
mod jsonrpc;
mod sandbox;

use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let target = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "custom-acp".to_string());

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let base_tmp = manifest_dir.join("tmp");
    let mock_dir = manifest_dir
        .parent()
        .expect("poc dir")
        .join("mock-acp-agent");
    let mcp_script = mock_dir.join("dummy_science_mcp.py");
    let test_image = mock_dir.join("test_problem.png");

    if !test_image.exists() {
        eprintln!(
            "Missing test image {}. Run `python poc/mock-acp-agent/generate_test_image.py` first.",
            test_image.display()
        );
        std::process::exit(1);
    }

    match target.as_str() {
        "custom-acp" => {
            let agent_script = mock_dir.join("acp_echo_agent.py");
            let args = vec![agent_script.to_string_lossy().to_string()];
            match acp::run_acp_poc(
                "Custom ACP (mock-acp-agent)",
                "python",
                &args,
                &base_tmp,
                &mcp_script,
                &test_image,
                true,
            )
            .await
            {
                Ok(report) => println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                ),
                Err(err) => {
                    eprintln!("Custom ACP PoC failed: {err}");
                    std::process::exit(1);
                }
            }
        }
        "codex" => match codex::run_codex_poc(&base_tmp, &mcp_script, &test_image).await {
            Ok(report) => println!(
                "{}",
                serde_json::to_string_pretty(&report).unwrap_or_default()
            ),
            Err(err) => {
                eprintln!("Codex App Server PoC failed: {err}");
                std::process::exit(1);
            }
        },
        "antigravity" => {
            let args = vec![
                "acp".to_string(),
                "--stdio".to_string(),
                "--sandbox".to_string(),
                "--disable-telemetry".to_string(),
            ];
            match acp::run_acp_poc(
                "Google Antigravity (agy acp)",
                "agy",
                &args,
                &base_tmp,
                &mcp_script,
                &test_image,
                false,
            )
            .await
            {
                Ok(report) => println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                ),
                Err(err) => {
                    eprintln!("Antigravity ACP PoC failed: {err}");
                    std::process::exit(1);
                }
            }
        }
        other => {
            eprintln!("Usage: harness-poc [custom-acp|codex|antigravity] (got: {other})");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_custom_acp_end_to_end_without_external_credentials() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let base_tmp = std::env::temp_dir().join("formuladesk-harness-poc-test");
        let mock_dir = manifest_dir
            .parent()
            .expect("poc dir")
            .join("mock-acp-agent");
        let agent_script = mock_dir.join("acp_echo_agent.py");
        let mcp_script = mock_dir.join("dummy_science_mcp.py");
        let test_image = mock_dir.join("test_problem.png");

        assert!(
            test_image.exists(),
            "test_problem.png must exist for ACP image test"
        );

        let args = vec![agent_script.to_string_lossy().to_string()];
        let report = acp::run_acp_poc(
            "Custom ACP (mock-acp-agent)",
            "python",
            &args,
            &base_tmp,
            &mcp_script,
            &test_image,
            true,
        )
        .await
        .expect("Custom ACP PoC must succeed");

        assert!(report.startup_initialized);
        assert!(report.session_created);
        assert!(report.mcp_scoped_passed_in_session_new);
        assert!(report.streaming_chunk_count >= 1);
        assert!(report.image_input_verified);
        assert!(report.session_resume_verified);
        assert!(report.cancel_verified);
        assert!(report.permission_control_verified);
    }
}
