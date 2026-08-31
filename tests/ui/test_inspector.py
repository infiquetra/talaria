"""The right inspector is responsive without fetching or scanning for state."""

from __future__ import annotations

from typing import ClassVar

import pytest
from rich.text import Text
from textual import events
from textual.app import App, ComposeResult
from textual.binding import Binding, BindingType
from textual.containers import Horizontal
from textual.theme import Theme
from textual.widgets import Static

from talaria.domain.changes import DiffSelection, InspectorView, inspector_view
from talaria.domain.models import QueueItem, SubagentRow, Usage
from talaria.domain.projection import SubagentView, entry_scoped_view
from talaria.domain.queue import NeedsYouQueue
from talaria.ui.inspector import (
    EMPTY_SECTION,
    MAX_INSPECTOR_WIDTH,
    MIN_INSPECTOR_WIDTH,
    Inspector,
)
from talaria.ui.picker import SessionModel
from tests.domain.conftest import raw_event, replay
from tests.ui.conftest import RecordingDispatcher, live_app, paused_app


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
        Binding("ctrl+b", "toggle_inspector", "inspector", priority=True),
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
        self.external_calls = 0

    def compose(self) -> ComposeResult:
        with Horizontal(id="main-and-inspector"):
            yield FocusTarget("transcript", id="main")
            yield Inspector(id="inspector")

    async def on_mount(self) -> None:
        await self.inspector.apply(self.view)

    def on_resize(self, event: events.Resize) -> None:
        self.inspector.set_terminal_width(event.size.width)

    @property
    def inspector(self) -> Inspector:
        return self.query_one("#inspector", Inspector)

    def action_toggle_inspector(self) -> None:
        self.inspector.toggle()

    def execute_local_command(self, command: str) -> None:
        if command == "/inspector":
            self.inspector.toggle()

    def on_inspector_file_selected(self, message: Inspector.FileSelected) -> None:
        self.selection = message.selection


def _seeded_view() -> InspectorView:
    state = replay(
        [
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
        ]
    )
    queue = NeedsYouQueue(
        items=(
            QueueItem(
                profile="default",
                session_id="session-7",
                request_key="approval-1",
                source="driven",
                kind="approval",
                summary="review command",
                row_key=("default", "session-7"),
            ),
        )
    )
    agents = SubagentView(
        rows=(
            SubagentRow(
                id="agent-1",
                name="tests",
                status="running",
                elapsed=2.0,
                detail="focused suite",
            ),
        ),
        active_count=1,
        terminal_count=0,
    )
    return inspector_view(
        entry_scoped_view(state),
        queue=queue,
        agents=agents,
        session_id="session-7",
        profile="default",
        endpoint="http://gateway.example",
        model="Muse/Spark 1.2",
        usage=Usage(input_tokens=25, output_tokens=10, observed=True),
    )


@pytest.mark.asyncio
async def test_ctrl_b_and_local_command_toggle_the_wide_dock() -> None:
    app = InspectorHarness(_seeded_view())
    async with app.run_test(size=(132, 30)) as pilot:
        assert app.inspector.is_docked
        assert app.inspector.panel_width == 36

        await pilot.press("ctrl+b")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed
        assert app.inspector.requested_collapsed

        app.execute_local_command("/inspector")
        await pilot.pause()
        assert app.inspector.is_docked
        assert not app.inspector.requested_collapsed


@pytest.mark.asyncio
async def test_keyboard_resize_moves_by_four_and_clamps_at_28_and_48() -> None:
    app = InspectorHarness(_seeded_view())
    async with app.run_test(size=(132, 30)) as pilot:
        app.inspector.focus()
        await pilot.pause()
        for _ in range(10):
            await pilot.press("shift+left")
        assert app.inspector.panel_width == MIN_INSPECTOR_WIDTH
        assert app.inspector.region.width == MIN_INSPECTOR_WIDTH

        for _ in range(10):
            await pilot.press("shift+right")
        assert app.inspector.panel_width == MAX_INSPECTOR_WIDTH
        assert app.inspector.region.width == MAX_INSPECTOR_WIDTH


@pytest.mark.asyncio
async def test_119_120_transitions_restore_only_the_requested_open_state() -> None:
    app = InspectorHarness(_seeded_view())
    async with app.run_test(size=(120, 30)) as pilot:
        assert app.inspector.is_docked

        await pilot.resize_terminal(119, 30)
        await pilot.pause()
        assert app.inspector.auto_collapsed
        assert app.inspector.is_effectively_collapsed
        assert not app.inspector.requested_collapsed

        await pilot.resize_terminal(120, 30)
        await pilot.pause()
        assert app.inspector.is_docked

        await pilot.press("ctrl+b")
        await pilot.pause()
        assert app.inspector.requested_collapsed
        await pilot.resize_terminal(119, 30)
        await pilot.resize_terminal(120, 30)
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed
        assert app.inspector.requested_collapsed


@pytest.mark.asyncio
async def test_narrow_overlay_keeps_main_width_and_escape_restores_focus() -> None:
    app = InspectorHarness(_seeded_view())
    async with app.run_test(size=(78, 30)) as pilot:
        main = app.query_one("#main", FocusTarget)
        main.focus()
        await pilot.pause()
        width_before = main.region.width

        await pilot.press("ctrl+b")
        await pilot.pause()
        assert app.inspector.is_overlay
        assert app.inspector.effective_width == 36
        assert app.inspector.region.width == 36
        assert Text.from_markup(app.inspector.border_title or "").plain == "Inspector [overlay]"
        assert main.region.width == width_before
        assert app.focused is not main

        await pilot.press("escape")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed
        assert app.focused is main

        await pilot.resize_terminal(30, 30)
        await pilot.press("ctrl+b")
        await pilot.pause()
        assert app.inspector.is_overlay
        assert app.inspector.effective_width == 30
        assert app.inspector.region.width == 30


@pytest.mark.asyncio
async def test_all_seeded_sections_render_and_file_selection_is_a_message_only() -> None:
    app = InspectorHarness(_seeded_view())
    async with app.run_test(size=(132, 30)) as pilot:
        assert any("[!] approval  waiting" in row for row in app.inspector.task_texts)
        assert any("[>] tests  running" in row for row in app.inspector.task_texts)
        assert "session  session-7" in app.inspector.context_text
        assert "profile  default" in app.inspector.context_text
        assert "model    Muse/Spark 1.2" in app.inspector.context_text
        assert app.inspector.file_texts == ("  M talaria/config.py",)
        assert "edit_file · talaria/config.py" in app.inspector.operation_text
        assert "completed" in app.inspector.operation_text

        app.inspector.query_one(".inspector--file").focus()
        await pilot.press("enter")
        await pilot.pause()
        assert app.selection == DiffSelection("talaria/config.py", 0)
        assert app.inspector.selected_file_key == "talaria/config.py"
        assert app.external_calls == 0


@pytest.mark.asyncio
async def test_every_empty_section_says_no_state_was_observed() -> None:
    empty = inspector_view(entry_scoped_view(replay([])))
    app = InspectorHarness(empty)
    async with app.run_test(size=(132, 30)):
        assert app.inspector.task_texts == ()
        assert EMPTY_SECTION in app.inspector.context_text
        assert app.inspector.file_texts == ()
        assert EMPTY_SECTION in app.inspector.operation_text
        assert app.inspector.query(".inspector--empty").nodes


@pytest.mark.asyncio
async def test_production_app_ctrl_b_and_inspector_command_are_socket_free() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.pause()
        assert app.inspector.is_docked
        assert "ctrl+b inspector" in app.help_bar.help_text
        assert "/ commands" in app.help_bar.help_text

        await pilot.press("ctrl+b")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed

        app.composer.text = "/inspector"
        await pilot.press("enter")
        await pilot.pause()
        assert app.inspector.is_docked
        assert app.composer.text == ""
        assert dispatcher.operator_calls == []


@pytest.mark.asyncio
async def test_production_resize_adapter_preserves_dock_and_overlay_geometry() -> None:
    app = live_app(RecordingDispatcher())

    async with app.run_test(size=(120, 30)) as pilot:
        await pilot.pause()
        assert app.inspector.is_docked

        await pilot.resize_terminal(119, 30)
        await pilot.pause()
        assert app.inspector.auto_collapsed
        assert not app.inspector.requested_collapsed

        await pilot.resize_terminal(120, 30)
        await pilot.pause()
        assert app.inspector.is_docked

        await pilot.resize_terminal(78, 30)
        await pilot.pause()
        body_width = app.query_one("#body").region.width
        app.composer.text_area.focus()

        await pilot.press("ctrl+b")
        await pilot.pause()
        assert app.inspector.is_overlay
        assert app.inspector.region.width == 36
        assert app.query_one("#body").region.width == body_width

        await pilot.press("escape")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed
        assert app.focused is app.composer.text_area


@pytest.mark.asyncio
async def test_production_render_boundary_populates_all_held_sections() -> None:
    app, _controls = paused_app(
        [],
        current_profile="default",
        profile_endpoints={"default": "http://gateway.example"},
    )
    app.state = replay(
        [
            raw_event("session.info", {"usage": {"input_tokens": 25}}),
            raw_event("message.start"),
            raw_event(
                "subagent.start",
                {"subagent_id": "tests", "goal": "run tests"},
            ),
            raw_event("approval.request", {"description": "approve change"}),
            raw_event(
                "tool.start",
                {"name": "edit_file", "context": "talaria/config.py"},
            ),
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
        ]
    )
    app.session_model = SessionModel(
        session_id="sess-focus",
        provider_slug="Muse",
        model="Spark 1.2",
    )

    async with app.run_test(size=(132, 30)) as pilot:
        await app.render_snapshot()
        await pilot.pause()

        assert any("approval" in row for row in app.inspector.task_texts)
        assert any("run tests" in row for row in app.inspector.task_texts)
        assert "session  sess-focus" in app.inspector.context_text
        assert "profile  default" in app.inspector.context_text
        assert "endpoint http://gateway.example" in app.inspector.context_text
        assert "model    Muse/Spark 1.2" in app.inspector.context_text
        assert "usage    25 input" in app.inspector.context_text
        assert app.inspector.file_texts == ("  M talaria/config.py",)
        assert "edit_file · talaria/config.py" in app.inspector.operation_text

        app.inspector.query_one(".inspector--file").focus()
        await pilot.press("enter")
        await pilot.pause()
        assert app.inspector.selected_file_key == "talaria/config.py"
        assert app.dispatcher is None
