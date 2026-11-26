"""Tests for calculus tools (differentiate, integrate, calculate_limit) and solve_ode."""

from __future__ import annotations

import math

import sympy as sp

from science_mcp.parser import parse_safe_expression
from science_mcp.tools.calculus import calculate_limit, differentiate, integrate
from science_mcp.tools.differential_equations import solve_ode


def test_differentiate_high_order_and_partial() -> None:
    res = differentiate("x**3 * sin(x)", variable="x", order=1)
    assert res["ok"] is True
    assert res["verification"]["status"] == "passed"
    got = parse_safe_expression(res["result"]["expression"])
    expected = parse_safe_expression("3*x**2*sin(x) + x**3*cos(x)")
    assert sp.simplify(got - expected) == 0

    partial = differentiate("x**2 * y**3 + exp(x*y)", variable="y", order=2)
    assert partial["ok"] is True
    got_p = parse_safe_expression(partial["result"]["expression"])
    expected_p = parse_safe_expression("6*x**2*y + x**2*exp(x*y)")
    assert sp.simplify(got_p - expected_p) == 0


def test_integrate_indefinite_and_definite_with_verification() -> None:
    indef = integrate("x**2 + cos(x)", variable="x")
    assert indef["ok"] is True
    assert indef["verification"]["status"] == "passed"
    assert "x**3/3" in indef["result"]["expression"]

    gaussian = integrate("exp(-x**2)", variable="x", lower=0, upper="oo")
    assert gaussian["ok"] is True
    assert gaussian["verification"]["status"] == "passed"
    assert "sqrt(pi)/2" in gaussian["result"]["expression"]
    assert math.isclose(gaussian["result"]["numeric"], math.sqrt(math.pi) / 2.0, rel_tol=1e-9)


def test_calculate_limit_two_sided_and_one_sided() -> None:
    sinc = calculate_limit("sin(x)/x", variable="x", point=0)
    assert sinc["ok"] is True
    assert sinc["result"]["expression"] == "1"

    one_sided = calculate_limit("1/x", variable="x", point=0, direction="+")
    assert one_sided["ok"] is True
    assert one_sided["result"]["expression"] == "oo"


def test_solve_ode_damped_oscillator_with_initial_conditions() -> None:
    # Damped harmonic oscillator: y'' + 2*y' + 5*y = 0, y(0) = 1, y'(0) = 0
    # Exact solution: y(t) = exp(-t) * (cos(2*t) + sin(2*t)/2)
    res = solve_ode(
        "Derivative(y(t), t, 2) + 2*Derivative(y(t), t) + 5*y(t) = 0",
        func_name="y",
        variable="t",
        ics={"y(0)": "1", "y'(0)": "0"},
    )
    assert res["ok"] is True
    assert res["verification"]["status"] == "passed"
    primary = res["result"]["primary_expression"]
    assert "exp(-t)" in primary
    assert "cos(2*t)" in primary
    assert "sin(2*t)" in primary

    # Also accept diff() alias and SymPy Subs(Derivative(...), t, 0) IC format
    res_alt = solve_ode(
        "diff(x(t), t, 2) + 2*diff(x(t), t) + 5*x(t) = 0",
        func_name="x",
        variable="t",
        ics={"x(0)": 1, "Subs(Derivative(x(t), t), t, 0)": 0},
    )
    assert res_alt["ok"] is True
    assert res_alt["verification"]["status"] == "passed"
    assert "exp(-t)" in res_alt["result"]["primary_expression"]
