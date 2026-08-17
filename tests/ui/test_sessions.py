"""U7's ``/sessions`` switcher: the picker, the switch, and the two refusals.

Every test here drives the real :class:`~talaria.ui.app.TalariaApp` over a
real :class:`~talaria.replay.source.ReplaySource`, the same convention
``tests/ui/test_picker.py`` and ``tests/ui/test_prompts.py`` follow — the
dispatcher is a double so the outcome is chosen rather than provoked, and no
socket is needed to prove what Talaria does with the answer.

KTD3's whole point is that the switcher reuses the startup landing path —
``session.resume`` then ``_land_session`` — rather than growing its own. That
reuse is exercised end to end here: choosing a row in the picker is asserted
to render seeded history (U6) and to drop the previously focused session's
prompts (U5), through the *second* caller of both, not through a second
implementation of either.
"""

from __future__ import annotations

import asyncio
from collections.abc import Mapping
from dataclasses import replace
from typing import Any

import pytest

from talaria.domain.commands import CATALOG_METHOD
from talaria.domain.state import ATTACH_RESIDUE_PREFIX, REFUSED_SWITCH_WHILE_ANSWERING
from talaria.replay.controls import ReplayControls
from talaria.replay.source import ReplaySource
from talaria.transport.rpc import RpcOutcome
from talaria.ui.app import (
    ATTACH_CONFIRM_UNKNOWN_KIND,
    ATTACH_DECLINED,
    LIST_ACTIVE_METHOD,
    LIST_SESSIONS_METHOD,
    NO_SESSIONS,
    RESUME_METHOD,
    SESSIONS_LIST_FAILED,
    SESSIONS_STALE_EPOCH,
    SESSIONS_UNAVAILABLE,
    SWITCH_ALREADY_IN_FLIGHT,
    TalariaApp,
)
from talaria.ui.dialog import REFUSED_PREFIX, ConfirmDialog, PickerDialog
from talaria.ui.picker import SESSION_ALREADY_FOCUSED
from tests.ui.conftest import event, feed, live_app, records, screen_text, settle


def session_row(
    session_id: str,
    *,
    title: str = "",
    started_at: float = 1785000000.0,
    message_count: int = 0,
    source: str = "cli",
    preview: str = "",
) -> dict[str, Any]:
    return {
        "id": session_id,
        "title": title,
        "preview": preview,
        "started_at": started_at,
        "message_count": message_count,
        "source": source,
    }


def list_outcome(*rows: Mapping[str, Any]) -> RpcOutcome:
    return RpcOutcome(
        status="ok",
        method=LIST_SESSIONS_METHOD,
        request_id="1",
        epoch=1,
        result={"sessions": list(rows)},
    )


def failed_list_outcome(reason: str = "the gateway said no") -> RpcOutcome:
    return RpcOutcome(
        status="error",
        method=LIST_SESSIONS_METHOD,
        request_id="1",
        epoch=1,
        error_code=4000,
        error_message=reason,
    )


def resume_outcome(
    session_id: str,
    *,
    session_key: str | None = None,
    messages: list[dict[str, Any]] | None = None,
    message_count: int = 0,
) -> RpcOutcome:
    stored = session_key or session_id
    return RpcOutcome(
        status="ok",
        method=RESUME_METHOD,
        request_id="1",
        epoch=1,
        result={
            "session_id": session_id,
            "resumed": stored,
            "session_key": stored,
            "message_count": message_count,
            "messages": messages or [],
            "messages_omitted": False,
            "info": {},
            "inflight": None,
            "running": False,
            "started_at": 1785000000.0,
            "status": "idle",
        },
    )


def approval_frame(command: str, description: str, *, session: str = "s1") -> dict[str, Any]:
    return event(
        "approval.request",
        {"description": description, "command": command, "choices": ["once", "deny"]},
        session=session,
    )


class ScriptedDispatcher:
    """Answers each method from a fixed table, recording every call.

    Unlike ``tests.ui.conftest.RecordingDispatcher`` — one outcome for every
    call — this test file needs ``session.list`` and ``session.resume`` to
    answer differently in the same run.
    """

    def __init__(self, outcomes: Mapping[str, RpcOutcome] | None = None) -> None:
        self._outcomes = dict(outcomes or {})
        self.calls: list[tuple[str, Mapping[str, Any]]] = []

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome:
        self.calls.append((method, dict(params or {})))
        outcome = self._outcomes.get(method)
        if outcome is not None:
            return outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})

    @property
    def operator_calls(self) -> list[tuple[str, Mapping[str, Any]]]:
        """Every call except the startup catalogue fetch."""
        return [call for call in self.calls if call[0] != CATALOG_METHOD]


class HoldingScriptedDispatcher(ScriptedDispatcher):
    """Parks its first non-catalogue call until :attr:`gate` is set.

    Mirrors ``tests/ui/test_prompts.py``'s ``HoldingDispatcher`` — the only
    way to observe the window ``respond_to_prompt`` opens: the answer is on
    the wire, the reply has not come back, and ``/sessions`` is typed into
    exactly that window.
    """

    def __init__(self, outcomes: Mapping[str, RpcOutcome] | None = None) -> None:
        super().__init__(outcomes)
        self.gate = asyncio.Event()
        self._hold_next = True

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome:
        self.calls.append((method, dict(params or {})))
        if self._hold_next and method != CATALOG_METHOD:
            self._hold_next = False
            await self.gate.wait()
        outcome = self._outcomes.get(method)
        if outcome is not None:
            return outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})


class MethodHoldingDispatcher(ScriptedDispatcher):
    """Parks calls for one named method until :attr:`gate` is set; every
    other method answers immediately from the outcome table.

    Unlike :class:`HoldingScriptedDispatcher`, which holds whichever call
    happens to be first, this holds by method name — needed for a race that
    has to start *after* an earlier call (``session.list``, here) has
    already completed and returned control to the test.
    """

    def __init__(
        self, outcomes: Mapping[str, RpcOutcome] | None = None, *, hold_method: str
    ) -> None:
        super().__init__(outcomes)
        self.gate = asyncio.Event()
        self._hold_method = hold_method

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome:
        self.calls.append((method, dict(params or {})))
        if method == self._hold_method:
            await self.gate.wait()
        outcome = self._outcomes.get(method)
        if outcome is not None:
            return outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})


class SwitchRaceDispatcher(ScriptedDispatcher):
    """``session.resume`` answers per requested ``session_id``, from a table
    keyed by target rather than by method — the two switches in a
    B-then-C race dispatch the SAME method with different params, which a
    plain method-keyed table cannot tell apart. One target is held until
    the test releases :attr:`gate`; every other call, including a resume for
    a different target, answers immediately.
    """

    def __init__(
        self,
        outcomes: Mapping[str, RpcOutcome] | None = None,
        *,
        resume_outcomes: Mapping[str, RpcOutcome],
        hold_session_id: str,
    ) -> None:
        super().__init__(outcomes)
        self.gate = asyncio.Event()
        self._resume_outcomes = resume_outcomes
        self._hold_session_id = hold_session_id

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
            if target == self._hold_session_id:
                await self.gate.wait()
            return self._resume_outcomes[target]
        outcome = self._outcomes.get(method)
        if outcome is not None:
            return outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})


def dialog(app: TalariaApp) -> PickerDialog:
    screen = app.screen
    assert isinstance(screen, PickerDialog), f"expected the picker, got {type(screen).__name__}"
    return screen


def no_dialog(app: TalariaApp) -> bool:
    return not isinstance(app.screen, PickerDialog)


async def open_sessions(app: TalariaApp, pilot: Any) -> None:
    app.composer.text_area.focus()
    app.composer.text = "/sessions"
    await pilot.press("enter")
    await app.settle_live()
    await pilot.pause()


# ── opening the picker ──────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_a_non_finite_started_at_does_not_crash_the_dialog_open() -> None:
    """P2, U7 round two. ``session.list`` rows reach
    ``datetime.fromtimestamp`` by way of ``format_session_detail`` while the
    dialog is being built — a NaN, infinite, or absurdly large
    ``started_at`` used to raise there before any row was ever shown.
    """
    dispatcher = ScriptedDispatcher(
        {
            LIST_SESSIONS_METHOD: list_outcome(
                session_row("s1", title="current"),
                session_row("s2", title="malformed timestamp", started_at=1e300),
            )
        }
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)

        assert not no_dialog(app)
        assert any("malformed timestamp" in row for row in dialog(app).row_texts)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_sessions_opens_the_dialog_listing_the_stub_with_the_focused_one_marked() -> None:
    """P1, U7 round two: ``focused_session_id`` (``"s1-runtime"``) and
    ``session_key`` (``"s1"``) are deliberately DISTINCT. The row marker
    compares a listing row's id against :attr:`SessionState.session_key`,
    never against the runtime id — with the two equal, this test could not
    tell a marker keyed on the wrong field from one keyed correctly, which
    is exactly how the durable-identity defects this round fixes went
    unnoticed."""
    dispatcher = ScriptedDispatcher(
        {
            LIST_SESSIONS_METHOD: list_outcome(
                session_row("s1", title="the one already open"),
                session_row("s2", title="a different conversation"),
            )
        }
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1-runtime", session_key="s1")
        await open_sessions(app, pilot)

        rows = dialog(app).row_texts
        assert any(
            "the one already open" in row and row.startswith("*") for row in rows
        )
        assert any(
            "a different conversation" in row and not row.startswith("*") for row in rows
        )
        # Two calls, not one: v0.4's picker reads the registry, and the
        # registry is fed by the historical listing *and* the live roster.
        # ``session.list`` alone carries no lifecycle field at all, so a
        # one-call picker could only ever show identity.
        assert [method for method, _ in dispatcher.operator_calls] == [
            LIST_SESSIONS_METHOD,
            LIST_ACTIVE_METHOD,
        ]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_escape_closes_without_a_wire_call_and_returns_the_caret() -> None:
    dispatcher = ScriptedDispatcher(
        {
            LIST_SESSIONS_METHOD: list_outcome(
                session_row("s1", title="one"), session_row("s2", title="two")
            )
        }
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)
        assert not no_dialog(app)

        await pilot.press("escape")
        await pilot.pause()

        assert no_dialog(app)
        assert app.focused is app.composer.text_area
        # Backing out sent nothing beyond the two reads that opened it.
        assert [method for method, _ in dispatcher.operator_calls] == [
            LIST_SESSIONS_METHOD,
            LIST_ACTIVE_METHOD,
        ]
        await app.shutdown_sources()


# ── choosing a row switches through the landing path (KTD3) ──────────────


@pytest.mark.asyncio
async def test_choosing_a_row_dispatches_session_resume_and_lands_it() -> None:
    """Arrows and enter, end to end: the history renders (U6's second caller)
    and the session left behind's prompts are gone (U5's first UI caller)."""
    dispatcher = ScriptedDispatcher(
        {
            LIST_SESSIONS_METHOD: list_outcome(
                session_row("s1", title="current"), session_row("s2", title="other")
            ),
            RESUME_METHOD: resume_outcome(
                "s2",
                messages=[
                    {
                        "role": "user",
                        "text": "hello from the other session — canary-8fQ2",
                        "row_id": "r1",
                    }
                ],
                message_count=1,
            ),
        }
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        feed(app, approval_frame("rm -rf /data-canary-1c9x", "a dangerous command"))
        await settle(app, pilot)
        assert app.state.prompts != ()
        assert "a dangerous command" in screen_text(app)

        await open_sessions(app, pilot)
        # The marked row (s1, current) is highlighted first; one step down
        # reaches s2.
        assert "current" in dialog(app).active_row_text
        await pilot.press("down")
        await pilot.pause()
        assert "other" in dialog(app).active_row_text
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        resume_call = next(params for method, params in dispatcher.calls if method == RESUME_METHOD)
        assert resume_call["session_id"] == "s2"

        assert app.state.focused_session_id == "s2"
        assert app.state.session_key == "s2"
        # U5 through its first UI caller (CR3 finding 1's fix): the gateway
        # never re-announces an outstanding prompt across a switch, so
        # ``focus_session`` now RETAINS it in the registry — dropping it
        # would leave it unanswerable forever if the operator switches back
        # to s1. It stays in ``state.prompts``, answerable again on return;
        # ``prompt_view`` filters *rendering* to the focused session, which
        # is what the screen assertion below actually checks.
        assert [p.session_id for p in app.state.prompts] == ["s1"]

        await app.render_snapshot()
        await pilot.pause()
        screen = screen_text(app)
        assert "hello from the other session — canary-8fQ2" in screen
        assert "a dangerous command" not in screen
        assert no_dialog(app)
        assert app.focused is app.composer.text_area
        # B3 (AE5): landing a *different* session is the real-switch branch —
        # B5's durable row announces it there, and no "already the focused
        # session" notice belongs on a landing that changed focus.
        assert SESSION_ALREADY_FOCUSED not in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_enter_on_the_marked_current_row_is_refused_in_the_dialog() -> None:
    """B3 (AE6): the marked row already refuses itself in the dialog — enter
    on it shows ``cannot select — already the focused session``, the same
    fact B3's composer notice states for the unmarked landing (KTD4). One
    fact, one voice on both surfaces, asserted on the surface that shipped
    it (v0.2) rather than rebuilt here."""
    dispatcher = ScriptedDispatcher(
        {
            LIST_SESSIONS_METHOD: list_outcome(
                session_row("s1", title="current"), session_row("s2", title="other")
            ),
        }
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)
        # The marked current row is the initially active row.
        assert "current" in dialog(app).active_row_text

        await pilot.press("enter")
        await pilot.pause()

        # Still in the dialog, and the refusal names the fact the composer
        # uses for the unmarked landing.
        assert not no_dialog(app)
        assert f"{REFUSED_PREFIX}{SESSION_ALREADY_FOCUSED}" in dialog(app).refusal_text
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_switch_chosen_while_one_is_already_in_flight_is_refused() -> None:
    """P1, U7 round two; C1, U7 round three — the reviewer's live-reproduced
    interleaving, and the fix that replaced the original one.

    Choosing B, reopening ``/sessions`` before B's ``session.resume`` reply
    lands, and choosing C used to dispatch two resumes with no ordering
    guarantee on the gateway's side either — a client-side generation
    counter could discard whichever reply arrived out of order, but nothing
    stopped the *second send*, so the gateway's own active session and
    Talaria's belief could still diverge underneath a client showing no
    error. C1 refuses the second send outright instead: choosing C while
    B's resume is still on the wire is refused before it ever reaches the
    dispatcher, and only one ``session.resume`` is ever in flight. B's own
    reply, once released, lands normally — it was never superseded, because
    nothing else was ever actually sent.
    """
    dispatcher = SwitchRaceDispatcher(
        resume_outcomes={
            "s-b": resume_outcome(
                "s-b", messages=[{"role": "user", "text": "in b"}], message_count=1
            ),
            "s-c": resume_outcome(
                "s-c", messages=[{"role": "user", "text": "in c"}], message_count=1
            ),
        },
        hold_session_id="s-b",
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await settle(app, pilot)

        # Choose B — its resume is held on the wire.
        switch_to_b = asyncio.create_task(app.switch_session("s-b"))
        while not any(method == RESUME_METHOD for method, _ in dispatcher.calls):
            await asyncio.sleep(0)

        # Reopen and choose C before B's reply lands — refused outright,
        # never dispatched, and B's own switch is still the only one in
        # flight.
        result = await app.switch_session("s-c")
        await settle(app, pilot)
        assert result is None
        assert SWITCH_ALREADY_IN_FLIGHT in app.composer.notice
        assert [method for method, _ in dispatcher.calls].count(RESUME_METHOD) == 1
        assert app.state.focused_session_id == "s1", "C must never have been sent at all"

        # B's reply lands, unopposed.
        dispatcher.gate.set()
        await switch_to_b
        await settle(app, pilot)

        assert app.state.focused_session_id == "s-b", (
            "the only switch ever actually sent must land"
        )

        # The guard releases once B lands — a later switch is not stuck
        # refused forever.
        await app.switch_session("s-c")
        await settle(app, pilot)
        assert app.state.focused_session_id == "s-c"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_reconnect_between_scheduling_and_running_the_switch_is_still_caught() -> None:
    """C2 (MEDIUM, app.py:3247): the epoch was validated only in the closure
    that SCHEDULES the switch (``_sessions_dismissed``'s ``dismissed``), at
    dismissal time — but that closure only hands the switch to
    :meth:`~talaria.ui.app.TalariaApp._spawn_live`, which schedules it via
    ``asyncio.create_task`` rather than running it immediately. A reconnect
    landing in the window between scheduling and the task's body actually
    executing went uncaught, and would have dispatched ``session.resume`` on
    a connection the listing was never fetched under. Modelled directly
    against :meth:`~talaria.ui.app.TalariaApp._switch_session_and_discard`,
    the coroutine the scheduled task actually runs, since the race is about
    what runs *inside* it, not about how it gets scheduled.
    """
    dispatcher = ScriptedDispatcher(
        {RESUME_METHOD: resume_outcome("s2", messages=[], message_count=0)}
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await settle(app, pilot)

        stale_epoch = app._connection_epoch
        # A reconnect happens in the window between the closure scheduling
        # the switch and the scheduled task actually running.
        app._connection_epoch += 1

        await app._switch_session_and_discard("s2", stale_epoch)
        await settle(app, pilot)

        assert RESUME_METHOD not in [method for method, _ in dispatcher.calls]
        assert SESSIONS_STALE_EPOCH in app.composer.notice
        assert app.state.focused_session_id == "s1"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_landing_the_already_focused_session_does_not_reseed_history() -> None:
    """CR6 finding 1 — ``land_session``'s retain branch (same session id)
    keeps the transcript on screen exactly as it is; ``_land_session`` used to
    call ``seed_history`` unconditionally regardless of which branch ran, so
    landing a session that is already focused re-appended its whole resumed
    history a second time. Reachable the moment the picker lets the operator
    choose the row that is already focused — modelled here through
    :meth:`~talaria.ui.app.TalariaApp.switch_session`, the same call the
    picker's own dismissal handler makes (KTD3), landing session ``s1``
    twice in a row.
    """
    dispatcher = ScriptedDispatcher(
        {
            RESUME_METHOD: resume_outcome(
                "s1",
                messages=[
                    {
                        "role": "user",
                        "text": "already present — canary-4mQ7",
                        "row_id": "r1",
                    }
                ],
                message_count=1,
            ),
        }
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        await app.switch_session("s1")
        await pilot.pause()
        assert app.state.focused_session_id == "s1"
        seeded_once = [entry.text for entry in app.state.transcript if "canary-4mQ7" in entry.text]
        assert len(seeded_once) == 1

        # Landing the session that is already focused — the retain branch.
        await app.switch_session("s1")
        await pilot.pause()

        seeded_again = [entry.text for entry in app.state.transcript if "canary-4mQ7" in entry.text]
        assert len(seeded_again) == 1, "landing the focused session twice re-seeded its history"
        # B3 (AE5): the landing changed nothing, so the keypress says so on
        # the composer surface — carrying the picker's own
        # ``SESSION_ALREADY_FOCUSED`` wording (KTD4), one fact one voice.
        assert SESSION_ALREADY_FOCUSED in app.composer.notice
        await app.shutdown_sources()


# ── the two refusals ───────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_sessions_refuses_while_an_answer_is_in_flight_and_sends_nothing() -> None:
    """U5's refusal, surfaced by U7 for the first UI caller that can reach it.

    ``switch_refusal`` fires *before* ``session.list`` is even dispatched —
    the notice names the same reason a refused switch would, and the wire
    carries nothing new at all.
    """
    dispatcher = HoldingScriptedDispatcher(
        {LIST_SESSIONS_METHOD: list_outcome(session_row("s1", title="current"))}
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        feed(app, approval_frame("rm -rf /data-canary-2d8y", "a dangerous command"))
        await settle(app, pilot)

        first = asyncio.create_task(app.respond_live("approval:s1#1", "once"))
        while not dispatcher.operator_calls:
            await asyncio.sleep(0)
        # The answer is on the wire and the prompt is out of the registry —
        # exactly the window ``switch_refusal`` exists to close.
        assert app.state.answering != ()
        before = len(dispatcher.calls)

        await open_sessions(app, pilot)

        assert no_dialog(app)
        assert len(dispatcher.calls) == before, "session.list must not have been sent"
        assert REFUSED_SWITCH_WHILE_ANSWERING in app.composer.notice

        dispatcher.gate.set()
        await first
        await settle(app, pilot)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_late_answer_after_the_dialog_opens_still_refuses_the_selection() -> None:
    """P1, U7 round two. The only pre-dispatch guard used to run once, before
    ``session.list`` — an answer that starts travelling AFTER the listing is
    back and the dialog is already open reached ``session.resume``'s own
    dispatch unrefused, and the RPC went out regardless of what its reply
    would do with it. Rechecked immediately before that dispatch instead.
    """
    dispatcher = MethodHoldingDispatcher(
        {
            LIST_SESSIONS_METHOD: list_outcome(
                session_row("s1", title="current"), session_row("s2", title="other")
            )
        },
        hold_method="approval.respond",
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        feed(app, approval_frame("rm -rf /data-canary-race", "a dangerous command"))
        await settle(app, pilot)

        await open_sessions(app, pilot)
        assert not no_dialog(app)

        # The answer starts travelling only now — after the listing has come
        # back and the dialog is already open, which the earlier pre-listing
        # guard cannot see.
        answer = asyncio.create_task(app.respond_live("approval:s1#1", "once"))
        while app.state.answering == ():
            await asyncio.sleep(0)

        await pilot.press("down")
        await pilot.pause()
        assert "other" in dialog(app).active_row_text
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert RESUME_METHOD not in [method for method, _ in dispatcher.operator_calls]
        assert REFUSED_SWITCH_WHILE_ANSWERING in app.composer.notice
        assert app.state.focused_session_id == "s1"

        dispatcher.gate.set()
        await answer
        await settle(app, pilot)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_reconnect_while_the_dialog_is_open_refuses_the_stale_selection() -> None:
    """P1, U7 round two. The epoch was checked only up to the dialog opening
    (``open_sessions_picker``'s own check, right after the listing lands) —
    a reconnect while the dialog sat open afterward went unnoticed, and
    choosing a row from that now-stale listing sent ``session.resume`` on
    the NEW connection. The listing epoch is now carried into the dismiss
    callback and compared again immediately before the dispatch.
    """
    dispatcher = ScriptedDispatcher(
        {
            LIST_SESSIONS_METHOD: list_outcome(
                session_row("s1", title="current"), session_row("s2", title="other")
            )
        }
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)
        assert not no_dialog(app)

        # A reconnect while the dialog sits open: the epoch moves on with no
        # fresh listing fetched for it.
        app._connection_epoch += 1

        await pilot.press("down")
        await pilot.pause()
        assert "other" in dialog(app).active_row_text
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert RESUME_METHOD not in [method for method, _ in dispatcher.operator_calls]
        assert SESSIONS_STALE_EPOCH in app.composer.notice
        assert app.state.focused_session_id == "s1"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_sessions_while_disconnected_refuses_with_a_notice_not_a_crash() -> None:
    controls = ReplayControls(paused=True)
    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    app = TalariaApp(
        source, mode="live", controls=controls, dispatcher=None, coalesce_interval=3600.0
    )

    async with app.run_test() as pilot:
        await open_sessions(app, pilot)

        assert no_dialog(app)
        assert app.composer.notice == SESSIONS_UNAVAILABLE
        await app.shutdown_sources()


# ── the other honest refusals (fetch failed, empty listing) ──────────────


@pytest.mark.asyncio
async def test_a_refused_listing_names_the_gateways_own_reason() -> None:
    dispatcher = ScriptedDispatcher({LIST_SESSIONS_METHOD: failed_list_outcome("no such method")})
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        await open_sessions(app, pilot)

        assert no_dialog(app)
        assert SESSIONS_LIST_FAILED in app.composer.notice
        assert "no such method" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_empty_listing_is_a_claim_the_gateway_made_not_a_blank_dialog() -> None:
    dispatcher = ScriptedDispatcher({LIST_SESSIONS_METHOD: list_outcome()})
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        await open_sessions(app, pilot)

        assert no_dialog(app)
        assert app.composer.notice == NO_SESSIONS
        await app.shutdown_sources()


# ── v0.4 OP2: the confirmation before a live session is taken (KTD8) ─────


def active_row(
    session_id: str,
    *,
    session_key: str = "",
    status: str = "idle",
    title: str = "",
) -> dict[str, Any]:
    """One ``session.active_list`` row, in the shape U1 verified on the wire."""
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


def roster_outcome(*rows: Mapping[str, Any]) -> RpcOutcome:
    return RpcOutcome(
        status="ok",
        method=LIST_ACTIVE_METHOD,
        request_id="1",
        epoch=1,
        result={"sessions": list(rows)},
    )


def confirm(app: TalariaApp) -> ConfirmDialog:
    screen = app.screen
    assert isinstance(screen, ConfirmDialog), (
        f"expected the steal confirmation, got {type(screen).__name__}"
    )
    return screen


def _steal_dispatcher(
    *,
    live_waiting: bool = True,
    resume: RpcOutcome | None = None,
) -> ScriptedDispatcher:
    """A gateway whose roster reports ``s2`` live and ``s3`` only historical."""
    return ScriptedDispatcher(
        {
            LIST_SESSIONS_METHOD: list_outcome(
                session_row("s1", title="current"),
                session_row("s2", title="somebody else is here"),
                session_row("s3", title="yesterday's session"),
            ),
            LIST_ACTIVE_METHOD: roster_outcome(
                active_row("s1", title="current"),
                active_row(
                    "s2",
                    status="waiting" if live_waiting else "working",
                    title="somebody else is here",
                ),
            ),
            RESUME_METHOD: resume or resume_outcome("s2"),
        }
    )


async def choose_row(app: TalariaApp, pilot: Any, needle: str) -> None:
    """Walk the open picker to the row whose text contains ``needle``, and
    press enter on it."""
    for _ in range(len(dialog(app).row_texts)):
        if needle in dialog(app).active_row_text:
            break
        await pilot.press("down")
        await pilot.pause()
    assert needle in dialog(app).active_row_text, dialog(app).row_texts
    await pilot.press("enter")
    await app.settle_live()
    await pilot.pause()


@pytest.mark.asyncio
async def test_taking_a_live_session_asks_first_and_names_the_consequence() -> None:
    """OP2. Rows carry no transport-identity field, so a detached orphan and a
    session another client is actively driving are indistinguishable from
    here — the copy states both possibilities rather than asserting the more
    decisive-sounding one. Nothing reaches the wire while the dialog is up."""
    dispatcher = _steal_dispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)
        await choose_row(app, pilot, "somebody else is here")

        dialog_screen = confirm(app)
        assert "somebody else is here" in dialog_screen.title_text
        assert any("detaches that client" in line for line in dialog_screen.body_texts)
        # The waiting kind is the flattened one the poll reports, so the copy
        # also says the prompt may not survive the attach (KTD8's unobserved
        # clause).
        assert any(
            ATTACH_CONFIRM_UNKNOWN_KIND == line for line in dialog_screen.body_texts
        )
        assert RESUME_METHOD not in [method for method, _ in dispatcher.operator_calls]
        assert app.state.focused_session_id == "s1"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_declining_the_steal_sends_nothing_and_moves_no_focus() -> None:
    """Fire-and-observe on the decline as much as on the confirm: no optimistic
    focus move, no row marked, nothing dispatched — just the sentence saying
    so."""
    dispatcher = _steal_dispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)
        await choose_row(app, pilot, "somebody else is here")
        assert isinstance(app.screen, ConfirmDialog)

        await pilot.press("escape")
        await app.settle_live()
        await pilot.pause()

        assert not isinstance(app.screen, ConfirmDialog)
        assert app.composer.notice == ATTACH_DECLINED
        assert RESUME_METHOD not in [method for method, _ in dispatcher.operator_calls]
        assert app.state.focused_session_id == "s1"
        assert app.focused is app.composer.text_area
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_enter_on_the_opening_row_cancels_rather_than_steals() -> None:
    """The highlight opens on cancel, so the reflex keypress is the safe one.
    Reaching the destructive row takes a deliberate move first."""
    dispatcher = _steal_dispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)
        await choose_row(app, pilot, "somebody else is here")
        assert "cancel" in confirm(app).active_row_text

        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert app.composer.notice == ATTACH_DECLINED
        assert RESUME_METHOD not in [method for method, _ in dispatcher.operator_calls]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_confirming_the_steal_resumes_the_session() -> None:
    dispatcher = _steal_dispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)
        await choose_row(app, pilot, "somebody else is here")

        await pilot.press("down")
        await pilot.pause()
        assert "detach" in confirm(app).active_row_text
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        resume_call = next(
            params for method, params in dispatcher.calls if method == RESUME_METHOD
        )
        assert resume_call["session_id"] == "s2"
        assert app.state.focused_session_id == "s2"
        # The session is now one this run drives, so a later switch back to it
        # is not a steal and asks nothing.
        assert app.fleet.rows[(app.fleet_profile, "s2")].ownership == "we_drive"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_historical_session_resumes_with_no_dialog_at_all() -> None:
    """KTD8's exemption: nothing live is stolen, and a dialog on every resume
    would train the operator to click through the one that matters."""
    dispatcher = _steal_dispatcher(resume=resume_outcome("s3"))
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)
        await choose_row(app, pilot, "yesterday's session")

        assert not isinstance(app.screen, ConfirmDialog)
        resume_call = next(
            params for method, params in dispatcher.calls if method == RESUME_METHOD
        )
        assert resume_call["session_id"] == "s3"
        await app.shutdown_sources()


# ── v0.4 KTD8: what a confirmed attach recovers, and what it cannot ──────


@pytest.mark.asyncio
async def test_a_confirmed_attach_renders_the_cards_its_reply_hydrated() -> None:
    """The scripted twin of U1's live hydration leg.

    The shared live-session payload builder behind create, resume and activate
    carries ``pending_approval`` and ``pending_clarify`` when they exist at the
    pinned revision. Both become ordinary request events, so both arrive as
    ordinary cards — this asserts the cards on screen, not a field on a reply.
    """
    hydrating = resume_outcome("s2")
    hydrating = replace(
        hydrating,
        result={
            **(hydrating.result or {}),
            "pending_approval": {
                "request_id": "a-1",
                "command": "rm -rf /data-canary-hydrate",
                "description": "a hydrated dangerous command",
                "choices": ["once", "deny"],
            },
            "pending_clarify": {
                "request_id": "c-1",
                "question": "which branch should I use — canary-clarify?",
                "choices": ["main", "release"],
            },
        },
    )
    dispatcher = _steal_dispatcher(resume=hydrating)
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)
        await choose_row(app, pilot, "somebody else is here")
        await pilot.press("down")
        await pilot.pause()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()
        await settle(app, pilot)

        assert sorted(prompt.kind for prompt in app.state.prompts) == [
            "approval",
            "clarify",
        ]
        screen = screen_text(app)
        assert "a hydrated dangerous command" in screen
        assert "which branch should I use — canary-clarify?" in screen
        # Nothing was invented beyond what the reply carried.
        assert app.fleet.rows[(app.fleet_profile, "s2")].last_notice == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_attach_that_hydrates_nothing_fails_visibly_on_the_row() -> None:
    """Day-one behaviour, not an edge case: the gateway revision serving this
    machine has no hydration fields in its payload builder at all (U1), so a
    confirmed attach on a waiting session recovers no card. The wait settles as
    resolved-failed — visible on the registry row and in the transcript —
    rather than leaving a control the operator can never answer or inventing
    one that was never announced to them.
    """
    dispatcher = _steal_dispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)
        await choose_row(app, pilot, "somebody else is here")
        await pilot.press("down")
        await pilot.pause()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()
        await settle(app, pilot)

        assert app.state.prompts == (), "no card may be invented for a lost prompt"
        row = app.fleet.rows[(app.fleet_profile, "s2")]
        assert ATTACH_RESIDUE_PREFIX in row.last_notice
        assert "resolved-failed" in row.last_notice
        assert row.waiting_kind == ""
        # The same sentence reaches the operator twice: the notice bar now, and
        # the transcript, which outlives the next notice.
        assert ATTACH_RESIDUE_PREFIX in app.composer.notice
        assert any(
            ATTACH_RESIDUE_PREFIX in entry.text for entry in app.state.transcript
        )

        # And it is still on the row the next time the picker is opened, which
        # is where an operator looking at the fleet would find it.
        await open_sessions(app, pilot)
        assert any(
            ATTACH_RESIDUE_PREFIX in row_text for row_text in dialog(app).row_texts
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_reconnect_while_the_confirmation_is_open_refuses_the_steal() -> None:
    """CR4's blocking finding: the OP2 dialog reopened a window that was closed.

    Every step from opening the picker to dispatching ``session.resume`` compared
    the connection generation — at the listing fetch, at dialog dismissal, and
    once more inside the scheduled task. U4 inserted the steal confirmation
    between the last comparison and the dispatch, and the confirmation's callback
    carried no generation at all. That made the window human-scale: exactly as
    long as an operator takes to read a warning about detaching somebody.

    On the profile-switch flavour of a reconnect the consequence is worse than a
    stale id. The decision was made against one gateway's rows, and the resume
    would go to whichever gateway is current when the operator presses enter.
    """
    dispatcher = _steal_dispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = replace(app.state, focused_session_id="s1", session_key="s1")
        await open_sessions(app, pilot)
        await choose_row(app, pilot, "somebody else is here")

        await pilot.press("down")
        await pilot.pause()
        assert "detach" in confirm(app).active_row_text

        # The connection is replaced while the operator is reading the warning.
        # This is literally what note_connection("connected") does.
        app._connection_epoch += 1  # noqa: SLF001 - the generation is the mechanism

        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        # Nothing was sent, the focus did not move, and the refusal says why.
        assert not any(method == RESUME_METHOD for method, _ in dispatcher.calls)
        assert app.state.focused_session_id == "s1"
        assert SESSIONS_STALE_EPOCH in app.composer.notice
        await app.shutdown_sources()
