"""The startup compatibility check (R34, AE7) — the half of U3's baseline that runs.

``talaria.domain.compat`` records what each required gateway method looked like at
Hermes ``7f4d15515``. This module is the only thing that compares a live gateway
against that record, and its whole design is set by one constraint: **discovering
whether a method exists must not be a way of using it.**

That constraint is not decorative. The terminal gateway publishes no capability
endpoint, so the obvious way to learn whether ``session.create`` is still there is
to call it — and calling it creates a session. R34 forbids exactly that, so this
module probes only the methods :mod:`talaria.domain.compat` classified
``read-only`` (invoked with their pinned fixture, answer compared) or
``presence-only`` (invoked bare, only the refusal read), and every other required
method is reported as *unverified at runtime* rather than quietly assumed
present. Under-claiming is the point: a check that says "I did not test this" is
worth more than one that says "fine" because it never looked. No count appears in
this paragraph on purpose — the split is data in ``compat.py``, and every earlier
version of this sentence that carried a number went stale within two units.

**How absence is recognized.** The gateway's dispatcher answers an unregistered
name with JSON-RPC ``-32601`` and the text ``unknown method: <name>``
(``tui_gateway/server.py:1762`` at ``7f4d15515``). Any *other* error code means
the request reached a registered handler and that handler refused it, which is
evidence the method exists — it is not evidence that its response shape is
unchanged, and the two are reported as different verdicts for that reason.

**Absence is never concluded from a request that carried parameters** (v0.4 R11).
``-32601`` on a parameterized call is re-asked bare before ``missing`` may be
returned, because the recorded ``absent_capability`` misdiagnosis is exactly what
happens without that second call: a 404 caused by a mistyped profile name was
read as a gateway too old to have the endpoint. The same discipline is what makes
a bare ``approval.pending`` a *presence test* — a handler that refuses a
parameterless call has proved it is registered (KTD11).

**Seams sit on top of method verdicts, and are what an operator reads.** A method
is a name; a seam is a capability whose absence turns a feature off *by name*
(R10). :func:`probe_seams` runs one round and returns both, and
:mod:`talaria.domain.compat` owns the vocabulary, the board and the rendering, so
none of it depends on a socket having been opened.

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
from typing import Any, Literal, Protocol, runtime_checkable

from talaria.domain.compat import (
    BASELINE_PIN,
    COMPAT_BASELINE,
    EVIDENCE_ONLY_METHODS,
    PROBEABLE_METHODS,
    SEAM_CATALOGUE,
    Classification,
    MethodBaseline,
    ProbeTrigger,
    SeamObservation,
    SeamStatus,
    ShapeDrift,
    compare_shape,
)
from talaria.domain.redaction import redact_probe_detail
from talaria.transport.rpc import RpcOutcome

__all__ = [
    "BLOCKING_STATUSES",
    "METHOD_NOT_FOUND",
    "CompatReport",
    "HealthAnswer",
    "HealthProbe",
    "MethodVerdict",
    "ProbeDispatcher",
    "VerdictStatus",
    "check_compatibility",
    "probe_method",
    "probe_presence",
    "probe_seams",
    "probe_set_is_permitted",
    "request_for",
    "seam_status_for_verdict",
    "unprobed_methods",
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
#: * ``parameter-invalid`` (U5, R11) — the *parameterized* request was refused
#:   with ``-32601``, and the **bare re-ask was not**. The name exists, and the
#:   parameters this probe chose were the problem. This is the verdict the
#:   recorded ``absent_capability`` misdiagnosis should have produced: it blamed
#:   a gateway's version for a mistyped profile name, and the whole reason it
#:   could is that nothing re-asked without the parameter before concluding.
VerdictStatus = Literal[
    "present",
    "missing",
    "drifted",
    "refused",
    "unproved",
    "not-probed",
    "parameter-invalid",
]

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
#:
#: ``parameter-invalid`` is not in here either, for the same reason ``refused``
#: is not: the request was this module's own, and a gateway is not at fault for
#: refusing a fixture Talaria chose. It is named in the report instead.
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
    #: Mirrors :attr:`~talaria.domain.compat.MethodBaseline.blocks_daily_driver`
    #: for the entry this verdict came from. Copied onto the verdict rather than
    #: looked up again, so a report stays readable without the baseline beside it.
    #:
    #: **Required, deliberately.** This field had a ``True`` default until
    #: 2026-08-17, on the reasoning that a verdict built for a method outside the
    #: baseline should have a defined answer. It did — the wrong one, silently.
    #: One of the fourteen construction sites in this module omitted it, and the
    #: entry it mis-graded was ``session.active_list``: the only probed method
    #: whose fixture is bare *and* whose baseline sets this ``False``, so an
    #: absent roster condemned the whole gateway rather than naming a disabled
    #: seam. A default that is right thirteen times out of fourteen is worse than
    #: no default, because the fourteenth is silent. Build verdicts with
    #: :func:`_verdict`, which fills this from the entry and cannot forget.
    blocks_daily_driver: bool
    drifts: tuple[ShapeDrift, ...] = ()
    detail: str = ""
    #: Whether absence was checked with a second, parameterless call (U5, R11).
    #: ``False`` on a method whose fixture was already bare, where a re-ask
    #: would send the identical request twice and prove nothing.
    bare_reasked: bool = False

    @property
    def blocks_ready(self) -> bool:
        """Whether this verdict blocks the daily-driver verdict (AE7).

        Two conditions, not one. The status has to be a gap *and* the method has
        to be one the daily-driver verdict is about: v0.4's fleet methods report
        their absence as a named seam (R10) and do not condemn a gateway that
        runs the single-session core perfectly.
        """
        return self.status in BLOCKING_STATUSES and self.blocks_daily_driver

    def describe(self) -> str:
        """One operator-facing line naming the method and what happened to it."""
        if self.status == "present":
            return (
                f"{self.method}: present, top-level response shape matches "
                f"{BASELINE_PIN}"
            )
        if self.status == "parameter-invalid":
            return (
                f"{self.method}: present, but refused the probe's parameters — "
                f"the bare re-ask was answered ({self.detail})"
            )
        if self.status == "missing":
            confirmed = (
                ", confirmed by a bare re-ask" if self.bare_reasked else ""
            )
            return (
                f"{self.method}: MISSING — this gateway has no such method"
                f"{confirmed}"
            )
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


def _error_detail(outcome: RpcOutcome) -> str:
    """A bounded, de-credentialled classification of one refusal (R22).

    The gateway's own message is kept because it is what tells an operator
    *which* refusal this is — ``session not found`` and ``request_id required``
    are different facts — but it goes through
    :func:`~talaria.domain.compat.redact_probe_detail` first, so a message that
    quotes a URL or a credential-shaped assignment cannot reach a probe
    diagnostic. The response *body* is never touched at all: only the error
    code and message are read.
    """
    message = redact_probe_detail(outcome.error_message) or "no message"
    code = outcome.error_code
    return f"code {code}: {message}" if code is not None else message


def _verdict(
    entry: MethodBaseline,
    *,
    status: VerdictStatus,
    detail: str = "",
    drifts: tuple[ShapeDrift, ...] = (),
    bare_reasked: bool = False,
) -> MethodVerdict:
    """Build a verdict for ``entry``, carrying everything the entry already knows.

    Three of :class:`MethodVerdict`'s fields — the method, its classification, and
    whether its absence blocks the daily-driver verdict — are copies of the
    baseline entry, and every probe in this module was spelling all three out at
    every return. Fourteen sites, three fields each: one of the forty-two was
    missing, and it was the one that mattered.

    So the copying happens once, here. A caller states only what the *probe*
    learned; what the *entry* is stays out of its hands. This is the same repair
    U3 and U4 each made in their own review rounds — when one fact is written in
    several places, the fix is to leave it in one, not to correct the copies.
    """
    return MethodVerdict(
        method=entry.method,
        classification=entry.classification,
        blocks_daily_driver=entry.blocks_daily_driver,
        status=status,
        detail=detail,
        drifts=drifts,
        bare_reasked=bare_reasked,
    )


async def _reask_bare(
    dispatcher: ProbeDispatcher,
    entry: MethodBaseline,
    *,
    parameterized_detail: str,
    timeout: float | None,
) -> MethodVerdict:
    """R11's second call: ask again with nothing, then decide.

    Only reached when a request that *carried parameters* was refused with
    ``-32601``. Four answers, three verdicts:

    * ``-32601`` again — absence confirmed, ``missing``.
    * any other error — a handler answered a bare call, so the name exists and
      the parameters were the problem: ``parameter-invalid``.
    * a successful answer — same conclusion, more emphatically. The shape is
      deliberately **not** compared here: this call did not carry the pinned
      fixture, so grading it against the pinned signature would report drift
      that is the re-ask's own doing.
    * unknown — nothing was learned by the second call, so nothing is claimed:
      ``unproved``, not the ``missing`` the first call suggested. Downgrading
      here is the point of the whole rule.
    """
    bare = await dispatcher.call(entry.method, {}, timeout=timeout)
    if bare.status == "unknown":
        return _verdict(
            entry,
            status="unproved",
            detail=(
                "the parameterized probe answered method-not-found and the bare "
                f"re-ask never came back ({bare.reason or 'no reply'})"
            ),
            bare_reasked=True,
        )
    if bare.status == "error" and bare.error_code == METHOD_NOT_FOUND:
        return _verdict(
            entry,
            status="missing",
            detail=redact_probe_detail(bare.error_message),
            bare_reasked=True,
        )
    settled = _error_detail(bare) if bare.status == "error" else "the bare call was answered"
    return _verdict(
        entry,
        status="parameter-invalid",
        detail=f"parameterized: {parameterized_detail}; bare: {settled}",
        bare_reasked=True,
    )


async def probe_presence(
    dispatcher: ProbeDispatcher,
    entry: MethodBaseline,
    *,
    timeout: float | None = None,
) -> MethodVerdict:
    """Learn whether a ``presence-only`` method exists, without using it (KTD11).

    One bare call. No session id, no parameters of any kind — the fixture is
    asserted empty rather than trusted, because the whole safety argument for
    probing ``approval.pending`` at all is that a parameterless call cannot reach
    the ``_wait_agent`` build-warming path behind ``_sess``.

    The verdict inverts the usual reading of a refusal, and deliberately: a
    handler that answers ``4001 session not found`` has *proved it is
    registered*, which is precisely R11's parameter-invalid distinction used as
    the presence test. Only ``-32601`` means absent. That distinction is not
    theoretical here — U1 measured both answers on this machine's wire
    (``docs/analysis/2026-08-17-v0-4-topology-verification.md``): the serving
    process answers ``-32601 unknown method: approval.pending``, while a present
    method with a bad session answers ``4001``.
    """
    if entry.classification != "presence-only":
        raise ValueError(
            f"{entry.method} is classified {entry.classification}; probe_presence "
            "may only be used on the presence-only set (KTD11)"
        )
    if entry.request_fixture:
        raise ValueError(
            f"{entry.method} is presence-only but its pinned fixture is not bare; "
            "a presence probe sends no parameters at all (KTD11)"
        )

    outcome = await dispatcher.call(entry.method, {}, timeout=timeout)
    if outcome.status == "unknown":
        return _verdict(entry, status="unproved", detail=outcome.reason or "no reply")
    if outcome.status == "error":
        if outcome.error_code == METHOD_NOT_FOUND:
            return _verdict(
                entry,
                status="missing",
                detail=redact_probe_detail(outcome.error_message),
            )
        return _verdict(
            entry,
            status="present",
            detail=f"proved by refusal — {_error_detail(outcome)}",
        )
    # A bare call that *succeeded* still proves presence, and its shape is not
    # compared: the bare request is not the pinned fixture, so a comparison here
    # would grade the gateway against a question this probe did not ask.
    return _verdict(entry, status="present", detail="the bare call was answered")


async def probe_method(
    dispatcher: ProbeDispatcher,
    entry: MethodBaseline,
    *,
    session_id: str = "",
    timeout: float | None = None,
) -> MethodVerdict:
    """Invoke one **read-only** method and grade the answer against the pin.

    Raises :class:`ValueError` for any entry U3 classified ``evidence-only`` and
    for the ``presence-only`` class U5 added (those go to :func:`probe_presence`,
    which is allowed to send nothing but a bare call). That guard is reachable —
    ``tests/transport/test_compat_baseline.py`` calls this function with
    ``session.create`` and asserts it raises before any dispatcher call — because
    a guard nothing can exercise is a guard nobody can trust, and this one is the
    last thing standing between a startup probe and a created session.

    **Absence is never claimed from a parameterized request alone (R11).** When
    a request that carried parameters comes back ``-32601``, the method is
    re-asked with no parameters at all before ``missing`` may be returned; if the
    bare call is answered, the verdict is ``parameter-invalid`` instead. A
    fixture that was already bare is not re-asked — sending the identical request
    a second time is not evidence, it is noise — and :attr:`MethodVerdict.bare_reasked`
    records which of the two happened rather than leaving a reader to infer it.
    """
    if entry.classification != "read-only":
        raise ValueError(
            f"{entry.method} is classified {entry.classification} at {BASELINE_PIN}; "
            "a startup probe may only invoke the read-only set (R34, KTD9)"
        )

    params = request_for(entry, session_id)
    outcome = await dispatcher.call(entry.method, params, timeout=timeout)

    if outcome.status == "unknown":
        return _verdict(entry, status="unproved", detail=outcome.reason or "no reply")

    if outcome.status == "error":
        if outcome.error_code == METHOD_NOT_FOUND:
            if not params:
                return _verdict(
                    entry,
                    status="missing",
                    detail=redact_probe_detail(outcome.error_message),
                )
            return await _reask_bare(
                dispatcher,
                entry,
                parameterized_detail=redact_probe_detail(outcome.error_message),
                timeout=timeout,
            )
        return _verdict(entry, status="refused", detail=_error_detail(outcome))

    # ``result`` is ``None`` when the gateway answered with a non-object result.
    # That is drift of the most basic kind — the baseline records a key set, and
    # there is no key set here — so it is reported as such rather than compared
    # against an empty mapping, which would report every recorded key as missing
    # and bury the actual fact.
    if outcome.result is None:
        return _verdict(
            entry,
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
        return _verdict(entry, status="drifted", drifts=drifts)
    return _verdict(entry, status="present")


async def check_compatibility(
    dispatcher: ProbeDispatcher,
    *,
    session_id: str = "",
    timeout: float | None = None,
    baseline: Sequence[MethodBaseline] = COMPAT_BASELINE,
) -> CompatReport:
    """Probe the read-only and presence-only sets; record the rest as unverified.

    The iteration order is the baseline's own, so the report reads in the order
    U3 wrote the evidence down. Probes run in sequence rather than concurrently:
    a gateway answering a startup burst in parallel is not the thing under test,
    and a sequential call log is what makes "no mutating method was invoked"
    something a reader can check line by line.

    U5 widened this from six calls to eight. ``session.active_list`` is probed
    like any other read-only method, and ``approval.pending`` goes through
    :func:`probe_presence`, which sends a bare call and reads only the refusal.
    Everything evidence-only is still never called, and the widening is visible
    in :attr:`CompatReport.probed` rather than inferred from this docstring.
    """
    verdicts: list[MethodVerdict] = []
    probed: list[str] = []

    for entry in baseline:
        if entry.classification == "read-only":
            probed.append(entry.method)
            verdicts.append(
                await probe_method(
                    dispatcher, entry, session_id=session_id, timeout=timeout
                )
            )
            continue
        if entry.classification == "presence-only":
            probed.append(entry.method)
            verdicts.append(await probe_presence(dispatcher, entry, timeout=timeout))
            continue
        verdicts.append(_verdict(entry, status="not-probed", detail=entry.evidence))

    return CompatReport(verdicts=tuple(verdicts), probed=tuple(probed))


def unprobed_methods(report: CompatReport) -> tuple[str, ...]:
    """Required methods this check did not call. Always the evidence-only set.

    Written as a derivation from the report rather than as a re-export of
    :data:`~talaria.domain.compat.EVIDENCE_ONLY_METHODS`, so a probe that started
    calling a mutating method would change this answer instead of leaving a
    constant that still describes the intent.
    """
    return tuple(v.method for v in report.verdicts if v.status == "not-probed")


def probe_set_is_permitted(report: CompatReport) -> bool:
    """Whether the recorded call log stayed inside the set a probe may call.

    Renamed from ``probe_set_is_read_only`` by U5, because the set it checks
    stopped being the read-only one: ``approval.pending`` is presence-only, and a
    predicate whose name still said "read-only" would have to be read against a
    body that no longer meant it. The property being asserted is unchanged and is
    the one KTD9 and R34 care about — nothing mutating went on the wire.
    """
    called = set(report.probed)
    return called <= set(PROBEABLE_METHODS) and not called & set(EVIDENCE_ONLY_METHODS)


# ── U5: the seam board, assembled from probes (R9, R10, R12, R24) ────────


@dataclass(frozen=True)
class HealthAnswer:
    """What a bare ``GET /api/health`` came back with.

    ``status`` is ``None`` for "the request never produced a status line" —
    unreachable host, refused connection, timeout — which is not evidence about
    the route and must not be graded as absence. ``ok`` is the decoded body's own
    ``ok`` field, which the handler at :data:`~talaria.domain.compat.RUNNING_PIN`
    always sets (``hermes_cli/web_server.py``'s ``get_health``); a 200 without it
    is a different server answering on that origin.
    """

    status: int | None = None
    ok: bool = False
    detail: str = ""


@runtime_checkable
class HealthProbe(Protocol):
    """The one operation the ``http-runner`` seam probe needs.

    Declared as a shape rather than imported from
    :mod:`talaria.transport.admin`, for the same reason
    :class:`ProbeDispatcher` is: an admin client too old to have the method, or
    a test double that never had one, is a real state and it renders as
    never-observed rather than raising.
    """

    async def probe_health(self) -> HealthAnswer: ...


def seam_status_for_verdict(verdict: MethodVerdict) -> tuple[SeamStatus | None, str]:
    """Map one method verdict onto its seam's classification.

    ``unproved`` becomes ``None`` — no observation — rather than a verdict of its
    own. That is what lets :func:`~talaria.domain.compat.apply_probe_round`
    decide between "never observed" and "degraded" from what the board already
    knew, which is a decision this function has no business making.
    """
    if verdict.status == "present":
        return "present", ""
    if verdict.status == "missing":
        return "absent", verdict.detail
    if verdict.status == "drifted":
        return "incompatible", "; ".join(drift.describe() for drift in verdict.drifts)
    if verdict.status in ("refused", "parameter-invalid"):
        return "parameter-invalid", verdict.detail
    return None, verdict.detail


def _health_seam(answer: HealthAnswer) -> tuple[SeamStatus | None, str]:
    """Map one health answer onto the ``http-runner`` seam's classification.

    404 is the only absence. Every other status the origin managed to send is
    ``incompatible`` — something is serving there and it is not the pinned
    handler — and no status at all settles nothing, so it returns ``None``. A
    401 in particular is *not* absence: ``/api/health`` takes no credential at
    :data:`~talaria.domain.compat.RUNNING_PIN`, so a 401 means a reverse proxy is
    in front of the gateway, which is a fact worth naming rather than a missing
    route.
    """
    if answer.status is None:
        return None, answer.detail
    if answer.status == 404:
        return "absent", answer.detail or "the origin answered 404"
    if 200 <= answer.status < 300:
        if answer.ok:
            return "present", ""
        return "incompatible", "the health route answered without an 'ok' field"
    return "incompatible", f"the origin answered status {answer.status}"


async def probe_seams(
    dispatcher: ProbeDispatcher,
    *,
    trigger: ProbeTrigger,
    session_id: str = "",
    timeout: float | None = None,
    health: HealthProbe | None = None,
    baseline: Sequence[MethodBaseline] = COMPAT_BASELINE,
) -> tuple[CompatReport, tuple[SeamObservation, ...]]:
    """One probe round for one connection: the report, plus the seam results.

    ``trigger`` is required and has no render-time member — the cadence rule
    (R12: attach, reconnect, five-minute revalidation, never per-render) is
    enforced by there being nothing a render path could pass. Whether a
    revalidation round is due is
    :func:`~talaria.domain.compat.seam_probe_due`'s answer, not this function's;
    this function runs the round it is asked to run.

    Seams are returned as results rather than folded into a board here, because
    folding needs the previous board and the frame clock — both of which belong
    to the caller. ``kanban-dispatcher`` is absent from the returned results
    **by construction**: it has no method and no path in the catalogue, so the
    loop below has nothing to call and invents nothing. Its row therefore keeps
    whatever the board already said about it, which is never-observed, forever.
    """
    report = await check_compatibility(
        dispatcher, session_id=session_id, timeout=timeout, baseline=baseline
    )

    results: list[SeamObservation] = []
    for seam in SEAM_CATALOGUE:
        if seam.method is not None:
            try:
                verdict = report.verdict_for(seam.method)
            except KeyError:
                # The seam's method is not in the baseline this round used.
                # Nothing was asked, so nothing is claimed.
                continue
            status, detail = seam_status_for_verdict(verdict)
            results.append(
                SeamObservation(
                    seam=seam.name,
                    status=status,
                    source=f"probe {seam.method}",
                    trigger=trigger,
                    detail=detail,
                )
            )
            continue
        if seam.http_path is not None and health is not None:
            answer = await health.probe_health()
            status, detail = _health_seam(answer)
            results.append(
                SeamObservation(
                    seam=seam.name,
                    status=status,
                    source=f"probe GET {seam.http_path}",
                    trigger=trigger,
                    detail=redact_probe_detail(detail),
                )
            )
    return report, tuple(results)
