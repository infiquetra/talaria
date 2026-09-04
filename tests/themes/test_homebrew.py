"""Issue #123 U4: the Homebrew fifth built-in, at the data level.

Registry behavior (resolution, picker rows, startup selection) is covered
in ``tests/ui/test_theme.py``; what is pinned here needs no terminal
framework: Homebrew is complete and valid, restrainedly green-black and
distinct from Dark Green Terminal, holds every readability floor, and is
never the default.
"""

from __future__ import annotations

from talaria.themes import THEME_TOKENS, TRANSCRIPT_CATEGORIES, contrast_ratio
from talaria.themes.builtins import (
    BUILTIN_THEMES,
    DARK_GREEN_TERMINAL,
    HOMEBREW,
    REFINED_DEFAULT,
)

#: Body pairings that must hold the 4.5 text floor on Homebrew.
_TEXT_PAIRS: tuple[tuple[str, str], ...] = (
    ("talaria.text", "talaria.canvas"),
    ("talaria.text", "talaria.surface"),
    ("talaria.text", "talaria.canvas"),
    ("talaria.text.muted", "talaria.canvas"),
    ("talaria.primary", "talaria.canvas"),
    ("talaria.success", "talaria.canvas"),
    ("talaria.warning", "talaria.canvas"),
    ("talaria.error", "talaria.canvas"),
    ("talaria.selection.text", "talaria.selection.background"),
    ("talaria.selection.text", "talaria.focus"),
    ("talaria.status.text", "talaria.status.background"),
    ("talaria.status.muted", "talaria.status.background"),
    ("talaria.status.success", "talaria.status.background"),
    ("talaria.status.warning", "talaria.status.background"),
    ("talaria.status.error", "talaria.status.background"),
    ("talaria.status.attention", "talaria.status.background"),
    ("talaria.diff.added", "talaria.diff.added.background"),
    ("talaria.diff.removed", "talaria.diff.removed.background"),
    ("talaria.diff.hunk", "talaria.diff.hunk.background"),
)

#: Component pairings that must hold the 3.0 non-text floor on Homebrew.
_COMPONENT_PAIRS: tuple[tuple[str, str], ...] = (
    ("talaria.border", "talaria.surface"),
    ("talaria.border.muted", "talaria.canvas"),
    ("talaria.focus", "talaria.canvas"),
    ("talaria.selection.background", "talaria.canvas"),
    ("talaria.status.separator", "talaria.status.background"),
)


def test_homebrew_is_the_registered_fifth_builtin() -> None:
    """Available by default: last in the registry, behind the old four."""
    assert BUILTIN_THEMES[-1] is HOMEBREW
    assert tuple(spec.slug for spec in BUILTIN_THEMES) == (
        "refined-default",
        "dark-green-terminal",
        "neutral-dark",
        "accessible-high-contrast",
        "homebrew",
    )
    assert HOMEBREW.name == "Homebrew"
    assert HOMEBREW.dark is True


def test_homebrew_is_complete_and_valid() -> None:
    """Fifty-eight canonical tokens, every one an opaque uppercase color."""
    assert tuple(sorted(HOMEBREW.tokens)) == tuple(sorted(THEME_TOKENS))
    assert all(
        value.startswith("#") and len(value) == 7 and value == value.upper()
        for value in HOMEBREW.tokens.values()
    )


def test_homebrew_states_its_provenance_per_d4() -> None:
    """D4(a): point-of-use provenance; the palette itself is unchanged."""
    assert HOMEBREW.description == (
        "Talaria-designed green-black palette (v0.6.0) — not sampled "
        "from a host terminal, not the Homebrew package manager"
    )
    for spec in BUILTIN_THEMES:
        if spec is not HOMEBREW:
            assert spec.description == "", spec.slug


def test_homebrew_is_never_the_default() -> None:
    """The startup default does not move: Refined Default stays first."""
    assert REFINED_DEFAULT.slug == "refined-default"
    assert BUILTIN_THEMES[0] is REFINED_DEFAULT
    assert HOMEBREW.slug != REFINED_DEFAULT.slug


def test_homebrew_is_restrained_next_to_dark_green_terminal() -> None:
    """A second green-black theme must earn its slot: no neon accents."""
    assert HOMEBREW.tokens["talaria.accent"] != DARK_GREEN_TERMINAL.tokens[
        "talaria.accent"
    ]
    assert HOMEBREW.tokens["talaria.canvas"] != DARK_GREEN_TERMINAL.tokens[
        "talaria.canvas"
    ]
    assert HOMEBREW.tokens["talaria.focus"] != DARK_GREEN_TERMINAL.tokens[
        "talaria.focus"
    ]
    # Restrained means darker: every signature surface sits below #12 in red.
    for token in (
        "talaria.canvas",
        "talaria.surface",
        "talaria.status.background",
        "talaria.transcript.fault.background",
    ):
        assert int(HOMEBREW.tokens[token][1:3], 16) <= 0x16, token


def test_homebrew_holds_every_readability_floor() -> None:
    """The documented provenance claims, measured rather than trusted."""
    for foreground, background in _TEXT_PAIRS:
        ratio = contrast_ratio(
            HOMEBREW.tokens[foreground], HOMEBREW.tokens[background]
        )
        assert ratio >= 4.5, (foreground, background, round(ratio, 2))
    for foreground, background in _COMPONENT_PAIRS:
        ratio = contrast_ratio(
            HOMEBREW.tokens[foreground], HOMEBREW.tokens[background]
        )
        assert ratio >= 3.0, (foreground, background, round(ratio, 2))


def test_homebrew_category_triples_hold_their_floors() -> None:
    """Each category's group text and marker survive their own fill."""
    groups = dict(HOMEBREW.groups)
    assert sorted(groups) == sorted(TRANSCRIPT_CATEGORIES)
    for category in TRANSCRIPT_CATEGORIES:
        background = HOMEBREW.tokens[f"talaria.transcript.{category}.background"]
        text = groups[category]["talaria.text"]
        marker = HOMEBREW.tokens[f"talaria.transcript.{category}"]
        assert isinstance(text, str)
        assert contrast_ratio(text, background) >= 4.5, (category, text)
        assert contrast_ratio(marker, background) >= 3.0, (category, marker)
