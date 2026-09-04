"""Issue #104 theme data, Textual bridge, and live palette preview."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from collections.abc import Mapping
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

from talaria.config import load_config
from talaria.themes import THEME_TOKENS, ThemeSpec
from talaria.themes.builtins import BUILTIN_THEMES, REFINED_DEFAULT
from talaria.themes.storage import load_user_theme_specs
from talaria.ui.theme import (
    BUILTIN_THEME_REGISTRY,
    DEFAULT_THEME_SLUG,
    ThemeRegistry,
    contrast_ratio,
    textual_variable_name,
)
from talaria.ui.theme_import import import_vscode_theme
from tests.ui.conftest import event, paused_app, screen_text

VISUAL_SPEC = (
    Path(__file__).parents[2]
    / "docs"
    / "design"
    / "2026-08-30-talaria-v0-5-0-visual-spec.md"
)
STORED_THEME_SCHEMA = (
    Path(__file__).parents[2] / "docs" / "formats" / "stored-theme.schema.json"
)
VSCODE_THEME_FIXTURE = (
    Path(__file__).parents[1] / "fixtures" / "vscode-themes" / "sample-dark.json"
)

BUILTIN_NAMES = (
    "Refined Default",
    "Dark Green Terminal",
    "Neutral Dark",
    "Accessible High Contrast",
)

#: The v0.5.0 visual specification pins exactly these four themes. Homebrew
#: (issue #123) is the fifth built-in beside them: it is measured against
#: the same floors but pinned in ``tests/themes/test_homebrew.py``, never by
#: the release specification's tables.
ORIGINAL_BUILTIN_NAMES = BUILTIN_NAMES
HOMEBREW_NAME = "Homebrew"

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
    originals = [spec for spec in BUILTIN_THEMES if spec.name in ORIGINAL_BUILTIN_NAMES]
    assert tuple(spec.name for spec in originals) == ORIGINAL_BUILTIN_NAMES
    assert tuple(expected[BUILTIN_NAMES[0]]) == THEME_TOKENS
    for spec in originals:
        assert dict(spec.tokens) == expected[spec.name]


def test_import_replaces_a_planted_theme_symlink_without_writing_through_it(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    themes_dir = config_dir / "themes"
    themes_dir.mkdir(parents=True)
    outside = tmp_path / "outside.json"
    original = b'outside bytes that are not a theme\n'
    outside.write_bytes(original)
    planted = themes_dir / "planted-theme.json"
    planted.symlink_to(outside)

    report = import_vscode_theme(
        VSCODE_THEME_FIXTURE,
        name="planted-theme",
        config_dir=config_dir,
    )

    assert report.target_path == planted
    assert outside.read_bytes() == original
    assert not os.path.islink(planted)
    assert json.loads(planted.read_text(encoding="utf-8"))["slug"] == "planted-theme"


def _stored_theme_document(*, name: str, slug: str) -> dict[str, object]:
    return {
        "dark": REFINED_DEFAULT.dark,
        "name": name,
        "schema_version": "talaria-theme-v1",
        "slug": slug,
        "tokens": dict(REFINED_DEFAULT.tokens),
    }


def test_stored_theme_with_an_escape_in_its_display_name_is_rejected_at_load(
    tmp_path: Path,
) -> None:
    path = tmp_path / "themes" / "unsafe-name.json"
    path.parent.mkdir()
    path.write_text(
        json.dumps(_stored_theme_document(name="\x1b[31m", slug="unsafe-name")),
        encoding="utf-8",
    )

    specs, notices = load_user_theme_specs(config_dir=tmp_path)

    assert specs == ()
    assert len(notices) == 1
    assert str(path) in notices[0]
    assert "display name" in notices[0]


@pytest.mark.parametrize(
    "name",
    ("bell\x07", "delete\x7f", "bidi\u202eoverride", "x" * 129),
    ids=("c0-control", "del", "unicode-format", "over-limit"),
)
def test_theme_spec_rejects_unsafe_or_unbounded_display_names(name: str) -> None:
    with pytest.raises(ValueError, match="display name"):
        ThemeSpec(
            slug="unsafe-name",
            name=name,
            dark=REFINED_DEFAULT.dark,
            tokens=REFINED_DEFAULT.tokens,
        )


@pytest.mark.parametrize("name", ("\x1b[31m", "bidi\u202eoverride", "x" * 129))
def test_published_schema_rejects_the_same_unsafe_display_names(name: str) -> None:
    schema = json.loads(STORED_THEME_SCHEMA.read_text(encoding="utf-8"))
    name_schema = schema["properties"]["name"]
    assert "pattern" in name_schema
    assert name_schema["maxLength"] == 128

    errors = tuple(
        Draft202012Validator(schema).iter_errors(
            _stored_theme_document(name=name, slug="unsafe-name")
        )
    )

    assert errors
    assert any(tuple(error.path) == ("name",) for error in errors)


def test_importing_framework_independent_themes_does_not_import_textual() -> None:
    completed = subprocess.run(
        [
            sys.executable,
            "-c",
            "import talaria.themes, sys; assert 'textual' not in sys.modules",
        ],
        check=False,
        capture_output=True,
        text=True,
    )

    assert completed.returncode == 0, completed.stderr


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
        # The release specification measures the original four; Homebrew
        # holds the same floor without rewriting the release tables.
        assert observed[:4] == ratios
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
        assert observed[:4] == expected[pair], pair
        assert min(observed) >= minimum, pair

    reachable = {spec.name: spec for spec in BUILTIN_THEMES}
    accessible = min(
        contrast_ratio(
            reachable["Accessible High Contrast"].tokens[foreground],
            reachable["Accessible High Contrast"].tokens[background],
        )
        for foreground, background in pairs.values()
    )
    assert round(accessible, 2) >= (6.22 if minimum == 4.5 else 6.08)


@pytest.mark.asyncio
async def test_theme_picker_renders_five_rows_previews_and_escape_restores() -> None:
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
        assert HOMEBREW_NAME in rendered
        assert list(app.palette.row_texts) == [
            "> Refined Default",
            "  Dark Green Terminal",
            "  Neutral Dark",
            "  Accessible High Contrast",
            "  Homebrew — Talaria-designed green-black palette (v0.6.0) — "
            "not sampled from a host terminal, not the Homebrew package manager",
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


def test_homebrew_is_selectable_but_never_startup_selected() -> None:
    """Issue #123 U4: available by default, never the default."""
    assert DEFAULT_THEME_SLUG == "refined-default"

    selected = BUILTIN_THEME_REGISTRY.resolve("homebrew")
    assert selected.slug == "homebrew"
    assert selected.notices == ()

    assert BUILTIN_THEME_REGISTRY.resolve("not-installed").slug == "refined-default"
    assert BUILTIN_THEME_REGISTRY.resolve(7).slug == "refined-default"
    assert BUILTIN_THEME_REGISTRY.default.slug == "refined-default"


@pytest.mark.asyncio
async def test_startup_with_homebrew_configured_selects_homebrew() -> None:
    """Selectability, end to end: an operator who configures it gets it."""
    app, _ = paused_app([event("gateway.ready", {})], theme_name="homebrew")

    assert app.theme == "homebrew"
    assert app.session_theme_slug is None
    async with app.run_test(size=(80, 24)) as pilot:
        await pilot.pause()
        await app.shutdown_sources()


def test_resolve_layers_host_palette_between_default_and_overrides() -> None:
    """The single funnel order: defaults, then host, then groups, then tokens."""
    partial = ThemeSpec(
        slug="host-layered",
        name="Host Layered",
        dark=True,
        tokens={"talaria.text": "#EEEEEE"},
    )
    registry = ThemeRegistry((REFINED_DEFAULT, partial))

    resolved = registry.resolve(
        "host-layered",
        host_palette={
            "talaria.text": "#DDDDDD",
            "talaria.canvas": "#101010",
            "talaria.transcript.operator": "#333333",
        },
    )

    # The override beats the host for the text ...
    assert resolved.tokens["talaria.text"] == "#EEEEEE"
    # ... the host beats the default for the canvas ...
    assert resolved.tokens["talaria.canvas"] == "#101010"
    # ... and the host never reaches Talaria-semantic tokens.
    assert (
        resolved.tokens["talaria.transcript.operator"]
        == REFINED_DEFAULT.tokens["talaria.transcript.operator"]
    )
    assert any("inherited host terminal colors" in notice for notice in resolved.notices)


def test_host_values_that_break_readability_revert_with_a_notice() -> None:
    """Decision C: readability survives inheritance; overrides never revert."""
    partial = ThemeSpec(
        slug="host-unreadable",
        name="Host Unreadable",
        dark=True,
        tokens={},
    )
    registry = ThemeRegistry((REFINED_DEFAULT, partial))

    resolved = registry.resolve(
        "host-unreadable",
        host_palette={"talaria.text": "#F6F8FA", "talaria.canvas": "#F6F8FA"},
    )

    assert resolved.tokens["talaria.text"] == REFINED_DEFAULT.tokens["talaria.text"]
    assert resolved.tokens["talaria.canvas"] == REFINED_DEFAULT.tokens[
        "talaria.canvas"
    ]
    assert any("keeps the readability floor" in notice for notice in resolved.notices)


def test_an_explicit_override_survives_a_failing_host_pair() -> None:
    """Only host-adopted tokens revert — an override stands even unreadable."""
    partial = ThemeSpec(
        slug="host-override-stands",
        name="Host Override Stands",
        dark=True,
        tokens={"talaria.text": "#F6F8FA"},
    )
    registry = ThemeRegistry((REFINED_DEFAULT, partial))

    resolved = registry.resolve(
        "host-override-stands",
        host_palette={"talaria.canvas": "#F6F8FA"},
    )

    assert resolved.tokens["talaria.text"] == "#F6F8FA"


def test_an_unresolvable_host_palette_degrades_with_a_notice() -> None:
    """No palette, no crash, no blank theme — the built-in mapping stands."""
    partial = ThemeSpec(
        slug="host-missing",
        name="Host Missing",
        dark=True,
        tokens={},
    )
    registry = ThemeRegistry((REFINED_DEFAULT, partial))

    host: object
    for host in ("not-a-palette", {}):
        resolved = registry.resolve("host-missing", host_palette=host)

        assert dict(resolved.tokens) == dict(REFINED_DEFAULT.tokens)
        assert any("host terminal palette" in notice for notice in resolved.notices)


def test_no_host_argument_means_no_host_notices() -> None:
    """The host layer is opt-in: existing resolutions stay byte-identical."""
    partial = ThemeSpec(
        slug="no-host-layer",
        name="No Host Layer",
        dark=True,
        tokens={},
    )
    resolved = ThemeRegistry((REFINED_DEFAULT, partial)).resolve("no-host-layer")

    assert not any("host" in notice for notice in resolved.notices)


# ── issue #124: /theme select previews immediately, persisting explicitly ──


@pytest.mark.asyncio
async def test_theme_select_previews_immediately_without_persisting(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "user"
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
    )

    async with app.run_test(size=(80, 24)) as pilot:
        app.composer.text = "/theme select neutral-dark"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert app.theme == "neutral-dark"
        assert app.session_theme_slug == "neutral-dark"
        rendered = screen_text(app)
        assert "'neutral-dark' previewing for this session" in rendered
        assert not config_dir.exists(), "a preview wrote user config"
        assert not (tmp_path / ".talaria").exists(), (
            "a preview wrote repository config"
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_select_invalid_name_keeps_current_theme() -> None:
    app, _ = paused_app([event("gateway.ready", {})])

    async with app.run_test(size=(80, 24)) as pilot:
        app.composer.text = "/theme select not-installed"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert app.theme == "refined-default"
        assert app.session_theme_slug is None
        rendered = screen_text(app)
        assert "'not-installed' is not available" in rendered
        assert "keeping 'refined-default'" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_select_broken_spec_keeps_rendering_old_theme(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from talaria.ui.theme import ThemeRegistry

    original = ThemeRegistry.to_textual_theme

    def refuse_neutral_dark(
        self: ThemeRegistry, requested: object
    ) -> object:
        if requested == "neutral-dark":
            raise RuntimeError("synthetic render failure")
        return original(self, requested)

    app, _ = paused_app([event("gateway.ready", {})])
    # Patch after construction: startup registers every built-in theme, and
    # only the later select may fail.
    monkeypatch.setattr(
        ThemeRegistry, "to_textual_theme", refuse_neutral_dark
    )

    async with app.run_test(size=(80, 24)) as pilot:
        app.composer.text = "/theme select neutral-dark"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert app.theme == "refined-default"
        rendered = screen_text(app)
        assert "could not preview" in rendered
        assert "keeping 'refined-default'" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_select_then_save_persists_explicitly(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "user"
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
    )

    async with app.run_test(size=(80, 24)) as pilot:
        app.composer.text = "/theme select neutral-dark"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        app.composer.text = "/theme save"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()

        assert (config_dir / "config.toml").read_text(encoding="utf-8") == (
            '[theme]\nname = "neutral-dark"\n'
        )
        await app.shutdown_sources()
