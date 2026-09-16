"""The settings workspace: target headers, rows, overlays, and honesty copy.

Covers the A1-3/A1-4/A1-7/A1-10 UI halves, the canonical-``default`` display
rule, read-only host rows, and the row/overlay contracts the installed
A1-21–26 scenarios and the eight usability journeys drive (effect chips,
save summary, model/secrets/restart/reset overlays, search, small-terminal
layout, focus return).

Interface under test (unimplemented until CFG B5, ``dev-4``; view models
until CFG B2, ``dev-1-3``):

* ``talaria.ui.settings_workspace.SettingsWorkspaceScreen(view,
  on_command=None)`` — renders a domain ``SettingsWorkspaceView``,
  reports typed commands through ``on_command``, refreshes through
  ``update_view``. Widget ids: ``#settings-search``, ``#settings-save``,
  ``#settings-discard``, ``#settings-group-<owner>``, ``#wake-toggle``.
* ``talaria.ui.settings_overlays`` — ``TargetSwitchOverlay``,
  ``ResetConfirmOverlay``, ``RestartConfirmOverlay``,
  ``RevealSecretOverlay``, ``ModelPickerOverlay``; each takes
  ``(view, on_result)`` and reports a small frozen result record.
* Workspace orchestration: ``open_target_switch``,
  ``open_reset``, ``open_restart``, ``open_reveal``, ``open_model_picker``.

Domain records constructed here (all ``talaria.domain.settings``):
``SettingsWorkspaceView``, ``TargetHeaderView``, ``SettingsRowGroupView``,
``FieldRowView``, ``SaveSummaryView``/``SaveSummaryGroup``,
``TargetSwitchPrompt``, ``ResetConfirmView``, ``RestartConfirmView``,
``RevealSecretView``, ``ModelPickerView``/``ModelPickerOption``.

Copy pinned here is the operator-facing honesty contract: current/selected
labels, ownership and provenance chips, the five effect sentences, the D8
restart titles, and read-only markers. Layout is free; words are not.
"""

from __future__ import annotations

import importlib
from collections.abc import Callable
from typing import Any, cast

import pytest
from textual.app import App, ComposeResult
from textual.css.query import NoMatches
from textual.pilot import Pilot
from textual.widgets import Button, Input, Static

from talaria.ui.app import TalariaApp
from talaria.ui.settings_widgets import row_widget_id
from tests.ui.conftest import screen_text


def _require_module(name: str) -> Any:
    try:
        return importlib.import_module(name)
    except ImportError as exc:
        pytest.fail(f"unimplemented interface {name} (CFG v0.6.2 B2/B5): {exc}")


def _require_attr(module: Any, name: str) -> Any:
    try:
        return getattr(module, name)
    except AttributeError:
        pytest.fail(
            f"unimplemented interface {module.__name__}.{name} (CFG v0.6.2 B2/B5)"
        )


def _settings() -> Any:
    return _require_module("talaria.domain.settings")


def _workspace_mod() -> Any:
    return _require_module("talaria.ui.settings_workspace")


def _overlays_mod() -> Any:
    return _require_module("talaria.ui.settings_overlays")


def _commands() -> Any:
    return _require_module("talaria.domain.settings_commands")


SIZE = (120, 36)
SMALL = (80, 24)


class _Host(App[None]):
    """Mounts one workspace screen or overlay the way the app seam will."""

    def __init__(self, factory: Callable[[], Any]) -> None:
        super().__init__()
        self._factory = factory
        self.result: Any = None
        self.view: Any = None

    def compose(self) -> ComposeResult:
        yield Static("behind the workspace")

    def open_workspace(self) -> None:
        self.view = self._factory()
        self.push_screen(self.view, self._capture)

    def _capture(self, result: Any) -> None:
        self.result = result


async def _mounted(pilot: Pilot[None], host: _Host) -> Any:
    host.open_workspace()
    for _ in range(3):
        await pilot.pause()
    assert host.view is not None
    return host.view


def _text(host: _Host) -> str:
    """Rendered text of the minimal host.

    ``screen_text`` is typed for the real app; the host quacks alike
    (``export_screenshot``), so the cast lives here rather than at every
    call site.
    """
    return screen_text(cast("TalariaApp", host))


def _row(settings: Any, **overrides: Any) -> Any:
    fields: dict[str, Any] = {
        "key": "agent.max_turns",
        "label": "Maximum agent turns",
        "help_text": "Cap on agent turns per task.",
        "type": "number",
        "provenance": "saved",
        "default_value": 25,
        "saved_value": 40,
        "effective_value": 40,
        "pending_value": None,
        "effect": "next-session",
        "tier": 1,
        "read_only": False,
        "validation_message": None,
    }
    fields.update(overrides)
    return _require_attr(settings, "FieldRowView")(**fields)


def _view(settings: Any, **overrides: Any) -> Any:
    header = _require_attr(settings, "TargetHeaderView")(
        connection_label="local-fixture",
        current_profile="beta-fixture",
        selected_profile="alpha-fixture",
        auth_mode="loopback",
        hermes_version="0.21.3",
        shows_both_names=True,
    )
    groups = (
        _require_attr(settings, "SettingsRowGroupView")(
            owner="hermes-profile",
            title="Hermes profile",
            rows=(_row(settings),),
        ),
    )
    fields: dict[str, Any] = {
        "header": header,
        "groups": groups,
        "pending_count": 0,
        "save_enabled": False,
        "notice": "",
        "summary": None,
        "search_text": "",
        "wake_state": None,
        "reset_patch": {},
        "secrets": {},
        "model_picker": None,
    }
    fields.update(overrides)
    return _require_attr(settings, "SettingsWorkspaceView")(**fields)


def _model_picker(settings: Any) -> Any:
    option = _require_attr(settings, "ModelPickerOption")
    return _require_attr(settings, "ModelPickerView")(
        target=_require_attr(settings, "ConfigTarget")(
            connection_id="local-fixture", profile_name="alpha-fixture"
        ),
        options=(
            option(provider="example-provider", model="example-small"),
            option(provider="example-provider", model="example-large"),
        ),
        current_provider="example-provider",
        current_model="example-small",
    )


def _screen(settings: Any, requested: list[Any], **view_overrides: Any) -> Any:
    return _require_attr(_workspace_mod(), "SettingsWorkspaceScreen")(
        _view(settings, **view_overrides), on_command=requested.append
    )


def _row_inputs(view: Any) -> list[Input]:
    """Row editors: every Input except the search box."""
    search = view.query_one("#settings-search", Input)
    return [node for node in view.query(Input) if node is not search]


# ── A1-4 UI: the target header ───────────────────────────────────────────


@pytest.mark.asyncio
async def test_the_header_names_connection_current_selected_auth_and_version() -> None:
    settings = _settings()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested))

    async with host.run_test(size=SIZE) as pilot:
        await _mounted(pilot, host)
        text = _text(host)

        assert "local-fixture" in text
        assert "current: beta-fixture" in text
        assert "selected: alpha-fixture" in text
        assert "loopback" in text
        assert "0.21.3" in text


@pytest.mark.asyncio
async def test_the_canonical_default_id_is_shown_for_the_default_profile() -> None:
    """A1-16's UI rule: the workspace shows the canonical id ``default``
    and may additionally show its display name — never the display name
    alone, which would send writes at a name the server rejects."""
    settings = _settings()
    requested: list[Any] = []
    header = _require_attr(settings, "TargetHeaderView")(
        connection_label="local-fixture",
        current_profile="default",
        selected_profile="default",
        auth_mode="loopback",
        hermes_version="0.21.3",
        shows_both_names=False,
    )
    host = _Host(lambda: _screen(settings, requested, header=header))

    async with host.run_test(size=SIZE) as pilot:
        await _mounted(pilot, host)

        assert "selected: default" in _text(host)


# ── rows: ownership, provenance, effect, read-only ───────────────────────


@pytest.mark.asyncio
async def test_a_row_shows_owner_provenance_effect_and_help() -> None:
    settings = _settings()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested))

    async with host.run_test(size=SIZE) as pilot:
        await _mounted(pilot, host)
        text = _text(host)

        assert "Maximum agent turns" in text
        assert "Cap on agent turns per task." in text
        assert "hermes-profile" in text
        assert "saved" in text
        assert "takes effect for new sessions" in text


@pytest.mark.asyncio
async def test_a_talaria_row_never_renders_under_a_hermes_heading() -> None:
    """A1-5's UI half: ownership grouping is visible, and the adversarial
    pair (Talaria ``theme.name`` vs Hermes ``display.theme``) lands on
    opposite sides of it."""
    settings = _settings()
    requested: list[Any] = []
    groups = (
        _require_attr(settings, "SettingsRowGroupView")(
            owner="talaria-user",
            title="Talaria",
            rows=(
                _row(
                    settings,
                    key="theme.name",
                    label="Theme",
                    help_text="Talaria color theme.",
                    type="string",
                    provenance="saved",
                    default_value="refined-default",
                    saved_value="neutral-dark",
                    effective_value="neutral-dark",
                    effect="live",
                ),
            ),
        ),
        _require_attr(settings, "SettingsRowGroupView")(
            owner="hermes-profile",
            title="Hermes profile",
            rows=(
                _row(
                    settings,
                    key="display.theme",
                    label="Hermes terminal theme",
                    help_text="Hermes TUI presentation.",
                    type="string",
                    provenance="default",
                    default_value="dark",
                    saved_value="dark",
                    effective_value="dark",
                    effect="unverified",
                    tier=2,
                ),
            ),
        ),
    )
    host = _Host(lambda: _screen(settings, requested, groups=groups))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        # Both ownership containers exist under their pinned ids; the
        # partition itself is pinned at the domain seam (A1-5), and here
        # each side's content renders.
        view.query_one(f"#settings-group-{'talaria-user'}")
        view.query_one("#settings-group-hermes-profile")
        text = _text(host)

        assert "Theme" in text and "Hermes terminal theme" in text
        assert "talaria-user" in text and "hermes-profile" in text


@pytest.mark.asyncio
async def test_an_unknown_typed_row_is_read_only_with_its_type_named() -> None:
    """A1-7's UI half: no editor, a read-only marker, and the explanation."""
    settings = _settings()
    requested: list[Any] = []
    groups = (
        _require_attr(settings, "SettingsRowGroupView")(
            owner="hermes-profile",
            title="Hermes profile",
            rows=(
                _row(
                    settings,
                    key="example.future",
                    label="Future field",
                    help_text="Invented next release.",
                    type="color",
                    provenance="default",
                    default_value="red",
                    saved_value="red",
                    effective_value="red",
                    effect="unverified",
                    tier=2,
                    read_only=True,
                    validation_message="unsupported type: color",
                ),
            ),
        ),
    )
    host = _Host(lambda: _screen(settings, requested, groups=groups))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        text = _text(host)

        assert _row_inputs(view) == []
        assert "read-only" in text
        assert "unsupported type: color" in text
        assert "Future field" in text


@pytest.mark.asyncio
async def test_appearance_resume_last_session_is_a_first_class_row() -> None:
    """A1-7: ``display.resume_last_session`` renders as a Tier-1 row under
    its Desktop label, not as generic schema fallback."""
    settings = _settings()
    requested: list[Any] = []
    groups = (
        _require_attr(settings, "SettingsRowGroupView")(
            owner="hermes-profile",
            title="Hermes profile",
            rows=(
                _row(
                    settings,
                    key="display.resume_last_session",
                    label="Resume last session",
                    help_text="Reopen the previous session at launch.",
                    type="boolean",
                    provenance="default",
                    default_value=True,
                    saved_value=True,
                    effective_value=True,
                    effect="next-session",
                    tier=1,
                ),
            ),
        ),
    )
    host = _Host(lambda: _screen(settings, requested, groups=groups))

    async with host.run_test(size=SIZE) as pilot:
        await _mounted(pilot, host)
        text = _text(host)

        assert "Resume last session" in text
        assert "read-only" not in text


@pytest.mark.asyncio
async def test_host_status_rows_are_read_only_and_offer_no_mutation() -> None:
    """Coverage assertion 6: update check, local-model status, and migration
    plan render as status; no Hermes-update, local-model write, or
    gateway-migrate action is offered anywhere in the Host group."""
    settings = _settings()
    requested: list[Any] = []
    groups = (
        _require_attr(settings, "SettingsRowGroupView")(
            owner="hermes-host",
            title="Host",
            rows=(
                _row(
                    settings,
                    key="host.hermes_update",
                    label="Hermes update",
                    help_text="Read-only status.",
                    type="string",
                    provenance="default",
                    default_value="up to date",
                    saved_value="up to date",
                    effective_value="up to date",
                    effect="unverified",
                    tier=1,
                    read_only=True,
                ),
                _row(
                    settings,
                    key="host.local_models",
                    label="Local models",
                    help_text="Catalog status.",
                    type="string",
                    provenance="default",
                    default_value="no local runtime",
                    saved_value="no local runtime",
                    effective_value="no local runtime",
                    effect="unverified",
                    tier=1,
                    read_only=True,
                ),
            ),
        ),
    )
    host = _Host(lambda: _screen(settings, requested, groups=groups))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        text = _text(host)
        host_group = view.query_one("#settings-group-hermes-host")

        assert "Hermes update" in text
        assert "Local models" in text
        assert "read-only" in text
        assert list(host_group.query(Button)) == []
        for forbidden in ("Update Hermes", "Install", "Migrate gateway"):
            assert forbidden not in text


# ── save / discard ───────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_save_requests_a_sparse_typed_write_to_the_selected_target() -> None:
    settings = _settings()
    commands = _commands()
    requested: list[Any] = []
    host = _Host(
        lambda: _screen(settings, requested, save_enabled=True, pending_count=1)
    )

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        (editor,) = _row_inputs(view)
        editor.value = "50"

        await pilot.click("#settings-save")
        await pilot.pause()

        assert len(requested) == 1
        command = requested[0]
        assert isinstance(command, _require_attr(commands, "SaveConfig"))
        assert command.target.profile_name == "alpha-fixture"
        assert command.patch == {"agent": {"max_turns": 50}}


@pytest.mark.asyncio
async def test_discard_issues_no_command_and_restores_saved_values() -> None:
    settings = _settings()
    requested: list[Any] = []
    host = _Host(
        lambda: _screen(settings, requested, save_enabled=True, pending_count=1)
    )

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        (editor,) = _row_inputs(view)
        editor.value = "50"

        await pilot.click("#settings-discard")
        await pilot.pause()

        assert requested == []
        assert editor.value == "40"


@pytest.mark.asyncio
async def test_an_invalid_row_blocks_save_and_keeps_its_message_visible() -> None:
    """Validation before I/O: the domain's message renders inline and Save
    issues nothing until the value is fixed."""
    settings = _settings()
    requested: list[Any] = []
    groups = (
        _require_attr(settings, "SettingsRowGroupView")(
            owner="hermes-profile",
            title="Hermes profile",
            rows=(
                _row(
                    settings,
                    pending_value="fast",
                    validation_message="'fast' is not a number",
                ),
            ),
        ),
    )
    host = _Host(
        lambda: _screen(
            settings, requested, groups=groups, save_enabled=True, pending_count=1
        )
    )

    async with host.run_test(size=SIZE) as pilot:
        await _mounted(pilot, host)

        await pilot.click("#settings-save")
        await pilot.pause()

        assert requested == []
        assert "'fast' is not a number" in _text(host)


@pytest.mark.asyncio
async def test_a_failed_save_shows_server_detail_and_retains_edits() -> None:
    """The installed A1-21–26 rule at the widget seam: rejection renders the
    server's words next to the field group and the pending edit survives."""
    settings = _settings()
    requested: list[Any] = []
    summary = _require_attr(settings, "project_save_summary")(
        results=(
            _require_attr(settings, "FieldSaveResult")(
                key="agent.max_turns", outcome="rejected", detail="must be a number"
            ),
        )
    )
    host = _Host(lambda: _screen(settings, requested))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        (editor,) = _row_inputs(view)
        editor.value = "50"
        view.update_view(
            _view(
                settings,
                summary=summary,
                notice="save rejected — edits retained",
                save_enabled=True,
                pending_count=1,
            )
        )
        await pilot.pause()

        text = _text(host)
        assert "must be a number" in text
        assert "saved + active" not in text
        assert _row_inputs(view)[0].value == "50"


# ── A1-3 UI: the target-switch prompt ────────────────────────────────────


def _switch_prompt(settings: Any) -> Any:
    target = _require_attr(settings, "ConfigTarget")
    return _require_attr(settings, "TargetSwitchPrompt")(
        old_target=target(
            connection_id="local-fixture", profile_name="alpha-fixture"
        ),
        new_target=target(
            connection_id="local-fixture", profile_name="beta-fixture"
        ),
        pending_count=1,
    )


@pytest.mark.asyncio
async def test_the_switch_prompt_offers_exactly_save_discard_and_stay() -> None:
    settings = _settings()
    choices: list[Any] = []
    host = _Host(
        lambda: _require_attr(_overlays_mod(), "TargetSwitchOverlay")(
            _switch_prompt(settings), on_result=choices.append
        )
    )

    async with host.run_test(size=SIZE) as pilot:
        await _mounted(pilot, host)
        text = _text(host)

        assert "Save to alpha-fixture" in text
        assert "Discard" in text
        assert "Stay" in text


@pytest.mark.asyncio
async def test_stay_closes_the_prompt_and_issues_no_command() -> None:
    settings = _settings()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested, save_enabled=True))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        (editor,) = _row_inputs(view)
        editor.value = "50"
        view.open_target_switch(
            _require_attr(settings, "ConfigTarget")(
                connection_id="local-fixture", profile_name="beta-fixture"
            )
        )
        await pilot.pause()

        await pilot.click("#switch-stay")
        await pilot.pause()

        assert requested == []
        assert "selected: alpha-fixture" in _text(host)
        assert _row_inputs(view)[0].value == "50"


@pytest.mark.asyncio
async def test_switch_save_writes_the_old_target_only() -> None:
    settings = _settings()
    commands = _commands()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested, save_enabled=True))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        (editor,) = _row_inputs(view)
        editor.value = "50"
        view.open_target_switch(
            _require_attr(settings, "ConfigTarget")(
                connection_id="local-fixture", profile_name="beta-fixture"
            )
        )
        await pilot.pause()

        await pilot.click("#switch-save")
        await pilot.pause()

        assert len(requested) == 1
        assert isinstance(requested[0], _require_attr(commands, "SaveConfig"))
        assert requested[0].target.profile_name == "alpha-fixture"
        assert requested[0].patch == {"agent": {"max_turns": 50}}


# ── Reset, restart, wake ─────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_the_reset_confirmation_names_its_target_profile() -> None:
    """The profile-explicit Reset: the overlay names the target and confirm
    requests a Reset scoped to it — never ambient."""
    settings = _settings()
    commands = _commands()
    requested: list[Any] = []
    host = _Host(
        lambda: _screen(
            settings, requested, reset_patch={"agent": {"max_turns": 25}}
        )
    )

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.open_reset()
        await pilot.pause()
        assert "alpha-fixture" in _text(host)
        assert "Reset" in _text(host)

        await pilot.click("#reset-confirm")
        await pilot.pause()

        assert len(requested) == 1
        assert isinstance(requested[0], _require_attr(commands, "ResetConfig"))
        assert requested[0].target.profile_name == "alpha-fixture"
        assert requested[0].patch == {"agent": {"max_turns": 25}}


@pytest.mark.parametrize(
    ("facts", "title"),
    [
        (
            {"target_gateway_running": True, "multiplexer_running": False,
             "multiplexed_siblings": ()},
            "Restart the gateway for profile alpha-fixture",
        ),
        (
            {"target_gateway_running": False, "multiplexer_running": True,
             "multiplexed_siblings": ("beta-fixture", "gamma-fixture")},
            "Restart the shared default gateway — this also restarts the "
            "gateways serving beta-fixture, gamma-fixture",
        ),
        (
            {"target_gateway_running": False, "multiplexer_running": False,
             "multiplexed_siblings": ()},
            "No gateway is running for alpha-fixture; start it instead",
        ),
    ],
)
@pytest.mark.asyncio
async def test_the_restart_overlay_renders_the_truthful_plan(
    facts: dict[str, Any], title: str
) -> None:
    """A1-10's UI half: exactly the computed D8 sentence, the dashboard
    note, and — for the shared case — every affected sibling."""
    settings = _settings()
    plan = _require_attr(settings, "compute_restart_plan")(
        target=_require_attr(settings, "ConfigTarget")(
            connection_id="local-fixture", profile_name="alpha-fixture"
        ),
        **facts,
    )
    confirmations: list[Any] = []
    host = _Host(
        lambda: _require_attr(_overlays_mod(), "RestartConfirmOverlay")(
            _require_attr(settings, "RestartConfirmView")(plan=plan),
            on_result=confirmations.append,
        )
    )

    async with host.run_test(size=SIZE) as pilot:
        await _mounted(pilot, host)
        text = _text(host)

        assert title in text
        assert "Talaria's own connection is to the dashboard" in text


@pytest.mark.asyncio
async def test_the_wake_toggle_requests_a_scoped_wake_command() -> None:
    settings = _settings()
    commands = _commands()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested, wake_state="on"))

    async with host.run_test(size=SIZE) as pilot:
        await _mounted(pilot, host)
        assert "Wake word" in _text(host)

        await pilot.click("#wake-toggle")
        await pilot.pause()

        assert len(requested) == 1
        assert isinstance(requested[0], _require_attr(commands, "WakeWord"))
        assert requested[0].target.profile_name == "alpha-fixture"
        assert requested[0].action == "stop"


@pytest.mark.asyncio
async def test_no_wake_toggle_is_offered_when_the_capability_is_absent() -> None:
    settings = _settings()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested, wake_state=None))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)

        assert list(view.query("#wake-toggle")) == []
        assert "Maximum agent turns" in _text(host)


# ── model picker and secret reveal ───────────────────────────────────────


@pytest.mark.asyncio
async def test_the_model_overlay_applies_a_scoped_assignment() -> None:
    """J2/A1-25's widget contract: options listed, current marked, apply
    requests a SetModel naming target, provider, and model.

    Options render as ``#model-option-<index>`` buttons in catalog order;
    the current selection carries a ``(current)`` marker."""
    settings = _settings()
    commands = _commands()
    requested: list[Any] = []
    host = _Host(
        lambda: _screen(settings, requested, model_picker=_model_picker(settings))
    )

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.open_model_picker()
        await pilot.pause()
        text = _text(host)
        assert "example-small" in text
        assert "example-large" in text
        assert "(current)" in text

        await pilot.click("#model-option-1")
        await pilot.pause()
        await pilot.click("#model-apply")
        await pilot.pause()

        applied = [c for c in requested
                   if isinstance(c, _require_attr(commands, "SetModel"))]
        assert len(applied) == 1
        assert applied[0].target.profile_name == "alpha-fixture"
        assert applied[0].provider == "example-provider"
        assert applied[0].model == "example-large"


@pytest.mark.asyncio
async def test_the_reveal_overlay_shows_masked_and_requests_one_shot() -> None:
    """J3's widget contract: the masked value and key show; the reveal is
    an explicit one-shot request scoped to the target — and the overlay
    never renders a value the test did not put in the masked field."""
    settings = _settings()
    commands = _commands()
    requested: list[Any] = []
    host = _Host(
        lambda: _screen(
            settings,
            requested,
            secrets={"EXAMPLE_API_KEY": (True, "sk-…abcd")},
        )
    )

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.open_reveal("EXAMPLE_API_KEY")
        await pilot.pause()
        assert "EXAMPLE_API_KEY" in _text(host)
        assert "sk-…abcd" in _text(host)

        await pilot.click("#reveal-once")
        await pilot.pause()

        assert len(requested) == 1
        assert isinstance(requested[0], _require_attr(commands, "RevealEnv"))
        assert requested[0].target.profile_name == "alpha-fixture"
        assert requested[0].key == "EXAMPLE_API_KEY"


# ── search, layout, keyboard, focus ──────────────────────────────────────


@pytest.mark.asyncio
async def test_search_filters_rows_by_label_help_and_key() -> None:
    settings = _settings()
    requested: list[Any] = []
    groups = (
        _require_attr(settings, "SettingsRowGroupView")(
            owner="hermes-profile",
            title="Hermes profile",
            rows=(
                _row(settings),
                _row(
                    settings,
                    key="approvals.mode",
                    label="Approval mode",
                    help_text="When the agent must ask first.",
                    type="select",
                    provenance="default",
                    default_value="smart",
                    saved_value="smart",
                    effective_value="smart",
                    effect="live",
                    tier=1,
                ),
            ),
        ),
    )
    host = _Host(lambda: _screen(settings, requested, groups=groups))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.query_one("#settings-search", Input).value = "approv"
        await pilot.pause()
        text = _text(host)

        assert "Approval mode" in text
        assert "Maximum agent turns" not in text


@pytest.mark.asyncio
async def test_slash_focuses_search() -> None:
    """Type-to-search mirrors the Desktop: ``/`` lands in the search box."""
    settings = _settings()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        # Park focus outside any Input first: "/" typed into a row editor
        # is text, not the search binding.
        view.query_one("#settings-save", Button).focus()
        await pilot.pause()

        await pilot.press("/")
        await pilot.pause()

        assert view.query_one("#settings-search", Input).has_focus


@pytest.mark.asyncio
async def test_the_target_header_stays_visible_at_80x24() -> None:
    """The small-terminal contract: navigation may collapse, but the
    selected target must never scroll out of the operator's sight."""
    settings = _settings()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested))

    async with host.run_test(size=SMALL) as pilot:
        await _mounted(pilot, host)
        text = _text(host)

        assert "selected: alpha-fixture" in text
        assert "current: beta-fixture" in text


@pytest.mark.asyncio
async def test_escape_cancels_an_overlay_without_a_command() -> None:
    settings = _settings()
    requested: list[Any] = []
    host = _Host(
        lambda: _screen(
            settings, requested, reset_patch={"agent": {"max_turns": 25}}
        )
    )

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.query_one("#settings-save", Button).focus()
        await pilot.pause()
        view.open_reset()
        await pilot.pause()
        assert list(host.screen.query("#reset-confirm")) != []

        await pilot.press("escape")
        await pilot.pause()

        assert requested == []
        assert host.screen is view
        assert list(host.screen.query("#reset-confirm")) == []
        assert view.query_one("#settings-save", Button).has_focus


# ── P2-1: pending switch and the live target control (dev-4) ───────────────


@pytest.mark.asyncio
async def test_switch_save_keeps_the_header_on_the_old_target() -> None:
    """P2-1 KTD1 at the screen seam: choosing Save emits the old-target
    write but the header stays on the old profile until the re-read
    commits the pending switch — no premature selection flip."""
    settings = _settings()
    commands = _commands()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested, save_enabled=True))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        (editor,) = _row_inputs(view)
        editor.value = "50"
        view.open_target_switch(
            _require_attr(settings, "ConfigTarget")(
                connection_id="local-fixture", profile_name="beta-fixture"
            )
        )
        await pilot.pause()

        await pilot.click("#switch-save")
        await pilot.pause()

        assert len(requested) == 1
        assert isinstance(requested[0], _require_attr(commands, "SaveConfig"))
        assert "selected: alpha-fixture" in _text(host)
        assert "selected: beta-fixture" not in _text(host)


@pytest.mark.asyncio
async def test_the_target_control_lists_options_and_routes_choice() -> None:
    """The live target picker: a header control lists the projected
    options as ``"<connection> / <profile>"`` buttons, and choosing one
    opens the switch prompt — the production route the P2-1 acceptance
    file drives end to end."""
    settings = _settings()
    option_type = _require_attr(settings, "TargetOption")
    options = (
        option_type(
            connection_id="local-fixture",
            profile_name="alpha-fixture",
            is_current=False,
            is_selected=True,
        ),
        option_type(
            connection_id="local-fixture",
            profile_name="beta-fixture",
            is_current=True,
            is_selected=False,
        ),
    )
    requested: list[Any] = []
    try:
        screen_view = _view(settings, target_options=options)
    except TypeError:
        pytest.fail(
            "SettingsWorkspaceView carries no target_options (P2-1 residual)"
        )
    host = _Host(
        lambda: _require_attr(_workspace_mod(), "SettingsWorkspaceScreen")(
            screen_view, on_command=requested.append
        )
    )

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        try:
            view.query_one("#settings-target", Button)
        except NoMatches:
            pytest.fail(
                "settings workspace exposes no #settings-target control (P2-1)"
            )

        await pilot.click("#settings-target")
        await pilot.pause()
        labels = {
            str(button.label).strip() for button in view.query(Button)
        }
        assert "local-fixture / alpha-fixture" in labels
        assert "local-fixture / beta-fixture" in labels

        for button in view.query(Button):
            if str(button.label).strip() == "local-fixture / beta-fixture":
                button.press()
                break
        await pilot.pause()

        overlay_type = _require_attr(_overlays_mod(), "TargetSwitchOverlay")
        assert isinstance(host.screen, overlay_type), (
            "choosing a picker option must open the switch prompt (P2-1)"
        )
        assert "Save to alpha-fixture" in _text(host)


# ── P2-2: secret reveal button and display bound (dev-4) ───────────────────


@pytest.mark.asyncio
async def test_a_secret_row_reveal_button_emits_a_scoped_reveal() -> None:
    """A row whose key the view marks secret renders a Reveal control;
    confirming it emits one profile-explicit RevealEnv — the production
    route the P2-2 acceptance file drives end to end."""
    settings = _settings()
    commands = _commands()
    requested: list[Any] = []
    groups = (
        _require_attr(settings, "SettingsRowGroupView")(
            owner="hermes-profile",
            title="Environment",
            rows=(
                _row(
                    settings,
                    key="EXAMPLE_P2_KEY",
                    label="EXAMPLE_P2_KEY",
                    help_text="P2 test key.",
                    type="string",
                    provenance="saved",
                    default_value="",
                    saved_value="sk-…p2ab",
                    effective_value="sk-…p2ab",
                    effect="next-session",
                    tier=1,
                    read_only=True,
                ),
            ),
        ),
    )
    host = _Host(
        lambda: _screen(
            settings,
            requested,
            groups=groups,
            secrets={"EXAMPLE_P2_KEY": (True, "sk-…p2ab")},
        )
    )

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        revealers = [
            button
            for button in view.query(Button)
            if str(button.label).strip().lower() == "reveal"
        ]
        assert len(revealers) == 1, (
            "secret row renders no Reveal control (P2-2 residual)"
        )

        revealers[0].press()
        await pilot.pause()
        overlay_type = _require_attr(_overlays_mod(), "RevealSecretOverlay")
        assert isinstance(host.screen, overlay_type)

        await pilot.click("#reveal-once")
        await pilot.pause()

        assert len(requested) == 1
        assert isinstance(requested[0], _require_attr(commands, "RevealEnv"))
        assert requested[0].target.profile_name == "alpha-fixture"
        assert requested[0].key == "EXAMPLE_P2_KEY"


def test_the_reveal_display_interval_is_bounded() -> None:
    """KTD6's timeout half, statically: the overlay wipes after a bounded
    interval. Close-wipe is proven behaviorally; firing is installed-J3
    territory. The bound keeps the constant honest in between."""
    bound = getattr(_overlays_mod(), "REVEAL_DISPLAY_SECONDS", None)
    assert bound is not None, (
        "settings overlays define no REVEAL_DISPLAY_SECONDS (P2-2)"
    )
    assert isinstance(bound, (int, float)) and 1 <= bound <= 60


# ── P3-1: loaded-view group reconciliation (dev-4, KTD1/KTD2) ──────────────


def _row_present(view: Any, key: str) -> bool:
    try:
        view.query_one(f"#{row_widget_id(key)}")
    except NoMatches:
        return False
    return True


def _loaded_view(settings: Any, **overrides: Any) -> Any:
    """A second-target view: different schema row plus a secret row."""
    groups = (
        _require_attr(settings, "SettingsRowGroupView")(
            owner="hermes-profile",
            title="Hermes profile",
            rows=(
                _row(
                    settings,
                    key="agent.api_max_retries",
                    label="Maximum API retries",
                    help_text="Cap on API retries.",
                    saved_value=3,
                    effective_value=3,
                ),
            ),
        ),
        _require_attr(settings, "SettingsRowGroupView")(
            owner="hermes-profile",
            title="Environment",
            rows=(
                _row(
                    settings,
                    key="EXAMPLE_P3_KEY",
                    label="EXAMPLE_P3_KEY",
                    help_text="P3 test key.",
                    type="string",
                    provenance="saved",
                    default_value="",
                    saved_value="sk-…p3ab",
                    effective_value="sk-…p3ab",
                    effect="next-session",
                    tier=1,
                    read_only=True,
                ),
            ),
        ),
    )
    fields: dict[str, Any] = {
        "groups": groups,
        "secrets": {"EXAMPLE_P3_KEY": (True, "sk-…p3ab")},
    }
    fields.update(overrides)
    return _view(settings, **fields)


@pytest.mark.asyncio
async def test_apply_loaded_view_mounts_new_rows_and_secret_controls() -> None:
    """KTD1: the loaded view is authoritative — rows and groups absent from
    the old screen mount with editors and Reveal controls intact."""
    settings = _settings()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.apply_loaded_view(_loaded_view(settings), "local-fixture")
        for _ in range(5):
            await pilot.pause()

        try:
            new_row = view.query_one(f"#{row_widget_id('agent.api_max_retries')}")
        except NoMatches:
            pytest.fail(
                "apply_loaded_view mounted no new schema row (P3-1 residual: "
                "in-place updates cannot create widgets)"
            )
        assert new_row.query_one(Input).value == "3"
        try:
            secret_row = view.query_one(f"#{row_widget_id('EXAMPLE_P3_KEY')}")
        except NoMatches:
            pytest.fail(
                "apply_loaded_view mounted no secret row (P3-2 residual)"
            )
        revealers = [
            button
            for button in secret_row.query(Button)
            if str(button.label).strip().lower() == "reveal"
        ]
        assert len(revealers) == 1


@pytest.mark.asyncio
async def test_apply_loaded_view_removes_rows_and_groups_it_replaces() -> None:
    """R2: no previous target's schema or mask survives — old-only rows
    and groups disappear instead of lingering beside the new ones."""
    settings = _settings()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        assert _row_present(view, "agent.max_turns")
        view.apply_loaded_view(_loaded_view(settings), "local-fixture")
        for _ in range(5):
            await pilot.pause()

        assert not _row_present(view, "agent.max_turns"), (
            "stale old-target row survives the loaded view (P3-1)"
        )


@pytest.mark.asyncio
async def test_apply_loaded_view_reapplies_search_and_keeps_focus_attached() -> None:
    """KTD5: the search text survives remounting and filters the new rows;
    focus never points at a removed widget."""
    settings = _settings()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.query_one("#settings-search", Input).value = "retries"
        await pilot.pause()
        view.apply_loaded_view(_loaded_view(settings), "local-fixture")
        for _ in range(5):
            await pilot.pause()

        assert view.query_one("#settings-search", Input).value == "retries"
        assert _row_present(view, "agent.api_max_retries"), (
            "remounted row missing, so search has nothing to apply to (P3-1)"
        )
        row = view.query_one(f"#{row_widget_id('agent.api_max_retries')}")
        assert row.display
        focused = view.focused
        assert focused is None or focused.is_mounted


# ── P4-1: the load-error notice surface (dev-4 preserves) ──────────────────


@pytest.mark.asyncio
async def test_apply_loaded_view_renders_the_load_error_notice() -> None:
    """U1 routes typed load failures through an actionable placeholder
    notice. Whatever the loader reports, the apply path must render the
    notice text — this pins the surface while the loader work lands."""
    settings = _settings()
    requested: list[Any] = []
    host = _Host(lambda: _screen(settings, requested))

    async with host.run_test(size=SIZE) as pilot:
        view = await _mounted(pilot, host)
        view.apply_loaded_view(
            _view(settings, notice="schema unavailable (unreachable)"),
            "local-fixture",
        )
        for _ in range(3):
            await pilot.pause()

        assert "schema unavailable (unreachable)" in _text(host)
