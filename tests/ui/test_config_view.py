"""The ``/config`` view: effective values, sources, and the allowlist write.

Issue #149, D8 recorded. The screen's own contract — the rows, the readonly
rules, the chooser, and the write — is driven through a minimal host app,
following ``tests/ui/test_dialog.py``, with the values a real process would
have resolved at startup passed in explicitly (the screen takes parameters,
not a fresh file read, because restart-to-apply is the contract the rows
display). The mounting seam's tests against the real
:class:`~talaria.ui.app.TalariaApp` live at the bottom of this file.

The writes go through the real :func:`~talaria.config.save_status_settings`
against the autouse ``isolated_global_config_dir`` redirection, so every
"nothing was written" assertion is about bytes on disk, not a stub.
"""

from __future__ import annotations

from collections.abc import Callable, Mapping
from pathlib import Path

import pytest
from textual.app import App, ComposeResult
from textual.pilot import Pilot
from textual.widgets import Input, Static

from talaria.config import setting_scopes
from talaria.domain.commands import LocalInvocation, resolve_command
from talaria.ui.config_view import ConfigViewResult, ConfigViewScreen
from tests.ui.conftest import event, paused_app, screen_text

#: The startup bundle every render-and-write test passes: what a process
#: launched on ``USER_CONFIG`` below would have resolved.
THEME = "refined-default"
COMMAND = "git status --short"
INTERVAL = 5
SEGMENTS = ("cwd", "version")

#: The size the screen is driven at: tall enough that the whole modal,
#: chooser included, is on screen for clicks.
SIZE = (100, 44)


def _default_user_bytes() -> bytes:
    """The bundle's user file, as bytes, for byte-exact write assertions."""
    return (
        "[status]\n"
        f'command = "{COMMAND}"\n'
        f"interval_seconds = {INTERVAL}\n"
        f'segments = ["{SEGMENTS[0]}", "{SEGMENTS[1]}"]\n'
        f"[theme]\nname = \"{THEME}\"\n"
    ).encode()


class _Host(App[None]):
    """The minimal host that mounts the screen the way the seam will."""

    def __init__(self, factory: Callable[[], ConfigViewScreen]) -> None:
        super().__init__()
        self._factory = factory
        self.result: ConfigViewResult | None = None
        self.view: ConfigViewScreen | None = None

    def compose(self) -> ComposeResult:
        yield Static("behind the modal")

    def open_config(self) -> None:
        self.view = self._factory()
        self.push_screen(self.view, self._capture)

    def _capture(self, result: ConfigViewResult | None) -> None:
        self.result = result


def _write_user_config(isolated_global_config_dir: Path, text: str = "") -> Path:
    """Place a user configuration file; defaults to the bundle's home."""
    if not text:
        text = (
            "[status]\n"
            f'command = "{COMMAND}"\n'
            f"interval_seconds = {INTERVAL}\n"
            f'segments = ["{SEGMENTS[0]}", "{SEGMENTS[1]}"]\n'
            f"[theme]\nname = \"{THEME}\"\n"
        )
    path = isolated_global_config_dir / "config.toml"
    path.write_text(text, encoding="utf-8")
    return path


def _scopes() -> dict[tuple[str, str], str]:
    return setting_scopes(cwd=Path.cwd())


def _factory(
    scopes: Mapping[tuple[str, str], str],
    *,
    command: str = COMMAND,
    interval: int = INTERVAL,
    segments_now: tuple[str, ...] = (),
) -> Callable[[], ConfigViewScreen]:
    def build() -> ConfigViewScreen:
        return ConfigViewScreen(
            theme_name=THEME,
            status_command=command,
            status_interval_seconds=interval,
            status_segments=SEGMENTS,
            scopes=scopes,
            segments_now=segments_now,
        )

    return build


async def _mounted(pilot: Pilot[None], host: _Host) -> ConfigViewScreen:
    host.open_config()
    for _ in range(3):
        await pilot.pause()
    assert host.view is not None
    return host.view


def _interval_input(view: ConfigViewScreen) -> Input:
    return view.query_one("#interval", Input)


def _command_input(view: ConfigViewScreen) -> Input:
    return view.query_one("#command", Input)


@pytest.mark.asyncio
async def test_rows_render_effective_values_sources_and_modes(
    isolated_global_config_dir: Path,
) -> None:
    _write_user_config(isolated_global_config_dir)
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)

        assert view.theme_line_text == (
            f"theme.name {THEME} · source: user · live"
        )
        assert view.command_sub_text == (
            f"effective: {COMMAND} · source: user · mode: restart"
        )
        assert view.interval_sub_text == (
            f"effective: {INTERVAL} · source: user · mode: restart"
        )
        assert view.segments_sub_text == (
            f"effective: {SEGMENTS[0]}, {SEGMENTS[1]} · "
            "source: user · mode: restart"
        )
        assert view.chooser_row_texts == (
            f"[x] {SEGMENTS[0]}",
            f"[x] {SEGMENTS[1]}",
            "[ ] git_branch",
            "[ ] agent_model",
            "[ ] context",
            "[ ] task_progress",
            "[ ] connection",
        )


@pytest.mark.asyncio
async def test_environment_sourced_rows_render_read_only_with_the_reason(
    isolated_global_config_dir: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    _write_user_config(isolated_global_config_dir)
    monkeypatch.setenv("TALARIA_STATUS_INTERVAL_SECONDS", "30")
    host = _Host(_factory(_scopes(), interval=30))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)

        assert _interval_input(view).disabled is True
        assert view.interval_sub_text == (
            "set by environment variable TALARIA_STATUS_INTERVAL_SECONDS — "
            "edit the environment; a file write would be shadowed"
        )


@pytest.mark.asyncio
async def test_an_invalid_interval_is_rejected_inline_and_nothing_is_written(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(isolated_global_config_dir)
    original = user_config.read_bytes()
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _interval_input(view).value = "0"

        await pilot.click("#apply-user")
        await pilot.pause()

        assert view.notice_text == (
            "status.interval_seconds must be an integer between "
            "1 and 3600 — nothing written"
        )
        assert user_config.read_bytes() == original


@pytest.mark.parametrize("bad", ["0", "3601", "fast", ""])
@pytest.mark.asyncio
async def test_every_invalid_interval_shape_is_rejected_inline(
    isolated_global_config_dir: Path, bad: str
) -> None:
    _write_user_config(isolated_global_config_dir)
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _interval_input(view).value = bad

        await pilot.click("#apply-user")
        await pilot.pause()

        assert "must be an integer between" in view.notice_text
        assert "nothing written" in view.notice_text


@pytest.mark.asyncio
async def test_apply_writes_only_the_changed_keys(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(isolated_global_config_dir)
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _interval_input(view).value = "17"

        await pilot.click("#apply-user")
        await pilot.pause()

        assert user_config.read_bytes() == _default_user_bytes().replace(
            b"interval_seconds = 5", b"interval_seconds = 17"
        )
        assert view.interval_sub_text == (
            "saved: 17 · effective now: 5 · takes effect on restart"
        )
        assert view.notice_text == "saved to user configuration"
        # The rows the apply did not write keep their pre-apply reading.
        assert view.command_sub_text == (
            f"effective: {COMMAND} · source: user · mode: restart"
        )


@pytest.mark.asyncio
async def test_apply_with_nothing_changed_writes_nothing(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(isolated_global_config_dir)
    original = user_config.read_bytes()
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)

        await pilot.click("#apply-user")
        await pilot.pause()

        assert view.notice_text == "nothing changed — no write"
        assert user_config.read_bytes() == original


@pytest.mark.asyncio
async def test_escape_writes_nothing_and_dismisses_without_a_notice(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(isolated_global_config_dir)
    original = user_config.read_bytes()
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _interval_input(view).value = "17"

        await pilot.press("escape")
        await pilot.pause()

        assert host.result is None
        assert user_config.read_bytes() == original


@pytest.mark.asyncio
async def test_cancel_writes_nothing_and_dismisses(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(isolated_global_config_dir)
    original = user_config.read_bytes()
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _command_input(view).value = "echo edited"

        await pilot.click("#cancel")
        await pilot.pause()

        assert host.result is None
        assert user_config.read_bytes() == original


@pytest.mark.asyncio
async def test_a_repo_sourced_row_refuses_a_user_apply_and_saves_to_repository(
    isolated_global_config_dir: Path,
) -> None:
    _write_user_config(isolated_global_config_dir)
    repo_config = Path.cwd() / ".talaria" / "config.toml"
    repo_config.parent.mkdir(exist_ok=True)
    repo_config.write_text('[status]\ncommand = "repo-status"\n', encoding="utf-8")
    original_repo = repo_config.read_bytes()
    host = _Host(_factory(_scopes(), command="repo-status"))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        assert view.command_sub_text == (
            "effective: repo-status · source: repository file · mode: restart · "
            "a user-file save would be shadowed — save to repository instead"
        )
        _command_input(view).value = "repo-new"

        await pilot.click("#apply-user")
        await pilot.pause()
        assert "set by the repository file" in view.notice_text
        assert "user-file write would be shadowed" in view.notice_text
        assert repo_config.read_bytes() == original_repo

        await pilot.click("#apply-repository")
        await pilot.pause()
        assert repo_config.read_bytes() == b'[status]\ncommand = "repo-new"\n'
        assert view.command_sub_text == (
            "saved: repo-new · effective now: repo-status · takes effect on restart"
        )


@pytest.mark.asyncio
async def test_the_segments_row_names_the_session_layer_when_a_bar_toggle_diverged(
    isolated_global_config_dir: Path,
) -> None:
    _write_user_config(isolated_global_config_dir)
    host = _Host(_factory(_scopes(), segments_now=(SEGMENTS[0],)))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)

        assert view.segments_sub_text == (
            f"effective now: {SEGMENTS[0]} · source: session (/bar) · mode: restart"
        )


@pytest.mark.asyncio
async def test_the_chooser_reorders_with_shift_arrows_and_toggles_with_space(
    isolated_global_config_dir: Path,
) -> None:
    _write_user_config(isolated_global_config_dir)
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.query_one("#segments").focus()
        await pilot.pause()

        # Focus opens on the first shown row and follows the *name*.
        assert view.chooser_active_row_text == f"[x] {SEGMENTS[0]}"

        await pilot.press("shift+down")
        await pilot.pause()
        assert view.chooser_selected == (SEGMENTS[1], SEGMENTS[0])
        assert view.chooser_active_row_text == f"[x] {SEGMENTS[0]}"

        await pilot.press("space")
        await pilot.pause()
        after_toggle: tuple[str, ...] = view.chooser_selected
        assert after_toggle == (SEGMENTS[1],)
        assert view.chooser_active_row_text == f"[ ] {SEGMENTS[0]}"

        await pilot.press("up")
        await pilot.pause()
        assert view.chooser_active_row_text == f"[x] {SEGMENTS[1]}"

        # Space toggles the *focused* row, so walk back onto the unselected
        # row before toggling it back on — the focus-follows-the-name rule
        # that makes the chooser usable one row at a time.
        await pilot.press("down")
        await pilot.pause()
        assert view.chooser_active_row_text == f"[ ] {SEGMENTS[0]}"

        await pilot.press("space")
        await pilot.pause()
        after_readd: tuple[str, ...] = view.chooser_selected
        assert after_readd == (SEGMENTS[1], SEGMENTS[0])
        assert view.chooser_active_row_text == f"[x] {SEGMENTS[0]}"


@pytest.mark.asyncio
async def test_apply_saves_the_reordered_segment_selection(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(isolated_global_config_dir)
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.query_one("#segments").focus()
        await pilot.pause()
        await pilot.press("shift+down")
        await pilot.pause()

        await pilot.click("#apply-user")
        await pilot.pause()

        assert user_config.read_bytes() == _default_user_bytes().replace(
            b'segments = ["cwd", "version"]',
            b'segments = [\n  "version",\n  "cwd",\n]',
        )
        assert view.segments_sub_text == (
            "saved: version, cwd · effective now: cwd, version · "
            "takes effect on restart"
        )


@pytest.mark.asyncio
async def test_an_empty_command_saves_as_the_explicit_empty_value(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(isolated_global_config_dir)
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _command_input(view).value = ""

        await pilot.click("#apply-user")
        await pilot.pause()

        assert user_config.read_bytes() == _default_user_bytes().replace(
            f'command = "{COMMAND}"'.encode(), b'command = ""'
        )
        assert view.command_sub_text == (
            "saved: (no status script) · effective now: "
            f"{COMMAND} · takes effect on restart"
        )


@pytest.mark.asyncio
async def test_the_edit_by_hand_refusal_surfaces_in_the_notice(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(
        isolated_global_config_dir, 'status = { command = "inline" }\n'
    )
    original = user_config.read_bytes()
    host = _Host(_factory(_scopes(), command="inline"))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _command_input(view).value = "new"

        await pilot.click("#apply-user")
        await pilot.pause()

        assert "edit the file by hand" in view.notice_text
        assert user_config.read_bytes() == original


@pytest.mark.asyncio
async def test_the_theme_row_closes_the_view_and_asks_for_the_picker(
    isolated_global_config_dir: Path,
) -> None:
    """The picker is the palette's theme mode, not a stackable screen, so the
    row's button closes the view and the seam opens the picker (D8: the
    existing picker, no second one)."""
    _write_user_config(isolated_global_config_dir)
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)

        await pilot.click("#theme-picker")
        await pilot.pause()

        assert host.result == ConfigViewResult(open_theme_picker=True)
        # The theme row's own reading never changed: the picker happens after
        # the view closes, and a reopened view reads the new state.
        assert view.theme_line_text == f"theme.name {THEME} · source: user · live"


@pytest.mark.asyncio
async def test_a_notice_generated_in_the_view_is_the_dismiss_payload(
    isolated_global_config_dir: Path,
) -> None:
    """The wiring surfaces the view's own message; the payload carries it."""
    _write_user_config(isolated_global_config_dir)
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _interval_input(view).value = "17"

        await pilot.click("#apply-user")
        await pilot.pause()
        await pilot.press("escape")
        await pilot.pause()

        assert host.result == ConfigViewResult(notice="saved to user configuration")


@pytest.mark.asyncio
async def test_the_modal_owns_the_keyboard(
    isolated_global_config_dir: Path,
) -> None:
    """A chord bound beneath the modal must not act beneath it."""
    _write_user_config(isolated_global_config_dir)
    host = _Host(_factory(_scopes()))

    async with host.run_test(size=SIZE) as pilot:
        await _mounted(pilot, host)

        await pilot.press("ctrl+o")
        await pilot.pause()

        # The host app never received a toggle: nothing beneath changed.
        assert host.result is None


def test_status_write_keys_order_is_the_append_order() -> None:
    """The fixed key order is load-bearing: it is the order a missing-key
    append writes, pinned here so a reorder cannot slip past the byte-exact
    tests above unnoticed."""
    from talaria.config import STATUS_WRITE_KEYS

    assert STATUS_WRITE_KEYS == ("command", "interval_seconds", "segments")


# ── the mounting seam, against the real application ─────────────────────


@pytest.mark.asyncio
async def test_the_real_app_mounts_the_view_with_its_own_state() -> None:
    """``/config`` resolves, dispatches, and mounts over the real app — fed
    the process's own state, not a fresh file read (restart-to-apply)."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=SIZE) as pilot:
        invocation = resolve_command("/config", None)
        assert isinstance(invocation, LocalInvocation)
        assert app.perform_local_command(invocation) is True
        await pilot.pause()
        view = app.screen
        assert isinstance(view, ConfigViewScreen)
        # Replay runs no status script and no files are configured here: the
        # honest rows say default-sourced, and the command row says none runs.
        assert view.command_sub_text == (
            "effective: (no status script) · source: default · mode: restart"
        )
        assert view.interval_sub_text == (
            "effective: 5 · source: default · mode: restart"
        )
        assert view.segments_sub_text == (
            "effective: cwd, git_branch, agent_model, context, task_progress, "
            "connection, version · source: default · mode: restart"
        )
        await pilot.press("escape")
        await pilot.pause()
        assert not isinstance(app.screen, ConfigViewScreen)


@pytest.mark.asyncio
async def test_the_theme_row_closes_the_view_and_the_app_opens_the_picker() -> None:
    """The row's dismiss asks the seam for the picker, and the seam opens the
    real W1 path: the palette's theme mode over the transcript."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=SIZE) as pilot:
        invocation = resolve_command("/config", None)
        assert isinstance(invocation, LocalInvocation)
        assert app.perform_local_command(invocation) is True
        await pilot.pause()
        view = app.screen
        assert isinstance(view, ConfigViewScreen)

        await pilot.click("#theme-picker")
        for _ in range(3):
            await pilot.pause()

        assert not isinstance(app.screen, ConfigViewScreen)
        assert app.palette.is_theme_active is True


@pytest.mark.asyncio
async def test_an_apply_through_the_real_app_writes_the_user_file(
    isolated_global_config_dir: Path,
) -> None:
    """The whole path: edit a row in the mounted view, apply, and the real
    byte-preserving write lands in the user configuration file; the view's
    own message surfaces through the app when the view closes."""
    app, _ = paused_app([event("gateway.ready", {})])
    user_config = isolated_global_config_dir / "config.toml"
    async with app.run_test(size=SIZE) as pilot:
        invocation = resolve_command("/config", None)
        assert isinstance(invocation, LocalInvocation)
        assert app.perform_local_command(invocation) is True
        await pilot.pause()
        view = app.screen
        assert isinstance(view, ConfigViewScreen)
        view.query_one("#interval", Input).value = "9"

        await pilot.click("#apply-user")
        await pilot.pause()

        assert user_config.read_bytes() == b"[status]\ninterval_seconds = 9\n"
        assert view.interval_sub_text == (
            "saved: 9 · effective now: 5 · takes effect on restart"
        )

        await pilot.press("escape")
        for _ in range(2):
            await pilot.pause()
        assert not isinstance(app.screen, ConfigViewScreen)
        assert "saved to user configuration" in screen_text(app)
