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

CFG v0.6.2 replacement contract: ``/config`` mounts the settings workspace
(``talaria.ui.settings_workspace``), and this screen's rows persist as the
workspace's Talaria-owned branch — same effective/source/mode sentences,
same ``#interval``/``#apply-user``/``#theme-picker`` hooks. The direct-screen
tests above pin that branch contract; the mounting-seam tests below pin the
workspace mount.
"""

from __future__ import annotations

import importlib
from collections.abc import Callable, Mapping
from pathlib import Path
from typing import Any

import pytest
from textual.app import App, ComposeResult
from textual.css.query import NoMatches
from textual.pilot import Pilot
from textual.widgets import Button, Input, Static

from talaria.config import setting_scopes
from talaria.domain.commands import LocalInvocation, resolve_command
from talaria.transport.connection_set import EnsureReport
from talaria.ui.config_view import ConfigViewResult, ConfigViewScreen
from talaria.ui.settings_overlays import TargetSwitchOverlay
from tests.ui.conftest import event, paused_app, screen_text


def _workspace_screen_type() -> Any:
    """The B5 workspace screen, failing as missing behavior until it lands."""
    try:
        module = importlib.import_module("talaria.ui.settings_workspace")
    except ImportError as exc:
        pytest.fail(f"unimplemented interface (CFG v0.6.2 B5): {exc}")
    try:
        return module.SettingsWorkspaceScreen
    except AttributeError:
        pytest.fail("unimplemented interface SettingsWorkspaceScreen (CFG B5)")

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
#
# CFG v0.6.2 replacement: /config mounts the settings workspace, fed the
# process's own state (restart-to-apply still holds for Talaria rows).
#
# CFG-T5-T1AR5 (C2 mount): live /config mounts the Hermes groups alongside
# the Talaria branch, so the branch's lower rows are no longer guaranteed
# on-screen in a 44-row screenshot. The seam tests prove the mounted groups
# on screen and read branch rows through the mounted widgets — production
# must not hide groups to keep a screenshot green.


def _branch_static_texts(branch: Any) -> list[str]:
    """Every Static text the Talaria branch composed, visible or covered."""
    return [str(line.render()) for line in branch.query(Static)]


@pytest.mark.asyncio
async def test_the_real_app_mounts_the_workspace_with_its_own_state() -> None:
    """/config resolves, dispatches, and mounts the settings workspace over
    the real app — and the workspace's Talaria-owned branch reads the
    process's own state, not a fresh file read."""
    workspace_type = _workspace_screen_type()
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=SIZE) as pilot:
        invocation = resolve_command("/config", None)
        assert isinstance(invocation, LocalInvocation)
        assert app.perform_local_command(invocation) is True
        await pilot.pause()
        assert isinstance(app.screen, workspace_type)
        # Hermes groups mount on live /config: their titles and read-only
        # status rows render on screen beside the Talaria branch.
        app.screen.query_one("#settings-group-hermes-profile")
        app.screen.query_one("#settings-group-hermes-host")
        text = screen_text(app)
        assert "Hermes profile" in text
        assert "Profile schema" in text
        assert "Host" in text
        assert "Hermes update" in text
        assert "read-only" in text
        # Replay runs no status script and no files are configured here: the
        # honest branch rows say default-sourced, in the same sentences the
        # direct-screen tests pin above. The branch head is on screen; its
        # lower rows are read through the mounted widgets, which hold even
        # where the mounted groups cover them.
        assert "theme.name" in text
        assert "refined-default" in text
        assert "source: default" in text
        branch_text = " ".join(
            _branch_static_texts(app.screen.query_one("#settings-talaria-branch"))
        )
        assert (
            "theme.name refined-default · source: default · live" in branch_text
        )
        assert (
            "effective: (no status script) · source: default · mode: restart"
            in branch_text
        )
        assert "effective: 5 · source: default · mode: restart" in branch_text
        assert "source: default · mode: restart" in branch_text
        await pilot.press("escape")
        await pilot.pause()
        assert not isinstance(app.screen, workspace_type)


@pytest.mark.asyncio
async def test_the_theme_row_closes_the_workspace_and_the_app_opens_the_picker() -> None:
    """The branch row's dismiss asks the seam for the picker, and the seam
    opens the real W1 path: the palette's theme mode over the transcript."""
    workspace_type = _workspace_screen_type()
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=SIZE) as pilot:
        invocation = resolve_command("/config", None)
        assert isinstance(invocation, LocalInvocation)
        assert app.perform_local_command(invocation) is True
        await pilot.pause()
        assert isinstance(app.screen, workspace_type)

        await pilot.click("#theme-picker")
        for _ in range(3):
            await pilot.pause()

        assert not isinstance(app.screen, workspace_type)
        assert app.palette.is_theme_active is True


@pytest.mark.asyncio
async def test_an_apply_through_the_real_app_writes_the_user_file(
    isolated_global_config_dir: Path,
) -> None:
    """The whole path: edit a Talaria-branch row in the mounted workspace,
    apply, and the real byte-preserving write lands in the user
    configuration file; the branch's own message surfaces through the app
    when the workspace closes.

    The apply runs through the mounted button widget rather than screen
    coordinates, so the mounted Hermes groups covering it cannot turn the
    write into a click on empty space."""
    workspace_type = _workspace_screen_type()
    app, _ = paused_app([event("gateway.ready", {})])
    user_config = isolated_global_config_dir / "config.toml"
    async with app.run_test(size=SIZE) as pilot:
        invocation = resolve_command("/config", None)
        assert isinstance(invocation, LocalInvocation)
        assert app.perform_local_command(invocation) is True
        await pilot.pause()
        assert isinstance(app.screen, workspace_type)
        branch = app.screen.query_one("#settings-talaria-branch")
        app.screen.query_one("#interval", Input).value = "9"

        app.screen.query_one("#apply-user", Button).press()
        await pilot.pause()

        assert user_config.read_bytes() == b"[status]\ninterval_seconds = 9\n"
        assert branch.notice_text == "saved to user configuration"
        branch_text = " ".join(_branch_static_texts(branch))
        assert (
            "saved: 9 · effective now: 5 · takes effect on restart" in branch_text
        )

        await pilot.press("escape")
        for _ in range(2):
            await pilot.pause()
        assert not isinstance(app.screen, workspace_type)
        assert "saved to user configuration" in screen_text(app)


@pytest.mark.asyncio
async def test_a_malformed_configuration_file_is_named_rather_than_swallowed(
    isolated_global_config_dir: Path,
) -> None:
    """F-1 of the C11 review (#149): ``load_config`` tolerates a malformed
    file at launch, so a running application on a broken configuration is
    exactly the state an operator reaches for /config in. The refusal must
    name the file and the parse error — silence would leave the operator
    unable to tell a broken file from a broken command."""
    (isolated_global_config_dir / "config.toml").write_text("[status\n", encoding="utf-8")
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=SIZE) as pilot:
        invocation = resolve_command("/config", None)
        assert isinstance(invocation, LocalInvocation)
        assert app.perform_local_command(invocation) is True
        for _ in range(3):
            await pilot.pause()

        assert not isinstance(app.screen, ConfigViewScreen)
        # The refusal is rendered — the one-row notice carries at least its
        # leading clause on screen — and the delivered message names the
        # command, the file, and the parse error. The row ellipsizes past its
        # width by design (``composer.py``), so the full text is asserted on
        # the composer's notice, the same surface the composer suite uses.
        assert "/config:" in screen_text(app)
        assert app.composer.notice.startswith("/config:")
        assert "is not valid TOML" in app.composer.notice
        assert "config.toml" in app.composer.notice


# ── P2-1: the Talaria branch across a Hermes target switch (dev-4) ─────────


class _P2Connections:
    """One configured connection; the switch must not disturb sessions."""

    def __init__(self, home: str) -> None:
        self._home = home

    @property
    def home(self) -> str:
        return self._home

    async def ensure(self, profile: str) -> EnsureReport:
        return EnsureReport(profile, "already_up", "connected")


class _P2SettingsClient:
    """Minimal recording Hermes stand-in for the branch-ownership test."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, str]] = []

    def factory(self, endpoint: str) -> _P2SettingsClient:
        del endpoint
        return self

    async def get_schema(self, target: Any) -> Any:
        self.calls.append(("get_schema", target.profile_name))
        return {
            "fields": {
                "agent.max_turns": {
                    "type": "number",
                    "description": "Maximum agent turns.",
                    "category": "agent",
                }
            },
            "category_order": ["agent"],
        }

    async def get_config(self, target: Any) -> Any:
        self.calls.append(("get_config", target.profile_name))
        saved = {"agent.max_turns": 25}
        if target.profile_name.endswith("-switch"):
            saved = {"agent.max_turns": 30}
        return {"saved": dict(saved), "effective": dict(saved), "defaults": {}}


@pytest.mark.asyncio
async def test_talaria_branch_edits_survive_a_hermes_target_switch(
    isolated_global_config_dir: Path,
) -> None:
    """P2-1 ownership across the switch: Talaria-owned branch edits are not
    Hermes pending edits — discarding the Hermes edit and switching targets
    leaves the branch input intact, and its apply still writes the user
    file afterwards."""
    test_a = "talaria-v062-cfg-p2-local-active-a"
    test_e = "talaria-v062-cfg-p2-local-active-switch"
    url = "http://127.0.0.1:8765"
    workspace_type = _workspace_screen_type()
    fake = _P2SettingsClient()
    app, _ = paused_app(
        [event("gateway.ready", {})],
        profile_endpoints={test_a: url, test_e: url},
        current_profile=test_a,
        connections=_P2Connections(home="local"),
        settings_factory=fake.factory,
    )
    user_config = isolated_global_config_dir / "config.toml"
    async with app.run_test(size=SIZE) as pilot:
        invocation = resolve_command("/config", None)
        assert isinstance(invocation, LocalInvocation)
        assert app.perform_local_command(invocation) is True
        for _ in range(30):
            await pilot.pause()
            if isinstance(app.screen, workspace_type):
                break
        assert isinstance(app.screen, workspace_type)

        app.screen.query_one("#command", Input).value = "echo branch-edit"
        schema_row = app.screen.query_one("#settings-row-agent-max_turns")
        schema_row.query_one(Input).value = "40"
        try:
            app.screen.query_one("#settings-target", Button)
        except NoMatches:
            pytest.fail(
                "live /config exposes no reachable target control (P2-1)"
            )
        await pilot.click("#settings-target")
        await pilot.pause()
        for button in app.screen.query(Button):
            if str(button.label).strip() == f"local / {test_e}":
                button.press()
                break
        for _ in range(30):
            await pilot.pause()
            if isinstance(app.screen, TargetSwitchOverlay):
                break
        assert isinstance(app.screen, TargetSwitchOverlay)

        await pilot.click("#switch-discard")
        for _ in range(60):
            await pilot.pause()
            if f"selected: {test_e}" in screen_text(app):
                break
        assert f"selected: {test_e}" in screen_text(app)
        assert ("get_config", test_e) in fake.calls
        assert app.screen.query_one("#command", Input).value == "echo branch-edit"
        schema_row = app.screen.query_one("#settings-row-agent-max_turns")
        assert schema_row.query_one(Input).value == "30"

        app.screen.query_one("#apply-user", Button).press()
        await pilot.pause()
        assert user_config.read_bytes() == (
            b'[status]\ncommand = "echo branch-edit"\n'
        )
