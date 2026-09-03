"""Issue #123 U2: independent category body text, stripe/marker, background.

Each transcript category resolves an independent (text, marker, background)
triple through the registry: the marker and background ride the category's
existing tokens (overridable per token or per group), the body text rides
the category's group ``talaria.text`` value defaulting to the shared body
text, and every triple is held to the existing contrast floors. Unknown
categories fall back to the plain-surface group default.
"""

from __future__ import annotations

from talaria.themes import ThemeSpec, contrast_ratio
from talaria.themes.builtins import REFINED_DEFAULT
from talaria.ui.theme import (
    BUILTIN_THEME_REGISTRY,
    TRANSCRIPT_MARKER_CONTRAST_FLOOR,
    TRANSCRIPT_TEXT_CONTRAST_FLOOR,
    ThemeRegistry,
)


def _registry(spec: ThemeSpec) -> ThemeRegistry:
    return ThemeRegistry((REFINED_DEFAULT, spec))


def test_per_category_body_text_renders_independently() -> None:
    """Two categories name different texts; the rest keep the shared text."""
    spec = ThemeSpec(
        slug="independent-text",
        name="Independent Text",
        dark=True,
        tokens={},
        groups={
            "assistant": {"talaria.text": "#101010"},
            "fault": {"talaria.text": "#202020"},
        },
    )
    resolved = _registry(spec).resolve("independent-text")

    assert resolved.transcript_text["assistant"] == "#101010"
    assert resolved.transcript_text["fault"] == "#202020"
    for category in ("operator", "reasoning", "activity", "session"):
        assert (
            resolved.transcript_text[category]
            == REFINED_DEFAULT.tokens["talaria.text"]
        )


def test_marker_and_background_vary_independently() -> None:
    """Overriding a marker leaves its background (and every sibling) alone."""
    spec = ThemeSpec(
        slug="independent-channel",
        name="Independent Channel",
        dark=True,
        tokens={"talaria.transcript.reasoning": "#303030"},
        groups={
            "reasoning": {"talaria.transcript.reasoning.background": "#D8D8D8"}
        },
    )
    resolved = _registry(spec).resolve("independent-channel")

    assert resolved.tokens["talaria.transcript.reasoning"] == "#303030"
    assert resolved.tokens["talaria.transcript.reasoning.background"] == "#D8D8D8"
    assert (
        resolved.tokens["talaria.transcript.assistant"]
        == REFINED_DEFAULT.tokens["talaria.transcript.assistant"]
    )
    assert (
        resolved.tokens["talaria.transcript.assistant.background"]
        == REFINED_DEFAULT.tokens["talaria.transcript.assistant.background"]
    )


def test_group_marker_loses_to_a_token_override() -> None:
    """The override layer beats the group layer for the stripe, verbatim."""
    spec = ThemeSpec(
        slug="marker-chain",
        name="Marker Chain",
        dark=True,
        tokens={"talaria.transcript.activity": "#505050"},
        groups={"activity": {"talaria.transcript.activity": "#606060"}},
    )
    resolved = _registry(spec).resolve("marker-chain")

    assert resolved.tokens["talaria.transcript.activity"] == "#505050"


def test_unreadable_body_text_resolves_to_the_floor_with_a_notice() -> None:
    """Dark-on-dark body text never renders — it holds the 4.5 floor."""
    spec = ThemeSpec(
        slug="unreadable-text",
        name="Unreadable Text",
        dark=True,
        tokens={},
        groups={"assistant": {"talaria.text": "#E0E0E0"}},
    )
    resolved = _registry(spec).resolve("unreadable-text")

    background = resolved.tokens["talaria.transcript.assistant.background"]
    assert contrast_ratio("#E0E0E0", background) < TRANSCRIPT_TEXT_CONTRAST_FLOOR
    held = resolved.transcript_text["assistant"]
    assert contrast_ratio(held, background) >= TRANSCRIPT_TEXT_CONTRAST_FLOOR
    assert any(
        "holds the readability floor" in notice and "assistant.text" in notice
        for notice in resolved.notices
    )


def test_unreadable_marker_resolves_to_its_own_floor() -> None:
    """A stripe that melts into its fill is held to the 3.0 component floor."""
    spec = ThemeSpec(
        slug="unreadable-marker",
        name="Unreadable Marker",
        dark=True,
        tokens={"talaria.transcript.fault": "#E8B0B0"},
    )
    resolved = _registry(spec).resolve("unreadable-marker")

    background = resolved.tokens["talaria.transcript.fault.background"]
    assert contrast_ratio("#E8B0B0", background) < TRANSCRIPT_MARKER_CONTRAST_FLOOR
    held = resolved.tokens["talaria.transcript.fault"]
    assert contrast_ratio(held, background) >= TRANSCRIPT_MARKER_CONTRAST_FLOOR
    assert any(
        "holds the readability floor" in notice and "fault.marker" in notice
        for notice in resolved.notices
    )


def test_unknown_categories_fall_back_to_the_group_default() -> None:
    """A category no theme named renders as plain body text on the surface."""
    resolved = BUILTIN_THEME_REGISTRY.resolve("refined-default")
    style = resolved.category_style("not-a-category")

    assert style.text == resolved.tokens["talaria.text"]
    assert style.marker == resolved.tokens["talaria.text.muted"]
    assert style.background == resolved.tokens["talaria.surface"]
    assert (
        contrast_ratio(style.text, style.background)
        >= TRANSCRIPT_TEXT_CONTRAST_FLOOR
    )


def test_known_categories_expose_independent_text_variables() -> None:
    """The Textual bridge carries one body-text variable per category."""
    theme = BUILTIN_THEME_REGISTRY.to_textual_theme("homebrew")

    for category in (
        "operator",
        "assistant",
        "reasoning",
        "activity",
        "session",
        "fault",
    ):
        variable = f"talaria-transcript-{category}-text"
        assert variable in theme.variables
    texts = {
        theme.variables[f"talaria-transcript-{category}-text"]
        for category in ("operator", "reasoning", "fault")
    }
    assert len(texts) == 3, "Homebrew proves the texts vary per category"
