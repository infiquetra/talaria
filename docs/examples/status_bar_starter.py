#!/usr/bin/env python3
"""Starter status script for Talaria's status bar (C4, issue #143).

Copy this file somewhere durable (for example ``~/.talaria/``) and point
``status.command`` at it with an absolute path — the runner splits the
command with POSIX quoting and execs it directly, so there is no shell
and ``~`` does not expand::

    [status]
    command = "python3 /home/operator/.talaria/status_bar_starter.py"

Standard library only. Reads the KTD5 payload document (JSON) on stdin
and prints plain-text rows on stdout — at most eight; anything past the
row bound truncates with a visible marker, and anything past the byte
bound is cut the same way. Exit status is always zero: a starter must
never break the bar it demonstrates.

Fields the payload does not carry (context-window size, rate limits,
spending) print as labelled unavailable rows. They are real absences in
the script input (verified against ``StatusPayload.to_json_dict``), never
misreadings — do not replace these rows with guessed numbers.
"""

from __future__ import annotations

import json
import sys
from typing import Any


def _read_payload() -> dict[str, Any]:
    try:
        raw = sys.stdin.read()
    except (OSError, UnicodeDecodeError):
        return {}
    try:
        payload = json.loads(raw) if raw.strip() else {}
    except (json.JSONDecodeError, ValueError):
        return {}
    return payload if isinstance(payload, dict) else {}


def _usage_row(payload: dict[str, Any]) -> str:
    usage = payload.get("usage")
    if isinstance(usage, dict):
        received = usage.get("input_tokens")
        sent = usage.get("output_tokens")
        if (
            isinstance(received, int)
            and not isinstance(received, bool)
            and isinstance(sent, int)
            and not isinstance(sent, bool)
        ):
            return f"usage {received} in · {sent} out"
    return "usage: not observed (no usage event yet)"


def _rows(payload: dict[str, Any]) -> list[str]:
    session = payload.get("session")
    if isinstance(session, dict):
        session_id = session.get("id", "?")
        title = session.get("title") or "untitled"
        session_row = f"session {session_id} — {title}"
    else:
        session_row = "session: unavailable (no session in script input)"
    mode = payload.get("mode", "?")
    connection = payload.get("connection", "?")
    return [
        session_row,
        f"{mode} · {connection}",
        _usage_row(payload),
        "context window: unavailable (not in script input)",
        "rate limits: unavailable (not in script input)",
        "spending: unavailable (not in script input)",
    ]


def main() -> int:
    for row in _rows(_read_payload()):
        # One row per line, literal text: the region renders rows as-is and
        # defangs escape sequences, so keep rows plain and single-line.
        print(str(row).replace("\n", " "))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
