"""Tests for algebra, equation solving, verification (passed/failed/inconclusive), and eval."""

from __future__ import annotations

import math

from science_mcp.tools.algebra import (
    expand_expression,
    factor_expression,
    numerical_evaluate,
    simplify_expression,
    solve_equation,
    solve_system,
    verify_equation,
)


def test_solve_equation_quadratic_and_domain_filter() -> None:
    res = solve_equation("x**2 - 5*x + 6 = 0", variable="x", domain="real")
    assert res["ok"] is True
    assert res["verification"]["status"] == "passed"
    sols = {s["expression"] for s in res["result"]["solutions"]}
    assert sols == {"2", "3"}

    # x**2 + 1 = 0 has no real solutions
    res_no_real = solve_equation("x**2 + 1 = 0", variable="x", domain="real")
    assert res_no_real["ok"] is True
    assert res_no_real["result"]["solution_count"] == 0


def test_verify_equation_passed_failed_and_inconclusive() -> None:
    # 1. Exact trigonometric identity -> passed
    passed = verify_equation("sin(x)**2 + cos(x)**2", "1", assumptions=["x is real"])
    assert passed["ok"] is True
    assert passed["result"]["status"] == "passed"
    assert passed["verification"]["status"] == "passed"

    # 2. Positive assumption resolves sqrt(x**2) == x -> passed
    pos_passed = verify_equation("sqrt(x**2)", "x", assumptions={"x": "positive"})
    assert pos_passed["verification"]["status"] == "passed"

    # 3. False identity -> failed with counterexample
    failed = verify_equation("sin(x)**2 + cos(x)**2", "2", assumptions=["x is real"])
    assert failed["ok"] is True
    assert failed["result"]["status"] == "failed"
    assert failed["verification"]["status"] == "failed"
    assert failed["result"]["counterexample"] is not None

    # 4. Domain-sensitive expression without sign assumption -> failed at negative real sample
    neg_refuted = verify_equation("sqrt(x**2)", "x", assumptions=["x is real"])
    assert neg_refuted["verification"]["status"] == "failed"

    # 5. Conditional identity where symbolic simplification does not reduce to 0 on positive reals
    # but matches numerically -> inconclusive
    inconc = verify_equation(
        "atan(x) + atan(1/x)",
        "pi/2",
        assumptions={"x": "positive"},
    )
    assert inconc["ok"] is True
    assert inconc["verification"]["status"] in ("passed", "inconclusive")


def test_simplify_expand_factor_and_system() -> None:
    simp = simplify_expression("(x**2 - 1)/(x - 1)")
    assert simp["result"]["expression"] == "x + 1"
    assert simp["verification"]["status"] == "passed"

    expd = expand_expression("(x + y)**3")
    assert expd["result"]["expression"] == "x**3 + 3*x**2*y + 3*x*y**2 + y**3"

    fact = factor_expression("x**3 - 1")
    assert fact["result"]["expression"] == "(x - 1)*(x**2 + x + 1)"

    sys_res = solve_system(["2*x + y = 7", "x - y = 2"], ["x", "y"])
    assert sys_res["ok"] is True
    assert sys_res["verification"]["status"] == "passed"
    assert sys_res["result"]["solutions"][0]["x"]["expression"] == "3"
    assert sys_res["result"]["solutions"][0]["y"]["expression"] == "1"


def test_numerical_evaluate_precision() -> None:
    res = numerical_evaluate("sqrt(pi)/2", precision=25)
    assert res["ok"] is True
    assert math.isclose(res["result"]["numeric"], math.sqrt(math.pi) / 2.0, rel_tol=1e-12)
    assert res["result"]["value_str"].startswith("0.886226925452758")
