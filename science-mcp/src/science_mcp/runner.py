"""Isolated worker-subprocess execution engine with hard timeouts."""

from __future__ import annotations

import contextlib
import multiprocessing
import os
import queue
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any

from science_mcp.models import (
    DEFAULT_TIMEOUT_SECONDS,
    MAX_TIMEOUT_SECONDS,
    MIN_TIMEOUT_SECONDS,
    PlotFormat,
    ScienceMcpError,
    make_error_response,
)

# Tools that require the fixed session artifacts_dir & default_format injected by the server.
_PLOTTING_TOOLS: frozenset[str] = frozenset(
    {
        "plot_function",
        "plot_data",
        "plot_vector_field",
        "plot_free_body_diagram",
        "plot_circuit_diagram",
        "plot_field_lines",
        "plot_wave",
        "plot_potential_energy",
        "plot_trajectory",
    }
)


def _resolve_tool_callable(tool_name: str) -> Callable[..., dict[str, Any]]:
    """Lazily import ONLY the target tool module inside the isolated worker process."""
    if tool_name in {
        "solve_equation",
        "verify_equation",
        "simplify_expression",
        "expand_expression",
        "factor_expression",
        "solve_system",
        "numerical_evaluate",
    }:
        from science_mcp.tools import algebra

        return getattr(algebra, tool_name)

    if tool_name in {"differentiate", "integrate", "calculate_limit"}:
        from science_mcp.tools import calculus

        return getattr(calculus, tool_name)

    if tool_name in {
        "calculate_determinant",
        "calculate_inverse_matrix",
        "matrix_operation",
        "calculate_eigenvalues",
        "calculate_eigenvectors",
    }:
        from science_mcp.tools import linear_algebra

        return getattr(linear_algebra, tool_name)

    if tool_name == "solve_ode":
        from science_mcp.tools import differential_equations

        return differential_equations.solve_ode

    if tool_name in {"convert_units", "check_dimensions"}:
        from science_mcp.tools import dimensions

        return getattr(dimensions, tool_name)

    if tool_name in {"plot_function", "plot_data", "plot_vector_field"}:
        from science_mcp.tools import plotting

        return getattr(plotting, tool_name)

    if tool_name in {
        "plot_free_body_diagram",
        "plot_circuit_diagram",
        "plot_field_lines",
        "plot_wave",
        "plot_potential_energy",
        "plot_trajectory",
    }:
        from science_mcp.tools import physics_diagrams

        return getattr(physics_diagrams, tool_name)

    raise ScienceMcpError("UNKNOWN_TOOL", f"未登録のツールです: {tool_name}")


def _worker_entrypoint(
    tool_name: str,
    kwargs: dict[str, Any],
    artifacts_dir_str: str,
    default_format: PlotFormat,
    result_queue: multiprocessing.Queue[dict[str, Any]],
) -> None:
    """Top-level pickleable worker target running in an isolated spawned child process."""
    os.environ["MPLBACKEND"] = "Agg"
    started = time.perf_counter()
    try:
        fn = _resolve_tool_callable(tool_name)
        call_kwargs = dict(kwargs)
        # Never allow callers to inject path overrides into kwargs
        call_kwargs.pop("artifacts_dir", None)
        call_kwargs.pop("default_format", None)
        if tool_name in _PLOTTING_TOOLS:
            call_kwargs["artifacts_dir"] = Path(artifacts_dir_str)
            call_kwargs["default_format"] = default_format

        response = fn(**call_kwargs)
        elapsed_ms = int((time.perf_counter() - started) * 1000)
        response["duration_ms"] = elapsed_ms
        result_queue.put(response)
    except ScienceMcpError as exc:
        elapsed_ms = int((time.perf_counter() - started) * 1000)
        result_queue.put(
            make_error_response(
                tool_name,
                exc.code,
                exc.message,
                verification_target=f"{tool_name} 入力・計算検証",
                duration_ms=elapsed_ms,
            )
        )
    except Exception as exc:  # noqa: BLE001
        elapsed_ms = int((time.perf_counter() - started) * 1000)
        result_queue.put(
            make_error_response(
                tool_name,
                "CALCULATION_ERROR",
                f"計算中にエラーが発生しました: {type(exc).__name__}: {exc}",
                verification_target=f"{tool_name} 実行",
                duration_ms=elapsed_ms,
            )
        )


def clamp_timeout(timeout_seconds: float | None) -> float:
    """Clamp requested timeout within safe operational bounds."""
    if timeout_seconds is None:
        return DEFAULT_TIMEOUT_SECONDS
    return max(MIN_TIMEOUT_SECONDS, min(MAX_TIMEOUT_SECONDS, float(timeout_seconds)))


def execute_tool_isolated(
    tool_name: str,
    kwargs: dict[str, Any],
    *,
    artifacts_dir: Path,
    timeout_seconds: float = DEFAULT_TIMEOUT_SECONDS,
    default_format: PlotFormat = "svg",
) -> dict[str, Any]:
    """Run a Science MCP tool inside an isolated spawned subprocess with a hard timeout."""
    effective_timeout = clamp_timeout(timeout_seconds)
    resolved_artifacts = artifacts_dir.resolve()
    resolved_artifacts.mkdir(parents=True, exist_ok=True)

    ctx = multiprocessing.get_context("spawn")
    result_queue: multiprocessing.Queue[dict[str, Any]] = ctx.Queue(maxsize=1)
    started = time.perf_counter()
    proc = ctx.Process(
        target=_worker_entrypoint,
        args=(tool_name, kwargs, str(resolved_artifacts), default_format, result_queue),
        daemon=True,
    )
    proc.start()

    try:
        response = result_queue.get(timeout=effective_timeout)
        proc.join(timeout=2.0)
        if proc.is_alive():
            proc.terminate()
            proc.join(timeout=1.0)
        return response
    except queue.Empty:
        # Hard timeout: terminate and kill worker immediately to prevent runaway CPU/RAM
        if proc.is_alive():
            proc.terminate()
            proc.join(timeout=0.5)
        if proc.is_alive():
            proc.kill()
            proc.join(timeout=1.5)
        elapsed_ms = int((time.perf_counter() - started) * 1000)
        return make_error_response(
            tool_name,
            "TIMEOUT",
            f"計算処理が制限時間（{effective_timeout:g} 秒）を超過したため"
            "ワーカープロセスを強制終了しました。",
            verification_target=f"{tool_name} タイムアウト監視",
            duration_ms=elapsed_ms,
        )
    finally:
        with contextlib.suppress(Exception):
            result_queue.close()
            result_queue.join_thread()
        if proc.is_alive():
            proc.kill()
            proc.join(timeout=1.0)
        proc.close()
