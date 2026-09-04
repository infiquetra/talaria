"""Issue #105's bounded Visual Studio Code theme-import contract."""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import re
import threading
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator
from jsonschema.exceptions import ValidationError
from textual.color import Color

import talaria.cli
from talaria.cli import build_parser, main
from talaria.config import load_config
from talaria.themes import THEME_TOKENS
from talaria.themes.builtins import BUILTIN_THEMES
from talaria.themes.marketplace import (
    MAX_MARKETPLACE_BYTES,
    MarketplaceEntry,
    MarketplaceError,
    convert_page_url,
    fetch_marketplace_bytes,
    resolve_marketplace_source,
    search_marketplace,
    slugify_theme_label,
)
from talaria.themes.sources import load_import_sources
from talaria.themes.storage import StoredThemeError, load_user_theme_spec
from talaria.ui.theme import serialize_user_theme, theme_registry_for_config
from talaria.ui.theme_import import (
    ALWAYS_FALLBACK_TOKENS,
    SYNTAX_MAPPINGS,
    WORKBENCH_MAPPINGS,
    ImportReport,
    ThemeImportError,
    import_marketplace_theme,
    import_vscode_theme,
    prepare_marketplace_import,
    prepare_vscode_theme_import,
    prepare_vscode_theme_import_bytes,
    reload_imported_theme,
)
from tests.ui.conftest import event, paused_app, screen_text

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


# ── issue #124: marketplace import and live reload ────────────────────────

SAMPLE_BYTES = SAMPLE.read_bytes()


def _marketplace_entry(
    *,
    source_id: str = "acme/solar/1",
    publisher: str = "acme",
    extension: str = "solar",
    theme_label: str = "Solar Flare",
    description: str = "A synthetic marketplace theme",
    download_url: str = "https://example.invalid/themes/solar-flare.json",
) -> MarketplaceEntry:
    return MarketplaceEntry(
        source_id=source_id,
        publisher=publisher,
        extension=extension,
        theme_label=theme_label,
        description=description,
        download_url=download_url,
    )


class _FakeMarketplaceTransport:
    """In-memory marketplace double: no network, no manual download step."""

    def __init__(
        self,
        entries: tuple[MarketplaceEntry, ...] = (),
        files: dict[str, bytes] | None = None,
    ) -> None:
        self._entries = tuple(entries)
        self._files = dict(files or {})
        self.search_calls: list[tuple[str, int]] = []
        self.lookup_calls: list[tuple[str, str]] = []
        self.fetch_calls: list[str] = []
        self.search_error: Exception | None = None
        self.fetch_error: Exception | None = None
        self.fetch_gate: threading.Event | None = None
        self._guard = threading.Lock()
        self._active = 0
        self.max_active = 0

    def search(
        self, query: str, *, limit: int
    ) -> tuple[MarketplaceEntry, ...]:
        self.search_calls.append((query, limit))
        if self.search_error is not None:
            raise self.search_error
        lowered = query.strip().lower()
        matched = tuple(
            entry
            for entry in self._entries
            if lowered
            in f"{entry.theme_label} {entry.publisher} "
            f"{entry.extension} {entry.description}".lower()
        )
        return matched[:limit]

    def lookup(
        self, publisher: str, extension: str
    ) -> tuple[MarketplaceEntry, ...]:
        self.lookup_calls.append((publisher, extension))
        return tuple(
            entry
            for entry in self._entries
            if entry.publisher == publisher and entry.extension == extension
        )

    def fetch_bytes(self, entry: MarketplaceEntry) -> bytes:
        self.fetch_calls.append(entry.source_id)
        if self.fetch_gate is not None:
            with self._guard:
                self._active += 1
                self.max_active = max(self.max_active, self._active)
            try:
                assert self.fetch_gate.wait(timeout=10)
            finally:
                with self._guard:
                    self._active -= 1
        if self.fetch_error is not None:
            raise self.fetch_error
        return self._files[entry.source_id]


def _solar_transport() -> _FakeMarketplaceTransport:
    entry = _marketplace_entry()
    return _FakeMarketplaceTransport(
        entries=(entry,), files={entry.source_id: SAMPLE_BYTES}
    )


def test_marketplace_bytes_parse_identically_to_the_same_file_on_disk(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    entry = _marketplace_entry()

    from_bytes = prepare_marketplace_import(
        SAMPLE_BYTES, entry, name="solar-flare", config_dir=config_dir
    )
    from_disk = prepare_vscode_theme_import(
        SAMPLE, name="solar-flare", config_dir=config_dir
    )

    assert from_bytes.mapped_values == from_disk.mapped_values
    assert from_bytes.fallback_tokens == from_disk.fallback_tokens
    assert from_bytes.unsupported_entries == from_disk.unsupported_entries
    assert dict(from_bytes.theme.tokens) == dict(from_disk.theme.tokens)
    assert from_bytes.theme.dark is from_disk.theme.dark
    assert from_bytes.target_path == from_disk.target_path
    assert not config_dir.exists(), "preparing either report changed the filesystem"


@pytest.mark.parametrize(
    ("data", "message"),
    [
        (b"{oops", "is not strict JSON"),
        (b"   ", "is empty"),
        (b"[1, 2]", "root must be a JSON object"),
        (b'{"colors": {"editor.background": "#12"}}', "no usable supported"),
    ],
)
def test_marketplace_bytes_fail_with_file_identical_strictness(
    tmp_path: Path, data: bytes, message: str
) -> None:
    entry = _marketplace_entry()

    with pytest.raises(ThemeImportError, match=re.escape(message)) as from_bytes:
        prepare_marketplace_import(data, entry, config_dir=tmp_path / "config")

    disk_source = tmp_path / "same-bytes.json"
    disk_source.write_bytes(data)
    with pytest.raises(ThemeImportError, match=re.escape(message)) as from_disk:
        prepare_vscode_theme_import(disk_source, name="same-bytes")

    assert from_bytes.value.kind == from_disk.value.kind


def test_marketplace_search_select_fetch_round_trip_without_manual_download(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    transport = _solar_transport()

    found = search_marketplace("solar", transport=transport)
    assert [entry.source_id for entry in found] == ["acme/solar/1"]

    selected = resolve_marketplace_source("acme/solar", transport=transport)
    assert selected.source_id == "acme/solar/1"

    fetched = fetch_marketplace_bytes(selected, transport=transport)
    assert fetched == SAMPLE_BYTES

    report = import_marketplace_theme(
        selected, transport=transport, name="solar-flare", config_dir=config_dir
    )

    assert report.target_path == config_dir / "themes" / "solar-flare.json"
    assert report.target_path.is_file()
    # No manual download step: the only new files are the stored theme and
    # its recorded source; nothing was staged by hand.
    assert sorted(
        path.relative_to(config_dir).as_posix()
        for path in config_dir.rglob("*")
        if path.is_file()
    ) == ["theme-sources.json", "themes/solar-flare.json"]
    sources, notices = load_import_sources(config_dir=config_dir)
    assert notices == ()
    assert sources["solar-flare"].kind == "marketplace"
    assert sources["solar-flare"].ref == "acme/solar/1"


def test_marketplace_reference_forms_resolve_or_reject() -> None:
    first = _marketplace_entry()
    second = _marketplace_entry(
        source_id="acme/solar/2",
        theme_label="Solar Ember",
        download_url="https://example.invalid/themes/solar-ember.json",
    )
    transport = _FakeMarketplaceTransport(entries=(first, second))

    assert (
        resolve_marketplace_source("acme/solar/2", transport=transport).theme_label
        == "Solar Ember"
    )
    assert (
        resolve_marketplace_source(
            "acme/solar/Solar Ember", transport=transport
        ).theme_label
        == "Solar Ember"
    )
    direct = resolve_marketplace_source(
        "https://example.invalid/themes/solar-flare.json", transport=transport
    )
    assert direct.download_url == "https://example.invalid/themes/solar-flare.json"

    with pytest.raises(MarketplaceError) as unknown:
        resolve_marketplace_source("nope/nothing", transport=transport)
    assert unknown.value.kind == "unknown-source"

    with pytest.raises(MarketplaceError) as ambiguous:
        resolve_marketplace_source("acme/solar", transport=transport)
    assert ambiguous.value.kind == "ambiguous-source"
    assert "Solar Flare" in str(ambiguous.value)

    with pytest.raises(MarketplaceError) as bad_scheme:
        resolve_marketplace_source("ftp://example.invalid/theme.json", transport=transport)
    assert bad_scheme.value.kind == "unknown-source"


def test_github_file_page_url_converts_to_the_raw_file() -> None:
    assert convert_page_url(
        "https://github.com/acme/themes/blob/main/solar.json"
    ) == "https://raw.githubusercontent.com/acme/themes/main/solar.json"
    assert convert_page_url(
        "https://github.com/acme/themes/raw/v1.2/themes/solar.json"
    ) == "https://raw.githubusercontent.com/acme/themes/v1.2/themes/solar.json"
    assert convert_page_url("https://example.invalid/themes/solar.json") is None
    assert convert_page_url("acme/solar") is None
    # Registry file URLs are already direct: no conversion, no gallery block.
    api_url = "https://open-vsx.org/api/acme/solar/1.0/file/solar.json"
    assert convert_page_url(api_url) is None
    resolved = resolve_marketplace_source(
        api_url, transport=_FakeMarketplaceTransport(entries=())
    )
    assert resolved.download_url == api_url


def test_case_varied_page_urls_convert_on_scheme_and_host_only() -> None:
    assert convert_page_url(
        "HTTPS://GITHUB.COM/acme/themes/blob/main/solar.json"
    ) == "https://raw.githubusercontent.com/acme/themes/main/solar.json"
    assert convert_page_url(
        "https://Open-VSX.Org/extension/acme/solar"
    ) == "acme/solar"
    # Path shapes stay exact: an uppercased page kind is not a file page.
    assert (
        convert_page_url("https://github.com/acme/themes/BLOB/main/solar.json")
        is None
    )
    entry = _marketplace_entry()
    transport = _FakeMarketplaceTransport(entries=(entry,))
    resolved = resolve_marketplace_source(
        "HTTPS://Open-VSX.Org/extension/acme/solar", transport=transport
    )
    assert transport.lookup_calls == [("acme", "solar")]
    assert resolved.source_id == "acme/solar/1"


@pytest.mark.asyncio
async def test_theme_fetch_renders_appearance_lead_and_full_report(
    tmp_path: Path,
) -> None:
    """The full report must be rendered, not just stored.

    The composer notice is one ellipsized row, so this test sweeps the
    scrollable transcript and asserts the confirmation, the appearance
    lead, a fallback line, and a warning line each RENDER on screen, in
    that order. Backing-store text alone proves nothing about visibility.
    """
    config_dir = tmp_path / "config"
    raw_url = "https://example.invalid/warnings-dark.json"
    payload = (FIXTURES / "unsupported-dark.json").read_bytes()
    transport = _FakeMarketplaceTransport(entries=(), files={raw_url: payload})
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
        marketplace_transport=transport,
    )
    # Short line-start fragments: transcript rows wrap, so only a fragment
    # at the start of its row is guaranteed contiguous on screen.
    markers = {
        "confirmation": "theme 'warnings-dark' fetched:",
        "counts": "2 source tokens, 56 fallbacks,",
        "lead": "Appearance: 56 tokens",
        "fallback": "fallback: talaria.surface",
        "warning": "warning: root.include",
    }
    async with app.run_test(size=(80, 24)) as pilot:
        await _submit_theme_command(
            app, pilot, f"/theme fetch {raw_url} --name warnings-dark"
        )
        assert app.theme == "warnings-dark"
        assert app.session_theme_slug == "warnings-dark"
        pane = app.transcript
        bottom = int(pane.max_scroll_y)
        positions = sorted(set([0] + list(range(0, bottom + 1, 8)) + [bottom]))
        seen: dict[str, int] = {}
        for position in positions:
            pane.scroll_to(y=position, animate=False, immediate=True)
            await pilot.pause()
            frame = screen_text(app)
            for key, fragment in markers.items():
                if key not in seen and fragment in frame:
                    seen[key] = position
        assert set(seen) == set(markers), (
            "report lines never rendered",
            sorted(set(markers) - set(seen)),
        )
        assert (
            seen["confirmation"]
            <= seen["lead"]
            <= seen["fallback"]
            <= seen["warning"]
        ), seen
        await app.shutdown_sources()


def test_open_vsx_extension_page_resolves_as_its_registry_reference() -> None:
    entry = _marketplace_entry()
    transport = _FakeMarketplaceTransport(entries=(entry,))

    resolved = resolve_marketplace_source(
        "https://open-vsx.org/extension/acme/solar", transport=transport
    )

    assert transport.lookup_calls == [("acme", "solar")]
    assert resolved.source_id == "acme/solar/1"


def test_gallery_search_page_yields_supported_forms_not_a_size_error() -> None:
    transport = _FakeMarketplaceTransport(entries=())

    for page_url in (
        "https://open-vsx.org/?search=solar",
        "https://marketplace.visualstudio.com/items?itemName=acme.solar",
    ):
        with pytest.raises(MarketplaceError) as page:
            resolve_marketplace_source(page_url, transport=transport)

        assert page.value.kind == "unknown-source"
        message = str(page.value)
        assert "gallery web page" in message
        assert "publisher/extension" in message
        assert "raw theme" in message
    assert transport.lookup_calls == []
    assert transport.fetch_calls == []


def test_file_page_url_never_reports_a_small_theme_as_oversized(
    tmp_path: Path,
) -> None:
    """The reported failure mode: a blob page URL for a small theme file."""
    config_dir = tmp_path / "config"
    raw_url = "https://raw.githubusercontent.com/acme/themes/main/solar.json"
    transport = _FakeMarketplaceTransport(
        entries=(), files={raw_url: SAMPLE_BYTES}
    )

    resolved = resolve_marketplace_source(
        "https://github.com/acme/themes/blob/main/solar.json",
        transport=transport,
    )
    assert resolved.download_url == raw_url

    report = import_marketplace_theme(
        resolved, transport=transport, name="solar-flare", config_dir=config_dir
    )
    assert report.mapped_count == 40
    assert report.fallback_count == 18


def test_oversized_notice_distinguishes_pages_from_raw_files() -> None:
    entry = _marketplace_entry()
    oversized = _FakeMarketplaceTransport(
        entries=(entry,),
        files={entry.source_id: b"x" * (MAX_MARKETPLACE_BYTES + 1)},
    )

    with pytest.raises(MarketplaceError) as too_big:
        fetch_marketplace_bytes(entry, transport=oversized)

    assert too_big.value.kind == "oversized"
    assert "raw theme file" in str(too_big.value)


def test_import_summary_leads_with_appearance_changing_fallbacks(
    tmp_path: Path,
) -> None:
    report = import_vscode_theme(
        FIXTURES / "unsupported-dark.json", config_dir=tmp_path
    )
    lines = report.lines()

    assert lines[0] == (
        "Imported warnings-dark as user theme warnings-dark: "
        "2 source tokens, 56 fallbacks, 19 warnings."
    )
    assert lines[1].startswith("Appearance: 56 tokens use Refined Default")
    assert "19 source features were not imported" in lines[1]
    fallback_first = next(
        index for index, line in enumerate(lines) if line.startswith("fallback: ")
    )
    warning_first = next(
        index for index, line in enumerate(lines) if line.startswith("warning: ")
    )
    assert fallback_first < warning_first
    assert sum(line.startswith("warning: ") for line in lines) == 19
    assert sum(line.startswith("fallback: ") for line in lines) == 56


def test_import_summary_with_no_fallbacks_or_warnings_states_it_plainly(
    tmp_path: Path,
) -> None:
    from talaria.themes import ThemeSpec

    report = ImportReport(
        mapped_values={"talaria.canvas": "#000000"},
        unsupported_entries=(),
        fallback_tokens=(),
        composites=(),
        target_name="plain",
        target_path=tmp_path / "plain.json",
        theme=ThemeSpec(slug="plain", name="Plain", dark=True, tokens={}),
    )

    assert report.lines()[1] == (
        "Appearance: every token renders as authored; nothing fell back "
        "and nothing was ignored."
    )


def test_marketplace_network_oversize_and_malformed_rejections_carry_kinds(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    network = _solar_transport()
    network.fetch_error = MarketplaceError("dial refused", kind="network")
    entry = _marketplace_entry()

    with pytest.raises(MarketplaceError) as network_error:
        fetch_marketplace_bytes(entry, transport=network)
    assert network_error.value.kind == "network"

    oversized = _FakeMarketplaceTransport(
        entries=(entry,),
        files={entry.source_id: b"x" * (MAX_MARKETPLACE_BYTES + 1)},
    )
    with pytest.raises(MarketplaceError) as oversize_error:
        import_marketplace_theme(entry, transport=oversized, config_dir=config_dir)
    assert oversize_error.value.kind == "oversized"
    assert not config_dir.exists(), "an oversized payload wrote before parsing"

    malformed = _FakeMarketplaceTransport(
        entries=(entry,), files={entry.source_id: b"{oops"}
    )
    with pytest.raises(ThemeImportError) as malformed_error:
        import_marketplace_theme(entry, transport=malformed, config_dir=config_dir)
    assert malformed_error.value.kind == "malformed"
    assert not config_dir.exists(), "a malformed payload wrote before parsing"


def test_marketplace_bytes_are_parsed_never_executed() -> None:
    for module in (
        "talaria/themes/marketplace.py",
        "talaria/ui/theme_import.py",
    ):
        source = (REPO_ROOT / module).read_text(encoding="utf-8")
        assert "importlib" not in source
        assert "__import__" not in source
        assert re.search(r"(?<![\w.])exec\s*\(", source) is None
        assert re.search(r"(?<![\w.])eval\s*\(", source) is None
        for match in re.finditer(r"compile\s*\(", source):
            start = max(0, source.rfind("\n", 0, match.start()))
            assert "re.compile" in source[start : match.end()]


def test_bytes_entry_point_derives_a_stem_from_its_label(
    tmp_path: Path,
) -> None:
    payload = dict(json.loads(SAMPLE_BYTES))
    del payload["name"]

    report = prepare_vscode_theme_import_bytes(
        json.dumps(payload).encode("utf-8"),
        source_label="https://example.invalid/Solar Flare.json",
        config_dir=tmp_path / "config",
    )

    assert report.target_name == "solar-flare"
    assert report.theme.tokens["talaria.canvas"] == "#102030"


def test_slugify_theme_label_derives_storage_stems() -> None:
    assert slugify_theme_label("Solar Flare") == "solar-flare"
    assert slugify_theme_label("  Dark Modern  ") == "dark-modern"
    assert slugify_theme_label("One_Dark.Pro") == "one-dark-pro"
    assert slugify_theme_label("!!!") == ""


def _patch_marketplace_transport(
    monkeypatch: pytest.MonkeyPatch, transport: _FakeMarketplaceTransport
) -> None:
    monkeypatch.setattr(
        talaria.cli, "_default_marketplace_transport", lambda: transport
    )


def test_cli_search_lists_bounded_entries_as_prose_and_json(
    isolated_global_config_dir: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    del isolated_global_config_dir
    first = _marketplace_entry()
    second = _marketplace_entry(
        source_id="acme/solar/2",
        theme_label="Solar Ember",
        download_url="https://example.invalid/themes/solar-ember.json",
    )
    transport = _FakeMarketplaceTransport(entries=(first, second))
    _patch_marketplace_transport(monkeypatch, transport)

    assert main(["theme", "search", "solar", "--limit", "999"]) == 0
    prose = capsys.readouterr().out
    assert "Solar Flare" in prose
    assert "Solar Ember" in prose
    assert "acme/solar/1" in prose
    # The page is bounded even when the operator asks for more.
    assert transport.search_calls == [("solar", 25)]

    assert main(["theme", "search", "solar", "--json"]) == 0
    document = json.loads(capsys.readouterr().out)
    assert document["schema_version"] == "talaria-theme-search-v1"
    assert document["query"] == "solar"
    assert document["count"] == 2
    assert [entry["source_id"] for entry in document["entries"]] == [
        "acme/solar/1",
        "acme/solar/2",
    ]


def test_cli_fetch_imports_without_manual_download(
    isolated_global_config_dir: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    _patch_marketplace_transport(monkeypatch, _solar_transport())

    assert main(["theme", "fetch", "acme/solar", "--name", "solar-flare"]) == 0
    prose = capsys.readouterr().out
    assert "Imported solar-flare as user theme solar-flare" in prose

    stored = isolated_global_config_dir / "themes" / "solar-flare.json"
    assert stored.is_file()
    sources, _notices = load_import_sources(
        config_dir=isolated_global_config_dir
    )
    assert sources["solar-flare"].ref == "acme/solar/1"


def test_cli_fetch_unknown_source_exits_three_and_writes_nothing(
    isolated_global_config_dir: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    _patch_marketplace_transport(monkeypatch, _solar_transport())

    assert main(["theme", "fetch", "nope/nothing", "--json"]) == 3
    document = json.loads(capsys.readouterr().out)
    assert document["schema_version"] == "talaria-theme-import-error-v1"
    assert document["kind"] == "unknown-source"
    assert not (isolated_global_config_dir / "themes").exists()


def test_cli_search_and_fetch_synopses_cover_every_parser_option() -> None:
    theme_parser = _subparser(build_parser(), "theme")
    guide = THEMES_GUIDE.read_text(encoding="utf-8")
    for operation, command in (("search", "QUERY"), ("fetch", "REF")):
        operation_parser = _subparser(theme_parser, operation)
        expected_options = {
            option
            for action in operation_parser._actions  # noqa: SLF001 - no public accessor
            if action.dest != "help"
            for option in action.option_strings
        }
        match = re.search(
            rf"(?m)^talaria theme {operation} {command}[^\n]*$", guide
        )
        assert match is not None, operation
        assert expected_options
        assert all(option in match.group(0) for option in expected_options)


def test_file_import_records_its_source_for_reload(tmp_path: Path) -> None:
    config_dir = tmp_path / "config"
    source = tmp_path / "solar.json"
    source.write_bytes(SAMPLE_BYTES)

    report = import_vscode_theme(source, name="solar-flare", config_dir=config_dir)

    sources, notices = load_import_sources(config_dir=config_dir)
    assert notices == ()
    assert sources["solar-flare"].kind == "file"
    assert sources["solar-flare"].ref == str(source.absolute())
    assert report.target_path.is_file()


def test_reload_reimports_edited_file_source_without_restart(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    source = tmp_path / "solar.json"
    source.write_bytes(SAMPLE_BYTES)
    report = import_vscode_theme(source, name="solar-flare", config_dir=config_dir)
    assert report.theme.tokens["talaria.canvas"] == "#102030"

    payload = json.loads(SAMPLE_BYTES)
    payload["colors"]["editor.background"] = "#112233"
    source.write_text(json.dumps(payload), encoding="utf-8")

    sources, _notices = load_import_sources(config_dir=config_dir)
    reread = reload_imported_theme(
        slug="solar-flare",
        source=sources["solar-flare"],
        config_dir=config_dir,
    )

    assert reread.theme.tokens["talaria.canvas"] == "#112233"
    stored = json.loads(reread.target_path.read_text(encoding="utf-8"))
    assert stored["tokens"]["talaria.canvas"] == "#112233"


def test_reload_of_now_invalid_source_preserves_the_stored_file(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    source = tmp_path / "solar.json"
    source.write_bytes(SAMPLE_BYTES)
    report = import_vscode_theme(source, name="solar-flare", config_dir=config_dir)
    before = report.target_path.read_bytes()

    source.write_text("{oops", encoding="utf-8")
    sources, _notices = load_import_sources(config_dir=config_dir)
    with pytest.raises(ThemeImportError, match="not strict JSON"):
        reload_imported_theme(
            slug="solar-flare",
            source=sources["solar-flare"],
            config_dir=config_dir,
        )

    assert report.target_path.read_bytes() == before


def test_reload_of_missing_source_file_preserves_the_stored_file(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    source = tmp_path / "solar.json"
    source.write_bytes(SAMPLE_BYTES)
    report = import_vscode_theme(source, name="solar-flare", config_dir=config_dir)
    before = report.target_path.read_bytes()
    source.unlink()

    sources, _notices = load_import_sources(config_dir=config_dir)
    with pytest.raises(ThemeImportError, match="could not be read"):
        reload_imported_theme(
            slug="solar-flare",
            source=sources["solar-flare"],
            config_dir=config_dir,
        )

    assert report.target_path.read_bytes() == before


def test_reload_of_edited_marketplace_source_without_restart(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    entry = _marketplace_entry()
    transport = _FakeMarketplaceTransport(
        entries=(entry,), files={entry.source_id: SAMPLE_BYTES}
    )
    report = import_marketplace_theme(
        entry, transport=transport, name="solar-flare", config_dir=config_dir
    )
    assert report.theme.tokens["talaria.canvas"] == "#102030"

    payload = json.loads(SAMPLE_BYTES)
    payload["colors"]["editor.background"] = "#445566"
    transport._files[entry.source_id] = json.dumps(payload).encode("utf-8")

    sources, _notices = load_import_sources(config_dir=config_dir)
    reread = reload_imported_theme(
        slug="solar-flare",
        source=sources["solar-flare"],
        transport=transport,
        config_dir=config_dir,
    )

    assert reread.theme.tokens["talaria.canvas"] == "#445566"


async def _submit_theme_command(app: object, pilot: object, text: str) -> None:
    app.composer.text = text  # type: ignore[attr-defined]
    app.composer.text_area.focus()  # type: ignore[attr-defined]
    await pilot.press("enter")  # type: ignore[attr-defined]
    await app.settle_live()  # type: ignore[attr-defined]
    await pilot.pause()  # type: ignore[attr-defined]


def _imported_app_config(
    tmp_path: Path, *, edited_canvas: str | None = None
) -> tuple[Path, Path]:
    """Stage one file-sourced theme and return (config_dir, source)."""
    config_dir = tmp_path / "config"
    source = tmp_path / "solar.json"
    payload = json.loads(SAMPLE_BYTES)
    if edited_canvas is not None:
        payload["colors"]["editor.background"] = edited_canvas
    source.write_text(json.dumps(payload), encoding="utf-8")
    import_vscode_theme(source, name="solar-flare", config_dir=config_dir)
    return config_dir, source


@pytest.mark.asyncio
async def test_theme_fetch_failure_preserves_current_theme_with_notice(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    transport = _solar_transport()
    transport.fetch_error = MarketplaceError("dial refused", kind="network")
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
        marketplace_transport=transport,
    )
    async with app.run_test(size=(80, 24)) as pilot:
        await _submit_theme_command(app, pilot, "/theme fetch acme/solar")
        assert app.theme == "refined-default"
        assert app.session_theme_slug is None
        rendered = screen_text(app)
        assert "theme fetch failed" in rendered
        assert "keeping 'refined-default'" in rendered
        assert not config_dir.exists(), "a failed fetch wrote user config"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_fetch_unknown_and_oversize_sources_preserve_theme(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    entry = _marketplace_entry()
    transport = _FakeMarketplaceTransport(
        entries=(entry,),
        files={entry.source_id: b"x" * (MAX_MARKETPLACE_BYTES + 1)},
    )
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
        marketplace_transport=transport,
    )
    async with app.run_test(size=(80, 24)) as pilot:
        await _submit_theme_command(app, pilot, "/theme fetch nope/nothing")
        assert app.theme == "refined-default"
        assert "is unknown" in screen_text(app)

        await _submit_theme_command(app, pilot, "/theme fetch acme/solar/1")
        assert app.theme == "refined-default"
        # The composer notice clips on screen; its full text names the bound.
        assert "past the" in app.composer.notice
        assert "byte bound" in app.composer.notice
        assert "keeping 'refined-default'" in app.composer.notice
        assert not (config_dir / "themes").exists()
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_fetch_applies_valid_theme_live_without_persisting(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    transport = _solar_transport()
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
        marketplace_transport=transport,
    )
    async with app.run_test(size=(80, 24)) as pilot:
        await _submit_theme_command(
            app, pilot, "/theme fetch acme/solar --name solar-flare"
        )
        assert app.theme == "solar-flare"
        assert app.session_theme_slug == "solar-flare"
        resolved = app.theme_registry.resolve("solar-flare")
        assert resolved.tokens["talaria.canvas"] == "#102030"
        assert "fetched" in screen_text(app)
        assert not (config_dir / "config.toml").exists(), (
            "fetching previewed without an explicit save"
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_reload_reimports_edited_source_without_restart(
    tmp_path: Path,
) -> None:
    config_dir, source = _imported_app_config(tmp_path)
    registry = theme_registry_for_config(config_dir=config_dir)
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_name="solar-flare",
        theme_registry=registry,
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
    )
    before_texts = tuple(entry.text for entry in app.state.transcript)
    async with app.run_test(size=(80, 24)) as pilot:
        assert app.theme == "solar-flare"

        payload = json.loads(source.read_text(encoding="utf-8"))
        payload["colors"]["editor.background"] = "#112233"
        source.write_text(json.dumps(payload), encoding="utf-8")

        await _submit_theme_command(app, pilot, "/theme reload")

        assert app.theme == "solar-flare"
        resolved = app.theme_registry.resolve("solar-flare")
        assert resolved.tokens["talaria.canvas"] == "#112233"
        assert "reloaded" in screen_text(app)
        # Reload is safe mid-turn: session state survives the re-resolve,
        # and the transcript gains exactly the reload report — item 4
        # holds for reload the same way it holds for fetch.
        after_texts = tuple(entry.text for entry in app.state.transcript)
        assert after_texts[: len(before_texts)] == before_texts
        assert len(after_texts) == len(before_texts) + 1
        assert "theme 'solar-flare' reloaded:" in after_texts[-1]
        assert "Appearance: 18 tokens use Refined Default values" in after_texts[-1]
        assert app.session_theme_slug == "solar-flare"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_same_slug_reload_repaints_the_live_screen(
    tmp_path: Path,
) -> None:
    """Stale-paint repair (option b): a same-slug spec swap repaints.

    Assigning the unchanged slug never wakes Textual's theme watcher, so
    without the explicit repaint the re-registered spec stays invisible even
    though the registry holds the new values.
    """
    config_dir, source = _imported_app_config(tmp_path)
    registry = theme_registry_for_config(config_dir=config_dir)
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_name="solar-flare",
        theme_registry=registry,
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
    )
    async with app.run_test(size=(80, 24)) as pilot:
        before = app.screen.styles.background

        payload = json.loads(source.read_text(encoding="utf-8"))
        payload["colors"]["editor.background"] = "#445566"
        source.write_text(json.dumps(payload), encoding="utf-8")

        await _submit_theme_command(app, pilot, "/theme reload")

        assert app.theme == "solar-flare"
        after = app.screen.styles.background
        assert after != before
        assert after == Color.parse("#445566")
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_same_slug_reload_without_changes_repaints_nothing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The option-b gate: an identical re-resolve schedules no refresh."""
    config_dir, _source = _imported_app_config(tmp_path)
    registry = theme_registry_for_config(config_dir=config_dir)
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_name="solar-flare",
        theme_registry=registry,
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
    )
    async with app.run_test(size=(80, 24)) as pilot:
        await pilot.pause()
        calls: list[str] = []
        original = app.refresh_css

        def spy(*, animate: bool = True) -> None:
            calls.append("refresh")
            original(animate=animate)

        monkeypatch.setattr(app, "refresh_css", spy)
        await _submit_theme_command(app, pilot, "/theme reload")

        assert app.theme == "solar-flare"
        assert calls == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_reload_of_invalid_source_preserves_theme_and_file(
    tmp_path: Path,
) -> None:
    config_dir, source = _imported_app_config(tmp_path)
    stored = config_dir / "themes" / "solar-flare.json"
    before = stored.read_bytes()
    registry = theme_registry_for_config(config_dir=config_dir)
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_name="solar-flare",
        theme_registry=registry,
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
    )
    async with app.run_test(size=(80, 24)) as pilot:
        source.write_text("{oops", encoding="utf-8")
        await _submit_theme_command(app, pilot, "/theme reload solar-flare")

        assert app.theme == "solar-flare"
        assert stored.read_bytes() == before
        assert app.theme_registry.resolve("solar-flare").tokens[
            "talaria.canvas"
        ] == "#102030"
        rendered = screen_text(app)
        assert "theme reload failed" in rendered
        assert "keeping 'solar-flare'" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_reload_without_recorded_source_preserves_theme(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
    )
    async with app.run_test(size=(80, 24)) as pilot:
        await _submit_theme_command(app, pilot, "/theme reload")

        assert app.theme == "refined-default"
        assert "no recorded import source" in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_edited_source_without_reload_changes_nothing_live(
    tmp_path: Path,
) -> None:
    """Without an explicit Reload, an edited source file changes nothing."""
    config_dir, source = _imported_app_config(tmp_path)
    registry = theme_registry_for_config(config_dir=config_dir)
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_name="solar-flare",
        theme_registry=registry,
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
    )
    async with app.run_test(size=(80, 24)) as pilot:
        payload = json.loads(source.read_text(encoding="utf-8"))
        payload["colors"]["editor.background"] = "#112233"
        source.write_text(json.dumps(payload), encoding="utf-8")

        await app.settle_live()
        await pilot.pause()

        assert app.theme == "solar-flare"
        assert app.theme_registry.resolve("solar-flare").tokens[
            "talaria.canvas"
        ] == "#102030"
        await app.shutdown_sources()


def test_theme_reload_registers_no_filesystem_watcher() -> None:
    for module in (
        "talaria/themes/marketplace.py",
        "talaria/themes/sources.py",
        "talaria/ui/theme_import.py",
        "talaria/ui/app.py",
    ):
        source = (REPO_ROOT / module).read_text(encoding="utf-8")
        for token in (
            "watchdog",
            "watchfiles",
            "inotify",
            "Observer",
            "add_watch",
            "watch_file",
            "poll_mtime",
        ):
            assert token not in source, f"{module} mentions {token}"


@pytest.mark.asyncio
async def test_concurrent_reloads_serialize(tmp_path: Path) -> None:
    config_dir = tmp_path / "config"
    entry = _marketplace_entry()
    transport = _FakeMarketplaceTransport(
        entries=(entry,), files={entry.source_id: SAMPLE_BYTES}
    )
    import_marketplace_theme(
        entry, transport=transport, name="solar-flare", config_dir=config_dir
    )
    payload = json.loads(SAMPLE_BYTES)
    payload["colors"]["editor.background"] = "#445566"
    transport._files[entry.source_id] = json.dumps(payload).encode("utf-8")

    registry = theme_registry_for_config(config_dir=config_dir)
    app, _ = paused_app(
        [event("gateway.ready", {})],
        theme_name="solar-flare",
        theme_registry=registry,
        theme_config_dir=config_dir,
        launch_cwd=tmp_path,
        marketplace_transport=transport,
    )
    async with app.run_test(size=(80, 24)):
        # The setup import above already fetched once; only Reloads count here.
        transport.fetch_calls.clear()
        transport.lookup_calls.clear()
        transport.fetch_gate = threading.Event()
        first = asyncio.create_task(app._reload_imported_theme("solar-flare"))
        await asyncio.sleep(0.3)
        second = asyncio.create_task(app._reload_imported_theme("solar-flare"))
        await asyncio.sleep(0.3)

        # The second Reload waits behind the lock instead of fetching alongside.
        assert transport.fetch_calls == [entry.source_id]

        transport.fetch_gate.set()
        await asyncio.gather(first, second)

        assert transport.fetch_calls == [entry.source_id, entry.source_id]
        assert transport.max_active == 1
        assert app.theme == "solar-flare"
        assert app.theme_registry.resolve("solar-flare").tokens[
            "talaria.canvas"
        ] == "#445566"
        await app.shutdown_sources()
