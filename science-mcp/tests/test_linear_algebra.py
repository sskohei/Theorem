"""Tests for linear algebra tools (determinant, inverse, operations, eigen)."""

from __future__ import annotations

import pytest

from science_mcp.models import CalculationError
from science_mcp.tools.linear_algebra import (
    calculate_determinant,
    calculate_eigenvalues,
    calculate_eigenvectors,
    calculate_inverse_matrix,
    matrix_operation,
)


def test_determinant_and_inverse_and_singular_error() -> None:
    mat = [[2, 1], [5, 3]]
    det = calculate_determinant(mat)
    assert det["ok"] is True
    assert det["result"]["expression"] == "1"

    inv = calculate_inverse_matrix(mat)
    assert inv["ok"] is True
    assert inv["verification"]["status"] == "passed"
    assert inv["result"]["matrix"] == [["3", "-1"], ["-5", "2"]]

    with pytest.raises(CalculationError) as exc_info:
        calculate_inverse_matrix([[1, 2], [2, 4]])
    assert exc_info.value.code == "SINGULAR_MATRIX"


def test_matrix_operations() -> None:
    a = [[1, 2], [3, 4]]
    b = [[2, 0], [1, 2]]
    mul = matrix_operation("multiply", [a, b])
    assert mul["result"]["matrix"] == [["4", "4"], ["10", "8"]]

    tr = matrix_operation("trace", [a])
    assert tr["result"]["expression"] == "5"

    rk = matrix_operation("rank", [[[1, 2], [2, 4]]])
    assert rk["result"]["scalar"] == 1

    trans = matrix_operation("transpose", [[[1, 2, 3], [4, 5, 6]]])
    assert trans["result"]["rows"] == 3
    assert trans["result"]["cols"] == 2


def test_eigenvalues_and_eigenvectors() -> None:
    mat = [[4, 1], [2, 3]]
    vals = calculate_eigenvalues(mat)
    assert vals["ok"] is True
    assert vals["verification"]["status"] == "passed"
    found = {item["value"] for item in vals["result"]["eigenvalues"]}
    assert found == {"2", "5"}

    vecs = calculate_eigenvectors(mat)
    assert vecs["ok"] is True
    assert vecs["verification"]["status"] == "passed"
    assert len(vecs["result"]["eigensystem"]) == 2
