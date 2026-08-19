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
