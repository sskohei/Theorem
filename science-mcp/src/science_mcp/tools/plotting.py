"""Safe Matplotlib visualization tools (SVG preferred / 300 DPI PNG, Agg backend, UUID filenames).

SECURITY INVARIANTS:
- Forces MPLBACKEND=Agg (never spawns GUI windows).
- NEVER accepts output file paths or filenames from tool arguments.
- Saves ONLY to `<fixed_artifacts_dir>/<uuid4>.(svg|png)` and verifies canonical containment.
- Evaluates validated SymPy expressions recursively on NumPy arrays (NEVER uses eval/exec/lambdify).
"""

from __future__ import annotations

import math
import os
import uuid
from pathlib import Path
from typing import Any

os.environ["MPLBACKEND"] = "Agg"

import matplotlib

matplotlib.use("Agg")

import matplotlib.pyplot as plt
import numpy as np
import sympy as sp

from science_mcp.models import (
    MAX_PLOT_DATA_POINTS,
    MAX_PLOT_SERIES,
    MAX_VECTOR_GRID_SIZE,
    ArtifactPayload,
    CalculationError,
    InputLimitError,
    PlotFormat,
    UnsafeExpressionError,
    VerificationPayload,
    make_success_response,
)
from science_mcp.parser import parse_safe_expression, validate_identifier

_NUMPY_UNARY_FUNCS: dict[str, Any] = {
    "sin": np.sin,
    "cos": np.cos,
    "tan": np.tan,
    "asin": np.arcsin,
    "acos": np.arccos,
    "atan": np.arctan,
    "sinh": np.sinh,
    "cosh": np.cosh,
    "tanh": np.tanh,
    "asinh": np.arcsinh,
    "acosh": np.arccosh,
    "atanh": np.arctanh,
    "exp": np.exp,
    "log": np.log,
    "sqrt": np.sqrt,
    "Abs": np.abs,
    "floor": np.floor,
    "ceiling": np.ceil,
    "sign": np.sign,
    "re": np.real,
    "im": np.imag,
    "conjugate": np.conjugate,
}


def _eval_sympy_on_numpy(
    expr: sp.Expr,
    env: dict[str, np.ndarray],
    shape: tuple[int, ...],
) -> np.ndarray:
    """Recursively evaluate a validated SymPy expression on NumPy arrays."""
    if expr is sp.pi:
        return np.full(shape, math.pi, dtype=float)
    if expr is sp.E:
        return np.full(shape, math.e, dtype=float)
    if expr.is_Number:
        return np.full(shape, float(expr.evalf()), dtype=float)

    if isinstance(expr, sp.Symbol):
        if expr.name not in env:
            raise CalculationError(
                "UNBOUND_PLOT_SYMBOL",
                f"プロット式に未定義の記号 '{expr.name}' が含まれています。",
            )
        return env[expr.name]

    if isinstance(expr, sp.Add):
        acc = np.zeros(shape, dtype=float)
        for arg in expr.args:
            acc = acc + _eval_sympy_on_numpy(arg, env, shape)
        return acc

    if isinstance(expr, sp.Mul):
        acc = np.ones(shape, dtype=float)
        for arg in expr.args:
            acc = acc * _eval_sympy_on_numpy(arg, env, shape)
        return acc

    if isinstance(expr, sp.Pow):
        base = _eval_sympy_on_numpy(expr.base, env, shape)
        exponent = _eval_sympy_on_numpy(expr.exp, env, shape)
        with np.errstate(all="ignore"):
            return np.power(base, exponent)

    if isinstance(expr, sp.Function):
        fname = expr.func.__name__
        if fname == "atan2" and len(expr.args) == 2:
            a = _eval_sympy_on_numpy(expr.args[0], env, shape)
            b = _eval_sympy_on_numpy(expr.args[1], env, shape)
            with np.errstate(all="ignore"):
                return np.arctan2(a, b)
        if fname == "Heaviside" and len(expr.args) >= 1:
            a = _eval_sympy_on_numpy(expr.args[0], env, shape)
            return np.heaviside(a, 0.5)
        if fname == "csc":
            with np.errstate(all="ignore"):
                return 1.0 / np.sin(_eval_sympy_on_numpy(expr.args[0], env, shape))
        if fname == "sec":
            with np.errstate(all="ignore"):
                return 1.0 / np.cos(_eval_sympy_on_numpy(expr.args[0], env, shape))
        if fname == "cot":
            with np.errstate(all="ignore"):
                return 1.0 / np.tan(_eval_sympy_on_numpy(expr.args[0], env, shape))
        if fname == "cbrt":
            return np.cbrt(_eval_sympy_on_numpy(expr.args[0], env, shape))
        fn = _NUMPY_UNARY_FUNCS.get(fname)
        if fn is None:
            raise CalculationError("UNSUPPORTED_PLOT_FUNCTION", f"描画未対応の関数です: {fname}")
        arg_arr = _eval_sympy_on_numpy(expr.args[0], env, shape)
        with np.errstate(all="ignore"):
            return np.asarray(fn(arg_arr), dtype=float)

    raise CalculationError(
        "UNSUPPORTED_PLOT_EXPR", f"描画できない数式ノードです: {type(expr).__name__}"
    )


def _validate_range(bounds: list[float | int], name: str) -> tuple[float, float]:
    if not isinstance(bounds, list) or len(bounds) != 2:
        raise CalculationError(
            "INVALID_RANGE", f"{name} は [min, max] の 2 要素配列で指定してください。"
        )
    lo, hi = float(bounds[0]), float(bounds[1])
    if not (math.isfinite(lo) and math.isfinite(hi)) or lo >= hi:
        raise CalculationError(
            "INVALID_RANGE", f"{name} は有限値で min < max を満たす必要があります。"
        )
    if abs(hi - lo) > 1e9:
        raise InputLimitError(f"{name} の範囲幅が大きすぎます。")
    return lo, hi


def _normalize_format(fmt: str | None, default_format: PlotFormat) -> PlotFormat:
    raw = (fmt or default_format).strip().lower()
    if raw == "svg":
        return "svg"
    if raw == "png":
        return "png"
    raise UnsafeExpressionError(f"出力形式は 'svg' または 'png' のみ許可されています: {raw}")


def _save_figure_confined(
    fig: plt.Figure,
    artifacts_dir: Path,
    fmt: PlotFormat,
    caption: str | None,
) -> ArtifactPayload:
    """Save a figure strictly inside `artifacts_dir` with a UUID filename."""
    resolved_dir = artifacts_dir.resolve()
    resolved_dir.mkdir(parents=True, exist_ok=True)
    filename = f"{uuid.uuid4()}.{fmt}"
    out_path = (resolved_dir / filename).resolve()
    if out_path.parent != resolved_dir:
        raise UnsafeExpressionError(
            "出力先パスが許可された artifacts ディレクトリ外を指しています。"
        )

    dpi = 300 if fmt == "png" else 150
    fig.savefig(out_path, format=fmt, dpi=dpi, bbox_inches="tight", facecolor="white")
    plt.close(fig)

    mime_type = "image/svg+xml" if fmt == "svg" else "image/png"
    width_px = int(fig.get_figwidth() * dpi)
    height_px = int(fig.get_figheight() * dpi)
    return ArtifactPayload(
        relative_path=f"artifacts/{filename}",
        filename=filename,
        mime_type=mime_type,
        format=fmt,
        caption=caption,
        width_px=width_px,
        height_px=height_px,
    )


def plot_function(
    expressions: list[str],
    *,
    artifacts_dir: Path,
    variable: str = "x",
    x_range: list[float | int] | None = None,
    labels: list[str] | None = None,
    title: str | None = None,
    x_label: str | None = None,
    y_label: str | None = None,
    output_format: str | None = None,
    default_format: PlotFormat = "svg",
) -> dict[str, Any]:
    """Plot one or more 1D functions safely into the session artifacts directory."""
    if not isinstance(expressions, list) or not expressions:
        raise CalculationError("MISSING_EXPRESSIONS", "描画する数式を 1 つ以上指定してください。")
    if len(expressions) > MAX_PLOT_SERIES:
        raise InputLimitError(f"同時描画できる関数は最大 {MAX_PLOT_SERIES} 本です。")

    var_name = validate_identifier(variable)
    x_min, x_max = _validate_range(x_range if x_range is not None else [-10.0, 10.0], "x_range")
    fmt = _normalize_format(output_format, default_format)

    parsed_exprs = [parse_safe_expression(expr, variable_names={var_name}) for expr in expressions]
    xs = np.linspace(x_min, x_max, 800)

    fig, ax = plt.subplots(figsize=(6.4, 4.0))
    valid_series = 0
    for idx, expr in enumerate(parsed_exprs):
        ys = _eval_sympy_on_numpy(expr, {var_name: xs}, xs.shape)
        # Mask non-finite spikes and huge asymptotes cleanly
        mask = np.isfinite(ys) & (np.abs(ys) < 1e6)
        ys_clean = np.where(mask, ys, np.nan)
        if np.any(mask):
            valid_series += 1
        label = (
            labels[idx]
            if labels is not None and idx < len(labels) and labels[idx].strip()
            else f"${sp.latex(expr)}$"
        )
        ax.plot(xs, ys_clean, linewidth=1.8, label=label)

    if valid_series == 0:
        plt.close(fig)
        raise CalculationError(
            "NO_VALID_POINTS", "指定範囲内で描画可能な実数値点が得られませんでした。"
        )

    ax.axhline(0.0, color="#94a3b8", linewidth=0.8)
    ax.axvline(0.0, color="#94a3b8", linewidth=0.8)
    ax.grid(True, linestyle="--", alpha=0.35)
    ax.set_xlabel(x_label or var_name)
    ax.set_ylabel(y_label or f"f({var_name})")
    if title:
        ax.set_title(title)
    ax.legend(loc="best", frameon=True)

    caption = title or f"関数グラフ: {', '.join(expressions[:2])}"
    artifact = _save_figure_confined(fig, artifacts_dir, fmt, caption)

    verification = VerificationPayload(
        status="passed",
        target=f"関数グラフ生成 ({len(expressions)} 系列, {fmt.upper()})",
        assumptions=[f"{var_name} in [{x_min:g}, {x_max:g}]"],
        method="Matplotlib Agg 非対話描画 + AST 検証済み NumPy 評価",
    )
    return make_success_response(
        "plot_function",
        {
            "variable": var_name,
            "x_range": [x_min, x_max],
            "series_count": len(expressions),
            "artifact_path": artifact.relative_path,
            "format": fmt,
        },
        verification=verification,
        artifacts=[artifact],
    )


def plot_data(
    x_values: list[float | int],
    y_values: list[float | int],
    *,
    artifacts_dir: Path,
    kind: str = "line",
    title: str | None = None,
    x_label: str | None = None,
    y_label: str | None = None,
    label: str | None = None,
    output_format: str | None = None,
    default_format: PlotFormat = "svg",
) -> dict[str, Any]:
    """Plot measured/numerical (x, y) data as a line or scatter chart."""
    if not isinstance(x_values, list) or not isinstance(y_values, list):
        raise CalculationError(
            "INVALID_DATA", "x_values と y_values は数値配列で指定してください。"
        )
    if len(x_values) == 0 or len(x_values) != len(y_values):
        raise CalculationError(
            "INVALID_DATA", "x_values と y_values は空でなく同じ長さにしてください。"
        )
    if len(x_values) > MAX_PLOT_DATA_POINTS:
        raise InputLimitError(f"データ点数が上限 ({MAX_PLOT_DATA_POINTS}) を超えています。")

    chart_kind = kind.strip().lower()
    if chart_kind not in ("line", "scatter", "both"):
        raise CalculationError(
            "INVALID_KIND",
            "kind は 'line' / 'scatter' / 'both' のいずれかを指定してください。",
        )

    xs = np.asarray([float(v) for v in x_values], dtype=float)
    ys = np.asarray([float(v) for v in y_values], dtype=float)
    if not (np.all(np.isfinite(xs)) and np.all(np.isfinite(ys))):
        raise CalculationError("INVALID_DATA", "データ配列に非有限値が含まれています。")

    fmt = _normalize_format(output_format, default_format)
    fig, ax = plt.subplots(figsize=(6.4, 4.0))
    if chart_kind == "line":
        ax.plot(xs, ys, linewidth=1.8, label=label or "data")
    elif chart_kind == "scatter":
        ax.scatter(xs, ys, s=24, label=label or "data")
    else:
        ax.plot(xs, ys, linewidth=1.4, marker="o", markersize=4, label=label or "data")

    ax.grid(True, linestyle="--", alpha=0.35)
    ax.set_xlabel(x_label or "x")
    ax.set_ylabel(y_label or "y")
    if title:
        ax.set_title(title)
    if label:
        ax.legend(loc="best", frameon=True)

    caption = title or f"データプロット ({chart_kind}, N={len(xs)})"
    artifact = _save_figure_confined(fig, artifacts_dir, fmt, caption)

    verification = VerificationPayload(
        status="passed",
        target=f"数値データ可視化 ({len(xs)} 点, {fmt.upper()})",
        assumptions=[],
        method="Matplotlib Agg 非対話描画",
    )
    return make_success_response(
        "plot_data",
        {
            "point_count": len(xs),
            "kind": chart_kind,
            "artifact_path": artifact.relative_path,
            "format": fmt,
        },
        verification=verification,
        artifacts=[artifact],
    )


def plot_vector_field(
    u_expr: str,
    v_expr: str,
    *,
    artifacts_dir: Path,
    x_var: str = "x",
    y_var: str = "y",
    x_range: list[float | int] | None = None,
    y_range: list[float | int] | None = None,
    grid_size: int = 18,
    title: str | None = None,
    output_format: str | None = None,
    default_format: PlotFormat = "svg",
) -> dict[str, Any]:
    """Plot a 2D vector field (u(x, y), v(x, y)) such as electric or velocity fields."""
    xv = validate_identifier(x_var)
    yv = validate_identifier(y_var)
    if xv == yv:
        raise CalculationError(
            "INVALID_VARIABLES", "x_var と y_var は異なる変数名を指定してください。"
        )
    if (
        not isinstance(grid_size, int)
        or isinstance(grid_size, bool)
        or not (4 <= grid_size <= MAX_VECTOR_GRID_SIZE)
    ):
        raise InputLimitError(
            f"grid_size は 4 以上 {MAX_VECTOR_GRID_SIZE} 以下で指定してください。"
        )

    x_min, x_max = _validate_range(x_range if x_range is not None else [-5.0, 5.0], "x_range")
    y_min, y_max = _validate_range(y_range if y_range is not None else [-5.0, 5.0], "y_range")
    fmt = _normalize_format(output_format, default_format)

    u_sym = parse_safe_expression(u_expr, variable_names={xv, yv})
    v_sym = parse_safe_expression(v_expr, variable_names={xv, yv})

    xs = np.linspace(x_min, x_max, grid_size)
    ys = np.linspace(y_min, y_max, grid_size)
    X, Y = np.meshgrid(xs, ys)

    U = _eval_sympy_on_numpy(u_sym, {xv: X, yv: Y}, X.shape)
    V = _eval_sympy_on_numpy(v_sym, {xv: X, yv: Y}, X.shape)
    mag = np.hypot(U, V)
    valid = np.isfinite(U) & np.isfinite(V) & np.isfinite(mag)
    U_clean = np.where(valid, U, 0.0)
    V_clean = np.where(valid, V, 0.0)
    mag_clean = np.where(valid, mag, 0.0)

    fig, ax = plt.subplots(figsize=(6.0, 5.2))
    quiv = ax.quiver(X, Y, U_clean, V_clean, mag_clean, cmap="viridis", pivot="mid")
    fig.colorbar(quiv, ax=ax, label="|F|")
    ax.set_xlabel(xv)
    ax.set_ylabel(yv)
    ax.set_aspect("equal", adjustable="box")
    ax.grid(True, linestyle="--", alpha=0.3)
    ax.set_title(title or f"Vector Field ({u_expr}, {v_expr})")

    caption = title or f"2D ベクトル場: ({u_expr}, {v_expr})"
    artifact = _save_figure_confined(fig, artifacts_dir, fmt, caption)

    verification = VerificationPayload(
        status="passed",
        target=f"2D ベクトル場可視化 ({grid_size}x{grid_size}, {fmt.upper()})",
        assumptions=[f"{xv} in [{x_min:g}, {x_max:g}]", f"{yv} in [{y_min:g}, {y_max:g}]"],
        method="Matplotlib quiver (Agg) + AST 検証済み NumPy 評価",
    )
    return make_success_response(
        "plot_vector_field",
        {
            "grid_size": grid_size,
            "x_range": [x_min, x_max],
            "y_range": [y_min, y_max],
            "artifact_path": artifact.relative_path,
            "format": fmt,
        },
        verification=verification,
        artifacts=[artifact],
    )
