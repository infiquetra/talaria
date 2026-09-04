"""Talaria theme resolution and the Textual 8.2.8 bridge."""

from __future__ import annotations

from collections.abc import Iterable, Mapping
from dataclasses import dataclass, field
from pathlib import Path
from types import MappingProxyType
from typing import Final, Protocol

from textual.theme import Theme

from talaria.config import atomic_replace_bytes, global_config_dir
from talaria.themes import (
    THEME_TOKENS,
    TRANSCRIPT_CATEGORIES,
    TRANSCRIPT_CATEGORY_TOKENS,
    ThemeSpec,
    contrast_ratio,
    is_opaque_hex_color,
    relative_luminance,
    theme_fallback_notice,
)
from talaria.themes.builtins import BUILTIN_THEMES, REFINED_DEFAULT
from talaria.themes.host_palette import apply_host_palette
from talaria.themes.storage import (
    StoredThemeError,
    load_user_theme_specs,
    serialize_user_theme,
)

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


#: The readability floor for per-category transcript body text: the same
#: 4.5 text minimum the visual specification measures every other body
#: pairing against.
TRANSCRIPT_TEXT_CONTRAST_FLOOR: Final[float] = 4.5

#: The readability floor for per-category transcript stripe markers: the
#: same 3.0 non-text component minimum the specification measures the
#: marker/fill pairings against.
TRANSCRIPT_MARKER_CONTRAST_FLOOR: Final[float] = 3.0


def _hold_contrast_floor(
    foreground: str,
    background: str,
    floor: float,
    fallback: str,
) -> tuple[str, bool]:
    """Return a foreground holding ``floor`` against ``background``.

    The second element reports whether the requested foreground was kept.
    A failing foreground falls back to ``fallback`` when it holds, else to
    whichever of black or white holds — one of the two always does: below
    a luminance of 0.175 white exceeds 4.5, above it black does, so the
    floor is a guarantee, never an aspiration.
    """
    if contrast_ratio(foreground, background) >= floor:
        return foreground, True
    if contrast_ratio(fallback, background) >= floor:
        return fallback, False
    white = contrast_ratio("#FFFFFF", background)
    black = contrast_ratio("#000000", background)
    return ("#FFFFFF" if white >= black else "#000000"), False


@dataclass(frozen=True)
class TranscriptCategoryStyle:
    """One transcript category's resolved text, stripe, and fill colors."""

    text: str
    marker: str
    background: str


@dataclass(frozen=True)
class ResolvedTheme:
    """A complete theme plus every visible normalization result."""

    spec: ThemeSpec
    tokens: Mapping[str, str]
    filled_tokens: tuple[str, ...] = ()
    notices: tuple[str, ...] = ()
    transcript_text: Mapping[str, str] = field(default_factory=dict)

    def __post_init__(self) -> None:
        object.__setattr__(self, "tokens", MappingProxyType(dict(self.tokens)))
        object.__setattr__(
            self, "transcript_text", MappingProxyType(dict(self.transcript_text))
        )

    def category_style(self, category: str) -> TranscriptCategoryStyle:
        """The resolved text/stripe/fill triple for one transcript category.

        Unknown categories fall back to the plain-surface default — global
        body text on the default surface — rather than raising, so a caller
        with a category this theme never named still renders legibly.
        """
        role_tokens = TRANSCRIPT_CATEGORY_TOKENS.get(category)
        if role_tokens is None:
            return TranscriptCategoryStyle(
                text=self.tokens["talaria.text"],
                marker=self.tokens["talaria.text.muted"],
                background=self.tokens["talaria.surface"],
            )
        return TranscriptCategoryStyle(
            text=self.transcript_text.get(category, self.tokens["talaria.text"]),
            marker=self.tokens[role_tokens["marker"]],
            background=self.tokens[role_tokens["background"]],
        )

    @property
    def slug(self) -> str:
        return self.spec.slug

    @property
    def name(self) -> str:
        return self.spec.name

    @property
    def dark(self) -> bool:
        return self.spec.dark


def transcript_text_variable(category: str) -> str:
    """Return the Textual custom-variable name for one category body text."""
    return f"talaria-transcript-{category}-text"


def _category_group_roles(
    groups: Mapping[str, Mapping[str, str | None]],
    category: str,
) -> tuple[dict[str, str], list[str]]:
    """Read one category's usable group roles and its malformed entries.

    Both the role nicknames (``text``, ``marker``, ``background``) and the
    canonical token names spell the same three roles; the canonical spelling
    wins when both name one role. Unknown keys, nulls, non-mapping entries,
    and malformed colors fall through — malformed colors are reported so the
    operator can see them, everything else is silently unset.
    """
    roles = TRANSCRIPT_CATEGORY_TOKENS[category]
    entry = groups.get(category)
    if not isinstance(entry, Mapping):
        return {}, []
    usable: dict[str, str] = {}
    malformed: list[str] = []
    canonical = {token: role for role, token in roles.items()}

    def claim(role: str, key: str, value: object) -> None:
        if role in usable or value is None:
            # JSON null spells "unset", the sparse layer's favorite word.
            return
        if isinstance(value, str) and is_opaque_hex_color(value):
            usable[role] = value
        else:
            malformed.append(f"{category}.{key}")

    for key, value in entry.items():
        if key in canonical:
            claim(canonical[key], str(key), value)
    for key, value in entry.items():
        if key in ("text", "marker", "background"):
            claim(str(key), str(key), value)
    return usable, malformed


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

    def resolve(
        self, requested: object, *, host_palette: object = None
    ) -> ResolvedTheme:
        """Resolve one requested slug and describe every fallback visibly.

        The single funnel every theme passes through, so built-in, user,
        and imported specifications share one inheritance semantics:
        shared defaults, then the host palette where terminal colors apply,
        then per-category groups, then individual overrides. Each layer is
        sparse — an empty, unknown, null, or malformed entry falls through
        to the next layer instead of breaking the rest.
        """
        notices: list[str] = []
        if not isinstance(requested, str):
            spec = self.default
            notices.append(theme_fallback_notice(requested, self._default_slug))
        else:
            spec = self._by_slug.get(requested, self.default)
            if spec is self.default and requested != self._default_slug:
                notices.append(theme_fallback_notice(requested, self._default_slug))

        default_tokens = self.default.tokens
        adopted: tuple[str, ...] = ()
        if host_palette is None:
            tokens = {
                token: spec.tokens.get(token, default_tokens[token])
                for token in THEME_TOKENS
            }
        else:
            hosted = apply_host_palette(
                default_tokens, host_palette, overrides=spec.tokens
            )
            tokens = {
                token: hosted.tokens.get(token, default_tokens[token])
                for token in THEME_TOKENS
            }
            adopted = hosted.adopted_tokens
            notices.extend(hosted.notices)
            self._hold_host_readability(spec, tokens, adopted, notices)

        group_sourced: list[str] = []
        malformed_groups: list[str] = []
        category_roles: dict[str, dict[str, str]] = {}
        for category in TRANSCRIPT_CATEGORIES:
            usable, malformed = _category_group_roles(spec.groups, category)
            category_roles[category] = usable
            malformed_groups.extend(malformed)
            roles = TRANSCRIPT_CATEGORY_TOKENS[category]
            for role in ("marker", "background"):
                token = roles[role]
                if token not in spec.tokens and role in usable:
                    tokens[token] = usable[role]
                    group_sourced.append(token)
        if group_sourced:
            notices.append(
                f"theme {spec.name!r} styles transcript categories from its groups: "
                + ", ".join(sorted(group_sourced))
            )
        if malformed_groups:
            notices.append(
                f"theme {spec.name!r} ignores malformed group colors: "
                + ", ".join(sorted(malformed_groups))
            )

        filled = tuple(
            token
            for token in THEME_TOKENS
            if token not in spec.tokens
            and token not in adopted
            and token not in group_sourced
        )
        if filled:
            notices.append(
                f"theme {spec.name!r} filled missing tokens from Refined Default: "
                + ", ".join(filled)
            )

        transcript_text = self._resolve_category_text(
            spec, tokens, category_roles, notices
        )
        return ResolvedTheme(
            spec=spec,
            tokens=tokens,
            filled_tokens=filled,
            notices=tuple(notices),
            transcript_text=transcript_text,
        )

    def _hold_host_readability(
        self,
        spec: ThemeSpec,
        tokens: dict[str, str],
        adopted: tuple[str, ...],
        notices: list[str],
    ) -> None:
        """Revert inherited host values that break the readability floor.

        Only host-adopted tokens are ever reverted — explicit Talaria
        overrides stay exactly as specified, even when they are the pair
        that fails. Each reverted token returns to the shared default with
        a visible notice.
        """
        adopted_set = frozenset(adopted)
        pairs = (
            ("talaria.text", "talaria.canvas", TRANSCRIPT_TEXT_CONTRAST_FLOOR),
            (
                "talaria.selection.text",
                "talaria.selection.background",
                TRANSCRIPT_TEXT_CONTRAST_FLOOR,
            ),
        )
        for foreground_token, background_token, floor in pairs:
            foreground = tokens[foreground_token]
            background = tokens[background_token]
            if contrast_ratio(foreground, background) >= floor:
                continue
            reverted = sorted(
                token
                for token in (foreground_token, background_token)
                if token in adopted_set
            )
            if not reverted:
                continue
            default_tokens = self.default.tokens
            for token in reverted:
                tokens[token] = default_tokens[token]
            notices.append(
                f"theme {spec.name!r} keeps the readability floor: host values "
                f"for {', '.join(reverted)} reverted to the built-in mapping"
            )

    def _resolve_category_text(
        self,
        spec: ThemeSpec,
        tokens: dict[str, str],
        category_roles: Mapping[str, Mapping[str, str]],
        notices: list[str],
    ) -> dict[str, str]:
        """Resolve each category's body text against the readability floor.

        The requested value is the category's group text when present, else
        the theme's shared body text. A value below the floor falls back to
        the shared text, then the default theme's text, then black or white
        — so an unreadable combination resolves to the floor rather than
        rendering. Stripe markers hold their own component floor the same
        way, falling back to the default theme's marker first.
        """
        default_tokens = self.default.tokens
        resolved: dict[str, str] = {}
        held: list[str] = []
        for category in TRANSCRIPT_CATEGORIES:
            roles = TRANSCRIPT_CATEGORY_TOKENS[category]
            background = tokens[roles["background"]]
            requested = category_roles[category].get("text", tokens["talaria.text"])
            fallback = (
                default_tokens["talaria.text"]
                if requested == tokens["talaria.text"]
                else tokens["talaria.text"]
            )
            held_text, text_kept = _hold_contrast_floor(
                requested,
                background,
                TRANSCRIPT_TEXT_CONTRAST_FLOOR,
                fallback,
            )
            resolved[category] = held_text
            if not text_kept:
                held.append(f"{category}.text")
            marker_token = roles["marker"]
            held_marker, marker_kept = _hold_contrast_floor(
                tokens[marker_token],
                background,
                TRANSCRIPT_MARKER_CONTRAST_FLOOR,
                default_tokens[marker_token],
            )
            if not marker_kept:
                tokens[marker_token] = held_marker
                held.append(f"{category}.marker")
        if held:
            notices.append(
                f"theme {spec.name!r} holds the readability floor instead of "
                f"the requested values: {', '.join(sorted(held))}"
            )
        return resolved

    def to_textual_theme(self, requested: object) -> Theme:
        """Convert one resolved theme to Textual's registered theme shape."""
        resolved = requested if isinstance(requested, ResolvedTheme) else self.resolve(requested)
        tokens = resolved.tokens
        variables = {
            textual_variable_name(token): value for token, value in tokens.items()
        }
        for category in TRANSCRIPT_CATEGORIES:
            variables[transcript_text_variable(category)] = resolved.category_style(
                category
            ).text
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


def user_theme_path(slug: str, *, config_dir: Path | None = None) -> Path:
    """Return the user-theme target for an already validated theme slug."""
    try:
        ThemeSpec(slug=slug, name=slug, dark=False, tokens={})
    except ValueError as exc:
        raise StoredThemeError(str(exc)) from exc
    root = config_dir if config_dir is not None else global_config_dir()
    return root / "themes" / f"{slug}.json"


def write_user_theme(spec: ThemeSpec, *, config_dir: Path | None = None) -> Path:
    """Atomically create or replace one complete user theme."""
    path = user_theme_path(spec.slug, config_dir=config_dir)
    atomic_replace_bytes(path, serialize_user_theme(spec))
    return path


def theme_registry_for_config(*, config_dir: Path | None = None) -> ThemeRegistry:
    """Build the restart-scoped registry from built-ins and stored user themes."""
    root = config_dir if config_dir is not None else global_config_dir()
    specs, _notices = load_user_theme_specs(config_dir=root)
    return ThemeRegistry((*BUILTIN_THEMES, *specs))

__all__ = [
    "BUILTIN_THEME_REGISTRY",
    "DEFAULT_THEME_SLUG",
    "TRANSCRIPT_MARKER_CONTRAST_FLOOR",
    "TRANSCRIPT_TEXT_CONTRAST_FLOOR",
    "ResolvedTheme",
    "StoredThemeError",
    "ThemeRegistry",
    "TranscriptCategoryStyle",
    "contrast_ratio",
    "relative_luminance",
    "serialize_user_theme",
    "theme_registry_for_config",
    "textual_variable_name",
    "transcript_text_variable",
    "user_theme_path",
    "write_user_theme",
]
