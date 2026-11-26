"""Shared response envelope, verification statuses, limits, and error types."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Literal

VerificationStatus = Literal["passed", "failed", "inconclusive", "not_run", "error"]
PlotFormat = Literal["svg", "png"]

# Hard security & resource limits (docs/science-mcp.md Section 4.3)
MAX_EXPRESSION_LENGTH: int = 2048
MAX_AST_NODES: int = 256
MAX_AST_DEPTH: int = 32
MAX_IDENTIFIER_LENGTH: int = 32
MAX_MATRIX_DIM: int = 20
MAX_EIGEN_DIM: int = 10
MAX_DERIVATIVE_ORDER: int = 10
MAX_EXPONENT_ABS: int = 100
MAX_POWER_OF_TEN_EXPONENT: int = 30
MAX_INTEGER_DIGITS: int = 100
MAX_FACTORIAL_ARG: int = 100
MAX_PLOT_SERIES: int = 8
MAX_PLOT_DATA_POINTS: int = 10_000
MAX_VECTOR_GRID_SIZE: int = 40
MAX_PRECISION_DIGITS: int = 100
DEFAULT_TIMEOUT_SECONDS: float = 10.0
MIN_TIMEOUT_SECONDS: float = 0.5
MAX_TIMEOUT_SECONDS: float = 60.0


class ScienceMcpError(Exception):
    """Base structured error raised inside Science MCP tools."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


class UnsafeExpressionError(ScienceMcpError):
    """Raised when an expression violates the strict AST security whitelist."""

    def __init__(self, message: str) -> None:
        super().__init__("UNSAFE_EXPRESSION", message)


class InputLimitError(ScienceMcpError):
    """Raised when an input exceeds size, depth, matrix, or numeric limits."""

    def __init__(self, message: str) -> None:
        super().__init__("INPUT_LIMIT_EXCEEDED", message)


class CalculationError(ScienceMcpError):
    """Raised when a valid mathematical or physical operation cannot be completed."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(code, message)


@dataclass(slots=True)
class VerificationPayload:
    """Structured verification metadata attached to a tool response."""

    status: VerificationStatus
    target: str
    assumptions: list[str] = field(default_factory=list)
    method: str = ""
    detail: dict[str, Any] | None = None

    def to_dict(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "status": self.status,
            "target": self.target,
            "assumptions": self.assumptions,
            "method": self.method,
        }
        if self.detail is not None:
            payload["detail"] = self.detail
        return payload


@dataclass(slots=True)
class ArtifactPayload:
    """Generated visualization artifact confined to the session artifacts directory."""

    relative_path: str
    filename: str
    mime_type: str
    format: PlotFormat
    caption: str | None = None
    width_px: int | None = None
    height_px: int | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "relative_path": self.relative_path,
            "filename": self.filename,
            "mime_type": self.mime_type,
            "format": self.format,
            "caption": self.caption,
            "width_px": self.width_px,
            "height_px": self.height_px,
        }


def make_success_response(
    tool: str,
    result: dict[str, Any],
    *,
    verification: VerificationPayload | None = None,
    artifacts: list[ArtifactPayload] | None = None,
    warnings: list[str] | None = None,
    duration_ms: int = 0,
) -> dict[str, Any]:
    """Build the standardized Science MCP success envelope."""
    return {
        "ok": True,
        "tool": tool,
        "result": result,
        "verification": verification.to_dict() if verification is not None else None,
        "artifacts": [a.to_dict() for a in (artifacts or [])],
        "warnings": list(warnings or []),
        "error": None,
        "duration_ms": duration_ms,
    }


def make_error_response(
    tool: str,
    code: str,
    message: str,
    *,
    verification_target: str | None = None,
    warnings: list[str] | None = None,
    duration_ms: int = 0,
) -> dict[str, Any]:
    """Build the standardized Science MCP error envelope."""
    verification: dict[str, Any] | None = None
    if verification_target is not None:
        verification = VerificationPayload(
            status="error",
            target=verification_target,
            assumptions=[],
            method=f"science_mcp.{tool}",
            detail={"code": code, "message": message},
        ).to_dict()
    return {
        "ok": False,
        "tool": tool,
        "result": None,
        "verification": verification,
        "artifacts": [],
        "warnings": list(warnings or []),
        "error": {"code": code, "message": message},
        "duration_ms": duration_ms,
    }
