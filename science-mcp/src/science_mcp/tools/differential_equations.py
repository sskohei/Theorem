"""Ordinary differential equation solver with initial/boundary conditions and verification."""

from __future__ import annotations

from typing import Any

import sympy as sp

from science_mcp.models import CalculationError, VerificationPayload, make_success_response
from science_mcp.parser import (
    make_symbol,
    parse_safe_equation,
    parse_safe_expression,
    validate_identifier,
)


def solve_ode(
    equation: str,
    func_name: str = "y",
    variable: str = "x",
    ics: dict[str, str | int | float] | None = None,
) -> dict[str, Any]:
    """Solve an ODE symbolically with optional initial/boundary conditions and substitution check.

    Example equation:
        "Derivative(y(t), t, 2) + 2*Derivative(y(t), t) + 5*y(t) = 0"
    Example ics:
        {"y(0)": "1", "Derivative(y(t), t).subs(t, 0)" -> use "y'(0)": "0" or "dy/dt(0)": "0"}
    Supported ic keys:
        - "y(0)" -> y(0) = val
        - "y'(0)" or "Derivative(y(t), t)|0" -> y'(0) = val
        - "y''(0)" -> y''(0) = val
    """
    fn_ident = validate_identifier(func_name)
    var_ident = validate_identifier(variable)
    if fn_ident == var_ident:
        raise CalculationError(
            "INVALID_ODE", "未知関数名と独立変数名は異なる識別子にしてください。"
        )

    func_cls = sp.Function(fn_ident)
    lhs, rhs = parse_safe_equation(
        equation,
        variable_names={var_ident},
        allowed_unknown_functions={fn_ident: func_cls},
        allow_derivative=True,
    )

    var_sym = next(
        (s for s in (lhs - rhs).free_symbols if s.name == var_ident),
        make_symbol(var_ident),
    )
    target_func = func_cls(var_sym)
    ode_eq = sp.Eq(lhs, rhs)

    parsed_ics: dict[sp.Expr, sp.Expr] | None = None
    ic_labels: list[str] = []
    if ics:
        parsed_ics = {}
        for raw_key, raw_val in ics.items():
            key = str(raw_key).strip()
            val_expr = parse_safe_expression(str(raw_val))
            ic_lhs = _parse_ic_key(key, fn_ident, func_cls, var_sym)
            parsed_ics[ic_lhs] = val_expr
            ic_labels.append(f"{key} = {raw_val}")

    try:
        sol = sp.dsolve(ode_eq, func=target_func, ics=parsed_ics)
    except Exception as exc:  # noqa: BLE001
        raise CalculationError(
            "ODE_SOLVE_FAILED", f"常微分方程式の求解に失敗しました: {exc}"
        ) from exc

    sol_list = sol if isinstance(sol, list) else [sol]
    formatted: list[dict[str, Any]] = []
    verified_all = len(sol_list) > 0

    for item in sol_list:
        simplified = (
            sp.Eq(item.lhs, sp.simplify(item.rhs)) if isinstance(item, sp.Equality) else item
        )
        try:
            is_valid, residual = sp.checkodesol(ode_eq, simplified, func=target_func)
            if not is_valid:
                verified_all = False
            res_str = str(residual)
        except Exception:  # noqa: BLE001
            verified_all = False
            res_str = "checkodesol_unavailable"

        rhs_expr = simplified.rhs if isinstance(simplified, sp.Equality) else simplified
        formatted.append(
            {
                "equation": str(simplified),
                "rhs_expression": str(rhs_expr),
                "latex": sp.latex(simplified),
                "rhs_latex": sp.latex(rhs_expr),
                "check_residual": res_str,
            }
        )

    verification = VerificationPayload(
        status="passed" if verified_all else "inconclusive",
        target=f"常微分方程式 {equation} の解検算",
        assumptions=ic_labels,
        method="sympy.dsolve + sympy.checkodesol 再代入検算",
        detail={"solution_count": len(formatted)},
    )
    return make_success_response(
        "solve_ode",
        {
            "function": f"{fn_ident}({var_ident})",
            "solutions": formatted,
            "primary_expression": formatted[0]["rhs_expression"] if formatted else None,
            "primary_latex": formatted[0]["latex"] if formatted else None,
        },
        verification=verification,
    )


def _parse_ic_key(
    key: str,
    fn_ident: str,
    func_cls: sp.FunctionClass,
    var_sym: sp.Symbol,
) -> sp.Expr:
    """Parse initial/boundary condition key.

    Supported formats:
    - 'y(0)', \"y'(0)\", \"y''(0)\"
    - 'Subs(Derivative(y(t), t), t, 0)' or 'Subs(Derivative(y(t), t, 2), t, 0)'
    """
    if key.startswith("Subs(") and key.endswith(")"):
        inner = key[len("Subs(") : -1].strip()
        # Extract trailing `, <var>, <point>` from Subs(Derivative(...), var, point)
        # Split from the right twice at top level (point & var are simple expressions)
        parts = inner.rsplit(",", 2)
        if len(parts) == 3:
            deriv_part = parts[0].strip()
            point_part = parts[2].strip()
            deriv_expr = parse_safe_expression(
                deriv_part,
                variable_names={var_sym.name},
                allowed_unknown_functions={fn_ident: func_cls},
                allow_derivative=True,
            )
            point_expr = parse_safe_expression(point_part)
            return deriv_expr.subs(var_sym, point_expr)

    if not key.startswith(fn_ident):
        raise CalculationError(
            "INVALID_IC_KEY",
            f"初期条件キー '{key}' は関数名 '{fn_ident}' で始めてください"
            f"（例: {fn_ident}(0), {fn_ident}'(0)）。",
        )
    rest = key[len(fn_ident) :].strip()
    deriv_order = 0
    while rest.startswith("'"):
        deriv_order += 1
        rest = rest[1:].strip()

    if deriv_order > 4:
        raise CalculationError(
            "INVALID_IC_KEY", "初期条件の微分階数は 4 階以下で指定してください。"
        )
    if not (rest.startswith("(") and rest.endswith(")")):
        raise CalculationError(
            "INVALID_IC_KEY",
            f"初期条件キーの形式が不正です: '{key}'（例: {fn_ident}(0), {fn_ident}'(0)）。",
        )
    point_str = rest[1:-1].strip()
    point_expr = parse_safe_expression(point_str)

    if deriv_order == 0:
        return func_cls(point_expr)
    return sp.diff(func_cls(var_sym), var_sym, deriv_order).subs(var_sym, point_expr)
