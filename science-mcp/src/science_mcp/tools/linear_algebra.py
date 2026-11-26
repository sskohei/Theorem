"""Linear algebra tools: determinant, inverse, operations, eigenvalues, and eigenvectors."""

from __future__ import annotations

import math
from typing import Any

import sympy as sp

from science_mcp.models import (
    MAX_EIGEN_DIM,
    MAX_MATRIX_DIM,
    CalculationError,
    VerificationPayload,
    make_success_response,
)
from science_mcp.parser import parse_safe_matrix


def _matrix_to_payload(mat: sp.Matrix) -> dict[str, Any]:
    rows_str = [[str(mat[r, c]) for c in range(mat.cols)] for r in range(mat.rows)]
    return {
        "rows": mat.rows,
        "cols": mat.cols,
        "matrix": rows_str,
        "latex": sp.latex(mat),
    }


def _try_float(expr: sp.Expr) -> float | None:
    if expr.is_number and expr.is_real is not False:
        try:
            cval = complex(expr.evalf())
            if abs(cval.imag) < 1e-12 and math.isfinite(cval.real):
                return float(cval.real)
        except (TypeError, ValueError, OverflowError):
            return None
    return None


def calculate_determinant(matrix: list[list[Any]]) -> dict[str, Any]:
    """Calculate the exact determinant of a square matrix (up to 20x20)."""
    mat = parse_safe_matrix(matrix, max_dim=MAX_MATRIX_DIM)
    if mat.rows != mat.cols:
        raise CalculationError(
            "NON_SQUARE_MATRIX",
            f"行列式は正方行列のみ計算できます（入力: {mat.rows}x{mat.cols}）。",
        )

    det = sp.simplify(mat.det())
    verification = VerificationPayload(
        status="passed",
        target=f"{mat.rows}x{mat.cols} 行列の行列式計算",
        assumptions=[],
        method="sympy.Matrix.det()",
    )
    return make_success_response(
        "calculate_determinant",
        {
            "expression": str(det),
            "latex": sp.latex(det),
            "numeric": _try_float(det),
            "size": mat.rows,
        },
        verification=verification,
    )


def calculate_inverse_matrix(matrix: list[list[Any]]) -> dict[str, Any]:
    """Calculate the inverse of a non-singular square matrix with A * A^-1 == I verification."""
    mat = parse_safe_matrix(matrix, max_dim=MAX_MATRIX_DIM)
    if mat.rows != mat.cols:
        raise CalculationError(
            "NON_SQUARE_MATRIX",
            f"逆行列は正方行列のみ計算できます（入力: {mat.rows}x{mat.cols}）。",
        )

    det = sp.simplify(mat.det())
    if det == 0:
        raise CalculationError(
            "SINGULAR_MATRIX",
            "行列式が 0（特異行列）のため逆行列は存在しません。",
        )

    inv = sp.simplify(mat.inv())
    identity = sp.eye(mat.rows)
    product_check = sp.simplify(mat * inv - identity) == sp.zeros(mat.rows)

    verification = VerificationPayload(
        status="passed" if product_check else "inconclusive",
        target=f"{mat.rows}x{mat.cols} 逆行列検算 (A * A^-1 == I)",
        assumptions=["det(A) != 0"],
        method="sympy.simplify(A * A.inv() - I) == 0",
        detail={"determinant": str(det)},
    )
    payload = _matrix_to_payload(inv)
    payload["determinant"] = str(det)
    return make_success_response(
        "calculate_inverse_matrix",
        payload,
        verification=verification,
    )


def matrix_operation(operation: str, matrices: list[list[list[Any]]]) -> dict[str, Any]:
    """Perform add, subtract, multiply, transpose, rank, or trace on validated matrices."""
    op = operation.strip().lower()
    if op not in ("add", "subtract", "multiply", "transpose", "rank", "trace"):
        raise CalculationError(
            "INVALID_OPERATION",
            f"未対応の行列演算です: {operation}（add/subtract/multiply/transpose/rank/trace）",
        )
    if not isinstance(matrices, list) or not matrices:
        raise CalculationError("MISSING_MATRIX", "少なくとも 1 つの行列を指定してください。")
    if len(matrices) > 6:
        raise CalculationError("TOO_MANY_MATRICES", "同時に指定できる行列は最大 6 個です。")

    parsed = [parse_safe_matrix(m, max_dim=MAX_MATRIX_DIM) for m in matrices]

    if op == "transpose":
        if len(parsed) != 1:
            raise CalculationError("INVALID_ARITY", "transpose は行列 1 つを指定してください。")
        res_mat = parsed[0].T
        return make_success_response(
            "matrix_operation",
            {"operation": op, **_matrix_to_payload(res_mat)},
            verification=VerificationPayload(
                status="passed",
                target="転置行列計算",
                method="sympy.Matrix.T",
            ),
        )

    if op == "rank":
        if len(parsed) != 1:
            raise CalculationError("INVALID_ARITY", "rank は行列 1 つを指定してください。")
        r = int(parsed[0].rank())
        return make_success_response(
            "matrix_operation",
            {"operation": op, "scalar": r, "expression": str(r), "latex": str(r)},
            verification=VerificationPayload(
                status="passed",
                target="行列の階数 (rank) 計算",
                method="sympy.Matrix.rank()",
            ),
        )

    if op == "trace":
        if len(parsed) != 1:
            raise CalculationError("INVALID_ARITY", "trace は行列 1 つを指定してください。")
        if parsed[0].rows != parsed[0].cols:
            raise CalculationError("NON_SQUARE_MATRIX", "トレースは正方行列のみ計算できます。")
        tr = sp.simplify(parsed[0].trace())
        return make_success_response(
            "matrix_operation",
            {
                "operation": op,
                "expression": str(tr),
                "latex": sp.latex(tr),
                "numeric": _try_float(tr),
            },
            verification=VerificationPayload(
                status="passed",
                target="行列のトレース計算",
                method="sympy.Matrix.trace()",
            ),
        )

    if len(parsed) < 2:
        raise CalculationError("INVALID_ARITY", f"{op} には 2 つ以上の行列が必要です。")

    acc = parsed[0]
    for nxt in parsed[1:]:
        if op in ("add", "subtract"):
            if acc.shape != nxt.shape:
                raise CalculationError(
                    "SHAPE_MISMATCH",
                    f"行列サイズが一致しません: {acc.shape} vs {nxt.shape}",
                )
            acc = acc + nxt if op == "add" else acc - nxt
        else:  # multiply
            if acc.cols != nxt.rows:
                raise CalculationError(
                    "SHAPE_MISMATCH",
                    f"行列積のサイズが不整合です: {acc.shape} * {nxt.shape}",
                )
            acc = acc * nxt

    acc = sp.simplify(acc)
    return make_success_response(
        "matrix_operation",
        {"operation": op, **_matrix_to_payload(acc)},
        verification=VerificationPayload(
            status="passed",
            target=f"行列演算 ({op})",
            method=f"sympy.Matrix {op}",
        ),
    )


def calculate_eigenvalues(matrix: list[list[Any]]) -> dict[str, Any]:
    """Compute eigenvalues and algebraic multiplicities with characteristic polynomial check."""
    mat = parse_safe_matrix(matrix, max_dim=MAX_EIGEN_DIM)
    if mat.rows != mat.cols:
        raise CalculationError("NON_SQUARE_MATRIX", "固有値は正方行列のみ計算できます。")

    eigvals = mat.eigenvals()
    items: list[dict[str, Any]] = []
    verified_all = True
    lam = sp.Symbol("_lam")
    charpoly = mat.charpoly(lam).as_expr()

    for val, mult in eigvals.items():
        simp = sp.simplify(val)
        items.append(
            {
                "value": str(simp),
                "latex": sp.latex(simp),
                "numeric": _try_float(simp),
                "multiplicity": int(mult),
            }
        )
        if sp.simplify(charpoly.subs(lam, simp)) != 0:
            verified_all = False

    verification = VerificationPayload(
        status="passed" if verified_all and items else "inconclusive",
        target=f"{mat.rows}x{mat.cols} 行列の固有値検算 (det(A - λI) == 0)",
        assumptions=[],
        method="sympy.Matrix.eigenvals() + 特性多項式代入検証",
    )
    return make_success_response(
        "calculate_eigenvalues",
        {"eigenvalues": items, "size": mat.rows},
        verification=verification,
    )


def calculate_eigenvectors(matrix: list[list[Any]]) -> dict[str, Any]:
    """Compute eigenvalues and eigenvectors with A*v == lambda*v verification."""
    mat = parse_safe_matrix(matrix, max_dim=MAX_EIGEN_DIM)
    if mat.rows != mat.cols:
        raise CalculationError("NON_SQUARE_MATRIX", "固有ベクトルは正方行列のみ計算できます。")

    eigvecs = mat.eigenvects()
    items: list[dict[str, Any]] = []
    verified_all = True

    for val, mult, basis in eigvecs:
        simp_val = sp.simplify(val)
        vec_entries: list[dict[str, Any]] = []
        for vec in basis:
            simp_vec = sp.simplify(vec)
            residual = sp.simplify(mat * simp_vec - simp_val * simp_vec)
            if residual != sp.zeros(mat.rows, 1):
                verified_all = False
            vec_entries.append(
                {
                    "components": [str(simp_vec[i, 0]) for i in range(simp_vec.rows)],
                    "latex": sp.latex(simp_vec),
                }
            )
        items.append(
            {
                "eigenvalue": str(simp_val),
                "eigenvalue_latex": sp.latex(simp_val),
                "multiplicity": int(mult),
                "eigenvectors": vec_entries,
            }
        )

    verification = VerificationPayload(
        status="passed" if verified_all and items else "inconclusive",
        target=f"{mat.rows}x{mat.cols} 行列の固有ベクトル検算 (A v == λ v)",
        assumptions=[],
        method="sympy.simplify(A * v - lambda * v) == 0",
    )
    return make_success_response(
        "calculate_eigenvectors",
        {"eigensystem": items, "size": mat.rows},
        verification=verification,
    )
