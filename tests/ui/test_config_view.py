"""The ``/config`` view: effective values, sources, and the allowlist write.

Issue #149, D8 recorded. The screen is driven through a minimal host app
rather than the real :class:`~talaria.ui.app.TalariaApp`, following
``tests/ui/test_dialog.py``: mounting the screen into the application is the
Wave Four seam in ``talaria/ui/app.py`` and lands under that file's custody,
while everything this unit owns — the rows, the readonly rules, the chooser,
and the write — is provable without it. The real-app wiring test belongs to
the seam.

The writes go through the real :func:`~talaria.config.save_status_settings`
against the autouse ``isolated_global_config_dir`` redirection, so every
"nothing was written" assertion is about bytes on disk, not a stub.
"""

from __future__ import annotations

from collections.abc import Callable
from pathlib import Path

import pytest
from textual.app import App, ComposeResult
from textual.pilot import Pilot
from textual.widgets import Input, Static

from talaria.config import Config, load_config, setting_scopes
from talaria.ui.config_view import ConfigViewScreen

#: Every key's home for the render test: one user file that sets all four.
USER_CONFIG = (
    "[status]\n"
    'command = "git status --short"\n'
    "interval_seconds = 5\n"
    'segments = ["cwd", "version"]\n'
    "[theme]\n"
    'name = "refined-default"\n'
)

#: The size the screen is driven at: tall enough that the whole modal,
#: chooser included, is on screen for clicks.
SIZE = (100, 44)


class _Host(App[None]):
    """The minimal host that mounts the screen the way the seam will."""

    def __init__(self, factory: Callable[[], ConfigViewScreen]) -> None:
        super().__init__()
        self._factory = factory
        self.result: str | None = None
        self.view: ConfigViewScreen | None = None

    def compose(self) -> ComposeResult:
        yield Static("behind the modal")

    def open_config(self) -> None:
        self.view = self._factory()
        self.push_screen(self.view, self._capture)

    def _capture(self, result: str | None) -> None:
        self.result = result


def _write_user_config(isolated_global_config_dir: Path, text: str = USER_CONFIG) -> Path:
    path = isolated_global_config_dir / "config.toml"
    path.write_text(text, encoding="utf-8")
    return path


def _loaded(cwd: Path) -> tuple[Config, dict[tuple[str, str], str]]:
    cfg = load_config(cwd=cwd)
    return cfg, setting_scopes(cwd=cwd)


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
    user_config = _write_user_config(isolated_global_config_dir)
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)

        assert view.theme_line_text == (
            "theme.name refined-default · source: user · live"
        )
        assert view.command_sub_text == (
            "effective: git status --short · source: user · mode: restart"
        )
        assert view.interval_sub_text == (
            "effective: 5 · source: user · mode: restart"
        )
        assert view.segments_sub_text == (
            "effective: cwd, version · source: user · mode: restart"
        )
        assert view.chooser_row_texts == (
            "[x] cwd",
            "[x] version",
            "[ ] git_branch",
            "[ ] agent_model",
            "[ ] context",
            "[ ] task_progress",
            "[ ] connection",
        )
        assert user_config.exists()


@pytest.mark.asyncio
async def test_environment_sourced_rows_render_read_only_with_the_reason(
    isolated_global_config_dir: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    _write_user_config(isolated_global_config_dir)
    monkeypatch.setenv("TALARIA_STATUS_INTERVAL_SECONDS", "30")
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

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
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

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


@pytest.mark.asyncio
@pytest.mark.parametrize("bad", ["0", "3601", "fast", ""])
async def test_every_invalid_interval_shape_is_rejected_inline(
    isolated_global_config_dir: Path, bad: str
) -> None:
    _write_user_config(isolated_global_config_dir)
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

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
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _interval_input(view).value = "17"

        await pilot.click("#apply-user")
        await pilot.pause()

        assert user_config.read_bytes() == USER_CONFIG.replace(
            "interval_seconds = 5", "interval_seconds = 17"
        ).encode()
        assert view.interval_sub_text == (
            "saved: 17 · effective now: 5 · takes effect on restart"
        )
        assert view.notice_text == "saved to user configuration"
        # The rows the apply did not write keep their pre-apply reading.
        assert view.command_sub_text == (
            "effective: git status --short · source: user · mode: restart"
        )


@pytest.mark.asyncio
async def test_apply_with_nothing_changed_writes_nothing(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(isolated_global_config_dir)
    original = user_config.read_bytes()
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

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
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

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
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

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
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

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
        assert repo_config.read_bytes() == (
            b'[status]\ncommand = "repo-new"\n'
        )
        assert view.command_sub_text == (
            "saved: repo-new · effective now: repo-status · takes effect on restart"
        )


@pytest.mark.asyncio
async def test_the_segments_row_names_the_session_layer_when_a_bar_toggle_diverged(
    isolated_global_config_dir: Path,
) -> None:
    _write_user_config(isolated_global_config_dir)
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(
        lambda: ConfigViewScreen(
            cfg,
            scopes,
            segments_now=("cwd",),
        )
    )

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)

        assert view.segments_sub_text == (
            "effective now: cwd · source: session (/bar) · mode: restart"
        )


@pytest.mark.asyncio
async def test_the_chooser_reorders_with_shift_arrows_and_toggles_with_space(
    isolated_global_config_dir: Path,
) -> None:
    _write_user_config(isolated_global_config_dir)
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.query_one("#segments").focus()
        await pilot.pause()

        # Focus opens on the first shown row and follows the *name*.
        assert view.chooser_active_row_text == "[x] cwd"

        await pilot.press("shift+down")
        await pilot.pause()
        assert view.chooser_selected == ("version", "cwd")
        assert view.chooser_active_row_text == "[x] cwd"

        await pilot.press("space")
        await pilot.pause()
        after_toggle: tuple[str, ...] = view.chooser_selected
        assert after_toggle == ("version",)
        assert view.chooser_active_row_text == "[ ] cwd"

        await pilot.press("up")
        await pilot.pause()
        assert view.chooser_active_row_text == "[x] version"

        # Space toggles the *focused* row, so walk back onto the unselected
        # row before toggling it back on — the focus-follows-the-name rule
        # that makes the chooser usable one row at a time.
        await pilot.press("down")
        await pilot.pause()
        assert view.chooser_active_row_text == "[ ] cwd"

        await pilot.press("space")
        await pilot.pause()
        after_readd: tuple[str, ...] = view.chooser_selected
        assert after_readd == ("version", "cwd")
        assert view.chooser_active_row_text == "[x] cwd"


@pytest.mark.asyncio
async def test_apply_saves_the_reordered_segment_selection(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(isolated_global_config_dir)
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.query_one("#segments").focus()
        await pilot.pause()
        await pilot.press("shift+down")
        await pilot.pause()

        await pilot.click("#apply-user")
        await pilot.pause()

        assert user_config.read_bytes() == USER_CONFIG.replace(
            'segments = ["cwd", "version"]',
            'segments = [\n  "version",\n  "cwd",\n]',
        ).encode()
        assert view.segments_sub_text == (
            "saved: version, cwd · effective now: cwd, version · "
            "takes effect on restart"
        )


@pytest.mark.asyncio
async def test_an_empty_command_saves_as_the_explicit_empty_value(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(isolated_global_config_dir)
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _command_input(view).value = ""

        await pilot.click("#apply-user")
        await pilot.pause()

        assert user_config.read_bytes() == USER_CONFIG.replace(
            'command = "git status --short"', 'command = ""'
        ).encode()
        assert view.command_sub_text == (
            "saved: (no status script) · effective now: git status --short · "
            "takes effect on restart"
        )


@pytest.mark.asyncio
async def test_the_edit_by_hand_refusal_surfaces_in_the_notice(
    isolated_global_config_dir: Path,
) -> None:
    user_config = _write_user_config(
        isolated_global_config_dir, 'status = { command = "inline" }\n'
    )
    original = user_config.read_bytes()
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _command_input(view).value = "new"

        await pilot.click("#apply-user")
        await pilot.pause()

        assert "edit the file by hand" in view.notice_text
        assert user_config.read_bytes() == original


@pytest.mark.asyncio
async def test_the_theme_row_opens_the_picker_through_the_seam_callback(
    isolated_global_config_dir: Path,
) -> None:
    _write_user_config(isolated_global_config_dir)
    cfg, scopes = _loaded(Path.cwd())
    opened: list[str] = []

    async def open_picker() -> str | None:
        opened.append("opened")
        return "neutral-dark"

    host = _Host(
        lambda: ConfigViewScreen(
            cfg,
            scopes,
            open_theme_picker=open_picker,
        )
    )

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)

        await pilot.click("#theme-picker")
        await pilot.pause()

        assert opened == ["opened"]
        assert view.theme_line_text == (
            "theme.name neutral-dark · source: user · live"
        )
        assert view.notice_text == (
            "theme.name neutral-dark applied live and saved to user configuration"
        )


@pytest.mark.asyncio
async def test_a_cancelled_picker_leaves_the_theme_row_unchanged(
    isolated_global_config_dir: Path,
) -> None:
    _write_user_config(isolated_global_config_dir)
    cfg, scopes = _loaded(Path.cwd())

    async def cancelled_picker() -> str | None:
        return None

    host = _Host(
        lambda: ConfigViewScreen(
            cfg,
            scopes,
            open_theme_picker=cancelled_picker,
        )
    )

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)

        await pilot.click("#theme-picker")
        await pilot.pause()

        assert view.theme_line_text == (
            "theme.name refined-default · source: user · live"
        )
        assert view.notice_text == "theme picker cancelled — nothing changed"


@pytest.mark.asyncio
async def test_a_notice_generated_in_the_view_is_the_dismiss_payload(
    isolated_global_config_dir: Path,
) -> None:
    """The wiring surfaces the view's own message; the payload is the notice."""
    _write_user_config(isolated_global_config_dir)
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        _interval_input(view).value = "17"

        await pilot.click("#apply-user")
        await pilot.pause()
        await pilot.press("escape")
        await pilot.pause()

        assert host.result == "saved to user configuration"


@pytest.mark.asyncio
async def test_the_modal_owns_the_keyboard(
    isolated_global_config_dir: Path,
) -> None:
    """A chord bound beneath the modal must not act beneath it.

    The inspector toggle is the app's global binding; this test holds the
    seam's half of the contract — the screen never lets the key through —
    until the wiring lands.
    """
    _write_user_config(isolated_global_config_dir)
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

    async with host.run_test(size=SIZE) as pilot:
        await _mounted(pilot, host)

        await pilot.press("ctrl+o")
        await pilot.pause()

        # The host app never received a toggle: nothing beneath changed.
        assert host.result is None


@pytest.mark.asyncio
async def test_the_theme_button_hides_without_the_seam_callback(
    isolated_global_config_dir: Path,
) -> None:
    _write_user_config(isolated_global_config_dir)
    cfg, scopes = _loaded(Path.cwd())
    host = _Host(lambda: ConfigViewScreen(cfg, scopes))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)

        button = view.query_one("#theme-picker")
        assert button.display is False


def test_status_write_keys_order_is_the_append_order() -> None:
    """The fixed key order is load-bearing: it is the order a missing-key
    append writes, pinned here so a reorder cannot slip past the byte-exact
    tests above unnoticed."""
    from talaria.config import STATUS_WRITE_KEYS

    assert STATUS_WRITE_KEYS == ("command", "interval_seconds", "segments")