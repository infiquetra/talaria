"""Talaria theme resolution and the Textual 8.2.8 bridge."""

from __future__ import annotations

from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from types import MappingProxyType
from typing import Final, Protocol

from textual.theme import Theme

from talaria.themes import THEME_TOKENS, ThemeSpec
from talaria.themes.builtins import BUILTIN_THEMES, REFINED_DEFAULT

DEFAULT_THEME_SLUG: Final[str] = REFINED_DEFAULT.slug


class ThemeRegistrar(Protocol):
    """The one application operation theme registration requires."""

    def register_theme(self, theme: Theme) -> None: ...

# Textual variable names omit the leading dollar sign used in CSS.  Theme fields
# are also repeated here deliberately: Textual derives many compatibility
# variables, while the visual contract requires these exact measured values.
_COMPATIBILITY_VARIABLES: Final[Mapping[str, tuple[str, ...]]] = {
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


def textual_variable_name(token: str) -> str:
    """Return the Textual custom-variable name for one canonical token."""
    return token.replace(".", "-")


@dataclass(frozen=True)
class ResolvedTheme:
    """A complete theme plus every visible normalization result."""

    spec: ThemeSpec
    tokens: Mapping[str, str]
    filled_tokens: tuple[str, ...] = ()
    notices: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        object.__setattr__(self, "tokens", MappingProxyType(dict(self.tokens)))

    @property
    def slug(self) -> str:
        return self.spec.slug

    @property
    def name(self) -> str:
        return self.spec.name

    @property
    def dark(self) -> bool:
        return self.spec.dark


class ThemeRegistry:
    """Resolve complete or partial theme specifications against one fallback."""

    def __init__(
        self,
        specs: Iterable[ThemeSpec],
        *,
        default_slug: str = DEFAULT_THEME_SLUG,
    ) -> None:
        ordered = tuple(specs)
        by_slug = {spec.slug: spec for spec in ordered}
        if len(by_slug) != len(ordered):
            raise ValueError("theme slugs must be unique")
        if default_slug not in by_slug:
            raise ValueError(f"default theme {default_slug!r} is not registered")
        default = by_slug[default_slug]
        missing_default = tuple(token for token in THEME_TOKENS if token not in default.tokens)
        if missing_default:
            raise ValueError("the default theme must define all canonical tokens")

        self._specs = ordered
        self._by_slug = MappingProxyType(by_slug)
        self._default_slug = default_slug

    @property
    def specs(self) -> tuple[ThemeSpec, ...]:
        return self._specs

    @property
    def slugs(self) -> tuple[str, ...]:
        return tuple(spec.slug for spec in self._specs)

    @property
    def default(self) -> ThemeSpec:
        return self._by_slug[self._default_slug]

    def resolve(self, requested: object) -> ResolvedTheme:
        """Resolve one requested slug and describe every fallback visibly."""
        notices: list[str] = []
        if not isinstance(requested, str):
            spec = self.default
            notices.append(
                "theme.name must be a string; using Refined Default "
                f"({self._default_slug})"
            )
        else:
            spec = self._by_slug.get(requested, self.default)
            if spec is self.default and requested != self._default_slug:
                notices.append(
                    f"theme {requested!r} is not available; using Refined Default "
                    f"({self._default_slug})"
                )

        filled = tuple(token for token in THEME_TOKENS if token not in spec.tokens)
        tokens = {
            token: spec.tokens.get(token, self.default.tokens[token])
            for token in THEME_TOKENS
        }
        if filled:
            notices.append(
                f"theme {spec.name!r} filled missing tokens from Refined Default: "
                + ", ".join(filled)
            )
        return ResolvedTheme(
            spec=spec,
            tokens=tokens,
            filled_tokens=filled,
            notices=tuple(notices),
        )

    def to_textual_theme(self, requested: object) -> Theme:
        """Convert one resolved theme to Textual's registered theme shape."""
        resolved = requested if isinstance(requested, ResolvedTheme) else self.resolve(requested)
        tokens = resolved.tokens
        variables = {
            textual_variable_name(token): value for token, value in tokens.items()
        }
        for token, names in _COMPATIBILITY_VARIABLES.items():
            value = tokens[token]
            variables.update({name: value for name in names})

        return Theme(
            name=resolved.slug,
            primary=tokens["talaria.primary"],
            secondary=tokens["talaria.secondary"],
            warning=tokens["talaria.warning"],
            error=tokens["talaria.error"],
            success=tokens["talaria.success"],
            accent=tokens["talaria.accent"],
            foreground=tokens["talaria.text"],
            background=tokens["talaria.canvas"],
            surface=tokens["talaria.surface"],
            panel=tokens["talaria.panel"],
            dark=resolved.dark,
            text_alpha=1.0,
            variables=variables,
            ansi=False,
        )

    def register(self, app: ThemeRegistrar) -> None:
        """Register every specification under its stable slug."""
        for spec in self._specs:
            app.register_theme(self.to_textual_theme(spec.slug))


BUILTIN_THEME_REGISTRY = ThemeRegistry(BUILTIN_THEMES)

__all__ = [
    "BUILTIN_THEME_REGISTRY",
    "DEFAULT_THEME_SLUG",
    "ResolvedTheme",
    "ThemeRegistry",
    "textual_variable_name",
]
