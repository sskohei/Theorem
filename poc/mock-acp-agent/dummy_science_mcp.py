"""Zero-dependency MCP stdio server for Phase 0 PoC (scoped MCP registration test).

Provides a single deterministic math tool `poc_verify_polynomial` using pure Python
without touching any global configuration files.
"""

from __future__ import annotations

import json
import sys
from typing import Any


def send_msg(payload: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def main() -> None:
    for raw_line in sys.stdin:
        raw_line = raw_line.strip()
        if not raw_line:
            continue
        try:
            msg = json.loads(raw_line)
        except json.JSONDecodeError:
            continue

        method = msg.get("method")
        req_id = msg.get("id")
        params = msg.get("params") or {}

        if method == "initialize":
            send_msg(
                {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "formuladesk-poc-science-mcp", "version": "0.1.0"},
                    },
                }
            )
        elif method == "notifications/initialized":
            continue
        elif method == "tools/list":
            send_msg(
                {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "tools": [
                            {
                                "name": "poc_definite_integral_power",
                                "description": "Compute exact definite integral of x^n from a to b (Phase 0 PoC).",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "n": {"type": "integer", "minimum": 0, "maximum": 10},
                                        "a": {"type": "number"},
                                        "b": {"type": "number"},
                                    },
                                    "required": ["n", "a", "b"],
                                },
                            }
                        ]
                    },
                }
            )
        elif method == "tools/call":
            name = params.get("name")
            args = params.get("arguments") or {}
            if name == "poc_definite_integral_power":
                n = int(args.get("n", 2))
                a = float(args.get("a", 0.0))
                b = float(args.get("b", 1.0))
                val = (b ** (n + 1) - a ** (n + 1)) / (n + 1)
                send_msg(
                    {
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "result": {
                            "content": [
                                {
                                    "type": "text",
                                    "text": json.dumps(
                                        {
                                            "ok": True,
                                            "formula": f"int_{a}^{b} x^{n} dx",
                                            "value": val,
                                            "exact_fraction": f"({b}^{n+1} - {a}^{n+1}) / {n+1}",
                                            "status": "passed",
                                        }
                                    ),
                                }
                            ]
                        },
                    }
                )
            else:
                send_msg(
                    {
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "error": {"code": -32602, "message": f"Unknown tool: {name}"},
                    }
                )
        elif req_id is not None:
            send_msg(
                {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": {"code": -32601, "message": f"Method not found: {method}"},
                }
            )


if __name__ == "__main__":
    main()
