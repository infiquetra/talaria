"""C2 (#141) bar-state rule: wrap protection and theme-driven visibility.

The bar-state rule (#141, item 16) hides the transcript's left offset column
under theme control through :meth:`TranscriptPane.set_show_left_offset` —
the pane reads the active theme's bar field at mount and on every theme
change, through the framework's own change signal.

Two layers of cases:

- The protective wrap cases pin what the seam already promised before the
  wiring landed: at narrow terminal widths wrapped rows reflow completely —
  content is never clipped — and toggling the offset reflows wrapped rows
  rather than merely repainting them. They were written first and passed
  against the pre-wiring seam, so a later change that clips or breaks
  wrapping at these widths fails here before it can be confounded with the
  theme wiring.
- The bar-state cases prove the wiring itself: a theme naming the hidden
  state reclaims the column at startup and through theme switches, at more
  than one terminal width, without clipping or losing content.
"""

from __future__ import annotations

import math
from dataclasses import replace
from pathlib import Path
from typing import Any, cast

import pytest
from rich.cells import cell_len
from textual.app import App, ComposeResult
from textual.content import Content
from textual.widgets._markdown import MarkdownParagraph

from talaria.themes import ThemeSpec
from talaria.themes.builtins import REFINED_DEFAULT
from talaria.themes.storage import load_user_theme_spec, serialize_user_theme
from talaria.ui.blocks import EntryMarkdown
from talaria.ui.theme import (
    BUILTIN_THEME_REGISTRY,
    ThemeRegistry,
    theme_registry_for_config,
    user_theme_path,
)
from talaria.ui.theme_import import import_vscode_theme
from talaria.ui.transcript import (
    TranscriptLine,
    TranscriptPane,
    kind_group_css_class,
)
from tests.ui.conftest import event, paused_app, screen_text, streaming_turn


def _rendered_words(document: EntryMarkdown) -> str:
    """The paragraph text actually on screen — the rendered surface a clip
    would remove words from, where ``document.source`` would still carry
    them."""
    blocks = list(document.query(MarkdownParagraph))
    assert blocks, "the document mounted no paragraphs"
    return " ".join(cast(Content, block.content).plain for block in blocks)

#: The narrow widths these cases exercise. 20 columns is well below any
#: realistic chat width and therefore exactly where a clipped or miscounted
#: wrap first becomes visible; 40 is the narrowest width an operator would
#: plausibly resize to on purpose.
NARROW_WIDTHS: tuple[int, ...] = (20, 24, 40)


class _Harness(App[None]):
    def __init__(self) -> None:
        super().__init__()
        BUILTIN_THEME_REGISTRY.register(self)
        self.theme = REFINED_DEFAULT.slug

    def compose(self) -> ComposeResult:
        yield TranscriptPane(id="t")


@pytest.mark.asyncio
@pytest.mark.parametrize("width", NARROW_WIDTHS)
async def test_a_long_line_wraps_completely_at_narrow_widths(width: int) -> None:
    """A line longer than the pane wraps into exactly the measured number of
    rows at narrow widths, with every character still renderable — clipping
    would show as fewer rows than the cell-width arithmetic, a shortened
    renderable, or both. Unbreakable text makes the row count exact: Rich
    wraps word-aware, so space-separated prose would wrap short of the
    cell arithmetic and confound the measurement."""
    text = "x" * 150
    app = _Harness()
    async with app.run_test(size=(width, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        widget = TranscriptLine(text, kind="assistant", first_in_entry=False)
        await pane.mount(widget)
        await pilot.pause()

        content_width = widget.content_size.width
        assert 0 < content_width <= width
        expected_rows = math.ceil(cell_len(text) / content_width)
        assert widget.content_size.height == expected_rows
        assert widget.source == text, "the pane's projection line lost content"
        assert str(widget.render()) == text, "the wrapped line lost content"


@pytest.mark.asyncio
@pytest.mark.parametrize("width", NARROW_WIDTHS)
async def test_a_markdown_block_wraps_completely_at_narrow_widths(width: int) -> None:
    """A wrapping paragraph keeps every word at narrow widths — the block
    path must not clip where the line path wraps, or a theme change that
    moves the wrap boundary would silently drop prose."""
    words = tuple(f"w{i}" for i in range(40))
    paragraph = " ".join(words)
    app = _Harness()
    async with app.run_test(size=(width, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        document = EntryMarkdown(paragraph, classes=kind_group_css_class("assistant"))
        await pane.mount(document)
        await pilot.pause()

        assert document.outer_size.height > 1, "the paragraph never wrapped"
        rendered = _rendered_words(document)
        for word in words:
            assert word in rendered, f"{word!r} was clipped at {width} columns"


@pytest.mark.asyncio
async def test_offset_toggle_reflows_a_wrapped_line_without_clipping() -> None:
    """Turning the offset off returns the stripe's column to the content and
    the line rewraps to the wider measure; turning it back restores the
    original wrap exactly."""
    text = "x" * 60
    app = _Harness()
    async with app.run_test(size=(24, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        widget = TranscriptLine(text, kind="assistant", first_in_entry=False)
        await pane.mount(widget)
        await pilot.pause()

        width_on = widget.content_size.width
        height_on = widget.content_size.height
        assert height_on == math.ceil(cell_len(text) / width_on)

        pane.set_show_left_offset(False)
        await pilot.pause()
        width_off = widget.content_size.width
        assert width_off == width_on + 1, (
            "the classed line trades its reserved padding column for the "
            "stripe while the offset is painted, so reclaiming the stripe "
            "returns exactly one column to the content"
        )
        assert widget.content_size.height == math.ceil(cell_len(text) / width_off)
        assert widget.source == text
        assert str(widget.render()) == text

        pane.set_show_left_offset(True)
        await pilot.pause()
        assert widget.content_size.width == width_on
        assert widget.content_size.height == height_on
        assert str(widget.render()) == text


@pytest.mark.asyncio
async def test_offset_toggle_reflows_a_markdown_block_without_clipping() -> None:
    """Same reflow claim for the block path: wider content can never grow a
    paragraph's wrapped height, and every word survives both directions."""
    words = tuple(f"w{i}" for i in range(40))
    paragraph = " ".join(words)
    app = _Harness()
    async with app.run_test(size=(24, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        document = EntryMarkdown(paragraph, classes=kind_group_css_class("assistant"))
        await pane.mount(document)
        await pilot.pause()

        height_on = document.outer_size.height

        pane.set_show_left_offset(False)
        await pilot.pause()
        assert document.outer_size.height <= height_on, (
            "reclaiming the offset columns made the paragraph taller — the "
            "content width shrank instead of growing"
        )
        rendered = _rendered_words(document)
        for word in words:
            assert word in rendered

        pane.set_show_left_offset(True)
        await pilot.pause()
        assert document.outer_size.height == height_on
        rendered = _rendered_words(document)
        for word in words:
            assert word in rendered


# ── the bar-state rule: visibility follows the active theme ────────────────

HIDDEN_BAR_SLUG = "bar-hidden"

#: The response text the live bar-state cases watch across every toggle and
#: resize — a clip or a lost line would remove it from the screen.
_BAR_RESPONSE = "the resumed answer renders in full at every width."


def _screen_words(app: Any) -> str:
    """The screen text with stripe glyphs dropped and every whitespace run
    collapsed to one space.

    A wrapped line breaks the contiguous response across exported rows, and
    the gutter stripe paints its own block glyph at the head of every row
    while the bar is visible — either one alone would shatter an exact
    substring match that has nothing to do with clipping. The collapsed
    form proves the same words render in order at every width, word-aware
    wrapping being the only way they can break.
    """
    return " ".join(screen_text(app).replace("\u2588", " ").split())


def _hidden_bar_registry() -> ThemeRegistry:
    spec = ThemeSpec(
        slug=HIDDEN_BAR_SLUG,
        name="Bar Hidden",
        dark=True,
        tokens=dict(REFINED_DEFAULT.tokens),
        transcript_bar_visible=False,
    )
    return ThemeRegistry((REFINED_DEFAULT, spec))


def _bar_frames() -> list[dict[str, Any]]:
    frames: list[dict[str, Any]] = [event("gateway.ready", {})]
    frames.extend(streaming_turn([_BAR_RESPONSE]))
    return frames


async def _drain(app: Any, pilot: Any, controls: Any) -> None:
    controls.resume()
    await app.drain(timeout=60.0)
    await pilot.pause()
    await app.render_snapshot()
    await pilot.pause()


@pytest.mark.asyncio
async def test_a_hidden_bar_theme_reclaims_the_offset_on_startup() -> None:
    """A theme naming the hidden state mounts with the column reclaimed;
    every theme that does not name it keeps the column it always had."""
    registry = _hidden_bar_registry()
    hidden, hidden_controls = paused_app(
        _bar_frames(), theme_name=HIDDEN_BAR_SLUG, theme_registry=registry
    )
    async with hidden.run_test(size=(80, 24)) as pilot:
        await _drain(hidden, pilot, hidden_controls)
        assert hidden.transcript.show_left_offset is False
        assert _BAR_RESPONSE in _screen_words(hidden)
        await hidden.shutdown_sources()

    visible, visible_controls = paused_app(_bar_frames())
    async with visible.run_test(size=(80, 24)) as pilot:
        await _drain(visible, pilot, visible_controls)
        assert visible.transcript.show_left_offset is True
        assert _BAR_RESPONSE in _screen_words(visible)
        await visible.shutdown_sources()


@pytest.mark.asyncio
@pytest.mark.parametrize("width", (24, 80))
async def test_switching_themes_hides_and_restores_the_bar_without_clipping(
    width: int,
) -> None:
    """Live 06's seam-level claim at two terminal widths: hiding the bar
    through theme control reclaims the stripe's column for content,
    restoring visibility re-reserves it, and the streamed response survives
    every toggle without clipping or lost lines."""
    registry = _hidden_bar_registry()
    app, controls = paused_app(_bar_frames(), theme_registry=registry)
    async with app.run_test(size=(width, 24)) as pilot:
        await _drain(app, pilot, controls)
        pane = app.transcript
        assert pane.show_left_offset is True

        probe = TranscriptLine("x" * 60, kind="assistant", first_in_entry=False)
        await pane.mount(probe)
        await pilot.pause()
        width_visible = probe.content_size.width

        app.theme = HIDDEN_BAR_SLUG
        await pilot.pause()
        await pilot.pause()
        assert pane.show_left_offset is False
        assert probe.content_size.width == width_visible + 1, (
            "hiding the bar did not return the stripe's column to content"
        )
        assert _BAR_RESPONSE in _screen_words(app)

        app.theme = REFINED_DEFAULT.slug
        await pilot.pause()
        await pilot.pause()
        assert pane.show_left_offset is True
        assert probe.content_size.width == width_visible
        assert _BAR_RESPONSE in _screen_words(app)

        await probe.remove()
        await app.shutdown_sources()


def _write_stored_bar_theme(config_dir: Path, slug: str, *, visible: bool) -> None:
    """Write the canonical stored theme document directly — no import
    machinery, because the stored-file refresh contract reads exactly this
    file."""
    spec = ThemeSpec(
        slug=slug,
        name="Bar Probe",
        dark=True,
        tokens=dict(REFINED_DEFAULT.tokens),
        transcript_bar_visible=visible,
    )
    path = user_theme_path(slug, config_dir=config_dir)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(serialize_user_theme(spec))


def _flip_stored_bar(config_dir: Path, slug: str, *, visible: bool) -> None:
    """The operator's lever: edit the stored theme's bar field on disk."""
    path = user_theme_path(slug, config_dir=config_dir)
    spec = replace(load_user_theme_spec(path), transcript_bar_visible=visible)
    path.write_bytes(serialize_user_theme(spec))


def _stored_bar(config_dir: Path, slug: str) -> bool:
    return load_user_theme_spec(
        user_theme_path(slug, config_dir=config_dir)
    ).transcript_bar_visible


#: The shared VS Code source fixture the import suite stages its reload
#: themes from — a real source file, not a synthesized report.
_SAMPLE_SOURCE = Path(__file__).parents[1] / "fixtures" / "vscode-themes" / "sample-dark.json"


#: The gutter stripe's own glyph. It paints only while the transcript's left
#: offset column is shown, so its presence on the exported screen is the
#: rendered fact — ``show_left_offset`` alone is a widget attribute and would
#: pass even if nothing repainted.
_GUTTER_GLYPH = "█"


async def _submit_theme_command(app: Any, pilot: Any, text: str) -> None:
    """Run one slash command the way the operator does: through the composer."""
    app.composer.text = text
    app.composer.text_area.focus()
    await pilot.press("enter")
    await app.settle_live()
    await pilot.pause()


@pytest.mark.asyncio
async def test_a_bar_only_stored_edit_moves_the_gutter_at_boot(
    tmp_path: Path,
) -> None:
    """The stored document governs the bar at boot (#141 item 16 / Live
    06): the operator edits the stored theme's bar field on disk, and the
    next time the app boots its registry from that document the mounted
    gutter follows it — in both directions, with the reclaimed column
    reflowing wrapped rows instead of clipping them. The live same-slug
    case — an edited stored field submitted through ``/theme reload`` on a
    mounted app — is covered by the companion regression below."""
    config_dir = tmp_path / "config"

    measured: list[tuple[bool, int]] = []
    for visible in (True, False, True):
        _write_stored_bar_theme(config_dir, "bar-probe", visible=visible)
        registry = theme_registry_for_config(config_dir=config_dir)
        app, controls = paused_app(
            _bar_frames(),
            theme_name="bar-probe",
            theme_registry=registry,
            theme_config_dir=config_dir,
            launch_cwd=tmp_path,
        )
        async with app.run_test(size=(80, 24)) as pilot:
            await _drain(app, pilot, controls)
            pane = app.transcript
            assert app.theme == "bar-probe"
            assert pane.show_left_offset is visible, (
                "the stored bar field did not govern the mounted gutter"
            )

            probe = TranscriptLine("x" * 60, kind="assistant", first_in_entry=False)
            await pane.mount(probe)
            await pilot.pause()
            measured.append((pane.show_left_offset, probe.content_size.width))
            assert _BAR_RESPONSE in _screen_words(app)
            await probe.remove()
            await app.shutdown_sources()

    visible_width = measured[0][1]
    assert measured[1][1] == visible_width + 1, (
        "hiding the bar did not return its column to the content"
    )
    assert measured[2][1] == visible_width


@pytest.mark.asyncio
async def test_a_bar_only_stored_edit_moves_the_gutter_through_theme_reload(
    tmp_path: Path,
) -> None:
    """P1 repair regression on the live ``/theme reload`` path (#141 item 16
    / Live 06): the operator edits the stored theme's bar field on disk and
    submits ``/theme reload`` in the composer, and the mounted gutter must
    follow in both directions without a restart.

    ``/theme reload`` is a stored-file refresh. It routes to
    :meth:`TalariaApp._refresh_stored_theme`, which loads the canonical
    stored document at ``<config>/themes/<slug>.json`` and installs it under
    the same slug; it is not a re-import and it does not re-read the recorded
    VS Code source. That is what makes this the defect's own path: the slug
    never changes, Textual's ``theme`` reactive is silent on a same-slug set,
    and ``_repaint_theme_if_changed`` publishes nothing unless the registered
    ``Theme`` value actually differs. A bar-only edit differs *only* in the
    bar, so without the bar riding in that value the swap compares equal, no
    repaint or change signal is published, and the mounted gutter stays stale
    while the stored file says otherwise.

    Delete the ``TRANSCRIPT_BAR_VARIABLE`` assignment in
    :meth:`ThemeRegistry.to_textual_theme` and this test must go red.
    """
    config_dir = tmp_path / "config"
    source = tmp_path / "bar-probe.json"
    source.write_bytes(_SAMPLE_SOURCE.read_bytes())
    import_vscode_theme(source, name="bar-probe", config_dir=config_dir)

    registry = theme_registry_for_config(config_dir=config_dir)
    app, controls = paused_app(
        _bar_frames(),
        theme_name="bar-probe",
        theme_registry=registry,
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
    )
    async with app.run_test(size=(80, 24)) as pilot:
        await _drain(app, pilot, controls)
        pane = app.transcript
        assert app.theme == "bar-probe"
        assert pane.show_left_offset is True
        assert _GUTTER_GLYPH in screen_text(app), "the gutter never painted"

        probe = TranscriptLine("x" * 60, kind="assistant", first_in_entry=False)
        await pane.mount(probe)
        await pilot.pause()
        width_visible = probe.content_size.width

        # Shown -> hidden, through the command the operator actually types.
        _flip_stored_bar(config_dir, "bar-probe", visible=False)
        await _submit_theme_command(app, pilot, "/theme reload")
        assert app.theme == "bar-probe", "the reload changed the active theme"
        assert pane.show_left_offset is False, (
            "a bar-only stored edit submitted through /theme reload left the "
            "mounted gutter stale — the same-slug swap published no repaint"
        )
        assert _GUTTER_GLYPH not in screen_text(app), (
            "the gutter stripe is still painted after a reload that hid it"
        )
        assert probe.content_size.width == width_visible + 1, (
            "hiding the bar did not return its column to the content"
        )
        assert _BAR_RESPONSE in _screen_words(app), "the reload lost content"
        assert _stored_bar(config_dir, "bar-probe") is False, (
            "the reload rewrote the stored bar choice"
        )

        # Hidden -> shown, the same way. Both directions or neither.
        _flip_stored_bar(config_dir, "bar-probe", visible=True)
        await _submit_theme_command(app, pilot, "/theme reload")
        assert pane.show_left_offset is True, (
            "a bar-only stored edit submitted through /theme reload did not "
            "bring the mounted gutter back"
        )
        assert _GUTTER_GLYPH in screen_text(app), (
            "the gutter stripe never repainted after a reload that showed it"
        )
        assert probe.content_size.width == width_visible, (
            "showing the bar did not take its column back from the content"
        )
        assert _BAR_RESPONSE in _screen_words(app), "the reload lost content"
        assert _stored_bar(config_dir, "bar-probe") is True, (
            "the reload rewrote the stored bar choice"
        )

        await probe.remove()
        await app.shutdown_sources()
