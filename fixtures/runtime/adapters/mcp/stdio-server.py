#!/usr/bin/env python3
import json
import os
import sys


BUFFER = b""
MAX_RESPONSE_BYTES = 1024 * 1024


def main() -> None:
    global BUFFER
    while True:
        chunk = os.read(sys.stdin.fileno(), 4096)
        if not chunk:
            return
        BUFFER += chunk
        parse_available_messages()


def parse_available_messages() -> None:
    global BUFFER
    while True:
        header_end = BUFFER.find(b"\r\n\r\n")
        if header_end == -1:
            return
        header = BUFFER[:header_end].decode("utf-8")
        content_length = None
        for line in header.splitlines():
            name, _, value = line.partition(":")
            if name.lower() == "content-length":
                content_length = int(value.strip())
                break
        if content_length is None:
            return
        body_start = header_end + 4
        body_end = body_start + content_length
        if len(BUFFER) < body_end:
            return
        body = BUFFER[body_start:body_end].decode("utf-8")
        BUFFER = BUFFER[body_end:]
        handle(json.loads(body))


def handle(request: dict[str, object]) -> None:
    request_id = request.get("id")
    if request_id is None:
        return
    method = request.get("method")
    if method == "initialize":
        respond(
            request_id,
            {
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "runx-rust-mcp-fixture", "version": "0.0.0"},
            },
        )
        return
    if method == "tools/list":
        respond(request_id, {"tools": tool_list()})
        return
    if method == "tools/call":
        handle_tool_call(request_id, request.get("params"))
        return
    respond_error(request_id, -32601, "method not found")


def tool_list() -> list[dict[str, object]]:
    return [
        tool("echo", "Echo a message through the fixture MCP server.", {"message": "string"}, ["message"]),
        tool("fail", "Return a fixture MCP error for testing.", {"message": "string"}, []),
        tool("sleep", "Never respond, for timeout testing.", {}, []),
        tool("env", "Return a single fixture server environment variable.", {"name": "string"}, ["name"]),
        tool("max-response", "Return a response body exactly at the MCP client size limit.", {}, []),
        tool("oversized-response", "Declare a response body over the MCP client size limit.", {}, []),
    ]


def tool(name: str, description: str, properties: dict[str, str], required: list[str]) -> dict[str, object]:
    return {
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {
                key: {"type": value, "description": f"{key}."} for key, value in properties.items()
            },
            "required": required,
            "additionalProperties": False,
        },
    }


def handle_tool_call(request_id: object, params: object) -> None:
    if not isinstance(params, dict) or not isinstance(params.get("name"), str):
        respond_error(request_id, -32602, "invalid tool call")
        return
    name = params["name"]
    args = params.get("arguments")
    if not isinstance(args, dict):
        args = {}
    if name == "sleep":
        return
    if name == "env":
        respond_text(request_id, os.environ.get(str(args.get("name", "")), ""))
        return
    if name == "fail":
        respond_error(request_id, -32000, f"fixture failure: {args.get('message', '')}")
        return
    if name == "max-response":
        respond_text(request_id, "x" * MAX_RESPONSE_BYTES)
        return
    if name == "oversized-response":
        respond_text(request_id, "x" * (MAX_RESPONSE_BYTES + 1))
        return
    if name != "echo":
        respond_error(request_id, -32601, "tool not found")
        return
    respond_text(request_id, str(args.get("message", "")))


def respond_text(request_id: object, text: str) -> None:
    respond(request_id, {"content": [{"type": "text", "text": text}]})


def respond(request_id: object, result: object) -> None:
    write({"jsonrpc": "2.0", "id": request_id, "result": result})


def respond_error(request_id: object, code: int, message: str) -> None:
    write({"jsonrpc": "2.0", "id": request_id, "error": {"code": code, "message": message}})


def write(message: dict[str, object]) -> None:
    body = json.dumps(message, separators=(",", ":")).encode("utf-8")
    sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body)
    sys.stdout.buffer.flush()


if __name__ == "__main__":
    main()
