"""A1-4, A1-5, A1-8, A1-10 — honest projection of target, ownership, value, restart.

Plus the effect-timing vocabulary the installed A1-21–26 scenarios and the
save summary speak: every "saved/active/awaiting" sentence is pinned here so
the UI cannot invent a sixth timing class or soften "no API performs it".

Interface under test (unimplemented until CFG B2, ``dev-1-3``):

* ``talaria.domain.settings.HermesConnection`` and
  ``project_target_header`` — the connection · current · selected header.
* ``talaria.domain.settings.SettingOwner`` and
  ``partition_settings_keys`` — ownership grouping with no key-prefix
  re-resolution.
* ``talaria.domain.settings.project_field_row`` — provenance chips and the
  full default/saved/effective/pending value set per row.
* ``talaria.domain.settings.compute_restart_plan`` — facts to exactly one
  D8 sentence. Resolving those facts from ``/api/status`` + ``/api/profiles``
  is transport work (Test Author Two, A1-19); mapping facts to wording is
  the pure function pinned here.
* ``talaria.domain.settings.effect_summary_label`` and
  ``project_save_summary`` — the per-field outcome vocabulary.

No network, no filesystem, no Textual (ADR-0002).
"""

from __future__ import annotations

import importlib
import inspect
import secrets
from typing import Any

import pytest


def _require_module(name: str) -> Any:
    try:
        return importlib.import_module(name)
    except ImportError as exc:
        pytest.fail(f"unimplemented interface {name} (CFG v0.6.2 B2): {exc}")


def _require_attr(module: Any, name: str) -> Any:
    try:
        return getattr(module, name)
    except AttributeError:
        pytest.fail(
            f"unimplemented interface {module.__name__}.{name} (CFG v0.6.2 B2)"
        )


def _settings() -> Any:
    return _require_module("talaria.domain.settings")


def _target(settings: Any, connection: str, profile: str) -> Any:
    return _require_attr(settings, "ConfigTarget")(
        connection_id=connection, profile_name=profile
    )


def _connection(settings: Any, **overrides: Any) -> Any:
    fields: dict[str, Any] = {
        "connection_id": "local-fixture",
        "label": "local-fixture",
        "base_url": "http://127.0.0.1:8765",
        "auth_mode": "loopback",
        "current_profile": "beta-fixture",
        "version": "0.21.3",
    }
    fields.update(overrides)
    return _require_attr(settings, "HermesConnection")(**fields)


# ── A1-4: current and selected stay distinct, and both show ───────────────


def test_a_connection_whose_current_differs_from_selected_shows_both() -> None:
    settings = _settings()

    header = _require_attr(settings, "project_target_header")(
        connection=_connection(settings, current_profile="beta-fixture"),
        selected=_target(settings, "local-fixture", "alpha-fixture"),
    )

    assert header.connection_label == "local-fixture"
    assert header.current_profile == "beta-fixture"
    assert header.selected_profile == "alpha-fixture"
    assert header.shows_both_names is True
    assert header.auth_mode == "loopback"
    assert header.hermes_version == "0.21.3"


def test_equal_current_and_selected_still_send_an_explicit_profile() -> None:
    """Same name on both sides changes the header, not the request: D1
    forbids relying on the server's launch profile even when it would
    resolve identically."""
    settings = _settings()
    commands = _require_module("talaria.domain.settings_commands")

    header = _require_attr(settings, "project_target_header")(
        connection=_connection(settings, current_profile="alpha-fixture"),
        selected=_target(settings, "local-fixture", "alpha-fixture"),
    )
    spec = _require_attr(commands, "rest_request")(
        _require_attr(commands, "SaveConfig")(
            target=_target(settings, "local-fixture", header.selected_profile),
            patch={"agent": {"max_turns": 40}},
        )
    )

    assert header.shows_both_names is False
    assert header.selected_profile == "alpha-fixture"
    assert spec.query["profile"] == "alpha-fixture"


# ── A1-5: ownership is a partition, never a guess ─────────────────────────


def test_no_talaria_setting_appears_under_hermes_and_vice_versa() -> None:
    """The adversarial pairs: ``theme.name`` (Talaria) vs ``display.theme``
    (Hermes's own TUI), ``status.command`` vs ``terminal.cwd``. Ownership
    comes from the caller's tagging, so a prefix rule cannot misfile them."""
    settings = _settings()
    partition = _require_attr(settings, "partition_settings_keys")

    groups = partition(
        talaria_user=("theme.name", "status.command"),
        talaria_repository=(),
        talaria_session=("status.segments",),
        hermes_host=("local_runtime.status",),
        hermes_profile=("display.theme", "display.resume_last_session", "terminal.cwd"),
    )
    placed = {
        key: group.owner for group in groups for key in group.keys
    }

    assert placed["theme.name"] == "talaria-user"
    assert placed["status.segments"] == "talaria-session"
    assert placed["display.theme"] == "hermes-profile"
    assert placed["display.resume_last_session"] == "hermes-profile"
    assert placed["local_runtime.status"] == "hermes-host"
    assert [group.owner for group in groups] == [
        "talaria-user",
        "talaria-repository",
        "talaria-session",
        "hermes-host",
        "hermes-profile",
    ]


def test_every_key_is_placed_exactly_once() -> None:
    settings = _settings()
    partition = _require_attr(settings, "partition_settings_keys")

    groups = partition(
        talaria_user=("theme.name",),
        talaria_repository=("status.command",),
        talaria_session=(),
        hermes_host=(),
        hermes_profile=("agent.max_turns",),
    )
    placed = [key for group in groups for key in group.keys]

    assert sorted(placed) == ["agent.max_turns", "status.command", "theme.name"]


def test_the_owner_vocabulary_is_exactly_the_five_documented_owners() -> None:
    settings = _settings()

    assert set(_require_attr(settings, "SettingOwner")) == {
        "talaria-user",
        "talaria-repository",
        "talaria-session",
        "hermes-host",
        "hermes-profile",
    }


# ── A1-8: provenance shows the right chip and every material value ─────────


def test_disagreeing_layers_show_the_chip_and_all_three_values() -> None:
    settings = _settings()
    schema = _require_attr(settings, "decode_settings_schema")(
        {
            "fields": {
                "agent.max_turns": {
                    "type": "number",
                    "description": "Maximum agent turns.",
                    "category": "agent",
                }
            },
            "category_order": ["agent"],
        }
    )
    field = schema.fields[0]

    row = _require_attr(settings, "project_field_row")(
        field=field,
        default_value=25,
        saved_value=40,
        effective_value=40,
        pending_value=None,
        ownership="hermes-profile",
        effect="next-session",
        tier=1,
    )

    assert row.provenance == "saved"
    assert row.default_value == 25
    assert row.saved_value == 40
    assert row.effective_value == 40
    assert row.pending_value is None
    assert row.read_only is False


def test_a_pending_edit_outranks_saved_provenance() -> None:
    settings = _settings()
    schema = _require_attr(settings, "decode_settings_schema")(
        {
            "fields": {
                "agent.max_turns": {
                    "type": "number",
                    "description": "Maximum agent turns.",
                    "category": "agent",
                }
            },
            "category_order": ["agent"],
        }
    )

    row = _require_attr(settings, "project_field_row")(
        field=schema.fields[0],
        default_value=25,
        saved_value=40,
        effective_value=40,
        pending_value=50,
        ownership="hermes-profile",
        effect="next-session",
        tier=1,
    )

    assert row.provenance == "changed"
    assert row.pending_value == 50


def test_an_at_default_row_says_default() -> None:
    settings = _settings()
    schema = _require_attr(settings, "decode_settings_schema")(
        {
            "fields": {
                "agent.max_turns": {
                    "type": "number",
                    "description": "Maximum agent turns.",
                    "category": "agent",
                }
            },
            "category_order": ["agent"],
        }
    )

    row = _require_attr(settings, "project_field_row")(
        field=schema.fields[0],
        default_value=25,
        saved_value=25,
        effective_value=25,
        pending_value=None,
        ownership="hermes-profile",
        effect="next-session",
        tier=1,
    )

    assert row.provenance == "default"


def test_an_unknown_typed_row_is_read_only_with_its_type_named() -> None:
    """The domain fact behind A1-7's read-only rendering: unknown types
    project ``read_only`` with an explanation, not an editor."""
    settings = _settings()
    schema = _require_attr(settings, "decode_settings_schema")(
        {
            "fields": {
                "example.future": {
                    "type": "color",
                    "description": "Invented next release.",
                    "category": "general",
                }
            },
            "category_order": ["general"],
        }
    )

    row = _require_attr(settings, "project_field_row")(
        field=schema.fields[0],
        default_value="red",
        saved_value="red",
        effective_value="red",
        pending_value=None,
        ownership="hermes-profile",
        effect="unverified",
        tier=2,
    )

    assert row.read_only is True
    assert row.validation_message is not None and "color" in row.validation_message


def test_an_invalid_pending_value_carries_the_validator_message() -> None:
    """A1-7's "validated": the projector runs the schema validator over the
    pending value, so the workspace can render the message inline and block
    Save before any I/O without owning validation rules itself."""
    settings = _settings()
    schema = _require_attr(settings, "decode_settings_schema")(
        {
            "fields": {
                "agent.max_turns": {
                    "type": "number",
                    "description": "Maximum agent turns.",
                    "category": "agent",
                }
            },
            "category_order": ["agent"],
        }
    )

    row = _require_attr(settings, "project_field_row")(
        field=schema.fields[0],
        default_value=25,
        saved_value=40,
        effective_value=40,
        pending_value="fast",
        ownership="hermes-profile",
        effect="next-session",
        tier=1,
    )

    assert row.provenance == "changed"
    assert row.validation_message is not None and "fast" in row.validation_message


# ── A1-10: restart wording is computed, not templated ─────────────────────


def test_an_own_gateway_projects_the_profile_only_sentence() -> None:
    settings = _settings()

    plan = _require_attr(settings, "compute_restart_plan")(
        target=_target(settings, "local-fixture", "alpha-fixture"),
        target_gateway_running=True,
        multiplexer_running=False,
        multiplexed_siblings=(),
    )

    assert plan.scope == "own-gateway"
    assert plan.title == "Restart the gateway for profile alpha-fixture"
    assert plan.dashboard_note == (
        "Talaria's own connection is to the dashboard and is unaffected."
    )
    assert plan.affected_profiles == ("alpha-fixture",)


def test_a_multiplexed_profile_names_the_shared_gateway_and_both_siblings() -> None:
    """D8's warning: restarting serves every multiplexed profile, and the UI
    must say so before confirmation — never label it profile-only."""
    settings = _settings()

    plan = _require_attr(settings, "compute_restart_plan")(
        target=_target(settings, "local-fixture", "alpha-fixture"),
        target_gateway_running=False,
        multiplexer_running=True,
        multiplexed_siblings=("beta-fixture", "gamma-fixture"),
    )

    assert plan.scope == "shared-multiplexer"
    assert plan.title == (
        "Restart the shared default gateway — this also restarts the "
        "gateways serving beta-fixture, gamma-fixture"
    )
    assert plan.dashboard_note == (
        "Talaria's own connection is to the dashboard and is unaffected."
    )
    assert plan.affected_profiles == (
        "alpha-fixture",
        "beta-fixture",
        "gamma-fixture",
    )


def test_no_gateway_projects_start_instead_without_a_restart_claim() -> None:
    """The 409 path: offering Restart here would fail server-side, so the
    projection offers Start and claims nothing about a restart."""
    settings = _settings()

    plan = _require_attr(settings, "compute_restart_plan")(
        target=_target(settings, "local-fixture", "alpha-fixture"),
        target_gateway_running=False,
        multiplexer_running=False,
        multiplexed_siblings=(),
    )

    assert plan.scope == "none"
    assert plan.title == "No gateway is running for alpha-fixture; start it instead"
    assert plan.affected_profiles == ()


def test_the_restart_plan_is_frozen() -> None:
    from dataclasses import FrozenInstanceError

    settings = _settings()

    plan = _require_attr(settings, "compute_restart_plan")(
        target=_target(settings, "local-fixture", "alpha-fixture"),
        target_gateway_running=True,
        multiplexer_running=False,
        multiplexed_siblings=(),
    )

    with pytest.raises(FrozenInstanceError):
        plan.title = "mutated"


# ── the effect-timing vocabulary A1-21–26 and the summary speak ───────────


@pytest.mark.parametrize(
    ("effect", "label"),
    [
        ("live", "saved + active"),
        ("next-session", "takes effect for new sessions"),
        ("gateway-restart", "awaiting gateway restart"),
        (
            "dashboard-restart-no-api",
            "requires a dashboard restart; no API performs it",
        ),
        (
            "unverified",
            "applies on next session or gateway restart — unverified",
        ),
    ],
)
def test_every_timing_class_has_its_exact_summary_sentence(
    effect: str, label: str
) -> None:
    """Five classes, five sentences (CFG-A1 D4, §13.3). "Active" appears only
    for the live class; the dashboard class names the missing API rather
    than implying a restart action exists."""
    settings = _settings()

    assert _require_attr(settings, "effect_summary_label")(effect) == label


def test_an_unknown_timing_class_is_refused() -> None:
    """No sixth class by typo: an unlisted effect fails loudly instead of
    rendering an empty-success sentence."""
    settings = _settings()

    with pytest.raises(ValueError):
        _require_attr(settings, "effect_summary_label")("eventually")


def test_the_save_summary_groups_fields_by_timing_and_keeps_server_detail() -> None:
    settings = _settings()
    project = _require_attr(settings, "project_save_summary")
    result_type = _require_attr(settings, "FieldSaveResult")

    summary = project(
        results=(
            result_type(key="approvals.mode", outcome="active"),
            result_type(key="agent.max_turns", outcome="saved"),
            result_type(
                key="gateway.port", outcome="rejected", detail="port in use"
            ),
        )
    )
    groups = {group.effect: group for group in summary.groups}

    assert groups["active"].keys == ("approvals.mode",)
    assert groups["saved"].keys == ("agent.max_turns",)
    assert groups["rejected"].keys == ("gateway.port",)
    assert groups["rejected"].details == ("port in use",)


def test_a_failed_save_never_paints_success() -> None:
    """A rejected field appears only under rejected: no "saved" entry, no
    timing sentence that predicts an effect that will not happen."""
    settings = _settings()
    project = _require_attr(settings, "project_save_summary")
    result_type = _require_attr(settings, "FieldSaveResult")

    summary = project(
        results=(result_type(key="agent.max_turns", outcome="rejected",
                             detail="must be a number"),)
    )

    assert [group.effect for group in summary.groups] == ["rejected"]
    assert summary.groups[0].keys == ("agent.max_turns",)


# ── P2-1: the live target picker projection (dev-4) ────────────────────────


def test_target_options_flag_current_and_selected_without_reordering() -> None:
    """The picker lists API-visible (connection, profile) pairs with stable
    order and exact current/selected markers — same-named profiles on two
    connections stay distinct entries (A1-2's pair rule, in the picker)."""
    settings = _settings()
    project = _require_attr(settings, "project_target_options")
    local_a = _target(settings, "local-fixture", "alpha-fixture")
    remote_a = _target(settings, "remote-fixture", "alpha-fixture")
    local_b = _target(settings, "local-fixture", "beta-fixture")

    options = project(
        [local_a, remote_a, local_b], selected=local_a, current=local_b
    )

    assert [(o.connection_id, o.profile_name) for o in options] == [
        ("local-fixture", "alpha-fixture"),
        ("remote-fixture", "alpha-fixture"),
        ("local-fixture", "beta-fixture"),
    ]
    assert [(o.is_selected, o.is_current) for o in options] == [
        (True, False),
        (False, False),
        (False, True),
    ]


def test_an_empty_target_list_projects_no_options() -> None:
    settings = _settings()
    project = _require_attr(settings, "project_target_options")
    target = _target(settings, "local-fixture", "alpha-fixture")

    assert project([], selected=target, current=target) == ()


def test_the_workspace_view_carries_the_projected_options() -> None:
    """The screen renders the picker from the view, not from a second
    fetch: the projector takes the visible pairs and attaches options."""
    settings = _settings()
    projection = _require_module("talaria.domain.projection")
    project_workspace = _require_attr(projection, "project_settings_workspace")
    if "targets" not in inspect.signature(project_workspace).parameters:
        pytest.fail(
            "unimplemented interface project_settings_workspace(targets=...) (P2-1)"
        )
    local_a = _target(settings, "local-fixture", "alpha-fixture")
    local_b = _target(settings, "local-fixture", "beta-fixture")

    identity = project_workspace(
        connection_id="local-fixture",
        connection_label="local-fixture",
        current_profile="beta-fixture",
        selected_profile="alpha-fixture",
        targets=[local_a, local_b],
    )

    options = identity.view.target_options
    assert [(o.profile_name, o.is_selected) for o in options] == [
        ("alpha-fixture", True),
        ("beta-fixture", False),
    ]


# ── P2-2: env decode and secret rows (dev-4) ───────────────────────────────


def test_decode_env_listing_keeps_masks_and_presence_only() -> None:
    """Hermes ``GET /api/env`` rows (``{is_set, redacted_value}`` at
    3236f440) decode to presence + masked pairs. Raw values never appear
    in this listing, so the decoder takes no value parameter at all."""
    settings = _settings()
    decode = _require_attr(settings, "decode_env_listing")

    listing = decode(
        {
            "EXAMPLE_ONE": {"is_set": True, "redacted_value": "sk-…aa11"},
            "EXAMPLE_TWO": {"is_set": False, "redacted_value": None},
        }
    )

    assert listing == {
        "EXAMPLE_ONE": (True, "sk-…aa11"),
        "EXAMPLE_TWO": (False, ""),
    }


def test_decode_env_listing_tolerates_shape_drift() -> None:
    """Unknown keys ride along untouched; mistyped presence/masks read as
    unset rather than refusing the whole listing."""
    settings = _settings()
    decode = _require_attr(settings, "decode_env_listing")

    listing = decode(
        {
            "EXAMPLE_ONE": {
                "is_set": "yes",
                "redacted_value": 7,
                "invented_next_release": True,
            },
            "EXAMPLE_TWO": "not-an-object",
        }
    )

    assert listing == {"EXAMPLE_ONE": (False, ""), "EXAMPLE_TWO": (False, "")}


def test_decode_env_listing_refuses_a_body_that_is_not_a_json_object() -> None:
    settings = _settings()
    decode = _require_attr(settings, "decode_env_listing")
    decode_error = _require_attr(settings, "SettingsDecodeError")

    with pytest.raises(decode_error):
        decode(["EXAMPLE_ONE"])


def test_project_secret_rows_maps_presence_to_rows_in_sorted_order() -> None:
    """One SecretRow per key, sorted for a deterministic listing; set rows
    carry the server mask, unset rows carry nothing."""
    settings = _settings()
    project = _require_attr(settings, "project_secret_rows")

    rows = project({"B_KEY": (True, "sk-…b"), "A_KEY": (False, "")})

    assert [row.key for row in rows] == ["A_KEY", "B_KEY"]
    assert rows[0].provenance == "default"
    assert rows[0].masked == ""
    assert rows[1].provenance == "saved"
    assert rows[1].masked == "sk-…b"


# ── P2-2: the presentation-ephemeral reveal value (dev-4, KTD6) ────────────


def test_a_reveal_value_is_takeable_exactly_once() -> None:
    """The overlay takes the plaintext a single time; every later take
    yields nothing. Single-take is what makes the display one-shot by
    construction rather than by discipline."""
    settings = _settings()
    holder_type = _require_attr(settings, "RevealDisplayValue")
    canary = f"p2-canary-{secrets.token_hex(4)}"

    holder = holder_type(key="EXAMPLE_P2_KEY", value=canary)

    assert holder.is_cleared is False
    assert holder.take() == canary
    assert holder.is_cleared is True
    assert holder.take() is None


def test_a_reveal_value_never_appears_in_its_repr() -> None:
    settings = _settings()
    holder_type = _require_attr(settings, "RevealDisplayValue")
    canary = f"p2-canary-{secrets.token_hex(4)}"

    holder = holder_type(key="EXAMPLE_P2_KEY", value=canary)

    assert canary not in repr(holder)
    assert "EXAMPLE_P2_KEY" in repr(holder)


def test_clear_wipes_without_returning_the_value() -> None:
    """Close/timeout wipe through the same path whether or not the value
    was ever displayed."""
    settings = _settings()
    holder_type = _require_attr(settings, "RevealDisplayValue")
    canary = f"p2-canary-{secrets.token_hex(4)}"

    holder = holder_type(key="EXAMPLE_P2_KEY", value=canary)
    holder.clear()

    assert holder.is_cleared is True
    assert holder.take() is None
