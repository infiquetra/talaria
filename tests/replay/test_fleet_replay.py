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


#  ── the gate's fleet checkpoints ─────────────────────────────────────────


@pytest.mark.asyncio
async def test_the_checkpoint_trace_is_identical_at_two_speeds(tmp_path: Path) -> None:
    """"Same checkpoints, twice, byte-identical" — the unit's own goal.

    One checkpoint per frame rather than a sample: the corpus is small by design
    and the claim is stronger when the checkpoints are all of them.
    """
    from talaria.replay.gate import replay_fleet_trace
    from talaria.replay.stress import build_fleet_corpus

    corpus = build_fleet_corpus()
    fast = await replay_fleet_trace(corpus, speed=64.0)
    unbounded = await replay_fleet_trace(corpus, speed=float("inf"))

    assert len(fast) == len(corpus.records), (
        "the trace has fewer checkpoints than frames, so it compared a prefix"
    )
    assert fast == unbounded


@pytest.mark.asyncio
async def test_the_checkpoint_trace_notices_a_corpus_it_should_notice() -> None:
    """The falsifier, and without it the four gate checks measure nothing.

    A determinism comparison passes trivially when both sides are constant, and
    the cheap way to build one is to fingerprint something that does not vary.
    Swapping which connection carried which frame is a change the trace MUST
    see — the registry keys move between profiles, the focused engine gets
    different traffic, and the queue and its ages follow. Asserted per aspect,
    because a single blob differing tells you nothing about which of the four
    was actually watching.
    """
    import dataclasses

    from talaria.replay.gate import replay_fleet_trace
    from talaria.replay.stress import build_fleet_corpus

    corpus = build_fleet_corpus()
    baseline = await replay_fleet_trace(corpus)

    # **Two perturbations, not one, because one was never guaranteed to reach
    # four aspects.** A single swap covered all four until the corpus's
    # declaration order changed, at which point the ages stopped moving under it
    # — the rendered strings are the same whichever connection holds the item.
    # A falsifier that happens to cover an aspect is a falsifier that stops
    # covering it the day the fixture moves.
    # Collapse every frame onto one connection — precisely what a reader that
    # ignored the tag would do, and the harm the version bump exists to prevent.
    # NOT a reversal: the corpus alternates connections over an odd number of
    # frames, so reversing its profiles yields the identical tuple and perturbs
    # nothing. A falsifier that silently perturbs nothing is worse than none.
    moved_by_swap = await replay_fleet_trace(
        dataclasses.replace(corpus, profiles=(corpus.connections[0],) * len(corpus.records))
    )
    # Shift the clock so the same items are older at every checkpoint. Only the
    # ages can see this: the rows, the queue and the focus are unchanged.
    stretched = dataclasses.replace(
        corpus,
        records=tuple(
            dataclasses.replace(record, at=record.at + index * 60.0)
            for index, record in enumerate(corpus.records)
        ),
    )
    moved_by_clock = await replay_fleet_trace(stretched)

    for aspect in ("registry", "queue", "focus"):
        pairs = list(zip(baseline, moved_by_swap, strict=True))
        assert any(first[aspect] != second[aspect] for first, second in pairs), (
            f"the {aspect} fingerprint did not move when the connections did, so "
            f"the gate check reading it is measuring a constant"
        )

    age_pairs = list(zip(baseline, moved_by_clock, strict=True))
    assert any(first["ages"] != second["ages"] for first, second in age_pairs), (
        "the ages fingerprint did not move when the corpus's own clock did, so "
        "the gate check reading it is measuring a constant"
    )


def test_the_fleet_corpus_carries_the_blind_approval_the_gate_must_see() -> None:
    """U8's corpus obligation, as a property of the corpus rather than a note.

    The unit's answer to the operator's keyless-approval question made U6's
    unplaceable fold load-bearing. A corpus with no keyless approval leaves the
    gate blind to the mechanism that answer made permanent, and a green run
    stands in for coverage it does not have.
    """
    from talaria.replay.stress import build_fleet_corpus

    corpus = build_fleet_corpus()
    assert corpus.keyless_approval_count >= 1
    # And one that IS aimable, so the corpus exercises both sides of the rule.
    assert any(
        isinstance(record.frame, dict)
        and record.frame["params"].get("payload", {}).get("request_id")
        for record in corpus.records
    ), "the corpus has no correlated approval, so it tests only the exception"
    assert build_fleet_corpus().sha256 == corpus.sha256, "the corpus is not deterministic"


@pytest.mark.asyncio
async def test_the_rendered_ages_come_from_the_corpus_clock_not_a_wall_clock() -> None:
    """The check the determinism comparison cannot be, and why it exists.

    Probed while building this unit: replacing the bar's frame clock with
    ``time.time()`` leaves **both** replays' rendered ages identical, because
    ``format_age`` rounds to whole seconds and two replays of a six-frame corpus
    finish milliseconds apart. No corpus size fixes that — a wall clock is stable
    to the second across two runs however long the recording is. So a
    determinism comparison over rendered ages is green under exactly the defect
    it looks like it would catch.

    What separates the two clocks is what the age SHOULD be: inside the corpus's
    own span for the frame clock, and the seconds since the recording was made
    for a wall clock. Measured under the mutation: 2,105,719 against a span of 6.

    The bound is an upper one and the clean run measures 0, so this does not
    prove the ages are *right* — only that they are not read from a clock that
    has nothing to do with the recording. Said here rather than left for a
    reader to assume the check is stronger than it is.
    """
    from talaria.replay.gate import _rendered_age_seconds, replay_fleet_trace
    from talaria.replay.stress import build_fleet_corpus

    corpus = build_fleet_corpus()
    span = max(r.at for r in corpus.records) - min(r.at for r in corpus.records)
    trace = await replay_fleet_trace(corpus)

    ages = [_rendered_age_seconds(checkpoint["ages"]) for checkpoint in trace]
    assert any(age is not None for age in ages), (
        "no checkpoint rendered an age at all, so this check is vacuous"
    )
    assert all(age is None or age <= span + 1.0 for age in ages), (
        "a rendered age lies outside the corpus's own time span, so the surface "
        "is reading a clock that has nothing to do with the recording"
    )


def test_a_session_title_cannot_corrupt_the_age_the_gate_measures() -> None:
    """The age parser reads Talaria's own words, never the gateway's.

    Found by probing this unit's own check: the first version parsed digits out
    of the whole ages fingerprint, which contains the summary line — and the
    summary line carries the session TITLE. A session called "deploy in 900s"
    made the parser report 900 against a corpus span of 6, failing the gate on a
    name somebody chose.

    It inflates rather than deflates, since the parser takes the maximum, so the
    corruption failed the gate CLOSED — the safer direction. A gate an operator's
    own session title can break is still a gate that gets disbelieved the first
    time it happens.
    """
    from talaria.replay.gate import _rendered_age_seconds

    hostile = "deploy in 900s"
    summary_shaped = repr((f"needs-you: 1 · waiting 3s · driven · {hostile}", ("waiting 3s",), ""))
    wait_lines_only = repr(("waiting 3s",))

    assert _rendered_age_seconds(summary_shaped) == 900.0, (
        "the precondition no longer holds; the summary line no longer carries "
        "the title, and this test's premise needs rechecking"
    )
    assert _rendered_age_seconds(wait_lines_only) == 3.0, (
        "the age parser reads something other than the wait lines, so gateway "
        "text can still reach it"
    )


def test_the_gate_check_itself_is_unmoved_by_a_hostile_session_title() -> None:
    """The wiring, driven rather than described.

    The first version of this test asserted only that the trace HAS a wait-lines
    key and that the key excludes the summary. Both true, and both survived a
    mutation pointing the gate's own parser back at the summary line — because
    the check was computed inline in ``run_gate`` and nothing but a full gate run
    could reach it. ``ages_within_corpus_span`` was factored out for exactly that
    reason, and this drives it with a checkpoint whose SUMMARY carries a title
    the parser must not see.
    """
    from talaria.replay.gate import ages_within_corpus_span
    from talaria.replay.stress import build_fleet_corpus

    corpus = build_fleet_corpus()
    hostile = {
        "registry": "",
        "queue": "",
        "focus": "",
        # A title naming an age far outside the corpus span, in the place the
        # gateway's own text lands.
        "ages": repr(("needs-you: 1 · waiting 3s · driven · deploy in 900s", ("waiting 3s",), "")),
        "wait_lines": repr(("waiting 3s",)),
    }

    passes, largest = ages_within_corpus_span([hostile], corpus)
    assert passes, "an operator's session title failed the gate"
    assert largest == 3.0, (
        "the measured age came from somewhere other than the wait lines, so "
        "gateway text reaches the number this check compares"
    )


#  ── CR8's blocking findings, pinned ──────────────────────────────────────


def two_landing_log(path: Path) -> Path:
    """The recording U7's own `/profiles` flow produces: land, switch, land again."""
    recorder = FrameRecorder(
        path,
        "ws://gateway.invalid/",
        connections=(
            RecordedConnection(profile=WORK, endpoint="ws://work.invalid/"),
            RecordedConnection(profile=LAB, endpoint="ws://lab.invalid/"),
        ),
    )
    def resume(session: str) -> str:
        return json.dumps(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "session.resume",
                "params": {"session_id": session},
            }
        )
    recorder.view(LAB).record("out", resume("sess-lab"))
    recorder.view(LAB).record("in", frame("sess-lab"))
    recorder.view(WORK).record("out", resume("sess-work"))
    for _ in range(3):
        recorder.view(WORK).record(
            "in",
            json.dumps({"jsonrpc": "2.0", "method": "event",
                        "params": {"type": "message.complete", "session_id": "sess-work",
                                   "payload": {"text": "work said this"}}}),
        )
    recorder.close()
    return path


def test_the_last_landing_call_wins_not_the_first(tmp_path: Path) -> None:
    """CR8's F14, and the flow that produces it is one U7 shipped.

    ``/profiles`` moves home and the next session is created on the connection
    just selected, so a recording of that has a landing call on the connection
    the operator LEFT and one on the connection they moved to. Taking the first
    focuses the one they moved away from — and then ``route_frame`` feeds the
    focused engine nothing, which is the empty transcript this derivation exists
    to prevent. Measured before the fix: 0 transcript entries against 6.
    """
    source = TaggedReplaySource.from_path(two_landing_log(tmp_path / "twice.jsonl"))
    assert source.focus_profile == WORK, (
        "the derivation focused the connection the operator moved away from"
    )


@pytest.mark.asyncio
async def test_the_derived_focus_renders_the_traffic_the_run_was_driving(
    tmp_path: Path,
) -> None:
    """The consequence half of F14, because the profile name alone proves nothing."""
    from talaria.domain.state import FleetState, route_frame
    from talaria.recorder.reader import read_frame_log

    path = two_landing_log(tmp_path / "twice.jsonl")
    log = read_frame_log(path)
    source = TaggedReplaySource.from_path(path)

    def transcript_at(focus: str) -> int:
        fleet = FleetState(focused_profile=focus)
        for entry in log.entries:
            decoded = decode_frame(entry.frame, at=float(entry.seq), seq=entry.seq)
            fleet = route_frame(fleet, decoded, profile=entry.profile, generation=1)
        return len(fleet.focused.transcript)

    assert transcript_at(LAB) == 0, "the precondition no longer holds"
    assert transcript_at(source.focus_profile) > 0, (
        "the derived focus still renders nothing, which is the defect itself"
    )


def test_the_fleet_corpus_can_tell_the_derivation_rules_apart() -> None:
    """CR8's F16: a corpus whose rules agree cannot test what chooses between them.

    The gate's ``fleet_derived_focus`` check ran with an EMPTY entry sequence, so
    only rule three was ever exercised — and two mutations of the derivation
    (return the last connection; return nothing at all) both left it passing.
    The corpus now declares its connections in the opposite order to the one its
    frames speak in, so rule two and rule three give different answers.
    """
    from talaria.recorder.reader import FrameLogHeader, RecordedConnectionRow
    from talaria.replay.gate import corpus_entries
    from talaria.replay.source import derive_focus_profile
    from talaria.replay.stress import build_fleet_corpus

    corpus = build_fleet_corpus()
    header = FrameLogHeader(
        version=2,
        started_at="",
        endpoint="",
        connections=tuple(
            RecordedConnectionRow(profile=name, endpoint="") for name in corpus.connections
        ),
    )

    from_entries = derive_focus_profile(header, corpus_entries(corpus))
    from_header_only = derive_focus_profile(header, ())
    assert from_entries != from_header_only, (
        "the corpus cannot distinguish the derivation's rules, so the gate check "
        "over it cannot fail for any mutation of the derivation"
    )
    assert from_entries == corpus.profiles[0]


def test_the_gate_refuses_a_recording_it_would_have_to_mis_measure(tmp_path: Path) -> None:
    """CR8's F15: the gate held the path and chose the untagged reader.

    A tagged recording replayed as one connection merges two gateways' equal
    session ids into a session that never existed — and the gate published
    ``determinism_identical: true`` over it. That is verbatim the harm the frame
    log's version bump exists to prevent, and this gate was the reader that did
    not stop.

    Refusing is not the finished answer — per-connection gate replay is filed as
    its own work — but it is the half that is correct on its own: a gate that
    cannot measure a file must not publish a measurement of it.
    """
    import asyncio

    from talaria.replay.gate import GateError, run_gate

    path = two_connection_log(tmp_path / "fleet.jsonl")
    with pytest.raises(GateError) as caught:
        asyncio.run(run_gate(deltas=50, live_corpus=str(path)))
    assert "multi-connection" in str(caught.value)
    assert "Refused rather than measured" in str(caught.value)


def test_the_registry_fingerprint_sees_a_dropped_delta() -> None:
    """CR8's F17, driven rather than described.

    Two fleets differing in ONE field. ``message_count`` is where a
    speed-dependent dropped delta shows, and the previous fingerprint omitted
    it — rows differing only in that field hashed identically. The falsifier
    beside this one cannot reach it: collapsing the fleet moves the registry
    through its keys, so it stays green whether or not the field is present.
    """
    import dataclasses

    from talaria.domain.registry import RegistryRow
    from talaria.domain.state import FleetState
    from talaria.replay.gate import registry_fingerprint

    row = RegistryRow(profile=WORK, durable_id="s-work", seeded_at=1.0, message_count=12)
    base = FleetState(focused_profile=WORK, rows={(WORK, "s-work"): row})
    fewer = FleetState(
        focused_profile=WORK, rows={(WORK, "s-work"): dataclasses.replace(row, message_count=3)}
    )

    assert registry_fingerprint(base) != registry_fingerprint(fewer), (
        "two registries differing by nine messages fingerprint identically, so "
        "a replay that dropped deltas at speed would pass the determinism check"
    )

    mid_turn = FleetState(
        focused_profile=WORK, rows={(WORK, "s-work"): dataclasses.replace(row, open_turn=True)}
    )
    assert registry_fingerprint(base) != registry_fingerprint(mid_turn), (
        "a row mid-turn fingerprints the same as a settled one"
    )


@pytest.mark.asyncio
async def test_the_trace_derives_its_focus_from_the_corpus_not_the_header() -> None:
    """CR8's F16, driven rather than described — the third instance of that shape.

    The test beside this one proves the CORPUS can tell the derivation's rules
    apart. It does not prove ``replay_fleet_trace`` passes the entries that make
    rules one and two reachable, and it stayed green under a mutation restoring
    the empty sequence. This asserts the wiring: the trace's focused profile is
    the entries-derived answer, which the corpus is built to make differ from
    the header-derived one.
    """
    from talaria.replay.gate import replay_fleet_trace
    from talaria.replay.stress import build_fleet_corpus

    corpus = build_fleet_corpus()
    trace = await replay_fleet_trace(corpus)

    focused = trace[-1]["focus"]
    assert corpus.profiles[0] in focused, (
        "the trace focused the header's first connection, so the derivation ran "
        "on an empty entry sequence and two of its three rules are unreachable"
    )
    assert corpus.connections[0] not in focused, (
        "the trace focused the first DECLARED connection rather than the one the "
        "corpus's frames name"
    )
