"""P4-1 typed live-load failures and the live-shape mount.

CFG-P4 residuals against published ``v0.6.4`` (``4497048``): the fixture
remount passes (kept here as the control), but the real app loader
collapses every schema/env failure — transport error, undecodable body,
unreachable host — into the same silent placeholder via broad exception
catches. These tests force each failure class through the real ``/config``
loader and require a distinct, typed safe state instead.

Pinned P4 contract (dev-4, U1): every load failure surfaces an actionable
placeholder notice naming the phase (``schema``/``env``) and the failure
class (the ``SettingsError`` reason token, or ``decode`` for undecodable
bodies). Distinct failures never share one silent state.

Hard rules for this file:

* Never call the switch/reveal opener methods directly — enforced by the
  AST source guard at the bottom of this file.
* Profiles are the P2 legal active family; ``testB`` and ``default`` are
  never write targets, and any write log naming them — or any profile
  the run did not receipt — hard-fails the test.
* Header movement alone never satisfies an assertion: every load test
  asserts mounted rows (or their reason-labeled absence).
* Secret values are random in-memory canaries, recorded as booleans
  only. No plaintext in logs or evidence.
"""

from __future__ import annotations

import ast
import asyncio
from pathlib import Path
from typing import Any

import pytest
from textual.css.query import NoMatches
from textual.pilot import Pilot
from textual.widgets import Button, Input

from talaria.domain.commands import LocalInvocation, resolve_command
from talaria.transport.connection_set import EnsureReport
from talaria.transport.settings import SettingsError
from talaria.ui.app import TalariaApp
from talaria.ui.settings_overlays import TargetSwitchOverlay
from talaria.ui.settings_widgets import row_widget_id
from talaria.ui.settings_workspace import SettingsWorkspaceScreen
from tests.ui.conftest import event, paused_app, screen_text

CONNECTION = "local"
TEST_A = "talaria-v062-cfg-p2-local-active-a"
TEST_E = "talaria-v062-cfg-p2-local-active-switch"
TEST_B = "testB"
URL_LOCAL = "http://127.0.0.1:8765"
SECRET_KEY = "EXAMPLE_P4_KEY"
MASKED = "sk-…p4ab"
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
    """One configured connection; loads must not disturb sessions."""

    def __init__(self, home: str) -> None:
        self._home = home

    @property
    def home(self) -> str:
        return self._home

    async def ensure(self, profile: str) -> EnsureReport:
        return EnsureReport(profile, "already_up", "connected")


class _FakeSettingsClient:
    """Recording Hermes stand-in with injectable load failures."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, str, str]] = []
        self.writes: list[tuple[str, str, str]] = []
        self.receipts: set[str] = set()
        self.configs: dict[str, dict[str, Any]] = {}
        self.envs: dict[str, Any] = {}
        self.schema_failures: dict[str, Exception] = {}
        self.schema_bodies: dict[str, Any] = {}
        self.env_failures: dict[str, Exception] = {}
        self.env_bodies: dict[str, Any] = {}
        self.gates: dict[tuple[str, str], asyncio.Event] = {}

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

    def _schema_body(self, profile: str) -> dict[str, Any]:
        if profile in self.schema_bodies:
            body: dict[str, Any] = self.schema_bodies[profile]
            return body
        field = FIELD_E if profile == TEST_E else FIELD_A
        description = (
            "Maximum API retries."
            if field == FIELD_E
            else "Maximum agent turns."
        )
        return {
            "fields": {
                field: {
                    "type": "number",
                    "description": description,
                    "category": "agent",
                }
            },
            "category_order": ["agent"],
        }

    async def get_schema(self, target: Any) -> Any:
        self._log("get_schema", target)
        await self._gate("get_schema", target.profile_name)
        failure = self.schema_failures.get(target.profile_name)
        if failure is not None:
            raise failure
        return self._schema_body(target.profile_name)

    async def get_config(self, target: Any) -> Any:
        self._log("get_config", target)
        await self._gate("get_config", target.profile_name)
        saved = dict(self.configs.get(target.profile_name, {"agent": {}}))
        return {"saved": saved, "effective": dict(saved), "defaults": {}}

    async def put_config(self, target: Any, patch: Any) -> Any:
        self._log("put_config", target)
        saved = self.configs.get(target.profile_name, {})
        merged = {key: value for key, value in saved.items()}
        for key, value in dict(patch).items():
            merged[key] = value
        self.configs[target.profile_name] = merged
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

    def assert_only_receipted_writes(self) -> None:
        """Hard-fail: a write to default/testB/unreceipted fails the run."""
        for method, _connection, profile in self.writes:
            if profile == "default" or profile == TEST_B or profile not in self.receipts:
                pytest.fail(
                    f"forbidden settings write: {method} targeted {profile!r} "
                    "(P4: writes only to run-receipted profiles)"
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
            "description": "P4 test key",
        }
    }
    fake.envs[TEST_E] = {}


async def _switch_and_discard(
    pilot: Pilot[None], app: TalariaApp, name: str, field: str, value: str
) -> None:
    """Edit the mounted field, choose ``name`` via Target, Discard."""
    row = app.screen.query_one(f"#{row_widget_id(field)}")
    editor: Input = row.query_one(Input)
    editor.value = value
    await pilot.click("#settings-target")
    label = f"{CONNECTION} / {name}"
    for _ in range(30):
        await pilot.pause()
        for button in app.screen.query(Button):
            if str(button.label).strip() == label:
                button.press()
                await pilot.pause()
                break
    await _await_condition(
        pilot,
        "switch overlay",
        lambda: isinstance(app.screen, TargetSwitchOverlay),
    )
    await pilot.click("#switch-discard")
    await _await_condition(
        pilot,
        f"{name} header",
        lambda: f"selected: {name}" in screen_text(app),
    )


def _live_shape_schema() -> dict[str, Any]:
    """Realistic multi-field schema modeled on the measured live shape:
    several categories, all five types, options, and one unknown type."""
    return {
        "fields": {
            "agent.max_turns": {
                "type": "number",
                "description": "Maximum agent turns.",
                "category": "agent",
            },
            "agent.image_input_mode": {
                "type": "select",
                "description": "Image input mode.",
                "category": "agent",
                "options": ["auto", "native", "text"],
            },
            "timezone": {
                "type": "string",
                "description": "IANA timezone.",
                "category": "general",
            },
            "display.show_reasoning": {
                "type": "boolean",
                "description": "Show reasoning blocks.",
                "category": "display",
            },
            "fallback_providers": {
                "type": "list",
                "description": "Fallback providers.",
                "category": "general",
            },
            "example.future": {
                "type": "color",
                "description": "Unknown future type.",
                "category": "general",
            },
        },
        "category_order": ["general", "agent", "display"],
    }


# ── Control: the fixture remount still passes ─────────────────────────────


@pytest.mark.asyncio
async def test_fixture_remount_control_stays_green(
    isolated_global_config_dir: Path,
) -> None:
    """P4 control, not acceptance: the valid-fixture E→A discard remount
    mounts new rows, removes stale rows, and mounts the secret row. This
    passing test proves the harness can remount; it proves nothing about
    the live loader, which is what the failing tests below exercise."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    app, _ = _live_config_app(fake)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _switch_and_discard(pilot, app, TEST_A, FIELD_E, "7")

        assert _row_present(app.screen, FIELD_A)
        editor_row = app.screen.query_one(f"#{row_widget_id(FIELD_A)}")
        editor: Input = editor_row.query_one(Input)
        assert editor.value == "25"
        assert not _row_present(app.screen, FIELD_E)
        assert _row_present(app.screen, SECRET_KEY)
        fake.assert_only_receipted_writes()


# ── Failing: typed load failures need distinct safe states ────────────────


@pytest.mark.asyncio
@pytest.mark.parametrize("reason", ["unreachable", "timeout"])
async def test_schema_transport_failure_names_its_reason(
    isolated_global_config_dir: Path, reason: str
) -> None:
    """A schema fetch raising a typed transport error must surface a
    labeled placeholder naming the phase and reason — never the same
    silent state as every other failure class."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    fake.schema_failures[TEST_A] = SettingsError(reason, f"schema fetch {reason}")  # type: ignore[arg-type]
    app, _ = _live_config_app(fake, current=TEST_A)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )

        text = screen_text(app).lower()
        assert "schema" in text, (
            f"schema {reason} renders no schema-named safe state (P4-1)"
        )
        assert reason in text, (
            f"schema {reason} renders no typed reason (P4-1 residual: "
            "broad catch erases the failure class)"
        )
        assert _row_present(app.screen, "hermes.schema")
        assert not _row_present(app.screen, FIELD_A)
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_schema_decode_failure_names_decode_distinctly(
    isolated_global_config_dir: Path,
) -> None:
    """An undecodable schema body (entry missing its type) must surface a
    decode-named safe state, distinct from any transport failure."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    fake.schema_bodies[TEST_A] = {
        "fields": {"agent.max_turns": {"description": "No type.", "category": "agent"}},
        "category_order": ["agent"],
    }
    app, _ = _live_config_app(fake, current=TEST_A)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )

        text = screen_text(app).lower()
        assert "schema" in text, "decode failure renders no schema state (P4-1)"
        assert "decode" in text, (
            "decode failure renders no decode-typed state (P4-1 residual: "
            "SettingsDecodeError swallowed by the broad catch)"
        )
        assert _row_present(app.screen, "hermes.schema")
        assert not _row_present(app.screen, FIELD_A)
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_env_transport_failure_keeps_schema_and_names_env(
    isolated_global_config_dir: Path,
) -> None:
    """Env fetch failure with good schema: schema rows mount normally and
    the env outage gets its own named state — never silent absence that
    reads identically to no-env-configured."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    fake.env_failures[TEST_A] = SettingsError("unreachable", "env fetch unreachable")
    app, _ = _live_config_app(fake, current=TEST_A)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )

        assert _row_present(app.screen, FIELD_A)
        text = screen_text(app).lower()
        assert "env" in text, (
            "env outage renders no env-named state (P4 residual: silent "
            "empty env reads as no-env-configured)"
        )
        assert "unreachable" in text
        fake.assert_only_receipted_writes()


@pytest.mark.asyncio
async def test_env_decode_failure_names_env_distinctly(
    isolated_global_config_dir: Path,
) -> None:
    """An undecodable env body must surface an env-named decode state
    while the good schema beside it mounts normally."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    fake.env_bodies[TEST_A] = ["not", "an", "object"]
    app, _ = _live_config_app(fake, current=TEST_A)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )

        assert _row_present(app.screen, FIELD_A)
        text = screen_text(app).lower()
        assert "env" in text, "env decode failure renders no env state (P4)"
        assert "decode" in text, (
            "env decode failure renders no decode-typed state (P4 residual)"
        )
        fake.assert_only_receipted_writes()


# ── Live-shape mount: rows, not just header ───────────────────────────────


@pytest.mark.asyncio
async def test_live_shape_schema_mounts_every_row_type(
    isolated_global_config_dir: Path,
) -> None:
    """Shape-fidelity anchor: a realistic multi-field, multi-category
    schema with all five types plus one unknown type mounts every row
    (unknown read-only); the header alone cannot satisfy this test."""
    fake = _FakeSettingsClient()
    _seed_remount_fixture(fake)
    fake.schema_bodies[TEST_A] = _live_shape_schema()
    fake.configs[TEST_A] = {
        "agent": {"max_turns": 25, "image_input_mode": "auto"},
        "display": {"show_reasoning": True},
        "fallback_providers": ["a", "b"],
    }
    app, _ = _live_config_app(fake, current=TEST_A)
    async with app.run_test(size=SIZE) as pilot:
        await _open_config(pilot, app)
        await _await_condition(
            pilot,
            "testA header",
            lambda: f"selected: {TEST_A}" in screen_text(app),
        )

        for key in (
            "agent.max_turns",
            "agent.image_input_mode",
            "timezone",
            "display.show_reasoning",
            "fallback_providers",
            "example.future",
        ):
            assert _row_present(app.screen, key), (
                f"live-shape row {key} did not mount (P4-1)"
            )
        assert not _row_present(app.screen, "hermes.schema")
        fake.assert_only_receipted_writes()


# ── Source guard: no direct opener calls in acceptance ────────────────────


def test_acceptance_never_calls_the_switch_or_reveal_openers() -> None:
    """Acceptance rule 4, enforced mechanically: no test in this file may
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
