"""Issue #104 theme data, Textual bridge, and live palette preview."""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path

import pytest

from talaria.themes import THEME_TOKENS, ThemeSpec
from talaria.themes.builtins import BUILTIN_THEMES, REFINED_DEFAULT
from talaria.ui.theme import BUILTIN_THEME_REGISTRY, ThemeRegistry, textual_variable_name
from tests.ui.conftest import event, paused_app, screen_text

VISUAL_SPEC = (
    Path(__file__).parents[2]
    / "docs"
    / "design"
    / "2026-08-30-talaria-v0-5-0-visual-spec.md"
)

BUILTIN_NAMES = (
    "Refined Default",
    "Dark Green Terminal",
    "Neutral Dark",
    "Accessible High Contrast",
)

BRIDGES: Mapping[str, tuple[str, ...]] = {
    "talaria.canvas": ("background",),
    "talaria.surface": ("surface",),
    "talaria.panel": ("panel",),
    "talaria.text": ("foreground", "text", "block-cursor-blurred-foreground"),
    "talaria.text.muted": ("text-muted", "foreground-muted"),
    "talaria.primary": ("primary", "text-primary"),
    "talaria.secondary": ("secondary", "text-secondary"),
    "talaria.accent": ("accent", "text-accent"),
    "talaria.success": ("success", "text-success"),
    "talaria.warning": ("warning", "text-warning"),
    "talaria.error": ("error", "text-error"),
    "talaria.border": ("border",),
    "talaria.border.muted": (
        "border-blurred",
        "block-cursor-blurred-background",
    ),
    "talaria.focus": ("block-cursor-background", "input-cursor-background"),
    "talaria.selection.background": (
        "input-selection-background",
        "screen-selection-background",
    ),
    "talaria.selection.text": (
        "input-selection-foreground",
        "screen-selection-foreground",
        "block-cursor-foreground",
        "input-cursor-foreground",
    ),
}


def _visual_spec_values() -> dict[str, dict[str, str]]:
    """Read the authoritative exact-value table without maintaining a second copy."""
    text = VISUAL_SPEC.read_text(encoding="utf-8")
    table = text.split("### Exact token values", 1)[1].split(
        "### Application and persistence behavior", 1
    )[0]
    values: dict[str, dict[str, str]] = {name: {} for name in BUILTIN_NAMES}
    for line in table.splitlines():
        if not line.startswith("| talaria."):
            continue
        token, *colors = (cell.strip() for cell in line.strip("|").split("|"))
        assert len(colors) == len(BUILTIN_NAMES)
        for name, color in zip(BUILTIN_NAMES, colors, strict=True):
            values[name][token] = color
    return values


def _relative_luminance(color: str) -> float:
    channels = [int(color[index : index + 2], 16) / 255 for index in (1, 3, 5)]
    linear = [
        channel / 12.92
        if channel <= 0.04045
        else ((channel + 0.055) / 1.055) ** 2.4
        for channel in channels
    ]
    return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]


def _contrast(first: str, second: str) -> float:
    lighter, darker = sorted(
        (_relative_luminance(first), _relative_luminance(second)), reverse=True
    )
    return (lighter + 0.05) / (darker + 0.05)


def test_the_registry_is_the_exact_58_token_visual_specification() -> None:
    expected = _visual_spec_values()

    assert len(THEME_TOKENS) == 58
    assert tuple(spec.name for spec in BUILTIN_THEMES) == BUILTIN_NAMES
    assert tuple(expected[BUILTIN_NAMES[0]]) == THEME_TOKENS
    for spec in BUILTIN_THEMES:
        assert dict(spec.tokens) == expected[spec.name]


def test_every_canonical_and_compatibility_variable_keeps_its_measured_value() -> None:
    for spec in BUILTIN_THEMES:
        resolved = BUILTIN_THEME_REGISTRY.resolve(spec.slug)
        theme = BUILTIN_THEME_REGISTRY.to_textual_theme(resolved)
        generated = {**theme.to_color_system().generate(), **theme.variables}

        assert theme.name == spec.slug
        assert theme.dark is spec.dark
        assert theme.text_alpha == 1.0
        assert theme.ansi is False
        for token, value in resolved.tokens.items():
            assert theme.variables[textual_variable_name(token)] == value
        for token, variable_names in BRIDGES.items():
            for variable_name in variable_names:
                assert generated[variable_name] == resolved.tokens[token]

        # These four fields are the values Textual itself reads before variables.
        assert theme.background == resolved.tokens["talaria.canvas"]
        assert theme.foreground == resolved.tokens["talaria.text"]
        assert theme.surface == resolved.tokens["talaria.surface"]
        assert theme.panel == resolved.tokens["talaria.panel"]


def test_partial_and_unknown_themes_fall_back_with_complete_visible_notes() -> None:
    partial = ThemeSpec(
        slug="partial-example",
        name="Partial Example",
        dark=True,
        tokens={
            token: value
            for token, value in REFINED_DEFAULT.tokens.items()
            if token not in {"talaria.status.attention", "talaria.syntax.constant"}
        },
    )
    registry = ThemeRegistry((REFINED_DEFAULT, partial))

    filled = registry.resolve("partial-example")
    assert filled.filled_tokens == (
        "talaria.status.attention",
        "talaria.syntax.constant",
    )
    assert filled.tokens["talaria.status.attention"] == "#58A6FF"
    assert filled.tokens["talaria.syntax.constant"] == "#A40E4C"
    assert len(filled.notices) == 1
    assert all(token in filled.notices[0] for token in filled.filled_tokens)

    unknown = registry.resolve("not-installed")
    assert unknown.slug == "refined-default"
    assert "not-installed" in unknown.notices[0]
    assert "Refined Default" in unknown.notices[0]

    wrong_type = registry.resolve(7)
    assert wrong_type.slug == "refined-default"
    assert "must be a string" in wrong_type.notices[0]


def test_all_sixteen_status_semantic_pairs_match_the_measured_ratios() -> None:
    expected = {
        "success": (5.77, 13.15, 9.85, 16.24),
        "warning": (5.80, 14.08, 11.09, 15.14),
        "error": (5.81, 8.05, 8.08, 7.57),
        "attention": (5.80, 15.31, 9.23, 15.64),
    }

    for state, ratios in expected.items():
        observed = tuple(
            round(
                _contrast(
                    spec.tokens[f"talaria.status.{state}"],
                    spec.tokens["talaria.status.background"],
                ),
                2,
            )
            for spec in BUILTIN_THEMES
        )
        assert observed == ratios
        assert min(observed) >= 4.5


@pytest.mark.asyncio
async def test_theme_picker_renders_four_rows_previews_and_escape_restores() -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(80, 24)) as pilot:
        BUILTIN_THEME_REGISTRY.register(app)
        app.theme = "refined-default"
        await app.palette.open_theme_picker(
            BUILTIN_THEMES,
            current_slug="refined-default",
            session_slug=None,
        )
        await pilot.pause()

        rendered = screen_text(app)
        assert all(name in rendered for name in BUILTIN_NAMES)
        assert list(app.palette.row_texts) == [
            "> Refined Default",
            "  Dark Green Terminal",
            "  Neutral Dark",
            "  Accessible High Contrast",
        ]

        await pilot.press("down")
        await pilot.pause()
        assert app.theme == "dark-green-terminal"
        assert app.palette.row_texts[1] == "> Dark Green Terminal"

        await pilot.press("escape")
        await pilot.pause()
        assert app.theme == "refined-default"
        assert app.palette.is_theme_active is False
        assert app.focused is app.composer.text_area
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_enter_keeps_the_preview_in_memory_and_browsing_writes_nothing(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    config_dir = tmp_path / "config"
    monkeypatch.setenv("TALARIA_CONFIG_DIR", str(config_dir))
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(80, 24)) as pilot:
        BUILTIN_THEME_REGISTRY.register(app)
        app.theme = "refined-default"
        await app.palette.open_theme_picker(
            BUILTIN_THEMES,
            current_slug="refined-default",
            session_slug=None,
        )

        await pilot.press("down", "down", "enter")
        await pilot.pause()

        assert app.theme == "neutral-dark"
        assert app.palette.is_theme_active is False
        assert not config_dir.exists()
        await app.shutdown_sources()
