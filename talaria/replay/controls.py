"""Replay transport controls, and the inert-control discipline (R40, AE11).

Two responsibilities that look unrelated and are not.

**Pause, resume, and speed** are the controls replay *adds*. They scale the
recorded ``at`` deltas; they never touch domain state. That separation is what
makes AE11's determinism claim true: replaying the same corpus at 1x, at 8x,
and paused-then-resumed produces the same final :class:`SessionState`, because
speed changes only how long the source sleeps between frames.

**Mutation controls** are the controls replay must *refuse*. In replay mode
there is no gateway, so submitting a message, interrupting a sub-agent, or
answering a prompt has nowhere to go. The dangerous failure is not the refusal
— it is a UI that quietly does something local instead. A composed message
echoed into the transcript would be indistinguishable on screen from a message
that was actually sent, so this module answers with an explicit, renderable
:class:`ControlOutcome` and the UI shows it. Refusing loudly is the requirement.
"""

from __future__ import annotations

import asyncio
import math
from dataclasses import dataclass
from typing import Final

#: Slowest and fastest *finite* replay rates. Below 0.25x a long corpus is
#: unwatchable; above 64x the recorded deltas are already below the coalescing
#: tick, so a finite multiplier stops meaning anything and
#: :data:`UNBOUNDED_SPEED` is the honest way to ask for "as fast as possible".
MIN_SPEED: Final[float] = 0.25
MAX_SPEED: Final[float] = 64.0

#: "Maximum replay speed" in KTD14's threshold sense: no inter-frame delay at
#: all. Distinguished from a large finite multiplier so the gate measures the
#: renderer rather than the sleep loop.
UNBOUNDED_SPEED: Final[float] = math.inf

#: The multiplicative step ``speed_up`` / ``slow_down`` take.
SPEED_STEP: Final[float] = 2.0

#: Longest gap the source will ever sleep, before speed scaling. A real corpus
#: contains minutes of operator think-time between turns; replaying those
#: faithfully would stall the interface on a gap that carries no information.
#: Recorded ordering and recorded ``at`` values are untouched — only the wall
#: clock the replay waits on is clamped, so domain state is unaffected.
MAX_GAP_SECONDS: Final[float] = 0.25

#: Controls that would change something on the gateway. Every one of them is
#: inert in replay mode (AE11). The set is named rather than inferred so a
#: control added later has to be classified deliberately.
MUTATION_CONTROLS: Final[frozenset[str]] = frozenset(
    {
        "submit",
        "interrupt",
        "prompt-respond",
        "command-dispatch",
        "paste-collapse",
        "session-create",
    }
)

#: Shown by the composer and by any mutation control that is invoked. Short
#: enough for a one-line affordance, explicit enough that it cannot be read as
#: "sent".
INERT_NOTICE: Final[str] = "replay — not connected"


@dataclass(frozen=True)
class ControlOutcome:
    """What happened when a control was invoked.

    ``performed`` is false for every mutation control in replay mode, and the
    ``notice`` is what the UI renders. A caller that ignores ``performed`` and
    acts anyway is the bug this type exists to make visible in review.
    """

    name: str
    performed: bool
    notice: str = ""

    @property
    def inert(self) -> bool:
        return not self.performed


class ReplayControls:
    """Pause/resume/speed state plus the mutation-control refusal.

    Not a Textual object and not a domain object: the UI binds keys to it and
    the source reads it, so both can be tested without the other.
    """

    def __init__(self, *, speed: float = 1.0, paused: bool = False) -> None:
        self._speed = _clamp_speed(speed)
        self._resumed = asyncio.Event()
        if not paused:
            self._resumed.set()
        #: Every inert invocation, in order, so a test can assert that a
        #: control was reached and refused rather than silently never bound.
        self.refusals: list[ControlOutcome] = []

    # ── pause / resume ───────────────────────────────────────────────────

    @property
    def paused(self) -> bool:
        return not self._resumed.is_set()

    def pause(self) -> None:
        self._resumed.clear()

    def resume(self) -> None:
        self._resumed.set()

    def toggle_pause(self) -> bool:
        """Flip the pause state and return the new value."""
        if self.paused:
            self.resume()
        else:
            self.pause()
        return self.paused

    async def wait_while_paused(self) -> None:
        """Block until resumed. Returns immediately when running."""
        await self._resumed.wait()

    # ── speed ────────────────────────────────────────────────────────────

    @property
    def speed(self) -> float:
        return self._speed

    @property
    def unbounded(self) -> bool:
        return math.isinf(self._speed)

    def set_speed(self, speed: float) -> float:
        self._speed = _clamp_speed(speed)
        return self._speed

    def speed_up(self) -> float:
        if self.unbounded:
            return self._speed
        return self.set_speed(self._speed * SPEED_STEP)

    def slow_down(self) -> float:
        if self.unbounded:
            return self.set_speed(MAX_SPEED)
        return self.set_speed(self._speed / SPEED_STEP)

    def set_unbounded(self) -> float:
        self._speed = UNBOUNDED_SPEED
        return self._speed

    def delay_for(self, gap_seconds: float) -> float:
        """Scale a recorded inter-frame gap into a wall-clock sleep.

        Negative gaps are clamped to zero: a corpus whose ``at`` values go
        backwards is a recorder defect, and sleeping a negative interval would
        turn it into a replay defect too.
        """
        if self.unbounded:
            return 0.0
        gap = min(max(gap_seconds, 0.0), MAX_GAP_SECONDS)
        return gap / self._speed

    @property
    def label(self) -> str:
        """One short string for the status region."""
        rate = "max" if self.unbounded else f"{self._speed:g}x"
        return f"paused · {rate}" if self.paused else f"replaying · {rate}"

    # ── mutation refusal ─────────────────────────────────────────────────

    def attempt(self, name: str) -> ControlOutcome:
        """Invoke a named control. Mutation controls are refused in replay.

        A name outside :data:`MUTATION_CONTROLS` is *not* silently permitted —
        it raises, because an unclassified control reaching this call site
        means someone added a control without deciding whether it mutates.
        """
        if name not in MUTATION_CONTROLS:
            raise ValueError(
                f"unclassified control {name!r}: add it to MUTATION_CONTROLS or "
                "handle it locally, but do not route it through the refusal path"
            )
        outcome = ControlOutcome(name=name, performed=False, notice=INERT_NOTICE)
        self.refusals.append(outcome)
        return outcome


def _clamp_speed(speed: float) -> float:
    if math.isinf(speed) and speed > 0:
        return UNBOUNDED_SPEED
    if math.isnan(speed) or speed <= 0:
        return MIN_SPEED
    return min(max(speed, MIN_SPEED), MAX_SPEED)
