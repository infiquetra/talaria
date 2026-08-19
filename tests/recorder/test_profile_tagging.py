"""KTD6's recording contract: one log, two shapes, and the line between them.

The version a recording carries follows from its connection count and from
nothing else. That is the whole contract, and the reason it is worth a file of
its own is the half that is easy to break without noticing: a
**single**-connection run must keep emitting version 1 with no ``profile`` key
anywhere, byte for byte, because that is the format the TypeScript-reference
equivalence corpus is written in — and that corpus is what makes the redaction
guarantee a measured property rather than a claim.

So these tests assert in both directions: that a two-connection run stamps
every frame and lists its connections, and that a one-connection run stamps
nothing even when the caller offers a profile.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from talaria.recorder.framelog import (
    FRAME_LOG_VERSION,
    FRAME_LOG_VERSION_MULTI_CONNECTION,
    FrameRecorder,
    RecordedConnection,
    RecorderError,
)
from talaria.recorder.reader import FrameLogError, read_frame_log

ALPHA = "alpha-fixture"
BETA = "beta-fixture"

FIRST = "ws://127.0.0.1:9119/api/ws"
SECOND = "ws://127.0.0.1:9120/api/ws"


def _lines(path: Path) -> list[dict[str, object]]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def _fleet_recorder(path: Path) -> FrameRecorder:
    return FrameRecorder(
        path,
        FIRST,
        connections=(
            RecordedConnection(ALPHA, FIRST),
            RecordedConnection(BETA, SECOND),
        ),
    )


# ── the multi-connection shape ───────────────────────────────────────────


def test_a_two_connection_run_writes_one_log_with_a_connections_header(
    tmp_path: Path,
) -> None:
    path = tmp_path / "fleet.jsonl"
    recorder = _fleet_recorder(path)
    recorder.close()

    header = _lines(path)[0]
    assert header["version"] == FRAME_LOG_VERSION_MULTI_CONNECTION
    assert header["connections"] == [
        {"profile": ALPHA, "endpoint": FIRST},
        {"profile": BETA, "endpoint": SECOND},
    ]
    # A v2 header is a superset of a v1 one: no field changes meaning by
    # disappearing, so ``endpoint`` still names the run's first connection.
    assert header["endpoint"] == FIRST


def test_every_frame_of_a_multi_connection_log_names_the_connection_it_crossed(
    tmp_path: Path,
) -> None:
    path = tmp_path / "fleet.jsonl"
    recorder = _fleet_recorder(path)
    recorder.view(ALPHA).record("in", json.dumps({"jsonrpc": "2.0", "method": "event"}))
    recorder.view(BETA).record("out", json.dumps({"jsonrpc": "2.0", "method": "session.list"}))
    recorder.close()

    frames = [line for line in _lines(path) if line["kind"] == "frame"]
    assert [frame["profile"] for frame in frames] == [ALPHA, BETA]
    # Native arrival order, one sequence: replay needs no merge rule.
    assert [frame["seq"] for frame in frames] == [1, 2]


def test_an_unparseable_frame_is_tagged_too(tmp_path: Path) -> None:
    """A withheld payload still says which gateway it came from.

    Protocol drift is exactly the thing a fleet operator needs attributed: "one
    of your gateways is sending garbage" is not actionable, and the tag is what
    makes it "this one is".
    """
    path = tmp_path / "fleet.jsonl"
    recorder = _fleet_recorder(path)
    recorder.view(BETA).record("in", "{not json")
    recorder.close()

    frame = [line for line in _lines(path) if line["kind"] == "frame"][0]
    assert frame["profile"] == BETA
    assert frame["frame"] is None
    assert "parseError" in frame


def test_a_frame_naming_no_connection_is_refused_rather_than_written_untagged(
    tmp_path: Path,
) -> None:
    """An untagged frame in a tagged log is a hole nothing downstream can fill."""
    recorder = _fleet_recorder(tmp_path / "fleet.jsonl")
    try:
        with pytest.raises(RecorderError):
            recorder.record("in", json.dumps({"jsonrpc": "2.0"}))
    finally:
        recorder.close()


def test_a_frame_naming_an_undeclared_connection_is_refused(tmp_path: Path) -> None:
    """A mislabelled frame is worse than an untagged one: it attributes a
    session to a gateway that never saw it."""
    recorder = _fleet_recorder(tmp_path / "fleet.jsonl")
    try:
        with pytest.raises(RecorderError):
            recorder.view("never-declared-fixture")
        with pytest.raises(RecorderError):
            recorder.record("in", json.dumps({"jsonrpc": "2.0"}), profile="never-declared-fixture")
    finally:
        recorder.close()


def test_two_connections_may_not_share_one_profile_name(tmp_path: Path) -> None:
    with pytest.raises(RecorderError):
        FrameRecorder(
            tmp_path / "fleet.jsonl",
            FIRST,
            connections=(RecordedConnection(ALPHA, FIRST), RecordedConnection(ALPHA, SECOND)),
        )


def test_one_connection_closing_does_not_end_the_log_the_others_are_writing(
    tmp_path: Path,
) -> None:
    """One gateway hanging up must not take the whole recording with it."""
    path = tmp_path / "fleet.jsonl"
    recorder = _fleet_recorder(path)
    alpha = recorder.view(ALPHA)
    beta = recorder.view(BETA)

    beta.close()
    beta.close()  # idempotent
    alpha.record("in", json.dumps({"jsonrpc": "2.0", "method": "event"}))
    alpha.close()

    frames = [line for line in _lines(path) if line["kind"] == "frame"]
    assert [frame["profile"] for frame in frames] == [ALPHA]
    assert recorder.stats().frames == 1
    with pytest.raises(RecorderError):
        alpha.record("in", "{}")


def test_the_credential_still_never_reaches_a_multi_connection_log(
    tmp_path: Path,
) -> None:
    """Tagging happens after redaction, not instead of it (R22)."""
    path = tmp_path / "fleet.jsonl"
    recorder = _fleet_recorder(path)
    canary = "NOT-A-REAL-SECRET-fleet-canary-3Kd"
    recorder.view(ALPHA).record(
        "out",
        json.dumps(
            {"jsonrpc": "2.0", "method": "sudo.respond", "params": {"password": canary}}
        ),
    )
    recorder.close()

    raw = path.read_text(encoding="utf-8")
    assert canary not in raw
    frame = [line for line in _lines(path) if line["kind"] == "frame"][0]
    assert frame["profile"] == ALPHA
    assert frame["redactions"]


# ── the single-connection shape stays exactly as it was ──────────────────


def test_a_single_connection_run_emits_no_profile_and_no_connections_list(
    tmp_path: Path,
) -> None:
    """The property the equivalence corpus depends on."""
    path = tmp_path / "single.jsonl"
    recorder = FrameRecorder(path, FIRST, connections=(RecordedConnection(ALPHA, FIRST),))
    recorder.view(ALPHA).record("in", json.dumps({"jsonrpc": "2.0", "method": "event"}))
    recorder.close()

    header, frame = _lines(path)
    assert header["version"] == FRAME_LOG_VERSION
    assert "connections" not in header
    assert "profile" not in frame


def test_a_recorder_told_nothing_about_connections_writes_the_v1_shape(
    tmp_path: Path,
) -> None:
    """``talaria record`` and ``talaria --record`` construct exactly this."""
    path = tmp_path / "classic.jsonl"
    recorder = FrameRecorder(path, FIRST)
    recorder.record("in", json.dumps({"jsonrpc": "2.0", "method": "event"}))
    recorder.close()

    header, frame = _lines(path)
    assert header["version"] == FRAME_LOG_VERSION
    assert set(header) == {"kind", "version", "startedAt", "endpoint"}
    assert set(frame) == {"kind", "seq", "at", "dir", "frame"}


def test_a_profile_offered_to_a_single_connection_log_is_dropped_unwritten(
    tmp_path: Path,
) -> None:
    """The caller need not know which shape it is writing into.

    A one-connection ``ConnectionSet`` still hands its source a profile-bound
    view; that view must produce a v1 record, or every single-gateway
    recording would silently change format the day the fleet code shipped.
    """
    path = tmp_path / "single.jsonl"
    recorder = FrameRecorder(path, FIRST, connections=(RecordedConnection(ALPHA, FIRST),))
    recorder.record("in", json.dumps({"jsonrpc": "2.0"}), profile=ALPHA)
    recorder.close()

    frame = _lines(path)[1]
    assert "profile" not in frame


def test_a_v0_1_format_log_still_loads_as_one_connection(tmp_path: Path) -> None:
    """A recording with no profile key anywhere is a one-connection recording."""
    path = tmp_path / "v1.jsonl"
    recorder = FrameRecorder(path, FIRST)
    recorder.record("in", json.dumps({"jsonrpc": "2.0", "method": "event"}))
    recorder.close()

    log = read_frame_log(path)
    assert log.header.version == FRAME_LOG_VERSION
    assert len(log.entries) == 1


def test_the_reader_names_the_version_2_keys_as_data(tmp_path: Path) -> None:
    """U8 did the work the previous version of this test was waiting for.

    It used to assert only that a version-2 log *parsed* — the unknown-key
    tolerance — and said in its own docstring that naming ``profile`` and
    ``connections`` as data was U8's job. It is done, so the assertion moves
    from "nothing broke" to "the tag arrived", which is what every step after
    this one reads.
    """
    path = tmp_path / "fleet.jsonl"
    recorder = _fleet_recorder(path)
    recorder.view(ALPHA).record("in", json.dumps({"jsonrpc": "2.0", "method": "event"}))
    recorder.close()

    log = read_frame_log(path)
    assert log.header.version == FRAME_LOG_VERSION_MULTI_CONNECTION
    assert len(log.entries) == 1
    assert [row.profile for row in log.header.connections] == [ALPHA, BETA]
    assert log.entries[0].profile == ALPHA, "the frame's connection was dropped on read"


def test_a_version_this_build_cannot_read_stops_rather_than_misreads(
    tmp_path: Path,
) -> None:
    """The guard the format document promised, which nothing implemented.

    ``docs/formats/frame-log.md`` states the rule: "a version-1-only reader that
    skipped the unknown ``profile`` key would merge equal ids from two different
    gateways into one session that never existed, and would do it silently. The
    bump makes such a reader stop instead of misread." Until U8 there was no
    version check at all — a header declaring anything at all parsed, and every
    field a future format added would have been dropped in exactly the silent
    way the paragraph forbids.

    Refusing a FORWARD version is the point rather than a limitation: this build
    cannot know what a version 3 means, and the failure mode of guessing is a
    merged session rather than an error.
    """
    path = tmp_path / "future.jsonl"
    recorder = _fleet_recorder(path)
    recorder.view(ALPHA).record("in", json.dumps({"jsonrpc": "2.0", "method": "event"}))
    recorder.close()

    lines = path.read_text(encoding="utf-8").splitlines()
    header = json.loads(lines[0])
    header["version"] = FRAME_LOG_VERSION_MULTI_CONNECTION + 1
    path.write_text("\n".join([json.dumps(header), *lines[1:]]) + "\n", encoding="utf-8")

    with pytest.raises(FrameLogError) as caught:
        read_frame_log(path)
    assert "3" in str(caught.value)
    assert "newer Talaria" in str(caught.value), (
        "the refusal does not tell the operator what would read this file"
    )

    # Both known versions still read, or the guard is a regression of its own.
    for version in (FRAME_LOG_VERSION, FRAME_LOG_VERSION_MULTI_CONNECTION):
        header["version"] = version
        path.write_text("\n".join([json.dumps(header), *lines[1:]]) + "\n", encoding="utf-8")
        assert read_frame_log(path).header.version == version
