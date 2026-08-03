"""FrameRecorder / frame-log v1 writer tests (R25, R26, AE15).

Ported from the behavior of ``src/record/recorder.ts`` (no direct
``recorder.test.ts`` exists upstream; this file is the Python original,
covering the requirement table's evidence obligations for R25/R26 against
``docs/formats/frame-log.md``, the authority).
"""

from __future__ import annotations

import json
import os
import stat
from pathlib import Path
from typing import Any

import pytest

from talaria.recorder.framelog import FrameRecorder, RecorderError, default_log_path
from talaria.recorder.reader import read_frame_log


def _lines(path: Path) -> list[dict[str, object]]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def test_construction_writes_exactly_one_header(tmp_path: Path) -> None:
    out = tmp_path / "rec.jsonl"
    recorder = FrameRecorder(out, "ws://127.0.0.1:8765/api/ws?token=abc")
    recorder.close()

    lines = _lines(out)
    assert len(lines) == 1
    assert lines[0]["kind"] == "header"
    assert lines[0]["version"] == 1
    assert lines[0]["endpoint"] == "ws://127.0.0.1:8765/api/ws?token=%5Bredacted%5D"


def test_an_empty_recording_is_still_a_complete_self_describing_file(tmp_path: Path) -> None:
    out = tmp_path / "empty.jsonl"
    recorder = FrameRecorder(out, "ws://127.0.0.1:8765/api/ws")
    recorder.close()

    assert out.exists()
    header = _lines(out)[0]
    assert header["kind"] == "header"


def test_second_header_never_appended_to_an_existing_file(tmp_path: Path) -> None:
    out = tmp_path / "rec.jsonl"
    first = FrameRecorder(out, "ws://127.0.0.1:8765/api/ws")
    first.record("in", json.dumps({"method": "event", "params": {}}))
    first.close()

    # Re-opening the same path appends -- FrameRecorder always writes a
    # header on construction, mirroring the TypeScript reference's
    # append-mode stream; a caller that wants a fresh file picks a fresh
    # path (default_log_path is timestamped for exactly this reason).
    second = FrameRecorder(out, "ws://127.0.0.1:8765/api/ws")
    second.close()

    lines = _lines(out)
    header_count = sum(1 for line in lines if line["kind"] == "header")
    assert header_count == 2  # two independent recordings share the file by construction


def test_seq_is_gapless_from_one(tmp_path: Path) -> None:
    out = tmp_path / "rec.jsonl"
    recorder = FrameRecorder(out, "ws://127.0.0.1:8765/api/ws")
    for i in range(5):
        recorder.record("in", json.dumps({"method": "event", "params": {"i": i}}))
    recorder.close()

    log = read_frame_log(out)
    assert [entry.seq for entry in log.entries] == [1, 2, 3, 4, 5]


def test_redaction_happens_before_the_value_reaches_disk(tmp_path: Path) -> None:
    out = tmp_path / "rec.jsonl"
    recorder = FrameRecorder(out, "ws://127.0.0.1:8765/api/ws")
    recorder.record(
        "in",
        json.dumps(
            {
                "method": "sudo.respond",
                "params": {"request_id": "r-1", "password": "hunter2"},
            }
        ),
    )
    recorder.close()

    raw_text = out.read_text(encoding="utf-8")
    assert "hunter2" not in raw_text

    log = read_frame_log(out)
    frame_entry = log.entries[0]
    assert frame_entry.frame["params"]["password"] == "[redacted]"
    assert len(frame_entry.redactions) == 1
    assert frame_entry.redactions[0].path == "params.password"
    assert frame_entry.redactions[0].reason == "deny-set:sudo.respond"


def test_unparseable_payload_becomes_a_categorical_hole_with_no_raw_bytes(
    tmp_path: Path,
) -> None:
    out = tmp_path / "rec.jsonl"
    recorder = FrameRecorder(out, "ws://127.0.0.1:8765/api/ws")
    raw_garbage = "{not json, and API_KEY=super-secret-leak-if-ever-written"
    recorder.record("in", raw_garbage)
    recorder.close()

    raw_text = out.read_text(encoding="utf-8")
    assert "super-secret-leak-if-ever-written" not in raw_text
    assert raw_garbage not in raw_text

    log = read_frame_log(out)
    entry = log.entries[0]
    assert entry.frame is None
    assert entry.parse_error is not None
    assert entry.redactions == ()

    stats = recorder.stats()
    assert stats.parse_errors == 1
    assert stats.frames == 1


def test_stats_count_frames_redactions_and_parse_errors(tmp_path: Path) -> None:
    out = tmp_path / "rec.jsonl"
    recorder = FrameRecorder(out, "ws://127.0.0.1:8765/api/ws")
    recorder.record("in", json.dumps({"method": "event", "params": {}}))
    recorder.record(
        "in", json.dumps({"method": "secret.respond", "params": {"value": "s3cr3t"}})
    )
    recorder.record("in", "not json at all {")
    recorder.close()

    stats = recorder.stats()
    assert stats.frames == 3
    assert stats.redactions == 1
    assert stats.parse_errors == 1


def test_close_is_idempotent(tmp_path: Path) -> None:
    out = tmp_path / "rec.jsonl"
    recorder = FrameRecorder(out, "ws://127.0.0.1:8765/api/ws")
    recorder.close()
    recorder.close()  # must not raise


@pytest.mark.skipif(
    hasattr(os, "geteuid") and os.geteuid() == 0,
    reason="root bypasses directory permission bits, so this proxy for a create "
    "failure cannot be exercised",
)
def test_create_failure_is_surfaced(tmp_path: Path) -> None:
    """R25: a create failure surfaces rather than silently producing nothing."""
    blocked_dir = tmp_path / "blocked"
    blocked_dir.mkdir()
    os.chmod(blocked_dir, stat.S_IREAD | stat.S_IEXEC)  # read+execute, no write

    try:
        with pytest.raises(RecorderError):
            FrameRecorder(blocked_dir / "rec.jsonl", "ws://127.0.0.1:8765/api/ws")
    finally:
        os.chmod(blocked_dir, stat.S_IRWXU)  # restore so tmp_path cleanup can remove it


def test_write_failure_after_close_is_surfaced(tmp_path: Path) -> None:
    """R25: writing to an already-closed stream is a visible failure, not a
    swallowed no-op -- the closest hermetic proxy for "disk went away
    mid-recording" available without mocking the filesystem."""
    out = tmp_path / "rec.jsonl"
    recorder = FrameRecorder(out, "ws://127.0.0.1:8765/api/ws")
    recorder.close()

    with pytest.raises(RecorderError):
        recorder.record("in", json.dumps({"method": "event", "params": {}}))


def test_default_log_path_is_timestamped_and_never_collides_by_construction(
    tmp_path: Path,
) -> None:
    from datetime import UTC, datetime

    recordings_dir = tmp_path / "recordings"
    first_at = datetime(2026, 8, 2, 12, 21, 35, 16000, tzinfo=UTC)
    second_at = datetime(2026, 8, 2, 12, 21, 36, 0, tzinfo=UTC)
    first = default_log_path(recordings_dir, now=first_at)
    second = default_log_path(recordings_dir, now=second_at)

    assert first != second
    assert first.parent == recordings_dir
    assert first.name == "2026-08-02T12-21-35-016Z.jsonl"


def test_outbound_synthetic_frames_fully_redacted_end_to_end_through_the_writer(
    tmp_path: Path,
) -> None:
    """R28: the four blocking bridges, `model.save_key`, and a suspicious-key
    unknown method, recorded through the full `FrameRecorder.record()` path
    (not just the pure `redact_frame` function) -- proving the writer never
    bypasses the boundary for the `dir="out"` direction docs/formats/frame-log.md
    names as the one that will actually carry credentials once Talaria gains
    a send path."""
    out = tmp_path / "outbound.jsonl"
    recorder = FrameRecorder(out, "ws://127.0.0.1:8765/api/ws")

    synthetic_outbound_frames: list[dict[str, Any]] = [
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sudo.respond",
            "params": {"request_id": "r-1", "password": "canary-outbound-sudo"},
        },
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "secret.respond",
            "params": {"request_id": "r-2", "value": "canary-outbound-secret"},
        },
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "terminal.read.respond",
            "params": {"request_id": "r-3", "text": "canary-outbound-terminal-buffer"},
        },
        {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "clarify.respond",
            "params": {"request_id": "r-4", "answer": "canary-outbound-clarify-answer"},
        },
        {
            "jsonrpc": "2.0",
            "id": 5,
            "method": "model.save_key",
            "params": {"slug": "deepseek", "api_key": "canary-outbound-model-key"},
        },
        {
            "jsonrpc": "2.0",
            "id": 6,
            "method": "future.unknown.respond",
            "params": {"secret_token": "canary-outbound-unknown-method"},
        },
    ]

    for frame in synthetic_outbound_frames:
        recorder.record("out", json.dumps(frame))
    recorder.close()

    raw_text = out.read_text(encoding="utf-8")
    for frame in synthetic_outbound_frames:
        for value in frame["params"].values():
            if isinstance(value, str) and value.startswith("canary-"):
                assert value not in raw_text

    log = read_frame_log(out)
    assert len(log.entries) == len(synthetic_outbound_frames)
    for entry in log.entries:
        assert entry.dir == "out"
        assert len(entry.redactions) == 1
