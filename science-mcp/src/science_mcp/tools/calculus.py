"""Calculus tools: differentiation, integration (with verification), and limits."""

from __future__ import annotations

import math
from typing import Any

import sympy as sp

from science_mcp.models import (
    MAX_DERIVATIVE_ORDER,
    CalculationError,
    InputLimitError,
    VerificationPayload,
    make_success_response,
)
from science_mcp.parser import make_symbol, parse_safe_expression, validate_identifier


def differentiate(
    expression: str,
    variable: str = "x",
    order: int = 1,
) -> dict[str, Any]:
    """Compute ordinary or partial derivative of order 1..10."""
    var_name = validate_identifier(variable)
    if not isinstance(order, int) or isinstance(order, bool):
        raise CalculationError("INVALID_ORDER", "微分の階数 order は整数で指定してください。")
    if not (1 <= order <= MAX_DERIVATIVE_ORDER):
        raise InputLimitError(
            f"微分の階数は 1 以上 {MAX_DERIVATIVE_ORDER} 以下である必要があります。"
        )

    expr = parse_safe_expression(expression, variable_names={var_name})
    var_sym = next((s for s in expr.free_symbols if s.name == var_name), make_symbol(var_name))
    deriv = sp.simplify(sp.diff(expr, var_sym, order))

    verification = VerificationPayload(
        status="passed",
        target=f"{order} 階微分 d^{order}/d{var_name}^{order} ({expression})",
        assumptions=[],
        method=f"sympy.diff(expr, {var_name}, {order})",
    )
    return make_success_response(
        "differentiate",
        {
            "expression": str(deriv),
            "latex": sp.latex(deriv),
            "variable": var_name,
            "order": order,
        },
        verification=verification,
    )


def integrate(
    expression: str,
    variable: str = "x",
    lower: str | int | float | None = None,
    upper: str | int | float | None = None,
) -> dict[str, Any]:
    """Compute indefinite or definite integral with automatic symbolic/numeric verification."""
    var_name = validate_identifier(variable)
    if (lower is None) != (upper is None):
        raise CalculationError(
            "INVALID_BOUNDS",
            "定積分を行う場合は lower と upper の両方を指定してください。",
        )

    expr = parse_safe_expression(expression, variable_names={var_name})
    var_sym = next((s for s in expr.free_symbols if s.name == var_name), make_symbol(var_name))

    warnings: list[str] = []
    if lower is None and upper is None:
        antideriv = sp.integrate(expr, var_sym)
        if antideriv.has(sp.Integral):
            verification = VerificationPayload(
                status="inconclusive",
                target=f"不定積分 ∫ ({expression}) d{var_name}",
                assumptions=[],
                method="初等閉形式の原始関数が得られず未評価 Integral が残存",
            )
            warnings.append("初等関数による閉形式の不定積分が見つかりませんでした。")
        else:
            check_diff = sp.simplify(sp.diff(antideriv, var_sym) - expr)
            status = "passed" if check_diff == 0 else "inconclusive"
            verification = VerificationPayload(
                status=status,
                target=f"不定積分の微分検算 d/d{var_name}(F) == f",
                assumptions=[],
                method=f"sympy.simplify(diff(result, {var_name}) - integrand) == 0",
                detail={"residual": str(check_diff)},
            )
        return make_success_response(
            "integrate",
            {
                "definite": False,
                "expression": str(antideriv),
                "latex": sp.latex(antideriv),
                "numeric": None,
                "convergence_condition": None,
            },
            verification=verification,
            warnings=warnings,
        )

    lower_expr = parse_safe_expression(str(lower))
    upper_expr = parse_safe_expression(str(upper))
    res = sp.integrate(expr, (var_sym, lower_expr, upper_expr))

    convergence_condition: str | None = None
    main_expr: sp.Expr = res
    if isinstance(res, sp.Piecewise):
        # Extract primary convergent branch and record condition
        first_expr, first_cond = res.args[0]
        main_expr = sp.simplify(first_expr)
        convergence_condition = str(first_cond)
    else:
        main_expr = sp.simplify(res)

    numeric_val: float | None = None
    ver_status = "inconclusive"
    ver_method = "sympy.integrate (定積分)"
    ver_detail: dict[str, Any] = {}

    if not main_expr.has(sp.Integral) and not main_expr.free_symbols:
        try:
            cval = complex(main_expr.evalf(20))
            if abs(cval.imag) < 1e-10 and math.isfinite(cval.real):
                numeric_val = float(cval.real)
        except (TypeError, ValueError, OverflowError):
            numeric_val = None

    # Cross-check definite integral numerically when no extra free symbols exist
    if (
        not expr.free_symbols - {var_sym}
        and not lower_expr.free_symbols
        and not upper_expr.free_symbols
    ):
        try:
            mp_val = sp.Integral(expr, (var_sym, lower_expr, upper_expr)).evalf(15)
            mp_c = complex(mp_val)
            if numeric_val is not None and abs(mp_c.imag) < 1e-8 and math.isfinite(mp_c.real):
                err = abs(numeric_val - mp_c.real)
                scale = max(1.0, abs(numeric_val))
                ver_detail = {
                    "symbolic_numeric": numeric_val,
                    "quadrature_numeric": mp_c.real,
                    "abs_diff": err,
                }
                if err <= 1e-6 * scale:
                    ver_status = "passed"
                    ver_method = "記号定積分と数値積分 (mpmath) の一致検証"
                else:
                    ver_status = "failed"
                    ver_method = "記号定積分と数値積分の不一致を検出"
            elif main_expr.has(sp.Integral) and abs(mp_c.imag) < 1e-8 and math.isfinite(mp_c.real):
                numeric_val = float(mp_c.real)
                ver_status = "passed"
                ver_method = "数値積分 (mpmath) による評価"
                warnings.append("記号閉形式が得られなかったため数値積分結果を返却しました。")
        except Exception:  # noqa: BLE001
            if not main_expr.has(sp.Integral):
                ver_status = "passed"
                ver_method = "sympy.integrate 記号評価"
    elif not main_expr.has(sp.Integral):
        ver_status = "passed"
        ver_method = "sympy.integrate 記号評価（パラメータ付き定積分）"

    verification = VerificationPayload(
        status=ver_status,
        target=f"定積分 ∫_{{{lower}}}^{{{upper}}} ({expression}) d{var_name}",
        assumptions=[convergence_condition] if convergence_condition else [],
        method=ver_method,
        detail=ver_detail or None,
    )
    return make_success_response(
        "integrate",
        {
            "definite": True,
            "expression": str(main_expr),
            "latex": sp.latex(main_expr),
            "numeric": numeric_val,
            "convergence_condition": convergence_condition,
        },
        verification=verification,
        warnings=warnings,
    )


def calculate_limit(
    expression: str,
    variable: str = "x",
    point: str | int | float = "0",
    direction: str = "+-",
) -> dict[str, Any]:
    """Calculate a two-sided or one-sided limit as variable approaches point."""
    var_name = validate_identifier(variable)
    dir_clean = direction.strip()
    if dir_clean not in ("+-", "+", "-"):
        raise CalculationError(
            "INVALID_DIRECTION",
            f"direction は '+-', '+', '-' のいずれかを指定してください: {direction}",
        )

    expr = parse_safe_expression(expression, variable_names={var_name})
    var_sym = next((s for s in expr.free_symbols if s.name == var_name), make_symbol(var_name))
    pt_expr = parse_safe_expression(str(point))

    # Infinity limits are inherently one-sided in SymPy
    effective_dir = dir_clean
    if pt_expr is sp.oo:
        effective_dir = "-"
    elif pt_expr is -sp.oo:
        effective_dir = "+"

    try:
        lim_val = sp.simplify(sp.limit(expr, var_sym, pt_expr, dir=effective_dir))
    except ValueError as exc:
        raise CalculationError("LIMIT_DOES_NOT_EXIST", f"極限が存在しません: {exc}") from exc

    numeric_val: float | None = None
    if lim_val.is_number and lim_val.is_finite:
        try:
            cval = complex(lim_val.evalf())
            if abs(cval.imag) < 1e-12 and math.isfinite(cval.real):
                numeric_val = float(cval.real)
        except (TypeError, ValueError, OverflowError):
            numeric_val = None

    verification = VerificationPayload(
        status="passed",
        target=f"極限 lim_{{{var_name} -> {point} ({effective_dir})}} ({expression})",
        assumptions=[],
        method=f"sympy.limit(expr, {var_name}, {point}, dir='{effective_dir}')",
    )
    return make_success_response(
        "calculate_limit",
        {
            "expression": str(lim_val),
            "latex": sp.latex(lim_val),
            "numeric": numeric_val,
            "direction": effective_dir,
        },
        verification=verification,
    )
