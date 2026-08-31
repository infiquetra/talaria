"""Issue #104 theme data, Textual bridge, and live palette preview."""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path

import pytest

from talaria.config import load_config
from talaria.themes import THEME_TOKENS, ThemeSpec
from talaria.themes.builtins import BUILTIN_THEMES, REFINED_DEFAULT
from talaria.ui.theme import (
    BUILTIN_THEME_REGISTRY,
    ThemeRegistry,
    contrast_ratio,
    textual_variable_name,
)
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


def test_config_and_registry_unknown_theme_notices_are_byte_identical(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    requested = "not-installed"
    (isolated_global_config_dir / "config.toml").write_text(
        f'[theme]\nname = "{requested}"\n', encoding="utf-8"
    )

    config_notice = load_config(cwd=tmp_path).notices
    registry_notice = BUILTIN_THEME_REGISTRY.resolve(requested).notices

    assert config_notice == registry_notice


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
                contrast_ratio(
                    spec.tokens[f"talaria.status.{state}"],
                    spec.tokens["talaria.status.background"],
                ),
                2,
            )
            for spec in BUILTIN_THEMES
        )
        assert observed == ratios
        assert min(observed) >= 4.5


def _measurement_table(heading: str) -> dict[str, tuple[float, ...]]:
    text = VISUAL_SPEC.read_text(encoding="utf-8")
    table = text.split(heading, 1)[1].split("| **Minimum**", 1)[0]
    measurements: dict[str, tuple[float, ...]] = {}
    for line in table.splitlines():
        if not line.startswith("| ") or line.startswith("| Pair") or line.startswith("|---"):
            continue
        pair, *ratios = (cell.strip() for cell in line.strip("|").split("|"))
        measurements[pair] = tuple(float(ratio.removesuffix(":1")) for ratio in ratios)
    return measurements


_TEXT_PAIRS: dict[str, tuple[str, str]] = {
    "body / canvas": ("talaria.text", "talaria.canvas"),
    "body / surface": ("talaria.text", "talaria.surface"),
    "body / panel": ("talaria.text", "talaria.panel"),
    "muted / canvas": ("talaria.text.muted", "talaria.canvas"),
    "muted / surface": ("talaria.text.muted", "talaria.surface"),
    "muted / panel": ("talaria.text.muted", "talaria.panel"),
    "primary / canvas": ("talaria.primary", "talaria.canvas"),
    "secondary / canvas": ("talaria.secondary", "talaria.canvas"),
    "accent / canvas": ("talaria.accent", "talaria.canvas"),
    "success / canvas": ("talaria.success", "talaria.canvas"),
    "warning / canvas": ("talaria.warning", "talaria.canvas"),
    "error / canvas": ("talaria.error", "talaria.canvas"),
    "selection text / selection": (
        "talaria.selection.text",
        "talaria.selection.background",
    ),
    "cursor text / focus": ("talaria.selection.text", "talaria.focus"),
    "status text / status": ("talaria.status.text", "talaria.status.background"),
    "status muted / status": ("talaria.status.muted", "talaria.status.background"),
    "status success / status": (
        "talaria.status.success",
        "talaria.status.background",
    ),
    "status warning / status": (
        "talaria.status.warning",
        "talaria.status.background",
    ),
    "status error / status": ("talaria.status.error", "talaria.status.background"),
    "status attention / status": (
        "talaria.status.attention",
        "talaria.status.background",
    ),
    "inspector body / inspector": (
        "talaria.text",
        "talaria.inspector.background",
    ),
    "inspector heading / inspector": (
        "talaria.inspector.heading",
        "talaria.inspector.background",
    ),
    "diff context / canvas": ("talaria.diff.context", "talaria.canvas"),
    "diff line number / canvas": ("talaria.diff.line-number", "talaria.canvas"),
    "diff added / added line": ("talaria.diff.added", "talaria.diff.added.background"),
    "diff removed / removed line": (
        "talaria.diff.removed",
        "talaria.diff.removed.background",
    ),
    "diff hunk / hunk line": ("talaria.diff.hunk", "talaria.diff.hunk.background"),
    "diff added / intraline": (
        "talaria.diff.added",
        "talaria.diff.intraline-added.background",
    ),
    "diff removed / intraline": (
        "talaria.diff.removed",
        "talaria.diff.intraline-removed.background",
    ),
}
for group in ("operator", "assistant", "reasoning", "activity", "session", "fault"):
    _TEXT_PAIRS[f"transcript body / {group}"] = (
        "talaria.text",
        f"talaria.transcript.{group}.background",
    )
for syntax in (
    "comment",
    "keyword",
    "string",
    "number",
    "function",
    "type",
    "variable",
    "operator",
    "constant",
):
    for background, token in (
        ("surface", "talaria.surface"),
        ("canvas", "talaria.canvas"),
        ("added line", "talaria.diff.added.background"),
        ("removed line", "talaria.diff.removed.background"),
    ):
        _TEXT_PAIRS[f"syntax {syntax} / {background}"] = (
            f"talaria.syntax.{syntax}",
            token,
        )

_COMPONENT_PAIRS: dict[str, tuple[str, str]] = {
    "border / surface": ("talaria.border", "talaria.surface"),
    "muted border / canvas": ("talaria.border.muted", "talaria.canvas"),
    "focus / canvas": ("talaria.focus", "talaria.canvas"),
    "selection / canvas": ("talaria.selection.background", "talaria.canvas"),
    "status separator / status": (
        "talaria.status.separator",
        "talaria.status.background",
    ),
    "inspector border / canvas": ("talaria.inspector.border", "talaria.canvas"),
    "added marker / added line": (
        "talaria.diff.added",
        "talaria.diff.added.background",
    ),
    "removed marker / removed line": (
        "talaria.diff.removed",
        "talaria.diff.removed.background",
    ),
    "hunk marker / hunk line": (
        "talaria.diff.hunk",
        "talaria.diff.hunk.background",
    ),
    "added marker / intraline": (
        "talaria.diff.added",
        "talaria.diff.intraline-added.background",
    ),
    "removed marker / intraline": (
        "talaria.diff.removed",
        "talaria.diff.intraline-removed.background",
    ),
}
for group in ("operator", "assistant", "reasoning", "activity", "session", "fault"):
    _COMPONENT_PAIRS[f"{group} marker / {group} fill"] = (
        f"talaria.transcript.{group}",
        f"talaria.transcript.{group}.background",
    )


@pytest.mark.parametrize(
    ("heading", "pairs", "minimum"),
    [
        ("### Text and glyph contrast measurements", _TEXT_PAIRS, 4.5),
        ("### Non-text component contrast measurements", _COMPONENT_PAIRS, 3.0),
    ],
)
def test_the_complete_contrast_matrix_matches_the_measured_visual_specification(
    heading: str,
    pairs: dict[str, tuple[str, str]],
    minimum: float,
) -> None:
    expected = _measurement_table(heading)
    assert set(pairs) == set(expected)

    for pair, (foreground, background) in pairs.items():
        observed = tuple(
            round(contrast_ratio(spec.tokens[foreground], spec.tokens[background]), 2)
            for spec in BUILTIN_THEMES
        )
        assert observed == expected[pair], pair
        assert min(observed) >= minimum, pair

    accessible = min(
        contrast_ratio(
            BUILTIN_THEMES[-1].tokens[foreground],
            BUILTIN_THEMES[-1].tokens[background],
        )
        for foreground, background in pairs.values()
    )
    assert round(accessible, 2) >= (6.22 if minimum == 4.5 else 6.08)


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


@pytest.mark.asyncio
async def test_startup_theme_and_fallback_notice_are_visible_from_first_mount() -> None:
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_name="not-installed",
    )

    assert app.theme == "refined-default"
    assert any("not-installed" in entry.text for entry in app.state.transcript)
    async with app.run_test(size=(80, 24)) as pilot:
        await app.render_snapshot()
        await pilot.pause()
        assert "not-installed" in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_command_applies_session_precedence_and_cancel_restores_it(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "user"
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_name="dark-green-terminal",
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
    )

    async with app.run_test(size=(80, 24)) as pilot:
        app.composer.text = "/theme"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert app.palette.is_theme_active
        assert app.palette.selected_theme is not None
        assert app.palette.selected_theme.slug == "dark-green-terminal"

        await pilot.press("down", "enter")
        await pilot.pause()
        assert app.theme == "neutral-dark"
        assert app.session_theme_slug == "neutral-dark"
        assert not config_dir.exists(), "an in-memory selection wrote user config"
        assert not (tmp_path / ".talaria").exists(), (
            "an in-memory selection wrote repository config"
        )

        app.composer.text = "/theme"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.press("down", "escape")
        await pilot.pause()

        assert app.theme == "neutral-dark"
        assert app.session_theme_slug == "neutral-dark"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_save_command_writes_user_default_and_explicit_repository(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "user"
    repository = tmp_path / "repository"
    repository.mkdir()
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_name="neutral-dark",
        theme_config_dir=config_dir,
        launch_cwd=repository,
    )

    async with app.run_test(size=(80, 24)) as pilot:
        app.composer.text = "/theme save"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()

        user_path = config_dir / "config.toml"
        assert user_path.read_text(encoding="utf-8") == (
            '[theme]\nname = "neutral-dark"\n'
        )

        app.composer.text = "/theme save repository"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()

        repository_path = repository / ".talaria" / "config.toml"
        assert repository_path.read_text(encoding="utf-8") == (
            '[theme]\nname = "neutral-dark"\n'
        )
        await app.shutdown_sources()
