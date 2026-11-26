"""Tests for isolated worker subprocess timeout enforcement and termination."""

from __future__ import annotations

from pathlib import Path

from science_mcp.runner import execute_tool_isolated


def test_heavy_symbolic_calculation_triggers_hard_timeout(tmp_path: Path) -> None:
    # Expanding a dense 6-variable polynomial to a high power reliably exceeds 0.6s
    res = execute_tool_isolated(
        "expand_expression",
        {"expression": "(x + y + z + u + v + w + 1)**14"},
        artifacts_dir=tmp_path / "artifacts",
        timeout_seconds=0.6,
    )
    assert res["ok"] is False
    assert res["error"] is not None
    assert res["error"]["code"] == "TIMEOUT"
    assert res["verification"]["status"] == "error"


def test_runner_returns_structured_error_for_rce_payload(tmp_path: Path) -> None:
    res = execute_tool_isolated(
        "differentiate",
        {"expression": "__import__('os').system('echo pwn')", "variable": "x"},
        artifacts_dir=tmp_path / "artifacts",
        timeout_seconds=10.0,
    )
    assert res["ok"] is False
    assert res["error"]["code"] == "UNSAFE_EXPRESSION"
    assert res["verification"]["status"] == "error"
