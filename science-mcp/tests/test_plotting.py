"""Tests for Matplotlib plotting tools, UUID filenames, and fixed artifacts dir confinement."""

from __future__ import annotations

import uuid
from pathlib import Path

from science_mcp.runner import execute_tool_isolated
from science_mcp.tools.plotting import plot_data, plot_function, plot_vector_field


def test_plot_function_svg_and_png_uuid_confinement(tmp_path: Path) -> None:
    artifacts_dir = tmp_path / "artifacts"

    svg_res = plot_function(
        ["exp(-t) * (cos(2*t) + sin(2*t)/2)", "exp(-t)"],
        artifacts_dir=artifacts_dir,
        variable="t",
        x_range=[0, 6],
        labels=["x(t)", "envelope"],
        title="Damped Oscillation",
        output_format="svg",
    )
    assert svg_res["ok"] is True
    assert len(svg_res["artifacts"]) == 1
    art = svg_res["artifacts"][0]
    assert art["format"] == "svg"
    assert art["mime_type"] == "image/svg+xml"
    assert art["relative_path"].startswith("artifacts/")
    # Verify filename stem is a valid UUIDv4
    stem = Path(art["filename"]).stem
    parsed_uuid = uuid.UUID(stem)
    assert parsed_uuid.version == 4

    out_file = artifacts_dir / art["filename"]
    assert out_file.is_file()
    content = out_file.read_text(encoding="utf-8")
    assert "<svg" in content

    png_res = plot_function(
        ["sin(x)"],
        artifacts_dir=artifacts_dir,
        variable="x",
        x_range=[-3.14, 3.14],
        output_format="png",
    )
    png_art = png_res["artifacts"][0]
    png_file = artifacts_dir / png_art["filename"]
    assert png_file.is_file()
    assert png_file.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")


def test_plot_data_and_vector_field(tmp_path: Path) -> None:
    artifacts_dir = tmp_path / "artifacts"

    data_res = plot_data(
        [0, 1, 2, 3, 4],
        [0, 1, 4, 9, 16],
        artifacts_dir=artifacts_dir,
        kind="both",
        title="Measured Curve",
    )
    assert data_res["ok"] is True
    assert (artifacts_dir / data_res["artifacts"][0]["filename"]).is_file()

    vec_res = plot_vector_field(
        "-y",
        "x",
        artifacts_dir=artifacts_dir,
        x_range=[-2, 2],
        y_range=[-2, 2],
        grid_size=10,
        title="Rotational Field",
    )
    assert vec_res["ok"] is True
    assert (artifacts_dir / vec_res["artifacts"][0]["filename"]).is_file()


def test_runner_strips_injected_artifacts_dir_override(tmp_path: Path) -> None:
    """Even if a caller tries passing artifacts_dir in kwargs, runner enforces fixed dir."""
    fixed_dir = tmp_path / "allowed_artifacts"
    evil_dir = tmp_path / "escape_dir"
    evil_dir.mkdir()

    res = execute_tool_isolated(
        "plot_function",
        {
            "expressions": ["x**2"],
            "artifacts_dir": str(evil_dir),
        },
        artifacts_dir=fixed_dir,
        timeout_seconds=15.0,
    )
    assert res["ok"] is True
    assert list(evil_dir.iterdir()) == []
    assert len(list(fixed_dir.glob("*.svg"))) == 1
