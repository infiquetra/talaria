"""The startup compatibility check (R34, AE7) — the half of U3's baseline that runs.

``talaria.domain.compat`` records what each required gateway method looked like at
Hermes ``7f4d15515``. This module is the only thing that compares a live gateway
against that record, and its whole design is set by one constraint: **discovering
whether a method exists must not be a way of using it.**

That constraint is not decorative. The terminal gateway publishes no capability
endpoint, so the obvious way to learn whether ``session.create`` is still there is
to call it — and calling it creates a session. R34 forbids exactly that, so this
module probes only the five methods U3 classified ``read-only`` after reading
their handlers at the pin, and every other required method is reported as
*unverified at runtime* rather than quietly assumed present. Under-claiming is
the point: a check that says "I did not test this" is worth more than one that
says "fine" because it never looked.

**How absence is recognized.** The gateway's dispatcher answers an unregistered
name with JSON-RPC ``-32601`` and the text ``unknown method: <name>``
(``tui_gateway/server.py:1762`` at ``7f4d15515``). Any *other* error code means
the request reached a registered handler and that handler refused it, which is
evidence the method exists — it is not evidence that its response shape is
unchanged, and the two are reported as different verdicts for that reason.

**What blocks the ready verdict.** A missing method, a drifted response shape,
and a probe whose outcome is unknown all block (AE7: blocked on any gap). A
refusal of the probe's own fixture does not block, because a fixture this module
chose is not something the gateway got wrong — it is named in the report instead.

**Blocking the verdict is not refusing to run.** AE7 governs the daily-driver
verdict, not the process: a client that quits on any drift is less useful than
one that starts and says which surface it could not verify. So
:class:`~talaria.ui.app.TalariaApp` names the gaps and continues, and
``docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`` is where the verdict
itself is blocked.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any, Literal, Protocol

from talaria.domain.compat import (
    BASELINE_PIN,
    COMPAT_BASELINE,
    EVIDENCE_ONLY_METHODS,
    READ_ONLY_METHODS,
    Classification,
    MethodBaseline,
    ShapeDrift,
    compare_shape,
)
from talaria.transport.rpc import RpcOutcome

__all__ = [
    "BLOCKING_STATUSES",
    "METHOD_NOT_FOUND",
    "CompatReport",
    "MethodVerdict",
    "ProbeDispatcher",
    "VerdictStatus",
    "check_compatibility",
    "probe_method",
    "request_for",
]

#: JSON-RPC code the gateway's dispatcher returns for a name it has no handler
#: for: ``_err(rid, -32601, f"unknown method: {method}")``
#: (``tui_gateway/server.py:1762`` at ``7f4d15515``). This is the one error that
#: means *absent*; every other code means a handler answered.
METHOD_NOT_FOUND: int = -32601

#: What a probe concluded about one method.
#:
#: * ``present``     — answered, and its top-level response signature matches.
#:   **Top-level only.** :func:`~talaria.domain.compat.compare_shape` compares
#:   the response's own key set and each value's kind; a payload nested inside
#:   one of those values can have changed completely and this still reads
#:   ``present``. That is U3's deliberate v0.1 scope (``compat.py`` documents
#:   why), and it is stated here as well because ``present`` is the word an
#:   operator reads and it sounds broader than it is.
#: * ``missing``     — answered ``-32601``: the gateway has no such method.
#: * ``drifted``     — answered, but the signature no longer matches the pin.
#: * ``refused``     — a handler refused this probe's own fixture. Present, shape
#:   unproved.
#: * ``unproved``    — the call's outcome is unknown (no reply, lost transport,
#:   not connected). Nothing was learned.
#: * ``not-probed``  — evidence-only. Never called, by design (R34).
VerdictStatus = Literal["present", "missing", "drifted", "refused", "unproved", "not-probed"]

#: The statuses that block the daily-driver ready verdict (AE7).
#:
#: ``unproved`` is in here and that is deliberate. "The probe did not come back"
#: is a gap in the evidence, and AE7 says the verdict is blocked on any gap — a
#: check that treated silence as a pass would report ready for a gateway it never
#: reached.
#:
#: ``not-probed`` is deliberately *not* in here. Every evidence-only method would
#: otherwise block every run forever, which would make the flag carry no
#: information at all; their absence from the runtime check is instead stated in
#: :meth:`CompatReport.lines` and carried as its own row in the verdict document.
BLOCKING_STATUSES: frozenset[str] = frozenset({"missing", "drifted", "unproved"})


class ProbeDispatcher(Protocol):
    """The one operation a compatibility probe needs.

    Structurally identical to :class:`~talaria.ui.app.LiveDispatcher`, and
    declared separately rather than imported: this module must not import the UI
    package to name a callable shape (ADR-0002 in the direction that matters —
    nothing below the seam reaches up).
    """

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome: ...


@dataclass(frozen=True)
class MethodVerdict:
    """What the check concluded about one required method."""

    method: str
    classification: Classification
    status: VerdictStatus
    drifts: tuple[ShapeDrift, ...] = ()
    detail: str = ""

    @property
    def blocks_ready(self) -> bool:
        return self.status in BLOCKING_STATUSES

    def describe(self) -> str:
        """One operator-facing line naming the method and what happened to it."""
        if self.status == "present":
            return (
                f"{self.method}: present, top-level response shape matches "
                f"{BASELINE_PIN}"
            )
        if self.status == "missing":
            return f"{self.method}: MISSING — this gateway has no such method"
        if self.status == "drifted":
            reasons = "; ".join(drift.describe() for drift in self.drifts)
            return f"{self.method}: DRIFTED — {reasons}"
        if self.status == "refused":
            return f"{self.method}: present but refused the probe fixture ({self.detail})"
        if self.status == "unproved":
            return f"{self.method}: UNPROVED — {self.detail or 'the probe never came back'}"
        return f"{self.method}: not probed (evidence-only at {BASELINE_PIN}; R34)"


@dataclass(frozen=True)
class CompatReport:
    """Every required method's verdict, plus the exact call log the probe made.

    ``probed`` is recorded rather than reconstructed. "No mutating method was
    invoked at startup" is a claim about what went onto the wire, and a claim
    about the wire that is checked by re-reading the code that was supposed to
    write it proves nothing. The stub gateway's own received-call record is what
    the tests assert against; this field is the same fact from Talaria's side, so
    a divergence between them is visible.
    """

    verdicts: tuple[MethodVerdict, ...]
    probed: tuple[str, ...]

    @property
    def blocking(self) -> tuple[MethodVerdict, ...]:
        return tuple(verdict for verdict in self.verdicts if verdict.blocks_ready)

    @property
    def ready(self) -> bool:
        """Whether the runtime check found nothing that blocks (AE7).

        A true value here is **not** the daily-driver verdict. It says the
        read-only methods matched the pin, and says nothing at all about the
        evidence-only ones, which are never called — see :meth:`lines`, which
        always states how many those are. (No count is written into this
        sentence on purpose: the split between the two sets is data in
        :mod:`talaria.domain.compat`, and a number in prose here would drift
        away from it silently, which it has already done once.)
        """
        return not self.blocking

    @property
    def missing(self) -> tuple[str, ...]:
        return tuple(v.method for v in self.verdicts if v.status == "missing")

    @property
    def drifted(self) -> tuple[str, ...]:
        return tuple(v.method for v in self.verdicts if v.status == "drifted")

    def verdict_for(self, method: str) -> MethodVerdict:
        for verdict in self.verdicts:
            if verdict.method == method:
                return verdict
        raise KeyError(f"{method} was not part of this compatibility check")

    def lines(self) -> tuple[str, ...]:
        """The operator-facing summary, blocking rows first.

        The unverified count is always stated, including on a clean run, and it
        is counted from the verdicts rather than written down here. A summary
        that said only "compatible" on the happy path would be claiming the
        never-probed methods had passed something.
        """
        not_probed = [v for v in self.verdicts if v.status == "not-probed"]
        head = (
            f"gateway compatibility: {len(self.blocking)} blocking, "
            f"{len(not_probed)} unverified at runtime (evidence-only, R34), "
            f"baseline {BASELINE_PIN}"
        )
        return (head, *[verdict.describe() for verdict in self.blocking])


def request_for(entry: MethodBaseline, session_id: str) -> dict[str, Any]:
    """The probe's request parameters: the pinned fixture, with a session id if one exists.

    U3's fixtures carry ``session_id: ""`` because the baseline records shapes,
    not a working call. When the caller knows a real session id, substituting it
    asks a session-scoped read a question it can actually answer.

    **At startup there is no such id, and that is the normal case, not a bug.**
    The only production caller is
    :meth:`~talaria.ui.app.TalariaApp.verify_gateway`, which by design runs
    *before* any session is opened, so ``session_id`` arrives empty and every
    session-scoped fixture goes out with ``session_id: ""``. A handler is
    entitled to refuse that, and ``refused`` is graded non-blocking precisely
    because such a refusal is this module's own doing rather than the gateway's.
    The substitution below is therefore exercised by tests and by any future
    caller that re-runs the check against an open session; it is not what
    happens on a bare ``talaria`` launch.
    """
    params = dict(entry.request_fixture)
    if session_id and "session_id" in params and not params["session_id"]:
        params["session_id"] = session_id
    return params


async def probe_method(
    dispatcher: ProbeDispatcher,
    entry: MethodBaseline,
    *,
    session_id: str = "",
    timeout: float | None = None,
) -> MethodVerdict:
    """Invoke one **read-only** method and grade the answer against the pin.

    Raises :class:`ValueError` for any entry U3 classified ``evidence-only``.
    That guard is reachable — ``tests/transport/test_compat_baseline.py`` calls
    this function with ``session.create`` and asserts it raises before any
    dispatcher call — because a guard nothing can exercise is a guard nobody can
    trust, and this one is the last thing standing between a startup probe and a
    created session.
    """
    if entry.classification != "read-only":
        raise ValueError(
            f"{entry.method} is classified {entry.classification} at {BASELINE_PIN}; "
            "a startup probe may only invoke the read-only set (R34, KTD9)"
        )

    outcome = await dispatcher.call(
        entry.method, request_for(entry, session_id), timeout=timeout
    )

    if outcome.status == "unknown":
        return MethodVerdict(
            method=entry.method,
            classification=entry.classification,
            status="unproved",
            detail=outcome.reason or "no reply",
        )

    if outcome.status == "error":
        if outcome.error_code == METHOD_NOT_FOUND:
            return MethodVerdict(
                method=entry.method,
                classification=entry.classification,
                status="missing",
                detail=outcome.error_message or "",
            )
        code = outcome.error_code
        detail = outcome.error_message or "no message"
        return MethodVerdict(
            method=entry.method,
            classification=entry.classification,
            status="refused",
            detail=f"code {code}: {detail}" if code is not None else detail,
        )

    # ``result`` is ``None`` when the gateway answered with a non-object result.
    # That is drift of the most basic kind — the baseline records a key set, and
    # there is no key set here — so it is reported as such rather than compared
    # against an empty mapping, which would report every recorded key as missing
    # and bury the actual fact.
    if outcome.result is None:
        return MethodVerdict(
            method=entry.method,
            classification=entry.classification,
            status="drifted",
            drifts=(
                ShapeDrift(
                    method=entry.method,
                    key="<result>",
                    kind="value-kind-changed",
                    expected="object",
                    observed="not an object",
                ),
            ),
        )

    drifts = compare_shape(entry, outcome.result)
    if drifts:
        return MethodVerdict(
            method=entry.method,
            classification=entry.classification,
            status="drifted",
            drifts=drifts,
        )
    return MethodVerdict(
        method=entry.method, classification=entry.classification, status="present"
    )


async def check_compatibility(
    dispatcher: ProbeDispatcher,
    *,
    session_id: str = "",
    timeout: float | None = None,
    baseline: Sequence[MethodBaseline] = COMPAT_BASELINE,
) -> CompatReport:
    """Probe the read-only set, record every other method as unverified.

    The iteration order is the baseline's own, so the report reads in the order
    U3 wrote the evidence down. Probes run in sequence rather than concurrently:
    a gateway answering a startup burst in parallel is not the thing under test,
    and a sequential call log is what makes "no mutating method was invoked"
    something a reader can check line by line.
    """
    verdicts: list[MethodVerdict] = []
    probed: list[str] = []

    for entry in baseline:
        if entry.classification != "read-only":
            verdicts.append(
                MethodVerdict(
                    method=entry.method,
                    classification=entry.classification,
                    status="not-probed",
                    detail=entry.evidence,
                )
            )
            continue
        probed.append(entry.method)
        verdicts.append(
            await probe_method(dispatcher, entry, session_id=session_id, timeout=timeout)
        )

    return CompatReport(verdicts=tuple(verdicts), probed=tuple(probed))


def unprobed_methods(report: CompatReport) -> tuple[str, ...]:
    """Required methods this check did not call. Always the evidence-only set.

    Written as a derivation from the report rather than as a re-export of
    :data:`~talaria.domain.compat.EVIDENCE_ONLY_METHODS`, so a probe that started
    calling a mutating method would change this answer instead of leaving a
    constant that still describes the intent.
    """
    return tuple(v.method for v in report.verdicts if v.status == "not-probed")


def probe_set_is_read_only(report: CompatReport) -> bool:
    """Whether the recorded call log stayed inside KTD9's read-only set."""
    called = set(report.probed)
    return called <= set(READ_ONLY_METHODS) and not called & set(EVIDENCE_ONLY_METHODS)
