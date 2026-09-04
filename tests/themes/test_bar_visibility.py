"""Issue #141's bar-state rule: the transcript bar is a theme-level field.

The transcript's left offset column paints unless the active theme says
otherwise: one optional boolean on the theme specification, carried through
the stored format and through resolution. A theme that does not name the
field keeps the column every prior theme always had, a non-boolean value is
rejected with an actionable error, and the registry never hides chrome it
cannot attribute to a known theme.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from talaria.themes import ThemeSpec
from talaria.themes.builtins import REFINED_DEFAULT
from talaria.themes.storage import (
    StoredThemeError,
    load_user_theme_spec,
    serialize_user_theme,
)
from talaria.ui.theme import (
    BUILTIN_THEME_REGISTRY,
    TRANSCRIPT_BAR_VARIABLE,
    ThemeRegistry,
)


def _hidden_spec() -> ThemeSpec:
    return ThemeSpec(
        slug="bar-hidden",
        name="Bar Hidden",
        dark=True,
        tokens=dict(REFINED_DEFAULT.tokens),
        transcript_bar_visible=False,
    )


def test_every_theme_keeps_the_bar_unless_it_says_otherwise() -> None:
    assert ThemeSpec(slug="plain", name="Plain", dark=True, tokens={}).transcript_bar_visible
    assert not _hidden_spec().transcript_bar_visible
    for spec in BUILTIN_THEME_REGISTRY.specs:
        assert spec.transcript_bar_visible, (
            "a built-in theme changed the bar every prior theme always had"
        )


def test_the_bar_field_must_be_a_boolean() -> None:
    with pytest.raises(ValueError, match="transcript_bar_visible must be a boolean"):
        ThemeSpec(
            slug="bad-bar",
            name="Bad Bar",
            dark=True,
            tokens={},
            transcript_bar_visible="no",  # type: ignore[arg-type]
        )


def test_resolution_carries_the_bar_state() -> None:
    registry = ThemeRegistry((REFINED_DEFAULT, _hidden_spec()))

    assert registry.resolve("bar-hidden").transcript_bar_visible is False
    assert registry.resolve("refined-default").transcript_bar_visible is True
    # An unresolvable request falls back to the default theme and its bar.
    assert registry.resolve("not-installed").transcript_bar_visible is True


def test_the_registry_degrades_visible_for_unknown_slugs() -> None:
    registry = ThemeRegistry((REFINED_DEFAULT, _hidden_spec()))

    assert registry.transcript_bar_visible("bar-hidden") is False
    assert registry.transcript_bar_visible("refined-default") is True
    assert registry.transcript_bar_visible("not-installed") is True


def test_stored_themes_round_trip_the_bar_field(tmp_path: Path) -> None:
    path = tmp_path / "bar-hidden.json"
    path.write_bytes(serialize_user_theme(_hidden_spec()))

    payload = json.loads(path.read_text(encoding="utf-8"))
    assert payload["transcript_bar_visible"] is False

    loaded = load_user_theme_spec(path)
    assert loaded.transcript_bar_visible is False


def test_stored_themes_without_the_field_keep_the_bar(tmp_path: Path) -> None:
    """A v0.6.0 file predates the field and loads exactly as before."""
    payload = {
        "dark": REFINED_DEFAULT.dark,
        "name": "Legacy Bar",
        "schema_version": "talaria-theme-v1",
        "slug": "legacy-bar",
        "tokens": dict(REFINED_DEFAULT.tokens),
    }
    path = tmp_path / "legacy-bar.json"
    path.write_text(json.dumps(payload), encoding="utf-8")

    assert load_user_theme_spec(path).transcript_bar_visible is True


def test_stored_themes_reject_a_non_boolean_bar_field(tmp_path: Path) -> None:
    payload = {
        "dark": REFINED_DEFAULT.dark,
        "name": "Misshapen Bar",
        "schema_version": "talaria-theme-v1",
        "slug": "misshapen-bar",
        "tokens": dict(REFINED_DEFAULT.tokens),
        "transcript_bar_visible": "hidden",
    }
    path = tmp_path / "misshapen-bar.json"
    path.write_text(json.dumps(payload), encoding="utf-8")

    with pytest.raises(
        StoredThemeError, match="transcript_bar_visible must be a boolean"
    ):
        load_user_theme_spec(path)


def test_the_bar_state_rides_in_the_registered_theme_value() -> None:
    """P1 repair (#141): Textual's theme reactive is silent on same-slug
    sets, and the app repaints a same-slug spec swap only when the
    registered Theme value actually differs. The bar state must therefore
    be part of that value, or a bar-only reload compares equal, paints
    nothing, and the mounted gutter stays stale."""
    registry = ThemeRegistry((REFINED_DEFAULT, _hidden_spec()))

    visible = registry.to_textual_theme("refined-default")
    hidden = registry.to_textual_theme("bar-hidden")
    assert visible.variables[TRANSCRIPT_BAR_VARIABLE] == "true"
    assert hidden.variables[TRANSCRIPT_BAR_VARIABLE] == "false"

    rebuilt = ThemeSpec(
        slug=REFINED_DEFAULT.slug,
        name=REFINED_DEFAULT.name,
        dark=REFINED_DEFAULT.dark,
        tokens=dict(REFINED_DEFAULT.tokens),
        transcript_bar_visible=False,
    )
    assert ThemeRegistry((rebuilt,)).to_textual_theme(rebuilt.slug) != visible, (
        "a bar-only same-slug rebuild must register as a real theme change"
    )
