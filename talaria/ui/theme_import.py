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
from talaria.themes.marketplace import (
    MAX_MARKETPLACE_BYTES,
    MarketplaceEntry,
    MarketplaceTransport,
    fetch_marketplace_bytes,
    resolve_marketplace_source,
    slugify_theme_label,
)
from talaria.themes.sources import (
    ImportSource,
    ImportSourceKind,
    record_import_source,
)
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

    def appearance_lead(self) -> str:
        """Lead with what changed visually; warnings are never dismissible."""
        if self.fallback_count and self.unsupported_count:
            return (
                f"Appearance: {self.fallback_count} tokens use Refined Default "
                "values (fallback: lines below) — these change what you see; "
                f"{self.unsupported_count} source features were not imported "
                "(warning: lines below) — check any you expected to see."
            )
        if self.fallback_count:
            return (
                f"Appearance: {self.fallback_count} tokens use Refined Default "
                "values (fallback: lines below) — these change what you see."
            )
        if self.unsupported_count:
            return (
                "Appearance: every mapped token renders as authored; "
                f"{self.unsupported_count} source features were not imported "
                "(warning: lines below) — check any you expected to see."
            )
        return (
            "Appearance: every token renders as authored; nothing fell back "
            "and nothing was ignored."
        )

    def records(self) -> tuple[ReportLine, ...]:
        """Return report records whose severity is independent of prose.

        The appearance-changing facts lead: the summary, then what the
        fallbacks change visually, then the full detail with fallbacks and
        composites before the ignored-feature warnings. Severity routing is
        unchanged — warnings still ride standard error.
        """
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
        return (
            ReportLine("info", summary),
            ReportLine("info", self.appearance_lead()),
            *fallbacks,
            *composites,
            *warnings,
        )

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
            "composites": [
                {
                    "severity": "info",
                    "path": item.path,
                    "token": item.token,
                    "source": item.source,
                    "background": item.background,
                    "value": item.value,
                }
                for item in self.composites
            ],
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


def _parse_bytes(data: bytes, *, label: str) -> Mapping[str, Any]:
    """Parse one in-memory payload with the single strict source strictness.

    Local files and marketplace downloads both land here, so the
    readability, emptiness, strict-JSON, and root-shape rules — and every
    message — hold identically for both sources.
    """
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ThemeImportError(
            f"{label} could not be read as UTF-8 JSON: {exc}",
            kind="unreadable",
        ) from exc
    if not text.strip():
        raise ThemeImportError(
            f"{label} is empty; expected a Visual Studio Code theme object",
            kind="empty",
        )
    try:
        payload = json.loads(text, parse_constant=_reject_json_constant)
    except (json.JSONDecodeError, ValueError) as exc:
        raise ThemeImportError(
            f"{label} is not strict JSON: {exc}", kind="malformed"
        ) from exc
    if not isinstance(payload, dict):
        raise ThemeImportError(
            f"{label} root must be a JSON object", kind="wrong-root"
        )
    return payload


def _parse_source(path: Path) -> Mapping[str, Any]:
    try:
        data = path.read_bytes()
    except OSError as exc:
        raise ThemeImportError(
            f"{path} could not be read as UTF-8 JSON: {exc}",
            kind="unreadable",
        ) from exc
    return _parse_bytes(data, label=str(path))


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


def _target_slug(stem: str, payload: Mapping[str, Any], name: str | None) -> str:
    selected: object
    if name is not None:
        selected = name
    elif "name" in payload:
        selected = payload["name"]
    else:
        selected = stem
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


def _prepare_report(
    payload: Mapping[str, Any],
    *,
    display: str,
    stem: str,
    name: str | None,
    config_dir: Path | None,
) -> ImportReport:
    """Run the one shared report pipeline over an already-parsed payload."""
    warnings = _root_warnings(payload)
    slug = _target_slug(stem, payload, name)
    dark = _dark_flag(payload, warnings)
    colors, had_colors = _workbench_colors(payload, warnings)
    rules, had_rules = _token_rules(payload, warnings)
    if not had_colors and not had_rules:
        raise ThemeImportError(
            f"{display} has neither a colors object nor a tokenColors array",
            kind="wrong-root",
        )

    mapped, composites = _map_workbench(colors)
    composites.extend(_map_syntax(rules, mapped, warnings))
    if not mapped:
        raise ThemeImportError(
            f"{display} contains no usable supported workbench color "
            "or TextMate scope",
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


def _write_report(
    report: ImportReport, *, config_dir: Path | None
) -> ImportReport:
    """Atomically store one fully validated report, changing nothing else."""
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


def prepare_vscode_theme_import(
    source: Path,
    *,
    name: str | None = None,
    config_dir: Path | None = None,
) -> ImportReport:
    """Parse and completely report one import without changing the filesystem."""
    payload = _parse_source(source)
    return _prepare_report(
        payload,
        display=str(source),
        stem=source.stem,
        name=name,
        config_dir=config_dir,
    )


def prepare_vscode_theme_import_bytes(
    data: bytes,
    *,
    source_label: str,
    name: str | None = None,
    config_dir: Path | None = None,
) -> ImportReport:
    """Parse in-memory bytes with the identical strictness as a local file.

    Marketplace downloads land here instead of growing a second parser:
    the same bytes that parse from disk parse here, and the same bytes
    that fail from disk fail here with the same kind.
    """
    payload = _parse_bytes(bytes(data), label=source_label)
    last_segment = source_label.rsplit("/", 1)[-1]
    if last_segment.lower().endswith(".json"):
        last_segment = last_segment[: -len(".json")]
    stem = slugify_theme_label(last_segment) or "marketplace-theme"
    return _prepare_report(
        payload,
        display=source_label,
        stem=stem,
        name=name,
        config_dir=config_dir,
    )


def prepare_marketplace_import(
    data: bytes,
    entry: MarketplaceEntry,
    *,
    name: str | None = None,
    config_dir: Path | None = None,
) -> ImportReport:
    """Parse fetched marketplace bytes without changing the filesystem.

    The storage name is the explicit ``name`` when given, else the
    slugified marketplace theme label; the payload's own top-level name
    is a display string and never a storage slug.
    """
    fallback = slugify_theme_label(entry.theme_label) or None
    return prepare_vscode_theme_import_bytes(
        data,
        source_label=entry.download_url,
        name=name if name is not None else fallback,
        config_dir=config_dir,
    )


def _record_source(
    report: ImportReport, *, kind: ImportSourceKind, ref: str
) -> None:
    """Remember one import source without letting a sidecar failure mislead.

    The theme file is already validated and written when this runs; a
    sidecar failure is reported with the same ``unwritable`` kind the
    write path uses so callers keep one failure vocabulary.
    """
    try:
        record_import_source(
            config_dir=report.target_path.parent.parent,
            slug=report.theme.slug,
            kind=kind,
            ref=ref,
        )
    except OSError as exc:
        raise ThemeImportError(
            f"theme {report.theme.slug!r} was stored but its import source "
            f"could not be recorded: {exc}",
            kind="unwritable",
        ) from exc


def import_vscode_theme(
    source: Path,
    *,
    name: str | None = None,
    config_dir: Path | None = None,
) -> ImportReport:
    """Validate fully, then atomically create or replace one user theme."""
    report = _write_report(
        prepare_vscode_theme_import(
            source,
            name=name,
            config_dir=config_dir,
        ),
        config_dir=config_dir,
    )
    _record_source(
        report, kind="file", ref=str(Path(source).expanduser().absolute())
    )
    return report


def import_marketplace_theme(
    entry: MarketplaceEntry,
    *,
    transport: MarketplaceTransport,
    name: str | None = None,
    config_dir: Path | None = None,
    max_bytes: int | None = None,
) -> ImportReport:
    """Fetch one marketplace entry and import it with no manual download step.

    Fetched bytes are parsed, never executed: they travel from the
    transport straight into the single strict bytes entry point.
    """
    data = fetch_marketplace_bytes(
        entry,
        transport=transport,
        max_bytes=MAX_MARKETPLACE_BYTES if max_bytes is None else max_bytes,
    )
    report = _write_report(
        prepare_marketplace_import(data, entry, name=name, config_dir=config_dir),
        config_dir=config_dir,
    )
    _record_source(report, kind="marketplace", ref=entry.source_id)
    return report


def reload_imported_theme(
    *,
    slug: str,
    source: ImportSource,
    transport: MarketplaceTransport | None = None,
    config_dir: Path | None = None,
    max_bytes: int | None = None,
) -> ImportReport:
    """Re-run the import pipeline for one recorded source without watching it.

    The stored theme is rewritten only after the fresh source validates
    fully; any failure raises before anything is written or applied, so
    the current theme is preserved.
    """
    from talaria.themes.marketplace import UrllibMarketplaceTransport

    if source.slug != slug:
        raise ThemeImportError(
            f"recorded source {source.ref!r} belongs to {source.slug!r}, "
            f"not {slug!r}",
            kind="wrong-root",
        )
    if source.kind == "file":
        path = Path(source.ref).expanduser()
        try:
            data = path.read_bytes()
        except OSError as exc:
            raise ThemeImportError(
                f"{source.ref} could not be read as UTF-8 JSON: {exc}",
                kind="unreadable",
            ) from exc
        label = source.ref
    elif source.kind == "marketplace":
        active = transport if transport is not None else UrllibMarketplaceTransport()
        entry = resolve_marketplace_source(source.ref, transport=active)
        data = fetch_marketplace_bytes(
            entry,
            transport=active,
            max_bytes=MAX_MARKETPLACE_BYTES if max_bytes is None else max_bytes,
        )
        label = entry.download_url
    else:  # pragma: no cover - ImportSource validates kind at construction
        raise ThemeImportError(
            f"recorded source kind {source.kind!r} is not supported",
            kind="wrong-root",
        )
    return _write_report(
        prepare_vscode_theme_import_bytes(
            data, source_label=label, name=slug, config_dir=config_dir
        ),
        config_dir=config_dir,
    )


__all__ = [
    "ALWAYS_FALLBACK_TOKENS",
    "AlphaComposite",
    "ImportReport",
    "ReportLine",
    "SYNTAX_MAPPINGS",
    "ThemeImportError",
    "ThemeImportErrorKind",
    "WORKBENCH_MAPPINGS",
    "import_marketplace_theme",
    "import_vscode_theme",
    "prepare_marketplace_import",
    "prepare_vscode_theme_import",
    "prepare_vscode_theme_import_bytes",
    "reload_imported_theme",
]
