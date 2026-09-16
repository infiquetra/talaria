"""P3-1 selected-schema remount and P3-2 reveal after target load.

CFG-P3 residuals against published ``v0.6.3`` (``a63db5e``): live target
selection moves the header but ``apply_loaded_view`` only refreshes row
widgets that already exist, so the selected target's new editable schema
group and Environment row never mount — leaving no editable field for
Stay/Discard/Save and no secret row from which Reveal can be reached,
even though the loaded view contains the data.

Every behavioral test here mounts the real ``TalariaApp`` on testE (whose
schema lacks testA's field and whose env is empty), opens ``/config``
normally, and selects testA through the live ``#settings-target`` control
with a recording fake settings client. All fail on ``v0.6.3`` at the
shared remount defect; header movement alone (``selected: testA``) is
asserted present first so each failure demonstrates it is insufficient.

Hard rules for this file:

* Never call the switch/reveal opener methods directly — reaching every
  overlay through real controls is enforced by the AST source guard at
  the bottom of this file, not just by review.
* Profiles are the P2 legal active family; ``testB`` and ``default`` are
  never write targets, and any write log naming them — or any profile
  the run did not receipt — hard-fails the test.
* Secret values are random in-memory canaries. Equality/visibility/cleared
  are recorded as booleans; no plaintext reaches notices, logs, recordings,
  or evidence. Timeout firing itself is installed-J3 territory; the
  bounded ``REVEAL_DISPLAY_SECONDS`` pin lives in
  ``tests/ui/test_settings_workspace.py`` and close-wipe is proven here.

Pinned P3 contract (dev-4): target load reconciles the complete dynamic
Hermes group subtree (mount new, remove stale, update kept) without
rewriting session ``current_profile``; search text is preserved and
applied to the new rows; focus lands on an attached widget.
"""

from __future__ import annotations

import ast
import asyncio
import secrets
from pathlib import Path
from typing import Any

import pytest
from textual.css.query import NoMatches
from textual.pilot import Pilot
from textual.widgets import Button, Input, Static

from talaria.domain.commands import LocalInvocation, resolve_command
from talaria.transport.connection_set import EnsureReport
from talaria.transport.settings import SettingsError
from talaria.ui.app import TalariaApp
from talaria.ui.settings_overlays import RevealSecretOverlay, TargetSwitchOverlay
from talaria.ui.settings_widgets import row_widget_id
from talaria.ui.settings_workspace import SettingsWorkspaceScreen
from tests.ui.conftest import event, paused_app, screen_text

CONNECTION = "local"
TEST_A = "talaria-v062-cfg-p2-local-active-a"
TEST_E = "talaria-v062-cfg-p2-local-active-switch"
TEST_B = "testB"
URL_LOCAL = "http://127.0.0.1:8765"
SECRET_KEY = "EXAMPLE_P3_KEY"
MASKED = "sk-…p3ab"
FIELD_A = "agent.max_turns"
FIELD_E = "agent.api_max_retries"
SIZE = (120, 36)

_WRITE_METHODS = frozenset(
    {
        "put_config",
        "put_env",
        "delete_env",
        "set_model",
        "clone_profile",
        "rename_profile",
        "delete_profile",
        "reset_profile",
        "restart_gateway",
    }
)


class _StubConnections:
    """One configured connection; target loads must not disturb sessions."""

    def __init__(self, home: str) -> None:
        self._home = home
        self.ensured: list[str] = []

    @property
    def home(self) -> str:
        return self._home

    async def ensure(self, profile: str) -> EnsureReport:
        self.ensured.append(profile)
        return EnsureReport(profile, "already_up", "connected")


def _deep_merge(base: dict[str, Any], patch: dict[str, Any]) -> dict[str, Any]:
    merged = {key: value for key, value in base.items()}
    for key, value in patch.items():
        if isinstance(value, dict) and isinstance(merged.get(key), dict):
            merged[key] = _deep_merge(merged[key], value)
        else:
            merged[key] = value
    return merged


class _FakeSettingsClient:
    """Recording Hermes stand-in with per-target schemas and env shapes."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, str, str]] = []
        self.writes: list[tuple[str, str, str]] = []
        self.receipts: set[str] = set()
        self.configs: dict[str, dict[str, Any]] = {}
        self.envs: dict[str, Any] = {}
        self.put_failures: dict[str, SettingsError] = {}
        self.gates: dict[tuple[str, str], asyncio.Event] = {}
        self.reveal_calls: list[tuple[str, str]] = []
        self.reveal_limit_from: int | None = None
        self.reveal_value = f"p3-canary-{secrets.token_hex(4)}"
        # P4: per-profile load failure injection. Empty means every load
        # succeeds (all P3 flows unchanged).
        self.schema_failures: dict[str, Exception] = {}
        self.schema_bodies: dict[str, Any] = {}
        self.env_failures: dict[str, Exception] = {}
        self.env_bodies: dict[str, Any] = {}

    def factory(self, endpoint: str) -> _FakeSettingsClient:
        del endpoint
        return self

    def _log(self, method: str, target: Any) -> None:
        entry = (method, target.connection_id, target.profile_name)
        self.calls.append(entry)
        if method in _WRITE_METHODS:
            self.writes.append(entry)

    async def _gate(self, method: str, profile: str) -> None:
        event_ = self.gates.get((method, profile))
        if event_ is not None:
            await event_.wait()

    async def get_schema(self, target: Any) -> Any:
        self._log("get_schema", target)
        await self._gate("get_schema", target.profile_name)
        failure = self.schema_failures.get(target.profile_name)
        if failure is not None:
            raise failure
        if target.profile_name in self.schema_bodies:
            body: dict[str, Any] = self.schema_bodies[target.profile_name]
            return body
        if target.profile_name == TEST_E:
            field, description = FIELD_E, "Maximum API retries."
        else:
            field, description = FIELD_A, "Maximum agent turns."
        return {
            "fields": {
                field: {"type": "number", "description": description, "category": "agent"}
            },
            "category_order": ["agent"],
        }

    async def get_config(self, target: Any) -> Any:
        self._log("get_config", target)
        await self._gate("get_config", target.profile_name)
        saved = dict(self.configs.get(target.profile_name, {"agent": {}}))
        return {"saved": saved, "effective": dict(saved), "defaults": {}}

    async def put_config(self, target: Any, patch: Any) -> Any:
        self._log("put_config", target)
        failure = self.put_failures.get(target.profile_name)
        if failure is not None:
            raise failure
        saved = self.configs.get(target.profile_name, {})
        self.configs[target.profile_name] = _deep_merge(saved, dict(patch))
        return {"ok": True}

    async def get_env(self, target: Any) -> Any:
        self._log("get_env", target)
        await self._gate("get_env", target.profile_name)
        failure = self.env_failures.get(target.profile_name)
        if failure is not None:
            raise failure
        if target.profile_name in self.env_bodies:
            return self.env_bodies[target.profile_name]
        return dict(self.envs.get(target.profile_name, {}))

    async def reveal_env(self, target: Any, key: str) -> Any:
        self._log("reveal_env", target)
        self.reveal_calls.append((target.profile_name, key))
        if (
            self.reveal_limit_from is not None
            and len(self.reveal_calls) >= self.reveal_limit_from
        ):
            raise SettingsError(
                "rate_limited",
                f"reveal is rate limited (429) for profile {target.profile_name!r}",
            )
        return {"key": key, "value": self.reveal_value}

    def assert_only_receipted_writes(self) -> None:
        """Hard-fail: a write to default/testB/unreceipted fails the run."""
        for method, _connection, profile in self.writes:
            if profile == "default" or profile == TEST_B or profile not in self.receipts:
                pytest.fail(
                    f"forbidden settings write: {method} targeted {profile!r} "
                    "(P3: writes only to run-receipted profiles)"
                )


def _live_config_app(
    fake: _FakeSettingsClient, *, current: str = TEST_E
) -> tuple[TalariaApp, Any]:
    app, controls = paused_app(
        [event("gateway.ready", {})],
        profile_endpoints={TEST_A: URL_LOCAL, TEST_E: URL_LOCAL},
        current_profile=current,
        connections=_StubConnections(home=CONNECTION),
        settings_factory=fake.factory,
    )
    return app, controls


async def _await_condition(
    pilot: Pilot[None], description: str, predicate: Any, *, limit: int = 60
) -> None:
    for _ in range(limit):
        await pilot.pause()
        if predicate():
            return
    raise AssertionError(f"timed out waiting for: {description}")


async def _open_config(pilot: Pilot[None], app: TalariaApp) -> SettingsWorkspaceScreen:
    invocation = resolve_command("/config", None)
    assert isinstance(invocation, LocalInvocation)
    assert app.perform_local_command(invocation) is True
    await _await_condition(
        pilot, "workspace mount", lambda: isinstance(app.screen, SettingsWorkspaceScreen)
    )
    screen = app.screen
    assert isinstance(screen, SettingsWorkspaceScreen)
    return screen


async def _switch_via_control(pilot: Pilot[None], app: TalariaApp, name: str) -> None:
    """Activate the live target control and choose ``name`` (P3 route)."""
    try:
        app.screen.query_one("#settings-target", Button)
    except NoMatches:
        pytest.fail("live /config exposes no reachable target control (P3-1)")
    await pilot.click("#settings-target")
    label = f"{CONNECTION} / {name}"
    for _ in range(30):
        await pilot.pause()
        for button in app.screen.query(Button):
            if str(button.label).strip() == label:
                button.press()
                await pilot.pause()
                return
    pytest.fail(f"target picker offers no option labeled {label!r} (P3-1)")


def _editor_for(screen: Any, key: str) -> Input:
    row = screen.query_one(f"#{row_widget_id(key)}")
    editor: Input = row.query_one(Input)
    return editor


def _row_present(screen: Any, key: str) -> bool:
    try:
        screen.query_one(f"#{row_widget_id(key)}")
    except NoMatches:
        return False
    return True


def _seed_remount_fixture(fake: _FakeSettingsClient) -> None:
    fake.receipts.update({TEST_A, TEST_E})
    fake.configs[TEST_A] = {"agent": {"max_turns": 25}}
    fake.configs[TEST_E] = {"agent": {"api_max_retries": 3}}
    fake.envs[TEST_A] = {
        SECRET_KEY: {
            "is_set": True,
            "redacted_value": MASKED,
            "description": "P3 test key",
        }
    }
    fake.envs[TEST_E] = {}


def _secret_row_button(screen: Any, key: str) -> Button:
    """The Reveal control attached to one secret row (P3-2 route)."""
    try:
        row = screen.query_one(f"#{row_widget_id(key)}")
    except NoMatches:
        pytest.fail(
            f"live /config renders no secret row for {key} "
            "(P3-2 residual: selected target Environment not mounted)"
        )
    for button in row.query(Button):
        if str(button.label).strip().lower() == "reveal":
            found: Button = button
            return found
    pytest.fail(f"secret row {key} exposes no Reveal control (P3-2)")


def _widget_texts(app: TalariaApp) -> list[str]:
    texts = [str(widget.render()) for widget in app.screen.query(Static)]
    texts.extend(widget.value for widget in app.screen.query(Input))
    texts.extend(str(button.label) for button in app.screen.query(Button))
    return texts


def _recording_texts(config_dir: Path) -> list[str]:
    texts: list[str] = []
    recordings = config_dir / "recordings"
    if not recordings.is_dir():
        return texts
    for path in sorted(recordings.rglob("*")):
        if not path.is_file():
            continue
        try:
            texts.append(path.read_text(encoding="utf-8", errors="strict"))
        except (OSError, UnicodeError):
            continue
    return texts


async def _switch_to_test_a(
    pilot: Pilot[None], app: TalariaApp, choice: str
) -> None:
    """Edit the mounted testE field, choose testA, answer ``choice``."""
    _editor_for(app.screen, FIELD_E).value = "7"
    await _switch_via_control(pilot, app, TEST_A)
    await _await_condition(
        pilot,
        "switch overlay",
        lambda: isinstance(app.screen, TargetSwitchOverlay),
    )
    await pilot.click(f"#switch-{choice}")
    await pilot.pause()


# ── P3-1. Selected target mounts authoritative rows ───────────────────────


@pytest.mark.asyncio
async def test_switch_load_schema_transport_failure_is_typed_not_silent(
    isolated_global_config_dir: Path,
) -> None:
    """P4 through the P3 switch flow: when testA's schema fetch raises a
    typed transport error, the committed load shows a reason-named safe
    state and the stale testE row is gone — never silent stale rows."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    fake.schema_failures[TEST_A] = SettingsError("unreachable", "schema unreachable")
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        for _ in range(10):
            await pilot.pause()

        text = screen_text(app).lower()
        assert "unreachable" in text, (
            "failed switch-load renders no typed reason (P4-1 residual)"
        )
        assert not _row_present(app.screen, FIELD_E), (
            "stale testE row survives a failed testA load (P4-1)"
        )
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_switch_load_schema_decode_failure_reports_decode_state(
    isolated_global_config_dir: Path,
) -> None:
    """P4 through the P3 switch flow: an undecodable testA schema body
    surfaces a decode-named state instead of the silent placeholder."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    fake.schema_bodies[TEST_A] = {
        "fields": {"agent.max_turns": {"description": "No type.", "category": "agent"}},
        "category_order": ["agent"],
    }
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        for _ in range(10):
            await pilot.pause()

        text = screen_text(app).lower()
        assert "decode" in text, (
            "undecodable switch-load renders no decode-typed state (P4-1)"
        )
        assert not _row_present(app.screen, FIELD_E)
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_switch_load_env_failure_mounts_schema_with_env_state(
    isolated_global_config_dir: Path,
) -> None:
    """P4 through the P3 switch flow: testA schema mounts while its env
    outage gets an env-named state — the secret row stays absent without
    reading as no-env-configured."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    fake.env_failures[TEST_A] = SettingsError("timeout", "env fetch timeout")
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )

        assert _row_present(app.screen, FIELD_A)
        text = screen_text(app).lower()
        assert "env" in text, (
            "failed env load on switch renders no env-named state (P4)"
        )
        assert not _row_present(app.screen, SECRET_KEY)
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_select_load_mounts_new_schema_and_environment_rows(
    isolated_global_config_dir: Path,
) -> None:
    """The P3-1 headline: header movement is necessary but insufficient.
    Selecting testA must mount its editable schema row and masked secret
    row with a Reveal control, and the old-only testE row must disappear —
    while the session home stays put."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )

        assert _row_present(app.screen, FIELD_A), (
            "selected testA header moved but its editable schema row never "
            "mounted (P3-1 residual: apply_loaded_view creates nothing)"
        )
        assert _editor_for(app.screen, FIELD_A).value == "25"
        assert not _row_present(app.screen, FIELD_E), (
            "stale testE-only row survives the testA load (P3-1)"
        )
        button = _secret_row_button(app.screen, SECRET_KEY)
        assert button is not None
        assert MASKED in screen_text(app)
        assert app.current_profile == TEST_E
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_target_load_never_rewrites_session_current_profile(
    isolated_global_config_dir: Path,
) -> None:
    """KTD4: loading through a target must not temporarily rewrite session
    home. While the testA load is gated mid-flight, the session profile
    still reads testE; after release the load completes normally."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        load_gate = asyncio.Event()
        fake.gates[("get_config", TEST_A)] = load_gate
        _editor_for(app.screen, FIELD_E).value = "7"
        await _switch_via_control(pilot, app, TEST_A)
        await _await_condition(
            pilot,
            "switch overlay",
            lambda: isinstance(app.screen, TargetSwitchOverlay),
        )
        await pilot.click("#switch-discard")
        for _ in range(10):
            await pilot.pause()

        assert app.current_profile == TEST_E, (
            "target load rewrites session current_profile mid-flight (P3-1)"
        )
        load_gate.set()
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        assert app.current_profile == TEST_E
        assert _row_present(app.screen, FIELD_A)
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_search_text_survives_and_applies_to_remounted_rows(
    isolated_global_config_dir: Path,
) -> None:
    """KTD5: remounting preserves the search text and applies it to the new
    rows instead of leaving hidden stale matches behind."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        screen = await _open_config(pilot, app)
        screen.query_one("#settings-search", Input).value = "turns"
        await pilot.pause()
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )

        assert screen.query_one("#settings-search", Input).value == "turns"
        assert _row_present(app.screen, FIELD_A), (
            "remounted testA row missing, so search has nothing to apply to (P3-1)"
        )
        row = app.screen.query_one(f"#{row_widget_id(FIELD_A)}")
        assert row.display, "remounted matching row hidden despite search (P3-1)"
        fake.assert_only_receipted_writes()


# ── P3-1 switch matrix on remounted rows ──────────────────────────────────


@pytest.mark.asyncio
async def test_stay_preserves_remounted_target_and_edit_without_io(
    isolated_global_config_dir: Path,
) -> None:
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        _editor_for(app.screen, FIELD_A).value = "40"
        calls_before = len(fake.calls)

        await _switch_via_control(pilot, app, TEST_E)
        await _await_condition(
            pilot,
            "switch overlay",
            lambda: isinstance(app.screen, TargetSwitchOverlay),
        )
        await pilot.click("#switch-stay")
        await _await_condition(
            pilot,
            "overlay close",
            lambda: isinstance(app.screen, SettingsWorkspaceScreen),
        )

        assert f"selected: {TEST_A}" in screen_text(app)
        assert _editor_for(app.screen, FIELD_A).value == "40"
        assert fake.calls[calls_before:] == []
        assert fake.writes == []
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_discard_replaces_rows_with_new_target(
    isolated_global_config_dir: Path,
) -> None:
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        _editor_for(app.screen, FIELD_A).value = "40"
        loads_before = len(
            [call for call in fake.calls if call == ("get_config", CONNECTION, TEST_E)]
        )

        await _switch_via_control(pilot, app, TEST_E)
        await _await_condition(
            pilot,
            "switch overlay",
            lambda: isinstance(app.screen, TargetSwitchOverlay),
        )
        await pilot.click("#switch-discard")
        await _await_condition(
            pilot,
            "testE header",
            lambda: f"selected: {TEST_E}" in screen_text(app),
        )

        assert fake.writes == []
        loads_after = len(
            [call for call in fake.calls if call == ("get_config", CONNECTION, TEST_E)]
        )
        assert loads_after > loads_before, (
            "discard must load the newly selected testE (P3-1)"
        )
        assert _editor_for(app.screen, FIELD_E).value == "3"
        assert not _row_present(app.screen, FIELD_A), (
            "stale testA-only row survives the discard back to testE (P3-1)"
        )
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_save_success_writes_old_then_loads_new_rows(
    isolated_global_config_dir: Path,
) -> None:
    """Save-before-switch across a remount: PUT and re-read hit testA
    while it stays selected; only then does testE load with testE rows."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        _editor_for(app.screen, FIELD_A).value = "40"

        await _switch_via_control(pilot, app, TEST_E)
        await _await_condition(
            pilot,
            "switch overlay",
            lambda: isinstance(app.screen, TargetSwitchOverlay),
        )
        await pilot.click("#switch-save")
        await _await_condition(
            pilot,
            "testE header after save",
            lambda: f"selected: {TEST_E}" in screen_text(app),
        )

        puts = [call for call in fake.calls if call[0] == "put_config"]
        assert puts == [("put_config", CONNECTION, TEST_A)]
        put_index = next(
            index
            for index, call in enumerate(fake.calls)
            if call == ("put_config", CONNECTION, TEST_A)
        )
        rereads = [
            index
            for index, call in enumerate(fake.calls)
            if call == ("get_config", CONNECTION, TEST_A)
        ]
        assert any(index > put_index for index in rereads), (
            "old-target re-read must follow the PUT before the switch (P3-1)"
        )
        assert fake.configs[TEST_A] == {"agent": {"max_turns": 40}}
        assert fake.configs[TEST_E] == {"agent": {"api_max_retries": 3}}
        assert _editor_for(app.screen, FIELD_E).value == "3"
        assert not _row_present(app.screen, FIELD_A)
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_failed_save_retains_selected_rows_and_edit(
    isolated_global_config_dir: Path,
) -> None:
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    fake.put_failures[TEST_A] = SettingsError("http_error", "PUT refused: locked")
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        _editor_for(app.screen, FIELD_A).value = "40"
        loads_before = [
            call for call in fake.calls if call[0] == "get_config"
        ]

        await _switch_via_control(pilot, app, TEST_E)
        await _await_condition(
            pilot,
            "switch overlay",
            lambda: isinstance(app.screen, TargetSwitchOverlay),
        )
        await pilot.click("#switch-save")
        await _await_condition(
            pilot,
            "rejection shown on workspace",
            lambda: isinstance(app.screen, SettingsWorkspaceScreen)
            and "locked" in screen_text(app),
        )

        assert f"selected: {TEST_A}" in screen_text(app)
        assert _editor_for(app.screen, FIELD_A).value == "40"
        loads_after = [call for call in fake.calls if call[0] == "get_config"]
        assert loads_after == loads_before, (
            "failed save must not load the pending target (P3-1)"
        )
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_delayed_old_response_cannot_repaint_new_rows(
    isolated_global_config_dir: Path,
) -> None:
    """A testA re-read released after testE is selected must not clobber
    the remounted testE rows or resurrect testA rows."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        reread_gate = asyncio.Event()
        fake.gates[("get_config", TEST_A)] = reread_gate
        _editor_for(app.screen, FIELD_A).value = "40"

        await pilot.click("#settings-save")
        await pilot.pause()
        await pilot.pause()
        _editor_for(app.screen, FIELD_A).value = "41"

        await _switch_via_control(pilot, app, TEST_E)
        await _await_condition(
            pilot,
            "switch overlay",
            lambda: isinstance(app.screen, TargetSwitchOverlay),
        )
        await pilot.click("#switch-discard")
        await _await_condition(
            pilot,
            "testE header",
            lambda: f"selected: {TEST_E}" in screen_text(app),
        )
        assert _editor_for(app.screen, FIELD_E).value == "3"

        reread_gate.set()
        for _ in range(10):
            await pilot.pause()

        assert f"selected: {TEST_E}" in screen_text(app)
        assert _editor_for(app.screen, FIELD_E).value == "3"
        assert not _row_present(app.screen, FIELD_A), (
            "stale testA re-read repainted the selected testE rows (P3-1)"
        )
        fake.assert_only_receipted_writes()


# ── P3-2. Reachable one-shot reveal after target load ─────────────────────


@pytest.mark.asyncio
async def test_reveal_reachable_after_load_sends_one_request(
    isolated_global_config_dir: Path,
) -> None:
    """The P3-2 route: the remounted Environment row offers Reveal, and
    confirming sends exactly one profile-explicit request whose value
    displays once, matched only in process."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        button = _secret_row_button(app.screen, SECRET_KEY)
        assert MASKED in screen_text(app)

        button.press()
        await _await_condition(
            pilot,
            "reveal overlay",
            lambda: isinstance(app.screen, RevealSecretOverlay),
        )
        assert fake.reveal_calls == []
        await pilot.click("#reveal-once")
        await _await_condition(
            pilot, "reveal request", lambda: len(fake.reveal_calls) == 1
        )
        await _await_condition(
            pilot,
            "value shown once",
            lambda: fake.reveal_value in " ".join(_widget_texts(app)),
        )

        assert fake.reveal_calls == [(TEST_A, SECRET_KEY)]
        assert ("reveal_env", CONNECTION, TEST_A) in fake.calls
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_revealed_value_clears_on_close_and_stays_out_of_state(
    isolated_global_config_dir: Path,
) -> None:
    """Close wipes the value: no plaintext in widgets, view state, notices,
    or frame recordings — recorded as booleans only."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        _secret_row_button(app.screen, SECRET_KEY).press()
        await _await_condition(
            pilot,
            "reveal overlay",
            lambda: isinstance(app.screen, RevealSecretOverlay),
        )
        await pilot.click("#reveal-once")
        await _await_condition(
            pilot, "reveal request", lambda: len(fake.reveal_calls) == 1
        )
        await _await_condition(
            pilot,
            "value shown",
            lambda: fake.reveal_value in " ".join(_widget_texts(app)),
        )

        await pilot.press("escape")
        await _await_condition(
            pilot,
            "overlay close",
            lambda: isinstance(app.screen, SettingsWorkspaceScreen),
        )

        leaked_widgets = fake.reveal_value in " ".join(_widget_texts(app))
        assert not leaked_widgets, "revealed value survives close in widgets (P3-2)"
        held = app.screen
        assert isinstance(held, SettingsWorkspaceScreen)
        leaked_view = fake.reveal_value in repr(held._view)
        assert not leaked_view, "revealed value persists in view state (P3-2)"
        leaked_notice = fake.reveal_value in (app.composer.notice or "")
        assert not leaked_notice, "revealed value leaks into a notice (P3-2)"
        leaked_files = fake.reveal_value in " ".join(
            _recording_texts(isolated_global_config_dir)
        )
        assert not leaked_files, "revealed value persists in recordings (P3-2)"
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_reopen_starts_masked_with_no_background_refetch(
    isolated_global_config_dir: Path,
) -> None:
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        _secret_row_button(app.screen, SECRET_KEY).press()
        await _await_condition(
            pilot,
            "reveal overlay",
            lambda: isinstance(app.screen, RevealSecretOverlay),
        )
        await pilot.click("#reveal-once")
        await _await_condition(
            pilot, "reveal request", lambda: len(fake.reveal_calls) == 1
        )
        await pilot.press("escape")
        await _await_condition(
            pilot,
            "overlay close",
            lambda: isinstance(app.screen, SettingsWorkspaceScreen),
        )

        assert MASKED in screen_text(app)
        _secret_row_button(app.screen, SECRET_KEY).press()
        await _await_condition(
            pilot,
            "reveal overlay reopened",
            lambda: isinstance(app.screen, RevealSecretOverlay),
        )
        for _ in range(3):
            await pilot.pause()
        assert fake.reveal_calls == [(TEST_A, SECRET_KEY)]
        assert MASKED in screen_text(app)
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_no_secret_state_survives_a_target_switch(
    isolated_global_config_dir: Path,
) -> None:
    """Remount replaces the whole subtree: after revealing on testA,
    closing, and switching back, testE shows no secret row, no Reveal
    control, and no trace of the value."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        _secret_row_button(app.screen, SECRET_KEY).press()
        await _await_condition(
            pilot,
            "reveal overlay",
            lambda: isinstance(app.screen, RevealSecretOverlay),
        )
        await pilot.click("#reveal-once")
        await _await_condition(
            pilot, "reveal request", lambda: len(fake.reveal_calls) == 1
        )
        await pilot.press("escape")
        await _await_condition(
            pilot,
            "overlay close",
            lambda: isinstance(app.screen, SettingsWorkspaceScreen),
        )
        _editor_for(app.screen, FIELD_A).value = "40"

        await _switch_via_control(pilot, app, TEST_E)
        await _await_condition(
            pilot,
            "switch overlay",
            lambda: isinstance(app.screen, TargetSwitchOverlay),
        )
        await pilot.click("#switch-discard")
        await _await_condition(
            pilot,
            "testE header",
            lambda: f"selected: {TEST_E}" in screen_text(app),
        )

        assert not _row_present(app.screen, SECRET_KEY)
        revealers = [
            button
            for button in app.screen.query(Button)
            if str(button.label).strip().lower() == "reveal"
        ]
        assert revealers == []
        leaked = fake.reveal_value in " ".join(_widget_texts(app))
        assert not leaked, "testA secret value survives on testE screen (P3-2)"
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_sixth_reveal_in_30s_shows_rate_limit_without_plaintext(
    isolated_global_config_dir: Path,
) -> None:
    """The fake models the server's 5-per-30s window: the sixth confirm
    surfaces rate-limit state and shows no stale value."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    fake.reveal_limit_from = 6
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_to_test_a(pilot, app, "discard")
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )
        for _ in range(5):
            _secret_row_button(app.screen, SECRET_KEY).press()
            await _await_condition(
                pilot,
                "reveal overlay",
                lambda: isinstance(app.screen, RevealSecretOverlay),
            )
            await pilot.click("#reveal-once")
            await pilot.pause()
            await pilot.press("escape")
            await _await_condition(
                pilot,
                "overlay close",
                lambda: isinstance(app.screen, SettingsWorkspaceScreen),
            )
        assert len(fake.reveal_calls) == 5

        _secret_row_button(app.screen, SECRET_KEY).press()
        await _await_condition(
            pilot,
            "reveal overlay",
            lambda: isinstance(app.screen, RevealSecretOverlay),
        )
        await pilot.click("#reveal-once")
        await _await_condition(
            pilot, "sixth reveal request", lambda: len(fake.reveal_calls) == 6
        )
        await _await_condition(
            pilot,
            "rate-limit state",
            lambda: "rate limit" in screen_text(app).lower()
            or "429" in screen_text(app),
        )

        leaked = fake.reveal_value in " ".join(_widget_texts(app))
        assert not leaked, "stale plaintext shown on rate-limited reveal (P3-2)"
        fake.assert_only_receipted_writes()


# ── Source guard: no direct opener calls in acceptance ────────────────────


def test_acceptance_never_calls_the_switch_or_reveal_openers() -> None:
    """Acceptance rule 3, enforced mechanically: no test in this file may
    call the switch/reveal opener methods. AST-based, so prose mentions
    in comments and docstrings cannot trip it."""
    # Built without the literal identifiers so the prohibition pattern
    # itself stays grep-clean in this file; only real calls can trip it.
    forbidden = {"open_" + name for name in ("target_switch", "reveal")}
    tree = ast.parse(Path(__file__).read_text(encoding="utf-8"))
    violations: list[str] = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        target = node.func
        name = ""
        if isinstance(target, ast.Name):
            name = target.id
        elif isinstance(target, ast.Attribute):
            name = target.attr
        if name in forbidden:
            violations.append(f"{name} called at line {node.lineno}")
    assert violations == [], (
        "acceptance must reach overlays through live controls, "
        f"never direct opener calls: {violations}"
    )
