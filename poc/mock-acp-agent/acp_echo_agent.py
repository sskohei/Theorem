"""Minimal ACP (Agent Client Protocol) JSON-RPC 2.0 stdio server for Phase 0 PoC.

Supports:
- initialize (capability negotiation: image, mcpCapabilities, loadSession)
- session/new (records cwd and scoped mcpServers without touching global config)
- session/load (verifies session resume)
- session/prompt (streams session/update chunks, detects image blocks, optionally
  triggers session/request_permission to verify client-side permission control,
  and supports long streaming turns to test session/cancel)
- session/cancel (notification that cancels an active streaming prompt)
"""

from __future__ import annotations

import json
import sys
import threading
import time
import uuid
from typing import Any

write_lock = threading.Lock()
sessions: dict[str, dict[str, Any]] = {}
cancel_events: dict[str, threading.Event] = {}
pending_client_requests: dict[int, tuple[threading.Event, dict[str, Any]]] = {}
next_server_req_id = 1000


def send_msg(payload: dict[str, Any]) -> None:
    line = json.dumps(payload, ensure_ascii=False)
    with write_lock:
        sys.stdout.write(line + "\n")
        sys.stdout.flush()


def send_result(req_id: Any, result: dict[str, Any]) -> None:
    send_msg({"jsonrpc": "2.0", "id": req_id, "result": result})


def send_error(req_id: Any, code: int, message: str) -> None:
    send_msg({"jsonrpc": "2.0", "id": req_id, "error": {"code": code, "message": message}})


def send_notification(method: str, params: dict[str, Any]) -> None:
    send_msg({"jsonrpc": "2.0", "method": method, "params": params})


def request_permission(session_id: str, command: str) -> dict[str, Any]:
    global next_server_req_id
    req_id = next_server_req_id
    next_server_req_id += 1
    ev = threading.Event()
    holder: dict[str, Any] = {}
    pending_client_requests[req_id] = (ev, holder)
    send_msg(
        {
            "jsonrpc": "2.0",
            "id": req_id,
            "method": "session/request_permission",
            "params": {
                "sessionId": session_id,
                "toolCall": {
                    "toolCallId": f"call-{req_id}",
                    "title": f"Execute shell command: {command}",
                    "kind": "execute",
                    "status": "pending",
                },
                "options": [
                    {"optionId": "allow_once", "name": "Allow once", "kind": "allow_once"},
                    {"optionId": "reject_once", "name": "Reject", "kind": "reject_once"},
                ],
            },
        }
    )
    ev.wait(timeout=10.0)
    pending_client_requests.pop(req_id, None)
    return holder.get("response", {"outcome": {"outcome": "cancelled"}})


def handle_prompt(req_id: Any, params: dict[str, Any]) -> None:
    session_id = str(params.get("sessionId", ""))
    if session_id not in sessions:
        send_error(req_id, -32602, f"Unknown sessionId: {session_id}")
        return

    prompt_blocks = params.get("prompt", [])
    text_parts: list[str] = []
    image_count = 0
    for block in prompt_blocks:
        btype = block.get("type")
        if btype == "text":
            text_parts.append(str(block.get("text", "")))
        elif btype == "image":
            image_count += 1

    joined = " ".join(text_parts)
    cancel_ev = threading.Event()
    cancel_events[session_id] = cancel_ev
    sess = sessions[session_id]
    sess["history"].append({"role": "user", "text": joined, "images": image_count})

    if "[TRIGGER_PERMISSION]" in joined:
        perm = request_permission(session_id, "echo unauthorized_shell")
        outcome = perm.get("outcome", {})
        send_notification(
            "session/update",
            {
                "sessionId": session_id,
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {
                        "type": "text",
                        "text": f"Permission outcome received: {json.dumps(outcome, ensure_ascii=False)}",
                    },
                },
            },
        )
        send_result(req_id, {"stopReason": "end_turn"})
        return

    if "[LONG_STREAM]" in joined:
        for i in range(40):
            if cancel_ev.is_set():
                send_result(req_id, {"stopReason": "cancelled"})
                return
            send_notification(
                "session/update",
                {
                    "sessionId": session_id,
                    "update": {
                        "sessionUpdate": "agent_message_chunk",
                        "content": {"type": "text", "text": f"chunk-{i} "},
                    },
                },
            )
            time.sleep(0.05)
        send_result(req_id, {"stopReason": "end_turn"})
        return

    mcp_names = [m.get("name", "unnamed") for m in sess.get("mcpServers", [])]
    chunks = [
        f"CustomACP OK | history_len={len(sess['history'])} ",
        f"| images={image_count} ",
        f"| mcp={','.join(mcp_names)} ",
        f"| echo={joined[:80]}",
    ]
    for ch in chunks:
        if cancel_ev.is_set():
            send_result(req_id, {"stopReason": "cancelled"})
            return
        send_notification(
            "session/update",
            {
                "sessionId": session_id,
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": ch},
                },
            },
        )
        time.sleep(0.02)

    sess["history"].append({"role": "assistant", "text": "".join(chunks)})
    send_result(req_id, {"stopReason": "end_turn"})


def main() -> None:
    for raw_line in sys.stdin:
        raw_line = raw_line.strip()
        if not raw_line:
            continue
        try:
            msg = json.loads(raw_line)
        except json.JSONDecodeError:
            continue

        # Response from client to our permission request
        if "method" not in msg and "id" in msg:
            rid = msg["id"]
            if rid in pending_client_requests:
                ev, holder = pending_client_requests[rid]
                holder["response"] = msg.get("result", {})
                ev.set()
            continue

        method = msg.get("method")
        req_id = msg.get("id")
        params = msg.get("params") or {}

        if method == "initialize":
            send_result(
                req_id,
                {
                    "protocolVersion": 1,
                    "agentInfo": {"name": "formuladesk-mock-acp", "version": "0.1.0"},
                    "agentCapabilities": {
                        "loadSession": True,
                        "promptCapabilities": {"image": True, "audio": False, "embeddedContext": True},
                        "mcpCapabilities": {"http": False, "sse": False},
                    },
                    "authMethods": [],
                },
            )
        elif method == "session/new":
            sid = str(uuid.uuid4())
            sessions[sid] = {
                "cwd": params.get("cwd", ""),
                "mcpServers": params.get("mcpServers", []),
                "history": [],
            }
            send_result(req_id, {"sessionId": sid})
        elif method == "session/load":
            sid = str(params.get("sessionId", ""))
            if sid not in sessions:
                send_error(req_id, -32602, f"Unknown sessionId: {sid}")
            else:
                send_result(req_id, {"sessionId": sid, "resumed": True})
        elif method == "session/prompt":
            t = threading.Thread(target=handle_prompt, args=(req_id, params), daemon=True)
            t.start()
        elif method == "session/cancel":
            sid = str(params.get("sessionId", ""))
            ev = cancel_events.get(sid)
            if ev is not None:
                ev.set()
        elif req_id is not None:
            send_error(req_id, -32601, f"Method not found: {method}")


if __name__ == "__main__":
    main()
