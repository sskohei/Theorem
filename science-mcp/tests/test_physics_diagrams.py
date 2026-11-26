"""Unit and security tests for Phase 5 advanced university physics diagram tools."""

from __future__ import annotations

from pathlib import Path

import pytest

from science_mcp.models import CalculationError, InputLimitError, UnsafeExpressionError
from science_mcp.tools.physics_diagrams import (
    plot_circuit_diagram,
    plot_field_lines,
    plot_free_body_diagram,
    plot_potential_energy,
    plot_trajectory,
    plot_wave,
)


def test_plot_free_body_diagram_equilibrium_and_incline(tmp_path: Path) -> None:
    res = plot_free_body_diagram(
        forces=[
            {"label": "mg", "fx": 0.0, "fy": -10.0},
            {"label": "N", "fx": 0.0, "fy": 10.0},
            {"label": "T", "fx": 5.0, "fy": 0.0},
            {"label": "f", "fx": -5.0, "fy": 0.0},
        ],
        body_label="M",
        incline_deg=25.0,
        show_components=True,
        artifacts_dir=tmp_path,
    )
    assert res["ok"] is True
    assert res["result"]["in_equilibrium"] is True
    assert res["verification"]["status"] == "passed"
    out = tmp_path / res["artifacts"][0]["filename"]
    assert out.is_file() and out.suffix == ".svg"


def test_plot_circuit_diagram_series_rlc_and_parallel(tmp_path: Path) -> None:
    series = plot_circuit_diagram(
        elements=[
            {"kind": "voltage_source", "label": "E", "value": "12V"},
            {"kind": "resistor", "label": "R", "value": "100Ω"},
            {"kind": "inductor", "label": "L", "value": "10mH"},
            {"kind": "capacitor", "label": "C", "value": "47μF"},
        ],
        topology="series",
        artifacts_dir=tmp_path,
    )
    assert series["ok"] is True
    assert (tmp_path / series["artifacts"][0]["filename"]).is_file()

    parallel = plot_circuit_diagram(
        elements=[
            {"kind": "voltage_source", "label": "V", "value": "5V"},
            {"kind": "resistor", "label": "R1", "value": "10Ω"},
            {"kind": "capacitor", "label": "C1", "value": "1μF"},
        ],
        topology="parallel",
        output_format="png",
        artifacts_dir=tmp_path,
    )
    assert parallel["ok"] is True
    out_png = tmp_path / parallel["artifacts"][0]["filename"]
    assert out_png.is_file() and out_png.suffix == ".png"


def test_plot_field_lines_electric_dipole_and_magnetic_wire(tmp_path: Path) -> None:
    e_res = plot_field_lines(
        sources=[{"x": -1.0, "y": 0.0, "q": 1.0}, {"x": 1.0, "y": 0.0, "q": -1.0}],
        field_kind="electric",
        show_equipotentials=True,
        artifacts_dir=tmp_path,
    )
    assert e_res["ok"] is True
    assert (tmp_path / e_res["artifacts"][0]["filename"]).is_file()

    b_res = plot_field_lines(
        sources=[{"x": 0.0, "y": 0.0, "current": 2.0}],
        field_kind="magnetic",
        artifacts_dir=tmp_path,
    )
    assert b_res["ok"] is True
    assert (tmp_path / b_res["artifacts"][0]["filename"]).is_file()


def test_plot_wave_superposition_and_envelope(tmp_path: Path) -> None:
    res = plot_wave(
        waves=[
            {"amplitude": 1.0, "wavenumber": 4.0, "omega": 2.0, "phase": 0.0, "label": "y1"},
            {"amplitude": 1.0, "wavenumber": 4.5, "omega": 2.1, "phase": 0.0, "label": "y2"},
        ],
        t=0.5,
        show_superposition=True,
        show_envelope=True,
        artifacts_dir=tmp_path,
    )
    assert res["ok"] is True
    assert res["result"]["max_amplitude"] > 1.5
    assert (tmp_path / res["artifacts"][0]["filename"]).is_file()


def test_plot_potential_energy_detects_turning_points_and_blocks_unsafe_expr(
    tmp_path: Path,
) -> None:
    res = plot_potential_energy(
        expression="x**2 / 2",
        variable="x",
        x_range=[-4, 4],
        energy_levels=[2.0, 4.5],
        artifacts_dir=tmp_path,
    )
    assert res["ok"] is True
    tps = res["result"]["turning_points"]
    assert len(tps) == 2
    assert tps[0] == pytest.approx(-2.0, abs=0.05)
    assert tps[1] == pytest.approx(2.0, abs=0.05)
    assert (tmp_path / res["artifacts"][0]["filename"]).is_file()

    # Security: AST whitelist rejects code injection
    with pytest.raises(UnsafeExpressionError):
        plot_potential_energy(
            expression="__import__('os').system('echo pwn')",
            artifacts_dir=tmp_path,
        )


def test_plot_trajectory_projectile_with_velocity_vectors(tmp_path: Path) -> None:
    res = plot_trajectory(
        x_expression="10 * t",
        y_expression="10 * t - 4.9 * t**2",
        parameter="t",
        t_range=[0, 2.04],
        show_velocity_vectors=True,
        num_vectors=6,
        artifacts_dir=tmp_path,
    )
    assert res["ok"] is True
    assert res["result"]["vx_expression"] == "10"
    assert (tmp_path / res["artifacts"][0]["filename"]).is_file()

    # Limits enforced cleanly
    with pytest.raises(InputLimitError):
        plot_free_body_diagram(
            forces=[{"label": f"F{i}", "fx": 1.0, "fy": 0.0} for i in range(25)],
            artifacts_dir=tmp_path,
        )
    with pytest.raises(CalculationError):
        plot_circuit_diagram(elements=[{"kind": "laser", "label": "X"}], artifacts_dir=tmp_path)
