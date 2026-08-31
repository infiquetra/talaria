#!/usr/bin/env python3
"""Build deterministic T1 visual-acceptance frame-log corpora.

The files are written through Talaria's production ``FrameRecorder`` so they
cross the normal redaction and frame-log serialization boundary.  They contain
only bounded acceptance identifiers and protocol events; no credential or
private operator value is present.
"""

from __future__ import annotations

import json
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any

from talaria.recorder.framelog import FrameRecorder

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "docs" / "acceptance" / "v0.5.0" / "corpora" / "t1"
ENDPOINT = "ws://127.0.0.1:8765/api/ws"
SESSION = "talaria-v050-t1-acceptance"


class ScheduledClock:
    """A recorder clock that can be moved to an exact corpus offset."""

    def __init__(self) -> None:
        self.base = datetime(2026, 8, 31, 12, 0, tzinfo=UTC)
        self.current = self.base

    def set(self, seconds: float) -> None:
        self.current = self.base + timedelta(seconds=seconds)

    def __call__(self) -> str:
        return self.current.isoformat(timespec="milliseconds").replace("+00:00", "Z")


def event(kind: str, payload: dict[str, Any], *, session: str = SESSION) -> str:
    return json.dumps(
        {
            "jsonrpc": "2.0",
            "method": "event",
            "params": {"type": kind, "session_id": session, "payload": payload},
        },
        sort_keys=True,
    )


def event_without_session(kind: str, payload: dict[str, Any]) -> str:
    return json.dumps(
        {
            "jsonrpc": "2.0",
            "method": "event",
            "params": {"type": kind, "payload": payload},
        },
        sort_keys=True,
    )


def outbound(text: str) -> str:
    return json.dumps(
        {
            "jsonrpc": "2.0",
            "id": "acceptance-prompt-1",
            "method": "prompt.submit",
            "params": {"session_id": SESSION, "text": text},
        },
        sort_keys=True,
    )


def record(
    recorder: FrameRecorder,
    clock: ScheduledClock,
    seconds: float,
    direction: str,
    raw: str,
) -> None:
    clock.set(seconds)
    recorder.record(direction, raw)  # type: ignore[arg-type]


def new_recorder(name: str) -> tuple[FrameRecorder, ScheduledClock]:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    path = OUTPUT / name
    path.unlink(missing_ok=True)
    clock = ScheduledClock()
    return FrameRecorder(path, ENDPOINT, clock=clock), clock


DIFF = """--- a/talaria/ui/transcript.py
+++ b/talaria/ui/transcript.py
@@ -71,7 +71,8 @@ def render_entry(entry):
-    marker = entry.kind
+    marker = fixed_identity(entry.kind)
+    background = theme_token(entry.kind)
     return marker, background
@@ -101,3 +102,7 @@ def stable_anchor(entry):
     return entry.sequence
+
+def long_comment_example() -> str:
+    # Imported comments keep their Visual Studio Code syntax colour.
+    return "acceptance-anchor-abcdefghijklmnopqrstuvwxyz-0123456789"
--- a/talaria/ui/status_bar.py
+++ b/talaria/ui/status_bar.py
@@ -18,5 +18,6 @@ def segments(state):
-    return [state.connection, state.model]
+    result = [state.connection, state.model]
+    return result
"""


def initial(recorder: FrameRecorder, clock: ScheduledClock, title: str) -> None:
    record(recorder, clock, 0.05, "in", event("gateway.ready", {}))
    record(
        recorder,
        clock,
        0.10,
        "in",
        event(
            "session.info",
            {
                "stored_session_id": "t1-acceptance-session",
                "title": title,
                "usage": {"input_tokens": 6100, "output_tokens": 1400},
            },
        ),
    )


def build_visual_surfaces() -> None:
    recorder, clock = new_recorder("visual-surfaces.jsonl")
    initial(recorder, clock, "T1 visual surfaces")
    record(recorder, clock, 0.15, "out", outbound("Review the complete terminal surface."))
    record(recorder, clock, 0.20, "in", event("message.start", {}))
    record(
        recorder,
        clock,
        0.25,
        "in",
        event(
            "subagent.start",
            {
                "subagent_id": "visual-reviewer",
                "goal": "Review theme coverage",
                "depth": 1,
                "task_index": 0,
                "model": "muse-spark-1.2-contributor-free",
            },
        ),
    )
    record(
        recorder,
        clock,
        0.30,
        "in",
        event(
            "tool.start",
            {"tool_id": "visual-diff", "name": "edit_file", "context": "two files"},
        ),
    )
    record(
        recorder,
        clock,
        0.35,
        "in",
        event(
            "tool.complete",
            {
                "tool_id": "visual-diff",
                "name": "edit_file",
                "summary": "2 files changed across 3 hunks",
                "inline_diff": DIFF,
            },
        ),
    )
    record(
        recorder,
        clock,
        0.38,
        "in",
        event(
            "subagent.complete",
            {
                "subagent_id": "visual-reviewer",
                "status": "completed",
                "summary": "Theme coverage held",
            },
        ),
    )
    transcript = "\n".join(
        [
            "The full interface is held for visual acceptance.",
            "Transcript, composer, agent row, status surfaces, inspector, and diff are populated.",
            *[f"Visual evidence line {index:02d} remains readable." for index in range(1, 25)],
        ]
    )
    record(recorder, clock, 0.40, "in", event("message.delta", {"text": transcript}))
    record(
        recorder,
        clock,
        0.45,
        "in",
        event(
            "message.complete",
            {"text": transcript, "usage": {"input_tokens": 6100, "output_tokens": 1400}},
        ),
    )
    recorder.close()


def build_focus_surface() -> None:
    recorder, clock = new_recorder("focus-surface.jsonl")
    initial(recorder, clock, "T1 focus surface")
    record(recorder, clock, 0.15, "out", outbound("Keep every focus target populated."))
    record(recorder, clock, 0.20, "in", event("message.start", {}))
    record(
        recorder,
        clock,
        0.25,
        "in",
        event(
            "subagent.start",
            {
                "subagent_id": "focus-worker",
                "goal": "Hold one interruptible row",
                "depth": 1,
                "task_index": 0,
            },
        ),
    )
    record(
        recorder,
        clock,
        0.30,
        "in",
        event(
            "tool.complete",
            {
                "tool_id": "focus-diff",
                "name": "edit_file",
                "summary": "focus fixture changed",
                "inline_diff": DIFF,
            },
        ),
    )
    record(
        recorder,
        clock,
        0.35,
        "in",
        event("message.delta", {"text": "Focus evidence is visible in the transcript."}),
    )
    record(
        recorder,
        clock,
        0.40,
        "in",
        event(
            "clarify.request",
            {
                "request_id": "focus-clarify-1",
                "question": "Which acceptance focus target should be reviewed?",
                "choices": ["composer", "transcript", "inspector"],
            },
        ),
    )
    recorder.close()


def build_agent_queue_states() -> None:
    recorder, clock = new_recorder("agent-queue-states.jsonl")
    initial(recorder, clock, "T1 agent and queue states")
    statuses = ("completed", "error", "failed", "interrupted", "timeout")
    record(
        recorder,
        clock,
        0.15,
        "in",
        event(
            "subagent.spawn_requested",
            {"subagent_id": "agent-queued", "goal": "queued acceptance work", "task_index": 0},
        ),
    )
    record(
        recorder,
        clock,
        0.20,
        "in",
        event(
            "subagent.start",
            {"subagent_id": "agent-running", "goal": "running acceptance work", "task_index": 1},
        ),
    )
    for index, status in enumerate(statuses, start=2):
        agent_id = f"agent-{status}"
        moment = 0.20 + index * 0.06
        record(
            recorder,
            clock,
            moment,
            "in",
            event(
                "subagent.start",
                {"subagent_id": agent_id, "goal": f"{status} acceptance work", "task_index": index},
            ),
        )
        record(
            recorder,
            clock,
            moment + 0.025,
            "in",
            event(
                "subagent.complete",
                {"subagent_id": agent_id, "status": status, "summary": f"settled as {status}"},
            ),
        )
    record(
        recorder,
        clock,
        1.50,
        "in",
        event(
            "approval.request",
            {
                "request_id": "approval-head",
                "description": "Review the head approval",
                "command": "uv run pytest tests/ui/test_agent_rows.py",
                "choices": ["once", "deny"],
            },
        ),
    )
    record(
        recorder,
        clock,
        2.50,
        "in",
        event(
            "approval.request",
            {
                "request_id": "approval-queued",
                "description": "Review the queued approval",
                "command": "uv run pytest tests/ui/test_needs_you.py",
                "choices": ["once", "deny"],
            },
        ),
    )
    record(
        recorder,
        clock,
        3.70,
        "in",
        event(
            "clarify.request",
            {
                "request_id": "possibly-duplicate-wait",
                "question": "Confirm the possibly duplicate queue row.",
                "choices": ["confirm", "hold"],
            },
        ),
    )
    record(
        recorder,
        clock,
        3.80,
        "in",
        event(
            "clarify.request",
            {
                "question": "Confirm the unanchored queue row.",
                "choices": ["confirm", "hold"],
            },
            session="unanchored-acceptance-session",
        ),
    )
    recorder.close()


def build_transcript_identities() -> None:
    recorder, clock = new_recorder("transcript-identities.jsonl")
    initial(recorder, clock, "T1 transcript identities")
    record(recorder, clock, 0.15, "out", outbound("Operator identity entry."))
    record(recorder, clock, 0.20, "in", event("message.start", {}))
    record(
        recorder,
        clock,
        0.25,
        "in",
        event("reasoning.delta", {"text": "Reasoning identity entry."}),
    )
    record(
        recorder,
        clock,
        0.30,
        "in",
        event("message.delta", {"text": "Assistant identity entry."}),
    )
    record(
        recorder,
        clock,
        0.35,
        "in",
        event("message.complete", {"text": "Assistant identity entry."}),
    )
    record(
        recorder,
        clock,
        0.40,
        "in",
        event(
            "tool.start",
            {"tool_id": "identity-tool", "name": "search", "context": "Activity identity entry."},
        ),
    )
    record(
        recorder,
        clock,
        0.45,
        "in",
        event("status.update", {"text": "Session identity entry."}),
    )
    record(
        recorder,
        clock,
        0.50,
        "in",
        event("error", {"message": "Fault identity entry."}),
    )
    recorder.close()


def build_startup_notice() -> None:
    recorder, clock = new_recorder("startup-notice.jsonl")
    record(recorder, clock, 0.05, "in", event_without_session("gateway.ready", {}))
    recorder.close()


def build_motion_and_scroll() -> None:
    recorder, clock = new_recorder("motion-and-scroll.jsonl")
    initial(recorder, clock, "T1 motion and scroll")
    operator_lines = "\n".join(
        [
            "Populate a transcript longer than three viewports.",
            *[
                f"READING-ANCHOR-{index:03d} stable source offset {1000 + index}"
                for index in range(1, 121)
            ],
        ]
    )
    record(recorder, clock, 0.15, "out", outbound(operator_lines))
    record(recorder, clock, 0.20, "in", event("message.start", {}))
    record(
        recorder,
        clock,
        0.25,
        "in",
        event(
            "subagent.start",
            {
                "subagent_id": "stream-worker",
                "goal": "Continue motion-producing agent work",
                "task_index": 0,
            },
        ),
    )
    record(
        recorder,
        clock,
        0.30,
        "in",
        event(
            "subagent.thinking",
            {"subagent_id": "stream-worker", "text": "streaming new transcript batches"},
        ),
    )
    for batch, moment in enumerate((0.35, 1.50, 2.50, 3.50, 4.50), start=1):
        lines = "\n".join(
            [
                f"APPEND-BATCH-{batch} line {line:02d} newest source offset "
                f"{batch * 100 + line}"
                for line in range(1, 13)
            ]
        )
        record(recorder, clock, moment, "in", event("message.delta", {"text": lines + "\n"}))
        if batch == 4:
            record(
                recorder,
                clock,
                moment + 0.10,
                "in",
                event(
                    "subagent.progress",
                    {"subagent_id": "stream-worker", "text": "latest batch remains fresh"},
                ),
            )
    record(
        recorder,
        clock,
        5.00,
        "in",
        event(
            "subagent.complete",
            {"subagent_id": "stream-worker", "status": "completed", "summary": "stream complete"},
        ),
    )
    record(
        recorder,
        clock,
        5.05,
        "in",
        event("message.complete", {"text": "NEWEST-BOTTOM-ENTRY follow target."}),
    )
    recorder.close()


def main() -> None:
    build_visual_surfaces()
    build_focus_surface()
    build_agent_queue_states()
    build_transcript_identities()
    build_startup_notice()
    build_motion_and_scroll()
    for path in sorted(OUTPUT.glob("*.jsonl")):
        print(path)


if __name__ == "__main__":
    main()
