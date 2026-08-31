"""Observable contracts for the bounded, strictly read-only diff viewer."""

from __future__ import annotations

import ast
import html
import inspect
import re
from pathlib import Path
from types import ModuleType
from typing import cast

import pytest
from rich.color import Color
from rich.segment import Segment
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.theme import Theme
from textual.widgets import Static

from talaria.domain.commands import TALARIA_LOCAL_COMMANDS
from talaria.themes.builtins import ACCESSIBLE_HIGH_CONTRAST, REFINED_DEFAULT
from talaria.ui import diff_viewer as diff_viewer_module
from talaria.ui.dialog import PickerDialog
from talaria.ui.diff_viewer import (
    INTRALINE_CELL_CAP,
    OVERSCAN_ROWS,
    SIDE_BY_SIDE_REFUSAL,
    DiffViewer,
    DiffViewerDocument,
    DiffViewerFile,
)
from talaria.ui.inspector import InspectorFileRow
from talaria.ui.theme import BUILTIN_THEME_REGISTRY
from tests.domain.conftest import raw_event, replay
from tests.ui.conftest import RecordingDispatcher, live_app

FIXTURES = Path(__file__).parents[1] / "fixtures" / "diffs"

THEME_A = Theme(
    name="diff-a",
    primary="#13579B",
    secondary="#2468AC",
    warning="#B36B00",
    error="#A40020",
    success="#006B2B",
    accent="#7A3E9D",
    foreground="#202124",
    background="#FAFAFA",
    surface="#F0F0F0",
    variables={
        "talaria-diff-context": "#202124",
        "talaria-diff-line-number": "#59636E",
        "talaria-diff-added": "#006B2B",
        "talaria-diff-added-background": "#DDF3E4",
        "talaria-diff-removed": "#A40020",
        "talaria-diff-removed-background": "#F8DFE2",
        "talaria-diff-hunk": "#13579B",
        "talaria-diff-hunk-background": "#E2ECFA",
        "talaria-diff-intraline-added-background": "#83C99A",
        "talaria-diff-intraline-removed-background": "#E69A9A",
        "talaria-syntax-comment": "#59636E",
        "talaria-syntax-keyword": "#A020F0",
        "talaria-syntax-string": "#006B2B",
        "talaria-syntax-number": "#B36B00",
        "talaria-syntax-function": "#13579B",
        "talaria-syntax-type": "#2468AC",
        "talaria-syntax-variable": "#202124",
        "talaria-syntax-operator": "#7A3E9D",
        "talaria-syntax-constant": "#B36B00",
    },
)

THEME_B = Theme(
    name="diff-b",
    primary="#8FD3FF",
    secondary="#9EC5F8",
    warning="#FFD75F",
    error="#FF9A9A",
    success="#9BFFB5",
    accent="#D7A9FF",
    foreground="#FFFFFF",
    background="#000000",
    surface="#101010",
    variables={
        **THEME_A.variables,
        "talaria-diff-context": "#FFFFFF",
        "talaria-diff-intraline-added-background": "#005C24",
        "talaria-diff-intraline-removed-background": "#6E0000",
        "talaria-syntax-keyword": "#00FFFF",
    },
)


def fixture_file(name: str, *, key: str | None = None, path: str | None = None) -> DiffViewerFile:
    source = (FIXTURES / name).read_text(encoding="utf-8")
    shown_path = path or name.removesuffix(".diff")
    return DiffViewerFile(key=key or shown_path, path=shown_path, unified_diff=source)


def sample_document() -> DiffViewerDocument:
    return DiffViewerDocument(
        (
            fixture_file("config.py.diff", key="config", path="talaria/config.py"),
            fixture_file("notes.unknown.diff", key="notes", path="notes.unknown"),
        )
    )


class Host(App[None]):
    """The smallest real App that exercises modal stacking and theme resolution."""

    def __init__(
        self,
        document: DiffViewerDocument,
        *,
        file_key: str | None = None,
        hunk_index: int = 0,
    ) -> None:
        super().__init__()
        self.document = document
        self.file_key = file_key
        self.hunk_index = hunk_index
        self.closed = False

    def compose(self) -> ComposeResult:
        yield Static("session behind diff")

    def on_mount(self) -> None:
        BUILTIN_THEME_REGISTRY.register(self)
        self.register_theme(THEME_A)
        self.register_theme(THEME_B)
        self.theme = THEME_A.name

        def closed(_: None) -> None:
            self.closed = True

        self.push_screen(
            DiffViewer(
                self.document,
                file_key=self.file_key,
                hunk_index=self.hunk_index,
            ),
            closed,
        )


def viewer(app: Host) -> DiffViewer:
    screen = app.screen
    assert isinstance(screen, DiffViewer)
    return screen


def screen_text(app: App[None]) -> str:
    body = re.sub(r"<[^>]+>", "", app.export_screenshot())
    return html.unescape(body).replace("\xa0", " ")


def rendered_segments(diff: DiffViewer) -> list[Segment]:
    rows: list[Segment] = []
    for y in range(diff.canvas.size.height):
        rows.extend(diff.canvas.render_line(y))
    return rows


def color_hex(color: Color | None) -> str | None:
    if color is None:
        return None
    triplet = color.get_truecolor()
    return f"#{triplet.red:02X}{triplet.green:02X}{triplet.blue:02X}"


def segment_colors(segments: list[Segment], text: str) -> set[str | None]:
    return {
        color_hex(segment.style.color if segment.style is not None else None)
        for segment in segments
        if text in segment.text
    }


@pytest.mark.asyncio
async def test_side_by_side_and_unified_are_real_rendered_modes() -> None:
    app = Host(sample_document())
    async with app.run_test(size=(112, 28)) as pilot:
        await pilot.pause()
        diff = viewer(app)
        wide = screen_text(app)
        assert diff.effective_mode == "side-by-side"
        assert "base · old" in wide
        assert "working tree · new" in wide
        assert "interval_seconds" in wide
        assert "[read only]" in wide

        await pilot.press("u")
        await pilot.pause()
        unified = screen_text(app)
        assert str(diff.effective_mode) == "unified"
        assert "old new" in unified
        unified_rows = "".join(segment.text for segment in rendered_segments(diff))
        assert re.search(r'-\s+"interval_seconds": 5,', unified_rows)
        assert re.search(r'\+\s+"interval_seconds": 10,', unified_rows)

        await pilot.press("s")
        await pilot.pause()
        assert str(diff.effective_mode) == "side-by-side"
        assert "working tree · new" in screen_text(app)


@pytest.mark.asyncio
async def test_111_112_fallback_preserves_selection_anchor_and_preference() -> None:
    large = fixture_file("large.py.diff", key="large", path="large.py")
    app = Host(DiffViewerDocument((large,)))
    async with app.run_test(size=(112, 28)) as pilot:
        await pilot.pause()
        diff = viewer(app)
        diff.canvas.scroll_to(y=60, animate=False, immediate=True, force=True)
        await pilot.pause()
        anchor = diff.canvas.anchor_row_id
        assert anchor is not None

        await pilot.resize_terminal(111, 28)
        await pilot.pause()
        assert diff.effective_mode == "unified"
        assert diff.preferred_mode == "side-by-side"
        assert diff.canvas.anchor_row_id == anchor

        header_height = diff.query_one(".diff--header", Static).size.height
        await pilot.press("s")
        await pilot.pause()
        assert diff.refusal_text == SIDE_BY_SIDE_REFUSAL
        assert SIDE_BY_SIDE_REFUSAL in diff.header_text
        assert diff.query_one(".diff--header", Static).size.height == header_height == 1

        await pilot.resize_terminal(112, 28)
        await pilot.pause()
        assert str(diff.effective_mode) == "side-by-side"
        assert diff.canvas.anchor_row_id == anchor
        assert SIDE_BY_SIDE_REFUSAL not in diff.header_text

        await pilot.press("u")
        await pilot.resize_terminal(111, 28)
        await pilot.resize_terminal(112, 28)
        await pilot.pause()
        assert str(diff.preferred_mode) == "unified"
        assert diff.effective_mode == "unified"


@pytest.mark.asyncio
async def test_hunk_file_and_picker_navigation_cycle_exactly() -> None:
    app = Host(sample_document(), file_key="config")
    async with app.run_test(size=(112, 28)) as pilot:
        await pilot.pause()
        diff = viewer(app)
        assert (diff.active_file_key, diff.hunk_index) == ("config", 0)

        await pilot.press("n")
        assert diff.hunk_index == 1
        await pilot.press("n")
        assert diff.hunk_index == 0
        await pilot.press("p")
        assert diff.hunk_index == 1

        await pilot.press("N")
        assert (diff.active_file_key, diff.hunk_index) == ("notes", 0)
        await pilot.press("N")
        assert diff.active_file_key == "config"
        await pilot.press("P")
        assert diff.active_file_key == "notes"

        await pilot.press("f")
        await pilot.pause()
        picker = app.screen
        assert isinstance(picker, PickerDialog)
        assert "notes.unknown" in picker.active_row_text
        await pilot.press("up", "enter")
        await pilot.pause()
        assert viewer(app) is diff
        assert diff.active_file_key == "config"


@pytest.mark.asyncio
async def test_intraline_spans_overlay_syntax_and_resolve_from_each_theme() -> None:
    app = Host(sample_document())
    async with app.run_test(size=(111, 28)) as pilot:
        await pilot.pause()
        diff = viewer(app)
        segments_a = rendered_segments(diff)
        changed_a = {
            color_hex(segment.style.bgcolor if segment.style is not None else None)
            for segment in segments_a
            if segment.text in {"5", "10"}
        }
        assert "#E69A9A" in changed_a
        assert "#83C99A" in changed_a
        assert "#A020F0" in segment_colors(segments_a, "return")

        app.theme = THEME_B.name
        diff.canvas.refresh()
        await pilot.pause()
        segments_b = rendered_segments(diff)
        changed_b = {
            color_hex(segment.style.bgcolor if segment.style is not None else None)
            for segment in segments_b
            if segment.text in {"5", "10"}
        }
        assert "#6E0000" in changed_b
        assert "#005C24" in changed_b
        assert "#00FFFF" in segment_colors(segments_b, "return")


@pytest.mark.asyncio
async def test_real_builtin_diff_tokens_repaint_refined_and_high_contrast() -> None:
    app = Host(sample_document())
    async with app.run_test(size=(111, 28)) as pilot:
        await pilot.pause()
        diff = viewer(app)

        app.theme = REFINED_DEFAULT.slug
        diff.canvas.refresh()
        await pilot.pause()
        refined = rendered_segments(diff)
        refined_changed = {
            color_hex(segment.style.bgcolor if segment.style is not None else None)
            for segment in refined
            if segment.text in {"5", "10"}
        }
        assert (
            REFINED_DEFAULT.tokens["talaria.diff.intraline-removed.background"]
            in refined_changed
        )
        assert (
            REFINED_DEFAULT.tokens["talaria.diff.intraline-added.background"]
            in refined_changed
        )
        assert REFINED_DEFAULT.tokens["talaria.syntax.keyword"] in segment_colors(
            refined, "return"
        )

        app.theme = ACCESSIBLE_HIGH_CONTRAST.slug
        diff.canvas.refresh()
        await pilot.pause()
        high_contrast = rendered_segments(diff)
        high_contrast_changed = {
            color_hex(segment.style.bgcolor if segment.style is not None else None)
            for segment in high_contrast
            if segment.text in {"5", "10"}
        }
        assert (
            ACCESSIBLE_HIGH_CONTRAST.tokens[
                "talaria.diff.intraline-removed.background"
            ]
            in high_contrast_changed
        )
        assert (
            ACCESSIBLE_HIGH_CONTRAST.tokens[
                "talaria.diff.intraline-added.background"
            ]
            in high_contrast_changed
        )
        assert ACCESSIBLE_HIGH_CONTRAST.tokens[
            "talaria.syntax.keyword"
        ] in segment_colors(high_contrast, "return")


@pytest.mark.asyncio
async def test_production_command_and_inspector_selection_open_only_held_diffs() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)
    app.state = replay(
        [
            raw_event("message.start"),
            raw_event("tool.start", {"name": "edit_file", "context": "first.py"}),
            raw_event(
                "tool.complete",
                {
                    "name": "edit_file",
                    "summary": "two files changed",
                    "inline_diff": (
                        "--- a/first.py\n"
                        "+++ b/first.py\n"
                        "@@ -1 +1 @@\n"
                        "-old first\n"
                        "+new first\n"
                        "--- a/second.py\n"
                        "+++ b/second.py\n"
                        "@@ -1 +1 @@\n"
                        "-old second\n"
                        "+new second"
                    ),
                },
            ),
        ]
    )

    async with app.run_test(size=(132, 30)) as pilot:
        await app.render_snapshot()
        await pilot.pause()
        inspector = app.inspector
        assert inspector.document.file_for("second.py") is not None

        app.composer.text = "/diffs"
        await pilot.press("enter")
        await pilot.pause()
        diff = app.screen
        assert isinstance(diff, DiffViewer)
        assert diff.active_file_key == "first.py"
        assert inspector.is_temporarily_hidden
        assert dispatcher.operator_calls == []

        await pilot.press("escape")
        await pilot.pause()
        assert inspector.is_docked
        assert not inspector.is_temporarily_hidden

        rows = list(inspector.query(InspectorFileRow))
        assert len(rows) == 2
        rows[1].focus()
        await pilot.press("enter")
        await pilot.pause()
        selected = app.screen
        assert isinstance(selected, DiffViewer)
        assert selected.active_file_key == "second.py"
        assert inspector.selected_file_key == "second.py"
        assert inspector.is_temporarily_hidden
        assert dispatcher.operator_calls == []


@pytest.mark.asyncio
async def test_unknown_language_and_hostile_text_are_literal() -> None:
    hostile = DiffViewerFile(
        key="hostile",
        path="[bold]odd[/]\x1b.unknown",
        unified_diff=(
            "--- a/odd\n"
            "+++ b/odd\n"
            "@@ -1 +1 @@\n"
            "-[red]before[/]\x1b[2J\n"
            "+[green]after[/]\x1b[2J"
        ),
    )
    app = Host(DiffViewerDocument((hostile,)))
    async with app.run_test(size=(111, 24)) as pilot:
        await pilot.pause()
        diff = viewer(app)
        shown = screen_text(app)
        assert diff.active_lexer == "plain text"
        assert "[bold]odd[/]␛.unknown" in shown
        assert "[red]before[/]␛[2J" in shown
        assert "[green]after[/]␛[2J" in shown


@pytest.mark.asyncio
async def test_large_fixture_formats_only_the_window_and_visible_pairs() -> None:
    large = fixture_file("large.py.diff", key="large", path="large.py")
    app = Host(DiffViewerDocument((large,)))
    async with app.run_test(size=(80, 16)) as pilot:
        await pilot.pause()
        diff = viewer(app)
        screen_text(app)
        first_pairs = diff.canvas.intraline_pair_ids
        assert diff.canvas.row_count > diff.canvas.size.height + 2 * OVERSCAN_ROWS
        assert diff.canvas.formatted_row_count <= (
            diff.canvas.size.height + 2 * OVERSCAN_ROWS
        )
        assert diff.canvas.intraline_compare_count == len(first_pairs)
        assert len(first_pairs) < 16

        await pilot.press("end")
        await pilot.pause()
        screen_text(app)
        assert diff.canvas.formatted_row_count <= (
            diff.canvas.size.height + 2 * OVERSCAN_ROWS
        )
        assert diff.canvas.intraline_compare_count == len(diff.canvas.intraline_pair_ids)
        assert diff.canvas.intraline_pair_ids != first_pairs
        assert diff.index_passes == 1


@pytest.mark.asyncio
async def test_pathological_pair_skips_sequence_comparison_and_long_lines_clip() -> None:
    old = "a" * (INTRALINE_CELL_CAP + 1)
    new = "b" * (INTRALINE_CELL_CAP + 1)
    source = DiffViewerFile(
        key="long",
        path="long.unknown",
        unified_diff=f"--- a/long\n+++ b/long\n@@ -1 +1 @@\n-{old}\n+{new}",
    )
    app = Host(DiffViewerDocument((source,)))
    async with app.run_test(size=(80, 18)) as pilot:
        await pilot.pause()
        diff = viewer(app)
        shown = screen_text(app)
        assert diff.canvas.intraline_compare_count == 0
        assert "…" in shown
        before = diff.canvas.scroll_offset.x
        await pilot.press("right")
        await pilot.pause()
        assert diff.canvas.scroll_offset.x > before
        assert diff.canvas.scroll_offset.x <= diff.canvas.max_scroll_x


@pytest.mark.asyncio
async def test_empty_document_is_honest_and_escape_returns_to_prior_surface() -> None:
    app = Host(DiffViewerDocument())
    async with app.run_test(size=(80, 20)) as pilot:
        await pilot.pause()
        shown = screen_text(app)
        assert "no session-reported changes" in shown
        assert "[none available from this session]" in shown
        await pilot.press("f")
        assert isinstance(app.screen, DiffViewer)
        await pilot.press("escape")
        await pilot.pause()
        assert app.closed
        assert "session behind diff" in screen_text(app)


def _tree(module: ModuleType) -> ast.Module:
    return ast.parse(inspect.getsource(module))


def test_read_only_boundary_is_proved_by_ast_keymap_and_command_introspection() -> None:
    """Reject mutation surfaces structurally; labels and prose are not evidence."""
    forbidden_actions = {
        "edit",
        "stage",
        "revert",
        "discard",
        "apply",
        "checkout",
        "write",
    }
    allowed_actions = {
        "close",
        "next_hunk",
        "previous_hunk",
        "next_file",
        "previous_file",
        "file_list",
        "unified",
        "side_by_side",
    }

    bindings = cast(tuple[Binding, ...], tuple(DiffViewer.BINDINGS))
    assert {binding.action for binding in bindings} == allowed_actions
    assert {binding.key for binding in bindings} == {"escape", "n", "p", "N", "P", "f", "u", "s"}
    assert not any(
        forbidden in f"{binding.action} {binding.description}".casefold()
        for binding in bindings
        for forbidden in forbidden_actions
    )

    tree = _tree(diff_viewer_module)
    action_methods = {
        node.name.removeprefix("action_")
        for node in ast.walk(tree)
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        and node.name.startswith("action_")
    }
    assert action_methods == allowed_actions

    forbidden_calls = {
        "open",
        "write_text",
        "write_bytes",
        "unlink",
        "rename",
        "replace",
        "run",
        "popen",
        "dispatch",
        "call",
    }
    calls = {
        node.func.attr if isinstance(node.func, ast.Attribute) else node.func.id
        for node in ast.walk(tree)
        if isinstance(node, ast.Call)
        and isinstance(node.func, (ast.Attribute, ast.Name))
    }
    assert calls.isdisjoint(forbidden_calls)

    direct_imports = {
        alias.name.split(".", 1)[0]
        for node in ast.walk(tree)
        if isinstance(node, ast.Import)
        for alias in node.names
    }
    from_imports = {
        node.module.split(".", 1)[0]
        for node in ast.walk(tree)
        if isinstance(node, ast.ImportFrom) and node.module is not None
    }
    imports = direct_imports | from_imports
    assert imports.isdisjoint({"pathlib", "subprocess", "shutil", "os"})

    assert any(command.name == "/quit" for command in TALARIA_LOCAL_COMMANDS)
    diff_commands = tuple(
        command for command in TALARIA_LOCAL_COMMANDS if "diff" in command.name
    )
    assert {command.name for command in diff_commands} == {"/diffs"}
    assert not any(
        forbidden in f"{command.name} {command.action} {command.description}".casefold()
        for command in diff_commands
        for forbidden in forbidden_actions
    )
