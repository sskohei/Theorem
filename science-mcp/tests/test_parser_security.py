"""Security regression tests for AST whitelist parser, DoS limits, and static RCE bans."""

from __future__ import annotations

import ast
from pathlib import Path

import pytest

from science_mcp.models import InputLimitError, UnsafeExpressionError
from science_mcp.parser import parse_safe_equation, parse_safe_expression, parse_safe_matrix


@pytest.mark.parametrize(
    "payload",
    [
        "__import__('os').system('echo pwn')",
        "().__class__.__base__.__subclasses__()",
        "open('/etc/passwd').read()",
        "eval('1+1')",
        "exec('import os')",
        "getattr(x, '__class__')",
        "(lambda x: x + 1)(2)",
        "[x for x in range(10)]",
        "__builtins__",
        "x.__dict__",
        "x[0]",
        "sin(x, evaluate=False)",
        "1; import os",
        "1\nimport os",
        "True + 1",
        "x ^ 2",
        "unknown_func(x)",
    ],
)
def test_parser_rejects_rce_and_injection_payloads(payload: str) -> None:
    with pytest.raises((UnsafeExpressionError, InputLimitError)):
        parse_safe_expression(payload)


def test_parser_rejects_huge_exponent_and_factorial_dos() -> None:
    with pytest.raises(InputLimitError):
        parse_safe_expression("10**1000000")
    with pytest.raises(InputLimitError):
        parse_safe_expression("2**500")
    with pytest.raises(InputLimitError):
        parse_safe_expression("999**99")
    with pytest.raises(InputLimitError):
        parse_safe_expression("factorial(100000)")
    with pytest.raises(InputLimitError):
        parse_safe_expression("x" + "+1" * 2000)


def test_parser_rejects_excessive_ast_depth_and_matrix_size() -> None:
    deep = "(" * 40 + "x" + ")" * 40
    # Parentheses alone don't add AST depth; use nested sin() calls
    deep_calls = "sin(" * 35 + "x" + ")" * 35
    assert parse_safe_expression(deep) is not None
    with pytest.raises(InputLimitError):
        parse_safe_expression(deep_calls)

    oversized_matrix = [[1] * 21 for _ in range(21)]
    with pytest.raises(InputLimitError):
        parse_safe_matrix(oversized_matrix)


def test_equation_parser_rejects_double_equals_and_multiple_equals() -> None:
    with pytest.raises(UnsafeExpressionError):
        parse_safe_equation("x == 2")
    with pytest.raises(UnsafeExpressionError):
        parse_safe_equation("x = y = 1")


def test_production_source_contains_no_eval_exec_parse_expr_or_lambdify() -> None:
    """AST static guard ensuring dangerous dynamic execution calls never enter src/."""
    src_root = Path(__file__).resolve().parents[1] / "src" / "science_mcp"
    banned_builtins = {"eval", "exec", "compile"}
    banned_attrs = {"parse_expr", "lambdify"}

    for py_file in src_root.rglob("*.py"):
        tree = ast.parse(py_file.read_text(encoding="utf-8"), filename=str(py_file))
        for node in ast.walk(tree):
            if isinstance(node, ast.Call):
                if isinstance(node.func, ast.Name):
                    assert node.func.id not in banned_builtins, (
                        f"Forbidden call {node.func.id}() in {py_file}:{node.lineno}"
                    )
                    assert node.func.id not in banned_attrs, (
                        f"Forbidden call {node.func.id}() in {py_file}:{node.lineno}"
                    )
                elif isinstance(node.func, ast.Attribute):
                    assert node.func.attr not in banned_attrs, (
                        f"Forbidden method .{node.func.attr}() in {py_file}:{node.lineno}"
                    )
