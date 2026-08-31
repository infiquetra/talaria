"""The configurable, responsive true-bottom status bar.

The layout is a pure projection until :class:`BottomStatusBar` turns semantic
runs into Textual component styles. That split keeps terminal-width decisions
deterministic and testable without putting session or transport state in a
widget. Values that originated outside Talaria are defanged before display,
and every state keeps its ASCII glyph or count when colour is unavailable.
"""

from __future__ import annotations

import shutil
import subprocess  # nosec B404
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import ClassVar, Literal

from rich.cells import cell_len, get_character_cell_size
from rich.text import Text
from textual import events
from textual.widgets import Static

from talaria.domain.models import ConnectionStatus
from talaria.domain.projection import StatusPayload
from talaria.domain.queue import NeedsYouQueue
from talaria.status.contract import StatusBarSettings, StatusSegmentName
from talaria.ui.literal import defang

StatusToken = Literal["text", "muted", "separator", "success", "warning", "error", "attention"]
SegmentForm = Literal["full", "compact", "minimum"]
SegmentRenderer = Callable[["BottomStatusBarView", int], tuple["StatusRun", ...]]

SEPARATOR = "│"


@dataclass(frozen=True)
class StatusRun:
    """A literal string plus the bar-scoped semantic token it uses."""

    text: str
    token: StatusToken = "text"


@dataclass(frozen=True)
class BottomStatusBarView:
    """One immutable status snapshot assembled at the app render boundary."""

    cwd: str
    git_branch: str
    agent_provider: str
    agent_model: str
    input_tokens: int | None
    output_tokens: int | None
    context_window: int | None
    tasks_completed: int
    tasks_total: int
    attention_count: int
    connection: ConnectionStatus
    version: str


@dataclass(frozen=True)
class LocalStatus:
    """Working-directory facts captured once when the app is constructed."""

    cwd: str
    git_branch: str


@dataclass(frozen=True)
class SegmentSpec:
    """One named segment's priority, budgets, and three rendering forms."""

    name: StatusSegmentName
    priority: int
    full_max: int
    compact_max: int
    minimum_max: int
    full: SegmentRenderer
    compact: SegmentRenderer
    minimum: SegmentRenderer

    def renderer(self, form: SegmentForm) -> SegmentRenderer:
        if form == "full":
            return self.full
        if form == "compact":
            return self.compact
        return self.minimum


def build_status_bar_view(
    *,
    local: LocalStatus,
    status: StatusPayload,
    queue: NeedsYouQueue,
    agent_provider: str,
    agent_model: str,
    context_window: int | None,
    version: str,
) -> BottomStatusBarView:
    """Assemble the held runtime facts without reading a transport or filesystem."""
    completed = max(0, status.subagents_terminal)
    active = max(0, status.subagents_active)
    return BottomStatusBarView(
        cwd=local.cwd,
        git_branch=local.git_branch,
        agent_provider=agent_provider,
        agent_model=agent_model,
        input_tokens=status.input_tokens,
        output_tokens=status.output_tokens,
        context_window=context_window,
        tasks_completed=completed,
        tasks_total=completed + active,
        attention_count=queue.count,
        connection=status.connection,
        version=version,
    )


@dataclass(frozen=True)
class RenderedSegment:
    """One segment after a responsive form has been selected."""

    name: StatusSegmentName
    priority: int
    form: SegmentForm
    runs: tuple[StatusRun, ...]

    @property
    def plain(self) -> str:
        return "".join(run.text for run in self.runs)

    @property
    def width(self) -> int:
        return cell_len(self.plain)


@dataclass(frozen=True)
class StatusBarRender:
    """The final one-row rendering and the surviving segment identities."""

    segments: tuple[RenderedSegment, ...]
    runs: tuple[StatusRun, ...]

    @property
    def plain(self) -> str:
        return "".join(run.text for run in self.runs)

    @property
    def width(self) -> int:
        return cell_len(self.plain)


def capture_local_status(cwd: Path | None = None) -> LocalStatus:
    """Capture cwd and Git branch from local process state, with no polling.

    The Git command uses a fixed argument vector and is run once by the app.
    Outside a repository, or when Git is unavailable, the branch is honestly
    unknown rather than guessed from a directory name.
    """
    launch_cwd = (cwd if cwd is not None else Path.cwd()).resolve()
    git = shutil.which("git")
    if git is None:
        return LocalStatus(str(launch_cwd), "?")
    try:
        # The executable path and argument vector are fixed; no shell is involved.
        result = subprocess.run(  # nosec B603
            [git, "branch", "--show-current"],
            cwd=launch_cwd,
            check=False,
            capture_output=True,
            text=True,
            timeout=2.0,
        )
    except (OSError, subprocess.TimeoutExpired):
        return LocalStatus(str(launch_cwd), "?")
    branch = result.stdout.strip() if result.returncode == 0 else ""
    return LocalStatus(str(launch_cwd), branch or "detached")


def _take_prefix(value: str, width: int) -> str:
    if width <= 0:
        return ""
    used = 0
    kept: list[str] = []
    for character in value:
        size = max(0, get_character_cell_size(character))
        if used + size > width:
            break
        kept.append(character)
        used += size
    return "".join(kept)


def _take_suffix(value: str, width: int) -> str:
    if width <= 0:
        return ""
    used = 0
    kept: list[str] = []
    for character in reversed(value):
        size = max(0, get_character_cell_size(character))
        if used + size > width:
            break
        kept.append(character)
        used += size
    return "".join(reversed(kept))


def _middle_ellipsis(value: str, width: int) -> str:
    value = defang(value)
    if cell_len(value) <= width:
        return value
    if width <= 0:
        return ""
    if width == 1:
        return "…"
    remaining = width - 1
    left = (remaining + 1) // 2
    right = remaining - left
    return f"{_take_prefix(value, left)}…{_take_suffix(value, right)}"


def _trailing_ellipsis(value: str, width: int) -> str:
    value = defang(value)
    if cell_len(value) <= width:
        return value
    if width <= 0:
        return ""
    if width == 1:
        return "…"
    return f"{_take_prefix(value, width - 1)}…"


def _labelled(prefix: str, value: str, budget: int, *, middle: bool) -> tuple[StatusRun, ...]:
    value_budget = max(1, budget - cell_len(prefix))
    clipped = _middle_ellipsis(value, value_budget) if middle else _trailing_ellipsis(
        value, value_budget
    )
    return (StatusRun(prefix, "muted"), StatusRun(clipped))


def _cwd_full(view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    return _labelled("cwd: ", view.cwd or "?", budget, middle=True)


def _cwd_compact(view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    path = defang(view.cwd).rstrip("/\\")
    basename = path.replace("\\", "/").rsplit("/", 1)[-1] or path or "?"
    return (StatusRun(_middle_ellipsis(basename, budget)),)


def _cwd_minimum(view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    path = defang(view.cwd).rstrip("/\\")
    basename = path.replace("\\", "/").rsplit("/", 1)[-1] or "?"
    tail = _take_suffix(basename, max(1, budget - 2))
    return (StatusRun(f"…/{tail}"),)


def _git_full(view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    return _labelled("git: ", view.git_branch or "?", budget, middle=True)


def _git_compact(view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    return _labelled("git: ", view.git_branch or "?", budget, middle=True)


def _git_minimum(_view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    text = _trailing_ellipsis("git:…", budget)
    return (StatusRun(text[:4], "muted"), StatusRun(text[4:]))


def _agent_value(view: BottomStatusBarView, separator: str) -> str:
    provider = defang(view.agent_provider).strip()
    model = defang(view.agent_model).strip()
    if provider and model:
        return f"{provider}{separator}{model}"
    return provider or model or "?"


def _agent_full(view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    return _labelled("agent: ", _agent_value(view, " · "), budget, middle=True)


def _agent_compact(view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    return (StatusRun(_middle_ellipsis(_agent_value(view, "/"), budget)),)


def _agent_minimum(_view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    return (StatusRun(_trailing_ellipsis("agt…", budget)),)


def _short_count(value: int) -> str:
    value = max(0, value)
    if value < 1_000:
        return str(value)
    if value < 1_000_000:
        amount = value / 1_000
        return f"{amount:.0f}k" if amount >= 10 or amount.is_integer() else f"{amount:.1f}k"
    amount = value / 1_000_000
    return f"{amount:.0f}m" if amount >= 10 or amount.is_integer() else f"{amount:.1f}m"


def _context_values(view: BottomStatusBarView) -> tuple[str, str, str]:
    if view.input_tokens is None or view.output_tokens is None:
        return "?", _short_count(view.context_window) if view.context_window else "?", "?"
    used = max(0, view.input_tokens) + max(0, view.output_tokens)
    if not view.context_window or view.context_window <= 0:
        return _short_count(used), "?", "?"
    percent = round(used * 100 / view.context_window)
    return _short_count(used), _short_count(view.context_window), str(percent)


def _context_full(view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    used, limit, percent = _context_values(view)
    value = _trailing_ellipsis(f"{used}/{limit} {percent}%", max(1, budget - 9))
    return (StatusRun("context: ", "muted"), StatusRun(value))


def _context_compact(view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    _used, _limit, percent = _context_values(view)
    return (StatusRun(_trailing_ellipsis(f"ctx {percent}%", budget)),)


def _context_minimum(view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    _used, _limit, percent = _context_values(view)
    return (StatusRun(_trailing_ellipsis(f"ctx…{percent}%", budget)),)


def _task_parts(view: BottomStatusBarView) -> tuple[str, str]:
    completed = max(0, view.tasks_completed)
    total = max(completed, view.tasks_total)
    attention = f"!{max(0, view.attention_count)}" if view.attention_count > 0 else ""
    return f"{completed}/{total}", attention


def _task_runs(prefix: str, view: BottomStatusBarView) -> tuple[StatusRun, ...]:
    progress, attention = _task_parts(view)
    runs = [StatusRun(prefix, "muted"), StatusRun(progress)]
    if attention:
        runs.extend((StatusRun(" "), StatusRun(attention, "attention")))
    return tuple(runs)


def _task_full(view: BottomStatusBarView, _budget: int) -> tuple[StatusRun, ...]:
    return _task_runs("tasks: ", view)


def _task_compact(view: BottomStatusBarView, _budget: int) -> tuple[StatusRun, ...]:
    return _task_runs("task ", view)


def _task_minimum(view: BottomStatusBarView, _budget: int) -> tuple[StatusRun, ...]:
    progress, attention = _task_parts(view)
    return (StatusRun(attention, "attention"),) if attention else (StatusRun(progress),)


_CONNECTION_FORMS: dict[ConnectionStatus, tuple[str, str, str, StatusToken]] = {
    "connected": ("[ok] connected", "[ok] up", "[ok]", "success"),
    "connecting": ("[..] connecting", "[..] wait", "[..]", "warning"),
    "reconnecting": ("[~] reconnecting", "[~] retry", "[~]", "warning"),
    "disconnected": ("[x] disconnected", "[x] down", "[x]", "error"),
    "auth_failed": ("[!] authentication failed", "[!] auth", "[!]", "error"),
}


def _connection(view: BottomStatusBarView, form: SegmentForm) -> tuple[StatusRun, ...]:
    full, compact, minimum, token = _CONNECTION_FORMS[view.connection]
    text = full if form == "full" else compact if form == "compact" else minimum
    return (StatusRun(text, token),)


def _connection_full(view: BottomStatusBarView, _budget: int) -> tuple[StatusRun, ...]:
    return _connection(view, "full")


def _connection_compact(view: BottomStatusBarView, _budget: int) -> tuple[StatusRun, ...]:
    return _connection(view, "compact")


def _connection_minimum(view: BottomStatusBarView, _budget: int) -> tuple[StatusRun, ...]:
    return _connection(view, "minimum")


def _version_text(view: BottomStatusBarView) -> str:
    version = defang(view.version).strip() or "?"
    return version if version.startswith("v") else f"v{version}"


def _version_full(view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    return (StatusRun(_trailing_ellipsis(_version_text(view), budget)),)


def _version_minimum(_view: BottomStatusBarView, budget: int) -> tuple[StatusRun, ...]:
    return (StatusRun(_trailing_ellipsis("v…", budget)),)


SEGMENT_SPECS: tuple[SegmentSpec, ...] = (
    SegmentSpec("cwd", 10, 24, 18, 10, _cwd_full, _cwd_compact, _cwd_minimum),
    SegmentSpec("git_branch", 20, 18, 14, 5, _git_full, _git_compact, _git_minimum),
    SegmentSpec("agent_model", 50, 24, 18, 4, _agent_full, _agent_compact, _agent_minimum),
    SegmentSpec("context", 40, 22, 8, 8, _context_full, _context_compact, _context_minimum),
    SegmentSpec("task_progress", 80, 20, 12, 12, _task_full, _task_compact, _task_minimum),
    SegmentSpec(
        "connection",
        100,
        16,
        8,
        4,
        _connection_full,
        _connection_compact,
        _connection_minimum,
    ),
    SegmentSpec("version", 0, 8, 8, 2, _version_full, _version_full, _version_minimum),
)
SPEC_BY_NAME: dict[StatusSegmentName, SegmentSpec] = {spec.name: spec for spec in SEGMENT_SPECS}


def _maximum(spec: SegmentSpec, form: SegmentForm, settings: StatusBarSettings) -> int:
    if form == "full":
        maximum = spec.full_max
    elif form == "compact":
        maximum = spec.compact_max
    else:
        maximum = spec.minimum_max
    configured: int | None = None
    if spec.name == "cwd":
        configured = settings.cwd_max_columns
    elif spec.name == "git_branch":
        configured = settings.git_branch_max_columns
    elif spec.name == "agent_model":
        configured = settings.agent_model_max_columns
    if configured is None:
        return maximum
    return configured if form == "full" else min(configured, maximum)


def _breakpoint(width: int) -> tuple[SegmentForm, frozenset[StatusSegmentName]]:
    if width >= 144:
        return "full", frozenset()
    if width >= 120:
        return "compact", frozenset()
    if width >= 96:
        return "compact", frozenset({"version"})
    if width >= 80:
        return "compact", frozenset({"version", "cwd"})
    if width >= 64:
        return "compact", frozenset({"version", "cwd", "git_branch"})
    if width >= 48:
        return "compact", frozenset({"version", "cwd", "git_branch", "context"})
    if width >= 32:
        return "compact", frozenset(
            {"version", "cwd", "git_branch", "context", "agent_model"}
        )
    if width >= 20:
        return "compact", frozenset(
            {"version", "cwd", "git_branch", "context", "agent_model", "task_progress"}
        )
    return "minimum", frozenset(
        {"version", "cwd", "git_branch", "context", "agent_model", "task_progress"}
    )


def _render_segment(
    spec: SegmentSpec,
    form: SegmentForm,
    view: BottomStatusBarView,
    settings: StatusBarSettings,
) -> RenderedSegment:
    budget = _maximum(spec, form, settings)
    return RenderedSegment(spec.name, spec.priority, form, spec.renderer(form)(view, budget))


def _joined_runs(segments: Sequence[RenderedSegment]) -> tuple[StatusRun, ...]:
    joined: list[StatusRun] = []
    for index, segment in enumerate(segments):
        if index:
            joined.append(StatusRun(SEPARATOR, "separator"))
        joined.extend(segment.runs)
    return tuple(joined)


def _segments_width(segments: Sequence[RenderedSegment]) -> int:
    return sum(segment.width for segment in segments) + max(0, len(segments) - 1)


def _clip_runs(runs: Sequence[StatusRun], width: int) -> tuple[StatusRun, ...]:
    """Apply the final terminal-width safeguard without losing run semantics."""
    if width <= 0:
        return ()
    if sum(cell_len(run.text) for run in runs) <= width:
        return tuple(runs)

    remaining = max(0, width - 1)
    clipped: list[StatusRun] = []
    last_token: StatusToken = "text"
    for run in runs:
        if remaining <= 0:
            break
        prefix = _take_prefix(run.text, remaining)
        if prefix:
            clipped.append(StatusRun(prefix, run.token))
            last_token = run.token
            remaining -= cell_len(prefix)
        if cell_len(prefix) < cell_len(run.text):
            break
    clipped.append(StatusRun("…", last_token))
    return tuple(clipped)


def render_status_bar(
    view: BottomStatusBarView,
    width: int,
    settings: StatusBarSettings | None = None,
) -> StatusBarRender:
    """Render the fixed breakpoint and priority contract into one row."""
    resolved = settings if settings is not None else StatusBarSettings()
    initial_form, breakpoint_drops = _breakpoint(width)
    segments = [
        _render_segment(SPEC_BY_NAME[name], initial_form, view, resolved)
        for name in resolved.segments
        if name not in breakpoint_drops
    ]

    next_form: dict[SegmentForm, SegmentForm | None] = {
        "full": "compact",
        "compact": "minimum",
        "minimum": None,
    }
    while _segments_width(segments) > width:
        target = min(segments, key=lambda segment: segment.priority, default=None)
        if target is None:
            break
        form = next_form[target.form]
        if form is not None:
            replacement = _render_segment(SPEC_BY_NAME[target.name], form, view, resolved)
            segments[segments.index(target)] = replacement
            continue
        if target.name == "connection":
            break
        segments.remove(target)

    runs = _joined_runs(segments)
    if sum(cell_len(run.text) for run in runs) > width:
        runs = _clip_runs(runs, width)
    return StatusBarRender(tuple(segments), runs)


class BottomStatusBar(Static):
    """Exactly one true-bottom row, styled only with status-bar tokens."""

    COMPONENT_CLASSES: ClassVar[set[str]] = {
        "bottom-status--text",
        "bottom-status--muted",
        "bottom-status--separator",
        "bottom-status--success",
        "bottom-status--warning",
        "bottom-status--error",
        "bottom-status--attention",
    }

    DEFAULT_CSS = """
    BottomStatusBar {
        height: 1;
        min-height: 1;
        max-height: 1;
        width: 1fr;
        overflow: hidden hidden;
        text-wrap: nowrap;
        text-overflow: ellipsis;
        color: $talaria-status-text;
        background: $talaria-status-background;

        & > .bottom-status--text {
            color: $talaria-status-text;
            background: $talaria-status-background;
        }
        & > .bottom-status--muted {
            color: $talaria-status-muted;
            background: $talaria-status-background;
        }
        & > .bottom-status--separator {
            color: $talaria-status-separator;
            background: $talaria-status-background;
        }
        & > .bottom-status--success {
            color: $talaria-status-success;
            background: $talaria-status-background;
        }
        & > .bottom-status--warning {
            color: $talaria-status-warning;
            background: $talaria-status-background;
        }
        & > .bottom-status--error {
            color: $talaria-status-error;
            background: $talaria-status-background;
        }
        & > .bottom-status--attention {
            color: $talaria-status-attention;
            background: $talaria-status-background;
        }
    }
    """

    _COMPONENT_FOR_TOKEN: ClassVar[dict[StatusToken, str]] = {
        "text": "bottom-status--text",
        "muted": "bottom-status--muted",
        "separator": "bottom-status--separator",
        "success": "bottom-status--success",
        "warning": "bottom-status--warning",
        "error": "bottom-status--error",
        "attention": "bottom-status--attention",
    }

    def __init__(
        self,
        view: BottomStatusBarView,
        *,
        settings: StatusBarSettings | None = None,
        **kwargs: object,
    ) -> None:
        super().__init__("", markup=False, **kwargs)  # type: ignore[arg-type]
        self._view = view
        self._settings = settings if settings is not None else StatusBarSettings()
        self._last_render = StatusBarRender((), ())

    @property
    def view(self) -> BottomStatusBarView:
        return self._view

    @property
    def settings(self) -> StatusBarSettings:
        return self._settings

    @property
    def last_render(self) -> StatusBarRender:
        return self._last_render

    def apply(self, view: BottomStatusBarView) -> None:
        """Replace the immutable view and repaint without changing geometry."""
        if view == self._view:
            return
        self._view = view
        self.refresh()

    def toggle_segment(self, segment: str) -> str | None:
        """Apply the session-only ``/bar`` toggle; no configuration is written."""
        settings, notice = self._settings.toggled(segment)
        if settings != self._settings:
            self._settings = settings
            self.refresh()
        return notice

    def on_resize(self, _event: events.Resize) -> None:
        self.refresh()

    def render(self) -> Text:
        rendered = render_status_bar(self._view, self.size.width, self._settings)
        self._last_render = rendered
        parts = [
            (
                run.text,
                self.get_component_rich_style(self._COMPONENT_FOR_TOKEN[run.token]),
            )
            for run in rendered.runs
        ]
        return Text.assemble(*parts, no_wrap=True, overflow="ellipsis", end="")
