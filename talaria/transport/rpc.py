"""Request/response correlation with honest lost-outcome semantics (R35, AE8).

Three rules, and every one of them exists because the obvious implementation is
wrong in a way that only shows up under a race.

**An interrupted call resolves to ``unknown``, never to success.** When the
socket closes after a request went out but before its reply came back, Talaria
does not know what happened — the gateway may have run the method, may have
crashed first, may have run it and lost the reply. AE8 requires that outcome be
reported as unknown. The failure this prevents is not cosmetic: reporting an
unconfirmed submit as sent invites the operator to conclude the message was
delivered, and reporting an unconfirmed interrupt as done invites them to
conclude the turn stopped.

**The correlation key is ``(epoch, id)``, not ``id``.** The epoch is an integer
incremented on every successful dial. Without it, a reply that arrives late from
a socket already declared dead can satisfy an identifier minted *after*
reconnect, turning an honest ``unknown`` into a reported success — exactly what
R35 forbids, and reachable only under a reconnect race. Replies whose epoch is
not the current one are counted and discarded (:attr:`RpcCorrelator.stale_epoch_replies`).

**Identifiers restart at 1 on every epoch.** That is deliberate, and it is what
makes the rule above load-bearing rather than decorative. A globally monotonic
counter would make identifier reuse impossible, so the epoch guard would never
fire and would never be tested — a guard that cannot be exercised is a guard
nobody can trust. One connection is in flight at a time, so restarting is also
what a freshly dialled client would naturally do.

Nothing in this module opens a socket or imports ``websockets``. It is a pure
bookkeeping object over futures, which is why the epoch race is testable without
a network.
"""

from __future__ import annotations

import asyncio
from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any, Literal

from talaria.domain.decode import is_reply_frame

__all__ = [
    "LOST_WITH_TRANSPORT",
    "NEVER_SENT",
    "NOT_CONNECTED",
    "NO_REPLY_IN_TIME",
    "RpcCorrelator",
    "RpcOutcome",
    "RpcRequest",
    "RpcStatus",
    "unknown_outcome",
]

#: What is known about a call. ``unknown`` is a first-class outcome rather than
#: an error, because "we do not know" is the truthful answer and an exception
#: would push callers toward treating it as a failure — which is a different,
#: also-untrue claim.
RpcStatus = Literal["ok", "error", "unknown"]

#: Reported when the transport dropped with the request in flight.
LOST_WITH_TRANSPORT = "the connection dropped before the gateway answered"

#: Reported when writing the request to the socket raised.
#:
#: Deliberately *not* "the request was never sent". ``connection.send()`` can
#: raise after a partial write, so what is known is that the write failed --
#: not that nothing reached the gateway. The distinction decides whether a
#: resend is safe, which is why it is worth the longer sentence.
NEVER_SENT = "writing the request to the socket failed"

#: Reported when no reply arrived within the caller's deadline.
NO_REPLY_IN_TIME = "no reply arrived before the deadline"

#: Reported when a call was attempted with no live connection.
NOT_CONNECTED = "not connected to a gateway"


@dataclass(frozen=True)
class RpcRequest:
    """One outbound JSON-RPC request, already assigned its epoch and id."""

    epoch: int
    id: str
    method: str
    params: Mapping[str, Any] = field(default_factory=dict)

    def to_frame(self) -> dict[str, Any]:
        """The wire frame, matching the gateway's own shape.

        ``tui_gateway.server.dispatch`` answers with
        ``{"jsonrpc": "2.0", "id": rid, "result"|"error": …}``
        (``tui_gateway/server.py:1719-1724`` at ``7f4d15515``), and
        ``tui_gateway/ws.py`` reads newline-delimited JSON in both directions
        with "no framing differences" from stdio.
        """
        return {
            "jsonrpc": "2.0",
            "id": self.id,
            "method": self.method,
            "params": dict(self.params),
        }


@dataclass(frozen=True)
class RpcOutcome:
    """What is known about one call. Never claims more than that."""

    status: RpcStatus
    method: str
    request_id: str
    epoch: int
    result: Mapping[str, Any] | None = None
    error_code: int | None = None
    error_message: str | None = None
    reason: str | None = None
    #: The frame-log sequence number the reply itself was read on, or
    #: ``None`` when the outcome never had a reply frame to read one from
    #: (``unknown``: no connection, a send that raised, or a timed-out
    #: wait). Mirrors :attr:`~talaria.domain.models.GatewayEvent.seq`,
    #: which is stamped from the same counter (``TalariaSource._seq``) — so
    #: a caller that also has a ``PendingPrompt.seq`` can compare the two
    #: to ask "did this exist before or after my reply arrived", the
    #: causal boundary :func:`~talaria.ui.app.TalariaApp.deny_all_approvals_live`
    #: needs and asyncio's own scheduling order does not reliably give (B3):
    #: the reply's wakeup and a concurrently-arriving event's wakeup both
    #: reach the loop through a different number of ``call_soon`` hops, so
    #: which one actually runs first is not something either coroutine may
    #: assume.
    seq: int | None = None

    @property
    def confirmed(self) -> bool:
        """True only for a reply the gateway actually sent and that said ``result``.

        Deliberately not named ``ok`` or ``success``: a caller writing
        ``if outcome.success`` around an ``unknown`` reads as though the two
        remaining cases are both failures, and one of them is not.
        """
        return self.status == "ok"

    @property
    def unknown(self) -> bool:
        return self.status == "unknown"

    @property
    def notice(self) -> str:
        """One operator-facing line. Empty for a confirmed call.

        The unknown wording says what is and is not known, because "failed"
        would be a claim and "sent" would be a lie.
        """
        if self.status == "ok":
            return ""
        if self.status == "error":
            code = f" (code {self.error_code})" if self.error_code is not None else ""
            refusal = f"{self.method} was refused by the gateway{code}"
            return f"{refusal}: {self.error_message}" if self.error_message else refusal
        if self.reason == NOT_CONNECTED:
            # The one unknown that is definite about delivery: ``begin()`` was
            # never called, so nothing was written to any socket. Saying "may
            # or may not have taken effect" here would understate what is
            # known and discourage the one resend that is actually safe.
            return f"{self.method} was not sent — {self.reason}. It cannot have taken effect."
        return (
            f"{self.method} outcome unknown — {self.reason or 'no reply'}. "
            "It may or may not have taken effect."
        )


@dataclass
class _Pending:
    request: RpcRequest
    future: asyncio.Future[RpcOutcome]


class RpcCorrelator:
    """The id→future map, qualified by connection epoch.

    Epoch ``0`` means "never connected". :meth:`begin` refuses to mint a request
    against it rather than producing an id nothing can ever answer.
    """

    def __init__(self) -> None:
        self._epoch = 0
        self._next_id = 0
        self._pending: dict[tuple[int, str], _Pending] = {}

        #: Replies read from a connection that is no longer the current one.
        #: Counted rather than ignored: a nonzero value here is the observable
        #: trace of the race the epoch key exists to defeat.
        self.stale_epoch_replies = 0
        #: Replies on the current epoch whose id matches nothing outstanding.
        self.unmatched_replies = 0
        #: Calls that resolved to ``unknown``. AE8's headline number.
        self.lost_outcomes = 0

    # ── epochs ───────────────────────────────────────────────────────────

    @property
    def epoch(self) -> int:
        return self._epoch

    def open_epoch(self) -> int:
        """Declare a new connection live. Returns the new epoch.

        The id counter restarts here. See the module docstring: reuse is the
        condition the epoch guard exists for, so making reuse impossible would
        quietly retire the guard.
        """
        self._epoch += 1
        self._next_id = 0
        return self._epoch

    # ── requests ─────────────────────────────────────────────────────────

    def begin(
        self, method: str, params: Mapping[str, Any] | None = None
    ) -> tuple[RpcRequest, asyncio.Future[RpcOutcome]]:
        """Mint a request and register the future its reply will resolve."""
        if self._epoch == 0:
            raise RuntimeError("no connection epoch is open; dial before calling begin()")
        self._next_id += 1
        request = RpcRequest(
            epoch=self._epoch,
            id=str(self._next_id),
            method=method,
            params=dict(params or {}),
        )
        future: asyncio.Future[RpcOutcome] = asyncio.get_running_loop().create_future()
        self._pending[(request.epoch, request.id)] = _Pending(request=request, future=future)
        return request, future

    @property
    def in_flight(self) -> int:
        return len(self._pending)

    def is_pending(self, epoch: int, request_id: str) -> bool:
        return (epoch, request_id) in self._pending

    # ── replies ──────────────────────────────────────────────────────────

    def resolve(self, frame: Any, *, epoch: int, seq: int | None = None) -> bool:
        """Resolve the call a reply belongs to. Returns whether it matched.

        ``epoch`` is the epoch of the connection the frame was **read from**,
        supplied by the reader loop. Passing the correlator's own epoch here
        would defeat the entire guard, which is why it is a required keyword.

        ``seq`` is the frame-log sequence number the reply itself was read
        on, threaded straight onto the resulting :class:`RpcOutcome` (B3) —
        see its own field docstring for what it is for.

        **The envelope is checked, not just the members**, by
        :func:`~talaria.domain.decode.is_reply_frame`. Without that check, any
        inbound object with a top-level ``id`` and ``result`` was accepted — and
        the gateway sends exactly such objects that are not replies. An event
        payload of the shape ``{"type": "tool.complete", "id": "1", "result":
        {…}}`` resolved an in-flight ``session.interrupt`` as a confirmed success
        and drove the turn to ``cancelled``, which is the precise harm
        :meth:`~talaria.ui.app.TalariaApp.interrupt_live` says must never happen:
        ``cancelled`` is sticky and suppresses the rest of a turn that never
        stopped.

        The predicate is imported rather than written here because the seam
        reconstruction reads recordings of this same wire and has to read it the
        same way. It did not, once, and condemned a seam on an event.
        """
        if not isinstance(frame, dict):
            return False
        if not is_reply_frame(frame):
            return False

        raw_id = frame.get("id")
        if raw_id is None:
            # A parse-error reply carries ``id: null``
            # (``tui_gateway/ws.py``). It answers no specific request.
            return False
        request_id = str(raw_id)

        if epoch != self._epoch:
            self.stale_epoch_replies += 1
            return False

        pending = self._pending.pop((epoch, request_id), None)
        if pending is None:
            self.unmatched_replies += 1
            return False
        if pending.future.done():  # pragma: no cover - defensive
            return False

        pending.future.set_result(_outcome_from_frame(pending.request, frame, seq=seq))
        return True

    # ── loss ─────────────────────────────────────────────────────────────

    def abandon(self, epoch: int, reason: str) -> int:
        """Resolve every in-flight call of ``epoch`` to ``unknown``.

        Returns how many were abandoned. Called by the transport the moment a
        disconnect is observed — before any reconnect — so no caller is ever
        left awaiting a future that a new socket can no longer answer.
        """
        keys = [key for key in self._pending if key[0] == epoch]
        for key in keys:
            pending = self._pending.pop(key)
            if pending.future.done():  # pragma: no cover - defensive
                continue
            pending.future.set_result(_unknown(pending.request, reason))
            self.lost_outcomes += 1
        return len(keys)

    def abandon_all(self, reason: str) -> int:
        """Resolve every in-flight call, on any epoch, to ``unknown``."""
        total = 0
        for epoch in {key[0] for key in self._pending}:
            total += self.abandon(epoch, reason)
        return total

    def discard(self, request: RpcRequest, reason: str) -> RpcOutcome:
        """Drop one specific call and report it unknown.

        Used for the two single-call losses a disconnect does not cover: a send
        that raised, and a deadline that expired.
        """
        self._pending.pop((request.epoch, request.id), None)
        self.lost_outcomes += 1
        return _unknown(request, reason)


def _unknown(request: RpcRequest, reason: str) -> RpcOutcome:
    return RpcOutcome(
        status="unknown",
        method=request.method,
        request_id=request.id,
        epoch=request.epoch,
        reason=reason,
    )


def unknown_outcome(method: str, reason: str, *, epoch: int = 0) -> RpcOutcome:
    """An ``unknown`` outcome for a call that never got as far as an id."""
    return RpcOutcome(
        status="unknown",
        method=method,
        request_id="",
        epoch=epoch,
        reason=reason,
    )


def _outcome_from_frame(
    request: RpcRequest, frame: Mapping[str, Any], *, seq: int | None
) -> RpcOutcome:
    error = frame.get("error")
    if isinstance(error, Mapping):
        raw_code = error.get("code")
        message = error.get("message")
        return RpcOutcome(
            status="error",
            method=request.method,
            request_id=request.id,
            epoch=request.epoch,
            error_code=raw_code if isinstance(raw_code, int) else None,
            error_message=message if isinstance(message, str) else None,
            seq=seq,
        )
    if "error" in frame:
        # An ``error`` member that is not an object is malformed. It is still a
        # refusal, not a success — reading past it into ``result`` is how a
        # malformed error frame becomes a reported success.
        return RpcOutcome(
            status="error",
            method=request.method,
            request_id=request.id,
            epoch=request.epoch,
            error_message="the gateway returned a malformed error",
            seq=seq,
        )

    result = frame.get("result")
    return RpcOutcome(
        status="ok",
        method=request.method,
        request_id=request.id,
        epoch=request.epoch,
        result=result if isinstance(result, Mapping) else None,
        seq=seq,
    )
