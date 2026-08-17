"""Focus movement under churn, and what the picker's rows say (v0.4 U4).

Three claims, all driven through the assembled application over a real replay
source, the convention every suite in this directory keeps.

*The focus-churn repairs stay green under the registry.* ``focus_session``
retains ``prompts``, ``flushed_prompt_ids`` and ``approvals_seen`` across a
switch and resets ``withdrawn_approvals``; the domain suite pins the rules and
this pins the consequence the operator actually feels — three sessions, each
blocked on its own approval, survive rapid switching and every one of them is
answerable when its session comes back into focus. The registry is live
underneath all of it, because the app now holds a ``FleetState`` whose
``focused`` field *is* the focused engine.

*The switch refusal is fleet-scoped* (KTD9/R6). An answer travelling for a
session Talaria is not looking at refuses a switch exactly as a focused answer
does, through the same picker path the operator uses.

*The picker's rows are registry summaries* (R3). Lifecycle, what the session is
waiting on, where the observation came from and how old it is — and those
survive a gateway-supplied title of any width, with every byte of that title
inert on screen (R23) and no operator-typed answer anywhere near a row (R22).
"""

from __future__ import annotations

import asyncio
from collections.abc import Mapping
from dataclasses import replace
from typing import Any

import pytest

from talaria.domain import state as domain_state
from talaria.domain.commands import CATALOG_METHOD
from talaria.transport.rpc import RpcOutcome
from talaria.ui.app import (
    LIST_ACTIVE_METHOD,
    LIST_SESSIONS_METHOD,
    RESUME_METHOD,
    TalariaApp,
)
from talaria.ui.dialog import PickerDialog
from talaria.ui.picker import NEVER_OBSERVED_STATUS
from tests.ui.conftest import event, feed, live_app, screen_text, settle

#: A title far wider than any row: the picker must clip it rather than let it
#: push the row's fixed facts off an 80-column screen.
LONG_TITLE = "an extremely long session title that nobody would ever type " * 3

#: What a gateway-supplied title must not be allowed to do on screen.
MARKUP_CANARY = "[b]styled?[/b]"
CONTROL_CANARY = "\x07bell\x1b[31m"


def session_row(session_id: str, *, title: str = "", **extra: Any) -> dict[str, Any]:
    row = {
        "id": session_id,
        "title": title,
        "preview": "",
        "started_at": 1785000000.0,
        "message_count": 0,
        "source": "cli",
    }
    row.update(extra)
    return row


def active_row(
    session_id: str,
    *,
    session_key: str = "",
    status: str = "idle",
    title: str = "",
) -> dict[str, Any]:
    return {
        "id": session_id,
        "session_key": session_key or session_id,
        "status": status,
        "title": title,
        "preview": "",
        "model": "a-model",
        "message_count": 2,
        "started_at": 1785000000.0,
        "last_active": 1785000000.0,
    }


def listing(*rows: Mapping[str, Any]) -> RpcOutcome:
    return RpcOutcome(
        status="ok",
        method=LIST_SESSIONS_METHOD,
        request_id="1",
        epoch=1,
        result={"sessions": list(rows)},
    )


def roster(*rows: Mapping[str, Any]) -> RpcOutcome:
    return RpcOutcome(
        status="ok",
        method=LIST_ACTIVE_METHOD,
        request_id="1",
        epoch=1,
        result={"sessions": list(rows)},
    )


def resume_reply(session_id: str, **extra: Any) -> RpcOutcome:
    result: dict[str, Any] = {
        "session_id": session_id,
        "resumed": session_id,
        "session_key": session_id,
        "message_count": 0,
        "messages": [],
        "messages_omitted": False,
        "info": {},
        "running": False,
        "started_at": 1785000000.0,
        "status": "idle",
    }
    result.update(extra)
    return RpcOutcome(
        status="ok", method=RESUME_METHOD, request_id="1", epoch=1, result=result
    )


class TargetedDispatcher:
    """Answers ``session.resume`` per requested session id, everything else
    from a method table. The three-session churn test dispatches one method at
    three different targets, which a method-keyed table cannot tell apart."""

    def __init__(
        self,
        outcomes: Mapping[str, RpcOutcome] | None = None,
        *,
        resumes: Mapping[str, RpcOutcome] | None = None,
    ) -> None:
        self._outcomes = dict(outcomes or {})
        self._resumes = dict(resumes or {})
        self.calls: list[tuple[str, Mapping[str, Any]]] = []

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome:
        self.calls.append((method, dict(params or {})))
        if method == RESUME_METHOD:
            target = str((params or {}).get("session_id") or "")
            if target in self._resumes:
                return self._resumes[target]
        outcome = self._outcomes.get(method)
        if outcome is not None:
            return outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})

    @property
    def operator_calls(self) -> list[tuple[str, Mapping[str, Any]]]:
        return [call for call in self.calls if call[0] != CATALOG_METHOD]


def approval_frame(request_id: str, description: str, *, session: str) -> dict[str, Any]:
    return event(
        "approval.request",
        {
            "request_id": request_id,
            "description": description,
            "command": f"rm -rf /tmp/{request_id}",
            "choices": ["once", "deny"],
        },
        session=session,
    )


def dialog(app: TalariaApp) -> PickerDialog:
    screen = app.screen
    assert isinstance(screen, PickerDialog), f"expected the picker, got {type(screen).__name__}"
    return screen


async def open_sessions(app: TalariaApp, pilot: Any) -> None:
    app.composer.text_area.focus()
    app.composer.text = "/sessions"
    await pilot.press("enter")
    await app.settle_live()
    await pilot.pause()


# ── PC6: the churn repairs, kept green under the registry ────────────────


@pytest.mark.asyncio
async def test_three_churned_sessions_are_each_answerable_when_refocused() -> None:
    """Rapid switching across three sessions, each holding its own approval.

    The gateway never re-announces an outstanding prompt across a switch — it
    started blocking on ``.request`` and has no reason to know Talaria stopped
    showing it — so a ``focus_session`` that dropped the registry would orphan
    every one of these controls the moment the operator moved on. What is
    asserted is the operator-visible consequence: each session's card is on
    screen when that session is focused, absent when it is not, and the answer
    for the first session still reaches the wire after two intervening
    switches.
    """
    dispatcher = TargetedDispatcher(
        resumes={
            "s2": resume_reply("s2"),
            "s3": resume_reply("s3"),
            "s1": resume_reply("s1"),
        }
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        feed(app, approval_frame("a-s1", "first session's command", session="s1"))
        await settle(app, pilot)
        assert "first session's command" in screen_text(app)

        await app.switch_session("s2")
        await settle(app, pilot)
        feed(app, approval_frame("a-s2", "second session's command", session="s2"))
        await settle(app, pilot)

        await app.switch_session("s3")
        await settle(app, pilot)
        feed(app, approval_frame("a-s3", "third session's command", session="s3"))
        await settle(app, pilot)

        # Only the focused session's card renders; the other two are retained
        # in the registry, not on screen.
        screen = screen_text(app)
        assert "third session's command" in screen
        assert "first session's command" not in screen
        assert "second session's command" not in screen
        assert sorted(p.session_id or "" for p in app.state.prompts) == ["s1", "s2", "s3"]

        # Back to the first session: its card is answerable again.
        await app.switch_session("s1")
        await settle(app, pilot)
        assert "first session's command" in screen_text(app)

        outcome = await app.respond_live("a-s1", "once", expected_kind="approval")
        await settle(app, pilot)
        assert outcome is not None and outcome.confirmed
        assert any(method == "approval.respond" for method, _ in dispatcher.operator_calls)
        assert [p.request_id for p in app.state.prompts] == ["a-s2", "a-s3"]

        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_registry_follows_the_focused_session_across_switches() -> None:
    """The registry is not a second copy of the focused state — it *contains*
    it. After a switch, the fleet's focused cursor is the session on screen,
    and the row it points at is the one this run drives."""
    dispatcher = TargetedDispatcher(resumes={"s2": resume_reply("s2")})
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await settle(app, pilot)

        await app.switch_session("s2")
        await settle(app, pilot)

        assert app.fleet.focused is app.state
        key = app.fleet.focused_key()
        assert key == (app.fleet_profile, "s2")
        assert app.fleet.rows[key].ownership == "we_drive"
        assert key in app.fleet.protected_keys()
        await app.shutdown_sources()


# ── KTD9: the refusal is fleet-scoped, and every path says so ────────────


@pytest.mark.asyncio
async def test_an_answer_for_another_session_refuses_the_picker_and_the_switch() -> None:
    """R6 through the two surfaces U4 owns.

    The answer is held through :meth:`TalariaApp.fleet_answer` — the public
    entry point the queue will use for a session it names but does not focus —
    with the focused engine's own ``answering`` tuple provably empty. On the
    shipped focused-only rule both acts below would go through.
    """
    dispatcher = TargetedDispatcher(
        outcomes={
            LIST_SESSIONS_METHOD: listing(session_row("s1", title="current")),
            LIST_ACTIVE_METHOD: roster(active_row("s1", title="current")),
        },
        resumes={"s2": resume_reply("s2")},
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await settle(app, pilot)

        with app.fleet_answer(session_id="s9", request_key="req-9"):
            assert app.state.answering == ()

            await open_sessions(app, pilot)
            assert not isinstance(app.screen, PickerDialog), (
                "the picker opened while an answer was travelling"
            )
            assert "still travelling" in app.composer.notice
            assert LIST_SESSIONS_METHOD not in [m for m, _ in dispatcher.operator_calls]

            result = await app.switch_session("s2")
            await settle(app, pilot)
            assert result is None
            assert RESUME_METHOD not in [m for m, _ in dispatcher.operator_calls]
            assert app.state.focused_session_id == "s1"

        # Released, both surfaces work again.
        await open_sessions(app, pilot)
        assert isinstance(app.screen, PickerDialog)
        await pilot.press("escape")
        await pilot.pause()
        await app.shutdown_sources()


# ── R3: what a registry-backed row says, and how wide it is allowed to be ─


@pytest.mark.asyncio
async def test_a_picker_row_keeps_its_status_and_age_at_eighty_columns() -> None:
    """The fixed facts come first and the gateway's title is clipped behind
    them. A row that led with the title would lose lifecycle, source and age
    to the first operator who names a session with a sentence."""
    dispatcher = TargetedDispatcher(
        outcomes={
            LIST_SESSIONS_METHOD: listing(
                session_row("s1", title="current"),
                session_row("s2", title=LONG_TITLE),
                session_row("s3", title="unpolled"),
            ),
            LIST_ACTIVE_METHOD: roster(
                active_row("s1", title="current"),
                active_row("s2", status="waiting", title=LONG_TITLE),
            ),
        }
    )
    app = live_app(dispatcher)

    async with app.run_test(size=(80, 24)) as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await settle(app, pilot)
        await open_sessions(app, pilot)

        rows = dialog(app).row_texts
        waiting = next(row for row in rows if "waiting" in row)
        # The poll named the wait but not its kind — the gateway's row carries
        # no kind at all — so the row says ``unobserved`` rather than guessing.
        assert "on unobserved" in waiting
        assert "poll" in waiting
        # The title is present and clipped; its far end never reaches the row.
        assert "an extremely long session" in waiting
        assert not waiting.rstrip().endswith("type")
        assert "…" in waiting

        # The listed-but-unpolled session is never-observed, not idle.
        assert any(NEVER_OBSERVED_STATUS in row for row in rows)
        assert not any(row.endswith("idle") and "unpolled" in row for row in rows)

        screen = screen_text(app)
        assert "waiting" in screen and "poll" in screen
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_gateway_title_is_inert_on_a_picker_row() -> None:
    """R23 on the surface U4 added. Markup is text, control sequences are
    pictures, and neither one styles or executes anything."""
    dispatcher = TargetedDispatcher(
        outcomes={
            LIST_SESSIONS_METHOD: listing(
                session_row("s1", title="current"),
                session_row("s2", title=f"{MARKUP_CANARY}{CONTROL_CANARY}"),
            ),
            LIST_ACTIVE_METHOD: roster(active_row("s1", title="current")),
        }
    )
    app = live_app(dispatcher)

    async with app.run_test(size=(80, 24)) as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await settle(app, pilot)
        await open_sessions(app, pilot)

        screen = screen_text(app)
        # The markup is shown, not applied — the canary reaches the screen as
        # the literal characters an operator would have to read.
        assert MARKUP_CANARY in screen
        # The control characters never reach it. What stands in for them is
        # the defanger's control picture, never a live escape sequence, and
        # the ordinary text around them is untouched.
        assert "\x07" not in screen and "\x1b[31m" not in screen
        assert "␇bell␛[31m" in screen
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_answered_secret_never_reaches_a_picker_row() -> None:
    """R22: a value the operator typed into a bridge is not row material, and
    the picker is a surface that renders a great deal of gateway text beside
    it. Driven through the real answer path rather than asserted about it."""
    secret = "canary-secret-8fQ2"
    dispatcher = TargetedDispatcher(
        outcomes={
            LIST_SESSIONS_METHOD: listing(
                session_row("s1", title="current"), session_row("s2", title="other")
            ),
            LIST_ACTIVE_METHOD: roster(active_row("s1", title="current")),
        }
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        feed(
            app,
            event(
                "secret.request",
                {"request_id": "sec-1", "prompt": "API token", "env_var": "TOKEN"},
                session="s1",
            ),
        )
        await settle(app, pilot)
        await app.respond_live("sec-1", secret, expected_kind="secret")
        await settle(app, pilot)

        await open_sessions(app, pilot)
        assert all(secret not in row for row in dialog(app).row_texts)
        assert secret not in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_withdrawal_hedge_does_not_follow_a_switch(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """AE6's first clause: no withdrawn-approval hedge leaks into another session.

    This replaces an assertion that could not fail. The earlier version checked
    ``withdrawn_approvals == 0`` at the end of a churn test in which nothing was
    ever withdrawn — so the counter read zero on the repaired and unrepaired code
    alike, and deleting the reset in ``focus_session`` left the whole file green.

    A hedge is the one screen state with no event behind it: it appears when an
    approval ages out, and it is a statement about *this* session's blocked
    turn. Carried across a switch it becomes a claim about a session where no
    withdrawal happened, on a screen the operator is reading to decide what that
    session is doing.
    """
    dispatcher = TargetedDispatcher(resumes={"s2": resume_reply("s2")})
    app = live_app(dispatcher, coalesce_interval=0.01)

    async with app.run_test(size=(100, 30)) as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        # A turn must be in flight for the activity line to render at all — the
        # hedge is what replaces a "working…" claim, so there has to be one.
        feed(app, event("message.start", {}))
        feed(
            app,
            approval_frame("a-s1", "first session's command", session="s1"),
            seq=101,
        )
        await settle(app, pilot)
        assert "first session's command" in screen_text(app)

        # Age it out: the hedge is now on the activity line and belongs to s1.
        monkeypatch.setattr(domain_state, "APPROVAL_STALE_AFTER", 0.05)
        for _ in range(20):
            await asyncio.sleep(0.01)
            await pilot.pause()
        assert app.state.withdrawn_approvals == 1
        # The hedge is the operator-visible consequence, and it lives on the
        # activity line rather than in the transcript.
        assert "withdrawn" in app.prompts.activity_text

        await app.switch_session("s2")
        await settle(app, pilot)

        # The hedge stayed with the session it happened in.
        assert app.state.withdrawn_approvals == 0
        assert "withdrawn" not in app.prompts.activity_text
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_returning_to_our_own_waiting_session_latches_no_false_residue() -> None:
    """CR4 round 2: the residue latch fired on a return, not only on a steal.

    ``_apply_handover`` runs on every focus-moving landing, and the latch decided
    from the row's waiting kind alone. So coming back through ``/sessions`` to a
    session Talaria itself drives — one the roster reports ``waiting`` because it
    is blocked on *our own* approval — printed "attach recovered no card … it was
    announced to the client this attach displaced" directly above that card,
    still on screen and still answerable.

    Three claims, all false: no card was lost, nothing is resolved-failed, and no
    client was displaced. With the gateway revision serving this machine
    hydrating nothing at all, this is the ordinary path rather than an edge.

    The guard is ownership, read *before* ``mark_we_drive`` destroys it, because
    the latch's own sentence is about a displaced transport and ownership is
    exactly the question "was anything displaced".
    """
    dispatcher = TargetedDispatcher(
        {
            LIST_SESSIONS_METHOD: listing(
                session_row("s1", title="current"),
                session_row("s2", title="ours and waiting"),
            ),
            LIST_ACTIVE_METHOD: roster(
                active_row("s1", title="current"),
                active_row("s2", status="waiting", title="ours and waiting"),
            ),
        },
        resumes={"s1": resume_reply("s1"), "s2": resume_reply("s2")},
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")

        # Talaria itself lands on s2 before any roster exists, so no dialog and
        # the run owns it.
        await app.switch_session("s2")
        await settle(app, pilot)
        assert app.state.focused_session_id == "s2"
        assert app.fleet.rows[(app.fleet_profile, "s2")].ownership == "we_drive"

        feed(app, approval_frame("a-s2", "our own blocked command", session="s2"))
        await settle(app, pilot)
        assert "our own blocked command" in screen_text(app)

        # Away, then back through the picker — which is what folds the roster in
        # and makes the row report ``waiting``, the precondition the latch reads.
        await app.switch_session("s1")
        await settle(app, pilot)
        assert app.state.focused_session_id == "s1"

        await app.open_sessions_picker()
        await settle(app, pilot)
        assert app.fleet.rows[(app.fleet_profile, "s2")].status == "waiting"
        for _ in range(len(dialog(app).row_texts)):
            if "ours and waiting" in dialog(app).active_row_text:
                break
            await pilot.press("down")
            await pilot.pause()
        await pilot.press("enter")
        await settle(app, pilot)
        assert app.state.focused_session_id == "s2"

        screen = screen_text(app)
        assert "resolved-failed" not in screen
        assert "displaced" not in screen
        assert "resolved-failed" not in (
            app.fleet.rows[(app.fleet_profile, "s2")].last_notice or ""
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_historical_resume_of_a_once_waiting_session_names_no_displaced_client() -> None:
    """CR4 round 3: the dialog and the residue asked different questions.

    ``attach_confirm_row`` asks "live and not ours"; the residue guard asked only
    "not ours". A row can be both at once — a roster sweep that omits a session
    clears its live flag and deliberately leaves ``status`` and ``waiting_kind``
    saying what they last knew — so an ordinary historical resume of a session
    whose client had since gone showed no dialog, which is right (KTD8: "nothing
    live is stolen"), and then told the operator a client was displaced anyway.

    Both now read one shared predicate, so the two spellings cannot drift again.
    """
    rosters = [
        roster(
            active_row("s1", title="current"),
            active_row("s2", status="waiting", title="somebody else is here"),
        ),
        # The other client's session ended: the sweep omits it, while
        # session.list still carries it as history.
        roster(active_row("s1", title="current")),
    ]

    class SequencedRoster(TargetedDispatcher):
        async def call(self, method: str, params: Any = None, *, timeout: Any = None) -> Any:
            if method == LIST_ACTIVE_METHOD and rosters:
                self.calls.append((method, dict(params or {})))
                return rosters.pop(0)
            return await super().call(method, params, timeout=timeout)

    dispatcher = SequencedRoster(
        {
            LIST_SESSIONS_METHOD: listing(
                session_row("s1", title="current"),
                session_row("s2", title="somebody else is here"),
            )
        },
        resumes={"s1": resume_reply("s1"), "s2": resume_reply("s2")},
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")

        # First look: s2 is live and somebody else's. The operator does not take it.
        await app.open_sessions_picker()
        await settle(app, pilot)
        await pilot.press("escape")
        await settle(app, pilot)

        # Second look: that client is gone, so the roster omits s2.
        await app.open_sessions_picker()
        await settle(app, pilot)
        row = app.fleet.rows[(app.fleet_profile, "s2")]
        assert not row.live_listed
        assert row.status == "waiting"  # the stale wait a latch would read
        assert row.ownership != "we_drive"

        for _ in range(len(dialog(app).row_texts)):
            if "somebody else is here" in dialog(app).active_row_text:
                break
            await pilot.press("down")
            await pilot.pause()

        # The warning and the gate must agree. The picker suffix once re-derived
        # this from the row's *freshness* class rather than the gateway's
        # liveness, so it said "live elsewhere" on a row whose confirmation
        # correctly never fired — the same two-spellings defect one surface over.
        assert "live elsewhere" not in dialog(app).active_row_text

        await pilot.press("enter")
        await settle(app, pilot)

        assert app.state.focused_session_id == "s2"
        screen = screen_text(app)
        assert "displaced" not in screen
        assert "resolved-failed" not in screen
        assert "resolved-failed" not in (
            app.fleet.rows[(app.fleet_profile, "s2")].last_notice or ""
        )
        await app.shutdown_sources()
