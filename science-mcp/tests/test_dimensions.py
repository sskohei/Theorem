"""Tests for Pint unit conversion and physics dimensional analysis."""

from __future__ import annotations

import math

import pytest

from science_mcp.models import CalculationError, UnsafeExpressionError
from science_mcp.tools.dimensions import check_dimensions, convert_units


def test_convert_units_si_and_physics_units() -> None:
    speed = convert_units(72, "km/h", "m/s")
    assert speed["ok"] is True
    assert speed["verification"]["status"] == "passed"
    assert math.isclose(speed["result"]["converted_value"], 20.0, rel_tol=1e-9)

    energy = convert_units(1.0, "eV", "J")
    assert energy["ok"] is True
    assert math.isclose(energy["result"]["converted_value"], 1.602176634e-19, rel_tol=1e-6)


def test_convert_units_rejects_incompatible_and_unsafe_units() -> None:
    with pytest.raises(CalculationError) as exc_info:
        convert_units(10, "m", "kg")
    assert exc_info.value.code == "DIMENSIONALITY_MISMATCH"

    with pytest.raises(UnsafeExpressionError):
        convert_units(1, "__import__('os')", "m")


def test_check_dimensions_mechanics_and_electromagnetism() -> None:
    # F = m * a (Newton's 2nd law)
    mech = check_dimensions(
        lhs_expression="F",
        rhs_expression="m * a",
        unit_map={"F": "N", "m": "kg", "a": "m/s**2"},
    )
    assert mech["ok"] is True
    assert mech["result"]["consistent"] is True
    assert mech["verification"]["status"] == "passed"
    assert "[M][L][T]^-2" in mech["result"]["lhs_dimension"]

    # Lorentz force: F = q * v * B
    lorentz = check_dimensions(
        lhs_expression="F",
        rhs_expression="q * v * B",
        unit_map={"F": "N", "q": "C", "v": "m/s", "B": "T"},
    )
    assert lorentz["result"]["consistent"] is True
    assert lorentz["verification"]["status"] == "passed"

    # Inconsistent equation: energy vs force
    mismatch = check_dimensions(
        lhs_expression="m * v**2 / 2",
        rhs_expression="m * a",
        unit_map={"m": "kg", "v": "m/s", "a": "m/s**2"},
    )
    assert mismatch["ok"] is True
    assert mismatch["result"]["consistent"] is False
    assert mismatch["verification"]["status"] == "failed"

    # Direct unit comparison
    direct = check_dimensions(lhs_unit="J / s", rhs_unit="W")
    assert direct["result"]["consistent"] is True
