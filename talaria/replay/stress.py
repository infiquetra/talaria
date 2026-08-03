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
from typing import Any

from talaria.transport.source import FrameRecord

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
