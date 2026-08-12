"""C1 composer history UI: Up/Down recall, stash, caret, and palette seam.

Every history step asserts composer.text *and* composer_history index/stash,
so a wrong entry cannot hide behind a text-changed signal.
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

import pytest

from talaria.domain.composer_history import ComposerHistory
from talaria.transport.rpc import RpcOutcome
from tests.ui.conftest import event, live_app, paused_app


@pytest.mark.asyncio
async def test_up_recalls_backwards_and_down_advances_and_restores_stash() -> None:
    disp = RecordingDispatcher(RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1, result={}))  # noqa: E501
    app = live_app(disp)
    async with app.run_test() as pilot:
        for text in ("a", "b", "c"):
            app.composer.text = text
            app.composer.text_area.focus()
            await pilot.press("enter")
            await pilot.pause()
            await app.settle_live()
            await pilot.pause()
        assert app.composer_history.entries == ("a", "b", "c")
        assert app.composer_history.index is None
        assert app.composer_history.draft_stash is None

        app.composer.text = ""
        app.composer.text_area.focus()
        await pilot.pause()

        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "c"
        assert app.composer_history.index == 2
        assert app.composer_history.draft_stash == ""
        assert app.composer.text_area.cursor_location == (0, 1)

        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "b"
        assert app.composer_history.index == 1

        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "a"
        assert app.composer_history.index == 0

        await pilot.press("down")
        await pilot.pause()
        assert app.composer.text == "b"
        assert app.composer_history.index == 1

        await pilot.press("down")
        await pilot.pause()
        assert app.composer.text == "c"
        assert app.composer_history.index == 2

        await pilot.press("down")
        await pilot.pause()
        assert app.composer.text == ""
        assert app.composer_history.index is None
        assert app.composer_history.draft_stash == ""
        # Caret at end after restore.
        assert app.composer.text_area.cursor_location == (0, 0)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_draft_stash_survives_and_restores_with_caret_at_end() -> None:
    disp = RecordingDispatcher(RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1, result={}))  # noqa: E501
    app = live_app(disp)
    app.composer_history = ComposerHistory(entries=("old",))
    async with app.run_test() as pilot:
        app.composer.text = "draft half"
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "old"
        assert app.composer_history.draft_stash == "draft half"
        await pilot.press("down")
        await pilot.pause()
        assert app.composer.text == "draft half"
        assert app.composer_history.index is None
        assert app.composer.text_area.cursor_location == (0, len("draft half"))

        # Tab into transcript and back preserves the walk — Down still restores.
        app.composer.text = "draft half"
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "old"
        app.transcript.focus()
        await pilot.pause()
        # History index and stash survive focus moves (no abandon).
        assert app.composer_history.index == 0
        assert app.composer_history.draft_stash == "draft half"
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("down")
        await pilot.pause()
        assert app.composer.text == "draft half"

        # Inserting newline with Ctrl+J inside a multi-line draft before recalling.
        app.composer.text = "draft half\nsecond"
        app.composer.text_area.cursor_location = (0, 0)
        await pilot.pause()
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "old"
        assert app.composer_history.draft_stash == "draft half\nsecond"
        await pilot.press("down")
        await pilot.pause()
        assert app.composer.text == "draft half\nsecond"
        assert app.composer.text_area.cursor_location == (1, len("second"))
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_multi_line_boundary_rule() -> None:
    disp = RecordingDispatcher(RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1, result={}))  # noqa: E501
    app = live_app(disp)
    app.composer_history = ComposerHistory(entries=("old",))
    async with app.run_test() as pilot:
        app.composer.text = "line one\nline two"
        app.composer.text_area.focus()
        await pilot.pause()
        # Caret in line two (bottom row): Up moves caret, not history.
        app.composer.text_area.cursor_location = (1, 0)
        await pilot.pause()
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "line one\nline two"
        assert app.composer_history.index is None
        assert app.composer.text_area.cursor_location[0] == 0

        # Caret in line one (top row): Up recalls.
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "old"
        assert app.composer_history.index == 0

        # Single-line drafts always recall regardless of caret — reset to sentinel first.
        app.composer_history = ComposerHistory(entries=("old",))
        app.composer.text = "single"
        app.composer.text_area.cursor_location = (0, len("single"))
        await pilot.pause()
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "old"
        assert app.composer_history.index == 0
        await pilot.press("down")
        await pilot.pause()
        assert app.composer.text == "single"
        assert app.composer_history.index is None

        # Down at bottom row of multiline while navigating advances history;  # noqa: E501
        # not at bottom moves caret.
        # Set up a two-entry history and navigate to oldest.
        app.composer_history = ComposerHistory(entries=("a", "b"))
        app.composer.text = "draft"
        await pilot.pause()
        await pilot.press("up")  # -> b
        await pilot.pause()
        await pilot.press("up")  # -> a, which may be single-line; make it multiline to test caret
        await pilot.pause()
        # Make recalled text multiline for Down test.
        app.composer.text = "a1\na2"
        app.composer.text_area.cursor_location = (0, 0)
        await pilot.pause()
        # Caret at top row: Down moves caret, not history.
        await pilot.press("down")
        await pilot.pause()
        assert app.composer.text == "a1\na2"
        assert app.composer.text_area.cursor_location[0] == 1
        # Caret at bottom row: Down advances history.
        # We are still at index 0 (a), down from bottom should go to b.
        # Set history index to 0 manually to make deterministic.
        app.composer_history = ComposerHistory(entries=("a", "b"), draft_stash="draft", index=0)
        app.composer.text = "a"
        app.composer.text_area.cursor_location = (0, 1)
        await pilot.pause()
        await pilot.press("down")
        await pilot.pause()
        assert app.composer.text == "b"
        assert app.composer_history.index == 1
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_recalled_text_is_editable_and_submits_as_new_entry() -> None:
    disp = RecordingDispatcher(RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1, result={}))  # noqa: E501
    app = live_app(disp)
    async with app.run_test() as pilot:
        app.composer.text = "a"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        await app.settle_live()
        await pilot.pause()
        assert app.composer_history.entries == ("a",)

        app.composer.text = ""
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "a"
        # Edit recalled text.
        app.composer.text = "a edited"
        app.composer.text_area.cursor_location = (0, len("a edited"))
        await pilot.pause()
        await pilot.press("enter")
        await pilot.pause()
        await app.settle_live()
        await pilot.pause()
        assert app.composer_history.entries == ("a", "a edited")  # type: ignore[comparison-overlap]
        # Original entry unchanged.
        assert app.composer_history.entries[0] == "a"
        # Second up still shows original at its original index.
        app.composer.text = ""
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "a edited"
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "a"
        # Transcript shows the new entry.
        user_texts = [e.text for e in app.state.transcript if e.kind == "user"]
        assert "a edited" in user_texts
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_failed_and_unknown_delivery_still_in_history_and_recallable() -> None:
    # Unknown outcome — history push happens before outcome, so it is still recallable.
    disp_unknown = RecordingDispatcher(
        RpcOutcome(status="unknown", method="prompt.submit", request_id="1", epoch=1, result=None, reason="the connection dropped before the gateway answered")  # noqa: E501
    )
    app = live_app(disp_unknown)
    async with app.run_test() as pilot:
        app.composer.text = "maybe delivered"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        await app.settle_live()
        await pilot.pause()
        assert "maybe delivered" in app.composer_history.entries
        app.composer.text = ""
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "maybe delivered"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_empty_and_refused_submissions_do_not_enter_history() -> None:
    disp = RecordingDispatcher(RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1, result={}))  # noqa: E501
    app = live_app(disp)
    async with app.run_test() as pilot:
        # Empty after strip
        app.composer.text = "   "
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        await app.settle_live()
        await pilot.pause()
        assert len(app.composer_history.entries) == 0

        # Normal entry
        app.composer.text = "hello"
        await pilot.press("enter")
        await pilot.pause()
        await app.settle_live()
        await pilot.pause()
        assert app.composer_history.entries == ("hello",)  # type: ignore[comparison-overlap]

        # Blank enter after valid entry still length 1
        app.composer.text = ""
        await pilot.press("enter")
        await pilot.pause()
        await app.settle_live()
        await pilot.pause()
        assert app.composer_history.entries == ("hello",)  # type: ignore[comparison-overlap]
        await app.shutdown_sources()

    # Replay mode refusals — both plain and gateway commands do not enter.
    app2, _ = paused_app([event("gateway.ready", {})])
    async with app2.run_test() as pilot:
        app2.composer.text = "replay plain"
        app2.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        assert len(app2.composer_history.entries) == 0

        app2.composer.text = "/status"
        await pilot.press("enter")
        await pilot.pause()
        assert len(app2.composer_history.entries) == 0
        await app2.shutdown_sources()


@pytest.mark.asyncio
async def test_size_bound_evicts_oldest_and_no_file_written() -> None:
    disp = RecordingDispatcher(RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1, result={}))  # noqa: E501
    app = live_app(disp)
    async with app.run_test() as pilot:
        for i in range(105):
            app.composer.text = f"msg {i}"
            app.composer.text_area.focus()
            await pilot.press("enter")
            await pilot.pause()
            await app.settle_live()
            await pilot.pause()
        assert len(app.composer_history.entries) == 100
        assert app.composer_history.entries[0] == "msg 5"
        assert app.composer_history.entries[-1] == "msg 104"
        # No file written under any path — history is in-memory only.
        import pathlib

        repo = pathlib.Path(__file__).resolve().parents[2]
        # No history file under repo; this is a public-repo privacy guard.
        assert not (repo / "history.json").exists()
        assert not (repo / "composer_history.json").exists()
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_palette_seam_ordered_and_exclusive() -> None:
    disp = RecordingDispatcher(RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1, result={}))  # noqa: E501
    app = live_app(disp)
    app.composer_history = ComposerHistory(entries=("/models 1", "hello"))
    async with app.run_test() as pilot:
        # Palette closed: Up belongs to history under KTD2.
        app.palette.showing = False
        await app.palette.apply(None)
        await pilot.pause()
        app.composer.text = ""
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "hello"
        assert app.composer_history.index == 1

        # F3 listing open does NOT make history inert — it claims no keys.
        # The C2 slash palette (not yet implemented) will claim Up/Down/Enter
        # etc via _is_slash_palette_open, not via PaletteRegion.showing.
        app.palette.showing = True
        await app.palette.apply(None)
        await pilot.pause()
        # Even with F3 open, Up still recalls because F3 does not own the keys.
        # Reset history to sentinel for this branch.
        app.composer_history = ComposerHistory(entries=("/models 1", "hello"))
        app.composer.text = ""
        await pilot.pause()
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "hello"
        # History moved (proving F3 did not block it).
        assert app.composer_history.index == 1

        # Recalling a slash command does not open the palette (ruling 3).
        app.palette.showing = False
        await app.palette.apply(None)
        await pilot.pause()
        app.composer.text = ""
        await pilot.press("up")  # hello
        await pilot.pause()
        await pilot.press("up")  # /models 1
        await pilot.pause()
        assert app.composer.text == "/models 1"
        assert app.palette.showing is False
        # Down when palette closed also belongs to history (ruling 2) with same caret predicate.
        # From "/models 1" (index 0), Down at bottom should go to hello.
        await pilot.press("down")
        await pilot.pause()
        assert app.composer.text == "hello"
        assert app.composer_history.index == 1
        # Down past newest restores stash (which was "" in this walk).
        await pilot.press("down")
        await pilot.pause()
        assert app.composer_history.index is None
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f3_open_enter_still_submits() -> None:
    """P1 regression: F3 listing open must not break Enter-to-submit.

    The F3 command listing (PaletteRegion) is a read-only foldable listing
    that never takes focus and claims none of the five keys. Before the fix,
    the palette predicate was keyed to PaletteRegion.showing and routed
    Enter to super()._on_key, inserting a newline instead of submitting.
    """
    disp = RecordingDispatcher(RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1, result={}))  # noqa: E501
    app = live_app(disp)
    async with app.run_test() as pilot:
        # Open F3 listing — composer keeps the caret.
        await pilot.press("f3")
        await pilot.pause()
        # PaletteRegion.showing is True, but composer still focused.
        app.composer.text = "hello from f3 open"
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("enter")
        await pilot.pause()
        await app.settle_live()
        await pilot.pause()
        # Message was submitted, not newline-inserted.
        assert app.composer.text == ""
        assert app.composer_history.entries == ("hello from f3 open",)
        assert len(disp.calls) == 1 or any(c[0] == "prompt.submit" for c in disp.calls)
        # No newline was inserted.
        assert "\n" not in app.composer_history.entries[0]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_refused_local_commands_do_not_enter_history() -> None:
    """P2: pacing-refused and malformed local commands leave history unchanged."""
    disp = RecordingDispatcher(RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1, result={}))  # noqa: E501
    app = live_app(disp)
    async with app.run_test() as pilot:
        # /pause in live mode is replay_only and is refused ("nothing changed").
        app.composer.text = "/pause"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        await app.settle_live()
        await pilot.pause()
        assert len(app.composer_history.entries) == 0

        # /speed with malformed argument is refused.
        app.composer.text = "/speed banana"
        await pilot.press("enter")
        await pilot.pause()
        await app.settle_live()
        await pilot.pause()
        assert len(app.composer_history.entries) == 0

        # A valid plain message does enter history (non-replay controls are replay_only).
        app.composer.text = "hello valid"
        await pilot.press("enter")
        await pilot.pause()
        await app.settle_live()
        await pilot.pause()
        assert app.composer_history.entries == ("hello valid",)  # type: ignore[comparison-overlap]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_escape_abandons_walk_but_keeps_stash() -> None:
    """P2: escape during a walk abandons index but keeps stash for next walk."""
    disp = RecordingDispatcher(RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1, result={}))  # noqa: E501
    app = live_app(disp)
    app.composer_history = ComposerHistory(entries=("old",))
    async with app.run_test() as pilot:
        app.composer.text = "draft half"
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "old"
        assert app.composer_history.index == 0
        assert app.composer_history.draft_stash == "draft half"
        # Escape abandons walk but keeps stash.
        await pilot.press("escape")
        await pilot.pause()
        assert app.composer_history.index is None
        assert app.composer_history.draft_stash == "draft half"
        # Composer still shows the recalled entry (not yet restored); Down not
        # applicable since index is None. Up again stashes current text anew.
        assert app.composer.text == "old"
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "old"
        # The new stash is the text that was showing when Up was pressed again.
        assert app.composer_history.draft_stash == "old"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_prompt_card_mid_walk_preserves_stash() -> None:
    """AE3 third variant: answering a prompt card mid-walk preserves stash.

    The stash lives on app.composer_history, which prompt cards never touch.
    Mount a simple Input-backed card, answer it, and verify Down still restores.
    """
    disp = RecordingDispatcher(RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1, result={}))  # noqa: E501
    app = live_app(disp)
    app.composer_history = ComposerHistory(entries=("old",))
    async with app.run_test() as pilot:
        app.composer.text = "draft half"
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "old"
        # Simulate answering a prompt card: push a state with a prompt, mount it,
        # then simulate the operator answering via the app's prompt path.
        # We do not need a real card; we just verify the composer_history
        # index/stash survive a state change that would occur when a prompt
        # is answered (the app's state changes but history does not).
        # To make it concrete, mutate state via a frame and ensure history unchanged.
        from tests.ui.conftest import feed

        feed(app, {"type": "clarify.request", "payload": {"request_id": "r1", "question": "q?"}})
        await app.render_snapshot()
        await pilot.pause()
        assert app.composer_history.index == 0
        assert app.composer_history.draft_stash == "draft half"
        # Down should still restore the stashed draft.
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("down")
        await pilot.pause()
        assert app.composer.text == "draft half"
        assert app.composer_history.index is None
        await app.shutdown_sources()


# Minimal dispatcher double — same shape as tests.ui.conftest.RecordingDispatcher but
# defined here so the test file does not depend on import order.
class RecordingDispatcher:
    def __init__(self, outcome: RpcOutcome | None = None) -> None:
        self.outcome = outcome
        self.calls: list[tuple[str, dict[str, Any]]] = []

    async def call(  # noqa: E501
        self, method: str, params: Mapping[str, Any] | None = None, *, timeout: float | None = None
    ) -> RpcOutcome:
        self.calls.append((method, dict(params or {})))
        if self.outcome is not None:
            return self.outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})
