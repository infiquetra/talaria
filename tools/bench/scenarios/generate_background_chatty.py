#!/usr/bin/env python3
"""Write the chatty terminal-output frame log used by the keystroke bench.

The first line is copied from fixtures/frames/plain-answer.jsonl. Each later
line is one agent.terminal.output event. The session id is the first event
frame in that source file that carries one.
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "fixtures" / "frames" / "plain-answer.jsonl"
TARGET = Path(__file__).with_name("background-chatty.jsonl")
SESSION = "local-30e93504638f13e7ce115f6746c73f6bb49701343ff39e9805e0c2a9cde820ec"
REPLAY_EPOCH = "c7d207d0e5154f0facd61b27f8cb01bd"
PROCESS = "proc_000000000001"
LETTERS = "abcdefghijklmnopqrstuvwxyz"
FRAMES = 6_000


def chunk(index: int) -> str:
    body = f"tick {index} "
    letter = 0
    while len(body) < 119:
        body += LETTERS[letter % len(LETTERS)]
        letter += 1
    return body + "\n"


def main() -> None:
    header = SOURCE.read_text(encoding="utf-8").splitlines()[0]
    lines = [header]
    for index in range(1, FRAMES + 1):
        record = {
            "kind": "frame",
            "seq": index,
            "tMs": (index - 1) * 5,
            "generation": 1,
            "dir": "in",
            "frame": {
                "jsonrpc": "2.0",
                "method": "event",
                "params": {
                    "type": "agent.terminal.output",
                    "session_id": SESSION,
                    "seq": index,
                    "replay_epoch": REPLAY_EPOCH,
                    "payload": {"process_id": PROCESS, "chunk": chunk(index)},
                },
            },
        }
        lines.append(json.dumps(record, ensure_ascii=False, separators=(",", ":")))
    TARGET.write_text("\n".join(lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
