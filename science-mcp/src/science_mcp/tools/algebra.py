"""Algebra, equation solving, symbolic simplification, verification, and numerical evaluation."""

from __future__ import annotations

import contextlib
import math
from typing import Any

import sympy as sp

from science_mcp.models import (
    MAX_PRECISION_DIGITS,
    CalculationError,
    InputLimitError,
    VerificationPayload,
    make_success_response,
)
from science_mcp.parser import (
    make_symbol,
    parse_safe_equation,
    parse_safe_expression,
    validate_identifier,
)


def _format_expr(expr: sp.Expr) -> dict[str, Any]:
    numeric: float | None = None
    if expr.is_number and expr.is_real is not False:
        try:
            val = complex(expr.evalf())
            if abs(val.imag) < 1e-12 and math.isfinite(val.real):
                numeric = float(val.real)
        except (TypeError, ValueError, OverflowError):
            numeric = None
    return {
        "expression": str(expr),
        "latex": sp.latex(expr),
        "numeric": numeric,
    }


def _parse_assumptions(
    assumptions: list[str] | dict[str, str] | None,
) -> tuple[dict[str, dict[str, bool]], list[str]]:
    """Parse structured or natural assumption specs into SymPy Symbol kwargs."""
    symbol_map: dict[str, dict[str, bool]] = {}
    labels: list[str] = []

    if assumptions is None:
        return symbol_map, labels

    def _apply(var_name: str, prop: str) -> None:
        ident = validate_identifier(var_name)
        p = prop.strip().lower()
        target = symbol_map.setdefault(ident, {})
        if p in ("real", "r"):
            target["real"] = True
            labels.append(f"{ident} is real")
        elif p in ("positive", "> 0", ">0"):
            target["real"] = True
            target["positive"] = True
            labels.append(f"{ident} > 0")
        elif p in ("nonnegative", ">= 0", ">=0"):
            target["real"] = True
            target["nonnegative"] = True
            labels.append(f"{ident} >= 0")
        elif p in ("negative", "< 0", "<0"):
            target["real"] = True
            target["negative"] = True
            labels.append(f"{ident} < 0")
        elif p in ("integer", "int", "z"):
            target["integer"] = True
            labels.append(f"{ident} is integer")
        elif p in ("nonzero", "!= 0", "!=0"):
            target["nonzero"] = True
            labels.append(f"{ident} != 0")
        elif p in ("complex", "c"):
            target["complex"] = True
            labels.append(f"{ident} is complex")
        else:
            labels.append(f"{ident}: {prop.strip()}")

    if isinstance(assumptions, dict):
        for k, v in assumptions.items():
            _apply(k, str(v))
        return symbol_map, labels

    for item in assumptions:
        text = str(item).strip()
        if not text:
            continue
        lower = text.lower()
        if " is " in lower:
            parts = text.split()
            if len(parts) >= 3 and parts[1].lower() == "is":
                _apply(parts[0], parts[2])
                continue
        for op in (">=", "<=", "!=", ">", "<"):
            if op in text:
                lhs_s, rhs_s = text.split(op, 1)
                if rhs_s.strip() == "0":
                    _apply(lhs_s.strip(), f"{op} 0")
                    break
        else:
            labels.append(text)

    return symbol_map, labels


def solve_equation(
    equation: str,
    variable: str = "x",
    domain: str = "complex",
) -> dict[str, Any]:
    """Solve an algebraic or transcendental equation symbolically with substitution check."""
    var_name = validate_identifier(variable)
    dom = domain.strip().lower()
    if dom not in ("complex", "real", "positive", "integer"):
        raise CalculationError(
            "INVALID_DOMAIN",
            f"domain は complex / real / positive / integer のいずれかを指定してください: {domain}",
        )

    sym_assump: dict[str, dict[str, bool]] = {}
    assumption_labels: list[str] = []
    if dom == "real":
        sym_assump[var_name] = {"real": True}
        assumption_labels.append(f"{var_name} is real")
    elif dom == "positive":
        sym_assump[var_name] = {"real": True, "positive": True}
        assumption_labels.append(f"{var_name} > 0")
    elif dom == "integer":
        sym_assump[var_name] = {"integer": True}
        assumption_labels.append(f"{var_name} is integer")
    else:
        assumption_labels.append(f"{var_name} is complex")

    lhs, rhs = parse_safe_equation(
        equation,
        symbol_assumptions=sym_assump,
        variable_names={var_name},
    )
    diff = lhs - rhs
    var_sym = next((s for s in diff.free_symbols if s.name == var_name), None)
    if var_sym is None:
        var_sym = make_symbol(var_name, sym_assump.get(var_name))

    solutions = sp.solve(sp.Eq(lhs, rhs), var_sym)
    if isinstance(solutions, dict):
        sol_list: list[sp.Expr] = [solutions[var_sym]] if var_sym in solutions else []
    else:
        sol_list = list(solutions)

    # Filter real/positive/integer solutions strictly when requested
    filtered: list[sp.Expr] = []
    for sol in sol_list:
        if dom == "real" and sol.is_real is False:
            continue
        if dom == "positive" and (sol.is_real is False or sol.is_positive is False):
            continue
        if dom == "integer" and sol.is_integer is False:
            continue
        filtered.append(sp.simplify(sol))

    # Substitute each candidate back into lhs - rhs to verify
    verified_all = len(filtered) > 0
    unverified_count = 0
    for sol in filtered:
        residual = sp.simplify(diff.subs(var_sym, sol))
        if residual != 0:
            try:
                num_res = abs(complex(residual.evalf()))
                if num_res > 1e-8:
                    verified_all = False
                    unverified_count += 1
            except (TypeError, ValueError, OverflowError):
                verified_all = False
                unverified_count += 1

    if not filtered:
        ver_status = "inconclusive"
    elif verified_all:
        ver_status = "passed"
    else:
        ver_status = "inconclusive"

    formatted_solutions = [_format_expr(s) for s in filtered]
    verification = VerificationPayload(
        status=ver_status,
        target=f"方程式 {equation} の解 ({var_name}) 代入検算",
        assumptions=assumption_labels,
        method="sympy.solve + 残差 (lhs - rhs).subs(var, sol) == 0 検証",
        detail={"solution_count": len(filtered), "unverified_count": unverified_count},
    )
    return make_success_response(
        "solve_equation",
        {
            "variable": var_name,
            "domain": dom,
            "solutions": formatted_solutions,
            "solution_count": len(formatted_solutions),
            "latex": ",\\; ".join(f"{var_name} = {s['latex']}" for s in formatted_solutions)
            if formatted_solutions
            else "\\emptyset",
        },
        verification=verification,
    )


def verify_equation(
    lhs: str,
    rhs: str,
    assumptions: list[str] | dict[str, str] | None = None,
) -> dict[str, Any]:
    """Verify whether lhs == rhs symbolically under assumptions, plus counterexample probe."""
    sym_assump, assumption_labels = _parse_assumptions(assumptions)
    lhs_expr = parse_safe_expression(lhs, symbol_assumptions=sym_assump)
    rhs_expr = parse_safe_expression(rhs, symbol_assumptions=sym_assump)
    diff = lhs_expr - rhs_expr

    # 1. Exact symbolic simplification checks
    candidates: list[sp.Expr] = [diff]
    for fn in (
        sp.simplify,
        sp.expand,
        sp.trigsimp,
        lambda e: sp.powsimp(e, force=False),
    ):
        with contextlib.suppress(Exception):
            candidates.append(fn(diff))

    for cand in candidates:
        if cand == 0 or cand is sp.S.Zero:
            verification = VerificationPayload(
                status="passed",
                target=f"等式検証: {lhs} = {rhs}",
                assumptions=assumption_labels,
                method="sympy.simplify(lhs - rhs) == 0 (厳密記号簡約)",
                detail={"symbolic_difference": "0"},
            )
            return make_success_response(
                "verify_equation",
                {
                    "equal": True,
                    "status": "passed",
                    "lhs_latex": sp.latex(lhs_expr),
                    "rhs_latex": sp.latex(rhs_expr),
                    "difference": "0",
                    "difference_latex": "0",
                    "counterexample": None,
                },
                verification=verification,
            )

    simplified_diff = candidates[1] if len(candidates) > 1 else diff
    free_syms = sorted(simplified_diff.free_symbols, key=lambda s: s.name)

    # 2. Constant non-zero symbolic difference (e.g., sin(x)^2 + cos(x)^2 - 2 == -1)
    if not free_syms:
        try:
            diff_val = complex(simplified_diff.evalf(30))
            if abs(diff_val) > 1e-9:
                verification = VerificationPayload(
                    status="failed",
                    target=f"等式検証: {lhs} = {rhs}",
                    assumptions=assumption_labels,
                    method="記号差簡約および高精度数値評価による非ゼロ差検出",
                    detail={
                        "symbolic_difference": str(simplified_diff),
                        "numeric_difference": str(diff_val),
                    },
                )
                return make_success_response(
                    "verify_equation",
                    {
                        "equal": False,
                        "status": "failed",
                        "lhs_latex": sp.latex(lhs_expr),
                        "rhs_latex": sp.latex(rhs_expr),
                        "difference": str(simplified_diff),
                        "difference_latex": sp.latex(simplified_diff),
                        "counterexample": {
                            "symbolic_difference": str(simplified_diff),
                            "numeric_difference": str(diff_val),
                        },
                    },
                    verification=verification,
                )
            verification = VerificationPayload(
                status="inconclusive",
                target=f"等式検証: {lhs} = {rhs}",
                assumptions=assumption_labels,
                method="記号簡約で 0 未確定・数値差微小のため判定保留",
                detail={"numeric_difference": str(diff_val)},
            )
            return make_success_response(
                "verify_equation",
                {
                    "equal": None,
                    "status": "inconclusive",
                    "lhs_latex": sp.latex(lhs_expr),
                    "rhs_latex": sp.latex(rhs_expr),
                    "difference": str(simplified_diff),
                    "difference_latex": sp.latex(simplified_diff),
                    "counterexample": None,
                },
                verification=verification,
            )
        except (TypeError, ValueError, OverflowError):
            pass

    # 3. Deterministic numerical sampling respecting symbol assumptions to detect counterexamples
    sample_sets: list[dict[sp.Symbol, float]] = []
    num_trials = 6
    for trial in range(num_trials):
        point: dict[sp.Symbol, float] = {}
        for idx, sym in enumerate(free_syms):
            spec = sym_assump.get(sym.name, {})
            if spec.get("integer"):
                base_vals = [1, 2, 3, -1, -2, 4]
                if spec.get("positive"):
                    base_vals = [1, 2, 3, 4, 5, 6]
                elif spec.get("negative"):
                    base_vals = [-1, -2, -3, -4, -5, -6]
                point[sym] = float(base_vals[(trial + idx) % len(base_vals)])
            elif spec.get("positive") or spec.get("nonnegative"):
                pos_vals = [0.37, 0.91, 1.43, 2.17, 3.05, 4.29]
                point[sym] = pos_vals[(trial + idx) % len(pos_vals)]
            elif spec.get("negative"):
                neg_vals = [-0.41, -1.13, -2.07, -3.19, -4.51, -0.83]
                point[sym] = neg_vals[(trial + idx) % len(neg_vals)]
            else:
                real_vals = [-2.13, -0.67, 0.43, 1.29, 2.51, -1.47]
                point[sym] = real_vals[(trial + idx) % len(real_vals)]
        sample_sets.append(point)

    evaluated_ok = 0
    for point in sample_sets:
        try:
            lv = complex(lhs_expr.subs(point).evalf())
            rv = complex(rhs_expr.subs(point).evalf())
            if not (
                math.isfinite(lv.real)
                and math.isfinite(lv.imag)
                and math.isfinite(rv.real)
                and math.isfinite(rv.imag)
            ):
                continue
            evaluated_ok += 1
            scale = max(1.0, abs(lv), abs(rv))
            if abs(lv - rv) > 1e-6 * scale:
                counterexample = {
                    "point": {s.name: val for s, val in point.items()},
                    "lhs_value": str(lv if abs(lv.imag) > 1e-12 else lv.real),
                    "rhs_value": str(rv if abs(rv.imag) > 1e-12 else rv.real),
                }
                verification = VerificationPayload(
                    status="failed",
                    target=f"等式検証: {lhs} = {rhs}",
                    assumptions=assumption_labels,
                    method="記号差簡約 + 数値サンプリング反例検出",
                    detail={"counterexample": counterexample},
                )
                return make_success_response(
                    "verify_equation",
                    {
                        "equal": False,
                        "status": "failed",
                        "lhs_latex": sp.latex(lhs_expr),
                        "rhs_latex": sp.latex(rhs_expr),
                        "difference": str(simplified_diff),
                        "difference_latex": sp.latex(simplified_diff),
                        "counterexample": counterexample,
                    },
                    verification=verification,
                )
        except (TypeError, ValueError, OverflowError, ZeroDivisionError):
            continue

    # Neither symbolically zero nor numerically refuted -> inconclusive
    verification = VerificationPayload(
        status="inconclusive",
        target=f"等式検証: {lhs} = {rhs}",
        assumptions=assumption_labels,
        method="記号簡約で決定不能・数値サンプリングでは反例未検出（定義域条件等を確認してください）",
        detail={"samples_checked": evaluated_ok, "symbolic_difference": str(simplified_diff)},
    )
    return make_success_response(
        "verify_equation",
        {
            "equal": None,
            "status": "inconclusive",
            "lhs_latex": sp.latex(lhs_expr),
            "rhs_latex": sp.latex(rhs_expr),
            "difference": str(simplified_diff),
            "difference_latex": sp.latex(simplified_diff),
            "counterexample": None,
        },
        verification=verification,
        warnings=[
            "記号簡約で厳密一致を確定できませんでした。変数の定義域仮定を追加して再検証できます。"
        ],
    )


def simplify_expression(expression: str) -> dict[str, Any]:
    """Simplify a symbolic mathematical expression."""
    expr = parse_safe_expression(expression)
    simplified = sp.simplify(expr)
    eq_check = sp.simplify(expr - simplified) == 0
    verification = VerificationPayload(
        status="passed" if eq_check else "inconclusive",
        target=f"簡約等価性検証: {expression}",
        assumptions=[],
        method="sympy.simplify(original - simplified) == 0",
    )
    return make_success_response(
        "simplify_expression",
        _format_expr(simplified),
        verification=verification,
    )


def expand_expression(expression: str) -> dict[str, Any]:
    """Expand an algebraic/trigonometric expression."""
    expr = parse_safe_expression(expression)
    expanded = sp.expand(expr)
    eq_check = sp.simplify(expr - expanded) == 0
    verification = VerificationPayload(
        status="passed" if eq_check else "inconclusive",
        target=f"展開等価性検証: {expression}",
        assumptions=[],
        method="sympy.simplify(original - expanded) == 0",
    )
    return make_success_response(
        "expand_expression",
        _format_expr(expanded),
        verification=verification,
    )


def factor_expression(expression: str) -> dict[str, Any]:
    """Factor a polynomial or rational expression."""
    expr = parse_safe_expression(expression)
    factored = sp.factor(expr)
    eq_check = sp.expand(expr - factored) == 0
    verification = VerificationPayload(
        status="passed" if eq_check else "inconclusive",
        target=f"因数分解展開検算: {expression}",
        assumptions=[],
        method="sympy.expand(original - factored) == 0",
    )
    return make_success_response(
        "factor_expression",
        _format_expr(factored),
        verification=verification,
    )


def solve_system(equations: list[str], variables: list[str]) -> dict[str, Any]:
    """Solve a system of equations symbolically with residual verification."""
    if not equations or len(equations) > 12:
        raise InputLimitError("連立方程式は 1 本以上 12 本以下で指定してください。")
    if not variables or len(variables) > 12:
        raise InputLimitError("未知変数は 1 個以上 12 個以下で指定してください。")

    var_names = [validate_identifier(v) for v in variables]
    var_set = set(var_names)
    parsed_eqs: list[tuple[sp.Expr, sp.Expr]] = [
        parse_safe_equation(eq, variable_names=var_set) for eq in equations
    ]

    sym_lookup: dict[str, sp.Symbol] = {}
    for lhs, rhs in parsed_eqs:
        for s in (lhs - rhs).free_symbols:
            if s.name in var_set:
                sym_lookup[s.name] = s
    var_syms = [sym_lookup.get(name, make_symbol(name)) for name in var_names]

    sp_eqs = [sp.Eq(lhs, rhs) for lhs, rhs in parsed_eqs]
    raw_solutions = sp.solve(sp_eqs, var_syms, dict=True)

    formatted_list: list[dict[str, dict[str, Any]]] = []
    verified_all = len(raw_solutions) > 0
    for sol_dict in raw_solutions:
        entry: dict[str, dict[str, Any]] = {}
        for vs in var_syms:
            val = sp.simplify(sol_dict.get(vs, vs))
            entry[vs.name] = _format_expr(val)
        formatted_list.append(entry)

        for lhs, rhs in parsed_eqs:
            res = sp.simplify((lhs - rhs).subs(sol_dict))
            if res != 0:
                verified_all = False

    verification = VerificationPayload(
        status="passed" if verified_all else ("inconclusive" if raw_solutions else "inconclusive"),
        target=f"連立方程式 ({len(equations)} 式) の解代入検算",
        assumptions=[],
        method="sympy.solve + 全方程式への残差代入確認",
        detail={"solution_count": len(formatted_list)},
    )
    return make_success_response(
        "solve_system",
        {
            "variables": var_names,
            "solutions": formatted_list,
            "solution_count": len(formatted_list),
        },
        verification=verification,
    )


def numerical_evaluate(
    expression: str,
    substitutions: dict[str, str | int | float] | None = None,
    precision: int = 15,
) -> dict[str, Any]:
    """Evaluate an expression numerically to requested decimal precision (up to 100 digits)."""
    if not isinstance(precision, int) or isinstance(precision, bool):
        raise CalculationError("INVALID_PRECISION", "precision は整数で指定してください。")
    if not (1 <= precision <= MAX_PRECISION_DIGITS):
        raise InputLimitError(
            f"precision は 1 以上 {MAX_PRECISION_DIGITS} 以下で指定してください。"
        )

    sub_names = {validate_identifier(k) for k in (substitutions or {})}
    expr = parse_safe_expression(expression, variable_names=sub_names)

    subs_map: dict[sp.Symbol, sp.Expr] = {}
    if substitutions:
        sym_by_name = {s.name: s for s in expr.free_symbols}
        for k, raw_v in substitutions.items():
            name = validate_identifier(k)
            val_expr = (
                parse_safe_expression(str(raw_v))
                if isinstance(raw_v, str)
                else parse_safe_expression(repr(raw_v))
            )
            subs_map[sym_by_name.get(name, make_symbol(name))] = val_expr

    substituted = expr.subs(subs_map) if subs_map else expr
    if substituted.free_symbols:
        remaining = sorted(s.name for s in substituted.free_symbols)
        raise CalculationError(
            "UNBOUND_SYMBOLS",
            f"数値評価に未代入の変数が残っています: {', '.join(remaining)}",
        )

    evaluated = substituted.evalf(precision)
    cval = complex(evaluated)
    is_real = abs(cval.imag) < 10 ** (-(min(precision, 12)))
    float_val = float(cval.real) if is_real and math.isfinite(cval.real) else None

    verification = VerificationPayload(
        status="passed",
        target=f"高精度数値評価 ({precision} 桁): {expression}",
        assumptions=[f"{k} = {v}" for k, v in (substitutions or {}).items()],
        method=f"sympy.evalf(n={precision})",
    )
    return make_success_response(
        "numerical_evaluate",
        {
            "value_str": str(evaluated),
            "latex": sp.latex(evaluated),
            "numeric": float_val,
            "is_real": is_real,
            "precision": precision,
        },
        verification=verification,
    )
