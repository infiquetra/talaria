"""P2-1 live Stay/Save/Discard and P2-2 reachable one-shot reveal.

CFG-P2 residuals against published ``v0.6.2`` (``cb93c5d``): the switch and
reveal overlays exist but are unreachable from live ``/config`` — no target
control drives the switch prompt, no secret row offers Reveal, Save is not
connected to live target loading, and reveal responses are discarded by app
dispatch. Every test here drives the real ``TalariaApp`` with ``/config``
opened normally and a recording fake settings client; all fail on ``cb93c5d``
for exactly those absences.

Hard rules for this file:

* Never call the switch/reveal opener methods directly — reaching
  the overlays through real controls is what is under test. (Grep-verified
  before commit; the production control may call them internally.)
* Profiles are the P2 legal family
  (``talaria-v062-cfg-p2-local-active-{a,switch}``); ``testB`` and ``default``
  are never write targets, and any write log naming them — or any profile
  the run did not receipt — hard-fails the test.
* Secret values are random in-memory canaries. Equality/visibility/cleared
  are recorded as booleans; no plaintext reaches notices, logs, recordings,
  or evidence. Tracebacks on failure may show the random canary (the same
  exposure the committed redaction suites accept); the evidence log itself
  is curated and canary-free.

Pinned P2-1/P2-2 interface contract (dev-4): a ``#settings-target`` header
control; a target picker whose option buttons are labeled exactly
``"<connection_id> / <profile_name>"``; per-secret-row Reveal buttons
labeled ``Reveal``; switch completion loads the new target through its
connection client; Save commits the switch only after the old target
re-read succeeds.
"""

from __future__ import annotations

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
SECRET_KEY = "EXAMPLE_P2_KEY"
MASKED = "sk-…p2ab"
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
    """One configured connection; settings must never disturb session home."""

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
    """Recording Hermes stand-in. Values stay in memory; logs name routes."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, str, str]] = []
        self.writes: list[tuple[str, str, str]] = []
        self.endpoints: list[str] = []
        self.receipts: set[str] = set()
        self.configs: dict[str, dict[str, Any]] = {}
        self.envs: dict[str, Any] = {}
        self.put_failures: dict[str, SettingsError] = {}
        self.gates: dict[tuple[str, str], asyncio.Event] = {}
        self.reveal_calls: list[tuple[str, str]] = []
        self.reveal_limit_from: int | None = None
        self.reveal_value = f"p2-canary-{secrets.token_hex(4)}"

    def factory(self, endpoint: str) -> _FakeSettingsClient:
        self.endpoints.append(endpoint)
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

    def _schema_body(self) -> dict[str, Any]:
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

    async def get_schema(self, target: Any) -> Any:
        self._log("get_schema", target)
        await self._gate("get_schema", target.profile_name)
        return self._schema_body()

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
                    "(P2-1: writes only to run-receipted profiles)"
                )


def _live_config_app(
    fake: _FakeSettingsClient, *, current: str = TEST_A
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
    """Activate the live target control and choose ``name`` (P2-1 route)."""
    try:
        app.screen.query_one("#settings-target", Button)
    except NoMatches:
        pytest.fail(
            "live /config exposes no reachable target control "
            "(P2-1 residual: target picker missing)"
        )
    await pilot.click("#settings-target")
    label = f"{CONNECTION} / {name}"
    for _ in range(30):
        await pilot.pause()
        for button in app.screen.query(Button):
            if str(button.label).strip() == label:
                button.press()
                await pilot.pause()
                return
    pytest.fail(f"target picker offers no option labeled {label!r} (P2-1)")


def _schema_editor(screen: Any) -> Input:
    """The live schema-row editor, found by row id whatever it shows."""
    row = screen.query_one(f"#{row_widget_id('agent.max_turns')}")
    editor: Input = row.query_one(Input)
    return editor


def _seed_switch_fixture(fake: _FakeSettingsClient) -> None:
    fake.receipts.update({TEST_A, TEST_E})
    fake.configs[TEST_A] = {"agent": {"max_turns": 25}}
    fake.configs[TEST_E] = {"agent": {"max_turns": 30}}


def _secret_row_button(screen: Any, key: str) -> Button:
    """The Reveal control attached to one secret row (P2-2 route)."""
    try:
        row = screen.query_one(f"#{row_widget_id(key)}")
    except NoMatches:
        pytest.fail(
            f"live /config renders no secret row for {key} "
            "(P2-2 residual: env masks not fetched into the view)"
        )
    for button in row.query(Button):
        if str(button.label).strip().lower() == "reveal":
            found: Button = button
            return found
    pytest.fail(
        f"secret row {key} exposes no Reveal control (P2-2 residual)"
    )


def _seed_reveal_fixture(fake: _FakeSettingsClient) -> None:
    fake.receipts.update({TEST_A, TEST_E})
    fake.configs[TEST_A] = {"agent": {"max_turns": 25}}
    fake.configs[TEST_E] = {"agent": {"max_turns": 30}}
    fake.envs[TEST_A] = {
        SECRET_KEY: {
            "is_set": True,
            "redacted_value": MASKED,
            "description": "P2 test key",
        }
    }


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


# ── P2-1. Live Stay / Save / Discard ─────────────────────────────────────


@pytest.mark.asyncio
async def test_target_control_routes_choice_through_switch_overlay(
    isolated_global_config_dir: Path,
) -> None:
    """The P2-1 route itself: a reachable control lists both legal targets
    and choosing testE opens the switch overlay — without any direct
    opener call from this test."""
    fake = _FakeSettingsClient()
    _seed_switch_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        screen = await _open_config(pilot, app)
        _schema_editor(screen).value = "40"

        try:
            app.screen.query_one("#settings-target", Button)
        except NoMatches:
            pytest.fail(
                "live /config exposes no reachable target control "
                "(P2-1 residual: target picker missing)"
            )
        await pilot.click("#settings-target")
        await pilot.pause()
        labels = {str(button.label).strip() for button in app.screen.query(Button)}
        assert f"{CONNECTION} / {TEST_A}" in labels
        assert f"{CONNECTION} / {TEST_E}" in labels

        await _switch_via_control(pilot, app, TEST_E)
        await _await_condition(
            pilot,
            "switch overlay",
            lambda: isinstance(app.screen, TargetSwitchOverlay),
        )
        assert f"Save to {TEST_A}" in screen_text(app)
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_stay_preserves_target_edit_and_issues_no_write(
    isolated_global_config_dir: Path,
) -> None:
    fake = _FakeSettingsClient()
    _seed_switch_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        screen = await _open_config(pilot, app)
        _schema_editor(screen).value = "40"

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
        assert _schema_editor(app.screen).value == "40"
        assert fake.writes == []
        assert ("get_config", CONNECTION, TEST_E) not in fake.calls
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_discard_clears_edit_and_loads_new_target(
    isolated_global_config_dir: Path,
) -> None:
    fake = _FakeSettingsClient()
    _seed_switch_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        screen = await _open_config(pilot, app)
        _schema_editor(screen).value = "40"

        await _switch_via_control(pilot, app, TEST_E)
        await _await_condition(
            pilot,
            "switch overlay",
            lambda: isinstance(app.screen, TargetSwitchOverlay),
        )
        await pilot.click("#switch-discard")
        await _await_condition(
            pilot,
            "testE selected",
            lambda: f"selected: {TEST_E}" in screen_text(app),
        )

        assert fake.writes == []
        assert ("get_config", CONNECTION, TEST_E) in fake.calls
        assert _schema_editor(app.screen).value == "30"
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_save_writes_old_target_then_switches_after_reread(
    isolated_global_config_dir: Path,
) -> None:
    """Save-before-switch: PUT and re-read hit testA while it stays
    selected; the switch to testE commits only after the re-read lands."""
    fake = _FakeSettingsClient()
    _seed_switch_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        screen = await _open_config(pilot, app)
        _schema_editor(screen).value = "40"

        await _switch_via_control(pilot, app, TEST_E)
        await _await_condition(
            pilot,
            "switch overlay",
            lambda: isinstance(app.screen, TargetSwitchOverlay),
        )
        await pilot.click("#switch-save")
        await _await_condition(
            pilot,
            "testE selected after save",
            lambda: f"selected: {TEST_E}" in screen_text(app),
        )

        puts = [call for call in fake.calls if call[0] == "put_config"]
        assert puts == [("put_config", CONNECTION, TEST_A)]
        rereads = [
            index
            for index, call in enumerate(fake.calls)
            if call == ("get_config", CONNECTION, TEST_A)
        ]
        put_index = next(
            index
            for index, call in enumerate(fake.calls)
            if call == ("put_config", CONNECTION, TEST_A)
        )
        assert any(index > put_index for index in rereads), (
            "old-target re-read must follow the PUT before the switch (P2-1)"
        )
        assert ("get_config", CONNECTION, TEST_E) in fake.calls
        assert fake.configs[TEST_A] == {"agent": {"max_turns": 40}}
        assert fake.configs[TEST_E] == {"agent": {"max_turns": 30}}
        assert _schema_editor(app.screen).value == "30"
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_failed_save_retains_edit_selection_and_shows_rejection(
    isolated_global_config_dir: Path,
) -> None:
    fake = _FakeSettingsClient()
    _seed_switch_fixture(fake)
    fake.put_failures[TEST_A] = SettingsError("http_error", "PUT refused: locked")
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        screen = await _open_config(pilot, app)
        _schema_editor(screen).value = "40"

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
        assert _schema_editor(app.screen).value == "40"
        assert ("get_config", CONNECTION, TEST_E) not in fake.calls
        assert [call for call in fake.calls if call[0] == "put_config"] == [
            ("put_config", CONNECTION, TEST_A)
        ]
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_delayed_old_target_response_is_dropped_after_switch(
    isolated_global_config_dir: Path,
) -> None:
    """A testA re-read released after testE is selected must not clobber
    the testE view: same-target footer save parks its re-read on a gate,
    a discard-switch loads testE, then the stale reply is dropped."""
    fake = _FakeSettingsClient()
    _seed_switch_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        screen = await _open_config(pilot, app)
        reread_gate = asyncio.Event()
        fake.gates[("get_config", TEST_A)] = reread_gate
        _schema_editor(screen).value = "40"

        await pilot.click("#settings-save")
        await pilot.pause()
        await pilot.pause()
        _schema_editor(app.screen).value = "41"

        await _switch_via_control(pilot, app, TEST_E)
        await _await_condition(
            pilot,
            "switch overlay",
            lambda: isinstance(app.screen, TargetSwitchOverlay),
        )
        await pilot.click("#switch-discard")
        await _await_condition(
            pilot,
            "testE selected",
            lambda: f"selected: {TEST_E}" in screen_text(app),
        )
        assert _schema_editor(app.screen).value == "30"

        reread_gate.set()
        for _ in range(10):
            await pilot.pause()

        assert f"selected: {TEST_E}" in screen_text(app)
        assert _schema_editor(app.screen).value == "30"
        fake.assert_only_receipted_writes()


# ── P2-2. Reachable one-shot secret reveal ───────────────────────────────


@pytest.mark.asyncio
async def test_secret_row_exposes_reachable_reveal_control(
    isolated_global_config_dir: Path,
) -> None:
    """The P2-2 route: a masked secret row with a Reveal control that opens
    the confirm overlay — without any direct opener call."""
    fake = _FakeSettingsClient()
    _seed_reveal_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        button = _secret_row_button(app.screen, SECRET_KEY)
        assert SECRET_KEY in screen_text(app)
        assert MASKED in screen_text(app)

        button.press()
        await _await_condition(
            pilot,
            "reveal overlay",
            lambda: isinstance(app.screen, RevealSecretOverlay),
        )
        assert SECRET_KEY in screen_text(app)
        assert fake.reveal_calls == []
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_confirm_sends_one_profile_explicit_reveal(
    isolated_global_config_dir: Path,
) -> None:
    fake = _FakeSettingsClient()
    _seed_reveal_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
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
    _seed_reveal_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
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
        assert not leaked_widgets, "revealed value survives close in widgets (P2-2)"
        # White-box read of the held view model: unrendered fields must be
        # clean too, and only the holder can say so.
        held = app.screen
        assert isinstance(held, SettingsWorkspaceScreen)
        leaked_view = fake.reveal_value in repr(held._view)
        assert not leaked_view, "revealed value persists in view state (P2-2)"
        leaked_notice = fake.reveal_value in (app.composer.notice or "")
        assert not leaked_notice, "revealed value leaks into a notice (P2-2)"
        leaked_files = fake.reveal_value in " ".join(
            _recording_texts(isolated_global_config_dir)
        )
        assert not leaked_files, "revealed value persists in recordings (P2-2)"
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_reopen_starts_masked_with_no_background_refetch(
    isolated_global_config_dir: Path,
) -> None:
    fake = _FakeSettingsClient()
    _seed_reveal_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
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
async def test_sixth_reveal_in_30s_shows_rate_limit_without_plaintext(
    isolated_global_config_dir: Path,
) -> None:
    """The fake models the server's 5-per-30s window: the sixth confirm
    surfaces rate-limit state and shows no stale value."""
    fake = _FakeSettingsClient()
    _seed_reveal_fixture(fake)
    fake.reveal_limit_from = 6
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
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
        assert not leaked, "stale plaintext shown on rate-limited reveal (P2-2)"
        fake.assert_only_receipted_writes()
