"""Issue #105's bounded Visual Studio Code theme-import contract."""

from __future__ import annotations

import argparse
import json
import os
import re
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator
from jsonschema.exceptions import ValidationError

from talaria.cli import build_parser
from talaria.config import load_config
from talaria.themes import THEME_TOKENS
from talaria.themes.builtins import BUILTIN_THEMES
from talaria.themes.storage import StoredThemeError, load_user_theme_spec
from talaria.ui.theme import serialize_user_theme, theme_registry_for_config
from talaria.ui.theme_import import (
    ALWAYS_FALLBACK_TOKENS,
    SYNTAX_MAPPINGS,
    WORKBENCH_MAPPINGS,
    ThemeImportError,
    import_vscode_theme,
    prepare_vscode_theme_import,
)

FIXTURES = Path(__file__).parents[1] / "fixtures" / "vscode-themes"
SAMPLE = FIXTURES / "sample-dark.json"
REPO_ROOT = Path(__file__).parents[2]
FORMAT_DOCUMENT = REPO_ROOT / "docs/formats/vscode-theme-import.md"
STORED_THEME_SCHEMA = REPO_ROOT / "docs/formats/stored-theme.schema.json"
THEMES_GUIDE = REPO_ROOT / "docs/themes.md"
VISUAL_SPECIFICATION = (
    REPO_ROOT / "docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md"
)


def _write_source(path: Path, payload: object) -> Path:
    path.write_text(json.dumps(payload), encoding="utf-8")
    return path


def _markdown_table(document: str, title: str) -> str:
    lines = document.splitlines(keepends=True)
    heading = next(
        index
        for index, line in enumerate(lines)
        if line.lstrip("#").strip() == title
    )
    start = next(
        index for index in range(heading + 1, len(lines)) if lines[index].startswith("|")
    )
    end = start
    while end < len(lines) and lines[end].startswith("|"):
        end += 1
    return "".join(lines[start:end])


def _markdown_rows(table: str) -> tuple[tuple[str, ...], ...]:
    return tuple(
        tuple(cell.strip() for cell in line.strip().strip("|").split("|"))
        for line in table.splitlines()[2:]
    )


def _stored_theme_validator() -> Draft202012Validator:
    schema = json.loads(STORED_THEME_SCHEMA.read_text(encoding="utf-8"))
    Draft202012Validator.check_schema(schema)
    return Draft202012Validator(schema)


def _subparser(
    parser: argparse.ArgumentParser, name: str
) -> argparse.ArgumentParser:
    for action in parser._actions:  # noqa: SLF001 - argparse has no public accessor
        if isinstance(action, argparse._SubParsersAction):  # noqa: SLF001
            candidate = action.choices[name]
            assert isinstance(candidate, argparse.ArgumentParser)
            return candidate
    raise AssertionError(f"{parser.prog} defines no subparser named {name!r}")


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


def test_public_mapping_tables_match_code_and_the_visual_specification() -> None:
    format_document = FORMAT_DOCUMENT.read_text(encoding="utf-8")
    visual_specification = VISUAL_SPECIFICATION.read_text(encoding="utf-8")
    titles = (
        "Supported workbench colors",
        "Supported tokenColors scopes",
        "Tokens with no Visual Studio Code source",
    )
    format_tables = tuple(
        _markdown_table(format_document, title) for title in titles
    )
    specification_tables = tuple(
        _markdown_table(visual_specification, title) for title in titles
    )

    assert format_tables == specification_tables

    workbench_rows = _markdown_rows(format_tables[0])
    assert tuple((row[0], tuple(row[1].split("; "))) for row in workbench_rows) == tuple(
        (mapping.token, mapping.candidates) for mapping in WORKBENCH_MAPPINGS
    )

    syntax_rows = _markdown_rows(format_tables[1])
    assert tuple((row[0], tuple(row[1].split("; "))) for row in syntax_rows) == tuple(
        (mapping.token, mapping.prefixes) for mapping in SYNTAX_MAPPINGS
    )

    mapped_tokens = {mapping.token for mapping in WORKBENCH_MAPPINGS} | {
        mapping.token for mapping in SYNTAX_MAPPINGS
    }
    expected_fallbacks = tuple(
        token for token in THEME_TOKENS if token not in mapped_tokens
    )
    fallback_rows = _markdown_rows(format_tables[2])
    assert tuple(row[0] for row in fallback_rows) == expected_fallbacks


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
    assert tuple(payload) == ("dark", "groups", "name", "schema_version", "slug", "tokens")
    assert payload["schema_version"] == "talaria-theme-v1"
    assert tuple(payload["tokens"]) == tuple(sorted(THEME_TOKENS))
    assert list((config_dir / "themes").iterdir()) == [first.target_path]

    restarted = theme_registry_for_config(config_dir=config_dir)
    resolved = restarted.resolve("sample-dark")
    assert resolved.slug == "sample-dark"
    assert resolved.filled_tokens == ()
    assert dict(resolved.tokens) == dict(first.theme.tokens)
    assert restarted.resolve("not-installed").slug == "refined-default"


def test_stored_theme_reader_accepts_versioned_and_legacy_version_one(
    tmp_path: Path,
) -> None:
    report = import_vscode_theme(SAMPLE, config_dir=tmp_path)
    versioned = json.loads(report.target_path.read_text(encoding="utf-8"))
    assert versioned["schema_version"] == "talaria-theme-v1"
    assert theme_registry_for_config(config_dir=tmp_path).resolve(
        "sample-dark"
    ).slug == "sample-dark"

    versioned.pop("schema_version")
    report.target_path.write_text(
        json.dumps(versioned, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    assert theme_registry_for_config(config_dir=tmp_path).resolve(
        "sample-dark"
    ).slug == "sample-dark"


def test_unknown_stored_theme_version_is_skipped_with_a_notice(tmp_path: Path) -> None:
    from talaria.themes.storage import load_user_theme_specs

    report = import_vscode_theme(SAMPLE, config_dir=tmp_path)
    payload = json.loads(report.target_path.read_text(encoding="utf-8"))
    payload["schema_version"] = "talaria-theme-v999"
    report.target_path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    specs, notices = load_user_theme_specs(config_dir=tmp_path)

    assert specs == ()
    assert len(notices) == 1
    assert str(report.target_path) in notices[0]
    assert "talaria-theme-v999" in notices[0]
    assert "skipped" in notices[0]


def test_stored_theme_schema_and_loader_share_version_compatibility(
    tmp_path: Path,
) -> None:
    report = import_vscode_theme(SAMPLE, config_dir=tmp_path)
    document = json.loads(report.target_path.read_text(encoding="utf-8"))
    validator = _stored_theme_validator()

    validator.validate(document)

    legacy = dict(document)
    legacy.pop("schema_version")
    validator.validate(legacy)
    report.target_path.write_text(
        json.dumps(legacy, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    assert load_user_theme_spec(report.target_path).slug == report.theme.slug

    wrong_version = dict(legacy)
    wrong_version["schema_version"] = "talaria-theme-v999"
    with pytest.raises(ValidationError):
        validator.validate(wrong_version)

    whitespace_name = dict(legacy)
    whitespace_name["name"] = " \u00a0 "
    with pytest.raises(ValidationError):
        validator.validate(whitespace_name)


@pytest.mark.parametrize(
    "invalid_class",
    ("whitespace-name", "built-in-slug", "filename-slug-mismatch"),
)
def test_published_stored_theme_contract_matches_loader_rejections(
    tmp_path: Path, invalid_class: str
) -> None:
    report = import_vscode_theme(SAMPLE, name="contract-theme", config_dir=tmp_path)
    document = json.loads(report.target_path.read_text(encoding="utf-8"))
    path = report.target_path

    if invalid_class == "whitespace-name":
        document["name"] = " \u00a0 "
    elif invalid_class == "built-in-slug":
        document["slug"] = "refined-default"
        path = path.with_name("refined-default.json")
    else:
        path = path.with_name("different-filename.json")
    path.write_text(json.dumps(document), encoding="utf-8")

    validator = _stored_theme_validator()
    schema = validator.schema
    assert isinstance(schema, dict)
    description = schema.get("description", "")
    assert isinstance(description, str)
    assert "validity is necessary but not sufficient" in description
    assert "talaria/themes/storage.py" in description
    assert "filename stem" in description
    assert "built-in theme" in description

    built_in_slugs = frozenset(theme.slug for theme in BUILTIN_THEMES)
    published_contract_accepts = (
        validator.is_valid(document)
        and path.stem == document["slug"]
        and document["slug"] not in built_in_slugs
    )
    try:
        load_user_theme_spec(path)
    except StoredThemeError:
        loader_accepts = False
    else:
        loader_accepts = True

    assert published_contract_accepts is loader_accepts
    assert not loader_accepts


def test_theme_import_guide_synopsis_includes_every_parser_option() -> None:
    import_parser = _subparser(_subparser(build_parser(), "theme"), "import")
    expected_options = {
        option
        for action in import_parser._actions  # noqa: SLF001 - no public accessor
        if action.dest != "help"
        for option in action.option_strings
    }
    guide = THEMES_GUIDE.read_text(encoding="utf-8")
    match = re.search(r"(?m)^talaria theme import FILE[^\n]*$", guide)

    assert match is not None
    synopsis = match.group(0)
    assert expected_options
    assert all(option in synopsis for option in expected_options), (
        expected_options,
        synopsis,
    )


def test_imported_theme_survives_restart_configuration_validation(
    isolated_global_config_dir: Path,
) -> None:
    import_vscode_theme(
        SAMPLE,
        name="probe-theme",
        config_dir=isolated_global_config_dir,
    )
    (isolated_global_config_dir / "config.toml").write_text(
        '[theme]\nname = "probe-theme"\n',
        encoding="utf-8",
    )

    cfg = load_config()

    assert cfg.get("theme", "name") == "probe-theme"
    assert cfg.notices == ()
    restarted = theme_registry_for_config(config_dir=cfg.config_dir)
    assert restarted.resolve(cfg.get("theme", "name")).slug == "probe-theme"


def test_unknown_theme_still_falls_back_with_visible_notice(
    isolated_global_config_dir: Path,
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[theme]\nname = "not-installed"\n',
        encoding="utf-8",
    )

    cfg = load_config()

    assert cfg.get("theme", "name") == "refined-default"
    assert cfg.notices == (
        "theme 'not-installed' is not available; "
        "using Refined Default (refined-default)",
    )


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
    ("fixture", "message", "kind"),
    [
        ("does-not-exist.json", "could not be read", "unreadable"),
        ("malformed.json", "not strict JSON", "malformed"),
        ("wrong-root.json", "root must be a JSON object", "wrong-root"),
        ("empty.json", "is empty", "empty"),
        ("wrong-schema.json", "root.colors must be an object", "wrong-root"),
    ],
)
def test_malformed_wrong_root_schema_and_empty_input_write_nothing(
    tmp_path: Path,
    fixture: str,
    message: str,
    kind: str,
) -> None:
    config_dir = tmp_path / "config"

    with pytest.raises(ThemeImportError, match=message) as excinfo:
        import_vscode_theme(FIXTURES / fixture, config_dir=config_dir)

    assert excinfo.value.kind == kind
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
    ("name", "kind"),
    [
        ("../escape", "invalid-slug"),
        ("two words", "invalid-slug"),
        ("Uppercase", "invalid-slug"),
        ("ends-", "invalid-slug"),
        ("refined-default", "reserved-slug"),
    ],
)
def test_invalid_unsafe_and_builtin_names_never_construct_a_target(
    tmp_path: Path, name: str, kind: str
) -> None:
    config_dir = tmp_path / "config"

    with pytest.raises(ThemeImportError) as excinfo:
        import_vscode_theme(SAMPLE, name=name, config_dir=config_dir)

    assert excinfo.value.kind == kind
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

    with pytest.raises(ThemeImportError, match="replace refused") as excinfo:
        import_vscode_theme(SAMPLE, config_dir=config_dir)

    assert excinfo.value.kind == "unwritable"
    assert target.read_bytes() == original
    assert list(target.parent.glob(".sample-dark.json.*")) == []


def test_user_theme_loader_rejects_noncanonical_manual_files(tmp_path: Path) -> None:
    from talaria.themes.storage import load_user_theme_spec

    themes = tmp_path / "themes"
    themes.mkdir()
    manual = themes / "manual.json"
    manual.write_text('{"slug":"manual"}\n', encoding="utf-8")

    with pytest.raises(ValueError, match="fields are not canonical"):
        load_user_theme_spec(manual)


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
