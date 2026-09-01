"""Framework-independent storage contract for imported user themes."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from talaria.themes import THEME_TOKENS, ThemeSpec
from talaria.themes.builtins import BUILTIN_THEMES

STORED_THEME_SCHEMA_VERSION = "talaria-theme-v1"
_REQUIRED_FIELDS = frozenset({"dark", "name", "slug", "tokens"})
_ALLOWED_FIELDS = _REQUIRED_FIELDS | {"schema_version"}


class StoredThemeError(ValueError):
    """A stored user theme is not one of Talaria's canonical theme files."""


def serialize_user_theme(spec: ThemeSpec) -> bytes:
    """Serialize one complete user theme as canonical, deterministic JSON."""
    missing = tuple(token for token in THEME_TOKENS if token not in spec.tokens)
    if missing:
        raise StoredThemeError(
            "stored user themes must define every canonical token: "
            + ", ".join(missing)
        )
    payload = {
        "dark": spec.dark,
        "name": spec.name,
        "schema_version": STORED_THEME_SCHEMA_VERSION,
        "slug": spec.slug,
        "tokens": dict(spec.tokens),
    }
    return (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode("utf-8")


def _reject_json_constant(value: str) -> Any:
    raise ValueError(f"non-JSON numeric constant {value}")


def load_user_theme_spec(path: Path) -> ThemeSpec:
    """Strictly load one stored theme, raising on every invalid document."""
    try:
        source = path.read_text(encoding="utf-8")
        payload = json.loads(source, parse_constant=_reject_json_constant)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, ValueError) as exc:
        raise StoredThemeError(
            f"{path} is not a valid stored user theme: {exc}"
        ) from exc
    if not isinstance(payload, dict):
        raise StoredThemeError(f"{path} stored user theme root must be an object")
    fields = set(payload)
    if not _REQUIRED_FIELDS.issubset(fields) or not fields.issubset(_ALLOWED_FIELDS):
        raise StoredThemeError(f"{path} stored user theme fields are not canonical")

    schema_version = payload.get("schema_version", STORED_THEME_SCHEMA_VERSION)
    if schema_version != STORED_THEME_SCHEMA_VERSION:
        raise StoredThemeError(
            f"{path} stored user theme schema_version {schema_version!r} is not supported"
        )

    dark = payload["dark"]
    name = payload["name"]
    slug = payload["slug"]
    tokens = payload["tokens"]
    if not isinstance(dark, bool):
        raise StoredThemeError(f"{path} stored user theme dark field must be boolean")
    if not isinstance(name, str) or not isinstance(slug, str):
        raise StoredThemeError(
            f"{path} stored user theme name and slug must be strings"
        )
    if not isinstance(tokens, dict):
        raise StoredThemeError(f"{path} stored user theme tokens must be an object")
    if any(
        not isinstance(token, str) or not isinstance(value, str)
        for token, value in tokens.items()
    ):
        raise StoredThemeError(
            f"{path} stored user theme token names and values must be strings"
        )
    if path.stem != slug:
        raise StoredThemeError(f"{path} filename does not match stored slug {slug!r}")
    try:
        spec = ThemeSpec(slug=slug, name=name, dark=dark, tokens=tokens)
    except ValueError as exc:
        raise StoredThemeError(
            f"{path} is not a valid stored user theme: {exc}"
        ) from exc
    try:
        serialize_user_theme(spec)
    except StoredThemeError as exc:
        raise StoredThemeError(f"{path} {exc}") from exc
    builtins = frozenset(theme.slug for theme in BUILTIN_THEMES)
    if spec.slug in builtins:
        raise StoredThemeError(
            f"{path} stored user theme cannot replace built-in {spec.slug!r}"
        )
    return spec


def load_user_theme_specs(
    *, config_dir: Path
) -> tuple[tuple[ThemeSpec, ...], tuple[str, ...]]:
    """Load valid themes and visibly skip invalid files for one fresh run."""
    themes_dir = config_dir / "themes"
    if not themes_dir.is_dir():
        return (), ()
    specs: list[ThemeSpec] = []
    notices: list[str] = []
    for path in sorted(themes_dir.glob("*.json")):
        try:
            specs.append(load_user_theme_spec(path))
        except StoredThemeError as exc:
            notices.append(f"stored user theme was skipped: {exc}")
    return tuple(specs), tuple(notices)


__all__ = [
    "STORED_THEME_SCHEMA_VERSION",
    "StoredThemeError",
    "load_user_theme_spec",
    "load_user_theme_specs",
    "serialize_user_theme",
]
