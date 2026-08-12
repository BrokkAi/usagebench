#!/usr/bin/env python3
"""Extract a structured review from a retained Claude Code JSONL session."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--session-log", type=Path, required=True)
    parser.add_argument("--session-id", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    final_text = None
    executed_at = None
    model = None
    for line in args.session_log.read_text().splitlines():
        event = json.loads(line)
        message = event.get("message") or {}
        if event.get("type") != "assistant" or message.get("role") != "assistant":
            continue
        for block in message.get("content", []):
            if block.get("type") == "text" and "\"schemaVersion\"" in block.get("text", ""):
                final_text = block["text"]
                executed_at = event["timestamp"]
                model = message["model"]

    if final_text is None or executed_at is None or model is None:
        raise SystemExit("session log has no final structured review response")
    match = re.search(r"```json\s*(\{.*\})\s*```", final_text, re.DOTALL)
    if match is None:
        raise SystemExit("final response has no JSON code block")
    response = json.loads(match.group(1))
    response["reviewer"] = {
        "provider": "anthropic",
        "model": model,
        "executionId": f"claude-code-session:{args.session_id}",
        "executedAt": executed_at,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(response, indent=2) + "\n")


if __name__ == "__main__":
    main()
