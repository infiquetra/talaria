"""Tests for C10 (#148): Mixture of Agents UI presentation in Inspector and TranscriptPane."""

from __future__ import annotations

from dataclasses import replace
from typing import ClassVar

import pytest
from textual.app import App, ComposeResult
from textual.binding import Binding, BindingType
from textual.containers import Horizontal
from textual.theme import Theme
from textual.widgets import Static

from talaria.domain.changes import DiffSelection, InspectorView, inspector_view
from talaria.domain.models import MoaView
from talaria.domain.projection import (
    EntryScopedView,
    ProvisionalTail,
    TranscriptEntryRecord,
    TranscriptView,
    entry_scoped_view,
)
from talaria.domain.normalize import MOA_FALLBACK_TEXT
from talaria.ui.inspector import (
    Inspector,
    InspectorMoaRow,
)
from talaria.ui.theme import BUILTIN_THEME_REGISTRY
from talaria.ui.transcript import TranscriptPane
from tests.domain.conftest import raw_event, replay


class FocusTarget(Static):
    can_focus = True


class InspectorHarness(App[None]):
    CSS = """
    Screen {
        layout: vertical;
    }
    #main-and-inspector {
        height: 1fr;
        width: 1fr;
    }
    #main {
        width: 1fr;
        height: 1fr;
    }
    """

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("ctrl+o", "toggle_inspector", "inspector", priority=True),
    ]

    def __init__(self, view: InspectorView) -> None:
        super().__init__()
        self.register_theme(
            Theme(
                name="inspector-test",
                primary="#0969DA",
                foreground="#1F2328",
                background="#F6F8FA",
                surface="#FFFFFF",
                variables={
                    "talaria-inspector-background": "#FFFFFF",
                    "talaria-inspector-border": "#6E7781",
                    "talaria-inspector-heading": "#0969DA",
                },
            )
        )
        self.theme = "inspector-test"
        self.view = view
        self.selection: DiffSelection | None = None

    def compose(self) -> ComposeResult:
        with Horizontal(id="main-and-inspector"):
            yield FocusTarget("transcript", id="main")
            yield Inspector(id="inspector")

    async def on_mount(self) -> None:
        await self.inspector.apply(self.view)

    @property
    def inspector(self) -> Inspector:
        return self.query_one("#inspector", Inspector)

    def on_inspector_file_selected(self, message: Inspector.FileSelected) -> None:
        self.selection = message.selection


def _seeded_inspector_view() -> InspectorView:
    state = replay([
        raw_event("message.start"),
        raw_event("tool.start", {"name": "edit_file", "context": "talaria/config.py"}),
        raw_event(
            "tool.complete",
            {
                "name": "edit_file",
                "summary": "1 hunk completed",
                "inline_diff": (
                    "--- a/talaria/config.py\n"
                    "+++ b/talaria/config.py\n"
                    "@@ -1 +1 @@\n"
                    "-old\n"
                    "+new"
                ),
            },
        ),
    ])
    return inspector_view(entry_scoped_view(state))


# ── Inspector MoA section tests ──────────────────────────────────────────


@pytest.mark.asyncio
async def test_inspector_moa_section_starts_with_fallback_empty_row() -> None:
    """Fallback state when no MoA events are observed (Live 19, D7, issue #148)."""
    app = InspectorHarness(_seeded_inspector_view())
    async with app.run_test(size=(132, 30)):
        assert app.inspector.moa_texts == (f"  {MOA_FALLBACK_TEXT}",)

        empty = app.inspector.query_one(".inspector--moa-empty", Static)
        assert str(empty.content) == f"  {MOA_FALLBACK_TEXT}"

        headings = [
            str(node.content)
            for node in app.inspector.query(".inspector--heading").nodes
            if isinstance(node, Static)
        ]
        assert "MIXTURE OF AGENTS" in headings
        assert "DIAGNOSTICS" in headings
        moa_idx = headings.index("MIXTURE OF AGENTS")
        diag_idx = headings.index("DIAGNOSTICS")
        assert moa_idx == diag_idx - 1


@pytest.mark.asyncio
async def test_inspector_moa_section_renders_active_rows_and_pending() -> None:
    """Active MoA run mounts phase row, finished advisor rows, and pending row."""
    base_view = _seeded_inspector_view()
    moa = MoaView(
        live_text="Mixture of Agents: collecting 2/5 references · model-b finished",
        inspector_rows=(
            "collecting 2/5 references · model-b finished",
            "model-a · finished 1/5 · First advice",
            "model-b · finished 2/5 · Second advice",
            "3 pending",
        ),
        is_active=True,
    )
    view = replace(base_view, moa=moa)

    app = InspectorHarness(view)
    async with app.run_test(size=(132, 30)):
        assert len(app.inspector.query(".inspector--moa-empty").nodes) == 0

        assert len(app.inspector.moa_texts) == 4
        assert app.inspector.moa_texts[0] == "  collecting 2/5 references · model-b finished"
        assert app.inspector.moa_texts[1] == "  model-a · finished 1/5 · First advice"
        assert app.inspector.moa_texts[2] == "  model-b · finished 2/5 · Second advice"
        assert app.inspector.moa_texts[3] == "  3 pending"


@pytest.mark.asyncio
async def test_inspector_moa_row_focus_gutter_and_folding() -> None:
    """MoA rows expand when focused and fold back when focus leaves."""
    long_advice = "model-a · finished 1/3 · " + ("very long advice text " * 10)
    base_view = _seeded_inspector_view()
    moa = MoaView(
        live_text="Mixture of Agents: collecting 1/3 references · model-a finished",
        inspector_rows=(
            "collecting 1/3 references · model-a finished",
            long_advice,
            "2 pending",
        ),
        is_active=True,
    )
    view = replace(base_view, moa=moa)

    app = InspectorHarness(view)
    async with app.run_test(size=(132, 40)) as pilot:
        await pilot.pause()
        moa_rows = [
            row
            for row in app.inspector.query(".inspector--moa").nodes
            if isinstance(row, InspectorMoaRow)
        ]
        assert len(moa_rows) == 3

        # Row 1 has long text; initially clips to height 1
        long_row = moa_rows[1]
        assert long_row.size.height == 1

        app.screen.set_focus(long_row)
        await pilot.pause()

        assert app.focused is long_row
        assert long_row.size.height > 1, "a focused MoA row must expand"

        app.screen.set_focus(None)
        await pilot.pause()
        assert long_row.size.height == 1, "focus leaving folds the row back"


# ── TranscriptPane MoA live widget tests ─────────────────────────────────


class _TranscriptHarness(App[None]):
    def __init__(self) -> None:
        super().__init__()
        BUILTIN_THEME_REGISTRY.register(self)
        self.theme = "refined-default"

    def compose(self) -> ComposeResult:
        yield TranscriptPane(id="t")

    @property
    def pane(self) -> TranscriptPane:
        return self.query_one("#t", TranscriptPane)


def _empty_tail(kind: str) -> ProvisionalTail:
    return ProvisionalTail(kind=kind, raw_text="", generation=0)  # type: ignore[arg-type]


@pytest.mark.asyncio
async def test_transcript_pane_reconciles_moa_live_widget() -> None:
    """TranscriptPane mounts live widget during run and removes it upon completion."""
    app = _TranscriptHarness()
    async with app.run_test(size=(100, 30)):
        pane = app.pane

        empty_view = TranscriptView(
            lines=(),
            entry_count=0,
            committed_lines=0,
            kinds=(),
        )

        # 1. Initially no live text -> no widget
        esv_idle = EntryScopedView(
            entries=(),
            assistant_tail=_empty_tail("assistant"),
            reasoning_tail=_empty_tail("reasoning"),
            moa=MoaView(),
        )
        await pane.apply(empty_view, esv_idle)
        assert pane.moa_live_text is None
        assert len(pane.query(".transcript--moa-live").nodes) == 0

        # 2. Live text arrives -> widget mounted above tails
        live_line = "Mixture of Agents: collecting 1/5 references · advisor-1 finished"
        esv_live = EntryScopedView(
            entries=(),
            assistant_tail=_empty_tail("assistant"),
            reasoning_tail=_empty_tail("reasoning"),
            moa=MoaView(live_text=live_line, is_active=True),
        )
        await pane.apply(empty_view, esv_live)
        assert pane.moa_live_text == live_line
        live_nodes = pane.query(".transcript--moa-live").nodes
        assert len(live_nodes) == 1
        assert str(live_nodes[0].render()) == live_line

        # 3. Terminal state (live_text=None) -> widget unmounted
        record = TranscriptEntryRecord(
            entry_id=1,
            kind="system",
            raw_body="Mixture of Agents: 5/5 references · aggregated by gpt-5.6",
            committed=True,
            line_span=(0, 1),
        )
        completed_view = TranscriptView(
            lines=(record.raw_body,),
            entry_count=1,
            committed_lines=1,
            kinds=("system",),
        )
        esv_complete = EntryScopedView(
            entries=(record,),
            assistant_tail=_empty_tail("assistant"),
            reasoning_tail=_empty_tail("reasoning"),
            moa=MoaView(committed_text=record.raw_body, is_active=False),
        )
        await pane.apply(completed_view, esv_complete)
        assert pane.moa_live_text is None
        assert len(pane.query(".transcript--moa-live").nodes) == 0
