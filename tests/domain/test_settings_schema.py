"""A1-7 — schema rendering, Tier-1 curation, and the Reset/wake contract.

The live ``GET /api/config/schema`` carries only type, description, category
and (for selects) options — no secret, restart, or read-only flags (CFG-A1
§4.2). Tier 1 is therefore a curated manifest over that schema (implementation
plan KTD4), and anything the manifest does not name falls through to the
generic Tier-2 renderer by server category.

Interface under test (unimplemented until CFG B2, ``dev-1-3``):

* ``talaria.domain.settings.decode_settings_schema`` — decodes the
  ``{fields, category_order}`` body into ``SettingsSchema``; refuses
  malformed bodies with ``SettingsDecodeError`` but tolerates unknown field
  *types* (they render read-only, never fatal).
* ``talaria.domain.settings.KNOWN_SCHEMA_TYPES`` — the five renderable types.
* ``talaria.domain.settings.validate_settings_value`` — per-type validation
  returning ``None`` when valid, else a human-readable message.
* ``talaria.domain.settings_catalog`` — ``TIER1`` manifest entries (each with
  owner, route/mechanism, scope, control, effect, disposition, rationale),
  ``tier_of``, and ``surface_disposition`` for explicit exclusions.
* ``talaria.domain.settings.build_reset_patch`` — the sparse
  defaults-restoring patch behind the profile-explicit Reset command.

No network, no filesystem, no Textual (ADR-0002).
"""

from __future__ import annotations

import importlib
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


def _catalog() -> Any:
    return _require_module("talaria.domain.settings_catalog")


def _schema_body() -> dict[str, Any]:
    """The wire shape at Hermes 3236f440 (``config_env.py:96-103``)."""
    return {
        "fields": {
            "timezone": {
                "type": "string",
                "description": "IANA timezone for timestamps.",
                "category": "general",
            },
            "agent.max_turns": {
                "type": "number",
                "description": "Maximum agent turns.",
                "category": "agent",
            },
            "display.show_reasoning": {
                "type": "boolean",
                "description": "Show reasoning blocks.",
                "category": "display",
            },
            "fallback_providers": {
                "type": "list",
                "description": "Fallback providers in order.",
                "category": "general",
            },
            "approvals.mode": {
                "type": "select",
                "description": "Approval mode.",
                "category": "safety",
                "options": ["manual", "smart", "off"],
                "searchable": True,
                "clearable": False,
            },
        },
        "category_order": ["general", "agent", "display", "safety"],
    }


# ── decode: the five known types and their metadata ──────────────────────


def test_all_five_known_types_decode_with_their_metadata() -> None:
    settings = _settings()

    schema = _require_attr(settings, "decode_settings_schema")(_schema_body())
    by_key = {field.key: field for field in schema.fields}

    assert by_key["timezone"].type == "string"
    assert by_key["agent.max_turns"].type == "number"
    assert by_key["display.show_reasoning"].type == "boolean"
    assert by_key["fallback_providers"].type == "list"
    assert by_key["approvals.mode"].type == "select"
    assert by_key["approvals.mode"].options == ("manual", "smart", "off")
    assert by_key["approvals.mode"].searchable is True
    assert by_key["approvals.mode"].clearable is False
    assert by_key["timezone"].description == "IANA timezone for timestamps."
    assert by_key["timezone"].category == "general"


def test_category_order_and_field_order_follow_the_server_verbatim() -> None:
    """The workspace groups by server category in ``category_order``; the
    decoder must not re-sort either list (the Hermes display order is a
    product decision, not alphabetical)."""
    settings = _settings()

    schema = _require_attr(settings, "decode_settings_schema")(_schema_body())

    assert schema.category_order == ("general", "agent", "display", "safety")
    assert [field.key for field in schema.fields] == [
        "timezone",
        "agent.max_turns",
        "display.show_reasoning",
        "fallback_providers",
        "approvals.mode",
    ]


def test_an_unknown_type_decodes_and_is_not_a_known_type() -> None:
    """New server types render read-only with an explanation (A1-7); they
    must not be fatal, or one new field breaks the whole workspace."""
    settings = _settings()
    body = _schema_body()
    body["fields"]["example.future"] = {
        "type": "color",
        "description": "Invented next release.",
        "category": "general",
    }

    schema = _require_attr(settings, "decode_settings_schema")(body)
    by_key = {field.key: field for field in schema.fields}

    assert by_key["example.future"].type == "color"
    assert "color" not in _require_attr(settings, "KNOWN_SCHEMA_TYPES")


def test_the_known_type_set_is_exactly_the_five_renderable_types() -> None:
    settings = _settings()

    assert _require_attr(settings, "KNOWN_SCHEMA_TYPES") == frozenset(
        {"string", "number", "boolean", "list", "select"}
    )


@pytest.mark.parametrize(
    "body",
    [
        pytest.param([], id="list"),
        pytest.param("schema", id="string"),
        pytest.param(None, id="null"),
        pytest.param({"fields": []}, id="fields-not-an-object"),
        pytest.param({"fields": {}}, id="category-order-absent"),
    ],
)
def test_a_malformed_schema_body_is_refused(body: Any) -> None:
    settings = _settings()
    decode_error = _require_attr(settings, "SettingsDecodeError")

    assert issubclass(decode_error, ValueError)
    with pytest.raises(decode_error):
        _require_attr(settings, "decode_settings_schema")(body)


@pytest.mark.parametrize(
    "entry",
    [
        pytest.param({"description": "x", "category": "general"}, id="type-absent"),
        pytest.param({"type": "string", "category": "general"}, id="no-description"),
        pytest.param({"type": "string", "description": "x"}, id="category-absent"),
        pytest.param("string", id="entry-not-an-object"),
    ],
)
def test_a_field_entry_missing_shape_is_refused(entry: Any) -> None:
    """Every Hermes field carries type/description/category; an entry without
    them is a shape mismatch, unlike an unknown *value* of type."""
    settings = _settings()
    decode_error = _require_attr(settings, "SettingsDecodeError")
    body = {"fields": {"example.broken": entry}, "category_order": ["general"]}

    with pytest.raises(decode_error):
        _require_attr(settings, "decode_settings_schema")(body)


def test_the_decoded_schema_is_frozen() -> None:
    from dataclasses import FrozenInstanceError

    settings = _settings()

    schema = _require_attr(settings, "decode_settings_schema")(_schema_body())

    with pytest.raises(FrozenInstanceError):
        schema.fields[0].key = "mutated"


# ── validation: known fields are editable and validated ──────────────────


def _field(settings: Any, key: str = "approvals.mode") -> Any:
    schema = _require_attr(settings, "decode_settings_schema")(_schema_body())
    return next(field for field in schema.fields if field.key == key)


def test_a_select_accepts_only_its_options() -> None:
    settings = _settings()
    validate = _require_attr(settings, "validate_settings_value")
    field = _field(settings)

    assert validate(field, "smart") is None
    message = validate(field, "sometimes")
    assert message is not None and "sometimes" in message


def test_a_number_parses_numerics_and_refuses_the_rest() -> None:
    settings = _settings()
    validate = _require_attr(settings, "validate_settings_value")
    field = _field(settings, "agent.max_turns")

    assert validate(field, 40) is None
    assert validate(field, "40") is None
    assert validate(field, "fast") is not None
    assert validate(field, "") is not None
    # bool is an int subclass; True must not parse as turns == 1.
    assert validate(field, True) is not None


def test_a_boolean_accepts_only_booleans() -> None:
    settings = _settings()
    validate = _require_attr(settings, "validate_settings_value")
    field = _field(settings, "display.show_reasoning")

    assert validate(field, True) is None
    assert validate(field, False) is None
    assert validate(field, 1) is not None
    assert validate(field, "true") is not None


def test_a_string_accepts_only_strings() -> None:
    settings = _settings()
    validate = _require_attr(settings, "validate_settings_value")
    field = _field(settings, "timezone")

    assert validate(field, "UTC") is None
    assert validate(field, 7) is not None


def test_a_list_accepts_sequences_but_never_a_string() -> None:
    """``"ab"`` must not validate as the two providers ``a`` and ``b``."""
    settings = _settings()
    validate = _require_attr(settings, "validate_settings_value")
    field = _field(settings, "fallback_providers")

    assert validate(field, ["a", "b"]) is None
    assert validate(field, ("a", "b")) is None
    assert validate(field, "ab") is not None


def test_an_unknown_type_is_never_editable() -> None:
    """The read-only rendering has a domain fact behind it: validation of an
    unknown type always explains rather than accepts."""
    settings = _settings()
    validate = _require_attr(settings, "validate_settings_value")
    body = _schema_body()
    body["fields"]["example.future"] = {
        "type": "color",
        "description": "Invented next release.",
        "category": "general",
    }
    schema = _require_attr(settings, "decode_settings_schema")(body)
    field = next(item for item in schema.fields if item.key == "example.future")

    message = validate(field, "red")
    assert message is not None and "color" in message


# ── Tier-1 curation over the live schema ─────────────────────────────────


def test_every_tier1_entry_is_fully_dispositioned() -> None:
    """Omission is not an exclusion (implementation plan, Coverage
    Dispositions): each curated entry names owner, route/mechanism, scope,
    control, effect, disposition, and rationale — and none is unresolved."""
    catalog = _catalog()

    entries = _require_attr(catalog, "TIER1")

    assert len(entries) > 0
    for entry in entries:
        assert entry.key, "manifest entry without a key"
        assert entry.owner, f"{entry.key} names no owner"
        assert entry.route or entry.mechanism, f"{entry.key} names no route"
        assert entry.scope in ("profile", "host", "request"), entry.key
        assert entry.control, f"{entry.key} names no control"
        assert entry.effect in (
            "live",
            "next-session",
            "gateway-restart",
            "dashboard-restart-no-api",
            "unverified",
        ), entry.key
        assert entry.disposition == "implement", entry.key
        assert entry.rationale, f"{entry.key} records no rationale"


@pytest.mark.parametrize(
    "key",
    [
        "display.resume_last_session",
        "approvals.mode",
        "agent.max_turns",
        "tts.provider",
        "model_context_length",
    ],
)
def test_curated_desktop_keys_are_tier1(key: str) -> None:
    """The Desktop SECTIONS mirror: first-class controls, not the generic
    fallback. Appearance's ``display.resume_last_session`` is Tier 1 even
    though its Desktop section is otherwise Desktop-local (A1-7)."""
    catalog = _catalog()

    assert _require_attr(catalog, "tier_of")(key) == 1


def test_a_hermes_tui_presentation_key_is_tier2() -> None:
    """Hermes's own TUI keys (``theme``, ``skin``, ``density``…) configure a
    terminal UI Talaria replaces; they stay reachable under Tier 2, never
    promoted to first-class Talaria controls."""
    catalog = _catalog()

    assert _require_attr(catalog, "tier_of")("display.theme") == 2


def test_an_uncurated_schema_key_falls_through_to_tier2() -> None:
    """``matrix`` is a live schema category at 3236f440 with no Desktop
    SECTIONS row; its keys render generically rather than vanishing."""
    catalog = _catalog()

    assert _require_attr(catalog, "tier_of")("matrix.homeserver_url") == 2


def test_pet_keys_are_excluded_with_the_recorded_rationale() -> None:
    """KTD11: Hermes profile state with no Talaria rendering surface."""
    catalog = _catalog()

    assert _require_attr(catalog, "surface_disposition")("pet.enabled") == "excluded"


# ── the profile-explicit Reset contract ──────────────────────────────────


def test_a_reset_patch_restores_only_values_that_differ_from_defaults() -> None:
    """Reset is a sparse patch of server defaults, not a full replace: keys
    already at their default stay out of the PUT (A1-6's sparseness holds
    for Reset too)."""
    settings = _settings()
    build = _require_attr(settings, "build_reset_patch")

    patch = build(
        defaults={"agent": {"max_turns": 25, "api_max_retries": 3}},
        saved={"agent": {"max_turns": 40, "api_max_retries": 3}},
        keys=("agent.max_turns", "agent.api_max_retries"),
    )

    assert patch == {"agent": {"max_turns": 25}}


def test_a_reset_command_always_names_its_target() -> None:
    """The Desktop resets the *ambient* profile (CFG-A1 §10.2); Talaria's
    Reset names its target explicitly and never falls through to ambient."""
    settings = _settings()
    commands = _require_module("talaria.domain.settings_commands")
    target = _require_attr(settings, "ConfigTarget")(
        connection_id="local-fixture", profile_name="alpha-fixture"
    )

    command = _require_attr(commands, "ResetConfig")(
        target=target, patch={"agent": {"max_turns": 25}}
    )
    spec = _require_attr(commands, "rest_request")(command)

    assert command.target == target
    assert spec.method == "PUT"
    assert spec.path == "/api/config"
    assert spec.query["profile"] == "alpha-fixture"
    assert spec.body == {"config": {"agent": {"max_turns": 25}}}
