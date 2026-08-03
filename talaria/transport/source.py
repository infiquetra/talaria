"""KTD3's one frame-source seam: ``FrameSource``.

Everything above this seam is identical code in replay and live mode, which is
what makes R31 and AE16 testable rather than aspirational. The seam is
deliberately narrow — an async iterator of :class:`FrameRecord` plus an
idempotent :meth:`FrameSource.close` — because every widening is a place where
replay and live can quietly diverge.

Why the record carries ``parse_error`` as a flag rather than a message: the
frame log records an unparseable frame as a *withheld hole* (``frame: null``
plus a categorical diagnostic, R26), and the decoder turns that flag into a
``ProtocolErrorFrame`` whose text is a fixed sentence. Passing the recorder's
diagnostic string through here would create a second path by which wire content
could reach a rendered line.

``close()`` is part of the protocol, not a courtesy. R36 requires teardown to
stop what Talaria started, and a consumer that stops iterating without closing
is a test failure rather than a tolerated pattern — :class:`ReplaySource` and
the live source both assert it.
"""

from __future__ import annotations

from collections.abc import AsyncIterator
from dataclasses import dataclass
from typing import Any, Literal, Protocol, runtime_checkable

#: Which way a frame travelled. Matches frame-log v1's ``dir`` field and
#: :data:`talaria.domain.models.Direction`; re-declared here so the transport
#: package does not import the domain to name a two-member enum.
Direction = Literal["in", "out"]


@dataclass(frozen=True)
class FrameRecord:
    """One frame as it crossed the seam, before any protocol interpretation.

    ``at`` is epoch seconds taken from the record that carried the frame — the
    recorded time in replay, the receive time live — never a clock read at
    render time. That is what makes replaying one corpus twice produce
    identical domain state (AE2).
    """

    seq: int
    at: float
    direction: Direction
    frame: Any
    parse_error: bool = False


@runtime_checkable
class FrameSource(Protocol):
    """An async iterator of frames that can be closed exactly once.

    Declared as a ``Protocol`` rather than an abstract base class so a test
    double is a source without inheriting anything — the seam is a shape, not a
    hierarchy.
    """

    def __aiter__(self) -> AsyncIterator[FrameRecord]:
        """Iterate frames in recorded order."""
        ...

    async def close(self) -> None:
        """Stop the source. Idempotent; cancels any in-flight read."""
        ...
