"""MCP stdio server exposing 21 safe math, physics, unit, and visualization tools.

NEVER exposes any generic Python execution tool (such as `run_python`).
Output artifacts are confined strictly to the server-startup artifacts directory.
"""

from __future__ import annotations

import argparse
import asyncio
import os
import tempfile
from pathlib import Path
from typing import Any

os.environ["MPLBACKEND"] = "Agg"

from mcp.server.mcpserver import MCPServer
from mcp.types import ToolAnnotations

from science_mcp.models import DEFAULT_TIMEOUT_SECONDS, PlotFormat
from science_mcp.runner import clamp_timeout, execute_tool_isolated

_READ_ONLY_CALC_ANNOTATIONS = ToolAnnotations(
    read_only_hint=True,
    destructive_hint=False,
    idempotent_hint=True,
    open_world_hint=False,
)
_PLOT_ARTIFACT_ANNOTATIONS = ToolAnnotations(
    read_only_hint=False,
    destructive_hint=False,
    idempotent_hint=False,
    open_world_hint=False,
)


def create_server(
    *,
    artifacts_dir: Path,
    timeout_seconds: float = DEFAULT_TIMEOUT_SECONDS,
    default_format: PlotFormat = "svg",
) -> MCPServer:
    """Create and configure the Science MCP server instance with all 21 tools."""
    fixed_artifacts_dir = artifacts_dir.resolve()
    fixed_artifacts_dir.mkdir(parents=True, exist_ok=True)
    fixed_timeout = clamp_timeout(timeout_seconds)
    fixed_format: PlotFormat = "png" if default_format == "png" else "svg"

    mcp = MCPServer(
        name="science-mcp",
        instructions=(
            "大学数学・大学物理の記号計算・数値評価・等式検証・単位変換・次元解析・グラフ生成を行う"
            "安全なローカル計算ツールサーバーです。数式には Python 演算子（べき乗は **）を使用し、"
            "任意の Python コード実行は一切サポートしません。"
        ),
    )

    async def _run(tool_name: str, **kwargs: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            execute_tool_isolated,
            tool_name,
            kwargs,
            artifacts_dir=fixed_artifacts_dir,
            timeout_seconds=fixed_timeout,
            default_format=fixed_format,
        )

    # ------------------------------------------------------------------
    # MVP Priority Tools (10)
    # ------------------------------------------------------------------

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def solve_equation(
        equation: str,
        variable: str = "x",
        domain: str = "complex",
    ) -> dict[str, Any]:
        """Solve an equation symbolically with substitution verification."""
        return await _run("solve_equation", equation=equation, variable=variable, domain=domain)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def differentiate(
        expression: str,
        variable: str = "x",
        order: int = 1,
    ) -> dict[str, Any]:
        """Compute an ordinary or partial derivative of order 1..10."""
        return await _run("differentiate", expression=expression, variable=variable, order=order)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def integrate(
        expression: str,
        variable: str = "x",
        lower: str | int | float | None = None,
        upper: str | int | float | None = None,
    ) -> dict[str, Any]:
        """Compute an indefinite or definite integral with automatic verification."""
        return await _run(
            "integrate",
            expression=expression,
            variable=variable,
            lower=lower,
            upper=upper,
        )

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def calculate_determinant(matrix: list[list[Any]]) -> dict[str, Any]:
        """Calculate the exact determinant of a square matrix (up to 20x20)."""
        return await _run("calculate_determinant", matrix=matrix)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def calculate_inverse_matrix(matrix: list[list[Any]]) -> dict[str, Any]:
        """Calculate a matrix inverse with A*A^-1 == I check (explicit error if singular)."""
        return await _run("calculate_inverse_matrix", matrix=matrix)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def matrix_operation(
        operation: str,
        matrices: list[list[list[Any]]],
    ) -> dict[str, Any]:
        """Perform matrix add, subtract, multiply, transpose, rank, or trace."""
        return await _run("matrix_operation", operation=operation, matrices=matrices)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def verify_equation(
        lhs: str,
        rhs: str,
        assumptions: list[str] | dict[str, str] | None = None,
    ) -> dict[str, Any]:
        """Verify whether lhs == rhs symbolically under assumptions (5-state status)."""
        return await _run("verify_equation", lhs=lhs, rhs=rhs, assumptions=assumptions)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def convert_units(
        value: float | int | str,
        from_unit: str,
        to_unit: str,
    ) -> dict[str, Any]:
        """Convert physical quantities between units using Pint (SI default)."""
        return await _run("convert_units", value=value, from_unit=from_unit, to_unit=to_unit)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def check_dimensions(
        lhs_unit: str | None = None,
        rhs_unit: str | None = None,
        lhs_expression: str | None = None,
        rhs_expression: str | None = None,
        unit_map: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        """Verify physical dimensional consistency between units or physics expressions."""
        return await _run(
            "check_dimensions",
            lhs_unit=lhs_unit,
            rhs_unit=rhs_unit,
            lhs_expression=lhs_expression,
            rhs_expression=rhs_expression,
            unit_map=unit_map,
        )

    @mcp.tool(annotations=_PLOT_ARTIFACT_ANNOTATIONS)
    async def plot_function(
        expressions: list[str],
        variable: str = "x",
        x_range: list[float | int] | None = None,
        labels: list[str] | None = None,
        title: str | None = None,
        x_label: str | None = None,
        y_label: str | None = None,
        output_format: str | None = None,
    ) -> dict[str, Any]:
        """Generate a 1D function plot with a UUID name inside the session artifacts dir."""
        return await _run(
            "plot_function",
            expressions=expressions,
            variable=variable,
            x_range=x_range,
            labels=labels,
            title=title,
            x_label=x_label,
            y_label=y_label,
            output_format=output_format,
        )

    # ------------------------------------------------------------------
    # Extended Tools (11)
    # ------------------------------------------------------------------

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def simplify_expression(expression: str) -> dict[str, Any]:
        """Simplify a symbolic mathematical expression."""
        return await _run("simplify_expression", expression=expression)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def expand_expression(expression: str) -> dict[str, Any]:
        """Expand an algebraic or trigonometric expression."""
        return await _run("expand_expression", expression=expression)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def factor_expression(expression: str) -> dict[str, Any]:
        """Factor a polynomial or rational expression."""
        return await _run("factor_expression", expression=expression)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def calculate_limit(
        expression: str,
        variable: str = "x",
        point: str | int | float = "0",
        direction: str = "+-",
    ) -> dict[str, Any]:
        """Calculate a two-sided or one-sided limit (direction: '+-', '+', or '-')."""
        return await _run(
            "calculate_limit",
            expression=expression,
            variable=variable,
            point=point,
            direction=direction,
        )

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def solve_system(
        equations: list[str],
        variables: list[str],
    ) -> dict[str, Any]:
        """Solve a system of equations symbolically with residual verification."""
        return await _run("solve_system", equations=equations, variables=variables)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def solve_ode(
        equation: str,
        func_name: str = "y",
        variable: str = "x",
        ics: dict[str, str | int | float] | None = None,
    ) -> dict[str, Any]:
        """Solve an ODE with optional initial conditions.

        Use Derivative(y(t), t, 2) or diff(y(t), t, 2) in equation,
        and ics keys like {"y(0)": 1, "y'(0)": 0}.
        """
        return await _run(
            "solve_ode",
            equation=equation,
            func_name=func_name,
            variable=variable,
            ics=ics,
        )

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def calculate_eigenvalues(matrix: list[list[Any]]) -> dict[str, Any]:
        """Compute eigenvalues and algebraic multiplicities of a square matrix (up to 10x10)."""
        return await _run("calculate_eigenvalues", matrix=matrix)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def calculate_eigenvectors(matrix: list[list[Any]]) -> dict[str, Any]:
        """Compute eigenvalues and eigenvectors with A*v == lambda*v verification (up to 10x10)."""
        return await _run("calculate_eigenvectors", matrix=matrix)

    @mcp.tool(annotations=_READ_ONLY_CALC_ANNOTATIONS)
    async def numerical_evaluate(
        expression: str,
        substitutions: dict[str, str | int | float] | None = None,
        precision: int = 15,
    ) -> dict[str, Any]:
        """Evaluate an expression numerically to up to 100 digits of precision."""
        return await _run(
            "numerical_evaluate",
            expression=expression,
            substitutions=substitutions,
            precision=precision,
        )

    @mcp.tool(annotations=_PLOT_ARTIFACT_ANNOTATIONS)
    async def plot_data(
        x_values: list[float | int],
        y_values: list[float | int],
        kind: str = "line",
        title: str | None = None,
        x_label: str | None = None,
        y_label: str | None = None,
        label: str | None = None,
        output_format: str | None = None,
    ) -> dict[str, Any]:
        """Plot numerical or experimental (x, y) data as a line or scatter chart."""
        return await _run(
            "plot_data",
            x_values=x_values,
            y_values=y_values,
            kind=kind,
            title=title,
            x_label=x_label,
            y_label=y_label,
            label=label,
            output_format=output_format,
        )

    @mcp.tool(annotations=_PLOT_ARTIFACT_ANNOTATIONS)
    async def plot_vector_field(
        u_expr: str,
        v_expr: str,
        x_var: str = "x",
        y_var: str = "y",
        x_range: list[float | int] | None = None,
        y_range: list[float | int] | None = None,
        grid_size: int = 18,
        title: str | None = None,
        output_format: str | None = None,
    ) -> dict[str, Any]:
        """Plot a 2D vector field (u(x, y), v(x, y)) into the session artifacts directory."""
        return await _run(
            "plot_vector_field",
            u_expr=u_expr,
            v_expr=v_expr,
            x_var=x_var,
            y_var=y_var,
            x_range=x_range,
            y_range=y_range,
            grid_size=grid_size,
            title=title,
            output_format=output_format,
        )

    return mcp


def parse_cli_args(argv: list[str] | None = None) -> argparse.Namespace:
    """Parse CLI arguments with environment variable fallbacks."""
    parser = argparse.ArgumentParser(description="Theorem Science MCP Server (stdio)")
    parser.add_argument(
        "--artifacts-dir",
        default=os.environ.get("SCIENCE_MCP_ARTIFACTS_DIR"),
        help="Fixed output directory for generated plots (server-generated UUID filenames only).",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=float(os.environ.get("SCIENCE_MCP_TIMEOUT_SEC", str(DEFAULT_TIMEOUT_SECONDS))),
        help="Per-tool worker hard timeout in seconds (0.5..60).",
    )
    parser.add_argument(
        "--default-format",
        choices=("svg", "png"),
        default=os.environ.get("SCIENCE_MCP_DEFAULT_FORMAT", "svg").lower(),
        help="Default plot output format (svg preferred).",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> None:
    """CLI entrypoint for `science-mcp` over MCP stdio transport."""
    args = parse_cli_args(argv)
    if args.artifacts_dir:
        artifacts_path = Path(args.artifacts_dir)
    else:
        artifacts_path = Path(tempfile.gettempdir()) / "theorem-science-mcp-artifacts"

    fmt: PlotFormat = "png" if args.default_format == "png" else "svg"
    server = create_server(
        artifacts_dir=artifacts_path,
        timeout_seconds=args.timeout,
        default_format=fmt,
    )
    server.run(transport="stdio")


if __name__ == "__main__":
    main()
