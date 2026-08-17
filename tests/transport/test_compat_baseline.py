"""The startup compatibility check against a real socket (R34, AE7).

Every test here runs the real :class:`~talaria.transport.source.LiveSource`
against the stub gateway on loopback, so a claim about what the startup check
puts on the wire is checked against a server that received it — not against the
code that was supposed to send it.

**No Hermes gateway was attached at any point in this run.** The bodies below are
transcribed from the gateway's handlers at ``7f4d15515``; the responder is still a
stub, and a stub answers every name it is asked. That is precisely what it cannot
testify about, so the "no mutating method was invoked" assertions read the
server's own received-call record rather than Talaria's, and the read-only/
evidence-only split itself is checked against the pinned classification rather
than against anything this file arranges.

**The method lists here are written out literally.** Deriving the expected probe
set from :data:`~talaria.domain.compat.READ_ONLY_METHODS` would produce a test
that still passes after an entry is deleted from the baseline, because the
expectation would shrink with the thing it is checking.
"""

from __future__ import annotations

import asyncio
from typing import Any

import pytest
import pytest_asyncio

from talaria.domain.compat import (
    COMPAT_BASELINE,
    EVIDENCE_ONLY_METHODS,
    PRESENCE_ONLY_METHODS,
    READ_ONLY_METHODS,
    baseline_for,
)
from talaria.transport.attach import AttachTarget
from talaria.transport.compat_check import (
    METHOD_NOT_FOUND,
    check_compatibility,
    probe_method,
    probe_set_is_permitted,
    unprobed_methods,
)
from talaria.transport.credentials import Credential
from talaria.transport.rpc import NO_REPLY_IN_TIME, RpcOutcome, unknown_outcome
from talaria.transport.source import LiveSource
from tests.transport.conftest import STUB_TOKEN, StubGateway, err, ok

FAST_RETRIES = (0.0, 0.01, 0.01)

#: The methods U3, U7 and v0.4's U5 proved side-effect-free by reading their
#: handlers at the pin, written out by hand. This is the whole set a startup
#: probe is allowed to invoke with its pinned fixture and grade the answer of
#: (KTD9, R34). ``session.list`` joined the first five in U7 of the 2026-08-08
#: v0.2 plan (R10); ``session.active_list`` joined them in v0.4's U5, which is
#: the promotion U4 said it was deferring — a bare in-memory snapshot with no
#: ``_sess`` call and nothing mutating in it.
EXPECTED_READ_ONLY_PROBES: tuple[str, ...] = (
    "session.most_recent",
    "spawn_tree.list",
    "agents.list",
    "delegation.status",
    "commands.catalog",
    "session.list",
    "session.active_list",
)

#: The presence-only set (v0.4 U5, KTD11): called bare, never with parameters,
#: and only the *refusal* is read. One member, and the reason it is not in the
#: list above is that calling it with a session id warms that session's agent
#: build — so it is never called with one.
EXPECTED_PRESENCE_PROBES: tuple[str, ...] = ("approval.pending",)

#: Everything a startup round puts on the wire, in baseline order.
EXPECTED_PROBE_SET: tuple[str, ...] = (
    *EXPECTED_READ_ONLY_PROBES,
    *EXPECTED_PRESENCE_PROBES,
)

#: Every required method a startup probe must **never** invoke, written out by
#: hand for the same reason. Each one either mutates session state or answers a
#: request-scoped bridge; discovering that ``session.create`` exists by calling
#: it creates a session, which is the exact failure R34 names.
#:
#: ``session.active_list`` **left this list in v0.4's U5**, deliberately and
#: with the reason written down: U4 put it here because U4 did not probe it, and
#: said in ``talaria/domain/compat.py`` that promoting it was U5's work. It is
#: now probed bare at startup, so listing it as forbidden would assert the
#: opposite of what the shipped probe set does.
FORBIDDEN_AT_STARTUP: tuple[str, ...] = (
    "session.create",
    "session.resume",
    "prompt.submit",
    "session.interrupt",
    "subagent.interrupt",
    "slash.exec",
    "command.dispatch",
    "paste.collapse",
    "approval.respond",
    "clarify.respond",
    "secret.respond",
    "sudo.respond",
    "terminal.read.respond",
)

#: A baseline-shaped reply for each read-only method, transcribed from the
#: handlers at ``7f4d15515`` cited in :data:`~talaria.domain.compat.COMPAT_BASELINE`.
#: Written out rather than generated from ``response_shape``: a body derived from
#: the signature it is compared against agrees with it by construction, and would
#: keep agreeing after the signature was corrupted.
HEALTHY: dict[str, dict[str, Any]] = {
    "session.most_recent": {
        "session_id": "s-9f12",
        "title": "refactor the ingest pipeline",
        "started_at": 1785000000.0,
        "source": "cli",
    },
    "spawn_tree.list": {"entries": []},
    "agents.list": {"processes": []},
    "delegation.status": {
        "active": [],
        "paused": False,
        "max_spawn_depth": 3,
        "max_concurrent_children": 4,
    },
    "commands.catalog": {
        "pairs": [["/status", "Show session status"]],
        "sub": {},
        "canon": {},
        "categories": [],
        "skills": {},
        "skill_count": 0,
        "warning": "",
    },
    # The roster's own reply. Same top-level key as ``session.list`` and a
    # deliberately different row: U1 verified live that an ``active_list`` row
    # carries ``session_key`` and ``status``, which a ``session.list`` row does
    # not, and a fixture that reused the listing row would make the two probes
    # indistinguishable in this file.
    "session.active_list": {
        "sessions": [
            {
                "current": True,
                "id": "s-9f12",
                "last_active": 1785000600.0,
                "message_count": 12,
                "model": "a-model",
                "preview": "let's pick this back up",
                "session_key": "k-9f12",
                "started_at": 1785000000.0,
                "status": "idle",
                "title": "refactor the ingest pipeline",
            }
        ]
    },
    "session.list": {
        "sessions": [
            {
                "id": "s-9f12",
                "title": "refactor the ingest pipeline",
                "preview": "let's pick this back up",
                "started_at": 1785000000.0,
                "message_count": 12,
                "source": "cli",
            }
        ]
    },
}


class StubProvider:
    """The loopback token, minted per dial as KTD11 requires."""

    async def acquire(self) -> Credential:
        return Credential("token", STUB_TOKEN, "file")


#: What a *present* ``approval.pending`` answers a bare call, transcribed from
#: the handler at ``RUNNING_PIN``: ``_sess`` looks up ``params["session_id"]``,
#: misses on the empty string, and returns ``_err(rid, 4001, "session not
#: found")`` before anything is warmed (``tui_gateway/server.py:2496-2503``).
#: U1 measured the same two answers on this machine's wire — 4001 from a present
#: method with a bad session, ``-32601`` from an absent one.
SESSION_NOT_FOUND: int = 4001


def healthy_responder(
    message: dict[str, Any], stub: StubGateway
) -> dict[str, Any] | None:
    """A gateway whose read-only methods match the pin and which has ``approval.pending``.

    ``approval.pending`` answers 4001, not a result, and that is the point: a
    bare presence probe is *supposed* to be refused, and a stub that answered it
    with data would be testing a call Talaria never makes.
    """
    method = str(message.get("method", ""))
    rid = message.get("id")
    if method == "approval.pending":
        return err(rid, SESSION_NOT_FOUND, "session not found")
    if method in HEALTHY:
        return ok(rid, HEALTHY[method])
    return err(rid, METHOD_NOT_FOUND, f"unknown method: {method}")


def responder_missing(name: str) -> Any:
    """A gateway that has every read-only method except ``name``.

    The refusal is the dispatcher's own: ``_err(rid, -32601, f"unknown method:
    {method}")`` at ``tui_gateway/server.py:1762``.
    """

    def _respond(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        method = str(message.get("method", ""))
        rid = message.get("id")
        if method == name:
            return err(rid, METHOD_NOT_FOUND, f"unknown method: {method}")
        return healthy_responder(message, stub)

    return _respond


def responder_drifted(name: str, body: dict[str, Any]) -> Any:
    """A gateway whose ``name`` answers a different shape than the pin recorded."""

    def _respond(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        method = str(message.get("method", ""))
        rid = message.get("id")
        if method == name:
            return ok(rid, body)
        return healthy_responder(message, stub)

    return _respond


@pytest_asyncio.fixture
async def healthy_gateway() -> Any:
    stub = StubGateway(responder=healthy_responder)
    await stub.start()
    try:
        yield stub
    finally:
        await stub.stop()


async def attached(gateway: StubGateway) -> LiveSource:
    """A live source connected to ``gateway``, with its reader running."""
    source = LiveSource(
        AttachTarget.from_url(gateway.url),
        StubProvider(),
        reconnect_delays=FAST_RETRIES,
    )
    state = await source.start()
    assert state == "connected", f"the probe never attached: {state}"
    await gateway.wait_for_attach()
    return source


def methods_received(gateway: StubGateway) -> list[str]:
    """Every method name the *server* saw, in arrival order."""
    return [str(message.get("method", "")) for message in gateway.current.received]


# ── the classification this whole unit rests on ──────────────────────────


def test_the_literal_probe_set_is_exactly_the_pinned_read_only_set() -> None:
    """Ties the hand-written lists above to U3's classification.

    Without this, deleting ``commands.catalog`` from the baseline would leave
    every scenario below still passing over four methods, and the shrunken sweep
    would be invisible. With it, the two lists have to be changed together and a
    reader can see both.
    """
    assert set(EXPECTED_READ_ONLY_PROBES) == set(READ_ONLY_METHODS)
    assert set(EXPECTED_PRESENCE_PROBES) == set(PRESENCE_ONLY_METHODS)
    assert set(FORBIDDEN_AT_STARTUP) == set(EVIDENCE_ONLY_METHODS)
    assert not set(EXPECTED_PROBE_SET) & set(FORBIDDEN_AT_STARTUP)
    assert len(EXPECTED_READ_ONLY_PROBES) == 7, "the read-only set changed size"
    assert len(EXPECTED_PRESENCE_PROBES) == 1, "the presence-only set changed size"
    assert len(FORBIDDEN_AT_STARTUP) == 13, "the evidence-only set changed size"


@pytest.mark.parametrize("method", FORBIDDEN_AT_STARTUP)
@pytest.mark.asyncio
async def test_probing_an_evidence_only_method_raises_before_any_call(method: str) -> None:
    """The guard between a startup probe and a created session, exercised.

    Called directly with each mutating entry, one case per method written out
    above. The dispatcher counts its calls, so this asserts the guard fires
    *before* anything reaches a socket rather than merely that it fires.
    """

    class CountingDispatcher:
        def __init__(self) -> None:
            self.calls = 0

        async def call(
            self,
            method: str,
            params: Any = None,
            *,
            timeout: float | None = None,
        ) -> RpcOutcome:
            self.calls += 1
            return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})

    dispatcher = CountingDispatcher()
    with pytest.raises(ValueError, match="read-only"):
        await probe_method(dispatcher, baseline_for(method))
    assert dispatcher.calls == 0, f"{method} reached the wire before the guard ran"


@pytest.mark.asyncio
async def test_the_guard_lets_a_read_only_method_through() -> None:
    """Pairs the refusals above with the case that must still work.

    A guard that refused everything would satisfy all thirteen cases above and
    break the product entirely.
    """

    class OneShot:
        async def call(
            self, method: str, params: Any = None, *, timeout: float | None = None
        ) -> RpcOutcome:
            return RpcOutcome(
                status="ok",
                method=method,
                request_id="1",
                epoch=1,
                result=HEALTHY[method],
            )

    verdict = await probe_method(OneShot(), baseline_for("agents.list"))
    assert verdict.status == "present"


# ── scenario 1: a healthy gateway ────────────────────────────────────────


@pytest.mark.asyncio
async def test_a_matching_gateway_is_ready_and_every_read_only_method_was_probed(
    healthy_gateway: StubGateway,
) -> None:
    """The positive half: six present verdicts, and six calls the server saw.

    ``ready`` alone would be satisfied by a check that probed nothing at all, so
    the call log is asserted in the same observation.
    """
    source = await attached(healthy_gateway)
    try:
        report = await check_compatibility(source, session_id="s-9f12", timeout=5.0)
    finally:
        await source.close()

    assert report.ready, [line for line in report.lines()]
    assert report.probed == EXPECTED_PROBE_SET
    assert methods_received(healthy_gateway) == list(EXPECTED_PROBE_SET)
    assert [v.status for v in report.verdicts if v.classification == "read-only"] == [
        "present"
    ] * len(EXPECTED_READ_ONLY_PROBES)
    assert unprobed_methods(report) == FORBIDDEN_AT_STARTUP


@pytest.mark.asyncio
async def test_no_mutating_method_appears_in_the_startup_call_log(
    healthy_gateway: StubGateway,
) -> None:
    """Asserted against the server's received-call record (R34, KTD9).

    The negative — no evidence-only name on the wire — is paired with the
    positive that the wire carried the six read-only names, because an empty
    call log satisfies the negative perfectly and proves nothing.
    """
    source = await attached(healthy_gateway)
    try:
        report = await check_compatibility(source, session_id="s-9f12", timeout=5.0)
    finally:
        await source.close()

    seen = methods_received(healthy_gateway)
    assert seen, "the startup check sent nothing at all"
    assert set(seen) == set(EXPECTED_PROBE_SET)
    for forbidden in FORBIDDEN_AT_STARTUP:
        assert forbidden not in seen, f"a startup probe invoked {forbidden}"
    assert probe_set_is_permitted(report)


@pytest.mark.asyncio
async def test_the_probe_sends_the_focused_session_id_not_an_empty_one(
    healthy_gateway: StubGateway,
) -> None:
    """``spawn_tree.list`` is the one read-only method with a session fixture."""
    source = await attached(healthy_gateway)
    try:
        await check_compatibility(source, session_id="s-9f12", timeout=5.0)
    finally:
        await source.close()

    sent = next(
        message
        for message in healthy_gateway.current.received
        if message.get("method") == "spawn_tree.list"
    )
    assert sent["params"]["session_id"] == "s-9f12"
    assert sent["params"]["limit"] == 50


# ── scenario 2: a missing required method (AE7) ──────────────────────────


#: The probed methods whose absence blocks the daily-driver verdict — the whole
#: probe set minus v0.4's two fleet capabilities, written out by hand for the
#: same reason every other list in this file is. ``session.active_list`` and
#: ``approval.pending`` turn a named seam off (v0.4 R10) and leave the verdict
#: alone: U1 verified that the process serving this machine's WebSocket has no
#: ``approval.pending`` and runs the single-session core perfectly, so blocking
#: on it would report every gateway at the old pin as incompatible forever.
BLOCKING_PROBE_SET: tuple[str, ...] = (
    "session.most_recent",
    "spawn_tree.list",
    "agents.list",
    "delegation.status",
    "commands.catalog",
    "session.list",
)


def test_the_blocking_probe_set_is_the_probe_set_minus_the_fleet_capabilities() -> None:
    """Ties the hand-written list above to the per-entry flag it mirrors."""
    non_blocking = {
        entry.method
        for entry in COMPAT_BASELINE
        if entry.method in EXPECTED_PROBE_SET and not entry.blocks_daily_driver
    }
    assert non_blocking == {"session.active_list", "approval.pending"}
    assert set(BLOCKING_PROBE_SET) == set(EXPECTED_PROBE_SET) - non_blocking


@pytest.mark.parametrize("absent", BLOCKING_PROBE_SET)
@pytest.mark.asyncio
async def test_a_missing_method_is_named_and_blocks_ready(absent: str) -> None:
    """AE7's headline: name the method, return not-ready.

    Every baseline read-only method gets its own case, written out in
    :data:`BLOCKING_PROBE_SET`, because a check that special-cased one name
    would pass a single-method test. v0.4's two fleet capabilities assert the
    opposite property and are covered separately:
    :func:`test_an_absent_roster_names_a_seam_without_condemning_the_gateway`
    below for ``session.active_list``, and
    ``tests/transport/test_seam_probes.py::test_an_absent_fleet_method_does_not_block_the_daily_driver_verdict``
    for ``approval.pending``.

    That sentence said "covered separately" of *both* until 2026-08-17, when
    only one of the two was — and the uncovered one was carrying a live defect.
    Narrowing a parametrized set is not free: the cases it drops stop being
    tested unless something else picks them up by name, so the claim that
    something else did is worth stating as two references that can be followed
    rather than one gesture at a file.
    """
    stub = StubGateway(responder=responder_missing(absent))
    await stub.start()
    try:
        source = await attached(stub)
        try:
            report = await check_compatibility(source, session_id="s-9f12", timeout=5.0)
        finally:
            await source.close()
    finally:
        await stub.stop()

    assert not report.ready
    assert report.missing == (absent,)
    assert report.verdict_for(absent).status == "missing"
    # The positive pair: every other probe still matched, so "not ready" points
    # at one method rather than at a gateway nobody reached.
    survivors = [v.method for v in report.verdicts if v.status == "present"]
    assert set(survivors) == set(EXPECTED_PROBE_SET) - {absent}
    assert any(absent in line and "MISSING" in line for line in report.lines())


@pytest.mark.asyncio
async def test_an_absent_roster_names_a_seam_without_condemning_the_gateway() -> None:
    """KTD4 on the probe's own output, not on the baseline data it reads.

    A gateway at the old pin has no ``session.active_list``. Its roster seam is
    off and must say so, and the gateway itself is a perfectly good single-session
    daily driver — that is the whole content of ``blocks_daily_driver``.

    This is asserted on the verdict the *prober* built, because the flag lives in
    two places: on the baseline entry, and on the verdict copied from it. A test
    that reads the entry proves only that somebody typed ``False`` in the table.
    Until 2026-08-17 the copy was silently lost on exactly this path — the entry's
    fixture is bare, so the probe returns from a branch that omitted the field and
    took a ``True`` default — and an absent roster reported the gateway as a
    compatibility gap while every test stayed green.
    """
    stub = StubGateway(responder=responder_missing("session.active_list"))
    await stub.start()
    try:
        source = await attached(stub)
        try:
            report = await check_compatibility(source, session_id="s-9f12", timeout=5.0)
        finally:
            await source.close()
    finally:
        await stub.stop()

    verdict = report.verdict_for("session.active_list")
    assert verdict.status == "missing"
    assert verdict.blocks_daily_driver is False
    assert verdict.blocks_ready is False
    # Named, and not fatal: both halves, because either alone is satisfiable by
    # a probe that simply failed to notice the method was gone.
    assert report.missing == ("session.active_list",)
    assert report.ready


# ── scenario 3: a drifted response shape ─────────────────────────────────


@pytest.mark.asyncio
async def test_a_dropped_response_key_is_flagged_by_name() -> None:
    body = dict(HEALTHY["delegation.status"])
    del body["active"]
    stub = StubGateway(responder=responder_drifted("delegation.status", body))
    await stub.start()
    try:
        source = await attached(stub)
        try:
            report = await check_compatibility(source, session_id="s-9f12", timeout=5.0)
        finally:
            await source.close()
    finally:
        await stub.stop()

    assert not report.ready
    assert report.drifted == ("delegation.status",)
    verdict = report.verdict_for("delegation.status")
    assert [(d.key, d.kind) for d in verdict.drifts] == [("active", "missing-key")]
    assert "no longer carries 'active'" in verdict.describe()
    # Still six probes, five of them clean: the drift is a statement about one
    # method, not about the run.
    assert report.probed == EXPECTED_PROBE_SET


@pytest.mark.asyncio
async def test_a_changed_value_kind_is_flagged_with_both_kinds_named() -> None:
    """``paused`` going from boolean to string is the drift a client breaks on."""
    body = dict(HEALTHY["delegation.status"])
    body["paused"] = "no"
    stub = StubGateway(responder=responder_drifted("delegation.status", body))
    await stub.start()
    try:
        source = await attached(stub)
        try:
            report = await check_compatibility(source, session_id="s-9f12", timeout=5.0)
        finally:
            await source.close()
    finally:
        await stub.stop()

    verdict = report.verdict_for("delegation.status")
    assert verdict.status == "drifted"
    assert [(d.key, d.expected, d.observed) for d in verdict.drifts] == [
        ("paused", "bool", "str")
    ]
    assert not report.ready


@pytest.mark.asyncio
async def test_an_added_response_key_is_flagged_rather_than_ignored() -> None:
    """New keys are drift too — quietly tolerating them is how a baseline rots."""
    body = dict(HEALTHY["agents.list"])
    body["cursor"] = "abc"
    stub = StubGateway(responder=responder_drifted("agents.list", body))
    await stub.start()
    try:
        source = await attached(stub)
        try:
            report = await check_compatibility(source, session_id="s-9f12", timeout=5.0)
        finally:
            await source.close()
    finally:
        await stub.stop()

    verdict = report.verdict_for("agents.list")
    assert [(d.key, d.kind) for d in verdict.drifts] == [("cursor", "unexpected-key")]
    assert not report.ready


@pytest.mark.asyncio
async def test_a_documented_empty_answer_is_not_reported_as_drift() -> None:
    """``session.most_recent`` answers ``{"session_id": null}`` on a quiet machine
    (``tui_gateway/methods_session.py:234``), and U3 recorded the other three keys
    as optional for exactly that reason. Flagging it would tell every operator
    with no prior session that their gateway was incompatible."""
    stub = StubGateway(responder=responder_drifted("session.most_recent", {"session_id": None}))
    await stub.start()
    try:
        source = await attached(stub)
        try:
            report = await check_compatibility(source, session_id="s-9f12", timeout=5.0)
        finally:
            await source.close()
    finally:
        await stub.stop()

    assert report.verdict_for("session.most_recent").status == "present"
    assert report.ready


# ── the outcomes that are not answers ────────────────────────────────────


@pytest.mark.asyncio
async def test_a_refused_fixture_is_reported_without_blocking_ready() -> None:
    """A handler that refuses *this* fixture still proves the method exists.

    Any code other than ``-32601`` means dispatch found a handler
    (``tui_gateway/server.py:1758-1762``). Blocking on it would make the verdict
    hostage to fixtures this repository chose.
    """
    stub = StubGateway(
        responder=lambda message, _stub: (
            err(message.get("id"), 4004, "no such session")
            if message.get("method") == "spawn_tree.list"
            else healthy_responder(message, _stub)
        )
    )
    await stub.start()
    try:
        source = await attached(stub)
        try:
            report = await check_compatibility(source, session_id="s-9f12", timeout=5.0)
        finally:
            await source.close()
    finally:
        await stub.stop()

    verdict = report.verdict_for("spawn_tree.list")
    assert verdict.status == "refused"
    assert "4004" in verdict.describe()
    assert report.ready, "a refused fixture must not block the verdict"


@pytest.mark.asyncio
async def test_a_probe_that_never_comes_back_blocks_rather_than_passing() -> None:
    """Silence is a gap, and AE7 says the verdict is blocked on any gap.

    The failure this prevents is the one that matters most: a check that treated
    "no reply" as a pass would report a compatible gateway for a socket that
    answered nothing at all.
    """

    class SilentDispatcher:
        async def call(
            self, method: str, params: Any = None, *, timeout: float | None = None
        ) -> RpcOutcome:
            return unknown_outcome(method, NO_REPLY_IN_TIME, epoch=1)

    report = await check_compatibility(SilentDispatcher())

    assert not report.ready
    assert [v.status for v in report.verdicts if v.classification == "read-only"] == [
        "unproved"
    ] * len(EXPECTED_READ_ONLY_PROBES)
    # The presence probe is silenced too, and it reports the same thing rather
    # than concluding the method is gone — silence is not absence anywhere.
    assert [
        v.status for v in report.verdicts if v.classification == "presence-only"
    ] == ["unproved"] * len(EXPECTED_PRESENCE_PROBES)
    assert all("UNPROVED" in line for line in report.lines()[1:])


@pytest.mark.asyncio
async def test_a_non_object_result_is_drift_rather_than_an_empty_comparison() -> None:
    """A gateway answering ``result: []`` has changed shape, not lost every key."""

    class ListResult:
        async def call(
            self, method: str, params: Any = None, *, timeout: float | None = None
        ) -> RpcOutcome:
            return RpcOutcome(
                status="ok", method=method, request_id="1", epoch=1, result=None
            )

    report = await check_compatibility(ListResult())
    verdict = report.verdict_for("agents.list")
    assert verdict.status == "drifted"
    assert [d.key for d in verdict.drifts] == ["<result>"]


# ── what the report says about what it did not do ────────────────────────


@pytest.mark.asyncio
async def test_a_clean_report_still_says_how_much_it_did_not_verify(
    healthy_gateway: StubGateway,
) -> None:
    """The honesty clause. A summary reading "compatible" on a run that probed
    eight of twenty-one methods would be a claim about thirteen it never
    touched."""
    source = await attached(healthy_gateway)
    try:
        report = await check_compatibility(source, session_id="s-9f12", timeout=5.0)
    finally:
        await source.close()

    head = report.lines()[0]
    assert "0 blocking" in head
    assert "13 unverified at runtime" in head
    assert "7f4d15515" in head
    assert len(report.verdicts) == len(COMPAT_BASELINE) == 21


@pytest.mark.asyncio
async def test_the_check_never_leaves_a_call_outstanding(
    healthy_gateway: StubGateway,
) -> None:
    """Sequential by construction: nothing is in flight when the report returns."""
    source = await attached(healthy_gateway)
    try:
        await check_compatibility(source, session_id="s-9f12", timeout=5.0)
        assert source.correlator.in_flight == 0
        assert source.correlator.lost_outcomes == 0
    finally:
        await source.close()


@pytest.mark.asyncio
async def test_the_probe_survives_a_gateway_that_hangs_up_mid_check() -> None:
    """A dropped socket produces an unproved row, never a raised exception.

    Startup is where a client is least able to recover from a traceback, so the
    check reports what it learned and lets the caller decide. The gateway drops
    the connection while answering the third probe — raising out of the stub's
    handler is how its ``_handle`` loop ends and the socket closes — so the loss
    lands on a named method rather than on whichever call the clock happened to
    catch.
    """

    def _hangs_up(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        if message.get("method") == "agents.list":
            raise RuntimeError("the stub drops the socket here")
        return healthy_responder(message, stub)

    stub = StubGateway(responder=_hangs_up)
    await stub.start()
    try:
        source = await attached(stub)
        try:
            report = await asyncio.wait_for(
                check_compatibility(source, session_id="s-9f12", timeout=2.0),
                timeout=20.0,
            )
        finally:
            await source.close()
    finally:
        await stub.stop()

    assert not report.ready
    assert report.verdict_for("agents.list").status == "unproved"
    # Paired with the positive: the two probes before the drop still landed, so
    # this is a check that ran and lost one answer, not one that never started.
    assert report.verdict_for("session.most_recent").status == "present"
    assert report.verdict_for("spawn_tree.list").status == "present"
