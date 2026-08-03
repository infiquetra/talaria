"""``ReplaySource`` — KTD3's seam, backed by a frame-log v1 recording.

The gate's whole claim (R30) rests on this module: the entire interface runs
from a file, with no socket open anywhere in the process. Nothing here imports
``websockets``, and nothing here dials anything.

**Recorded time and wall time are kept strictly apart.** ``FrameRecord.at``
carries the *recorded* observation time and is what the domain reducer sees, so
replaying one corpus twice produces identical state (AE2). The wall-clock sleep
between two frames is a separate number derived from the recorded gap through
:class:`~talaria.replay.controls.ReplayControls`, and changing it — pausing,
resuming, 8x, unbounded — cannot change domain state. That is why AE11's
"deterministic identical final state at any speed" is a property of the design
rather than a hope.
"""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator, Iterable, Sequence
from pathlib import Path

from talaria.domain.normalize import parse_frame_time
from talaria.recorder.reader import FrameLogEntry, FrameLogHeader, iter_frame_log, read_header
from talaria.replay.controls import ReplayControls
from talaria.transport.source import Direction, FrameRecord

#: How many zero-delay frames the source emits before handing control back to
#: the event loop. Small enough that a 50ms render tick is never missed by more
#: than a fraction of its period; large enough that a scheduler hop is not paid
#: per frame.
YIELD_EVERY = 64

__all__ = [
    "YIELD_EVERY",
    "ReplaySource",
    "load_frame_records",
    "load_header",
    "parse_frame_time",
    "record_from_entry",
]


def record_from_entry(entry: FrameLogEntry) -> FrameRecord:
    """Adapt one frame-log record to the seam's :class:`FrameRecord`.

    ``parseError``'s *text* is deliberately dropped. R26 keeps wire content out
    of diagnostics, and the decoder already turns the flag into a fixed
    protocol-error sentence; carrying the recorder's message across the seam
    would open a second route for a payload fragment to reach a rendered line.

    Time parsing is delegated to :func:`talaria.domain.normalize.parse_frame_time`
    rather than reimplemented, so replay and the domain's own fixtures cannot
    disagree about what a recorded timestamp means. Its unparseable-value
    fallback is ``0.0`` rather than a clock read, which is what keeps AE2's
    "replay the same corpus twice, get the same state" true even for a damaged
    record.
    """
    direction: Direction = "out" if entry.dir == "out" else "in"
    return FrameRecord(
        seq=entry.seq,
        at=parse_frame_time(entry.at),
        direction=direction,
        frame=entry.frame,
        parse_error=entry.parse_error is not None,
    )


def load_frame_records(path: str | Path) -> tuple[FrameRecord, ...]:
    """Read a whole frame log into seam records.

    Materializes the corpus. That is the right trade here: the gate replays one
    file repeatedly at different speeds and must compare final states, and
    re-reading the file per pass would make disk timing part of the
    measurement.
    """
    return tuple(record_from_entry(entry) for entry in iter_frame_log(path))


def load_header(path: str | Path) -> FrameLogHeader:
    return read_header(path)


class ReplaySource:
    """Yield recorded frames on a scaled clock, honouring pause and speed.

    Satisfies :class:`~talaria.transport.source.FrameSource`. ``close()`` is
    idempotent and cancels an in-flight sleep, so a consumer that stops early
    leaves nothing pending.
    """

    def __init__(
        self,
        records: Sequence[FrameRecord] | Iterable[FrameRecord],
        *,
        controls: ReplayControls | None = None,
    ) -> None:
        self._records: tuple[FrameRecord, ...] = tuple(records)
        self.controls = controls if controls is not None else ReplayControls()
        self._closed = False
        self._sleep_task: asyncio.Task[None] | None = None
        self._emitted = 0

    @classmethod
    def from_path(
        cls, path: str | Path, *, controls: ReplayControls | None = None
    ) -> ReplaySource:
        return cls(load_frame_records(path), controls=controls)

    def __len__(self) -> int:
        return len(self._records)

    @property
    def emitted(self) -> int:
        """How many frames have crossed the seam so far."""
        return self._emitted

    @property
    def closed(self) -> bool:
        return self._closed

    @property
    def records(self) -> tuple[FrameRecord, ...]:
        return self._records

    async def __aiter__(self) -> AsyncIterator[FrameRecord]:
        previous: float | None = None
        since_yield = 0
        for record in self._records:
            if self._closed:
                return
            await self.controls.wait_while_paused()
            if self._closed:
                return
            if previous is not None:
                delay = self.controls.delay_for(record.at - previous)
                if delay > 0:
                    await self._sleep(delay)
                    if self._closed:
                        return
                    since_yield = 0
                else:
                    # Zero delay means nothing above awaited, and an async
                    # generator that never awaits never returns control to the
                    # event loop. Without this the pump starves its own render
                    # timer: the whole corpus lands in domain state and the
                    # screen updates once at the end, which would make the
                    # gate's render-tick measurement meaningless and, worse,
                    # would freeze a live session during a burst. Yielding
                    # every :data:`YIELD_EVERY` frames keeps the coalescing
                    # tick alive without paying a scheduler hop per frame.
                    since_yield += 1
                    if since_yield >= YIELD_EVERY:
                        since_yield = 0
                        await asyncio.sleep(0)
                        if self._closed:
                            return
            previous = record.at
            self._emitted += 1
            yield record

    async def _sleep(self, delay: float) -> None:
        """Sleep in a cancellable task so ``close()`` interrupts it at once."""
        task = asyncio.ensure_future(asyncio.sleep(delay))
        self._sleep_task = task
        try:
            await task
        except asyncio.CancelledError:
            if not self._closed:
                raise
        finally:
            self._sleep_task = None

    async def close(self) -> None:
        """Stop the source. Safe to call twice, and safe to call mid-sleep."""
        if self._closed:
            return
        self._closed = True
        # A paused source is parked on the resume event; releasing it lets the
        # loop observe ``_closed`` and return instead of hanging on teardown.
        self.controls.resume()
        task = self._sleep_task
        if task is not None and not task.done():
            task.cancel()
