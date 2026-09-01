"""Issue #106: the one-row configurable responsive bottom status bar."""

from __future__ import annotations

import html
import re
from dataclasses import replace
from pathlib import Path

import pytest
from rich.cells import cell_len
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.widgets import Static

from talaria.domain.models_catalog import ModelProvider, ProviderCatalog
from talaria.domain.projection import StatusPayload
from talaria.domain.queue import NeedsYouQueue
from talaria.domain.session_list import decode_active_list
from talaria.domain.state import apply_active_list
from talaria.status.contract import StatusBarSettings, normalize_status_segments
from talaria.status.local import LocalStatus
from talaria.ui.literal import INVISIBLE_MARK, defang
from talaria.ui.status_bar import (
    BottomStatusBar,
    BottomStatusBarView,
    _breakpoint,
    build_status_bar_view,
    render_status_bar,
)
from tests.ui.conftest import RecordingDispatcher, event, live_app, paused_app


def _screen_text(app: App[None]) -> str:
    body = re.sub(r"<[^>]+>", "", app.export_screenshot())
    return html.unescape(body).replace("\xa0", " ")


def _view(**overrides: object) -> BottomStatusBarView:
    fields: dict[str, object] = {
        "cwd": "/Users/example/workspace/orch-design-codex",
        "git_branch": "orch/talaria-v0-5-0",
        "agent_provider": "Muse",
        "agent_model": "Spark 1.2",
        "input_tokens": 32_000,
        "output_tokens": 0,
        "context_window": 128_000,
        "tasks_completed": 3,
        "tasks_total": 7,
        "attention_count": 1,
        "connection": "connected",
        "version": "0.5.0",
    }
    fields.update(overrides)
    return BottomStatusBarView(**fields)  # type: ignore[arg-type]


class StatusBarHarness(App[None]):
    """A real Textual screen with flexible body and the bar on its last row."""

    CSS = """
    Screen {
        layout: vertical;
    }
    #body {
        height: 1fr;
    }
    """

    def __init__(
        self,
        view: BottomStatusBarView,
        settings: StatusBarSettings | None = None,
    ) -> None:
        super().__init__()
        self._view = view
        self._settings = settings

    def get_theme_variable_defaults(self) -> dict[str, str]:
        return {
            "talaria-status-background": "#010203",
            "talaria-status-text": "#111213",
            "talaria-status-muted": "#212223",
            "talaria-status-separator": "#313233",
            "talaria-status-success": "#41A243",
            "talaria-status-warning": "#B18223",
            "talaria-status-error": "#C14243",
            "talaria-status-attention": "#5182D3",
        }

    def compose(self) -> ComposeResult:
        yield Static("", id="body")
        yield BottomStatusBar(self._view, settings=self._settings, id="bottom-status")

    @property
    def bar(self) -> BottomStatusBar:
        return self.query_one("#bottom-status", BottomStatusBar)


@pytest.mark.asyncio
async def test_all_seven_segments_follow_configured_order_and_hidden_rows_stay_absent() -> None:
    settings = StatusBarSettings(
        segments=("connection", "task_progress", "cwd", "version", "agent_model")
    )
    app = StatusBarHarness(_view(), settings)

    async with app.run_test(size=(180, 24)) as pilot:
        await pilot.pause()
        line = app.bar.last_render.plain
        visible = _screen_text(app)

        assert line == (
            "[ok] connected│tasks: 3/7 !1│cwd: /Users/ex…ign-codex"
            "│v0.5.0│agent: Muse · Spark 1.2"
        )
        assert "[ok] connected" in visible
        assert "tasks: 3/7" in visible
        assert "!1" in visible
        assert "git:" not in line
        assert "context:" not in line
        assert line.count("│") == 4


def test_unknown_and_duplicate_config_rows_are_reported_without_hiding_known_rows() -> None:
    segments, notices = normalize_status_segments(
        ["version", "unknown", "connection", "version", 7]
    )

    assert segments == ("version", "connection")
    assert len(notices) == 3
    assert sum("unknown segment" in notice for notice in notices) == 2
    assert sum("duplicate" in notice for notice in notices) == 1

    fallback, fallback_notices = normalize_status_segments([])
    assert fallback == ("connection",)
    assert "connection only" in fallback_notices[-1]

    defaults, default_notices = normalize_status_segments("cwd,connection")
    assert defaults == StatusBarSettings().segments
    assert "default order" in default_notices[0]


def test_each_unknown_configured_segment_is_named_in_its_own_notice() -> None:
    segments, notices = normalize_status_segments(
        ["first_unknown", "connection", "second_unknown"]
    )

    assert segments == ("connection",)
    assert len(notices) == 2
    assert notices[0] != notices[1]
    assert "first_unknown" in notices[0]
    assert "second_unknown" in notices[1]


@pytest.mark.parametrize(
    ("hazard", "marker"),
    [
        pytest.param("\x00", "␀", id="null"),
        pytest.param("\x1b", "␛", id="escape"),
        pytest.param("\u202e", INVISIBLE_MARK, id="right-to-left-override"),
        pytest.param("\u200d", INVISIBLE_MARK, id="zero-width-joiner"),
        pytest.param("\ufeff", INVISIBLE_MARK, id="byte-order-mark"),
        pytest.param("\u00ad", INVISIBLE_MARK, id="soft-hyphen"),
        pytest.param("\U000e0072", INVISIBLE_MARK, id="unicode-tag-letter"),
        pytest.param("\u2066", INVISIBLE_MARK, id="left-to-right-isolate"),
    ],
)
def test_unknown_status_segments_use_the_canonical_defang_rule(
    hazard: str, marker: str
) -> None:
    unsafe = f"unsafe{hazard}segment"
    _segments, notices = normalize_status_segments([unsafe, "connection"])

    assert len(notices) == 1
    assert repr(defang(unsafe)) in notices[0]
    assert marker in notices[0]


_ALL_SEGMENTS = (
    "cwd",
    "git_branch",
    "agent_model",
    "context",
    "task_progress",
    "connection",
    "version",
)
_WITHOUT_VERSION = _ALL_SEGMENTS[:-1]

_REPO_ROOT = Path(__file__).resolve().parents[2]

_TRANSITIONS = (
    (144, _ALL_SEGMENTS, "full"),
    (143, _ALL_SEGMENTS, "compact"),
    (120, _ALL_SEGMENTS, "compact"),
    (119, _WITHOUT_VERSION, "compact"),
    (112, _WITHOUT_VERSION, "compact"),
    (111, _WITHOUT_VERSION, "compact"),
    (96, _WITHOUT_VERSION, "compact"),
    (95, ("git_branch", "agent_model", "context", "task_progress", "connection"), "compact"),
    (80, ("git_branch", "agent_model", "context", "task_progress", "connection"), "compact"),
    (79, ("agent_model", "context", "task_progress", "connection"), "compact"),
    (64, ("agent_model", "context", "task_progress", "connection"), "compact"),
    (63, ("agent_model", "task_progress", "connection"), "compact"),
    (48, ("agent_model", "task_progress", "connection"), "compact"),
    (47, ("task_progress", "connection"), "compact"),
    (32, ("task_progress", "connection"), "compact"),
    (31, ("task_progress", "connection"), "compact"),
    (20, ("task_progress", "connection"), "compact"),
    (19, ("connection",), "minimum"),
)


@pytest.mark.asyncio
async def test_complete_breakpoint_walk_reflows_in_place_and_never_grows_a_second_row() -> None:
    app = StatusBarHarness(_view())

    async with app.run_test(size=(144, 24)) as pilot:
        for width, expected_names, expected_form in _TRANSITIONS:
            await pilot.resize_terminal(width, 24)
            await pilot.pause()
            rendered = app.bar.last_render

            assert tuple(segment.name for segment in rendered.segments) == expected_names
            if isinstance(expected_form, tuple):
                assert tuple(segment.form for segment in rendered.segments) == expected_form
            else:
                assert all(segment.form == expected_form for segment in rendered.segments)
            assert rendered.width <= width
            assert cell_len(rendered.plain) <= width
            assert app.bar.region.height == 1
            assert app.bar.region.y == 23, f"the {width}-column bar left the true bottom row"


def test_overflow_shortens_then_drops_each_lower_priority_segment_before_the_next() -> None:
    settings = StatusBarSettings(
        cwd_max_columns=48,
        git_branch_max_columns=40,
        agent_model_max_columns=48,
    )
    hostile = _view(
        cwd="/" + "wide-directory-" * 8,
        git_branch="feature/" + "long-branch-" * 8,
        agent_provider="Provider" * 8,
        agent_model="Model" * 8,
    )

    rendered = render_status_bar(hostile, 144, settings)
    names = tuple(segment.name for segment in rendered.segments)

    assert "version" not in names, "the lowest-priority row was not dropped after shortening"
    assert "cwd" not in names, "the next-lowest row was not handled before higher-priority rows"
    assert "connection" in names
    assert rendered.width <= 144


def test_attention_marker_survives_at_thirty_one_columns_when_queue_is_nonempty() -> None:
    rendered = render_status_bar(_view(attention_count=2), 31)

    assert tuple(segment.name for segment in rendered.segments) == (
        "task_progress",
        "connection",
    )
    assert "3/7" in rendered.plain
    assert "!2" in rendered.plain
    assert any(run.text == "!2" and run.token == "attention" for run in rendered.runs)


def test_twenty_through_thirty_two_columns_keep_all_required_compact_content() -> None:
    required = "task 3/7 !1│[ok] up"

    for width in range(20, 33):
        rendered = render_status_bar(_view(), width)

        assert abs(rendered.width - cell_len(required)) <= 1, width
        assert rendered.plain == required


@pytest.mark.parametrize(
    ("width", "expected_names"),
    [
        (144, _ALL_SEGMENTS),
        (132, _ALL_SEGMENTS),
        (112, _WITHOUT_VERSION),
        (96, _WITHOUT_VERSION),
        (80, ("git_branch", "agent_model", "context", "task_progress", "connection")),
        (64, ("agent_model", "context", "task_progress", "connection")),
        (48, ("agent_model", "task_progress", "connection")),
        (32, ("task_progress", "connection")),
        (20, ("task_progress", "connection")),
        (19, ("connection",)),
    ],
)
def test_documented_default_breakpoint_bands_match_rendered_segments(
    width: int, expected_names: tuple[str, ...]
) -> None:
    rendered = render_status_bar(_view(), width)

    assert tuple(segment.name for segment in rendered.segments) == expected_names


def _derived_breakpoint_rows() -> tuple[tuple[str, tuple[object, object]], ...]:
    bands: list[tuple[int, int | None, tuple[object, object]]] = []
    band_start = 0
    prior = _breakpoint(0)
    for width in range(1, 4097):
        current = _breakpoint(width)
        if current == prior:
            continue
        bands.append((band_start, width - 1, prior))
        band_start = width
        prior = current
    bands.append((band_start, None, prior))

    rows: list[tuple[str, tuple[object, object]]] = []
    for start, end, result in reversed(bands):
        if end is None:
            label = f"{start} and wider"
        elif start == 0:
            label = f"Fewer than {end + 1}"
        else:
            label = f"{start}–{end}"
        rows.append((label, result))
    return tuple(rows)


def _documented_breakpoint_rows() -> tuple[tuple[str, tuple[object, object]], ...]:
    guide = (_REPO_ROOT / "docs" / "terminal-ui.md").read_text(encoding="utf-8")
    lines = guide.splitlines()
    header = lines.index("| Width | Default result |")
    dropped: set[str] = set()
    rows: list[tuple[str, tuple[object, object]]] = []
    for line in lines[header + 2 :]:
        if not line.startswith("|"):
            break
        label, description = (cell.strip() for cell in line.strip("|").split("|", 1))
        dropped_match = re.search(r"[Dd]rop `([^`]+)`", description)
        if dropped_match is not None:
            dropped.add(dropped_match.group(1))
        form = "minimum" if "minimum `connection` form" in description else "compact"
        if description == "All seven segments in full form":
            form = "full"
        rows.append((label, (form, frozenset(dropped))))
    return tuple(rows)


def test_documented_default_breakpoint_bands_are_derived_from_renderer() -> None:
    assert _documented_breakpoint_rows() == _derived_breakpoint_rows()


@pytest.mark.parametrize(
    ("state", "full", "compact", "minimum", "token"),
    [
        ("connected", "[ok] connected", "[ok] up", "[ok]", "success"),
        ("connecting", "[..] connecting", "[..] wait", "[..]", "warning"),
        ("reconnecting", "[~] reconnecting", "[~] retry", "[~]", "warning"),
        ("disconnected", "[x] disconnected", "[x] down", "[x]", "error"),
        ("auth_failed", "[!] authentication failed", "[!] auth", "[!]", "error"),
    ],
)
def test_every_connection_state_keeps_its_ascii_form_and_bar_semantic_token(
    state: str,
    full: str,
    compact: str,
    minimum: str,
    token: str,
) -> None:
    settings = StatusBarSettings(segments=("connection",))
    view = _view(connection=state)

    full_render = render_status_bar(view, 180, settings)
    compact_render = render_status_bar(view, 120, settings)
    minimum_render = render_status_bar(view, 19, settings)

    assert full_render.plain == full
    assert compact_render.plain == compact
    assert minimum_render.plain == minimum
    assert full_render.runs[0].token == token
    assert compact_render.runs[0].token == token
    assert minimum_render.runs[0].token == token


@pytest.mark.asyncio
async def test_live_reconnecting_transition_repaints_the_status_form_immediately() -> None:
    """The transport callback must paint a transient reconnect before a
    following dial state can replace it; the coalescing timer is too late.
    """
    app = live_app(RecordingDispatcher())

    async with app.run_test(size=(180, 24)) as pilot:
        app.note_connection_state("connected")
        await app._render_tick()
        await pilot.pause()
        assert "[ok] connected" in app.bottom_status_bar.last_render.plain

        app.note_connection_state("reconnecting")
        await pilot.pause()

        assert app.state.connection == "reconnecting"
        assert "connection lost — reconnecting" in app.composer.notice
        assert app.bottom_status_bar.view.connection == "reconnecting"
        assert "[~] reconnecting" in app.bottom_status_bar.last_render.plain
        assert "[~] reconnecting" in _screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_connection_and_attention_use_only_bar_tokens_on_the_bar_background() -> None:
    app = StatusBarHarness(
        _view(), StatusBarSettings(segments=("task_progress", "connection"))
    )

    async with app.run_test(size=(180, 24)) as pilot:
        await pilot.pause()
        rendered = app.bar.render()
        attention_at = rendered.plain.index("!1")
        connection_at = rendered.plain.index("[ok]")
        attention_style = rendered.get_style_at_offset(app.console, attention_at)
        connection_style = rendered.get_style_at_offset(app.console, connection_at)

        assert attention_style.color is not None
        assert connection_style.color is not None
        assert attention_style.bgcolor is not None
        assert connection_style.bgcolor is not None
        assert attention_style.color.get_truecolor().hex == "#5182d3"
        assert connection_style.color.get_truecolor().hex == "#41a243"
        assert attention_style.bgcolor.get_truecolor().hex == "#010203"
        assert connection_style.bgcolor.get_truecolor().hex == "#010203"


def test_hostile_values_are_literal_bounded_and_keep_identity_ends() -> None:
    view = _view(
        cwd="/repo/[red]unsafe[/red]\x1b[2J-tail",
        git_branch="feature/left-middle-right",
        agent_provider="[green]provider[/green]",
        agent_model="model\u202eright",
    )
    rendered = render_status_bar(
        view,
        180,
        StatusBarSettings(
            cwd_max_columns=48,
            git_branch_max_columns=40,
            agent_model_max_columns=48,
        ),
    )

    assert "[red]" in rendered.plain
    assert "␛[2J" in rendered.plain
    assert "�" in rendered.plain
    assert rendered.width <= 180
    assert "feature/" in rendered.plain
    assert "right" in rendered.plain


@pytest.mark.asyncio
async def test_session_toggle_changes_only_the_held_segment_set() -> None:
    app = StatusBarHarness(
        _view(), StatusBarSettings(segments=("cwd", "connection", "version"))
    )

    async with app.run_test(size=(180, 24)) as pilot:
        await pilot.pause()
        original_view = app.bar.view

        assert app.bar.toggle_segment("cwd") is None
        await pilot.pause()
        assert "cwd:" not in app.bar.last_render.plain

        assert app.bar.toggle_segment("cwd") is None
        await pilot.pause()
        assert app.bar.last_render.plain.endswith("cwd: /Users/ex…ign-codex")
        assert app.bar.view == original_view

        before = app.bar.settings
        assert app.bar.toggle_segment("made-up") == "bar: unknown segment made-up"
        assert app.bar.settings == before


@pytest.mark.asyncio
async def test_missing_context_and_model_facts_render_unknown_without_dispatching() -> None:
    dispatcher = RecordingDispatcher()
    app, _controls = paused_app(
        [],
        dispatcher=dispatcher,
        status_bar_settings=StatusBarSettings(
            segments=("agent_model", "context", "connection")
        ),
    )

    async with app.run_test(size=(180, 24)) as pilot:
        await pilot.pause()

        assert app.bottom_status_bar.last_render.plain == (
            "agent: ?│context: ?/? ?%│[x] disconnected"
        )
        assert dispatcher.operator_calls == []


@pytest.mark.asyncio
async def test_live_registry_model_replaces_unknown_after_the_first_render() -> None:
    app, _controls = paused_app(
        [],
        status_bar_settings=StatusBarSettings(segments=("agent_model", "connection")),
    )
    app.state = replace(app.state, focused_session_id="live-primary")

    async with app.run_test(size=(160, 36)) as pilot:
        await pilot.pause()
        assert app.bottom_status_bar.view.agent_model == ""
        assert "agent: ?" in app.bottom_status_bar.last_render.plain

        app.fleet = apply_active_list(
            app.fleet,
            decode_active_list(
                {
                    "sessions": [
                        {
                            "id": "live-primary",
                            "status": "idle",
                            "model": "muse-spark-1.2-contributor",
                        }
                    ]
                }
            ),
            profile=app.fleet_profile,
            at=1_785_000_001.0,
            poll_epoch=1,
        )
        app._dirty = True
        await app._render_tick()
        await pilot.pause()

        assert app.bottom_status_bar.view.agent_model == "muse-spark-1.2-contributor"
        assert "agent: muse-spa…tributor" in app.bottom_status_bar.last_render.plain
        assert "agent: ?" not in app.bottom_status_bar.last_render.plain


@pytest.mark.asyncio
async def test_roster_provider_prefix_is_status_only() -> None:
    model = "claude-opus-4"
    app, _controls = paused_app(
        [],
        status_bar_settings=StatusBarSettings(
            segments=("agent_model",),
            agent_model_max_columns=48,
        ),
    )
    app.state = replace(app.state, focused_session_id="live-primary")
    mismatched_catalog = ProviderCatalog(
        providers=(
            ModelProvider(
                slug="anthropic",
                name="Anthropic",
                models=(model, "different-model"),
                authenticated=True,
            ),
        ),
        current_provider="anthropic",
        current_model="different-model",
    )
    app.model_catalog = mismatched_catalog
    app.fleet = apply_active_list(
        app.fleet,
        decode_active_list(
            {
                "sessions": [
                    {
                        "id": "live-primary",
                        "status": "idle",
                        "model": model,
                    }
                ]
            }
        ),
        profile=app.fleet_profile,
        at=1_785_000_001.0,
        poll_epoch=1,
    )

    assert app._focused_agent_identity() == ("", model, False)

    app.model_catalog = replace(mismatched_catalog, current_model=model)
    async with app.run_test(size=(180, 30)) as pilot:
        await app.render_snapshot()
        await pilot.pause()

        assert app.bottom_status_bar.last_render.plain == (
            "agent: Anthropic · claude-opus-4"
        )
        assert "model    claude-opus-4" in app.inspector.context_text
        assert "model    Anthropic/claude-opus-4" not in app.inspector.context_text
        await app.shutdown_sources()


def test_runtime_view_reuses_held_status_and_queue_facts() -> None:
    status = StatusPayload(
        version=1,
        mode="live",
        connection="reconnecting",
        session_id="session-1",
        session_title="fixture",
        turn="streaming",
        pending_prompts=1,
        subagents_active=4,
        subagents_terminal=3,
        input_tokens=31_000,
        output_tokens=1_000,
    )

    view = build_status_bar_view(
        local=LocalStatus("/repo", "feature/status"),
        status=status,
        queue=NeedsYouQueue(),
        agent_provider="Muse",
        agent_model="Spark 1.2",
        context_window=128_000,
        version="0.5.0",
    )

    assert view.tasks_completed == 3
    assert view.tasks_total == 7
    assert view.attention_count == 0
    assert view.connection == "reconnecting"
    assert view.input_tokens == 31_000
    assert view.output_tokens == 1_000


@pytest.mark.asyncio
async def test_running_app_composes_help_above_the_true_bottom_status_row() -> None:
    app, _controls = paused_app([event("gateway.ready", {})])

    async with app.run_test(size=(100, 24)) as pilot:
        await pilot.pause()

        assert app.help_bar.region.y == 22
        assert app.bottom_status_bar.region.y == 23
        assert app.bottom_status_bar.region.height == 1
        assert not list(app.query("#needs-you"))
        assert "[x]" in app.bottom_status_bar.last_render.plain


@pytest.mark.asyncio
async def test_bar_command_toggles_one_known_segment_only_in_the_running_session() -> None:
    app, _controls = paused_app([event("gateway.ready", {})])

    async with app.run_test(size=(180, 24)) as pilot:
        await pilot.pause()
        configured = app.status_bar_settings
        assert "cwd" in app.bottom_status_bar.settings.segments

        app.composer.text = "/bar cwd"
        await pilot.press("enter")
        await pilot.pause()
        assert "cwd" not in app.bottom_status_bar.settings.segments
        assert "cwd:" not in app.bottom_status_bar.last_render.plain
        assert "hidden for this session" in app.composer.notice
        assert app.status_bar_settings is configured

        before = app.bottom_status_bar.settings
        app.composer.text = "/bar made-up"
        await pilot.press("enter")
        await pilot.pause()
        assert app.bottom_status_bar.settings == before
        assert "unknown segment" in app.composer.notice
        assert all(
            not isinstance(binding, Binding) or binding.action != "bar"
            for binding in app.BINDINGS
        )
