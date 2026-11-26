"""Phase 5 advanced university physics diagram tools (pure Matplotlib Agg, SVG/PNG).

Provides structured, safe diagram generation for:
- Free-body & force vector diagrams (`plot_free_body_diagram`)
- DC / RC / RL / RLC schematic circuit diagrams (`plot_circuit_diagram`)
- Electric & magnetic field lines + equipotentials (`plot_field_lines`)
- Wave propagation, superposition & standing waves (`plot_wave`)
- Potential energy curves with energy levels & turning points (`plot_potential_energy`)
- Motion trajectories with symbolic velocity/acceleration vectors (`plot_trajectory`)

SECURITY INVARIANTS:
- NEVER uses eval(), exec(), compile(), sympy.lambdify(), or raw parse_expr().
- All math expressions go strictly through `parse_safe_expression` (AST whitelist).
- All output files are confined strictly to `<artifacts_dir>/<uuid4>.(svg|png)`.
"""

from __future__ import annotations

import math
import os
from pathlib import Path
from typing import Any

os.environ["MPLBACKEND"] = "Agg"

import matplotlib

matplotlib.use("Agg")

import matplotlib.patches as patches
import matplotlib.pyplot as plt
import numpy as np
import sympy as sp

from science_mcp.models import (
    CalculationError,
    InputLimitError,
    PlotFormat,
    VerificationPayload,
    make_success_response,
)
from science_mcp.parser import parse_safe_expression, validate_identifier
from science_mcp.tools.plotting import (
    _eval_sympy_on_numpy,
    _normalize_format,
    _save_figure_confined,
    _validate_range,
)

plt.rcParams["font.sans-serif"] = [
    "Yu Gothic",
    "BIZ UDGothic",
    "Meiryo",
    "Noto Sans CJK JP",
    "DejaVu Sans",
]
plt.rcParams["axes.unicode_minus"] = False

MAX_FORCES: int = 16
MAX_CIRCUIT_ELEMENTS: int = 12
MAX_FIELD_SOURCES: int = 12
MAX_WAVES: int = 8
MAX_ENERGY_LEVELS: int = 8
MAX_TRAJECTORY_VECTORS: int = 24

_FORCE_COLORS: tuple[str, ...] = (
    "#2563eb",
    "#dc2626",
    "#059669",
    "#d97706",
    "#7c3aed",
    "#0891b2",
    "#db2777",
    "#4f46e5",
)


def _finite_float(val: Any, name: str) -> float:
    try:
        num = float(val)
    except (TypeError, ValueError) as exc:
        raise CalculationError(
            "INVALID_NUMBER", f"{name} は有限の実数で指定してください。"
        ) from exc
    if not math.isfinite(num):
        raise CalculationError("INVALID_NUMBER", f"{name} は有限の実数で指定してください。")
    return num


def _sanitize_label(raw: Any, fallback: str, max_len: int = 48) -> str:
    text = str(raw if raw is not None else fallback).strip()
    if not text:
        return fallback
    return text[:max_len]


def plot_free_body_diagram(
    forces: list[dict[str, Any]],
    *,
    artifacts_dir: Path,
    body_label: str = "m",
    incline_deg: float = 0.0,
    show_components: bool = False,
    show_net_force: bool = True,
    title: str | None = None,
    output_format: str | None = None,
    default_format: PlotFormat = "svg",
) -> dict[str, Any]:
    """Draw a free-body / force-vector diagram and verify force equilibrium."""
    if not isinstance(forces, list) or not forces:
        raise CalculationError("MISSING_FORCES", "力のベクトルを 1 つ以上指定してください。")
    if len(forces) > MAX_FORCES:
        raise InputLimitError(f"力のベクトルは最大 {MAX_FORCES} 本まで指定できます。")

    incline = _finite_float(incline_deg, "incline_deg")
    if abs(incline) >= 90.0:
        raise CalculationError("INVALID_ANGLE", "斜面角度は -90°〜90° の範囲で指定してください。")

    fmt = _normalize_format(output_format, default_format)

    parsed_forces: list[tuple[str, float, float, float, str]] = []
    net_fx = 0.0
    net_fy = 0.0
    max_mag = 1.0

    for idx, f in enumerate(forces):
        if not isinstance(f, dict):
            raise CalculationError("INVALID_FORCE", f"forces[{idx}] は辞書形式で指定してください。")
        label = _sanitize_label(f.get("label"), f"F{idx + 1}")
        if "fx" in f and "fy" in f and f["fx"] is not None and f["fy"] is not None:
            fx = _finite_float(f["fx"], f"forces[{idx}].fx")
            fy = _finite_float(f["fy"], f"forces[{idx}].fy")
            mag = math.hypot(fx, fy)
        else:
            mag = _finite_float(f.get("magnitude", 1.0), f"forces[{idx}].magnitude")
            if mag < 0.0:
                raise CalculationError(
                    "INVALID_MAGNITUDE", f"forces[{idx}].magnitude は 0 以上で指定してください。"
                )
            ang_rad = math.radians(
                _finite_float(f.get("angle_deg", 0.0), f"forces[{idx}].angle_deg")
            )
            fx = mag * math.cos(ang_rad)
            fy = mag * math.sin(ang_rad)

        color_raw = f.get("color")
        color = (
            str(color_raw)
            if isinstance(color_raw, str) and color_raw.startswith("#") and len(color_raw) in (4, 7)
            else _FORCE_COLORS[idx % len(_FORCE_COLORS)]
        )
        parsed_forces.append((label, fx, fy, mag, color))
        net_fx += fx
        net_fy += fy
        max_mag = max(max_mag, mag)

    net_mag = math.hypot(net_fx, net_fy)
    scale_max = max(max_mag, net_mag if show_net_force else 0.0, 1.0)
    limit = scale_max * 1.45

    fig, ax = plt.subplots(figsize=(6.0, 5.2))

    # Optional inclined plane surface
    if abs(incline) > 1e-6:
        rad = math.radians(incline)
        dx = limit * 0.9 * math.cos(rad)
        dy = limit * 0.9 * math.sin(rad)
        ax.plot([-dx, dx], [-dy - limit * 0.16, dy - limit * 0.16], color="#475569", linewidth=2.0)
        ax.text(
            -dx * 0.72,
            -dy * 0.72 - limit * 0.24,
            f"θ = {incline:g}°",
            fontsize=9,
            color="#475569",
        )

    # Coordinate axes
    ax.axhline(0.0, color="#cbd5e1", linewidth=0.9, linestyle=":")
    ax.axvline(0.0, color="#cbd5e1", linewidth=0.9, linestyle=":")

    # Central body box (rotated if inclined)
    box_size = scale_max * 0.26
    body = patches.FancyBboxPatch(
        (-box_size / 2.0, -box_size / 2.0),
        box_size,
        box_size,
        boxstyle="round,pad=0.02",
        facecolor="#e2e8f0",
        edgecolor="#0f172a",
        linewidth=1.6,
        zorder=4,
    )
    ax.add_patch(body)
    ax.text(
        0.0,
        0.0,
        _sanitize_label(body_label, "m", 16),
        ha="center",
        va="center",
        fontsize=11,
        fontweight="bold",
        color="#0f172a",
        zorder=5,
    )

    for label, fx, fy, mag, color in parsed_forces:
        if mag < 1e-12:
            continue
        ax.annotate(
            "",
            xy=(fx, fy),
            xytext=(0.0, 0.0),
            arrowprops={"arrowstyle": "-|>", "color": color, "lw": 2.2, "mutation_scale": 14},
            zorder=6,
        )
        # Place label slightly beyond arrow tip
        ux, uy = fx / mag, fy / mag
        lx = fx + ux * scale_max * 0.14
        ly = fy + uy * scale_max * 0.14
        ax.text(
            lx,
            ly,
            f"{label} ({mag:.3g})",
            ha="center",
            va="center",
            fontsize=9.5,
            fontweight="bold",
            color=color,
            bbox={
                "boxstyle": "round,pad=0.15",
                "facecolor": "white",
                "edgecolor": "none",
                "alpha": 0.8,
            },
            zorder=7,
        )
        if show_components and abs(fx) > 1e-6 and abs(fy) > 1e-6:
            ax.plot([0.0, fx], [0.0, 0.0], color=color, linestyle="--", linewidth=1.0, alpha=0.6)
            ax.plot([fx, fx], [0.0, fy], color=color, linestyle="--", linewidth=1.0, alpha=0.6)

    if show_net_force and net_mag > 1e-6:
        ax.annotate(
            "",
            xy=(net_fx, net_fy),
            xytext=(0.0, 0.0),
            arrowprops={
                "arrowstyle": "-|>",
                "color": "#0f172a",
                "lw": 2.0,
                "ls": "--",
                "mutation_scale": 13,
            },
            zorder=8,
        )
        ux, uy = net_fx / net_mag, net_fy / net_mag
        ax.text(
            net_fx + ux * scale_max * 0.16,
            net_fy + uy * scale_max * 0.16,
            f"F_net ({net_mag:.3g})",
            ha="center",
            va="center",
            fontsize=9,
            color="#0f172a",
            bbox={
                "boxstyle": "round,pad=0.15",
                "facecolor": "#fef3c7",
                "edgecolor": "#d97706",
                "lw": 0.6,
            },
            zorder=9,
        )

    ax.set_xlim(-limit, limit)
    ax.set_ylim(-limit, limit)
    ax.set_aspect("equal", adjustable="box")
    ax.set_xlabel("x")
    ax.set_ylabel("y")
    ax.set_title(title or "Free-Body Diagram (自由物体図)")

    caption = title or f"自由物体図 ({body_label}): {len(parsed_forces)} 本の力"
    artifact = _save_figure_confined(fig, artifacts_dir, fmt, caption)

    in_equilibrium = net_mag <= max(1e-7, 1e-6 * max_mag)
    net_angle_deg = math.degrees(math.atan2(net_fy, net_fx)) if net_mag > 1e-12 else 0.0
    verification = VerificationPayload(
        status="passed" if in_equilibrium else "inconclusive",
        target="力の合算および静的つり合い確認",
        assumptions=[f"forces={len(parsed_forces)}", f"incline_deg={incline:g}"],
        method="vector_sum(Fx, Fy)",
        detail={
            "net_fx": round(net_fx, 8),
            "net_fy": round(net_fy, 8),
            "net_magnitude": round(net_mag, 8),
            "net_angle_deg": round(net_angle_deg, 4),
            "in_equilibrium": in_equilibrium,
        },
    )
    return make_success_response(
        "plot_free_body_diagram",
        {
            "force_count": len(parsed_forces),
            "net_force": {
                "fx": net_fx,
                "fy": net_fy,
                "magnitude": net_mag,
                "angle_deg": net_angle_deg,
            },
            "in_equilibrium": in_equilibrium,
            "artifact_path": artifact.relative_path,
            "format": fmt,
        },
        verification=verification,
        artifacts=[artifact],
    )


def _draw_circuit_component(
    ax: plt.Axes,
    x0: float,
    y0: float,
    x1: float,
    y1: float,
    kind: str,
    label: str,
    value: str | None,
) -> None:
    """Draw a standard schematic component along segment (x0, y0) -> (x1, y1)."""
    dx, dy = x1 - x0, y1 - y0
    seg_len = math.hypot(dx, dy)
    if seg_len < 1e-6:
        return
    ux, uy = dx / seg_len, dy / seg_len
    nx, ny = -uy, ux  # left normal
    mx, my = (x0 + x1) / 2.0, (y0 + y1) / 2.0
    span = min(seg_len * 0.42, 0.9)
    a_x, a_y = mx - ux * span / 2.0, my - uy * span / 2.0
    b_x, b_y = mx + ux * span / 2.0, my + uy * span / 2.0

    # Lead wires
    ax.plot([x0, a_x], [y0, a_y], color="#0f172a", lw=1.8)
    ax.plot([b_x, x1], [b_y, y1], color="#0f172a", lw=1.8)

    if kind == "resistor":
        n_zig = 6
        pts_x = [a_x]
        pts_y = [a_y]
        amp = 0.14
        for i in range(1, n_zig):
            frac = i / n_zig
            sign = 1.0 if i % 2 == 1 else -1.0
            px = a_x + ux * span * frac + nx * amp * sign
            py = a_y + uy * span * frac + ny * amp * sign
            pts_x.append(px)
            pts_y.append(py)
        pts_x.append(b_x)
        pts_y.append(b_y)
        ax.plot(pts_x, pts_y, color="#2563eb", lw=2.0)
    elif kind == "capacitor":
        gap = 0.12
        p1x, p1y = mx - ux * gap / 2.0, my - uy * gap / 2.0
        p2x, p2y = mx + ux * gap / 2.0, my + uy * gap / 2.0
        ax.plot([a_x, p1x], [a_y, p1y], color="#0f172a", lw=1.8)
        ax.plot([p2x, b_x], [p2y, b_y], color="#0f172a", lw=1.8)
        plate = 0.22
        ax.plot(
            [p1x - nx * plate, p1x + nx * plate],
            [p1y - ny * plate, p1y + ny * plate],
            color="#059669",
            lw=2.4,
        )
        ax.plot(
            [p2x - nx * plate, p2x + nx * plate],
            [p2y - ny * plate, p2y + ny * plate],
            color="#059669",
            lw=2.4,
        )
    elif kind == "inductor":
        n_loops = 4
        ts = np.linspace(0.0, n_loops * math.pi, 160)
        along = (ts / (n_loops * math.pi)) * span
        perp = np.abs(np.sin(ts)) * 0.18
        xs = a_x + ux * along + nx * perp
        ys = a_y + uy * along + ny * perp
        ax.plot(xs, ys, color="#7c3aed", lw=2.0)
    elif kind == "voltage_source":
        circ = patches.Circle(
            (mx, my), radius=span * 0.42, facecolor="#fef2f2", edgecolor="#dc2626", lw=2.0, zorder=4
        )
        ax.add_patch(circ)
        ax.text(
            mx + ux * span * 0.16,
            my + uy * span * 0.16,
            "+",
            ha="center",
            va="center",
            fontsize=10,
            fontweight="bold",
            color="#dc2626",
            zorder=5,
        )
        ax.text(
            mx - ux * span * 0.16,
            my - uy * span * 0.16,
            "-",
            ha="center",
            va="center",
            fontsize=11,
            fontweight="bold",
            color="#dc2626",
            zorder=5,
        )
    elif kind == "switch":
        ax.plot(
            [a_x, b_x - ux * 0.05 + nx * 0.22],
            [a_y, b_y - uy * 0.05 + ny * 0.22],
            color="#d97706",
            lw=2.0,
        )
        ax.scatter([a_x, b_x], [a_y, b_y], s=22, color="#0f172a", zorder=5)
    elif kind in {"ammeter", "voltmeter"}:
        sym = "A" if kind == "ammeter" else "V"
        circ = patches.Circle(
            (mx, my), radius=span * 0.38, facecolor="#eff6ff", edgecolor="#1d4ed8", lw=1.8, zorder=4
        )
        ax.add_patch(circ)
        ax.text(
            mx,
            my,
            sym,
            ha="center",
            va="center",
            fontsize=10,
            fontweight="bold",
            color="#1d4ed8",
            zorder=5,
        )
    else:
        ax.plot([a_x, b_x], [a_y, b_y], color="#0f172a", lw=2.0)

    display_lbl = f"{label} ({value})" if value else label
    offset = 0.38
    ax.text(
        mx + nx * offset,
        my + ny * offset,
        display_lbl,
        ha="center",
        va="center",
        fontsize=9.5,
        fontweight="bold",
        color="#0f172a",
        bbox={
            "boxstyle": "round,pad=0.15",
            "facecolor": "white",
            "edgecolor": "none",
            "alpha": 0.85,
        },
    )


def plot_circuit_diagram(
    elements: list[dict[str, Any]],
    *,
    artifacts_dir: Path,
    topology: str = "series",
    current_label: str | None = "I",
    title: str | None = None,
    output_format: str | None = None,
    default_format: PlotFormat = "svg",
) -> dict[str, Any]:
    """Generate a clean DC / RC / RL / RLC schematic circuit diagram."""
    if not isinstance(elements, list) or not elements:
        raise CalculationError("MISSING_ELEMENTS", "回路素子を 1 つ以上指定してください。")
    if len(elements) > MAX_CIRCUIT_ELEMENTS:
        raise InputLimitError(f"回路素子は最大 {MAX_CIRCUIT_ELEMENTS} 個まで指定できます。")

    allowed_kinds = {
        "voltage_source",
        "resistor",
        "capacitor",
        "inductor",
        "switch",
        "ammeter",
        "voltmeter",
    }
    normalized: list[tuple[str, str, str | None]] = []
    for idx, elem in enumerate(elements):
        if not isinstance(elem, dict):
            raise CalculationError(
                "INVALID_ELEMENT", f"elements[{idx}] は辞書形式で指定してください。"
            )
        kind = str(elem.get("kind", "resistor")).strip().lower()
        if kind not in allowed_kinds:
            raise CalculationError("UNKNOWN_CIRCUIT_ELEMENT", f"未対応の回路素子種別です: {kind}")
        label = _sanitize_label(elem.get("label"), f"E{idx + 1}", 24)
        val_raw = elem.get("value")
        value = (
            _sanitize_label(val_raw, "", 24)
            if val_raw is not None and str(val_raw).strip()
            else None
        )
        normalized.append((kind, label, value))

    topo = topology.strip().lower()
    if topo not in {"series", "parallel"}:
        raise CalculationError(
            "INVALID_TOPOLOGY", "topology は 'series' または 'parallel' を指定してください。"
        )

    fmt = _normalize_format(output_format, default_format)
    fig, ax = plt.subplots(figsize=(6.8, 4.4))

    if topo == "series":
        # Place elements neatly along a rectangular loop: left (source), top, right, bottom
        width = max(4.2, 1.5 * max(1, (len(normalized) + 1) // 2))
        height = 2.6
        # Distribute segments around rectangle perimeter
        n = len(normalized)
        left_elems = [e for e in normalized if e[0] == "voltage_source"][:1]
        rest = [e for e in normalized if e not in left_elems]
        if not left_elems:
            left_elems = [normalized[0]]
            rest = normalized[1:]

        # Left side: (0,0) -> (0, height)
        _draw_circuit_component(ax, 0.0, 0.0, 0.0, height, *left_elems[0])

        # Split remaining across top, right, bottom
        n_rest = len(rest)
        if n_rest == 0:
            ax.plot([0.0, width, width, 0.0], [height, height, 0.0, 0.0], color="#0f172a", lw=1.8)
        elif n_rest == 1:
            ax.plot([0.0, width], [height, height], color="#0f172a", lw=1.8)
            _draw_circuit_component(ax, width, height, width, 0.0, *rest[0])
            ax.plot([width, 0.0], [0.0, 0.0], color="#0f172a", lw=1.8)
        elif n_rest == 2:
            _draw_circuit_component(ax, 0.0, height, width, height, *rest[0])
            _draw_circuit_component(ax, width, height, width, 0.0, *rest[1])
            ax.plot([width, 0.0], [0.0, 0.0], color="#0f172a", lw=1.8)
        else:
            top_count = max(1, (n_rest + 1) // 2)
            right_count = 1
            bottom_count = max(0, n_rest - top_count - right_count)
            top_items = rest[:top_count]
            right_items = rest[top_count : top_count + right_count]
            bottom_items = rest[top_count + right_count :]

            for i, item in enumerate(top_items):
                x0 = width * i / len(top_items)
                x1 = width * (i + 1) / len(top_items)
                _draw_circuit_component(ax, x0, height, x1, height, *item)
            if right_items:
                _draw_circuit_component(ax, width, height, width, 0.0, *right_items[0])
            else:
                ax.plot([width, width], [height, 0.0], color="#0f172a", lw=1.8)
            if bottom_count > 0:
                for i, item in enumerate(bottom_items):
                    x0 = width * (1.0 - i / len(bottom_items))
                    x1 = width * (1.0 - (i + 1) / len(bottom_items))
                    _draw_circuit_component(ax, x0, 0.0, x1, 0.0, *item)
            else:
                ax.plot([width, 0.0], [0.0, 0.0], color="#0f172a", lw=1.8)

        if current_label:
            ax.annotate(
                "",
                xy=(width * 0.28, height + 0.22),
                xytext=(width * 0.08, height + 0.22),
                arrowprops={"arrowstyle": "-|>", "color": "#dc2626", "lw": 1.6},
            )
            ax.text(
                width * 0.18,
                height + 0.36,
                _sanitize_label(current_label, "I", 12),
                ha="center",
                va="bottom",
                fontsize=9.5,
                fontweight="bold",
                color="#dc2626",
            )
        ax.set_xlim(-1.0, width + 1.0)
        ax.set_ylim(-0.9, height + 0.9)
    else:
        # Parallel branches between top rail y=2.6 and bottom rail y=0.0
        n = len(normalized)
        spacing = 1.6
        total_w = max(spacing, spacing * (n - 1))
        height = 2.6
        ax.plot([0.0, total_w], [height, height], color="#0f172a", lw=2.0)
        ax.plot([0.0, total_w], [0.0, 0.0], color="#0f172a", lw=2.0)
        for i, item in enumerate(normalized):
            x = i * spacing
            if i == 0 and item[0] == "voltage_source":
                _draw_circuit_component(ax, x, 0.0, x, height, *item)
            else:
                _draw_circuit_component(ax, x, height, x, 0.0, *item)
        ax.set_xlim(-1.0, total_w + 1.1)
        ax.set_ylim(-0.7, height + 0.7)

    ax.set_aspect("equal", adjustable="box")
    ax.axis("off")
    ax.set_title(title or f"Circuit Diagram ({topo})")

    caption = title or f"電気回路図 ({topo}, {len(normalized)} 素子)"
    artifact = _save_figure_confined(fig, artifacts_dir, fmt, caption)
    return make_success_response(
        "plot_circuit_diagram",
        {
            "topology": topo,
            "element_count": len(normalized),
            "elements": [{"kind": k, "label": lbl, "value": v} for k, lbl, v in normalized],
            "artifact_path": artifact.relative_path,
            "format": fmt,
        },
        verification=VerificationPayload(
            status="passed",
            target="電気回路図の概略描画",
            assumptions=[f"topology={topo}", f"elements={len(normalized)}"],
            method="matplotlib.patches schematic rendering",
        ),
        artifacts=[artifact],
    )


def plot_field_lines(
    sources: list[dict[str, Any]],
    *,
    artifacts_dir: Path,
    field_kind: str = "electric",
    x_range: list[float | int] | None = None,
    y_range: list[float | int] | None = None,
    show_equipotentials: bool = True,
    title: str | None = None,
    output_format: str | None = None,
    default_format: PlotFormat = "svg",
) -> dict[str, Any]:
    """Plot 2D electric field lines (+ equipotentials) or magnetic field lines of line currents."""
    if not isinstance(sources, list) or not sources:
        raise CalculationError("MISSING_SOURCES", "電荷または電流源を 1 つ以上指定してください。")
    if len(sources) > MAX_FIELD_SOURCES:
        raise InputLimitError(f"場源は最大 {MAX_FIELD_SOURCES} 個まで指定できます。")

    kind = field_kind.strip().lower()
    if kind not in {"electric", "magnetic"}:
        raise CalculationError(
            "INVALID_FIELD_KIND", "field_kind は 'electric' または 'magnetic' を指定してください。"
        )

    x_min, x_max = _validate_range(x_range if x_range is not None else [-3.0, 3.0], "x_range")
    y_min, y_max = _validate_range(y_range if y_range is not None else [-3.0, 3.0], "y_range")
    fmt = _normalize_format(output_format, default_format)

    parsed_sources: list[tuple[float, float, float]] = []
    for idx, src in enumerate(sources):
        if not isinstance(src, dict):
            raise CalculationError(
                "INVALID_SOURCE", f"sources[{idx}] は辞書形式で指定してください。"
            )
        sx = _finite_float(src.get("x", 0.0), f"sources[{idx}].x")
        sy = _finite_float(src.get("y", 0.0), f"sources[{idx}].y")
        strength = _finite_float(src.get("q", src.get("current", 1.0)), f"sources[{idx}].q/current")
        parsed_sources.append((sx, sy, strength))

    nx, ny = 80, 80
    xs = np.linspace(x_min, x_max, nx)
    ys = np.linspace(y_min, y_max, ny)
    xx, yy = np.meshgrid(xs, ys)

    fx = np.zeros_like(xx)
    fy = np.zeros_like(yy)
    potential = np.zeros_like(xx)
    eps2 = ((x_max - x_min) * 0.025) ** 2

    for sx, sy, s_val in parsed_sources:
        rx = xx - sx
        ry = yy - sy
        r2 = rx * rx + ry * ry + eps2
        r = np.sqrt(r2)
        if kind == "electric":
            fx += s_val * rx / (r2 * r)
            fy += s_val * ry / (r2 * r)
            potential += s_val / r
        else:
            # Biot-Savart 2D cross section: B ~ I * (-ry, rx) / r^2
            fx += s_val * (-ry) / r2
            fy += s_val * rx / r2

    fig, ax = plt.subplots(figsize=(6.2, 5.4))
    mag = np.hypot(fx, fy)
    color_field = np.log1p(mag)
    ax.streamplot(
        xx,
        yy,
        fx,
        fy,
        color=color_field,
        cmap="viridis" if kind == "electric" else "plasma",
        density=1.25,
        linewidth=1.1,
        arrowsize=1.1,
    )

    if kind == "electric" and show_equipotentials:
        p_clip = np.clip(potential, -6.0, 6.0)
        levels = np.linspace(-3.0, 3.0, 15)
        ax.contour(
            xx,
            yy,
            p_clip,
            levels=levels,
            colors="#64748b",
            linewidths=0.7,
            linestyles="--",
            alpha=0.65,
        )

    for sx, sy, s_val in parsed_sources:
        is_pos = s_val >= 0.0
        col = "#dc2626" if is_pos else "#2563eb"
        marker_txt = ("+" if is_pos else "-") if kind == "electric" else ("." if is_pos else "x")
        circ = patches.Circle(
            (sx, sy),
            radius=(x_max - x_min) * 0.032,
            facecolor=col,
            edgecolor="white",
            lw=1.2,
            zorder=6,
        )
        ax.add_patch(circ)
        ax.text(
            sx,
            sy,
            marker_txt,
            ha="center",
            va="center",
            color="white",
            fontsize=10,
            fontweight="bold",
            zorder=7,
        )

    ax.set_xlim(x_min, x_max)
    ax.set_ylim(y_min, y_max)
    ax.set_aspect("equal", adjustable="box")
    ax.set_xlabel("x")
    ax.set_ylabel("y")
    default_title = (
        "Electric Field Lines & Equipotentials (電場・等電位線)"
        if kind == "electric"
        else "Magnetic Field Lines (磁力線)"
    )
    ax.set_title(title or default_title)

    artifact = _save_figure_confined(fig, artifacts_dir, fmt, title or default_title)
    return make_success_response(
        "plot_field_lines",
        {
            "field_kind": kind,
            "source_count": len(parsed_sources),
            "show_equipotentials": bool(kind == "electric" and show_equipotentials),
            "artifact_path": artifact.relative_path,
            "format": fmt,
        },
        verification=VerificationPayload(
            status="passed",
            target=f"2D {kind} 場の数値サンプリング描画",
            assumptions=[f"sources={len(parsed_sources)}", f"grid={nx}x{ny}"],
            method="Coulomb / Biot-Savart 2D field evaluation",
        ),
        artifacts=[artifact],
    )


def plot_wave(
    waves: list[dict[str, Any]],
    *,
    artifacts_dir: Path,
    x_range: list[float | int] | None = None,
    t: float = 0.0,
    show_superposition: bool = True,
    show_envelope: bool = False,
    title: str | None = None,
    output_format: str | None = None,
    default_format: PlotFormat = "svg",
) -> dict[str, Any]:
    """Plot harmonic waves, wave superposition, beats, or standing waves."""
    if not isinstance(waves, list) or not waves:
        raise CalculationError("MISSING_WAVES", "波動パラメータを 1 つ以上指定してください。")
    if len(waves) > MAX_WAVES:
        raise InputLimitError(f"波動は最大 {MAX_WAVES} 本まで同時描画できます。")

    x_min, x_max = _validate_range(
        x_range if x_range is not None else [0.0, 4.0 * math.pi], "x_range"
    )
    t_val = _finite_float(t, "t")
    fmt = _normalize_format(output_format, default_format)

    xs = np.linspace(x_min, x_max, 900)
    y_total = np.zeros_like(xs)
    fig, ax = plt.subplots(figsize=(6.6, 4.0))

    for idx, w in enumerate(waves):
        if not isinstance(w, dict):
            raise CalculationError("INVALID_WAVE", f"waves[{idx}] は辞書形式で指定してください。")
        amp = _finite_float(w.get("amplitude", 1.0), f"waves[{idx}].amplitude")
        k = _finite_float(w.get("wavenumber", 1.0), f"waves[{idx}].wavenumber")
        omega = _finite_float(w.get("omega", 0.0), f"waves[{idx}].omega")
        phase = _finite_float(w.get("phase", 0.0), f"waves[{idx}].phase")
        label = _sanitize_label(w.get("label"), f"y_{idx + 1}(x, t)")

        ys = amp * np.sin(k * xs - omega * t_val + phase)
        y_total += ys
        ls = "--" if (show_superposition and len(waves) > 1) else "-"
        alpha = 0.72 if (show_superposition and len(waves) > 1) else 0.95
        ax.plot(xs, ys, linestyle=ls, linewidth=1.4, alpha=alpha, label=label)

    if show_superposition and len(waves) > 1:
        ax.plot(xs, y_total, color="#0f172a", linewidth=2.2, label="合成波 (Superposition)")

    if show_envelope and len(waves) == 2:
        a0 = abs(_finite_float(waves[0].get("amplitude", 1.0), "waves[0].amplitude"))
        a1 = abs(_finite_float(waves[1].get("amplitude", 1.0), "waves[1].amplitude"))
        k0 = _finite_float(waves[0].get("wavenumber", 1.0), "waves[0].wavenumber")
        k1 = _finite_float(waves[1].get("wavenumber", 1.0), "waves[1].wavenumber")
        w0 = _finite_float(waves[0].get("omega", 0.0), "waves[0].omega")
        w1 = _finite_float(waves[1].get("omega", 0.0), "waves[1].omega")
        p0 = _finite_float(waves[0].get("phase", 0.0), "waves[0].phase")
        p1 = _finite_float(waves[1].get("phase", 0.0), "waves[1].phase")
        dk = (k0 - k1) / 2.0
        dw = (w0 - w1) / 2.0
        dp = (p0 - p1) / 2.0
        env = np.sqrt(a0 * a0 + a1 * a1 + 2.0 * a0 * a1 * np.cos(2.0 * (dk * xs - dw * t_val + dp)))
        ax.plot(xs, env, color="#dc2626", linestyle=":", linewidth=1.4, label="包絡線 (Envelope)")
        ax.plot(xs, -env, color="#dc2626", linestyle=":", linewidth=1.4)

    ax.axhline(0.0, color="#94a3b8", linewidth=0.8)
    ax.grid(True, linestyle="--", alpha=0.35)
    ax.set_xlabel("x")
    ax.set_ylabel("y(x, t)")
    ax.set_title(title or f"Wave & Superposition (t = {t_val:g})")
    ax.legend(loc="best", fontsize=8.5)

    caption = title or f"波動・重ね合わせグラフ (t={t_val:g})"
    artifact = _save_figure_confined(fig, artifacts_dir, fmt, caption)
    return make_success_response(
        "plot_wave",
        {
            "wave_count": len(waves),
            "t": t_val,
            "max_amplitude": float(np.max(np.abs(y_total))),
            "artifact_path": artifact.relative_path,
            "format": fmt,
        },
        verification=VerificationPayload(
            status="passed",
            target="波動重ね合わせの数値評価",
            assumptions=[f"waves={len(waves)}", f"t={t_val:g}"],
            method="numpy harmonic superposition",
        ),
        artifacts=[artifact],
    )


def plot_potential_energy(
    expression: str,
    *,
    artifacts_dir: Path,
    variable: str = "x",
    x_range: list[float | int] | None = None,
    energy_levels: list[float | int] | None = None,
    labels: list[str] | None = None,
    title: str | None = None,
    output_format: str | None = None,
    default_format: PlotFormat = "svg",
) -> dict[str, Any]:
    """Plot a potential energy curve U(x) with total energy levels and classical turning points."""
    var_name = validate_identifier(variable)
    x_min, x_max = _validate_range(x_range if x_range is not None else [-4.0, 4.0], "x_range")
    fmt = _normalize_format(output_format, default_format)

    expr = parse_safe_expression(expression, variable_names={var_name})
    xs = np.linspace(x_min, x_max, 900)
    us = _eval_sympy_on_numpy(expr, {var_name: xs}, xs.shape)
    mask = np.isfinite(us) & (np.abs(us) < 1e6)
    if not np.any(mask):
        raise CalculationError(
            "NO_VALID_POINTS", "指定範囲内で有効なポテンシャル値が得られませんでした。"
        )
    u_clean = np.where(mask, us, np.nan)

    levels: list[float] = []
    if energy_levels is not None:
        if len(energy_levels) > MAX_ENERGY_LEVELS:
            raise InputLimitError(f"エネルギー準位は最大 {MAX_ENERGY_LEVELS} 本まで指定できます。")
        levels = [_finite_float(e, f"energy_levels[{i}]") for i, e in enumerate(energy_levels)]

    fig, ax = plt.subplots(figsize=(6.6, 4.2))
    ax.plot(
        xs, u_clean, color="#1d4ed8", linewidth=2.2, label=f"$U({var_name}) = {sp.latex(expr)}$"
    )

    turning_points: list[float] = []
    for idx, e_val in enumerate(levels):
        lbl = (
            labels[idx]
            if labels is not None and idx < len(labels) and labels[idx].strip()
            else f"E_{idx + 1} = {e_val:g}"
        )
        col = "#dc2626" if idx == 0 else _FORCE_COLORS[(idx + 2) % len(_FORCE_COLORS)]
        ax.axhline(e_val, color=col, linestyle="--", linewidth=1.4, label=lbl)
        if idx == 0:
            # Shade classically allowed region (U(x) <= E_0) and detect turning points
            allowed = mask & (us <= e_val)
            ax.fill_between(
                xs,
                u_clean,
                e_val,
                where=allowed,
                color="#bfdbfe",
                alpha=0.38,
                label="古典的許容領域",
            )
            diff = us - e_val
            for k in range(len(xs) - 1):
                if (
                    mask[k]
                    and mask[k + 1]
                    and diff[k] * diff[k + 1] <= 0.0
                    and diff[k] != diff[k + 1]
                ):
                    frac = abs(diff[k]) / (abs(diff[k]) + abs(diff[k + 1]))
                    tp_x = float(xs[k] + frac * (xs[k + 1] - xs[k]))
                    turning_points.append(round(tp_x, 5))
            if turning_points:
                ax.scatter(
                    turning_points,
                    [e_val] * len(turning_points),
                    color="#dc2626",
                    s=36,
                    zorder=6,
                    label="古典的転回点",
                )

    ax.axhline(0.0, color="#94a3b8", linewidth=0.8)
    ax.axvline(0.0, color="#94a3b8", linewidth=0.8)
    ax.grid(True, linestyle="--", alpha=0.35)
    ax.set_xlabel(var_name)
    ax.set_ylabel(f"U({var_name}), E")
    ax.set_title(title or f"Potential Energy Curve U({var_name})")
    ax.legend(loc="best", fontsize=8.5)

    caption = title or f"ポテンシャルエネルギー曲線 U({var_name})"
    artifact = _save_figure_confined(fig, artifacts_dir, fmt, caption)
    return make_success_response(
        "plot_potential_energy",
        {
            "expression": str(expr),
            "latex": sp.latex(expr),
            "energy_levels": levels,
            "turning_points": turning_points,
            "artifact_path": artifact.relative_path,
            "format": fmt,
        },
        verification=VerificationPayload(
            status="passed",
            target="ポテンシャルエネルギー曲線と古典的転回点検出",
            assumptions=[f"{var_name} in [{x_min:g}, {x_max:g}]"],
            method="AST-safe U(x) evaluation + linear turning-point root bracket",
            detail={"turning_points": turning_points},
        ),
        artifacts=[artifact],
    )


def plot_trajectory(
    x_expression: str,
    y_expression: str,
    *,
    artifacts_dir: Path,
    parameter: str = "t",
    t_range: list[float | int] | None = None,
    show_velocity_vectors: bool = True,
    num_vectors: int = 8,
    title: str | None = None,
    x_label: str | None = None,
    y_label: str | None = None,
    output_format: str | None = None,
    default_format: PlotFormat = "svg",
) -> dict[str, Any]:
    """Plot a 2D parametric motion trajectory (x(t), y(t)) with velocity vectors."""
    param_name = validate_identifier(parameter)
    t_min, t_max = _validate_range(t_range if t_range is not None else [0.0, 2.0], "t_range")
    fmt = _normalize_format(output_format, default_format)

    n_vec = max(0, min(MAX_TRAJECTORY_VECTORS, int(num_vectors)))
    x_expr = parse_safe_expression(x_expression, variable_names={param_name})
    y_expr = parse_safe_expression(y_expression, variable_names={param_name})
    param_sym = sp.Symbol(param_name)

    vx_expr = sp.diff(x_expr, param_sym)
    vy_expr = sp.diff(y_expr, param_sym)

    ts = np.linspace(t_min, t_max, 600)
    xs = _eval_sympy_on_numpy(x_expr, {param_name: ts}, ts.shape)
    ys = _eval_sympy_on_numpy(y_expr, {param_name: ts}, ts.shape)
    mask = np.isfinite(xs) & np.isfinite(ys) & (np.abs(xs) < 1e6) & (np.abs(ys) < 1e6)
    if not np.any(mask):
        raise CalculationError(
            "NO_VALID_POINTS", "指定範囲内で有効な軌跡座標が得られませんでした。"
        )

    fig, ax = plt.subplots(figsize=(6.4, 4.4))
    ax.plot(
        np.where(mask, xs, np.nan),
        np.where(mask, ys, np.nan),
        color="#2563eb",
        linewidth=2.2,
        label="運動の軌跡",
    )

    valid_idx = np.flatnonzero(mask)
    ax.scatter(
        [xs[valid_idx[0]]],
        [ys[valid_idx[0]]],
        color="#059669",
        s=45,
        zorder=6,
        label=f"始点 ({param_name}={t_min:g})",
    )
    ax.scatter(
        [xs[valid_idx[-1]]],
        [ys[valid_idx[-1]]],
        color="#dc2626",
        s=45,
        zorder=6,
        label=f"終点 ({param_name}={t_max:g})",
    )

    if show_velocity_vectors and n_vec > 0:
        t_sample = np.linspace(t_min, t_max, n_vec + 2)[1:-1]
        x_s = _eval_sympy_on_numpy(x_expr, {param_name: t_sample}, t_sample.shape)
        y_s = _eval_sympy_on_numpy(y_expr, {param_name: t_sample}, t_sample.shape)
        vx_s = _eval_sympy_on_numpy(vx_expr, {param_name: t_sample}, t_sample.shape)
        vy_s = _eval_sympy_on_numpy(vy_expr, {param_name: t_sample}, t_sample.shape)
        ok = np.isfinite(x_s) & np.isfinite(y_s) & np.isfinite(vx_s) & np.isfinite(vy_s)
        if np.any(ok):
            ax.quiver(
                x_s[ok],
                y_s[ok],
                vx_s[ok],
                vy_s[ok],
                color="#d97706",
                angles="xy",
                scale_units="xy",
                width=0.005,
                label="速度ベクトル v(t)",
                zorder=5,
            )

    ax.axhline(0.0, color="#94a3b8", linewidth=0.8)
    ax.axvline(0.0, color="#94a3b8", linewidth=0.8)
    ax.grid(True, linestyle="--", alpha=0.35)
    ax.set_xlabel(x_label or f"x({param_name})")
    ax.set_ylabel(y_label or f"y({param_name})")
    ax.set_title(title or "Motion Trajectory (運動の軌跡)")
    ax.legend(loc="best", fontsize=8.5)

    caption = title or f"運動の軌跡 ({x_expression}, {y_expression})"
    artifact = _save_figure_confined(fig, artifacts_dir, fmt, caption)
    return make_success_response(
        "plot_trajectory",
        {
            "x_expression": str(x_expr),
            "y_expression": str(y_expr),
            "vx_expression": str(vx_expr),
            "vy_expression": str(vy_expr),
            "t_range": [t_min, t_max],
            "artifact_path": artifact.relative_path,
            "format": fmt,
        },
        verification=VerificationPayload(
            status="passed",
            target="軌跡および記号速度ベクトルの算出",
            assumptions=[f"{param_name} in [{t_min:g}, {t_max:g}]"],
            method="SymPy symbolic diff(x,y) + AST-safe NumPy trajectory evaluation",
        ),
        artifacts=[artifact],
    )
