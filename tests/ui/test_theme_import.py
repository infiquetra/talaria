"""Issue #105's bounded Visual Studio Code theme-import contract."""

from __future__ import annotations

import json
import os
from pathlib import Path

import pytest

from talaria.themes import THEME_TOKENS
from talaria.ui.theme import serialize_user_theme, theme_registry_for_config
from talaria.ui.theme_import import (
    ALWAYS_FALLBACK_TOKENS,
    ThemeImportError,
    import_vscode_theme,
    prepare_vscode_theme_import,
)

FIXTURES = Path(__file__).parents[1] / "fixtures" / "vscode-themes"
SAMPLE = FIXTURES / "sample-dark.json"


def _write_source(path: Path, payload: object) -> Path:
    path.write_text(json.dumps(payload), encoding="utf-8")
    return path


def test_representative_dark_theme_maps_exact_values_and_alpha_before_writing(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"

    prepared = prepare_vscode_theme_import(SAMPLE, config_dir=config_dir)

    assert not config_dir.exists(), "preparing the complete report changed the filesystem"
    assert prepared.target_name == "sample-dark"
    assert prepared.target_path == config_dir / "themes" / "sample-dark.json"
    assert prepared.theme.dark is True
    assert prepared.mapped_count == 40
    assert prepared.fallback_count == 18
    assert prepared.unsupported_count == 0
    assert prepared.mapped_values["talaria.surface"] == "#203040"
    assert prepared.mapped_values["talaria.error"] == "#FF6677"
    assert prepared.mapped_values["talaria.selection.background"] == "#881018"
    assert prepared.mapped_values["talaria.syntax.number"] == "#FFAA00"
    assert prepared.mapped_values["talaria.syntax.constant"] == "#CC77FF"
    assert prepared.mapped_values["talaria.syntax.keyword"] == "#ABCDEF"
    assert len(prepared.composites) == 1
    composite = prepared.composites[0]
    assert (
        composite.path,
        composite.token,
        composite.source,
        composite.background,
        composite.value,
    ) == (
        "colors.editor.selectionBackground",
        "talaria.selection.background",
        "#FF000080",
        "#102030",
        "#881018",
    )
    assert all(
        value.startswith("#")
        and len(value) == 7
        and value == value.upper()
        for value in prepared.theme.tokens.values()
    )


def test_exact_fourteen_extension_tokens_are_distinguished_from_other_fallbacks() -> None:
    report = prepare_vscode_theme_import(SAMPLE, config_dir=Path("unused"))

    assert len(ALWAYS_FALLBACK_TOKENS) == 14
    assert tuple(
        token for token in report.fallback_tokens if token in ALWAYS_FALLBACK_TOKENS
    ) == ALWAYS_FALLBACK_TOKENS
    assert tuple(
        token for token in report.fallback_tokens if token not in ALWAYS_FALLBACK_TOKENS
    ) == (
        "talaria.status.success",
        "talaria.status.warning",
        "talaria.status.error",
        "talaria.status.attention",
    )


def test_import_is_canonical_idempotent_and_loads_in_a_fresh_registry(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "user"

    first = import_vscode_theme(SAMPLE, config_dir=config_dir)
    first_bytes = first.target_path.read_bytes()
    second = import_vscode_theme(SAMPLE, config_dir=config_dir)

    assert second.target_path.read_bytes() == first_bytes
    assert first_bytes == serialize_user_theme(first.theme)
    assert first_bytes.endswith(b"\n") and not first_bytes.endswith(b"\n\n")
    payload = json.loads(first_bytes)
    assert tuple(payload) == ("dark", "name", "slug", "tokens")
    assert tuple(payload["tokens"]) == tuple(sorted(THEME_TOKENS))
    assert list((config_dir / "themes").iterdir()) == [first.target_path]

    restarted = theme_registry_for_config(config_dir=config_dir)
    resolved = restarted.resolve("sample-dark")
    assert resolved.slug == "sample-dark"
    assert resolved.filled_tokens == ()
    assert dict(resolved.tokens) == dict(first.theme.tokens)
    assert restarted.resolve("not-installed").slug == "refined-default"


def test_unsupported_fixture_reports_every_occurrence_and_exact_counts(
    tmp_path: Path,
) -> None:
    report = import_vscode_theme(
        FIXTURES / "unsupported-dark.json", config_dir=tmp_path
    )

    assert report.mapped_count == 2
    assert report.fallback_count == 56
    assert report.unsupported_count == 19
    assert report.theme.dark is False
    assert report.unsupported_entries == (
        "root.include is unsupported; external theme files are not read",
        "root.semanticHighlighting is unsupported; semantic-token theming is not imported",
        "root.semanticTokenColors is unsupported; semantic-token theming is not imported",
        "root.productIconTheme is unsupported",
        "root.unknownRoot is unsupported",
        "root.type is invalid; expected light, dark, or hc and used Refined Default",
        "colors.input.background is invalid; expected #RGB, #RGBA, #RRGGBB, or #RRGGBBAA",
        "colors.editorCursor.foreground is unsupported",
        "colors.terminal.ansiBlack is unsupported",
        "tokenColors[0] is an unsupported unscoped default",
        "tokenColors[1].scope selector 'source.python keyword' is unsupported",
        "tokenColors[2].settings.background is unsupported and was ignored",
        "tokenColors[2].settings.fontStyle is unsupported and was ignored",
        "tokenColors[2].settings.strikethrough is unsupported",
        "tokenColors[2].scope selector 'markup.bold' is unsupported",
        "tokenColors[3].scope[1] is invalid",
        "tokenColors[4].name is unsupported",
        "tokenColors[4].scope selector 'keyword*' is unsupported",
        "tokenColors[5] is invalid; expected an object",
    )
    lines = report.lines()
    assert lines[0] == (
        "Imported warnings-dark as user theme warnings-dark: "
        "2 source tokens, 56 fallbacks, 19 warnings."
    )
    assert sum(line.startswith("warning: ") for line in lines) == 19
    assert sum(line.startswith("fallback: ") for line in lines) == 56


@pytest.mark.parametrize(
    ("literal", "expected"),
    [
        ("#ABC", "#AABBCC"),
        ("#ABCD", "#B4C3D2"),
        ("#AABBCC", "#AABBCC"),
        ("#AABBCCDD", "#B4C3D2"),
    ],
)
def test_all_four_documented_hex_forms_normalize_to_opaque_uppercase(
    tmp_path: Path, literal: str, expected: str
) -> None:
    source = _write_source(
        tmp_path / "hex-theme.json",
        {"colors": {"editor.background": literal}},
    )

    report = prepare_vscode_theme_import(source, config_dir=tmp_path / "config")

    assert report.mapped_values["talaria.canvas"] == expected


@pytest.mark.parametrize(
    ("fixture", "message"),
    [
        ("malformed.json", "not strict JSON"),
        ("wrong-root.json", "root must be a JSON object"),
        ("empty.json", "is empty"),
        ("wrong-schema.json", "root.colors must be an object"),
    ],
)
def test_malformed_wrong_root_schema_and_empty_input_write_nothing(
    tmp_path: Path, fixture: str, message: str
) -> None:
    config_dir = tmp_path / "config"

    with pytest.raises(ThemeImportError, match=message):
        import_vscode_theme(FIXTURES / fixture, config_dir=config_dir)

    assert not config_dir.exists()


@pytest.mark.parametrize(
    "literal",
    ["#12", "red", 7, None, "#GGG", "#AABBCCDDEE"],
)
def test_invalid_colors_cannot_create_a_theme_by_falling_back_everything(
    tmp_path: Path, literal: object
) -> None:
    source = _write_source(
        tmp_path / "invalid-color.json",
        {"colors": {"editor.background": literal}},
    )
    config_dir = tmp_path / "config"

    with pytest.raises(ThemeImportError, match="no usable supported"):
        import_vscode_theme(source, config_dir=config_dir)

    assert not config_dir.exists()


def test_first_valid_candidate_wins_and_invalid_first_candidate_falls_through(
    tmp_path: Path,
) -> None:
    source = _write_source(
        tmp_path / "candidate-theme.json",
        {
            "colors": {
                "editorWidget.background": "invalid",
                "input.background": "#234",
                "activityBarBadge.background": "#123456",
                "button.background": "#FFFFFF",
            }
        },
    )

    report = prepare_vscode_theme_import(source, config_dir=tmp_path / "config")

    assert report.mapped_values["talaria.surface"] == "#223344"
    assert report.mapped_values["talaria.accent"] == "#123456"
    assert report.unsupported_entries == (
        "colors.editorWidget.background is invalid; expected #RGB, #RGBA, #RRGGBB, or #RRGGBBAA",
    )


def test_name_precedence_is_override_then_source_name_then_file_stem(
    tmp_path: Path,
) -> None:
    named = _write_source(
        tmp_path / "file-name.json",
        {"name": "source-name", "colors": {"editor.background": "#123456"}},
    )
    unnamed = _write_source(
        tmp_path / "stem-name.json",
        {"colors": {"editor.background": "#123456"}},
    )

    assert prepare_vscode_theme_import(named, name="override-name").target_name == (
        "override-name"
    )
    assert prepare_vscode_theme_import(named).target_name == "source-name"
    assert prepare_vscode_theme_import(unnamed).target_name == "stem-name"


@pytest.mark.parametrize(
    "name",
    ["../escape", "two words", "Uppercase", "ends-", "refined-default"],
)
def test_invalid_unsafe_and_builtin_names_never_construct_a_target(
    tmp_path: Path, name: str
) -> None:
    config_dir = tmp_path / "config"

    with pytest.raises(ThemeImportError):
        import_vscode_theme(SAMPLE, name=name, config_dir=config_dir)

    assert not config_dir.exists()
    assert not (tmp_path / "escape.json").exists()


def test_external_token_colors_warns_but_a_valid_workbench_mapping_imports(
    tmp_path: Path,
) -> None:
    source = _write_source(
        tmp_path / "external-textmate.json",
        {
            "colors": {"editor.background": "#123456"},
            "tokenColors": "./syntax.tmTheme",
        },
    )

    report = prepare_vscode_theme_import(source, config_dir=tmp_path / "config")

    assert report.mapped_count == 1
    assert report.unsupported_entries == (
        "root.tokenColors is unsupported; external TextMate files are not read",
    )


def test_atomic_replace_failure_keeps_the_original_and_removes_the_temp(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    config_dir = tmp_path / "config"
    target = config_dir / "themes" / "sample-dark.json"
    target.parent.mkdir(parents=True)
    original = b'{"original": true}\n'
    target.write_bytes(original)

    def refuse_replace(source: object, destination: object) -> None:
        del source, destination
        raise OSError("replace refused")

    monkeypatch.setattr(os, "replace", refuse_replace)

    with pytest.raises(ThemeImportError, match="replace refused"):
        import_vscode_theme(SAMPLE, config_dir=config_dir)

    assert target.read_bytes() == original
    assert list(target.parent.glob(".sample-dark.json.*")) == []


def test_user_theme_loader_rejects_noncanonical_manual_files(tmp_path: Path) -> None:
    themes = tmp_path / "themes"
    themes.mkdir()
    (themes / "manual.json").write_text('{"slug":"manual"}\n', encoding="utf-8")

    with pytest.raises(ValueError, match="fields are not canonical"):
        theme_registry_for_config(config_dir=tmp_path)


def test_import_never_reads_an_include_path(tmp_path: Path) -> None:
    missing = tmp_path / "must-not-be-read.json"
    source = _write_source(
        tmp_path / "bounded-theme.json",
        {
            "include": str(missing),
            "colors": {"editor.background": "#123456"},
        },
    )

    report = import_vscode_theme(source, config_dir=tmp_path / "config")

    assert not missing.exists()
    assert report.unsupported_entries == (
        "root.include is unsupported; external theme files are not read",
    )
