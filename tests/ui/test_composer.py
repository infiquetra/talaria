"""AE4's composer sweep, plus R10/R12 (KTD4).

The interesting assertions here are the negative ones. It is easy to build a
composer that accepts a paste and easy to build one that submits on Enter; what
the gate has to prove is that a 400-line paste does *not* submit, that Enter in
replay mode does *not* echo, and that a wide or combining character survives a
round trip *unchanged* rather than being normalized into something that looks
close enough.
"""

from __future__ import annotations

import pytest
from textual.events import Paste

from talaria.replay.controls import INERT_NOTICE
from talaria.ui.app import PASTE_NOT_COLLAPSED
from talaria.ui.composer import PLACEHOLDER, Composer
from tests.ui.conftest import event, paused_app, streaming_turn


@pytest.mark.asyncio
async def test_enter_submits_and_ctrl_j_inserts_a_newline() -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.press("h", "i")
        await pilot.press("ctrl+j")
        await pilot.press("t", "h", "e", "r", "e")
        await pilot.pause()
        assert app.composer.text == "hi\nthere"

        await pilot.press("enter")
        await pilot.pause()
        # Enter did not insert: the text is unchanged, not "hi\nthere\n".
        assert app.composer.text == "hi\nthere"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_multi_line_entry_and_the_bindings_are_discoverable() -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test() as pilot:
        text_area = app.composer.text_area
        assert text_area.placeholder == PLACEHOLDER
        assert "Enter sends" in PLACEHOLDER and "Ctrl+J newline" in PLACEHOLDER
        # KTD4's exact configuration, asserted rather than assumed.
        assert text_area.language is None
        assert text_area.soft_wrap is True
        assert text_area.show_line_numbers is False

        text_area.focus()
        for _ in range(3):
            await pilot.press("a")
            await pilot.press("ctrl+j")
        await pilot.pause()
        assert app.composer.text.count("\n") == 3
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f7_selects_the_full_focused_composer_document() -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test() as pilot:
        text_area = app.composer.text_area
        app.composer.text = "first line\nsecond line"
        text_area.focus()

        await pilot.press("f7")
        await pilot.pause()

        assert text_area.selected_text == "first line\nsecond line"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f6_and_f7_reach_both_pickers_when_focus_is_elsewhere(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    reached: list[str] = []

    async def record_picker(mode: str) -> None:
        reached.append(mode)

    monkeypatch.setattr(app, "open_picker", record_picker)
    async with app.run_test() as pilot:
        app.transcript.focus()
        await pilot.press("f6")
        await pilot.press("f7")
        await pilot.pause()

        assert reached == ["models", "profiles"]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_several_hundred_line_paste_inserts_without_submitting() -> None:
    """R11/AE4: bracketed paste arrives as one event and must not submit."""
    pasted = "\n".join(f"pasted line {index}" for index in range(400))
    app, controls = paused_app([event("gateway.ready", {})])
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        # Posted to the *app*, which is where a real bracketed paste is
        # delivered — the terminal driver hands it to the App and Textual
        # forwards it to the focused widget. Posting straight to the TextArea
        # inserts twice in Textual 8.2.8, because the widget handles it and the
        # event then bubbles to the App, which forwards it back down.
        app.post_message(Paste(pasted))
        await pilot.pause()
        await pilot.pause()

        assert app.composer.text.count("\n") == 399
        assert app.composer.text.endswith("pasted line 399")
        # The literal insert is exact, not merely long: a paste inserted twice
        # is also "several hundred lines" and is a real defect Textual's
        # MRO-wide handler dispatch produced here (see ``ChatTextArea._on_paste``).
        assert app.composer.text == pasted
        # Nothing was submitted. What *was* reached is the collapse, which needs
        # a gateway — so replay refuses it out loud (AE11) and the full text
        # stays exactly where KTD4 put it. ``submit`` is absent from this list
        # and that is the assertion: Enter was never pressed.
        assert [outcome.name for outcome in controls.refusals] == ["paste-collapse"]
        assert INERT_NOTICE in app.composer.notice
        # The preservation clause leads, as it does on the live path. The notice
        # is one row and routinely narrower than the sentence; at 60 columns the
        # refusal-first ordering pushed "the paste was left in full" — the half
        # the operator needs — past the end of the row with nothing to mark it.
        assert app.composer.notice.startswith(PASTE_NOT_COLLAPSED)
        await app.shutdown_sources()


@pytest.mark.parametrize(
    "sample",
    [
        "端末エミュレータ",  # wide (East Asian) characters
        "été",  # combining acute accents
        "עברית",  # right-to-left
        "🜁🜂🜃🜄",  # astral-plane symbols
        "mixed 端末 é עברית 🜁",
    ],
)
@pytest.mark.asyncio
async def test_wide_and_combining_characters_survive_the_round_trip(sample: str) -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        app.post_message(Paste(sample))
        await pilot.pause()
        await pilot.pause()
        # Byte-for-byte, not "looks the same": a composer that NFC-normalizes a
        # combining sequence changes what the operator typed.
        assert app.composer.text == sample
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_border_is_present_while_the_transcript_streams() -> None:
    """R10: the bordered region is visible during streaming, not only at rest."""
    frames = [event("gateway.ready", {})]
    frames.extend(streaming_turn([f"token {index} " for index in range(30)]))
    app, controls = paused_app(frames)
    async with app.run_test() as pilot:
        controls.resume()
        await pilot.pause()
        composer = app.query_one(Composer)
        styles = composer.styles
        assert styles.border_top[0] not in ("", "none", "hidden")
        assert composer.border_title == "compose [*] caret here"
        assert composer.display is True
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_enter_in_replay_submits_nothing_echoes_nothing_and_keeps_the_text() -> None:
    """The inert-control discipline applied to the composer (AE11)."""
    app, controls = paused_app([event("gateway.ready", {})])
    async with app.run_test() as pilot:
        before = len(app.state.transcript)
        app.composer.text = "please send this"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()

        assert app.composer.text == "please send this", "the composed text was lost"
        assert len(app.state.transcript) == before, "replay echoed a message locally"
        assert [outcome.name for outcome in controls.refusals] == ["submit"]
        assert INERT_NOTICE in app.composer.notice
        await app.shutdown_sources()
