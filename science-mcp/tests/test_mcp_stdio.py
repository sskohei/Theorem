"""End-to-end test over real MCP stdio transport verifying all 21 tools and security invariants."""

from __future__ import annotations

import asyncio
import json
import sys
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

EXPECTED_TOOLS: set[str] = {
    # 10 MVP priority tools
    "solve_equation",
    "differentiate",
    "integrate",
    "calculate_determinant",
    "calculate_inverse_matrix",
    "matrix_operation",
    "verify_equation",
    "convert_units",
    "check_dimensions",
    "plot_function",
    # 11 extended tools
    "simplify_expression",
    "expand_expression",
    "factor_expression",
    "calculate_limit",
    "solve_system",
    "solve_ode",
    "calculate_eigenvalues",
    "calculate_eigenvectors",
    "numerical_evaluate",
    "plot_data",
    "plot_vector_field",
    # 6 Phase 5 advanced physics diagram tools
    "plot_free_body_diagram",
    "plot_circuit_diagram",
    "plot_field_lines",
    "plot_wave",
    "plot_potential_energy",
    "plot_trajectory",
}


def _extract_tool_json(call_result: object) -> dict:
    # Support structured_content (MCP 2.x), structuredContent (MCP 1.x), and text JSON content
    for attr in ("structured_content", "structuredContent"):
        structured = getattr(call_result, attr, None)
        if isinstance(structured, dict):
            if (
                "result" in structured
                and isinstance(structured["result"], dict)
                and "ok" in structured["result"]
            ):
                return structured["result"]
            if "ok" in structured:
                return structured
    content = getattr(call_result, "content", [])
    for block in content:
        text = getattr(block, "text", None)
        if isinstance(text, str):
            return json.loads(text)
    raise AssertionError(f"Could not extract JSON payload from MCP call result: {call_result!r}")


async def _run_stdio_roundtrip(artifacts_dir: Path) -> None:
    params = StdioServerParameters(
        command=sys.executable,
        args=[
            "-m",
            "science_mcp.server",
            "--artifacts-dir",
            str(artifacts_dir),
            "--timeout",
            "15",
            "--default-format",
            "svg",
        ],
    )
    async with stdio_client(params) as (read, write), ClientSession(read, write) as session:
        await session.initialize()
        listed = await session.list_tools()
        tool_names = {t.name for t in listed.tools}

        assert tool_names == EXPECTED_TOOLS
        # Security invariant: NEVER expose generic Python execution tools
        for forbidden in ("run_python", "exec_python", "eval_code", "shell", "bash"):
            assert forbidden not in tool_names

        # 1. Differentiate
        diff_res = _extract_tool_json(
            await session.call_tool("differentiate", {"expression": "x**4", "variable": "x"})
        )
        assert diff_res["ok"] is True
        assert diff_res["result"]["expression"] == "4*x**3"

        # 2. Check dimensions
        dim_res = _extract_tool_json(
            await session.call_tool(
                "check_dimensions",
                {
                    "lhs_expression": "m * a",
                    "rhs_expression": "k * x",
                    "unit_map": {"m": "kg", "a": "m/s**2", "k": "N/m", "x": "m"},
                },
            )
        )
        assert dim_res["ok"] is True
        assert dim_res["verification"]["status"] == "passed"

        # 3. Plot function into fixed artifacts_dir
        plot_res = _extract_tool_json(
            await session.call_tool(
                "plot_function",
                {"expressions": ["exp(-t)*cos(2*t)"], "variable": "t", "x_range": [0, 5]},
            )
        )
        assert plot_res["ok"] is True
        assert len(plot_res["artifacts"]) == 1
        fname = plot_res["artifacts"][0]["filename"]
        assert (artifacts_dir / fname).is_file()


def test_mcp_stdio_server_lists_21_tools_and_executes_roundtrip(tmp_path: Path) -> None:
    asyncio.run(_run_stdio_roundtrip(tmp_path / "session_artifacts"))
