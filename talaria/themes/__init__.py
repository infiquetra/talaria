"""Framework-independent theme specifications.

The canonical token names and their values are plain data.  Textual conversion
belongs in :mod:`talaria.ui.theme`, so importing a built-in theme never pulls a
terminal framework into a non-UI module.
"""

from __future__ import annotations

import re
from collections.abc import Mapping
from dataclasses import dataclass
from types import MappingProxyType

THEME_TOKENS: tuple[str, ...] = (
    "talaria.canvas",
    "talaria.surface",
    "talaria.panel",
    "talaria.text",
    "talaria.text.muted",
    "talaria.primary",
    "talaria.secondary",
    "talaria.accent",
    "talaria.success",
    "talaria.warning",
    "talaria.error",
    "talaria.border",
    "talaria.border.muted",
    "talaria.focus",
    "talaria.selection.background",
    "talaria.selection.text",
    "talaria.status.background",
    "talaria.status.text",
    "talaria.status.muted",
    "talaria.status.separator",
    "talaria.status.success",
    "talaria.status.warning",
    "talaria.status.error",
    "talaria.status.attention",
    "talaria.inspector.background",
    "talaria.inspector.border",
    "talaria.inspector.heading",
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
    "talaria.diff.context",
    "talaria.diff.line-number",
    "talaria.diff.added",
    "talaria.diff.added.background",
    "talaria.diff.removed",
    "talaria.diff.removed.background",
    "talaria.diff.hunk",
    "talaria.diff.hunk.background",
    "talaria.diff.intraline-added.background",
    "talaria.diff.intraline-removed.background",
    "talaria.syntax.comment",
    "talaria.syntax.keyword",
    "talaria.syntax.string",
    "talaria.syntax.number",
    "talaria.syntax.function",
    "talaria.syntax.type",
    "talaria.syntax.variable",
    "talaria.syntax.operator",
    "talaria.syntax.constant",
)

_TOKEN_SET = frozenset(THEME_TOKENS)
_SLUG_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
_OPAQUE_HEX_RE = re.compile(r"^#[0-9A-F]{6}$")

THEME_NAME_TYPE_FALLBACK_NOTICE = (
    "theme.name must be a string; using Refined Default ({default_slug})"
)
THEME_UNAVAILABLE_FALLBACK_NOTICE = (
    "theme {requested!r} is not available; using Refined Default ({default_slug})"
)


def theme_fallback_notice(requested: object, default_slug: str) -> str:
    """Format the one operator notice shared by both theme resolution paths."""
    template = (
        THEME_UNAVAILABLE_FALLBACK_NOTICE
        if isinstance(requested, str)
        else THEME_NAME_TYPE_FALLBACK_NOTICE
    )
    return template.format(requested=requested, default_slug=default_slug)


@dataclass(frozen=True)
class ThemeSpec:
    """One named theme before missing canonical tokens are filled.

    Partial specifications are intentional: the bounded importer added by issue
    #105 will construct them, and the registry fills their missing values from
    Refined Default.  Unknown tokens and non-runtime color forms are rejected at
    this boundary instead of being silently ignored by Textual.
    """

    slug: str
    name: str
    dark: bool
    tokens: Mapping[str, str]

    def __post_init__(self) -> None:
        if not _SLUG_RE.fullmatch(self.slug):
            raise ValueError(f"invalid theme slug: {self.slug!r}")
        if not self.name.strip():
            raise ValueError("theme display name must not be empty")

        copied = dict(self.tokens)
        unknown = sorted(set(copied) - _TOKEN_SET)
        if unknown:
            raise ValueError(f"unknown Talaria theme tokens: {', '.join(unknown)}")
        invalid = sorted(
            token for token, value in copied.items() if not _OPAQUE_HEX_RE.fullmatch(value)
        )
        if invalid:
            raise ValueError(
                "theme token values must be opaque uppercase #RRGGBB: "
                + ", ".join(invalid)
            )
        object.__setattr__(self, "tokens", MappingProxyType(copied))


__all__ = [
    "THEME_NAME_TYPE_FALLBACK_NOTICE",
    "THEME_TOKENS",
    "THEME_UNAVAILABLE_FALLBACK_NOTICE",
    "ThemeSpec",
    "theme_fallback_notice",
]
