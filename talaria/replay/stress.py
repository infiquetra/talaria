"""The synthetic stress corpus the gate's thresholds are measured against.

KTD14's numbers — mounted widgets, resident-memory growth, render ticks — are
stated against "a full 50k-delta replay". A real recorded session is the wrong
instrument for that: it proves the interface handles *actual* traffic (R30), but
its size is whatever the session happened to be, and a threshold measured
against an accidental number is not a threshold. So the gate replays both, and
this module generates the second one.

**Generated, not committed.** R29 keeps corpora out of version control, and a
50,000-frame file would be a corpus. Generation is seeded and deterministic, so
the gate's stress corpus is reproducible from its label alone — the results doc
records the seed and the sha256 of the generated frames, and re-running the
generator with that seed reproduces the digest exactly. That is a stronger
provenance claim than a checked-in blob, which nothing verifies.

**What it stresses, and why each part is there.** The frame mix is not a
uniform stream of deltas — it is chosen to hit the paths where a renderer
breaks:

* *Many completed turns*, so the mounted-widget cap is exercised by history
  that has to be condensed, not by one enormous streaming block.
* *Malformed and unknown frames* interleaved (R5), because the failure mode
  worth catching is a renderer that copes with 50,000 good frames and falls
  over on the one that is not an object.
* *Sub-agent fan-out* with terminal statuses (KTD8), so the rows region is
  under load at the same time as the transcript.
* *Wide, combining, and right-to-left characters* in the text (R11), so the
  measurement is not taken over pure ASCII the terminal never has to think
  about.
"""

from __future__ import annotations

import hashlib
import json
import random
from collections.abc import Iterator
from dataclasses import dataclass
from typing import Any, Final

from talaria.domain.state import SUBMIT_METHOD
from talaria.replay.source import SidebandAction, build_sideband
from talaria.transport.source import Direction, FrameRecord

#: Default size, straight from KTD14's "full 50k-delta replay".
DEFAULT_DELTA_COUNT = 50_000

#: Deltas per streaming turn. 80 gives ~625 turns at the default size, which is
#: comfortably more history than the 500-widget mount cap can hold — the point
#: of the corpus is that condensing happens hundreds of times, not once.
DELTAS_PER_TURN = 80

#: Recorded milliseconds between frames. Replay scales this; the gate runs
#: unbounded, so the value only shapes the recorded ``at`` series.
FRAME_INTERVAL_SECONDS = 0.004

#: Epoch seconds the generated corpus starts at. Fixed so two runs of the same
#: seed produce byte-identical records, timestamps included.
EPOCH_START = 1_785_000_000.0

#: Text fragments deltas are drawn from. Deliberately not ASCII-only: a wide
#: CJK character, a combining sequence, and an RTL run each change how a
#: terminal measures a cell (R11), and a corpus without them measures a
#: renderer that never had to do the hard part.
_FRAGMENTS: tuple[str, ...] = (
    "the quick brown fox ",
    "端末エミュレータの幅 ",
    "été combining ",
    "עברית rtl ",
    "streaming token ",
    "…ellipsis and — dash ",
    "🜁 alchemical glyph ",
    "tab\tseparated ",
)

_SUBAGENT_NAMES: tuple[str, ...] = ("indexer", "reviewer", "searcher", "summarizer")
_TERMINAL_STATUSES: tuple[str, ...] = ("completed", "failed", "interrupted", "timeout", "error")


@dataclass(frozen=True)
class StressCorpus:
    """A generated corpus plus the identity the results doc cites.

    ``label`` is opaque on purpose: this repository is public and R29 forbids
    citing a local path, so provenance is carried by seed, digest, and counts.
    """

    label: str
    seed: int
    records: tuple[FrameRecord, ...]
    delta_count: int
    turn_count: int
    sha256: str

    @property
    def frame_count(self) -> int:
        return len(self.records)


def build_stress_corpus(
    *, deltas: int = DEFAULT_DELTA_COUNT, seed: int = 20260802
) -> StressCorpus:
    """Generate the deterministic stress corpus.

    Same ``deltas`` and same ``seed`` always produce the same records and the
    same :attr:`StressCorpus.sha256`, which is what lets the results doc cite
    it without shipping the file.
    """
    records = tuple(_generate(deltas=deltas, seed=seed))
    digest = hashlib.sha256()
    for record in records:
        digest.update(
            json.dumps(
                {"seq": record.seq, "at": record.at, "frame": record.frame},
                sort_keys=True,
                separators=(",", ":"),
                ensure_ascii=False,
            ).encode("utf-8")
        )
    turns = max(1, deltas // DELTAS_PER_TURN)
    return StressCorpus(
        label=f"talaria-stress-v1-{deltas}d-seed{seed}",
        seed=seed,
        records=records,
        delta_count=deltas,
        turn_count=turns,
        sha256=digest.hexdigest(),
    )


def _event(kind: str, payload: dict[str, Any], session: str) -> dict[str, Any]:
    return {
        "jsonrpc": "2.0",
        "method": "event",
        "params": {"type": kind, "session_id": session, "payload": payload},
    }


def _generate(*, deltas: int, seed: int) -> Iterator[FrameRecord]:
    # A seeded Mersenne Twister is exactly right here. The corpus must be
    # *reproducible* from its seed, so the results doc can cite it by label
    # instead of shipping a 50,000-frame file; a cryptographic generator would
    # make that provenance claim impossible to check. Nothing here is a secret.
    rng = random.Random(seed)  # nosec B311
    session = "stress-session"
    seq = 0
    at = EPOCH_START

    def emit(frame: Any, *, parse_error: bool = False) -> FrameRecord:
        nonlocal seq, at
        seq += 1
        at += FRAME_INTERVAL_SECONDS
        return FrameRecord(
            seq=seq, at=at, direction="in", frame=frame, parse_error=parse_error
        )

    yield emit(_event("gateway.ready", {"skin": {"name": "stress"}}, session))
    yield emit(
        _event("session.info", {"session_id": session, "title": "stress corpus"}, session)
    )

    emitted_deltas = 0
    turn = 0
    while emitted_deltas < deltas:
        turn += 1
        yield emit(_event("message.start", {"session_id": session}, session))

        # A sub-agent fan-out on every fourth turn, so the rows region is under
        # load while the transcript streams (R14, R16).
        if turn % 4 == 0:
            for index in range(3):
                agent_id = f"agent-{turn}-{index}"
                yield emit(
                    _event(
                        "subagent.start",
                        {
                            # The payload keys the gateway actually sends
                            # (``turnController.ts:1013-1016``). Using a
                            # plausible-looking ``id``/``name`` pair instead
                            # would collapse every child onto one synthesized
                            # identity, and the fan-out this corpus exists to
                            # create would silently never happen.
                            "subagent_id": agent_id,
                            "goal": rng.choice(_SUBAGENT_NAMES),
                            "depth": 1,
                            "task_index": index,
                        },
                        session,
                    )
                )

        this_turn = min(DELTAS_PER_TURN, deltas - emitted_deltas)
        text_parts: list[str] = []
        for step in range(this_turn):
            fragment = rng.choice(_FRAGMENTS)
            if step and step % 16 == 0:
                fragment = f"{fragment}\n"
            text_parts.append(fragment)
            yield emit(_event("message.delta", {"text": fragment}, session))
            emitted_deltas += 1

            # Interleaved noise: one unparseable frame and one unknown event
            # type per turn, so the malformed path runs 600+ times over the
            # corpus rather than once at the end (R5).
            if step == this_turn // 3:
                yield emit(None, parse_error=True)
            elif step == (2 * this_turn) // 3:
                yield emit(
                    _event(f"stress.unknown.{turn % 7}", {"note": "unmodelled"}, session)
                )

        if turn % 4 == 0:
            for index in range(3):
                yield emit(
                    _event(
                        "subagent.complete",
                        {
                            "subagent_id": f"agent-{turn}-{index}",
                            "status": rng.choice(_TERMINAL_STATUSES),
                        },
                        session,
                    )
                )

        yield emit(
            _event(
                "message.complete",
                {"text": "".join(text_parts), "usage": {"input_tokens": 12, "output_tokens": 34}},
                session,
            )
        )

        # A frame that is well-formed JSON but not an object at all, every
        # eighth turn: the "frame-not-an-object" protocol-error path.
        if turn % 8 == 0:
            yield emit(["not", "an", "object"])


# ── U6: the feature corpus (gap 4) ──────────────────────────────────────────
#
# The stress corpus above measures KTD14's numbers against traffic mix, not
# against markdown *content* — its text fragments are plain prose, never a
# construct. This corpus is the other half: every R1 construct, early
# termination by cancel/error/typed-disconnect (mid-table included), the
# forgery/parser-attack payloads U3 already proves inert, a representative of
# every R7 kind group, and the sideband timeline (talaria/replay/source.py)
# that makes AE2's "early termination renders all received content" claim
# checkable at gate level. Fully scripted, not seeded-random — the point is
# specific, exact constructs, not statistical coverage — so there is no seed
# parameter; the "seed" a caller would look for is the corpus's own literal
# turn sequence, deterministic by construction.


@dataclass(frozen=True)
class FeatureCorpus:
    """The feature corpus's records, its sideband timeline, and its identity."""

    label: str
    records: tuple[FrameRecord, ...]
    sideband: tuple[SidebandAction, ...]
    sha256: str

    @property
    def frame_count(self) -> int:
        return len(self.records)


#: The exact markdown bodies each construct turn streams, named so a test can
#: assert against the same literal text this module generates rather than a
#: re-typed copy that could silently drift from it.
FEATURE_HEADING_TEXT = (
    "# Heading one\n\nAn opening paragraph with **bold**, _underscore emphasis_, "
    "and ~~strikethrough~~ (RA1)."
)
FEATURE_LIST_TEXT = (
    "- bullet one\n- bullet two\n- bullet three\n\n1. ordered one\n2. ordered two\n3. ordered three"
)
FEATURE_QUOTE_AND_ATTACKS_TEXT = (
    "> a quoted line\n> a second quoted line\n\n"
    "<script>alert(1)</script>\n\n"
    "[bold red]not rich markup[/]\n\n"
    "a bare url: https://example.com/bare\n\n"
    "![an image](https://example.com/pic.png)\n\n"
    "[a link](https://example.com/page)"
)
FEATURE_FENCE_TEXT = "```python\ndef f(x):\n    return x + 1\n```"
#: The table's three progressive fragments — header+separator+row1 as the
#: turn's first delta (already enough for markdown-it to recognize a table),
#: then one more row per delta. This is deliberately the shape that streams a
#: table across more than one ``EntryMarkdown.append`` call, because that is
#: the shape KTD1(d)'s own workload harness found corrupts (see
#: ``talaria/replay/workloads.py``'s module docstring, "A real, pre-existing
#: defect") — the feature corpus does not dodge that shape to keep the gate
#: green; it streams a real table the realistic way and lets the ownership
#: proof say what it finds.
FEATURE_TABLE_FRAGMENTS: tuple[str, ...] = (
    "| col |\n| --- |\n| r1 |",
    "\n| r2 |",
    "\n| r3 |",
)
FEATURE_MID_TABLE_FRAGMENTS: tuple[str, ...] = ("| a | b |\n| --- | --- |\n| x | y |",)


#: One session for the whole corpus, exactly like build_stress_corpus's own
#: constant ``session``. ``_apply_event`` (state.py) adopts the *first*
#: session id it ever sees on the wire as ``focused_session_id`` and then
#: silently drops every event carrying a different one as cross-talk — a
#: corpus that varied the session id per turn would have every turn after
#: the first discarded before ever reaching a handler, not committed and
#: not visible, which is not what this corpus is testing.
FEATURE_SESSION_ID = "feature-session"


def _session_id(turn: int) -> str:
    return FEATURE_SESSION_ID


def build_feature_corpus() -> FeatureCorpus:
    """Every R1 construct, early termination by cancel/error/typed-disconnect
    (mid-table included), parser attacks, a representative of every R7 kind
    group, and the sideband timeline those early-termination turns need.

    Deterministic by construction — no seed, nothing random — because this
    corpus exists to hit exact, named shapes, not to sample a distribution.
    """
    records: list[FrameRecord] = []
    sideband_actions: list[SidebandAction] = []
    seq = 0
    at = EPOCH_START

    def emit(frame: Any, *, direction: Direction = "in", parse_error: bool = False) -> None:
        nonlocal seq, at
        seq += 1
        at += FRAME_INTERVAL_SECONDS
        records.append(
            FrameRecord(seq=seq, at=at, direction=direction, frame=frame, parse_error=parse_error)
        )

    def streamed_turn(
        turn: int, deltas: tuple[str, ...], *, complete: bool = True
    ) -> None:
        session = _session_id(turn)
        emit(_event("message.start", {"session_id": session}, session))
        text_so_far: list[str] = []
        for fragment in deltas:
            text_so_far.append(fragment)
            emit(_event("message.delta", {"text": fragment}, session))
        if complete:
            emit(
                _event(
                    "message.complete",
                    {"text": "".join(text_so_far), "usage": {}},
                    session,
                )
            )

    # gateway.ready / session.info: the same opening pair every corpus in
    # this package starts with (build_stress_corpus's own shape).
    emit(_event("gateway.ready", {"skin": {"name": "feature"}}, FEATURE_SESSION_ID))
    emit(
        _event(
            "session.info",
            {"session_id": FEATURE_SESSION_ID, "title": "feature corpus"},
            FEATURE_SESSION_ID,
        )
    )

    # R7 kind group: operator. A replayed outbound prompt.submit, recovered
    # by replayed_submission_text (state.py) exactly as a real replay does.
    emit(
        {"method": SUBMIT_METHOD, "params": {"text": "please demonstrate every construct"}},
        direction="out",
    )

    # R1: heading + paragraph + RA1's emphasis/strikethrough allowlist.
    streamed_turn(1, (FEATURE_HEADING_TEXT,))

    # R1: both list types.
    streamed_turn(2, (FEATURE_LIST_TEXT,))

    # R1: block quote. Plus the AE3 parser-attack quartet U3 already proves
    # inert at the widget level — carried here so the *corpus* exercises them
    # end to end, not only the unit-level factory test.
    streamed_turn(3, (FEATURE_QUOTE_AND_ATTACKS_TEXT,))

    # R1: fence region, closed normally.
    streamed_turn(4, (FEATURE_FENCE_TEXT,))

    # R1: table grid, streamed progressively across three deltas and closed
    # normally (message.complete) — see FEATURE_TABLE_FRAGMENTS's docstring.
    streamed_turn(5, FEATURE_TABLE_FRAGMENTS)

    # R7 kind group: reasoning, alongside assistant prose in the same turn
    # (R18 — the domain holds both buffers at once).
    session6 = _session_id(6)
    emit(_event("message.start", {"session_id": session6}, session6))
    emit(_event("reasoning.delta", {"text": "# thinking about it\n\nreasoning body"}, session6))
    emit(_event("message.delta", {"text": "an assistant reply streamed alongside"}, session6))
    emit(
        _event(
            "message.complete",
            {"text": "an assistant reply streamed alongside", "usage": {}},
            session6,
        )
    )

    # R7 kind group: activity (tool).
    session7 = _session_id(7)
    emit(_event("message.start", {"session_id": session7}, session7))
    emit(_event("tool.start", {"name": "search", "context": "the codebase"}, session7))
    emit(_event("tool.complete", {"name": "search", "summary": "3 matches"}, session7))
    emit(_event("message.delta", {"text": "found it"}, session7))
    emit(_event("message.complete", {"text": "found it", "usage": {}}, session7))

    # R7 kind group: activity (subagent fan-out), the same payload shape
    # build_stress_corpus already uses.
    session8 = _session_id(8)
    emit(_event("message.start", {"session_id": session8}, session8))
    emit(
        _event(
            "subagent.start",
            {"subagent_id": "feature-agent-1", "goal": "indexer", "depth": 1, "task_index": 0},
            session8,
        )
    )
    emit(
        _event(
            "subagent.complete",
            {"subagent_id": "feature-agent-1", "status": "completed"},
            session8,
        )
    )
    emit(_event("message.delta", {"text": "subagent finished"}, session8))
    emit(_event("message.complete", {"text": "subagent finished", "usage": {}}, session8))

    # AE2 at gate level, arm 1: early termination by confirmed-cancel, plain
    # content (not mid-table) — an open fence, cancelled mid-stream. The
    # sideband action's frame_index is 1-based against this whole records
    # list, so it is captured *after* streamed_turn appends this turn's
    # frames, from len(records) at that point.
    streamed_turn(9, ("```text\nan unclosed fence, ", "cancelled mid-stream"), complete=False)
    sideband_actions.append(SidebandAction(frame_index=len(records), kind="confirmed_cancel"))

    # AE2 at gate level, arm 2: early termination by confirmed-cancel,
    # *mid-table* — cancelled right after the header/separator/first row, the
    # exact case the plan names: "the parser reinterprets the unterminated
    # table as a paragraph".
    streamed_turn(10, FEATURE_MID_TABLE_FRAGMENTS, complete=False)
    sideband_actions.append(SidebandAction(frame_index=len(records), kind="confirmed_cancel"))

    # AE2 at gate level, arm 3: early termination by error (a real wire
    # event, R7's fault kind group — this one needs no sideband at all).
    streamed_turn(11, ("partial content before an error arrives",), complete=False)
    session11 = _session_id(11)
    emit(_event("error", {"message": "the gateway reported a failure"}, session11))

    # AE2 at gate level, arm 4: early termination by typed-disconnect (KTD7)
    # — not a wire frame either; the transport callback this represents is
    # never recorded, which is exactly why the sideband exists.
    streamed_turn(12, ("partial content before the connection drops",), complete=False)
    sideband_actions.append(
        SidebandAction(frame_index=len(records), kind="typed_disconnect", cause="orderly_close")
    )

    # R7 kind group: fault (protocol-error / unknown-event), the same two
    # shapes build_stress_corpus already interleaves.
    emit(None, parse_error=True)
    emit(_event("feature.unknown.type", {"note": "unmodelled"}, FEATURE_SESSION_ID))
    emit(["not", "an", "object"])

    record_tuple = tuple(records)
    digest = hashlib.sha256()
    for record in record_tuple:
        digest.update(
            json.dumps(
                {"seq": record.seq, "at": record.at, "frame": record.frame},
                sort_keys=True,
                separators=(",", ":"),
                ensure_ascii=False,
            ).encode("utf-8")
        )
    return FeatureCorpus(
        label="talaria-feature-v1",
        records=record_tuple,
        sideband=build_sideband(sideband_actions),
        sha256=digest.hexdigest(),
    )


#  ── U8: the fleet corpus, and the keyless approval it must contain ───────


@dataclass(frozen=True)
class FleetCorpus:
    """A synthetic two-connection recording, with its tags beside its records.

    Synthetic rather than captured, and the choice is recorded rather than
    assumed: no frame log is committed to this repository (R29), and the only
    local recordings are version-1 files with no ``profile`` key anywhere. A
    live two-gateway capture cannot exist until a build carrying U8's recorder
    fix has been run against two gateways, which is a U9 activity. So the gate's
    fleet checkpoints run on a corpus this function generates, and that is
    stated here rather than left for a reader to infer from a fixture's absence.

    ``sha256`` covers the **profile as well as** the frame. The stress corpus's
    own digest hashes ``{seq, at, frame}`` only, which is right for a
    single-connection file and wrong here: two corpora differing solely in which
    connection carried which frame would otherwise hash identically, and the
    identity that names a corpus in a results document would not name it.
    """

    label: str
    records: tuple[FrameRecord, ...]
    profiles: tuple[str, ...]
    connections: tuple[str, ...]
    sha256: str

    @property
    def keyless_approval_count(self) -> int:
        """How many ``approval.request`` frames carry no ``request_id``.

        U8's operator-assigned question came back *yes* — a keyless approval can
        arrive from a supported input — which made U6's unplaceable fold
        load-bearing rather than insurance. A corpus that never contains one
        would leave the gate blind to the mechanism that answer made permanent.
        """
        return sum(
            1
            for record in self.records
            if isinstance(record.frame, dict)
            and isinstance(record.frame.get("params"), dict)
            and record.frame["params"].get("type") == "approval.request"
            and not record.frame["params"].get("payload", {}).get("request_id")
        )


#: The header's declaration order, which is DELIBERATELY not the order the
#: frames speak in.
#:
#: ``work-gateway`` sends the corpus's first frame and ``lab-gateway`` is
#: declared first, so ``derive_focus_profile``'s rule two (the first session
#: named) and rule three (the header's first connection) give DIFFERENT answers.
#: With them agreeing — as they did when this corpus was first written — the
#: gate's ``fleet_derived_focus`` check could not tell which rule had run, and
#: mutating the derivation left it passing. A corpus that cannot distinguish the
#: rules cannot test the thing that chooses between them.
FLEET_CORPUS_CONNECTIONS: Final[tuple[str, ...]] = ("lab-gateway", "work-gateway")


def build_fleet_corpus(*, base_time: float = 1_785_000_000.0) -> FleetCorpus:
    """Two connections, interleaved, including one approval with no request id.

    Deterministic by construction — no clock read, no randomness — so two calls
    produce identical records and the gate can cite the digest.
    """
    lab, work = FLEET_CORPUS_CONNECTIONS
    script: list[tuple[str, dict[str, Any]]] = [
        (work, {"type": "message.start", "session_id": "s-work", "payload": {}}),
        (lab, {"type": "message.start", "session_id": "s-lab", "payload": {}}),
        (
            work,
            {
                "type": "message.complete",
                "session_id": "s-work",
                "payload": {"text": "the work gateway answered"},
            },
        ),
        # The keyless approval. No ``request_id`` anywhere in the payload, which
        # is what the pinned gateway `7f4d15515` emitted for every approval.
        (
            lab,
            {
                "type": "approval.request",
                "session_id": "s-lab",
                "payload": {"description": "rm -rf /data", "choices": ["once", "deny"]},
            },
        ),
        # And one that IS aimable, so the corpus exercises both sides of the
        # rule rather than only the exceptional one.
        (
            work,
            {
                "type": "approval.request",
                "session_id": "s-work",
                "payload": {
                    "request_id": "gw-1",
                    "description": "git push --force",
                    "choices": ["once", "deny"],
                },
            },
        ),
        (lab, {"type": "message.complete", "session_id": "s-lab", "payload": {"text": "and lab"}}),
        # **One more frame on the FOCUSED connection, after its approval.**
        # Without it that approval is the focused connection's last frame, so the
        # focused clock never advances past its opening and every rendered age in
        # this corpus is "waiting 0s" — which made the gate's age checks compare
        # zeros with zeros. An age that is always zero is stable for a reason
        # that has nothing to do with the property being measured.
        (
            work,
            {
                "type": "message.complete",
                "session_id": "s-work",
                "payload": {"text": "and the work gateway again, later"},
            },
        ),
    ]

    records: list[FrameRecord] = []
    profiles: list[str] = []
    for index, (profile, params) in enumerate(script):
        records.append(
            FrameRecord(
                seq=index + 1,
                at=base_time + index,
                direction="in",
                frame={"jsonrpc": "2.0", "method": "event", "params": params},
            )
        )
        profiles.append(profile)

    digest = hashlib.sha256()
    for record, profile in zip(records, profiles, strict=True):
        digest.update(
            json.dumps(
                {
                    "seq": record.seq,
                    "at": record.at,
                    "frame": record.frame,
                    "profile": profile,
                },
                sort_keys=True,
                separators=(",", ":"),
                ensure_ascii=False,
            ).encode("utf-8")
        )
    return FleetCorpus(
        label="talaria-fleet-v1",
        records=tuple(records),
        profiles=tuple(profiles),
        connections=FLEET_CORPUS_CONNECTIONS,
        sha256=digest.hexdigest(),
    )
