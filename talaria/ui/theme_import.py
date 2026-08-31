"""Bounded Visual Studio Code color-theme import for Talaria user themes."""

from __future__ import annotations

import json
import re
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import Any, Final, Literal

from talaria.themes import THEME_TOKENS, ThemeSpec
from talaria.themes.builtins import REFINED_DEFAULT
from talaria.ui.theme import (
    BUILTIN_THEME_REGISTRY,
    ThemeRegistry,
    user_theme_path,
    write_user_theme,
)

ThemeImportErrorKind = Literal[
    "unreadable",
    "empty",
    "malformed",
    "wrong-root",
    "reserved-slug",
    "invalid-slug",
    "unwritable",
]


class ThemeImportError(ValueError):
    """The source cannot produce a valid bounded Talaria theme."""

    def __init__(self, message: str, *, kind: ThemeImportErrorKind) -> None:
        super().__init__(message)
        self.kind = kind


@dataclass(frozen=True)
class WorkbenchMapping:
    """One Talaria token and its ordered Visual Studio Code candidate keys."""

    token: str
    candidates: tuple[str, ...]


WORKBENCH_MAPPINGS: Final[tuple[WorkbenchMapping, ...]] = (
    WorkbenchMapping("talaria.canvas", ("editor.background",)),
    WorkbenchMapping("talaria.surface", ("editorWidget.background", "input.background")),
    WorkbenchMapping("talaria.panel", ("panel.background",)),
    WorkbenchMapping("talaria.text", ("editor.foreground",)),
    WorkbenchMapping(
        "talaria.text.muted",
        ("descriptionForeground", "input.placeholderForeground"),
    ),
    WorkbenchMapping("talaria.primary", ("textLink.foreground",)),
    WorkbenchMapping(
        "talaria.accent",
        ("activityBarBadge.background", "button.background"),
    ),
    WorkbenchMapping(
        "talaria.success",
        ("testing.iconPassed", "gitDecoration.addedResourceForeground"),
    ),
    WorkbenchMapping(
        "talaria.warning",
        ("editorWarning.foreground", "notificationsWarningIcon.foreground"),
    ),
    WorkbenchMapping("talaria.error", ("errorForeground", "editorError.foreground")),
    WorkbenchMapping("talaria.border", ("contrastBorder", "widget.border")),
    WorkbenchMapping("talaria.border.muted", ("editorGroup.border",)),
    WorkbenchMapping("talaria.focus", ("focusBorder",)),
    WorkbenchMapping("talaria.selection.background", ("editor.selectionBackground",)),
    WorkbenchMapping("talaria.selection.text", ("editor.selectionForeground",)),
    WorkbenchMapping("talaria.status.background", ("statusBar.background",)),
    WorkbenchMapping("talaria.status.text", ("statusBar.foreground",)),
    WorkbenchMapping("talaria.status.separator", ("statusBar.border",)),
    WorkbenchMapping("talaria.inspector.background", ("sideBar.background",)),
    WorkbenchMapping("talaria.inspector.border", ("sideBar.border",)),
    WorkbenchMapping(
        "talaria.inspector.heading",
        ("sideBarTitle.foreground", "sideBarSectionHeader.foreground"),
    ),
    WorkbenchMapping("talaria.diff.context", ("editor.foreground",)),
    WorkbenchMapping("talaria.diff.line-number", ("editorLineNumber.foreground",)),
    WorkbenchMapping(
        "talaria.diff.added", ("gitDecoration.addedResourceForeground",)
    ),
    WorkbenchMapping(
        "talaria.diff.added.background", ("diffEditor.insertedLineBackground",)
    ),
    WorkbenchMapping(
        "talaria.diff.removed", ("gitDecoration.deletedResourceForeground",)
    ),
    WorkbenchMapping(
        "talaria.diff.removed.background", ("diffEditor.removedLineBackground",)
    ),
    WorkbenchMapping("talaria.diff.hunk", ("editorInfo.foreground",)),
    WorkbenchMapping(
        "talaria.diff.hunk.background", ("editor.lineHighlightBackground",)
    ),
    WorkbenchMapping(
        "talaria.diff.intraline-added.background",
        ("diffEditor.insertedTextBackground",),
    ),
    WorkbenchMapping(
        "talaria.diff.intraline-removed.background",
        ("diffEditor.removedTextBackground",),
    ),
)


@dataclass(frozen=True)
class SyntaxMapping:
    """One Talaria syntax token and its supported TextMate scope prefixes."""

    token: str
    prefixes: tuple[str, ...]


SYNTAX_MAPPINGS: Final[tuple[SyntaxMapping, ...]] = (
    SyntaxMapping(
        "talaria.syntax.comment", ("comment", "punctuation.definition.comment")
    ),
    SyntaxMapping(
        "talaria.syntax.keyword", ("keyword", "keyword.control", "storage.modifier")
    ),
    SyntaxMapping(
        "talaria.syntax.string", ("string", "string.quoted", "string.template")
    ),
    SyntaxMapping("talaria.syntax.number", ("constant.numeric",)),
    SyntaxMapping(
        "talaria.syntax.function", ("entity.name.function", "support.function")
    ),
    SyntaxMapping(
        "talaria.syntax.type",
        ("entity.name.type", "entity.name.class", "support.type", "storage.type"),
    ),
    SyntaxMapping(
        "talaria.syntax.variable", ("variable", "variable.other", "variable.parameter")
    ),
    SyntaxMapping(
        "talaria.syntax.operator",
        ("keyword.operator", "punctuation.separator", "punctuation.accessor"),
    ),
    SyntaxMapping(
        "talaria.syntax.constant", ("constant", "constant.language", "support.constant")
    ),
)


ALWAYS_FALLBACK_TOKENS: Final[tuple[str, ...]] = (
    "talaria.secondary",
    "talaria.status.muted",
    "talaria.transcript.operator",
    "talaria.transcript.operator.background",
    "talaria.transcript.assistant",
    "talaria.transcript.assistant.background",
    "talaria.transcript.reasoning",
    "talaria.transcript.reasoning.background",
    "talaria.transcript.activity",
    "talaria.transcript.activity.background",
    "talaria.transcript.session",
    "talaria.transcript.session.background",
    "talaria.transcript.fault",
    "talaria.transcript.fault.background",
)


@dataclass(frozen=True)
class AlphaComposite:
    """One accepted alpha source after compositing to an opaque runtime value."""

    path: str
    token: str
    source: str
    background: str
    value: str


@dataclass(frozen=True)
class ReportLine:
    """One prose report record with routing independent of its text."""

    severity: Literal["info", "warning"]
    text: str


@dataclass(frozen=True)
class ImportReport:
    """The complete deterministic result prepared before the user-theme write."""

    mapped_values: Mapping[str, str]
    unsupported_entries: tuple[str, ...]
    fallback_tokens: tuple[str, ...]
    composites: tuple[AlphaComposite, ...]
    target_name: str
    target_path: Path
    theme: ThemeSpec

    def __post_init__(self) -> None:
        object.__setattr__(
            self, "mapped_values", MappingProxyType(dict(self.mapped_values))
        )

    @property
    def mapped_count(self) -> int:
        return len(self.mapped_values)

    @property
    def fallback_count(self) -> int:
        return len(self.fallback_tokens)

    @property
    def unsupported_count(self) -> int:
        return len(self.unsupported_entries)

    def records(self) -> tuple[ReportLine, ...]:
        """Return report records whose severity is independent of prose."""
        summary = (
            f"Imported {self.target_name} as user theme {self.theme.slug}: "
            f"{self.mapped_count} source tokens, {self.fallback_count} fallbacks, "
            f"{self.unsupported_count} warnings."
        )
        warnings = tuple(
            ReportLine("warning", f"warning: {entry}")
            for entry in self.unsupported_entries
        )
        composites = tuple(
            ReportLine(
                "info",
                "composite: "
                f"{item.path} -> {item.token}: {item.source} over "
                f"{item.background} = {item.value}",
            )
            for item in self.composites
        )
        fallbacks = tuple(
            ReportLine(
                "info",
                f"fallback: {token} <- Refined Default {self.theme.tokens[token]}",
            )
            for token in self.fallback_tokens
        )
        return (ReportLine("info", summary), *warnings, *composites, *fallbacks)

    def lines(self) -> tuple[str, ...]:
        """Return the stable prose report used by existing interactive callers."""
        return tuple(record.text for record in self.records())

    def to_json_dict(self) -> dict[str, object]:
        """Return the versioned machine-readable import report."""
        return {
            "schema_version": "talaria-theme-import-report-v1",
            "slug": self.theme.slug,
            "target_path": str(self.target_path),
            "source_token_count": self.mapped_count,
            "fallback_count": self.fallback_count,
            "warning_count": self.unsupported_count,
            "fallbacks": [
                {
                    "severity": "info",
                    "source": "refined-default",
                    "token": token,
                    "value": self.theme.tokens[token],
                }
                for token in self.fallback_tokens
            ],
            "warnings": [
                {"severity": "warning", "message": message}
                for message in self.unsupported_entries
            ],
        }


@dataclass(frozen=True)
class _SourceColor:
    raw: str
    red: int
    green: int
    blue: int
    alpha: int


_HEX_COLOR_RE: Final[re.Pattern[str]] = re.compile(
    r"^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$"
)
_SIMPLE_SCOPE_RE: Final[re.Pattern[str]] = re.compile(
    r"^[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)*$"
)

_WORKBENCH_KEYS: Final[frozenset[str]] = frozenset(
    key for mapping in WORKBENCH_MAPPINGS for key in mapping.candidates
)
_ROOT_KEYS: Final[frozenset[str]] = frozenset(
    {"$schema", "name", "type", "colors", "tokenColors"}
)

# Each alpha source is composited against the destination's normative surface.
# A background not mapped yet falls through to the same Refined Default token.
_BACKGROUND_TOKEN: Final[Mapping[str, str]] = MappingProxyType(
    {
        "talaria.canvas": "talaria.canvas",
        "talaria.surface": "talaria.canvas",
        "talaria.panel": "talaria.canvas",
        "talaria.text": "talaria.canvas",
        "talaria.text.muted": "talaria.canvas",
        "talaria.primary": "talaria.canvas",
        "talaria.accent": "talaria.canvas",
        "talaria.success": "talaria.canvas",
        "talaria.warning": "talaria.canvas",
        "talaria.error": "talaria.canvas",
        "talaria.border": "talaria.surface",
        "talaria.border.muted": "talaria.canvas",
        "talaria.focus": "talaria.canvas",
        "talaria.selection.background": "talaria.canvas",
        "talaria.selection.text": "talaria.selection.background",
        "talaria.status.background": "talaria.canvas",
        "talaria.status.text": "talaria.status.background",
        "talaria.status.separator": "talaria.status.background",
        "talaria.inspector.background": "talaria.canvas",
        "talaria.inspector.border": "talaria.canvas",
        "talaria.inspector.heading": "talaria.inspector.background",
        "talaria.diff.context": "talaria.canvas",
        "talaria.diff.line-number": "talaria.canvas",
        "talaria.diff.added": "talaria.diff.added.background",
        "talaria.diff.added.background": "talaria.canvas",
        "talaria.diff.removed": "talaria.diff.removed.background",
        "talaria.diff.removed.background": "talaria.canvas",
        "talaria.diff.hunk": "talaria.diff.hunk.background",
        "talaria.diff.hunk.background": "talaria.canvas",
        "talaria.diff.intraline-added.background": "talaria.diff.added.background",
        "talaria.diff.intraline-removed.background": "talaria.diff.removed.background",
        **{mapping.token: "talaria.surface" for mapping in SYNTAX_MAPPINGS},
    }
)


def _reject_json_constant(value: str) -> Any:
    raise ValueError(f"non-JSON numeric constant {value}")


def _display_key(key: object) -> str:
    """Render a source-controlled object key without live control characters."""
    rendered = repr(key)
    if len(rendered) >= 2 and rendered[0] == rendered[-1] and rendered[0] in "'\"":
        return rendered[1:-1]
    return rendered


def _parse_source(path: Path) -> Mapping[str, Any]:
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise ThemeImportError(
            f"{path} could not be read as UTF-8 JSON: {exc}",
            kind="unreadable",
        ) from exc
    if not text.strip():
        raise ThemeImportError(
            f"{path} is empty; expected a Visual Studio Code theme object",
            kind="empty",
        )
    try:
        payload = json.loads(text, parse_constant=_reject_json_constant)
    except (json.JSONDecodeError, ValueError) as exc:
        raise ThemeImportError(
            f"{path} is not strict JSON: {exc}", kind="malformed"
        ) from exc
    if not isinstance(payload, dict):
        raise ThemeImportError(
            f"{path} root must be a JSON object", kind="wrong-root"
        )
    return payload


def _parse_color(value: object) -> _SourceColor | None:
    if not isinstance(value, str) or _HEX_COLOR_RE.fullmatch(value) is None:
        return None
    digits = value[1:]
    if len(digits) in (3, 4):
        digits = "".join(character * 2 for character in digits)
    if len(digits) == 6:
        digits += "FF"
    return _SourceColor(
        raw=value,
        red=int(digits[0:2], 16),
        green=int(digits[2:4], 16),
        blue=int(digits[4:6], 16),
        alpha=int(digits[6:8], 16),
    )


def _opaque(red: int, green: int, blue: int) -> str:
    return f"#{red:02X}{green:02X}{blue:02X}"


def _composite_channel(source: int, background: int, alpha: int) -> int:
    return (source * alpha + background * (255 - alpha) + 127) // 255


def _mapped_color(
    source: _SourceColor,
    *,
    path: str,
    token: str,
    mapped: Mapping[str, str],
) -> tuple[str, AlphaComposite | None]:
    if source.alpha == 255:
        return _opaque(source.red, source.green, source.blue), None
    background_token = _BACKGROUND_TOKEN[token]
    background = mapped.get(background_token, REFINED_DEFAULT.tokens[background_token])
    red = int(background[1:3], 16)
    green = int(background[3:5], 16)
    blue = int(background[5:7], 16)
    value = _opaque(
        _composite_channel(source.red, red, source.alpha),
        _composite_channel(source.green, green, source.alpha),
        _composite_channel(source.blue, blue, source.alpha),
    )
    return value, AlphaComposite(path, token, source.raw, background, value)


def _root_warnings(payload: Mapping[str, Any]) -> list[str]:
    warnings: list[str] = []
    for key in payload:
        if key in _ROOT_KEYS:
            continue
        path = f"root.{_display_key(key)}"
        if key == "include":
            warnings.append(
                f"{path} is unsupported; external theme files are not read"
            )
        elif key in {"semanticHighlighting", "semanticTokenColors"}:
            warnings.append(
                f"{path} is unsupported; semantic-token theming is not imported"
            )
        else:
            warnings.append(f"{path} is unsupported")
    return warnings


def _dark_flag(payload: Mapping[str, Any], warnings: list[str]) -> bool:
    value = payload.get("type")
    if value is None:
        return REFINED_DEFAULT.dark
    if value == "light":
        return False
    if value in {"dark", "hc"}:
        return True
    warnings.append(
        "root.type is invalid; expected light, dark, or hc and used Refined Default"
    )
    return REFINED_DEFAULT.dark


def _target_slug(path: Path, payload: Mapping[str, Any], name: str | None) -> str:
    selected: object
    if name is not None:
        selected = name
    elif "name" in payload:
        selected = payload["name"]
    else:
        selected = path.stem
    if not isinstance(selected, str):
        raise ThemeImportError(
            "theme name must be a lowercase hyphenated string",
            kind="invalid-slug",
        )
    try:
        ThemeSpec(slug=selected, name=selected, dark=False, tokens={})
    except ValueError as exc:
        raise ThemeImportError(str(exc), kind="invalid-slug") from exc
    if selected in BUILTIN_THEME_REGISTRY.slugs:
        raise ThemeImportError(
            f"theme name {selected!r} is reserved by a built-in theme",
            kind="reserved-slug",
        )
    return selected


def _workbench_colors(
    payload: Mapping[str, Any],
    warnings: list[str],
) -> tuple[dict[str, _SourceColor], bool]:
    if "colors" not in payload:
        return {}, False
    colors = payload["colors"]
    if not isinstance(colors, dict):
        raise ThemeImportError(
            "root.colors must be an object when present", kind="wrong-root"
        )
    parsed: dict[str, _SourceColor] = {}
    for key, value in colors.items():
        path = f"colors.{_display_key(key)}"
        if key not in _WORKBENCH_KEYS:
            warnings.append(f"{path} is unsupported")
            continue
        color = _parse_color(value)
        if color is None:
            warnings.append(
                f"{path} is invalid; expected #RGB, #RGBA, #RRGGBB, or #RRGGBBAA"
            )
            continue
        parsed[key] = color
    return parsed, True


def _token_rules(
    payload: Mapping[str, Any], warnings: list[str]
) -> tuple[Sequence[object], bool]:
    if "tokenColors" not in payload:
        return (), False
    rules = payload["tokenColors"]
    if isinstance(rules, str):
        warnings.append(
            "root.tokenColors is unsupported; external TextMate files are not read"
        )
        return (), True
    if not isinstance(rules, list):
        raise ThemeImportError(
            "root.tokenColors must be an array when present", kind="wrong-root"
        )
    return rules, True


def _map_workbench(
    colors: Mapping[str, _SourceColor],
) -> tuple[dict[str, str], list[AlphaComposite]]:
    mapped: dict[str, str] = {}
    composites: list[AlphaComposite] = []
    for mapping in WORKBENCH_MAPPINGS:
        selected = next(
            ((key, colors[key]) for key in mapping.candidates if key in colors), None
        )
        if selected is None:
            continue
        key, source = selected
        value, composite = _mapped_color(
            source,
            path=f"colors.{key}",
            token=mapping.token,
            mapped=mapped,
        )
        mapped[mapping.token] = value
        if composite is not None:
            composites.append(composite)
    return mapped, composites


def _selectors(
    scope: object, index: int, warnings: list[str]
) -> tuple[tuple[str, ...], bool]:
    values: list[str] = []
    if isinstance(scope, str):
        raw_values: Sequence[object] = (scope,)
    elif isinstance(scope, list):
        raw_values = scope
    else:
        warnings.append(f"tokenColors[{index}].scope is invalid")
        return (), False
    for scope_index, raw in enumerate(raw_values):
        if not isinstance(raw, str):
            warnings.append(
                f"tokenColors[{index}].scope[{scope_index}] is invalid"
            )
            continue
        values.extend(part.strip() for part in raw.split(",") if part.strip())
    return tuple(values), True


def _selector_matches(selector: str) -> tuple[tuple[str, int], ...]:
    matches: list[tuple[str, int]] = []
    for mapping in SYNTAX_MAPPINGS:
        for prefix in mapping.prefixes:
            if selector == prefix or selector.startswith(prefix + "."):
                matches.append((mapping.token, len(prefix.split("."))))
    if not matches:
        return ()
    longest = max(length for _token, length in matches)
    return tuple(
        (token, length) for token, length in matches if length == longest
    )


@dataclass(frozen=True)
class _SyntaxCandidate:
    prefix_length: int
    rule_index: int
    path: str
    color: _SourceColor


def _syntax_candidates(
    rules: Sequence[object], warnings: list[str]
) -> dict[str, _SyntaxCandidate]:
    candidates: dict[str, _SyntaxCandidate] = {}
    for index, raw_rule in enumerate(rules):
        base = f"tokenColors[{index}]"
        if not isinstance(raw_rule, dict):
            warnings.append(f"{base} is invalid; expected an object")
            continue
        for key in raw_rule:
            if key not in {"scope", "settings"}:
                warnings.append(f"{base}.{_display_key(key)} is unsupported")

        settings = raw_rule.get("settings")
        if not isinstance(settings, dict):
            warnings.append(f"{base}.settings is invalid; expected an object")
        else:
            for key, value in settings.items():
                path = f"{base}.settings.{_display_key(key)}"
                if key == "foreground":
                    continue
                if key == "fontStyle" and not value:
                    continue
                if key == "background":
                    warnings.append(f"{path} is unsupported and was ignored")
                elif key == "fontStyle":
                    warnings.append(f"{path} is unsupported and was ignored")
                else:
                    warnings.append(f"{path} is unsupported")

        selectors: tuple[str, ...] = ()
        scope_valid = True
        if "scope" not in raw_rule:
            warnings.append(f"{base} is an unsupported unscoped default")
            scope_valid = False
        else:
            selectors, scope_valid = _selectors(raw_rule["scope"], index, warnings)
            if scope_valid and not selectors:
                warnings.append(f"{base} is an unsupported unscoped default")

        foreground_path = f"{base}.settings.foreground"
        color = _parse_color(settings.get("foreground")) if isinstance(settings, dict) else None
        if color is None:
            if isinstance(settings, dict) and "foreground" in settings:
                warnings.append(
                    f"{foreground_path} is invalid; expected #RGB, #RGBA, "
                    "#RRGGBB, or #RRGGBBAA"
                )
        if color is None or not scope_valid or not selectors:
            continue
        per_rule: dict[str, int] = {}
        for selector in selectors:
            if _SIMPLE_SCOPE_RE.fullmatch(selector) is None:
                warnings.append(
                    f"{base}.scope selector {selector!r} is unsupported"
                )
                continue
            matches = _selector_matches(selector)
            if not matches:
                warnings.append(
                    f"{base}.scope selector {selector!r} is unsupported"
                )
                continue
            for token, length in matches:
                per_rule[token] = max(per_rule.get(token, 0), length)

        for token, length in per_rule.items():
            candidate = _SyntaxCandidate(length, index, foreground_path, color)
            previous = candidates.get(token)
            if previous is None or (
                candidate.prefix_length,
                candidate.rule_index,
            ) >= (previous.prefix_length, previous.rule_index):
                candidates[token] = candidate
    return candidates


def _map_syntax(
    rules: Sequence[object],
    mapped: dict[str, str],
    warnings: list[str],
) -> list[AlphaComposite]:
    composites: list[AlphaComposite] = []
    candidates = _syntax_candidates(rules, warnings)
    for mapping in SYNTAX_MAPPINGS:
        candidate = candidates.get(mapping.token)
        if candidate is None:
            continue
        value, composite = _mapped_color(
            candidate.color,
            path=candidate.path,
            token=mapping.token,
            mapped=mapped,
        )
        mapped[mapping.token] = value
        if composite is not None:
            composites.append(composite)
    return composites


def prepare_vscode_theme_import(
    source: Path,
    *,
    name: str | None = None,
    config_dir: Path | None = None,
) -> ImportReport:
    """Parse and completely report one import without changing the filesystem."""
    payload = _parse_source(source)
    warnings = _root_warnings(payload)
    slug = _target_slug(source, payload, name)
    dark = _dark_flag(payload, warnings)
    colors, had_colors = _workbench_colors(payload, warnings)
    rules, had_rules = _token_rules(payload, warnings)
    if not had_colors and not had_rules:
        raise ThemeImportError(
            f"{source} has neither a colors object nor a tokenColors array",
            kind="wrong-root",
        )

    mapped, composites = _map_workbench(colors)
    composites.extend(_map_syntax(rules, mapped, warnings))
    if not mapped:
        raise ThemeImportError(
            f"{source} contains no usable supported workbench color or TextMate scope",
            kind="wrong-root",
        )

    partial = ThemeSpec(slug=slug, name=slug, dark=dark, tokens=mapped)
    resolved = ThemeRegistry((REFINED_DEFAULT, partial)).resolve(slug)
    complete = ThemeSpec(
        slug=slug,
        name=slug,
        dark=dark,
        tokens=resolved.tokens,
    )
    target = user_theme_path(slug, config_dir=config_dir)
    return ImportReport(
        mapped_values=mapped,
        unsupported_entries=tuple(warnings),
        fallback_tokens=tuple(
            token for token in THEME_TOKENS if token not in mapped
        ),
        composites=tuple(composites),
        target_name=slug,
        target_path=target,
        theme=complete,
    )


def import_vscode_theme(
    source: Path,
    *,
    name: str | None = None,
    config_dir: Path | None = None,
) -> ImportReport:
    """Validate fully, then atomically create or replace one user theme."""
    report = prepare_vscode_theme_import(
        source,
        name=name,
        config_dir=config_dir,
    )
    try:
        written = write_user_theme(report.theme, config_dir=config_dir)
    except OSError as exc:
        raise ThemeImportError(
            f"{report.target_path} could not be written: {exc}",
            kind="unwritable",
        ) from exc
    if written != report.target_path:  # pragma: no cover - one shared path helper
        raise ThemeImportError(
            "the prepared and written user-theme targets differed",
            kind="unwritable",
        )
    return report


__all__ = [
    "ALWAYS_FALLBACK_TOKENS",
    "AlphaComposite",
    "ImportReport",
    "ReportLine",
    "SYNTAX_MAPPINGS",
    "ThemeImportError",
    "ThemeImportErrorKind",
    "WORKBENCH_MAPPINGS",
    "import_vscode_theme",
    "prepare_vscode_theme_import",
]
