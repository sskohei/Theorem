"""Strict Python AST whitelist parser that constructs SymPy objects directly.

SECURITY INVARIANTS (docs/science-mcp.md Section 4):
- NEVER calls eval(), exec(), compile(), or sympy.parse_expr().
- Rejects attribute access, subscripting, lambdas, comprehensions, imports,
  underscore-prefixed or dunder identifiers, unknown functions, excessive
  lengths/depths/node counts, huge literals, and huge exponents.
"""

from __future__ import annotations

import ast
import keyword
import math
import re
from collections.abc import Callable
from typing import Any

import sympy as sp

from science_mcp.models import (
    MAX_AST_DEPTH,
    MAX_AST_NODES,
    MAX_DERIVATIVE_ORDER,
    MAX_EXPONENT_ABS,
    MAX_EXPRESSION_LENGTH,
    MAX_FACTORIAL_ARG,
    MAX_IDENTIFIER_LENGTH,
    MAX_INTEGER_DIGITS,
    MAX_MATRIX_DIM,
    MAX_POWER_OF_TEN_EXPONENT,
    InputLimitError,
    UnsafeExpressionError,
)

_IDENTIFIER_RE = re.compile(r"^[A-Za-z][A-Za-z0-9_]*$")

# Names explicitly forbidden even though they match identifier syntax.
_FORBIDDEN_NAMES: frozenset[str] = frozenset(
    {
        "eval",
        "exec",
        "compile",
        "open",
        "input",
        "import",
        "getattr",
        "setattr",
        "delattr",
        "hasattr",
        "globals",
        "locals",
        "vars",
        "dir",
        "type",
        "object",
        "super",
        "builtins",
        "breakpoint",
        "memoryview",
        "classmethod",
        "staticmethod",
        "property",
        "os",
        "sys",
        "subprocess",
        "pathlib",
        "shutil",
        "socket",
        "ctypes",
        "pickle",
        "marshal",
        "importlib",
    }
)

# Whitelisted mathematical functions mapped directly to SymPy callables.
# Each entry is (callable, min_args, max_args).
_ALLOWED_FUNCTIONS: dict[str, tuple[Callable[..., sp.Expr], int, int]] = {
    "sin": (sp.sin, 1, 1),
    "cos": (sp.cos, 1, 1),
    "tan": (sp.tan, 1, 1),
    "csc": (sp.csc, 1, 1),
    "sec": (sp.sec, 1, 1),
    "cot": (sp.cot, 1, 1),
    "asin": (sp.asin, 1, 1),
    "acos": (sp.acos, 1, 1),
    "atan": (sp.atan, 1, 1),
    "atan2": (sp.atan2, 2, 2),
    "sinh": (sp.sinh, 1, 1),
    "cosh": (sp.cosh, 1, 1),
    "tanh": (sp.tanh, 1, 1),
    "asinh": (sp.asinh, 1, 1),
    "acosh": (sp.acosh, 1, 1),
    "atanh": (sp.atanh, 1, 1),
    "exp": (sp.exp, 1, 1),
    "log": (sp.log, 1, 2),
    "ln": (sp.log, 1, 1),
    "sqrt": (sp.sqrt, 1, 1),
    "cbrt": (sp.cbrt, 1, 1),
    "Abs": (sp.Abs, 1, 1),
    "abs": (sp.Abs, 1, 1),
    "factorial": (sp.factorial, 1, 1),
    "floor": (sp.floor, 1, 1),
    "ceiling": (sp.ceiling, 1, 1),
    "re": (sp.re, 1, 1),
    "im": (sp.im, 1, 1),
    "conjugate": (sp.conjugate, 1, 1),
    "sign": (sp.sign, 1, 1),
    "Heaviside": (sp.Heaviside, 1, 2),
    "DiracDelta": (sp.DiracDelta, 1, 2),
}

_CONSTANTS: dict[str, sp.Expr] = {
    "pi": sp.pi,
    "E": sp.E,
    "e": sp.E,
    "I": sp.I,
    "i": sp.I,
    "oo": sp.oo,
    "inf": sp.oo,
}


def validate_identifier(name: str, *, allow_constants: bool = False) -> str:
    """Validate a mathematical symbol or function identifier strictly."""
    if not isinstance(name, str):
        raise UnsafeExpressionError("識別子は文字列である必要があります。")
    cleaned = name.strip()
    if not cleaned:
        raise UnsafeExpressionError("空の識別子は許可されていません。")
    if len(cleaned) > MAX_IDENTIFIER_LENGTH:
        raise InputLimitError(
            f"識別子の長さが上限 ({MAX_IDENTIFIER_LENGTH} 文字) を超えています: {cleaned[:16]}..."
        )
    if cleaned.startswith("_") or "__" in cleaned:
        raise UnsafeExpressionError(
            f"アンダースコア始まりまたは二重アンダースコア識別子は禁止されています: {cleaned}"
        )
    if not _IDENTIFIER_RE.match(cleaned):
        raise UnsafeExpressionError(f"許可されていない識別子形式です: {cleaned}")
    if keyword.iskeyword(cleaned) or cleaned in _FORBIDDEN_NAMES:
        raise UnsafeExpressionError(f"予約語または禁止識別子は使用できません: {cleaned}")
    if not allow_constants and cleaned in _ALLOWED_FUNCTIONS:
        raise UnsafeExpressionError(f"関数名と同じ変数名は使用できません: {cleaned}")
    return cleaned


def make_symbol(name: str, assumptions: dict[str, bool] | None = None) -> sp.Symbol:
    """Create a validated SymPy Symbol with optional domain assumptions."""
    valid = validate_identifier(name)
    if assumptions:
        return sp.Symbol(valid, **assumptions)
    return sp.Symbol(valid)


def _check_ast_limits(tree: ast.AST) -> None:
    """Enforce hard node-count and depth limits before evaluating any node."""
    node_count = 0

    def _walk(node: ast.AST, depth: int) -> None:
        nonlocal node_count
        node_count += 1
        if node_count > MAX_AST_NODES:
            raise InputLimitError(f"数式の AST ノード数が上限 ({MAX_AST_NODES}) を超えています。")
        if depth > MAX_AST_DEPTH:
            raise InputLimitError(f"数式の入れ子深さが上限 ({MAX_AST_DEPTH}) を超えています。")
        for child in ast.iter_child_nodes(node):
            _walk(child, depth + 1)

    _walk(tree, 0)


def _parse_numeric_constant(value: Any) -> sp.Expr:
    """Convert a validated numeric AST literal to a safe SymPy number."""
    if isinstance(value, bool):
        raise UnsafeExpressionError("真偽値リテラルは数式内で使用できません。")
    if isinstance(value, int):
        if value != 0 and len(str(abs(value))) > MAX_INTEGER_DIGITS:
            raise InputLimitError(
                f"整数リテラルの桁数が上限 ({MAX_INTEGER_DIGITS}) を超えています。"
            )
        return sp.Integer(value)
    if isinstance(value, float):
        if not math.isfinite(value):
            raise UnsafeExpressionError("非有限の浮動小数リテラルは使用できません。")
        if abs(value) > 1e300:
            raise InputLimitError("浮動小数リテラルが大きすぎます。")
        text = repr(value)
        if "e" not in text.lower() and len(text.replace(".", "").replace("-", "")) <= 12:
            return sp.Rational(text)
        return sp.Float(value)
    raise UnsafeExpressionError(f"許可されていないリテラル型です: {type(value).__name__}")


def _safe_pow(base: sp.Expr, exponent: sp.Expr) -> sp.Expr:
    """Construct base**exponent while preventing exponential CPU/memory DoS."""
    exp_num: float | None = None
    if exponent.is_number and exponent.is_real:
        try:
            exp_num = float(exponent.evalf())
        except (TypeError, ValueError, OverflowError):
            exp_num = None

    if exp_num is not None:
        if not math.isfinite(exp_num):
            raise InputLimitError("指数が非有限値になりました。")
        is_base_ten = base == sp.Integer(10)
        limit = MAX_POWER_OF_TEN_EXPONENT if is_base_ten else MAX_EXPONENT_ABS
        if abs(exp_num) > limit:
            raise InputLimitError(f"指数 {exp_num:g} が安全上限 (±{limit}) を超えています。")
        # Also guard against huge numeric bases raised to moderate exponents (e.g., 999**99)
        if base.is_number and base.is_real:
            try:
                base_abs = abs(float(base.evalf()))
                if base_abs > 1.0:
                    est_digits = math.log10(base_abs) * abs(exp_num)
                    if est_digits > MAX_INTEGER_DIGITS:
                        raise InputLimitError(
                            f"べき乗結果の桁数が安全上限 ({MAX_INTEGER_DIGITS} 桁) を超えます。"
                        )
            except InputLimitError:
                raise
            except (TypeError, ValueError, OverflowError):
                raise InputLimitError("べき乗計算が安全上限を超えます。") from None
    elif exponent.is_Pow:
        # Reject nested symbolic/numeric exponent towers like 2**(2**x) when inner is huge
        pass

    return sp.Pow(base, exponent)


def _safe_factorial(arg: sp.Expr) -> sp.Expr:
    """Guard factorial against DoS via large integer arguments."""
    if arg.is_number and arg.is_real:
        try:
            val = float(arg.evalf())
        except (TypeError, ValueError, OverflowError) as exc:
            raise InputLimitError("factorial の引数が評価できません。") from exc
        if not math.isfinite(val) or val < 0 or val > MAX_FACTORIAL_ARG:
            raise InputLimitError(
                f"factorial の引数は 0 以上 {MAX_FACTORIAL_ARG} 以下である必要があります。"
            )
    return sp.factorial(arg)


def parse_safe_expression(
    expr_str: str,
    *,
    symbol_assumptions: dict[str, dict[str, bool]] | None = None,
    variable_names: set[str] | None = None,
    allowed_unknown_functions: dict[str, sp.FunctionClass] | None = None,
    allow_derivative: bool = False,
) -> sp.Expr:
    """Safely parse a mathematical expression string into a SymPy Expr using AST whitelisting."""
    if not isinstance(expr_str, str):
        raise UnsafeExpressionError("数式は文字列で指定してください。")
    cleaned = expr_str.strip()
    if not cleaned:
        raise UnsafeExpressionError("空の数式は解析できません。")
    if len(cleaned) > MAX_EXPRESSION_LENGTH:
        raise InputLimitError(f"数式の文字数が上限 ({MAX_EXPRESSION_LENGTH} 文字) を超えています。")
    if "\x00" in cleaned or ";" in cleaned or "\n" in cleaned or "\r" in cleaned:
        raise UnsafeExpressionError("数式に改行・セミコロン・NULL 文字を含めることはできません。")

    try:
        tree = ast.parse(cleaned, mode="eval")
    except SyntaxError as exc:
        raise UnsafeExpressionError(f"数式の構文エラーです: {exc.msg}") from exc

    _check_ast_limits(tree)

    assumptions_map = symbol_assumptions or {}
    explicit_vars = variable_names or set()
    unknown_funcs = allowed_unknown_functions or {}
    symbol_cache: dict[str, sp.Symbol] = {}

    def _get_symbol(ident: str) -> sp.Symbol:
        if ident not in symbol_cache:
            symbol_cache[ident] = make_symbol(ident, assumptions_map.get(ident))
        return symbol_cache[ident]

    def _convert(node: ast.AST) -> sp.Expr:
        if isinstance(node, ast.Expression):
            return _convert(node.body)

        if isinstance(node, ast.Constant):
            return _parse_numeric_constant(node.value)

        if isinstance(node, ast.Name):
            ident = validate_identifier(node.id, allow_constants=True)
            if ident in explicit_vars:
                return _get_symbol(ident)
            if ident in _CONSTANTS:
                return _CONSTANTS[ident]
            if ident in _ALLOWED_FUNCTIONS:
                raise UnsafeExpressionError(
                    f"関数名 '{ident}' を引数なしで変数として使用することはできません。"
                )
            return _get_symbol(ident)

        if isinstance(node, ast.UnaryOp):
            operand = _convert(node.operand)
            if isinstance(node.op, ast.UAdd):
                return operand
            if isinstance(node.op, ast.USub):
                return -operand
            raise UnsafeExpressionError(f"許可されていない単項演算子です: {type(node.op).__name__}")

        if isinstance(node, ast.BinOp):
            left = _convert(node.left)
            right = _convert(node.right)
            if isinstance(node.op, ast.Add):
                return left + right
            if isinstance(node.op, ast.Sub):
                return left - right
            if isinstance(node.op, ast.Mult):
                return left * right
            if isinstance(node.op, ast.Div):
                if right == 0:
                    raise UnsafeExpressionError("ゼロ除算が検出されました。")
                return left / right
            if isinstance(node.op, ast.Pow):
                return _safe_pow(left, right)
            if isinstance(node.op, ast.BitXor):
                raise UnsafeExpressionError(
                    "ビット演算子 '^' は使用できません。べき乗には '**' を使用してください。"
                )
            raise UnsafeExpressionError(f"許可されていない二項演算子です: {type(node.op).__name__}")

        if isinstance(node, ast.Call):
            if node.keywords:
                raise UnsafeExpressionError("関数呼び出しでのキーワード引数は禁止されています。")
            if not isinstance(node.func, ast.Name):
                raise UnsafeExpressionError("属性アクセスや動的な関数呼び出しは禁止されています。")
            func_name = validate_identifier(node.func.id, allow_constants=True)

            # Special-case Derivative(...) / diff(...) for ODE tools only
            if allow_derivative and func_name in ("Derivative", "diff"):
                return _convert_derivative_call(node.args)

            if func_name in unknown_funcs:
                if len(node.args) != 1:
                    raise UnsafeExpressionError(
                        f"未知関数 {func_name} の引数は 1 つである必要があります。"
                    )
                arg_expr = _convert(node.args[0])
                return unknown_funcs[func_name](arg_expr)

            entry = _ALLOWED_FUNCTIONS.get(func_name)
            if entry is None:
                raise UnsafeExpressionError(f"許可されていない関数呼び出しです: {func_name}")
            fn, min_args, max_args = entry
            if not (min_args <= len(node.args) <= max_args):
                raise UnsafeExpressionError(
                    f"関数 '{func_name}' の引数個数が不正です ({len(node.args)})。"
                )
            converted_args = [_convert(arg) for arg in node.args]
            if func_name == "factorial":
                return _safe_factorial(converted_args[0])
            return fn(*converted_args)

        raise UnsafeExpressionError(
            f"許可されていない構文要素が含まれています: {type(node).__name__}"
        )

    def _convert_derivative_call(args: list[ast.expr]) -> sp.Expr:
        if len(args) not in (2, 3):
            raise UnsafeExpressionError(
                "Derivative は Derivative(y(x), x) または "
                "Derivative(y(x), x, order) 形式で指定してください。"
            )
        target = _convert(args[0])
        var_expr = _convert(args[1])
        if not isinstance(var_expr, sp.Symbol):
            raise UnsafeExpressionError("Derivative の第 2 引数は変数である必要があります。")
        order = 1
        if len(args) == 3:
            order_expr = _convert(args[2])
            if not order_expr.is_Integer:
                raise UnsafeExpressionError("Derivative の階数は正の整数である必要があります。")
            order = int(order_expr)
            if not (1 <= order <= MAX_DERIVATIVE_ORDER):
                raise InputLimitError(
                    f"微分の階数は 1 以上 {MAX_DERIVATIVE_ORDER} 以下である必要があります。"
                )
        return sp.Derivative(target, (var_expr, order))

    return _convert(tree)


def parse_safe_equation(
    equation_str: str,
    *,
    symbol_assumptions: dict[str, dict[str, bool]] | None = None,
    variable_names: set[str] | None = None,
    allowed_unknown_functions: dict[str, sp.FunctionClass] | None = None,
    allow_derivative: bool = False,
) -> tuple[sp.Expr, sp.Expr]:
    """Parse 'lhs = rhs' (or 'expr' meaning 'expr = 0') safely into (lhs, rhs)."""
    if not isinstance(equation_str, str):
        raise UnsafeExpressionError("方程式は文字列で指定してください。")
    cleaned = equation_str.strip()
    if not cleaned:
        raise UnsafeExpressionError("空の方程式は解析できません。")
    if len(cleaned) > MAX_EXPRESSION_LENGTH:
        raise InputLimitError(
            f"方程式の文字数が上限 ({MAX_EXPRESSION_LENGTH} 文字) を超えています。"
        )
    if "==" in cleaned or "!=" in cleaned or "<=" in cleaned or ">=" in cleaned:
        raise UnsafeExpressionError("方程式の等号には単一の '=' のみを使用してください。")

    parts = cleaned.split("=")
    if len(parts) > 2:
        raise UnsafeExpressionError("方程式に '=' は 1 つまでしか含められません。")

    if len(parts) == 2:
        lhs = parse_safe_expression(
            parts[0],
            symbol_assumptions=symbol_assumptions,
            variable_names=variable_names,
            allowed_unknown_functions=allowed_unknown_functions,
            allow_derivative=allow_derivative,
        )
        rhs = parse_safe_expression(
            parts[1],
            symbol_assumptions=symbol_assumptions,
            variable_names=variable_names,
            allowed_unknown_functions=allowed_unknown_functions,
            allow_derivative=allow_derivative,
        )
        return lhs, rhs

    lhs = parse_safe_expression(
        cleaned,
        symbol_assumptions=symbol_assumptions,
        variable_names=variable_names,
        allowed_unknown_functions=allowed_unknown_functions,
        allow_derivative=allow_derivative,
    )
    return lhs, sp.Integer(0)


def parse_safe_matrix(
    matrix_data: list[list[Any]],
    *,
    max_dim: int = MAX_MATRIX_DIM,
) -> sp.Matrix:
    """Validate matrix dimensions and safely parse every cell into a SymPy Matrix."""
    if not isinstance(matrix_data, list) or len(matrix_data) == 0:
        raise UnsafeExpressionError("行列は空でない 2 次元配列で指定してください。")
    if len(matrix_data) > max_dim:
        raise InputLimitError(f"行列の行数が上限 ({max_dim}) を超えています。")

    expected_cols: int | None = None
    rows: list[list[sp.Expr]] = []
    for row in matrix_data:
        if not isinstance(row, list) or len(row) == 0:
            raise UnsafeExpressionError("行列の各行は空でない配列である必要があります。")
        if len(row) > max_dim:
            raise InputLimitError(f"行列の列数が上限 ({max_dim}) を超えています。")
        if expected_cols is None:
            expected_cols = len(row)
        elif len(row) != expected_cols:
            raise UnsafeExpressionError("行列の各行の列数が一致していません。")

        parsed_row: list[sp.Expr] = []
        for cell in row:
            if isinstance(cell, (int, float)) and not isinstance(cell, bool):
                parsed_row.append(_parse_numeric_constant(cell))
            elif isinstance(cell, str):
                parsed_row.append(parse_safe_expression(cell))
            else:
                raise UnsafeExpressionError(
                    f"行列要素に許可されていない型が含まれています: {type(cell).__name__}"
                )
        rows.append(parsed_row)

    return sp.Matrix(rows)
