"""Fleet replay: a recording of N connections replays as N connections (v0.4 U8).

The shape a recording opens as follows from its header version and from nothing
else. That is the contract, and the half worth a file of its own is the one that
is easy to break silently: a **version-1** log must keep yielding bare
``FrameRecord``s exactly as it did before this unit, because that is what every
shipped replay path and the whole determinism gate already consume.

So these tests assert in both directions — that a two-connection recording
carries its tags across the seam, and that a one-connection recording is
untouched by the machinery that carries them.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from talaria.domain.decode import decode_frame
from talaria.domain.state import FleetState, route_frame
from talaria.recorder.framelog import FrameRecorder, RecordedConnection
from talaria.replay.controls import ReplayControls
from talaria.replay.source import (
    ReplaySource,
    SidebandAction,
    TaggedReplaySource,
    source_from_path,
)
from talaria.transport.connection_set import TaggedFrame
from talaria.transport.source import FrameRecord

WORK = "work-fixture"
LAB = "lab-fixture"


def frame(session: str) -> str:
    return json.dumps(
        {
            "jsonrpc": "2.0",
            "method": "event",
            "params": {"type": "message.start", "session_id": session, "payload": {}},
        }
    )


def two_connection_log(path: Path) -> Path:
    recorder = FrameRecorder(
        path,
        "ws://gateway.invalid/",
        connections=(
            RecordedConnection(profile=WORK, endpoint="ws://work.invalid/"),
            RecordedConnection(profile=LAB, endpoint="ws://lab.invalid/"),
        ),
    )
    recorder.view(WORK).record("in", frame("s-work"))
    recorder.view(LAB).record("in", frame("s-lab"))
    recorder.view(WORK).record("in", frame("s-work"))
    recorder.close()
    return path


def one_connection_log(path: Path) -> Path:
    recorder = FrameRecorder(path, "ws://gateway.invalid/")
    recorder.record("in", frame("s1"))
    recorder.record("in", frame("s1"))
    recorder.close()
    return path


def instant() -> ReplayControls:
    """No inter-frame delay — these tests are about identity, not pacing."""
    return ReplayControls(speed=0.0)


@pytest.mark.asyncio
async def test_a_two_connection_recording_yields_each_frames_own_connection(
    tmp_path: Path,
) -> None:
    """The whole of KTD6 at the replay seam.

    Session ids are unique within a gateway process and not across gateways, so
    the tag is the only thing that keeps two connections' sessions apart. Without
    it a replayed fleet collapses into one session that never existed — which is
    exactly what the format document says the version bump exists to prevent.
    """
    source = source_from_path(two_connection_log(tmp_path / "fleet.jsonl"), controls=instant())

    assert isinstance(source, TaggedReplaySource)
    items = [item async for item in source]

    assert [item.profile for item in items] == [WORK, LAB, WORK]
    assert all(isinstance(item, TaggedFrame) for item in items)
    assert [item.record.seq for item in items] == [1, 2, 3], (
        "native arrival order was not preserved; replay invented a merge rule"
    )
    # NOT ``== {REPLAY_EPOCH}``, which was the first draft and compares the
    # constant with itself. What the epoch has to achieve is asserted instead:
    # ``route_frame`` persists a connection's channel only when
    # ``generation > channel.generation``, and both are 0 on a fresh fleet — so
    # an epoch of zero replays this log into a fleet with no channels, and the
    # queue then names only the focused profile instead of every connection the
    # recording holds.
    fleet = FleetState(focused_profile=WORK)
    for item in items:
        decoded = decode_frame(item.record.frame, at=item.record.at, seq=item.record.seq)
        fleet = route_frame(fleet, decoded, profile=item.profile, generation=item.epoch)
    assert sorted(fleet.channels) == sorted({WORK, LAB}), (
        "replaying this log left the fleet with no channel for a connection it "
        "contains, so the queue cannot name that connection at all"
    )


@pytest.mark.asyncio
async def test_a_one_connection_recording_is_untouched_by_the_tagging_machinery(
    tmp_path: Path,
) -> None:
    """A version-1 log yields bare records, exactly as before this unit.

    Asserted on the *type* rather than on an empty profile string, because the
    two are different promises: "the tag is blank" would still hand every
    existing consumer an object it does not accept. The determinism gate and
    every shipped replay path take ``FrameRecord``.
    """
    source = source_from_path(one_connection_log(tmp_path / "solo.jsonl"), controls=instant())

    assert isinstance(source, ReplaySource)
    items = [item async for item in source]

    assert all(isinstance(item, FrameRecord) for item in items), (
        "a single-connection replay started yielding tagged frames; every "
        "existing consumer takes FrameRecord"
    )
    assert [item.seq for item in items] == [1, 2]


def test_the_shape_follows_the_declared_version_not_a_guess_from_the_entries(
    tmp_path: Path,
) -> None:
    """The header decides, and a stripped tag does not change the answer.

    Inferring the shape from "does any entry carry a profile" would make a log
    whose frames all happen to come from one connection open as a
    single-connection recording — and then its second connection's frames, when
    they arrived later in the same file, would fold into the first one's
    sessions. The version is a statement about the whole file; the entries are
    not.
    """
    path = two_connection_log(tmp_path / "fleet.jsonl")
    lines = path.read_text(encoding="utf-8").splitlines()
    stripped = [lines[0]]
    for line in lines[1:]:
        entry = json.loads(line)
        entry.pop("profile", None)
        stripped.append(json.dumps(entry))
    path.write_text("\n".join(stripped) + "\n", encoding="utf-8")

    source = source_from_path(path, controls=instant())
    assert isinstance(source, TaggedReplaySource), (
        "the shape was inferred from the entries rather than read from the header"
    )


@pytest.mark.asyncio
async def test_the_sideband_timeline_fires_the_same_way_through_a_tagged_source(
    tmp_path: Path,
) -> None:
    """One copy of the pacing rule, asserted where a second copy would show.

    The tagged source composes ``ReplaySource.paced`` rather than reimplementing
    the loop, so the sideband ordering — an action tied to a frame index fires
    immediately after that frame is applied — has to hold identically here. A
    forked loop would drift on exactly this, because it is the part of the rule
    with no visible output of its own.
    """
    fired: list[int] = []
    source = TaggedReplaySource.from_path(
        two_connection_log(tmp_path / "fleet.jsonl"), controls=instant()
    )
    source.bind_sideband(
        (SidebandAction(frame_index=2, kind="confirmed_cancel"),),
        lambda action: fired.append(action.frame_index),
    )

    seen = 0
    async for _item in source:
        seen += 1
        if seen == 2:
            assert fired == [], "the action fired before its own frame was applied"

    assert fired == [2], "the sideband action never fired through the tagged source"


#  ── the focus derivation, and what a replayed fleet says about itself ────


def landing_log(path: Path) -> Path:
    """Two connections, and the run demonstrably drove the second one."""
    recorder = FrameRecorder(
        path,
        "ws://gateway.invalid/",
        connections=(
            RecordedConnection(profile=WORK, endpoint="ws://work.invalid/"),
            RecordedConnection(profile=LAB, endpoint="ws://lab.invalid/"),
        ),
    )
    # WORK speaks first, so the first-session-named rule would choose it.
    recorder.view(WORK).record("in", frame("s-work"))
    recorder.view(LAB).record(
        "out",
        json.dumps({"jsonrpc": "2.0", "id": 1, "method": "session.resume",
                    "params": {"session_id": "s-lab"}}),
    )
    recorder.view(LAB).record("in", frame("s-lab"))
    recorder.close()
    return path


def test_a_recorded_landing_reply_outranks_the_first_session_named(tmp_path: Path) -> None:
    """Rule one beats rule two, and the log is built so they disagree.

    ``WORK`` speaks first, so the adoption rule alone would focus it. But the run
    issued ``session.resume`` on ``LAB`` — it *chose* that session — and a
    recording of a run replays as the run behaved.
    """
    source = TaggedReplaySource.from_path(landing_log(tmp_path / "landed.jsonl"))
    assert source.focus_profile == LAB, (
        "the derivation ignored the strongest statement the log makes about "
        "what this run was driving"
    )


def test_with_no_landing_call_the_first_session_named_decides(tmp_path: Path) -> None:
    """Rule two is the live engine's own adoption rule, so a log with no landing
    call replays the way the run itself behaved."""
    source = TaggedReplaySource.from_path(two_connection_log(tmp_path / "fleet.jsonl"))
    assert source.focus_profile == WORK


def test_a_log_naming_no_session_falls_back_to_its_first_declared_connection(
    tmp_path: Path,
) -> None:
    """Rule three. A recording of a connection that only ever carried
    gateway-level traffic still belongs to that connection."""
    path = tmp_path / "quiet.jsonl"
    recorder = FrameRecorder(
        path,
        "ws://gateway.invalid/",
        connections=(
            RecordedConnection(profile=WORK, endpoint="ws://work.invalid/"),
            RecordedConnection(profile=LAB, endpoint="ws://lab.invalid/"),
        ),
    )
    recorder.view(LAB).record(
        "in",
        json.dumps({"jsonrpc": "2.0", "method": "event",
                    "params": {"type": "gateway.ready", "payload": {}}}),
    )
    recorder.close()

    source = TaggedReplaySource.from_path(path)
    assert source.focus_profile == WORK, "the header's own order was not used"


@pytest.mark.asyncio
async def test_the_focus_derivation_is_what_makes_a_replay_render_anything(
    tmp_path: Path,
) -> None:
    """The reason this is not a cosmetic output to assert.

    ``route_frame`` feeds the focused engine only frames whose profile equals
    ``focused_profile``, and ``_adopt_profile`` never runs in replay. So a tagged
    log replayed at the constructor's default renders an **empty transcript** —
    both halves are asserted here, because "the derived profile is LAB" alone
    would be satisfied by a value nothing reads.
    """
    from talaria.domain.state import FleetState, route_frame

    path = tmp_path / "spoken.jsonl"
    recorder = FrameRecorder(
        path,
        "ws://gateway.invalid/",
        connections=(
            RecordedConnection(profile=WORK, endpoint="ws://work.invalid/"),
            RecordedConnection(profile=LAB, endpoint="ws://lab.invalid/"),
        ),
    )
    recorder.view(WORK).record("in", frame("s-work"))
    recorder.view(WORK).record(
        "in",
        json.dumps({"jsonrpc": "2.0", "method": "event",
                    "params": {"type": "message.complete", "session_id": "s-work",
                               "payload": {"text": "the work connection said this"}}}),
    )
    recorder.view(LAB).record("in", frame("s-lab"))
    recorder.close()

    source = TaggedReplaySource.from_path(path, controls=instant())
    items = [item async for item in source]

    def transcript_len(focus: str) -> int:
        fleet = FleetState(focused_profile=focus)
        for item in items:
            decoded = decode_frame(item.record.frame, at=item.record.at, seq=item.record.seq)
            fleet = route_frame(fleet, decoded, profile=item.profile, generation=item.epoch)
        return len(fleet.focused.transcript)

    assert transcript_len("default") == 0, "the precondition does not hold"
    assert transcript_len(source.focus_profile) > 0, (
        "replaying at the derived profile still rendered nothing, so the "
        "derivation is not reaching the engine that draws the transcript"
    )


@pytest.mark.asyncio
async def test_a_replayed_fleet_does_not_report_its_own_recording_as_down(
    tmp_path: Path,
) -> None:
    """A recording exists because those gateways answered.

    ``ConnectionChannel.connected`` defaults False, so without the mount-time
    fold every replayed connection made the queue say "connection down before it
    was ever polled" — about gateways that demonstrably answered, since the log
    is the evidence they did. Measured before the fix: both connections of a
    two-connection replay reported exactly that.

    This states a recorded fact rather than assuming one. A connection that
    really did drop mid-recording is marked down again by the log's own terminal
    cause when replay reaches it.
    """
    from talaria.ui.app import TalariaApp

    source = TaggedReplaySource.from_path(
        two_connection_log(tmp_path / "fleet.jsonl"), controls=instant()
    )
    app = TalariaApp(source, mode="replay", controls=source.controls,
                     current_profile=source.focus_profile)

    async with app.run_test() as pilot:
        await pilot.pause()

        channels = app.fleet.channels
        assert sorted(channels) == sorted({WORK, LAB}), (
            "the recording's declared connections never reached the fleet"
        )
        assert all(channel.connected for channel in channels.values()), (
            "a replayed connection is reported down, which is a false sentence "
            "about a recording made while it was answering"
        )
        assert not any("connection down" in notice for notice in app.needs_you.notices), (
            "the queue tells the operator a recorded connection was never up"
        )
        await app.shutdown_sources()
