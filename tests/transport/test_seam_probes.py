"""Seam probes: named presence, absence, and degradation per connection (v0.4 U5).

AE3 verbatim — "when a probed seam is absent, and when a probe's parameterized
request fails, the absent seam is named with its disabled feature and no surface
renders empty data as zero; the parameterized failure is re-asked bare before
absence may be claimed" — plus AE4's mid-session degradation and AE7's canaries
on the new surface.

**Where the gateway is a real socket and where it is a double.** Every test that
makes a claim about *what went onto the wire* runs the real
:class:`~talaria.transport.source.LiveSource` against the loopback stub, so the
server's own received-call record is what the assertion reads. Tests that only
grade an answer use a dispatcher double, because a stub answers every name it is
asked and therefore cannot testify about classification at all.

No Hermes gateway was attached at any point. The error codes below are
transcribed from the handlers at ``7095e23eb`` and from U1's live verification
(``docs/analysis/2026-08-17-v0-4-topology-verification.md``), which measured both
answers this unit turns on: ``-32601 unknown method: approval.pending`` from the
serving process, ``4001 session not found`` from a present method given a bad
session.
"""

from __future__ import annotations

from dataclasses import replace
from typing import Any

import pytest
import pytest_asyncio

from talaria.domain.compat import (
    PROBE_REVALIDATION_S,
    SEAM_CATALOGUE,
    SeamObservation,
    apply_probe_round,
    baseline_for,
    board_lines,
    empty_board,
    next_probe_due_at,
    seam_for,
    seam_line,
    seam_probe_due,
)
from talaria.domain.redaction import redact_probe_detail
from talaria.transport.attach import AttachTarget
from talaria.transport.compat_check import (
    METHOD_NOT_FOUND,
    HealthAnswer,
    check_compatibility,
    probe_method,
    probe_presence,
    probe_seams,
)
from talaria.transport.credentials import Credential
from talaria.transport.rpc import NO_REPLY_IN_TIME, RpcOutcome, unknown_outcome
from talaria.transport.source import LiveSource
from tests.transport.conftest import STUB_TOKEN, StubGateway, err, ok
from tests.transport.test_compat_baseline import (
    EXPECTED_PROBE_SET,
    HEALTHY,
    SESSION_NOT_FOUND,
    healthy_responder,
)

FAST_RETRIES = (0.0, 0.01, 0.01)


class StubProvider:
    async def acquire(self) -> Credential:
        return Credential("token", STUB_TOKEN, "file")


async def attached(gateway: StubGateway) -> LiveSource:
    source = LiveSource(
        AttachTarget.from_url(gateway.url),
        StubProvider(),
        reconnect_delays=FAST_RETRIES,
    )
    state = await source.start()
    assert state == "connected", f"the probe never attached: {state}"
    await gateway.wait_for_attach()
    return source


def params_sent(gateway: StubGateway, method: str) -> list[Any]:
    """Every ``params`` the *server* saw for one method, in arrival order."""
    return [
        message.get("params")
        for message in gateway.current.received
        if message.get("method") == method
    ]


class HealthyDispatcher:
    """A gateway with every probeable method, answering each one as the pin does.

    Written once and shared, because the interesting variation across the tests
    below is which *seam* fails — repeating the healthy body per test would make
    the difference between two neighbouring cases hard to see. ``approval.pending``
    answers 4001, which is what a *present* handler does to a bare call.
    """

    async def call(
        self, method: str, params: Any = None, *, timeout: float | None = None
    ) -> RpcOutcome:
        if method == "approval.pending":
            return RpcOutcome(
                status="error",
                method=method,
                request_id="1",
                epoch=1,
                error_code=SESSION_NOT_FOUND,
                error_message="session not found",
            )
        return RpcOutcome(
            status="ok", method=method, request_id="1", epoch=1, result=HEALTHY[method]
        )


class NullHealth:
    """An admin origin that never answers — the HTTP seam learns nothing."""

    async def probe_health(self) -> HealthAnswer:
        return HealthAnswer(status=None, detail="the admin origin did not answer")


class HealthyHealth:
    async def probe_health(self) -> HealthAnswer:
        return HealthAnswer(status=200, ok=True)


def startup_responder_for_seams() -> Any:
    """A stub that answers the probe set *and* the startup calls.

    Imported from the startup tests rather than rewritten, so the two files
    cannot disagree about what a healthy gateway answers — which they did once
    already, when the presence probe was added to one and not the other.
    """
    from tests.transport.test_session_startup import startup_responder

    return startup_responder()


@pytest_asyncio.fixture
async def healthy_gateway() -> Any:
    stub = StubGateway(responder=healthy_responder)
    await stub.start()
    try:
        yield stub
    finally:
        await stub.stop()


# ── AE3, first half: an absent seam is named with its disabled feature ───


@pytest.mark.asyncio
async def test_an_absent_session_active_list_names_the_disabled_roster() -> None:
    """AE3: the seam line says *which* feature is off, not that a list is empty.

    The assertion is on the rendered sentence rather than on a flag, because the
    requirement (R10) is about what an operator reads. A test pinning
    ``status == "absent"`` would pass against a renderer that showed an empty
    roster and said nothing.
    """

    class NoRoster:
        async def call(
            self, method: str, params: Any = None, *, timeout: float | None = None
        ) -> RpcOutcome:
            if method == "session.active_list":
                return RpcOutcome(
                    status="error",
                    method=method,
                    request_id="1",
                    epoch=1,
                    error_code=METHOD_NOT_FOUND,
                    error_message=f"unknown method: {method}",
                )
            if method in HEALTHY:
                return RpcOutcome(
                    status="ok",
                    method=method,
                    request_id="1",
                    epoch=1,
                    result=HEALTHY[method],
                )
            return RpcOutcome(
                status="error",
                method=method,
                request_id="1",
                epoch=1,
                error_code=SESSION_NOT_FOUND,
                error_message="session not found",
            )

    _report, results = await probe_seams(NoRoster(), trigger="attach")
    board = apply_probe_round(empty_board("default"), results, at=100.0)
    line = seam_line(board.observation_for("roster"), 100.0)

    assert board.observation_for("roster").status == "absent"
    assert "roster unavailable: session.active_list absent" in line
    assert "probe session.active_list" in line, "R20: the line must name its source"
    assert "0" not in line.replace("0s ago", ""), "R10: absence is never a zero"


@pytest.mark.asyncio
async def test_an_absent_approval_pending_names_its_disabled_feature() -> None:
    """The state U1 measured on this machine: roster present, approval detail gone.

    Both halves are asserted in one observation. A test that only checked the
    absent seam would pass against a probe round that had also lost the roster,
    which is a different and much worse install.
    """

    class NoApprovalPending:
        async def call(
            self, method: str, params: Any = None, *, timeout: float | None = None
        ) -> RpcOutcome:
            if method == "approval.pending":
                return RpcOutcome(
                    status="error",
                    method=method,
                    request_id="1",
                    epoch=1,
                    error_code=METHOD_NOT_FOUND,
                    error_message="unknown method: approval.pending",
                )
            return RpcOutcome(
                status="ok", method=method, request_id="1", epoch=1, result=HEALTHY[method]
            )

    _report, results = await probe_seams(NoApprovalPending(), trigger="attach")
    board = apply_probe_round(empty_board("default"), results, at=10.0)

    assert board.observation_for("roster").status == "present"
    assert board.observation_for("approval-detail").status == "absent"
    line = seam_line(board.observation_for("approval-detail"), 10.0)
    assert "approval detail unavailable: approval.pending absent" in line
    assert "waiting rows are shown without their prompts" in line


# ── AE3, second half: R11's bare re-ask before absence ───────────────────


@pytest.mark.asyncio
async def test_a_parameterized_failure_is_reasked_bare_before_absence(
    healthy_gateway: StubGateway,
) -> None:
    """The recorded ``absent_capability`` misdiagnosis, closed (R11).

    The stub answers ``-32601`` **only** when the request carries parameters —
    the exact shape of the recorded defect, where a mistyped profile name
    produced a 404 that was read as a gateway too old to have the endpoint. The
    server's own received-call record is what proves the second, parameterless
    call was actually sent; the verdict is what proves it changed the answer.
    """
    sent: list[dict[str, Any]] = []

    def responder(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        method = str(message.get("method", ""))
        rid = message.get("id")
        if method == "spawn_tree.list":
            sent.append(message)
            if message.get("params"):
                return err(rid, METHOD_NOT_FOUND, f"unknown method: {method}")
            return ok(rid, {"entries": []})
        return healthy_responder(message, stub)

    stub = StubGateway(responder=responder)
    await stub.start()
    try:
        source = await attached(stub)
        try:
            verdict = await probe_method(
                source, baseline_for("spawn_tree.list"), session_id="s-9f12", timeout=5.0
            )
        finally:
            await source.close()
    finally:
        await stub.stop()

    assert [bool(message.get("params")) for message in sent] == [True, False], (
        "the parameterized call must be followed by a bare one"
    )
    # ``parameter-invalid``, not ``missing``: the name exists and the probe's own
    # parameters were the problem, which is the verdict the recorded
    # misdiagnosis should have produced.
    assert verdict.status == "parameter-invalid"
    assert verdict.bare_reasked


@pytest.mark.asyncio
async def test_absence_is_claimed_only_when_the_bare_reask_also_refuses(
    healthy_gateway: StubGateway,
) -> None:
    """The paired positive. A rule that never concluded absence would satisfy the
    test above perfectly and make the whole probe set useless."""
    stub = StubGateway(
        responder=lambda message, gateway: (
            err(
                message.get("id"),
                METHOD_NOT_FOUND,
                f"unknown method: {message.get('method')}",
            )
            if message.get("method") == "spawn_tree.list"
            else healthy_responder(message, gateway)
        )
    )
    await stub.start()
    try:
        source = await attached(stub)
        try:
            verdict = await probe_method(
                source, baseline_for("spawn_tree.list"), session_id="s-9f12", timeout=5.0
            )
        finally:
            await source.close()
    finally:
        await stub.stop()

    assert verdict.status == "missing"
    assert verdict.bare_reasked
    assert "confirmed by a bare re-ask" in verdict.describe()


@pytest.mark.asyncio
async def test_a_bare_fixture_is_not_reasked(healthy_gateway: StubGateway) -> None:
    """``agents.list``'s fixture is already empty, so a re-ask would send the same
    request twice and prove nothing. The wire is what says it did not happen."""
    stub = StubGateway(
        responder=lambda message, gateway: err(
            message.get("id"), METHOD_NOT_FOUND, f"unknown method: {message.get('method')}"
        )
    )
    await stub.start()
    try:
        source = await attached(stub)
        try:
            verdict = await probe_method(source, baseline_for("agents.list"), timeout=5.0)
        finally:
            await source.close()
        calls = [
            message
            for message in stub.current.received
            if message.get("method") == "agents.list"
        ]
    finally:
        await stub.stop()

    assert len(calls) == 1, "a bare fixture must not be re-asked"
    assert verdict.status == "missing"
    assert not verdict.bare_reasked


@pytest.mark.asyncio
async def test_a_silent_bare_reask_downgrades_absence_to_unproved() -> None:
    """The first call said method-not-found; the second said nothing at all.

    Absence must not survive that. This is the branch a naive implementation
    gets wrong by keeping the first answer when the second is inconclusive.
    """
    calls = 0

    class DiesOnReask:
        async def call(
            self, method: str, params: Any = None, *, timeout: float | None = None
        ) -> RpcOutcome:
            nonlocal calls
            calls += 1
            if calls == 1:
                return RpcOutcome(
                    status="error",
                    method=method,
                    request_id="1",
                    epoch=1,
                    error_code=METHOD_NOT_FOUND,
                    error_message="unknown method",
                )
            return unknown_outcome(method, NO_REPLY_IN_TIME, epoch=1)

    verdict = await probe_method(
        DiesOnReask(), baseline_for("spawn_tree.list"), session_id="s-1"
    )
    assert verdict.status == "unproved"
    assert verdict.bare_reasked


# ── KTD11: presence through the parameter-invalid distinction ────────────


@pytest.mark.asyncio
async def test_a_refused_bare_approval_pending_proves_presence(
    healthy_gateway: StubGateway,
) -> None:
    """4001 proves the method exists; the wire proves no session was named.

    Both halves matter and neither is sufficient. The verdict alone would pass
    for a probe that sent a real session id and warmed a build to learn it; the
    wire alone would pass for a probe that never read the answer.
    """
    source = await attached(healthy_gateway)
    try:
        verdict = await probe_presence(
            source, baseline_for("approval.pending"), timeout=5.0
        )
    finally:
        await source.close()

    assert verdict.status == "present"
    assert params_sent(healthy_gateway, "approval.pending") == [{}], (
        "KTD11: the presence probe must name no session"
    )
    assert "proved by refusal" in verdict.detail
    assert "session not found" in verdict.detail


@pytest.mark.asyncio
async def test_the_presence_probe_refuses_a_non_bare_fixture() -> None:
    """The guard between a presence probe and a warmed agent build.

    Exercised by handing :func:`probe_presence` an entry whose fixture is not
    empty. It must raise before the dispatcher is touched, so the counter is
    asserted at zero — a guard nothing can exercise is a guard nobody can trust.
    """
    from dataclasses import replace as dataclass_replace

    class Counting:
        def __init__(self) -> None:
            self.calls = 0

        async def call(
            self, method: str, params: Any = None, *, timeout: float | None = None
        ) -> RpcOutcome:
            self.calls += 1
            return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})

    dispatcher = Counting()
    entry = dataclass_replace(
        baseline_for("approval.pending"), request_fixture={"session_id": "s-1"}
    )
    with pytest.raises(ValueError, match="no parameters"):
        await probe_presence(dispatcher, entry)
    assert dispatcher.calls == 0


@pytest.mark.asyncio
async def test_probe_method_refuses_a_presence_only_entry() -> None:
    """The other half of the same guard: a presence-only method must never be
    called with its parameters through the read-only path."""

    class Counting:
        def __init__(self) -> None:
            self.calls = 0

        async def call(
            self, method: str, params: Any = None, *, timeout: float | None = None
        ) -> RpcOutcome:
            self.calls += 1
            return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})

    dispatcher = Counting()
    with pytest.raises(ValueError, match="read-only"):
        await probe_method(dispatcher, baseline_for("approval.pending"))
    assert dispatcher.calls == 0


@pytest.mark.asyncio
async def test_the_startup_round_sends_the_whole_probe_set_and_nothing_else(
    healthy_gateway: StubGateway,
) -> None:
    """The wire, in order, from the server's own record.

    Paired with the negative in ``test_compat_baseline.py``: this asserts the
    eight names that must appear, that one asserts the thirteen that must not.
    """
    source = await attached(healthy_gateway)
    try:
        report = await check_compatibility(source, session_id="s-9f12", timeout=5.0)
    finally:
        await source.close()

    seen = [str(m.get("method", "")) for m in healthy_gateway.current.received]
    assert seen == list(EXPECTED_PROBE_SET)
    assert report.probed == EXPECTED_PROBE_SET
    assert report.verdict_for("session.active_list").status == "present"
    assert report.verdict_for("approval.pending").status == "present"


# ── the HTTP runner seam ─────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_a_healthy_admin_origin_makes_the_http_runner_seam_present() -> None:
    _report, results = await probe_seams(
        HealthyDispatcher(), trigger="attach", health=HealthyHealth()
    )
    board = apply_probe_round(empty_board(), results, at=5.0)
    observation = board.observation_for("http-runner")
    assert observation.status == "present"
    assert observation.source == "probe GET /api/health"


@pytest.mark.asyncio
async def test_a_404_health_route_names_the_admin_catalogue_as_the_disabled_feature() -> (
    None
):
    class Missing:
        async def probe_health(self) -> HealthAnswer:
            return HealthAnswer(status=404)

    _report, results = await probe_seams(
        HealthyDispatcher(), trigger="attach", health=Missing()
    )
    board = apply_probe_round(empty_board(), results, at=5.0)
    line = seam_line(board.observation_for("http-runner"), 5.0)
    assert board.observation_for("http-runner").status == "absent"
    assert "admin catalogue unavailable: GET /api/health absent" in line


@pytest.mark.asyncio
async def test_a_401_health_route_is_not_read_as_absence() -> None:
    """``/api/health`` takes no credential at the pin, so a 401 is a proxy in
    front of the gateway — a fact worth naming, and not a missing route."""

    class Proxied:
        async def probe_health(self) -> HealthAnswer:
            return HealthAnswer(status=401)

    _report, results = await probe_seams(
        HealthyDispatcher(), trigger="attach", health=Proxied()
    )
    board = apply_probe_round(empty_board(), results, at=5.0)
    observation = board.observation_for("http-runner")
    assert observation.status == "incompatible"
    assert "401" in seam_line(observation, 5.0)


@pytest.mark.asyncio
async def test_no_health_probe_leaves_the_http_runner_seam_never_observed() -> None:
    """An admin client too old to have the method, or none at all. The honest
    rendering is "not observed", never "absent"."""

    _report, results = await probe_seams(
        HealthyDispatcher(), trigger="attach", health=None
    )
    board = apply_probe_round(empty_board(), results, at=5.0)
    observation = board.observation_for("http-runner")
    assert observation.status is None
    assert observation.observation(5.0) == "never-observed"
    assert "not observed" in seam_line(observation, 5.0)


@pytest.mark.asyncio
async def test_an_unreachable_admin_origin_is_not_read_as_absence() -> None:
    """A probe that produced no status line learned nothing about the route.

    The pairing matters: on a *first* round the seam stays never-observed, and on
    a round that follows a confirmed one it becomes ``degraded`` carrying the age
    of the last good answer. Reading either as ``absent`` would tell an operator
    their gateway lacks an admin surface when the truth is that Talaria could not
    reach it.
    """
    _report, first = await probe_seams(
        HealthyDispatcher(), trigger="attach", health=NullHealth()
    )
    cold = apply_probe_round(empty_board(), first, at=10.0)
    assert cold.observation_for("http-runner").status is None
    assert "not observed" in seam_line(cold.observation_for("http-runner"), 10.0)

    warm = apply_probe_round(
        empty_board(),
        (
            SeamObservation(
                seam="http-runner", status="present", source="probe GET /api/health"
            ),
        ),
        at=20.0,
    )
    _report, second = await probe_seams(
        HealthyDispatcher(), trigger="revalidation", health=NullHealth()
    )
    lost = apply_probe_round(warm, second, at=380.0)
    line = seam_line(lost.observation_for("http-runner"), 380.0)
    assert lost.observation_for("http-runner").status == "degraded"
    assert "last confirmed present 360s ago" in line
    assert "admin catalogue unavailable" in line



# ── the kanban-dispatcher seam: never observed, never zero ───────────────


def test_the_kanban_seam_has_no_probe_and_no_invented_url() -> None:
    """No route exists at the running revision, so none is invented (R11's error
    class, committed one layer out).

    Asserted structurally — the catalogue entry has neither a method nor a path —
    rather than by searching the module for a string, because a structural
    absence is what stops a later edit from adding one.
    """
    seam = seam_for("kanban-dispatcher")
    assert seam.method is None
    assert seam.http_path is None
    assert not seam.probeable


@pytest.mark.asyncio
async def test_the_kanban_seam_is_never_observed_and_never_reads_as_zero() -> None:
    """After a full successful probe round against a healthy gateway, the seam
    with no route is still not observed — and says "board queue source off"."""

    _report, results = await probe_seams(
        HealthyDispatcher(), trigger="attach", health=HealthyHealth()
    )
    assert "kanban-dispatcher" not in {result.seam for result in results}

    board = apply_probe_round(empty_board(), results, at=42.0)
    observation = board.observation_for("kanban-dispatcher")
    line = seam_line(observation, 42.0)

    assert observation.status is None
    assert observation.observation(42.0) == "never-observed"
    assert "board queue source off" in line
    assert "0" not in line
    assert "zero" not in line.lower()
    assert "idle" not in line.lower()
    assert "stale" not in line.lower(), "R24: never-observed is not stale-since-nothing"


# ── AE4: a seam that degrades mid-session updates with source and age ────


def test_a_degrading_seam_updates_its_line_with_source_and_age() -> None:
    """R12/AE4. The seam was present, the next round could not settle it, and the
    line says so *with* how old the last good answer is.

    The failure this prevents is a frozen "present" re-presented as current,
    which R20 forbids in so many words.
    """
    board = apply_probe_round(
        empty_board("default"),
        (
            SeamObservation(
                seam="roster", status="present", source="probe session.active_list"
            ),
        ),
        at=100.0,
    )
    degraded = apply_probe_round(
        board,
        (
            SeamObservation(
                seam="roster",
                status=None,
                source="probe session.active_list",
                detail="no reply arrived before the deadline",
            ),
        ),
        at=160.0,
    )
    observation = degraded.observation_for("roster")
    line = seam_line(observation, 190.0)

    assert observation.status == "degraded"
    assert observation.previous_status == "present"
    assert observation.confirmed_at == 100.0
    assert "probe session.active_list" in line, "R20: source"
    assert "30s ago" in line, "R20: the age of the observation that changed it"
    assert "last confirmed present 90s ago" in line
    assert "roster unavailable" in line


def test_a_present_to_absent_transition_carries_the_last_confirmation_age() -> None:
    """The settled counterpart. A seam that went away is ``absent``, not
    ``degraded`` — and still says how old the last good answer was."""
    board = apply_probe_round(
        empty_board(),
        (SeamObservation(seam="roster", status="present", source="probe x"),),
        at=10.0,
    )
    gone = apply_probe_round(
        board,
        (SeamObservation(seam="roster", status="absent", source="probe x"),),
        at=70.0,
    )
    line = seam_line(gone.observation_for("roster"), 70.0)
    assert gone.observation_for("roster").status == "absent"
    assert "last confirmed present 60s ago" in line


def test_a_failed_first_probe_leaves_a_seam_never_observed_not_degraded() -> None:
    """Degradation requires something to degrade *from*. A seam whose very first
    probe failed has observed nothing, and "degraded" would claim otherwise."""
    board = apply_probe_round(
        empty_board(),
        (SeamObservation(seam="roster", status=None, source="probe x", detail="no reply"),),
        at=10.0,
    )
    observation = board.observation_for("roster")
    assert observation.status is None
    assert observation.observation(10.0) == "never-observed"
    assert "not observed" in seam_line(observation, 10.0)


def test_a_probe_older_than_the_revalidation_interval_renders_stale() -> None:
    """R12: the probe story is a live claim, not a startup banner. Past one whole
    interval with no new round, the line says the claim has not been re-established."""
    board = apply_probe_round(
        empty_board(),
        (SeamObservation(seam="roster", status="present", source="probe x"),),
        at=0.0,
    )
    observation = board.observation_for("roster")
    assert observation.observation(PROBE_REVALIDATION_S - 1) == "live"
    assert observation.observation(PROBE_REVALIDATION_S + 1) == "stale"
    assert "stale" in seam_line(observation, PROBE_REVALIDATION_S + 1)


# ── cadence: attach, reconnect, five-minute revalidation, never per-render ──


def test_attach_and_reconnect_probe_unconditionally_and_revalidation_does_not() -> None:
    """The cadence rule, as three answers from one function.

    A reconnect may be dialling a different install, so waiting out a five-minute
    interval before re-asking would leave a stale capability story on screen for
    the whole window. A revalidation is the scheduled case and is the only one
    the interval governs.
    """
    board = apply_probe_round(
        empty_board(),
        (SeamObservation(seam="roster", status="present", source="probe x"),),
        at=1000.0,
    )
    assert seam_probe_due(board, 1001.0, "attach")
    assert seam_probe_due(board, 1001.0, "reconnect")
    assert not seam_probe_due(board, 1001.0, "revalidation")
    assert not seam_probe_due(board, 1000.0 + PROBE_REVALIDATION_S - 1, "revalidation")
    assert seam_probe_due(board, 1000.0 + PROBE_REVALIDATION_S, "revalidation")
    assert next_probe_due_at(board) == 1000.0 + PROBE_REVALIDATION_S


def test_a_board_that_has_never_probed_is_due_immediately() -> None:
    assert next_probe_due_at(empty_board()) == 0.0
    assert seam_probe_due(empty_board(), 0.0, "revalidation")


def test_probe_seams_has_no_render_trigger() -> None:
    """The cadence is enforced by the type, not by discipline: there is no
    trigger a render path could pass, so a render cannot start a probe round."""
    from typing import get_args

    from talaria.domain.compat import ProbeTrigger

    assert set(get_args(ProbeTrigger)) == {"attach", "reconnect", "revalidation"}


# ── AE7's canaries on the new surface ────────────────────────────────────


@pytest.mark.parametrize(
    ("message", "leaked"),
    [
        ("token=hunter2-not-a-real-token", "hunter2-not-a-real-token"),
        ("Authorization: Bearer abc.def.ghi", "abc.def.ghi"),
        ("session token: s3cr3t-value", "s3cr3t-value"),
        ("api_key=AKIAIOSFODNN7EXAMPLE", "AKIAIOSFODNN7EXAMPLE"),
        ("dial https://user:pw@gateway.internal/api failed", "pw"),
        ("dial wss://host/api/ws?token=leaked-token failed", "leaked-token"),
    ],
)
def test_a_credential_canary_never_survives_into_a_probe_diagnostic(
    message: str, leaked: str
) -> None:
    """R22: a probe diagnostic is on the same footing as a frame log.

    Each case pairs the withholding with a positive — the surviving text still
    says something — because a redactor that returned the empty string would
    satisfy every negative here and destroy the diagnostic.
    """
    redacted = redact_probe_detail(message)
    assert leaked not in redacted
    assert redacted, "the whole message must not be withheld"


def test_the_probe_diagnostic_is_bounded_and_single_line() -> None:
    """A gateway error that is a whole log must not own several rows of a
    one-line seam surface."""
    redacted = redact_probe_detail("a" * 500 + "\nsecond line\nthird line")
    assert "\n" not in redacted
    assert len(redacted) <= 121


@pytest.mark.asyncio
async def test_markup_and_control_canaries_reach_the_seam_line_inert() -> None:
    """AE7's other half: a gateway error carrying Rich markup, an ANSI escape and
    an HTML tag renders as visible literal text.

    Asserted through the widget rather than through the domain string, because
    the defanging happens at the render boundary (R23) and a domain-level
    assertion would be checking the wrong layer.
    """
    from textual.app import App, ComposeResult

    from talaria.ui.status_region import StatusRegion

    canary = "[bold red]markup[/] \x1b[2J <script>x</script>"

    class Host(App[None]):
        def compose(self) -> ComposeResult:
            yield StatusRegion(id="status")

    class Refusing(HealthyDispatcher):
        async def call(
            self, method: str, params: Any = None, *, timeout: float | None = None
        ) -> RpcOutcome:
            if method == "session.active_list":
                return RpcOutcome(
                    status="error",
                    method=method,
                    request_id="1",
                    epoch=1,
                    error_code=SESSION_NOT_FOUND,
                    error_message=canary,
                )
            return await super().call(method, params, timeout=timeout)

    _report, results = await probe_seams(Refusing(), trigger="attach")
    board = apply_probe_round(empty_board(), results, at=3.0)

    app = Host()
    async with app.run_test() as pilot:
        region = app.query_one("#status", StatusRegion)
        await region.apply_seams(board_lines(board, 3.0))
        await pilot.pause()
        roster = next(text for text in region.seam_texts if text.startswith("roster:"))

    assert "\x1b" not in roster, "the escape sequence must not reach the screen"
    assert "␛[2J" in roster, "and it must be visible as a literal instead"
    assert "[bold red]markup[/]" in roster, "markup renders as text, not as style"
    assert "<script>x</script>" in roster


@pytest.mark.asyncio
async def test_the_seam_rows_render_every_seam_in_catalogue_order() -> None:
    """A board with nothing probed still shows every seam — R24's whole point is
    that a gap is rendered, not omitted."""
    from textual.app import App, ComposeResult

    from talaria.ui.status_region import StatusRegion

    class Host(App[None]):
        def compose(self) -> ComposeResult:
            yield StatusRegion(id="status")

    app = Host()
    async with app.run_test() as pilot:
        region = app.query_one("#status", StatusRegion)
        await region.apply_seams(board_lines(empty_board(), 0.0))
        await pilot.pause()
        texts = region.seam_texts

    assert len(texts) == len(SEAM_CATALOGUE)
    assert [text.split(":")[0] for text in texts] == [s.name for s in SEAM_CATALOGUE]
    assert all("not observed" in text for text in texts)


@pytest.mark.asyncio
async def test_seam_rows_survive_a_status_command_tick() -> None:
    """Seam rows are a separate list from the status command's rows, so an
    unrelated status tick does not blink the seam board off screen."""
    from textual.app import App, ComposeResult

    from talaria.status.runner import StatusTickResult
    from talaria.ui.status_region import StatusRegion

    class Host(App[None]):
        def compose(self) -> ComposeResult:
            yield StatusRegion(id="status")

    app = Host()
    async with app.run_test() as pilot:
        region = app.query_one("#status", StatusRegion)
        await region.apply_seams(board_lines(empty_board(), 0.0))
        await region.apply(
            StatusTickResult(outcome="ok", rows=("branch: main",), marker="")
        )
        await pilot.pause()
        assert region.row_texts == ("branch: main",)
        assert len(region.seam_texts) == len(SEAM_CATALOGUE)


# ── what a fleet seam's absence does *not* do ────────────────────────────


@pytest.mark.asyncio
async def test_an_absent_fleet_method_does_not_block_the_daily_driver_verdict() -> None:
    """KTD4: fleet features go off by name; the single-session core keeps the
    old-pin baseline.

    Both halves in one observation, because either alone is satisfiable by a
    wrong implementation: a check that blocked on nothing would pass the first
    assertion, and one that blocked on everything would pass the second. The
    state modelled here is the one U1 measured on this machine's wire — a
    gateway with the roster and without ``approval.pending``.
    """

    class OldPin(HealthyDispatcher):
        async def call(
            self, method: str, params: Any = None, *, timeout: float | None = None
        ) -> RpcOutcome:
            if method == "approval.pending":
                return RpcOutcome(
                    status="error",
                    method=method,
                    request_id="1",
                    epoch=1,
                    error_code=METHOD_NOT_FOUND,
                    error_message="unknown method: approval.pending",
                )
            return await super().call(method, params, timeout=timeout)

    report = await check_compatibility(OldPin())
    assert report.verdict_for("approval.pending").status == "missing"
    assert report.ready, "an absent fleet capability must not condemn the gateway"
    assert "approval.pending" in report.missing, (
        "and it must still be reported as missing"
    )


@pytest.mark.asyncio
async def test_an_absent_baseline_method_still_blocks() -> None:
    """The paired positive for the rule above."""

    class NoAgents(HealthyDispatcher):
        async def call(
            self, method: str, params: Any = None, *, timeout: float | None = None
        ) -> RpcOutcome:
            if method == "agents.list":
                return RpcOutcome(
                    status="error",
                    method=method,
                    request_id="1",
                    epoch=1,
                    error_code=METHOD_NOT_FOUND,
                    error_message="unknown method: agents.list",
                )
            return await super().call(method, params, timeout=timeout)

    report = await check_compatibility(NoAgents())
    assert not report.ready
    assert [verdict.method for verdict in report.blocking] == ["agents.list"]


# ── the app's own cadence, against a real socket ─────────────────────────


@pytest.mark.asyncio
async def test_the_app_probes_at_attach_and_renders_the_board() -> None:
    """The end of the wire an operator actually sees.

    Asserted against the *server's* received-call record and the widget's own
    text, because the two halves fail independently: a round that never reached
    the socket and a board that never reached the screen look identical from the
    app object.
    """
    from talaria.domain.startup import StartupSelection
    from talaria.ui.status_region import StatusRegion
    from tests.transport.test_session_startup import live_app, sent, until

    stub = StubGateway(responder=startup_responder_for_seams())
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="new"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            calls = sent(stub)
            assert "session.active_list" in calls
            assert "approval.pending" in calls
            region = app.query_one("#status", StatusRegion)
            texts = region.seam_texts
        await app.shutdown_sources()
    finally:
        await stub.stop()

    assert [text.split(":")[0] for text in texts] == [s.name for s in SEAM_CATALOGUE]
    roster = next(text for text in texts if text.startswith("roster:"))
    assert "present" in roster
    kanban = next(text for text in texts if text.startswith("kanban-dispatcher:"))
    assert "board queue source off" in kanban


@pytest.mark.asyncio
async def test_a_painted_seam_line_grows_older_and_eventually_says_stale() -> None:
    """R20 and R24 on the surface rather than in the board.

    The board knew how to say ``stale`` from the day it was written; nothing ever
    repainted a seam row, so the sentence could not reach a screen. The single
    paint happened on the statement after a probe round folded in, with no await
    between them, which drew every seam at an age of exactly zero — and then the
    text was frozen. Two whole revalidation intervals could pass with the line
    still claiming ``0s ago``.

    Asserted as three states of one row, because each is satisfiable alone by
    something that is not the fix: painted at zero, aged past zero, and finally
    stale. The middle one is what rules out a repaint that simply re-renders the
    same constant.

    The clock advanced is the *frame* clock, which is the one the line reads
    (KTD12). A test that advanced a wall clock would pass against a renderer that
    reads a wall clock, which is the defect R20's ages exist to prevent.
    """
    from talaria.domain.startup import StartupSelection
    from talaria.ui.status_region import StatusRegion
    from tests.transport.test_session_startup import live_app, sent, until

    def roster_line(region: StatusRegion) -> str:
        return next(text for text in region.seam_texts if text.startswith("roster:"))

    stub = StubGateway(responder=startup_responder_for_seams())
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="new"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            probed_at_attach = len([n for n in sent(stub) if n == "session.active_list"])
            region = app.query_one("#status", StatusRegion)
            assert "0s ago" in roster_line(region)

            app.state = replace(app.state, last_observed_at=app.state.last_observed_at + 30.0)
            await app._render_tick()
            aged = roster_line(region)
            assert "30s ago" in aged
            assert "stale" not in aged

            app.state = replace(
                app.state,
                last_observed_at=app.state.last_observed_at + PROBE_REVALIDATION_S,
            )
            await app._render_tick()
            assert "stale" in roster_line(region)

            # Drawing is not probing. The cadence rule this unit is built around
            # governs the socket, and two repaints must not have touched it.
            assert (
                len([n for n in sent(stub) if n == "session.active_list"])
                == probed_at_attach
            ), "a repaint probed the gateway"
        await app.shutdown_sources()
    finally:
        await stub.stop()


@pytest.mark.asyncio
async def test_the_revalidation_timer_does_nothing_before_its_interval() -> None:
    """R12's cadence at the app level: the interval is a floor, and the callback
    re-checks it rather than trusting the timer.

    Paired with the positive — a round *does* run once the frame clock has moved
    past the interval — because a callback that never probed would satisfy the
    first assertion and leave the probe story frozen forever.
    """
    from talaria.domain.startup import StartupSelection
    from tests.transport.test_session_startup import live_app, sent, until

    stub = StubGateway(responder=startup_responder_for_seams())
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="new"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            after_attach = len([name for name in sent(stub) if name == "approval.pending"])

            await app.revalidate_seams()
            assert (
                len([name for name in sent(stub) if name == "approval.pending"])
                == after_attach
            ), "a revalidation ran before its interval had elapsed"

            app.seams = replace(
                app.seams,
                last_round_at=app.state.last_observed_at - PROBE_REVALIDATION_S - 1,
            )
            await app.revalidate_seams()
            assert (
                len([name for name in sent(stub) if name == "approval.pending"])
                == after_attach + 1
            ), "a due revalidation did not re-probe"
        await app.shutdown_sources()
    finally:
        await stub.stop()


@pytest.mark.asyncio
async def test_a_reconnect_reprobes_without_reopening_the_session() -> None:
    """R9: probe results are re-validated on reconnect.

    The startup sequence deliberately does not re-run — re-running it would
    abandon the operator's conversation for a new one — so the seam round has to
    be scheduled separately for exactly that case. Both halves are asserted:
    the probe went out again, and no second ``session.create`` did.
    """
    from talaria.domain.startup import StartupSelection
    from tests.transport.test_session_startup import live_app, sent, until

    stub = StubGateway(responder=startup_responder_for_seams())
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="new"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            before = sent(stub)
            creates = before.count("session.create")

            app.note_connection_state("connected")
            await until(
                lambda: sent(stub).count("approval.pending")
                > before.count("approval.pending")
            )
            after = sent(stub)
        await app.shutdown_sources()
    finally:
        await stub.stop()

    assert after.count("session.active_list") > before.count("session.active_list")
    assert after.count("session.create") == creates, "a reconnect reopened the session"
