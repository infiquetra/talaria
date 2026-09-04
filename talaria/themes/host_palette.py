"""Host terminal palette inheritance for Talaria themes (decision C).

A host terminal theme defines flat chrome colors — a background, a
foreground, a selection, a few borders — not Talaria's semantic content
channels. Inheriting the host palette therefore applies only where terminal
colors meaningfully apply: the flat application surfaces. The transcript
markers, diff glyphs, and syntax classes stay Talaria-owned, so a host
palette can never silently recolor a meaning-carrying channel, and an
explicit Talaria override always wins over an inherited value.

The entry point is :func:`apply_host_palette`, a pure function over plain
data so the domain-core boundary check keeps passing: importing this module
never pulls the terminal framework in. Unresolvable input (no palette, a
non-mapping, or nothing usable inside it) degrades to the base mapping with
a visible notice — never a crash, never a blank theme.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from types import MappingProxyType
from typing import Final

from talaria.themes import THEME_TOKENS, is_opaque_hex_color

#: Tokens where a host terminal palette meaningfully applies: flat chrome
#: surfaces with a direct terminal analog (background, foreground,
#: selection, status fill, inspector chrome). Everything semantic —
#: transcript markers and fills, diff glyphs and fills, syntax classes —
#: is Talaria-specific and is never inherited, which is what "Talaria-
#: specific overrides preserved" means at the token level: there is no host
#: value that can reach those tokens through this function.
HOST_INHERITED_TOKENS: Final[tuple[str, ...]] = (
    "talaria.canvas",
    "talaria.surface",
    "talaria.panel",
    "talaria.text",
    "talaria.text.muted",
    "talaria.border",
    "talaria.border.muted",
    "talaria.focus",
    "talaria.selection.background",
    "talaria.selection.text",
    "talaria.status.background",
    "talaria.status.text",
    "talaria.status.muted",
    "talaria.status.separator",
    "talaria.inspector.background",
    "talaria.inspector.border",
)

_HOST_INHERITED_SET = frozenset(HOST_INHERITED_TOKENS)

_HOST_UNAVAILABLE_NOTICE: Final[str] = (
    "host terminal palette is unavailable; using the built-in mapping"
)


@dataclass(frozen=True)
class HostPaletteResult:
    """One host-palette inheritance attempt and its visible accounting."""

    tokens: Mapping[str, str]
    notices: tuple[str, ...] = ()
    used_host: bool = False
    adopted_tokens: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        object.__setattr__(self, "tokens", MappingProxyType(dict(self.tokens)))

    @property
    def adopted(self) -> tuple[str, ...]:
        """The inherited tokens, in canonical order."""
        return self.adopted_tokens


def _sanitized_host_entries(host: Mapping[object, object]) -> dict[str, str]:
    """Keep the usable host entries: known inheritable tokens, valid colors."""
    usable: dict[str, str] = {}
    for token, value in host.items():
        if token not in _HOST_INHERITED_SET:
            continue
        if not is_opaque_hex_color(value):
            continue
        usable[str(token)] = str(value)
    return usable


def apply_host_palette(
    base_tokens: Mapping[str, str],
    host: object,
    *,
    overrides: Mapping[str, object] | None = None,
) -> HostPaletteResult:
    """Layer a host palette between shared defaults and Talaria overrides.

    Resolution order per token: ``overrides`` (explicit Talaria values)
    beat ``host`` (inherited, only for :data:`HOST_INHERITED_TOKENS`) beat
    ``base_tokens`` (the shared-default mapping the caller passes, usually
    Refined Default). Sparse input stays sparse: a missing, null, unknown,
    or malformed host entry falls through to the base value, and one bad
    entry never breaks the rest. A non-mapping host degrades to the base
    mapping with a notice.
    """
    base = {token: base_tokens[token] for token in THEME_TOKENS if token in base_tokens}
    valid_overrides: dict[str, str] = {}
    if isinstance(overrides, Mapping):
        for token, value in overrides.items():
            if isinstance(token, str) and token in frozenset(THEME_TOKENS):
                if is_opaque_hex_color(value):
                    valid_overrides[token] = str(value)

    if not isinstance(host, Mapping):
        merged = dict(base)
        merged.update(valid_overrides)
        return HostPaletteResult(
            tokens=merged,
            notices=(_HOST_UNAVAILABLE_NOTICE,),
            used_host=False,
        )

    usable = _sanitized_host_entries(host)
    merged = dict(base)
    adopted: list[str] = []
    for token in HOST_INHERITED_TOKENS:
        if token in usable and token in merged and token not in valid_overrides:
            merged[token] = usable[token]
            adopted.append(token)
    # Explicit Talaria overrides win over every inherited value, on any
    # token — that is what keeps them overrides rather than suggestions.
    merged.update(valid_overrides)

    notices: list[str] = []
    if adopted:
        notices.append(
            "inherited host terminal colors for: " + ", ".join(adopted)
        )
    else:
        notices.append(_HOST_UNAVAILABLE_NOTICE + " (no usable host entries)")
    ignored = sorted(
        str(token)
        for token in host
        if token not in usable and token not in valid_overrides
    )
    if ignored:
        notices.append(
            "host palette entries ignored (not inheritable or not a color): "
            + ", ".join(ignored)
        )
    return HostPaletteResult(
        tokens=merged,
        notices=tuple(notices),
        used_host=bool(adopted),
        adopted_tokens=tuple(adopted),
    )


__all__ = [
    "HOST_INHERITED_TOKENS",
    "HostPaletteResult",
    "apply_host_palette",
]
