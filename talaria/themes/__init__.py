"""Framework-independent theme specifications.

The canonical token names and their values are plain data.  Textual conversion
belongs in :mod:`talaria.ui.theme`, so importing a built-in theme never pulls a
terminal framework into a non-UI module.
"""

from __future__ import annotations

import re
import unicodedata
from collections.abc import Mapping
from dataclasses import dataclass, field
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
_THEME_NAME_MAX_LENGTH = 128

#: The six transcript categories a theme can style as one group each. The
#: names match the ``talaria.transcript.<category>`` token infix, not the UI
#: ``KindGroup`` spelling: the session-record group reads the ``session``
#: infix. ``talaria/ui/transcript.py`` owns the mapping between the two.
TRANSCRIPT_CATEGORIES: tuple[str, ...] = (
    "operator",
    "assistant",
    "reasoning",
    "activity",
    "session",
    "fault",
)

_TRANSCRIPT_CATEGORY_SET = frozenset(TRANSCRIPT_CATEGORIES)

#: The canonical token each transcript category's group entry may address.
#: ``text`` reinterprets the shared ``talaria.text`` token in that category's
#: scope: a group-level ``talaria.text`` value paints only that category's
#: body text, while the theme-wide ``talaria.text`` override still paints
#: every category that names no group value. ``marker`` and ``background``
#: are the category's existing stripe and fill tokens.
TRANSCRIPT_CATEGORY_TOKENS: Mapping[str, Mapping[str, str]] = MappingProxyType(
    {
        category: MappingProxyType(
            {
                "text": "talaria.text",
                "marker": f"talaria.transcript.{category}",
                "background": f"talaria.transcript.{category}.background",
            }
        )
        for category in TRANSCRIPT_CATEGORIES
    }
)

#: The one non-color value a group entry may carry, and only on its
#: ``background`` role (issue #141, design selection D2): the category's
#: transcript text is painted on ``talaria.canvas`` with no distinct fill,
#: resolved at theme resolution time so a canvas change re-resolves it.
#: The value names a representation, not a color — it wins over the
#: background token value it replaces, which stored themes must still
#: define. Any other role spelling it is malformed and reported.
INHERIT_BACKGROUND: str = "inherit"


def is_transcript_category(name: object) -> bool:
    """True for one of the six known transcript category names."""
    return name in _TRANSCRIPT_CATEGORY_SET


def is_opaque_hex_color(value: object) -> bool:
    """True for an opaque uppercase ``#RRGGBB`` runtime color."""
    return isinstance(value, str) and _OPAQUE_HEX_RE.fullmatch(value) is not None


def relative_luminance(color: str) -> float:
    """Return WCAG 2.2 relative luminance for one opaque ``#RRGGBB`` colour."""
    channels = [int(color[index : index + 2], 16) / 255 for index in (1, 3, 5)]
    linear = [
        channel / 12.92
        if channel <= 0.04045
        else ((channel + 0.055) / 1.055) ** 2.4
        for channel in channels
    ]
    return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]


def contrast_ratio(first: str, second: str) -> float:
    """Measure the WCAG 2.2 contrast ratio between two opaque colours."""
    lighter, darker = sorted(
        (relative_luminance(first), relative_luminance(second)), reverse=True
    )
    return (lighter + 0.05) / (darker + 0.05)

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
    #: Sparse per-category group values: category name to token name to
    #: color. Deliberately lenient where :attr:`tokens` is strict — unknown
    #: categories, unknown tokens, nulls, and malformed colors all fall
    #: through at resolution instead of raising here, so one bad group
    #: entry can never break the rest of the theme. An importer (issue
    #: #124) targets this layer without reimplementing resolution.
    groups: Mapping[str, Mapping[str, str | None]] = field(default_factory=dict)
    #: Whether the transcript's left offset column — the reserved padding
    #: column plus the group gutter stripe — is painted (issue #141's
    #: bar-state rule). Theme-level on purpose: visibility follows the
    #: active theme, never a separate configuration switch, and a theme
    #: that does not name the field keeps the column every prior theme
    #: always had.
    transcript_bar_visible: bool = True

    def __post_init__(self) -> None:
        if not _SLUG_RE.fullmatch(self.slug):
            raise ValueError(f"invalid theme slug: {self.slug!r}")
        if not self.name.strip():
            raise ValueError("theme display name must not be empty")
        if len(self.name) > _THEME_NAME_MAX_LENGTH:
            raise ValueError(
                "theme display name must be at most "
                f"{_THEME_NAME_MAX_LENGTH} characters"
            )
        if any(
            ord(character) < 0x20
            or ord(character) == 0x7F
            or unicodedata.category(character) == "Cf"
            for character in self.name
        ):
            raise ValueError(
                "theme display name must not contain control or format characters"
            )

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
        raw_groups = self.groups
        if not isinstance(raw_groups, Mapping):
            raise ValueError("theme groups must be a mapping of category to token values")
        frozen_groups = {
            category: MappingProxyType(dict(values))
            if isinstance(values, Mapping)
            else MappingProxyType({})
            for category, values in raw_groups.items()
        }
        object.__setattr__(
            self, "groups", MappingProxyType(frozen_groups)
        )
        if not isinstance(self.transcript_bar_visible, bool):
            raise ValueError("theme transcript_bar_visible must be a boolean")


__all__ = [
    "INHERIT_BACKGROUND",
    "THEME_NAME_TYPE_FALLBACK_NOTICE",
    "THEME_TOKENS",
    "THEME_UNAVAILABLE_FALLBACK_NOTICE",
    "TRANSCRIPT_CATEGORIES",
    "TRANSCRIPT_CATEGORY_TOKENS",
    "ThemeSpec",
    "contrast_ratio",
    "is_opaque_hex_color",
    "is_transcript_category",
    "relative_luminance",
    "theme_fallback_notice",
]
