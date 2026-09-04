"""Issue #123 U1: sparse inheritance across shared defaults, groups, overrides.

The resolution order lives in :meth:`ThemeRegistry.resolve` — the single
funnel every built-in, user, and imported specification passes through, so
issue #124's imports share this semantics with no second implementation.
Shared defaults are the registry default's tokens, groups are the spec's
per-category values, and overrides are the spec's own tokens.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from talaria.themes import THEME_TOKENS, ThemeSpec
from talaria.themes.builtins import (
    ACCESSIBLE_HIGH_CONTRAST,
    BUILTIN_THEMES,
    DARK_GREEN_TERMINAL,
    NEUTRAL_DARK,
    REFINED_DEFAULT,
)
from talaria.themes.storage import (
    StoredThemeError,
    load_user_theme_spec,
    serialize_user_theme,
)
from talaria.ui.theme import BUILTIN_THEME_REGISTRY, ThemeRegistry

ORIGINAL_FOUR = (
    REFINED_DEFAULT,
    DARK_GREEN_TERMINAL,
    NEUTRAL_DARK,
    ACCESSIBLE_HIGH_CONTRAST,
)


def _sparse_registry(spec: ThemeSpec) -> ThemeRegistry:
    return ThemeRegistry((REFINED_DEFAULT, spec))


def test_override_beats_group_beats_default_for_marker_and_text() -> None:
    """One chain proving the full order on both group-addressable roles."""
    spec = ThemeSpec(
        slug="chain-example",
        name="Chain Example",
        dark=True,
        tokens={"talaria.transcript.assistant": "#111111"},
        groups={
            "assistant": {
                "talaria.transcript.assistant": "#222222",
                "talaria.text": "#333333",
            }
        },
    )
    resolved = _sparse_registry(spec).resolve("chain-example")

    # The override beats the group for the marker ...
    assert resolved.tokens["talaria.transcript.assistant"] == "#111111"
    # ... the group beats the shared default for the category body text ...
    assert resolved.transcript_text["assistant"] == "#333333"
    # ... and an untouched category still reads the shared default.
    assert (
        resolved.transcript_text["reasoning"]
        == REFINED_DEFAULT.tokens["talaria.text"]
    )


def test_sparse_groups_fall_through_without_resetting_siblings() -> None:
    """A group naming one category and one role leaves everything else alone."""
    spec = ThemeSpec(
        slug="sparse-example",
        name="Sparse Example",
        dark=True,
        tokens={},
        groups={"fault": {"talaria.text": "#444444"}},
    )
    resolved = _sparse_registry(spec).resolve("sparse-example")

    assert resolved.transcript_text["fault"] == "#444444"
    for category in ("operator", "assistant", "reasoning", "activity", "session"):
        assert (
            resolved.transcript_text[category]
            == REFINED_DEFAULT.tokens["talaria.text"]
        )
    assert dict(resolved.tokens) == dict(REFINED_DEFAULT.tokens)
    assert resolved.filled_tokens == tuple(THEME_TOKENS)


def test_empty_groups_unknown_keys_and_nulls_fall_through_silently() -> None:
    """The lenient layer: nothing usable means defaults with no notices."""
    spec = ThemeSpec(
        slug="lenient-example",
        name="Lenient Example",
        dark=True,
        tokens={},
        groups={
            "not-a-category": {"talaria.text": "#555555"},
            "assistant": {
                "talaria.text": None,
                "not-a-token": "#666666",
                "talaria.primary": "#777777",
            },
            "reasoning": "not-a-mapping",  # type: ignore[dict-item]
        },
    )
    resolved = _sparse_registry(spec).resolve("lenient-example")

    assert dict(resolved.tokens) == dict(REFINED_DEFAULT.tokens)
    assert (
        resolved.transcript_text["assistant"]
        == REFINED_DEFAULT.tokens["talaria.text"]
    )
    assert resolved.notices == (
        "theme 'Lenient Example' filled missing tokens from Refined Default: "
        + ", ".join(THEME_TOKENS),
    )


def test_a_malformed_group_value_never_breaks_the_rest() -> None:
    """One bad color is ignored with a notice; its siblings still apply."""
    spec = ThemeSpec(
        slug="malformed-example",
        name="Malformed Example",
        dark=True,
        tokens={},
        groups={
            "assistant": {
                "talaria.text": "not-a-color",
                "talaria.transcript.assistant.background": "#101010",
            },
            "operator": {"talaria.text": "#202020"},
        },
    )
    resolved = _sparse_registry(spec).resolve("malformed-example")

    # The malformed text falls through, and the shared text it falls back to
    # cannot survive the group's near-black background either — so the floor
    # holds with white rather than rendering dark-on-dark.
    assert resolved.transcript_text["assistant"] == "#FFFFFF"
    assert resolved.tokens["talaria.transcript.assistant.background"] == "#101010"
    assert resolved.transcript_text["operator"] == "#202020"
    assert any(
        "ignores malformed group colors" in notice
        and "assistant.talaria.text" in notice
        for notice in resolved.notices
    )
    assert any(
        "holds the readability floor" in notice and "assistant.text" in notice
        for notice in resolved.notices
    )


def test_canonical_token_spelling_beats_role_nicknames_deterministically() -> None:
    """Both spellings name the same role; the canonical one always wins."""
    spec = ThemeSpec(
        slug="spelling-example",
        name="Spelling Example",
        dark=True,
        tokens={},
        groups={"assistant": {"text": "#303030", "talaria.text": "#404040"}},
    )
    resolved = _sparse_registry(spec).resolve("spelling-example")

    assert resolved.transcript_text["assistant"] == "#404040"


def test_the_original_four_carry_no_groups_and_resolve_byte_identical() -> None:
    """U1's preservation clause: the v0.5.0 themes pass through untouched."""
    assert tuple(BUILTIN_THEMES[:4]) == ORIGINAL_FOUR
    for spec in ORIGINAL_FOUR:
        assert dict(spec.groups) == {}
        resolved = BUILTIN_THEME_REGISTRY.resolve(spec.slug)
        assert dict(resolved.tokens) == dict(spec.tokens)
        assert resolved.filled_tokens == ()
        assert resolved.notices == ()
        for category in (
            "operator",
            "assistant",
            "reasoning",
            "activity",
            "session",
            "fault",
        ):
            assert resolved.transcript_text[category] == spec.tokens["talaria.text"]


def test_theme_spec_groups_coerce_leniently_without_raising() -> None:
    """Construction never raises for group content — resolution filters it."""
    spec = ThemeSpec(
        slug="coerce-example",
        name="Coerce Example",
        dark=True,
        tokens={},
        groups={"assistant": "nope", "operator": {"talaria.text": "#505050"}},  # type: ignore[dict-item]
    )
    assert dict(spec.groups["assistant"]) == {}
    assert dict(spec.groups["operator"]) == {"talaria.text": "#505050"}

    with pytest.raises(ValueError, match="theme groups must be a mapping"):
        ThemeSpec(
            slug="bad-groups",
            name="Bad Groups",
            dark=True,
            tokens={},
            groups="nope",  # type: ignore[arg-type]
        )


def test_stored_themes_round_trip_groups_including_nulls(tmp_path: Path) -> None:
    """The canonical stored format carries the groups layer, nulls included."""
    spec = ThemeSpec(
        slug="stored-groups",
        name="Stored Groups",
        dark=True,
        tokens=dict(REFINED_DEFAULT.tokens),
        groups={"assistant": {"talaria.text": None, "text": "#606060"}},
    )
    path = tmp_path / "stored-groups.json"
    path.write_bytes(serialize_user_theme(spec))

    payload = json.loads(path.read_text(encoding="utf-8"))
    assert payload["groups"] == {"assistant": {"talaria.text": None, "text": "#606060"}}

    loaded = load_user_theme_spec(path)
    assert dict(loaded.groups["assistant"]) == {
        "talaria.text": None,
        "text": "#606060",
    }
    resolved = ThemeRegistry((REFINED_DEFAULT, loaded)).resolve("stored-groups")
    assert resolved.transcript_text["assistant"] == "#606060"


def test_stored_themes_without_groups_still_load(tmp_path: Path) -> None:
    """v0.5.0 files predate the layer and resolve exactly as before."""
    payload = {
        "dark": REFINED_DEFAULT.dark,
        "name": "Legacy Groups",
        "schema_version": "talaria-theme-v1",
        "slug": "legacy-groups",
        "tokens": dict(REFINED_DEFAULT.tokens),
    }
    path = tmp_path / "legacy-groups.json"
    path.write_text(json.dumps(payload), encoding="utf-8")

    loaded = load_user_theme_spec(path)
    assert dict(loaded.groups) == {}
    resolved = ThemeRegistry((REFINED_DEFAULT, loaded)).resolve("legacy-groups")
    assert dict(resolved.tokens) == dict(REFINED_DEFAULT.tokens)
    assert resolved.notices == ()


def test_stored_themes_reject_misshapen_groups(tmp_path: Path) -> None:
    """The stored format shape-checks groups even though resolution is lenient."""
    payload = {
        "dark": REFINED_DEFAULT.dark,
        "groups": {"assistant": {"talaria.text": 7}},
        "name": "Misshapen Groups",
        "schema_version": "talaria-theme-v1",
        "slug": "misshapen-groups",
        "tokens": dict(REFINED_DEFAULT.tokens),
    }
    path = tmp_path / "misshapen-groups.json"
    path.write_text(json.dumps(payload), encoding="utf-8")

    with pytest.raises(StoredThemeError, match="groups must map categories"):
        load_user_theme_spec(path)
