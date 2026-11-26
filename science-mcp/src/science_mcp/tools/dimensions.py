"""Physical unit conversion and dimensional analysis powered by Pint (SI default)."""

from __future__ import annotations

import math
import re
from typing import Any

import pint
import sympy as sp

from science_mcp.models import (
    CalculationError,
    InputLimitError,
    UnsafeExpressionError,
    VerificationPayload,
    make_success_response,
)
from science_mcp.parser import parse_safe_expression, validate_identifier

_UNIT_STR_RE = re.compile(r"^[A-Za-z0-9_/*^().+\- ]+$")
_MAX_UNIT_LENGTH = 128
_UREG = pint.UnitRegistry()

# Canonical physics dimension notation [M][L][T][I][Θ][N][J]
_DIM_SYMBOLS: dict[str, str] = {
    "[mass]": "M",
    "[length]": "L",
    "[time]": "T",
    "[current]": "I",
    "[temperature]": "Θ",
    "[substance]": "N",
    "[luminosity]": "J",
}


def _validate_unit_string(unit_str: str) -> str:
    if not isinstance(unit_str, str):
        raise UnsafeExpressionError("単位は文字列で指定してください。")
    cleaned = unit_str.strip()
    if not cleaned:
        raise UnsafeExpressionError("空の単位文字列は指定できません。")
    if len(cleaned) > _MAX_UNIT_LENGTH:
        raise InputLimitError(f"単位文字列が上限 ({_MAX_UNIT_LENGTH} 文字) を超えています。")
    if "__" in cleaned or not _UNIT_STR_RE.match(cleaned):
        raise UnsafeExpressionError(f"単位文字列に許可されていない文字が含まれています: {cleaned}")
    return cleaned


def _parse_pint_unit(unit_str: str) -> pint.Unit:
    cleaned = _validate_unit_string(unit_str)
    try:
        return _UREG.Unit(cleaned)
    except Exception as exc:  # noqa: BLE001
        raise CalculationError(
            "INVALID_UNIT", f"未知または不正な単位です: '{cleaned}' ({exc})"
        ) from exc


def _format_physics_dimensionality(dim: pint.util.UnitsContainer) -> str:
    """Format Pint dimensionality into textbook physics notation like [M][L][T]^-2."""
    if not dim:
        return "[1] (無次元)"
    parts: list[str] = []
    ordered = [
        "[mass]",
        "[length]",
        "[time]",
        "[current]",
        "[temperature]",
        "[substance]",
        "[luminosity]",
    ]
    seen: set[str] = set()
    for key in ordered:
        if key in dim and dim[key] != 0:
            seen.add(key)
            sym = _DIM_SYMBOLS[key]
            exp = dim[key]
            parts.append(f"[{sym}]" if exp == 1 else f"[{sym}]^{exp:g}")
    for key, exp in dim.items():
        if key not in seen and exp != 0:
            parts.append(f"{key}" if exp == 1 else f"{key}^{exp:g}")
    return "".join(parts) if parts else "[1] (無次元)"


def convert_units(
    value: float | int | str,
    from_unit: str,
    to_unit: str,
) -> dict[str, Any]:
    """Convert a physical quantity from `from_unit` to `to_unit`, plus base SI representation."""
    try:
        num_val = float(value)
    except (TypeError, ValueError) as exc:
        raise CalculationError("INVALID_VALUE", f"数値として解釈できません: {value}") from exc
    if not math.isfinite(num_val):
        raise CalculationError("INVALID_VALUE", "有限の数値を指定してください。")

    src_unit = _parse_pint_unit(from_unit)
    dst_unit = _parse_pint_unit(to_unit)

    try:
        qty = _UREG.Quantity(num_val, src_unit)
        converted = qty.to(dst_unit)
        si_qty = qty.to_base_units()
    except pint.DimensionalityError as exc:
        raise CalculationError(
            "DIMENSIONALITY_MISMATCH",
            f"次元が一致しないため '{from_unit}' から '{to_unit}' へ変換できません: {exc}",
        ) from exc

    dim_label = _format_physics_dimensionality(converted.dimensionality)
    verification = VerificationPayload(
        status="passed",
        target=f"単位変換: {num_val:g} {from_unit} -> {to_unit}",
        assumptions=["SI 単位系 (Pint UnitRegistry)"],
        method="pint.Quantity.to()",
        detail={"dimensionality": dim_label},
    )
    return make_success_response(
        "convert_units",
        {
            "input_value": num_val,
            "from_unit": str(src_unit),
            "converted_value": float(converted.magnitude),
            "to_unit": str(converted.units),
            "formatted": f"{float(converted.magnitude):.12g} {converted.units}",
            "si_value": float(si_qty.magnitude),
            "si_unit": str(si_qty.units),
            "dimensionality": dim_label,
        },
        verification=verification,
    )


def _eval_expr_dimension(expr: sp.Expr, unit_lookup: dict[str, pint.Quantity]) -> pint.Quantity:
    """Recursively evaluate the physical dimension of a validated SymPy expression."""
    if expr.is_Number or expr in (sp.pi, sp.E, sp.I):
        return _UREG.Quantity(1.0, _UREG.dimensionless)

    if isinstance(expr, sp.Symbol):
        if expr.name not in unit_lookup:
            raise CalculationError(
                "MISSING_SYMBOL_UNIT",
                f"変数 '{expr.name}' の単位が unit_map に指定されていません。",
            )
        return unit_lookup[expr.name]

    if isinstance(expr, sp.Add):
        first = _eval_expr_dimension(expr.args[0], unit_lookup)
        for arg in expr.args[1:]:
            nxt = _eval_expr_dimension(arg, unit_lookup)
            if first.dimensionality != nxt.dimensionality:
                raise CalculationError(
                    "ADDITION_DIMENSION_MISMATCH",
                    f"加算・減算の項どうしで次元が不一致です: "
                    f"{_format_physics_dimensionality(first.dimensionality)} vs "
                    f"{_format_physics_dimensionality(nxt.dimensionality)}",
                )
        return first

    if isinstance(expr, sp.Mul):
        acc = _UREG.Quantity(1.0, _UREG.dimensionless)
        for arg in expr.args:
            acc = acc * _eval_expr_dimension(arg, unit_lookup)
        return acc

    if isinstance(expr, sp.Pow):
        base_q = _eval_expr_dimension(expr.base, unit_lookup)
        exp_q = _eval_expr_dimension(expr.exp, unit_lookup)
        if exp_q.dimensionality != _UREG.dimensionless:
            dim_txt = _format_physics_dimensionality(exp_q.dimensionality)
            raise CalculationError(
                "NON_DIMENSIONLESS_EXPONENT",
                f"指数部は無次元である必要があります（検出次元: {dim_txt}）。",
            )
        if not expr.exp.is_number:
            if base_q.dimensionality != _UREG.dimensionless:
                raise CalculationError(
                    "SYMBOLIC_EXPONENT_ON_DIMENSIONAL_BASE",
                    "次元を持つ量に対する記号指数の次元解析は未確定です。",
                )
            return _UREG.Quantity(1.0, _UREG.dimensionless)
        exp_val = float(expr.exp.evalf())
        return base_q**exp_val

    if isinstance(expr, sp.Function):
        fname = expr.func.__name__
        if fname in ("Abs", "re", "im", "conjugate"):
            return _eval_expr_dimension(expr.args[0], unit_lookup)
        # Transcendental functions require dimensionless arguments and return dimensionless
        for arg in expr.args:
            arg_q = _eval_expr_dimension(arg, unit_lookup)
            if arg_q.dimensionality != _UREG.dimensionless:
                dim_txt = _format_physics_dimensionality(arg_q.dimensionality)
                raise CalculationError(
                    "NON_DIMENSIONLESS_FUNC_ARG",
                    f"関数 {fname} の引数は無次元である必要があります（検出次元: {dim_txt}）。",
                )
        return _UREG.Quantity(1.0, _UREG.dimensionless)

    raise CalculationError(
        "UNSUPPORTED_DIMENSION_NODE", f"次元解析できない式要素です: {type(expr).__name__}"
    )


def check_dimensions(
    lhs_unit: str | None = None,
    rhs_unit: str | None = None,
    lhs_expression: str | None = None,
    rhs_expression: str | None = None,
    unit_map: dict[str, str] | None = None,
) -> dict[str, Any]:
    """Check dimensional consistency between two units or two physical expressions."""
    if lhs_unit is not None and rhs_unit is not None:
        lq = _UREG.Quantity(1.0, _parse_pint_unit(lhs_unit))
        rq = _UREG.Quantity(1.0, _parse_pint_unit(rhs_unit))
        target_desc = f"次元一致確認: [{lhs_unit}] == [{rhs_unit}]"
        assumptions = ["Pint SI 次元解析"]
    elif lhs_expression is not None and rhs_expression is not None:
        units_lookup: dict[str, pint.Quantity] = {}
        assumptions = []
        for sym_name, u_str in (unit_map or {}).items():
            ident = validate_identifier(sym_name)
            parsed_u = _parse_pint_unit(u_str)
            units_lookup[ident] = _UREG.Quantity(1.0, parsed_u)
            assumptions.append(f"[{ident}] = {u_str}")

        var_set = set(units_lookup.keys())
        lhs_expr = parse_safe_expression(lhs_expression, variable_names=var_set)
        rhs_expr = parse_safe_expression(rhs_expression, variable_names=var_set)
        lq = _eval_expr_dimension(lhs_expr, units_lookup)
        rq = _eval_expr_dimension(rhs_expr, units_lookup)
        target_desc = f"物理式の次元整合性: {lhs_expression} = {rhs_expression}"
    else:
        raise CalculationError(
            "INVALID_ARGUMENTS",
            "(lhs_unit, rhs_unit) または "
            "(lhs_expression, rhs_expression, unit_map) を指定してください。",
        )

    lhs_dim_str = _format_physics_dimensionality(lq.dimensionality)
    rhs_dim_str = _format_physics_dimensionality(rq.dimensionality)
    consistent = lq.dimensionality == rq.dimensionality

    verification = VerificationPayload(
        status="passed" if consistent else "failed",
        target=target_desc,
        assumptions=assumptions,
        method="pint.dimensionality 一致検証",
        detail={
            "lhs_dimension": lhs_dim_str,
            "rhs_dimension": rhs_dim_str,
            "consistent": consistent,
        },
    )
    return make_success_response(
        "check_dimensions",
        {
            "consistent": consistent,
            "status": "passed" if consistent else "failed",
            "lhs_dimension": lhs_dim_str,
            "rhs_dimension": rhs_dim_str,
            "lhs_si_unit": str(lq.to_base_units().units),
            "rhs_si_unit": str(rq.to_base_units().units),
        },
        verification=verification,
    )
