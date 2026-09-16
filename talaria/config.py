"""talaria/config.py — KTD15's configuration precedence chain.

The only module in this repository that reads settings from the filesystem,
and the owner of Talaria's narrow explicit writes: the theme selection and the
configuration view's status keys (issue #149, D8 recorded).
Contents live under ``~/.talaria/`` (relocatable for tests via
``TALARIA_CONFIG_DIR``): ``config.toml`` for settings, ``credentials`` for the
attach credential, and ``recordings/`` for frame logs.

This module only *computes* the credential path. Creating that file, enforcing
its mode 0600, and reading it belong to U7's credential provider (KTD11) — no
code here opens it, so nothing here can leak its contents.

General precedence, highest first: an explicit command-line override, a ``TALARIA_*``
environment variable, a repo-local ``./.talaria/config.toml``, the global
``~/.talaria/config.toml`` (or its ``TALARIA_CONFIG_DIR`` redirection), and the
built-in default. Consumers (U5's status runner, U6, U7's credential provider)
call :func:`load_config` rather than touching the filesystem themselves.
``theme.name`` deliberately has no environment or command-line override; an
explicit selection in the UI persists it to user scope immediately via :func:`save_theme`,
while repository persistence is requested via explicit save.

**Consumer contract.** The resolved snapshot is deeply immutable, which changes
the types a caller gets back: every mapping is a ``MappingProxyType`` and every
list-declared setting — ``environment.allowlist``, for instance — is returned as
a **tuple**, whether it came from :data:`DEFAULTS`, a TOML file, or a CLI
override. So ``cfg.get("environment", "allowlist") == []`` is False, and
``json.dumps(cfg.values)`` raises ``TypeError``. Rebuild plain containers
explicitly when serializing; ``dict(cfg.values)`` is a shallow unwrap and leaves
nested sections as proxies.
"""

from __future__ import annotations

import os
import re
import stat
import tempfile
import tomllib
from collections.abc import Mapping, Sequence
from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import Any, Literal
from urllib.parse import urlsplit

from talaria.status.contract import (
    DEFAULT_AGENT_MODEL_MAX_COLUMNS,
    DEFAULT_CWD_MAX_COLUMNS,
    DEFAULT_GIT_BRANCH_MAX_COLUMNS,
    DEFAULT_STATUS_SEGMENTS,
    KNOWN_STATUS_SEGMENTS,
    normalize_status_settings,
    parse_command,
)
from talaria.themes import theme_fallback_notice
from talaria.themes.builtins import BUILTIN_THEMES, REFINED_DEFAULT
from talaria.themes.storage import load_user_theme_specs


class ConfigError(Exception):
    """A configuration source or surgical configuration write is invalid.

    Carries the offending file or environment variable in its message so the
    operator is pointed at the cause rather than at a traceback inside this
    module.
    """


#: Default chord for the session-inspector toggle (v0.6.0 #120, decision A).
DEFAULT_INSPECTOR_KEY = "ctrl+o"
#: Default chord for cancelling the in-flight turn (#120, decision B).
DEFAULT_INTERRUPT_KEY = "ctrl+s"
#: D12 remaining named app bindings — each default is the chord the action
#: already has in ``talaria/ui/app.py``.
DEFAULT_AGENTS_KEY = "ctrl+g"
DEFAULT_COMMANDS_KEY = "f3"
DEFAULT_MODELS_KEY = "f11"
DEFAULT_PROFILES_KEY = "f12"
DEFAULT_CONFIG_KEY = "ctrl+k"
DEFAULT_FOLLOW_KEY = "f5"
DEFAULT_REPLAY_PAUSE_KEY = "f8"
DEFAULT_REPLAY_SLOWER_KEY = "f9"
DEFAULT_REPLAY_FASTER_KEY = "f10"
#: The replaced inspector default. Herdr captures ``Ctrl+B`` before Talaria
#: sees it when nested, so it stays named here as the replaced default — it is
#: documentation, not a binding, and nothing binds it anymore.
REPLACED_INSPECTOR_KEY = "ctrl+b"
#: Keys a ``[keys]`` override may never claim. ``Ctrl+Q`` is the quit anchor,
#: and a cancel press must never quit the client (#120 U2) — so a configured
#: chord that collides with it falls back with a notice instead of binding.
RESERVED_KEYS = frozenset({"ctrl+q"})

#: Modifiers Talaria recognizes in a configured chord.
_KEY_MODIFIERS = frozenset({"ctrl", "alt", "shift", "super"})
#: Multi-character key names Talaria recognizes. Single alphanumeric keys and
#: ``f1``–``f12`` are accepted without being listed; anything else falls back
#: with a notice rather than binding a dead key that fails silently.
_KEY_NAMES = frozenset(
    {
        "enter",
        "escape",
        "tab",
        "backspace",
        "delete",
        "insert",
        "home",
        "end",
        "up",
        "down",
        "left",
        "right",
        "pageup",
        "pagedown",
        "space",
    }
)
_KEY_SHAPE_RE = re.compile(r"^[a-z0-9]+(\+[a-z0-9]+)*$")
_KEY_FUNCTION_RE = re.compile(r"^f([1-9]|1[0-2])$")


def _valid_key(value: Any) -> str | None:
    """The normalized key name when ``value`` names a bindable key, else None.

    Normalization is lowercase-and-strip, so ``Ctrl+O`` configures ``ctrl+o``.
    Anything that is not a string, is empty, has an unknown modifier, or ends
    in a key name outside the recognized set answers None, and the caller
    falls back to the default with a notice.
    """
    if not isinstance(value, str):
        return None
    lowered = value.strip().lower()
    if not lowered or _KEY_SHAPE_RE.fullmatch(lowered) is None:
        return None
    *modifiers, final = lowered.split("+")
    if any(modifier not in _KEY_MODIFIERS for modifier in modifiers):
        return None
    if len(final) == 1 and final.isalnum():
        return lowered
    if _KEY_FUNCTION_RE.fullmatch(final) is not None:
        return lowered
    if final in _KEY_NAMES:
        return lowered
    return None


#: Ordered ``[keys]`` defaults. Collision checks walk this map so a new
#: bindable action cannot be added to DEFAULTS without joining the rule.
KEY_SETTING_DEFAULTS: dict[str, str] = {
    "toggle_inspector": DEFAULT_INSPECTOR_KEY,
    "interrupt": DEFAULT_INTERRUPT_KEY,
    "agents": DEFAULT_AGENTS_KEY,
    "commands": DEFAULT_COMMANDS_KEY,
    "models": DEFAULT_MODELS_KEY,
    "profiles": DEFAULT_PROFILES_KEY,
    "config": DEFAULT_CONFIG_KEY,
    "follow": DEFAULT_FOLLOW_KEY,
    "replay_pause": DEFAULT_REPLAY_PAUSE_KEY,
    "replay_slower": DEFAULT_REPLAY_SLOWER_KEY,
    "replay_faster": DEFAULT_REPLAY_FASTER_KEY,
}


@dataclass(frozen=True)
class KeyBindings:
    """The resolved bindable chords after fallback normalization."""

    toggle_inspector: str
    interrupt: str
    agents: str = DEFAULT_AGENTS_KEY
    commands: str = DEFAULT_COMMANDS_KEY
    models: str = DEFAULT_MODELS_KEY
    profiles: str = DEFAULT_PROFILES_KEY
    config: str = DEFAULT_CONFIG_KEY
    follow: str = DEFAULT_FOLLOW_KEY
    replay_pause: str = DEFAULT_REPLAY_PAUSE_KEY
    replay_slower: str = DEFAULT_REPLAY_SLOWER_KEY
    replay_faster: str = DEFAULT_REPLAY_FASTER_KEY


def resolve_keybindings(
    raw: Mapping[str, Any] | None,
) -> tuple[KeyBindings, tuple[str, ...]]:
    """Resolve a ``[keys]`` mapping to chords, falling back safely.

    Every failure mode — a missing table, an empty, null, or non-string value,
    an unknown key name, a reserved collision, or two actions assigned the
    same chord — resolves to a default and is named in the returned notices.
    A duplicate assignment resets *both* actions to their defaults: falling
    back only one of them could land on the other's still-duplicate value.
    """
    source = dict(raw) if isinstance(raw, Mapping) else {}
    notices: list[str] = []
    resolved: dict[str, str] = {}

    for name, default in KEY_SETTING_DEFAULTS.items():
        value = _valid_key(source.get(name))
        if value is None:
            if name in source:
                notices.append(
                    f"keys.{name} {source.get(name)!r} is not a key Talaria "
                    f"recognizes; using {default}"
                )
            value = default
        if value in RESERVED_KEYS:
            notices.append(
                f"keys.{name} {value!r} is reserved for quitting; using {default}"
            )
            value = default
        resolved[name] = value

    grouped: dict[str, list[str]] = {}
    for name, chord in resolved.items():
        grouped.setdefault(chord, []).append(name)
    for chord, names in grouped.items():
        if len(names) < 2:
            continue
        for name in names:
            resolved[name] = KEY_SETTING_DEFAULTS[name]
        if len(names) == 2:
            first, second = names
            notices.append(
                f"keys.{first} and keys.{second} are both {chord!r}; "
                f"using {KEY_SETTING_DEFAULTS[first]} and "
                f"{KEY_SETTING_DEFAULTS[second]}"
            )
        else:
            listed = ", ".join(f"keys.{name}" for name in names)
            notices.append(f"{listed} are all {chord!r}; using defaults")

    return KeyBindings(**resolved), tuple(notices)


#: Built-in defaults. Every key a later unit reads must have an entry here so
#: that a missing config file never produces a missing setting. The type of the
#: value recorded here is also what environment-variable overrides are coerced
#: to, so a default of ``None`` means "string-valued, no default".
DEFAULTS: dict[str, Any] = {
    "theme": {"name": REFINED_DEFAULT.slug},
    "ui": {
        "reduced_motion": False,
        "inspector_width": 36,
        "inspector_open_at_start": False,
        "inspector_dock_min_columns": 120,
        "diff_side_by_side_min_columns": 112,
        "show_timestamps": False,
    },
    "status": {
        "command": None,
        "interval_seconds": 5,
        "segments": list(DEFAULT_STATUS_SEGMENTS),
        "cwd_max_columns": DEFAULT_CWD_MAX_COLUMNS,
        "git_branch_max_columns": DEFAULT_GIT_BRANCH_MAX_COLUMNS,
        "agent_model_max_columns": DEFAULT_AGENT_MODEL_MAX_COLUMNS,
    },
    "environment": {"allowlist": []},
    "composer": {
        "paste_collapse_lines": 6,
        "paste_collapse_bytes": 512,
        "attachment_max_mb": 16,
    },
    "notifications": {"transcript_line": True},
    # #120's keybinding surface plus D12's remaining named app bindings.
    # Normalized in _normalize_config, so an invalid value falls back with a
    # notice rather than binding a dead or dangerous key.
    "keys": dict(KEY_SETTING_DEFAULTS),
    # U4's profile endpoints: a name-to-gateway-URL map the operator writes.
    # It has no environment-variable override and never will — a map cannot be
    # expressed as one ``TALARIA_*`` scalar, and inventing an encoding for it
    # would be a second config syntax. Empty by default, which means every
    # listed profile renders as "no endpoint configured" until the operator
    # says otherwise; that is the honest starting state rather than a guess.
    "profiles": {"endpoints": {}},
    # D11: dashboard connection inventory. Not keyed by profile; credentials
    # stay in the 0600 credentials file, never beside the URL.
    "connections": {},
}

#: Maps a TALARIA_* environment variable to its (section, key) location in the
#: config tree. Extend this alongside DEFAULTS when a later unit adds a
#: setting; do not invent a parallel env-reading path elsewhere (KTD15).
_ENV_KEY_MAP: dict[str, tuple[str, str]] = {
    "TALARIA_STATUS_COMMAND": ("status", "command"),
    "TALARIA_STATUS_INTERVAL_SECONDS": ("status", "interval_seconds"),
    "TALARIA_COMPOSER_PASTE_COLLAPSE_LINES": ("composer", "paste_collapse_lines"),
    "TALARIA_COMPOSER_PASTE_COLLAPSE_BYTES": ("composer", "paste_collapse_bytes"),
    "TALARIA_KEYS_TOGGLE_INSPECTOR": ("keys", "toggle_inspector"),
    "TALARIA_KEYS_INTERRUPT": ("keys", "interrupt"),
    "TALARIA_KEYS_AGENTS": ("keys", "agents"),
    "TALARIA_KEYS_COMMANDS": ("keys", "commands"),
    "TALARIA_KEYS_MODELS": ("keys", "models"),
    "TALARIA_KEYS_PROFILES": ("keys", "profiles"),
    "TALARIA_KEYS_CONFIG": ("keys", "config"),
    "TALARIA_KEYS_FOLLOW": ("keys", "follow"),
    "TALARIA_KEYS_REPLAY_PAUSE": ("keys", "replay_pause"),
    "TALARIA_KEYS_REPLAY_SLOWER": ("keys", "replay_slower"),
    "TALARIA_KEYS_REPLAY_FASTER": ("keys", "replay_faster"),
}

_TRUE_LITERALS = frozenset({"1", "true", "yes", "on"})
_FALSE_LITERALS = frozenset({"0", "false", "no", "off"})

#: The two file scopes an explicit save may target.
ConfigSaveScope = Literal["user", "repository"]
#: The theme-specific spelling of :data:`ConfigSaveScope`, kept for the
#: existing ``save_theme``/``theme_config_path`` signatures.
ThemeSaveScope = ConfigSaveScope

#: The configuration-view allowlist (issue #149, D8 recorded 2026-09-05):
#: the only settings the view displays, and — for the three status rows —
#: the only keys its apply path writes. ``theme.name`` is displayed but never
#: view-writable: an explicit selection already persists through the theme
#: picker, and a second writer would be a second editor, which D8 rules out.
#: No credential, connection setting, environment allowlist, column limit, or
#: Hermes agent identity appears here, so none can reach the write path.
CONFIG_VIEW_KEYS: tuple[tuple[str, str], ...] = (
    ("theme", "name"),
    ("status", "command"),
    ("status", "interval_seconds"),
    ("status", "segments"),
)

_BUILTIN_THEME_SLUGS = frozenset(theme.slug for theme in BUILTIN_THEMES)
_THEME_SLUG_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
_THEME_HEADER_RE = re.compile(
    rb"(?m)^[ \t]*\[theme\][ \t]*(?:\#[^\r\n]*)?(?:\r?\n|$)"
)
_TABLE_HEADER_RE = re.compile(rb"(?m)^[ \t]*\[\[?[^\r\n]+\]\]?[ \t]*(?:\#[^\r\n]*)?(?:\r?\n|$)")
_THEME_NAME_RE = re.compile(
    rb"(?m)^[ \t]*(?:name|\"name\"|'name')[ \t]*=[^\r\n]*(?:\r?\n|$)"
)
_DOTTED_THEME_NAME_RE = re.compile(
    rb"(?m)^[ \t]*(?:theme|\"theme\"|'theme')[ \t]*\.[ \t]*"
    rb"(?:name|\"name\"|'name')[ \t]*=[ \t]*"
    rb"(?P<value>\"(?:\\.|[^\"\\\r\n])*\"|'[^'\r\n]*')"
)
_INLINE_THEME_RE = re.compile(
    rb"(?m)^[ \t]*(?:theme|\"theme\"|'theme')[ \t]*=[ \t]*"
    rb"(?P<table>\{[^\r\n]*\})[ \t]*(?:\#[^\r\n]*)?(?:\r?\n|$)"
)
_INLINE_THEME_NAME_RE = re.compile(
    rb"(?:\{|,)[ \t]*(?:name|\"name\"|'name')[ \t]*=[ \t]*"
    rb"(?P<value>\"(?:\\.|[^\"\\\r\n])*\"|'[^'\r\n]*')"
)
_STATUS_HEADER_RE = re.compile(
    rb"(?m)^[ \t]*\[status\][ \t]*(?:\#[^\r\n]*)?(?:\r?\n|$)"
)
#: A top-level inline ``status = { ... }`` table. Deliberately not supported by
#: the status writer: a multi-key inline table is a shape whose every neighbor
#: is load-bearing, so it is refused with the edit-by-hand message rather than
#: risked — the refusal is a designed outcome, not a failure (D8).
_INLINE_STATUS_RE = re.compile(
    rb"(?m)^[ \t]*(?:status|\"status\"|'status')[ \t]*=[ \t]*"
    rb"\{[^\r\n]*\}[ \t]*(?:\#[^\r\n]*)?(?:\r?\n|$)"
)
#: Any top-level dotted ``status.<key>`` assignment, used to detect the mixed
#: shape (some status keys dotted, some needing an append) that TOML cannot
#: express safely and the writer refuses.
_ANY_DOTTED_STATUS_RE = re.compile(rb"(?m)^[ \t]*status[ \t]*\.")


def global_config_dir() -> Path:
    """The global ``~/.talaria`` directory, or its ``TALARIA_CONFIG_DIR`` override.

    Tests redirect this via the environment variable so the operator's real
    ``~/.talaria`` is never touched by the test suite.
    """
    override = os.environ.get("TALARIA_CONFIG_DIR")
    if override:
        return Path(override).expanduser()
    return Path.home() / ".talaria"


def credentials_path(config_dir: Path | None = None) -> Path:
    """The KTD11 credential file: ``<config_dir>/credentials``.

    Path computation only. U7 owns creating it, enforcing mode 0600, and
    reading it.
    """
    return (config_dir if config_dir is not None else global_config_dir()) / "credentials"


def recordings_dir(config_dir: Path | None = None) -> Path:
    """The frame-log directory: ``<config_dir>/recordings/`` (git-ignored, R29)."""
    return (config_dir if config_dir is not None else global_config_dir()) / "recordings"


def _read_toml(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {}
    try:
        with path.open("rb") as fh:
            return tomllib.load(fh)
    except tomllib.TOMLDecodeError as exc:
        raise ConfigError(f"{path} is not valid TOML: {exc}") from exc
    except OSError as exc:
        raise ConfigError(f"{path} could not be read: {exc}") from exc


def _deep_merge(base: dict[str, Any], override: Mapping[str, Any]) -> dict[str, Any]:
    """Merge ``override`` onto ``base``, recursing into nested dicts only."""
    result = dict(base)
    for key, value in override.items():
        if isinstance(value, Mapping) and isinstance(result.get(key), Mapping):
            result[key] = _deep_merge(dict(result[key]), value)
        else:
            result[key] = value
    return result


def _coerce_env_value(env_name: str, raw: str, default: Any) -> Any:
    """Coerce ``raw`` to the type :data:`DEFAULTS` records for the setting.

    Keyed off the declared type rather than the shape of the string, so a
    string-valued setting given a numeric-looking value stays a string.
    """
    # bool before int: bool is a subclass of int.
    if isinstance(default, bool):
        lowered = raw.strip().lower()
        if lowered in _TRUE_LITERALS:
            return True
        if lowered in _FALSE_LITERALS:
            return False
        raise ConfigError(
            f"{env_name} expects a boolean "
            f"({'/'.join(sorted(_TRUE_LITERALS | _FALSE_LITERALS))}), got {raw!r}"
        )
    if isinstance(default, int):
        try:
            return int(raw.strip())
        except ValueError:
            raise ConfigError(f"{env_name} expects an integer, got {raw!r}") from None
    return raw


def _env_overrides() -> dict[str, Any]:
    overrides: dict[str, Any] = {}
    for env_name, (section, key) in _ENV_KEY_MAP.items():
        raw = os.environ.get(env_name)
        if raw is None:
            continue
        default = DEFAULTS.get(section, {}).get(key)
        try:
            value = _coerce_env_value(env_name, raw, default)
        except ConfigError:
            if (section, key) != ("status", "interval_seconds"):
                raise
            # This setting has a visible fallback contract. Preserve the
            # winning malformed value until the one post-precedence status
            # normalization pass can name the key and apply that fallback.
            value = raw
        overrides.setdefault(section, {})[key] = value
    return overrides


def _freeze(value: Any) -> Any:
    """Recursively convert containers to read-only equivalents.

    Without this, :class:`Config`'s ``frozen=True`` is cosmetic: a caller that
    mutates a nested dict or list reachable from ``values`` corrupts the
    snapshot, and — when the section came straight from :data:`DEFAULTS` —
    could corrupt the built-in defaults for the rest of the process.
    """
    if isinstance(value, Mapping):
        return MappingProxyType({key: _freeze(item) for key, item in value.items()})
    if isinstance(value, list):
        return tuple(_freeze(item) for item in value)
    return value


@dataclass(frozen=True)
class Config:
    """A fully resolved, deeply immutable configuration snapshot."""

    values: Mapping[str, Any]
    config_dir: Path
    notices: tuple[str, ...] = ()

    def get(self, *path: str, default: Any = None) -> Any:
        """Look up a dotted path, e.g. ``config.get("status", "command")``."""
        node: Any = self.values
        for part in path:
            if not isinstance(node, Mapping) or part not in node:
                return default
            node = node[part]
        return node


def keybindings(cfg: Config) -> KeyBindings:
    """The resolved inspector/interrupt chords for BINDINGS construction.

    Reads the normalized ``[keys]`` section, so every fallback the winning
    value needed is already applied — and already named in ``cfg.notices``,
    which is why the notices :func:`resolve_keybindings` also returns are
    discarded here rather than surfaced twice.
    """
    section = cfg.get("keys", default={})
    bindings, _ = resolve_keybindings(
        section if isinstance(section, Mapping) else None
    )
    return bindings


def profile_endpoints(cfg: Config) -> Mapping[str, str]:
    """The operator's ``[profiles.endpoints]`` map, with non-string rows dropped.

    ``GET /api/profiles`` publishes no address for a profile's gateway — see
    ``talaria/transport/admin.py``'s docstring for the measured key list — so
    this is where a profile's endpoint actually comes from (U4). A row whose
    value is not a string is skipped rather than coerced: the value is a URL
    that will be dialled, and ``str(7)`` is not one.

    Returns a plain ``dict``, not the frozen section, because callers pass it
    into pure functions that only read it and a ``MappingProxyType`` in that
    position is a type puzzle for no gain.
    """
    section = cfg.get("profiles", "endpoints", default={})
    if not isinstance(section, Mapping):
        return {}
    return {
        name: value
        for name, value in section.items()
        if isinstance(name, str) and isinstance(value, str) and value.strip()
    }


_INSPECTOR_WIDTH_BOUNDS = (28, 48)
_CONNECTION_AUTH_MODES = frozenset({"loopback", "gated"})
_CONNECTION_FILE_KEYS = frozenset({"url", "auth", "label"})


def _url_has_userinfo(url: str) -> bool:
    """True when ``url`` carries credentials in the userinfo position."""
    try:
        parts = urlsplit(url)
    except ValueError:
        return False
    return parts.username is not None or parts.password is not None


def _normalize_bool_setting(
    table: dict[str, Any],
    key: str,
    fallback: bool,
    path: str,
    notices: list[str],
) -> None:
    if not isinstance(table.get(key), bool):
        table[key] = fallback
        notices.append(f"{path} must be a boolean; using {str(fallback).lower()}")


def _normalize_positive_int(
    table: dict[str, Any],
    key: str,
    fallback: int,
    path: str,
    notices: list[str],
) -> None:
    value = table.get(key)
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        table[key] = fallback
        notices.append(f"{path} must be a positive integer; using {fallback}")


def _normalize_ui_geometry(ui: dict[str, Any], notices: list[str]) -> None:
    width = ui.get("inspector_width")
    low, high = _INSPECTOR_WIDTH_BOUNDS
    if isinstance(width, bool) or not isinstance(width, int) or not low <= width <= high:
        ui["inspector_width"] = 36
        notices.append(
            f"ui.inspector_width must be an integer between {low} and {high}; using 36"
        )
    _normalize_positive_int(
        ui,
        "inspector_dock_min_columns",
        120,
        "ui.inspector_dock_min_columns",
        notices,
    )
    _normalize_positive_int(
        ui,
        "diff_side_by_side_min_columns",
        112,
        "ui.diff_side_by_side_min_columns",
        notices,
    )


def _normalize_connections(merged: dict[str, Any]) -> tuple[str, ...]:
    """Validate the D11 inventory; loads never raise."""
    notices: list[str] = []
    source = merged.get("connections")
    if not isinstance(source, Mapping):
        merged["connections"] = {}
        if source is not None:
            notices.append("connections must be a table; using an empty inventory")
        return tuple(notices)

    normalized: dict[str, Any] = {}
    for conn_id, entry in source.items():
        name = str(conn_id)
        if not isinstance(entry, Mapping):
            notices.append(f"connections.{name} must be a table; ignoring the entry")
            continue
        item = dict(entry)
        auth = item.get("auth")
        if auth is not None and auth not in _CONNECTION_AUTH_MODES:
            item["auth"] = "loopback"
            notices.append(
                f"connections.{name} auth {auth!r} is not recognized; using loopback"
            )
        url = item.get("url")
        if isinstance(url, str) and _url_has_userinfo(url):
            item["url"] = ""
            notices.append(
                f"connections.{name} url carries a credential; dropping the url"
            )
        normalized[name] = item
    merged["connections"] = normalized
    return tuple(notices)


def _normalize_config(
    merged: dict[str, Any],
    *,
    available_theme_slugs: frozenset[str],
) -> tuple[dict[str, Any], tuple[str, ...]]:
    """Normalize winning configured values once, after every file layer merged."""
    notices: list[str] = []
    theme = merged.get("theme")
    if not isinstance(theme, Mapping):
        merged["theme"] = {"name": REFINED_DEFAULT.slug}
        notices.append(
            "theme must be a table with a name key; using Refined Default "
            f"({REFINED_DEFAULT.slug})"
        )
    elif not isinstance(requested := theme.get("name"), str):
        merged["theme"] = {"name": REFINED_DEFAULT.slug}
        notices.append(theme_fallback_notice(requested, REFINED_DEFAULT.slug))
    elif requested not in available_theme_slugs:
        merged["theme"] = {"name": REFINED_DEFAULT.slug}
        notices.append(theme_fallback_notice(requested, REFINED_DEFAULT.slug))
    else:
        merged["theme"] = dict(theme)

    ui_source = merged.get("ui")
    ui = dict(ui_source) if isinstance(ui_source, Mapping) else {}
    if not isinstance(ui.get("reduced_motion"), bool):
        ui["reduced_motion"] = False
        notices.append("ui.reduced_motion must be a boolean; using false")
    _normalize_ui_geometry(ui, notices)
    _normalize_bool_setting(
        ui, "inspector_open_at_start", False, "ui.inspector_open_at_start", notices
    )
    _normalize_bool_setting(ui, "show_timestamps", False, "ui.show_timestamps", notices)
    merged["ui"] = ui

    status_source = merged.get("status")
    normalized_status = normalize_status_settings(status_source)
    status = dict(status_source) if isinstance(status_source, Mapping) else {}
    command = status.get("command")
    _argv, command_notice = parse_command(command)
    status.update(
        {
            "command": None if command_notice is not None else command,
            "interval_seconds": normalized_status.interval_seconds,
            "segments": list(normalized_status.bar.segments),
            "cwd_max_columns": normalized_status.bar.cwd_max_columns,
            "git_branch_max_columns": normalized_status.bar.git_branch_max_columns,
            "agent_model_max_columns": normalized_status.bar.agent_model_max_columns,
        }
    )
    merged["status"] = status
    notices.extend(normalized_status.notices)
    if command_notice is not None:
        notices.append(command_notice)

    composer_source = merged.get("composer")
    composer = dict(composer_source) if isinstance(composer_source, Mapping) else {}
    attachment = composer.get("attachment_max_mb")
    if (
        isinstance(attachment, bool)
        or not isinstance(attachment, int)
        or attachment <= 0
    ):
        composer["attachment_max_mb"] = 16
        notices.append(
            "composer.attachment_max_mb must be a positive integer; using 16"
        )
    merged["composer"] = composer

    notifications_source = merged.get("notifications")
    notifications = (
        dict(notifications_source) if isinstance(notifications_source, Mapping) else {}
    )
    _normalize_bool_setting(
        notifications,
        "transcript_line",
        True,
        "notifications.transcript_line",
        notices,
    )
    merged["notifications"] = notifications

    keys_source = merged.get("keys")
    bindings, key_notices = resolve_keybindings(
        keys_source if isinstance(keys_source, Mapping) else None
    )
    if not isinstance(keys_source, Mapping):
        notices.append(
            "keys must be a table with toggle_inspector and interrupt keys; "
            f"using {DEFAULT_INSPECTOR_KEY} and {DEFAULT_INTERRUPT_KEY}"
        )
    merged["keys"] = {
        name: getattr(bindings, name) for name in KEY_SETTING_DEFAULTS
    }
    notices.extend(key_notices)

    connection_notices = _normalize_connections(merged)
    notices.extend(connection_notices)
    return merged, tuple(notices)


def theme_config_path(
    scope: ThemeSaveScope,
    *,
    config_dir: Path | None = None,
    cwd: Path | None = None,
) -> Path:
    """Return the explicit-save target for one supported theme scope."""
    if scope == "user":
        root = config_dir if config_dir is not None else global_config_dir()
        return root / "config.toml"
    if scope == "repository":
        root = cwd if cwd is not None else Path.cwd()
        return root / ".talaria" / "config.toml"
    raise ConfigError(f"unknown theme save scope: {scope!r}")


def atomic_replace_bytes(
    path: Path,
    content: bytes,
    *,
    follow_symlinks: bool = False,
) -> None:
    """Atomically replace a path, optionally resolving its final symlink first.

    The temporary file is created beside the path that will actually be
    replaced. By default, a symlink at ``path`` is itself replaced; callers
    must opt in when the intended contract is to update its target instead.
    """
    target = path.resolve() if follow_symlinks and path.is_symlink() else path
    target.parent.mkdir(parents=True, exist_ok=True)
    target_stat = os.lstat(target) if os.path.lexists(target) else None
    replacing_symlink = target_stat is not None and stat.S_ISLNK(target_stat.st_mode)
    existing_mode = (
        stat.S_IMODE(target_stat.st_mode)
        if target_stat is not None and not replacing_symlink
        else None
    )
    replacement_mode = existing_mode
    if replacing_symlink:
        previous_umask = os.umask(0o777)
        os.umask(previous_umask)
        replacement_mode = 0o666 & ~previous_umask
    descriptor, raw_temp = tempfile.mkstemp(
        prefix=f".{target.name}.", dir=target.parent
    )
    temp_path = Path(raw_temp)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            if replacement_mode is not None:
                os.fchmod(handle.fileno(), replacement_mode)
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temp_path, target)
    except BaseException:
        try:
            temp_path.unlink()
        except FileNotFoundError:
            pass
        raise


def _parse_toml_bytes(path: Path, content: bytes) -> dict[str, Any]:
    try:
        return tomllib.loads(content.decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as exc:
        raise ConfigError(f"{path} is not valid TOML: {exc}") from exc


def _inline_comment_suffix(line: bytes) -> bytes:
    """Return an unquoted TOML comment with its preceding spacing intact."""
    quote: int | None = None
    escaped = False
    for index, byte in enumerate(line):
        if quote == ord('"'):
            if escaped:
                escaped = False
            elif byte == ord("\\"):
                escaped = True
            elif byte == quote:
                quote = None
            continue
        if quote == ord("'"):
            if byte == quote:
                quote = None
            continue
        if byte in (ord('"'), ord("'")):
            quote = byte
        elif byte == ord("#"):
            start = index
            while start and line[start - 1] in (ord(" "), ord("\t")):
                start -= 1
            return line[start:]
    return b""


def _rewrite_theme_name(content: bytes, name: str) -> bytes:
    """Change only ``theme.name`` while retaining every neighboring byte."""
    assignment = f'name = "{name}"'.encode()
    header = _THEME_HEADER_RE.search(content)
    if header is None:
        separator = b"" if not content or content.endswith((b"\n", b"\r")) else b"\n"
        blank = b"" if not content or content.endswith((b"\n\n", b"\r\n\r\n")) else b"\n"
        return content + separator + blank + b"[theme]\n" + assignment + b"\n"

    next_header = _TABLE_HEADER_RE.search(content, header.end())
    table_end = next_header.start() if next_header is not None else len(content)
    name_match = _THEME_NAME_RE.search(content, header.end(), table_end)
    if name_match is not None:
        matched = name_match.group()
        newline = (
            b"\r\n"
            if matched.endswith(b"\r\n")
            else b"\n" if matched.endswith(b"\n") else b""
        )
        body = matched[: -len(newline)] if newline else matched
        replacement = assignment + _inline_comment_suffix(body) + newline
        return content[: name_match.start()] + replacement + content[name_match.end() :]

    newline = b"\r\n" if header.group().endswith(b"\r\n") else b"\n"
    return content[: header.end()] + assignment + newline + content[header.end() :]


def _rewrite_dotted_theme_name(content: bytes, name: str) -> bytes | None:
    """Rewrite a top-level dotted ``theme.name`` assignment when present."""
    name_match = _DOTTED_THEME_NAME_RE.search(content)
    if name_match is None:
        return None
    start, end = name_match.span("value")
    return content[:start] + f'"{name}"'.encode() + content[end:]


def _rewrite_inline_theme_name(content: bytes, name: str) -> bytes | None:
    """Rewrite the name pair in a top-level inline ``theme`` table."""
    table_match = _INLINE_THEME_RE.search(content)
    if table_match is None:
        return None
    table = table_match.group("table")
    name_match = _INLINE_THEME_NAME_RE.search(table)
    if name_match is None:
        return None
    table_start = table_match.start("table")
    value_start, value_end = name_match.span("value")
    start = table_start + value_start
    end = table_start + value_end
    return content[:start] + f'"{name}"'.encode() + content[end:]


def save_theme(
    name: str,
    scope: ThemeSaveScope = "user",
    *,
    config_dir: Path | None = None,
    cwd: Path | None = None,
) -> Path:
    """Persist only ``theme.name`` to the selected configuration scope."""
    if not _THEME_SLUG_RE.fullmatch(name):
        raise ConfigError(f"invalid theme name: {name!r}")

    path = theme_config_path(scope, config_dir=config_dir, cwd=cwd)
    try:
        before_bytes = path.read_bytes() if path.is_file() else b""
    except OSError as exc:
        raise ConfigError(f"{path} could not be read: {exc}") from exc
    before = _parse_toml_bytes(path, before_bytes)
    current_theme = before.get("theme")
    if current_theme is not None and not isinstance(current_theme, Mapping):
        raise ConfigError(f"{path} theme must be a table before it can be saved")

    if current_theme is not None and _THEME_HEADER_RE.search(before_bytes) is None:
        after_bytes = _rewrite_dotted_theme_name(before_bytes, name)
        if after_bytes is None:
            after_bytes = _rewrite_inline_theme_name(before_bytes, name)
        if after_bytes is None:
            raise ConfigError(
                f"{path} theme.name cannot be rewritten without a [theme] table "
                "or a supported dotted or inline theme.name assignment"
            )
    else:
        after_bytes = _rewrite_theme_name(before_bytes, name)
    try:
        after = _parse_toml_bytes(path, after_bytes)
    except ConfigError as exc:
        raise ConfigError(
            f"Talaria cannot safely rewrite theme.name in this form: {path}; "
            "no changes were written"
        ) from exc
    expected = deepcopy(before)
    expected_theme = expected.setdefault("theme", {})
    if not isinstance(expected_theme, dict):  # guarded above; keeps the proof local
        raise ConfigError(f"{path} theme must be a table before it can be saved")
    expected_theme["name"] = name
    if after != expected:
        raise ConfigError(
            f"refusing to write {path}: the edit changed more than theme.name"
        )

    try:
        atomic_replace_bytes(path, after_bytes, follow_symlinks=True)
    except OSError as exc:
        raise ConfigError(f"{path} could not be written: {exc}") from exc
    return path


# ── the configuration view's status write (issue #149, D8 recorded) ──────
#
# The same byte-preserving discipline ``save_theme`` uses, generalized to whole
# ``[status]`` assignments including a multi-line ``segments`` array: a matched
# assignment's *value* is replaced (its ``key = `` prefix and spacing stay as
# the operator wrote them), a missing assignment is appended, and a hand-
# formatted file the rewriter cannot match safely is refused — "edit the file
# by hand" — rather than reformatted (D8). ``theme.name`` never passes through
# here: selection already persists through the theme picker, and this writer
# must not become a second theme editor.

#: The only keys :func:`save_status_settings` writes, in the fixed order a
#: missing-key append uses. Anything else — a credential-like key, the
#: environment allowlist, a profile endpoint — is refused by name.
STATUS_WRITE_KEYS: tuple[str, ...] = ("command", "interval_seconds", "segments")

_STATUS_INTERVAL_BOUNDS = (1, 3600)


def _toml_basic_string(value: str) -> str:
    """Render ``value`` as a TOML basic string with real escaping.

    ``save_theme`` could rely on the theme-slug shape to skip escaping; a
    status command is arbitrary operator text, so every character TOML requires
    an escape for is escaped, and the remaining control characters become
    ``\\uXXXX`` rather than bytes TOML cannot carry.
    """
    escaped = (
        value.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\b", "\\b")
        .replace("\t", "\\t")
        .replace("\n", "\\n")
        .replace("\f", "\\f")
        .replace("\r", "\\r")
    )
    escaped = "".join(
        character if ord(character) >= 0x20 else f"\\u{ord(character):04x}"
        for character in escaped
    )
    return f'"{escaped}"'


def _render_status_value(key: str, value: Any) -> bytes:
    """One status setting's TOML value in the shape ``docs/configuration.md``
    documents: scalars on one line, ``segments`` as the multi-line array with
    every name on its own indented line."""
    if key == "command":
        return _toml_basic_string(value).encode()
    if key == "interval_seconds":
        return str(value).encode()
    names = tuple(value)
    if not names:
        return b"[]"
    lines = [b"["]
    lines.extend(b"  " + _toml_basic_string(name).encode() + b"," for name in names)
    lines.append(b"]")
    return b"\n".join(lines)


def _status_key_pattern(key: str) -> re.Pattern[bytes]:
    raw = re.escape(key.encode("ascii"))
    return re.compile(
        rb"(?m)^[ \t]*(?:" + raw + rb"|\"" + raw + rb"\"|'" + raw + rb"')[ \t]*="
    )


def _dotted_status_key_pattern(key: str) -> re.Pattern[bytes]:
    raw = re.escape(key.encode("ascii"))
    section = re.escape(b"status")
    return re.compile(
        rb"(?m)^[ \t]*(?:" + section + rb"|\"" + section + rb"\"|'" + section + rb"')"
        rb"[ \t]*\.[ \t]*(?:" + raw + rb"|\"" + raw + rb"\"|'" + raw + rb"')[ \t]*="
    )


def _scalar_value_end(content: bytes, value_start: int, limit: int) -> int:
    """Trim trailing blanks and a carriage return off a scalar value span."""
    end = limit
    while end > value_start and content[end - 1] in (0x20, 0x09, 0x0D):
        end -= 1
    return end


def _status_value_span(content: bytes, assignment: re.Match[bytes]) -> tuple[int, int]:
    """The byte span of one assignment's *value*, starting after its ``=``.

    A scalar ends at its line end; an array ends after its closing bracket,
    across as many lines as the brackets stay open. A comment *inside* a value
    this rewrite would replace is refused here rather than silently eaten
    ("comments and every neighboring byte survive", D8) — a trailing comment
    after the value stays outside the span and survives untouched.
    """
    index = assignment.end()
    while index < len(content) and content[index] in (0x20, 0x09):
        index += 1
    value_start = index
    depth = 0
    quote: int | None = None
    escaped = False
    position = value_start
    while position < len(content):
        byte = content[position]
        if quote == ord('"'):
            if escaped:
                escaped = False
            elif byte == ord("\\"):
                escaped = True
            elif byte == quote:
                quote = None
            position += 1
            continue
        if quote == ord("'"):
            if byte == quote:
                quote = None
            position += 1
            continue
        if byte in (ord('"'), ord("'")):
            quote = byte
        elif byte == ord("["):
            depth += 1
        elif byte == ord("]"):
            depth -= 1
            if depth <= 0:
                return value_start, position + 1
        elif byte == ord("#"):
            if depth > 0:
                raise ConfigError(
                    "the status value carries a comment inside it; "
                    "edit the file by hand"
                )
            return value_start, _scalar_value_end(content, value_start, position)
        elif byte == ord("\n") and depth == 0:
            return value_start, _scalar_value_end(content, value_start, position)
        position += 1
    if depth > 0:
        raise ConfigError("the status value never closes; edit the file by hand")
    return value_start, _scalar_value_end(content, value_start, len(content))


def _status_replacement(content: bytes, span: tuple[int, int], value_bytes: bytes) -> bytes:
    """The value to splice, in the line-ending style the replaced span used."""
    start, end = span
    if b"\r\n" in content[start:end]:
        return value_bytes.replace(b"\n", b"\r\n")
    return value_bytes


def _rewrite_dotted_or_append_block(
    content: bytes, rendered: Mapping[str, bytes]
) -> bytes:
    """The no-``[status]``-table case: rewrite dotted assignments, or append
    the documented block. Refuses the mixed shape TOML cannot express."""
    dotted = {
        key: _dotted_status_key_pattern(key).search(content) for key in rendered
    }
    missing = [key for key, match in dotted.items() if match is None]
    if missing:
        if any(match is not None for match in dotted.values()) or (
            _ANY_DOTTED_STATUS_RE.search(content) is not None
        ):
            # Appending is unsafe in both directions: a ``[status]`` block or a
            # new dotted line would have to land beside a dotted status
            # assignment TOML has already defined, and wherever it lands is
            # either a redeclaration or a move into another table's region.
            raise ConfigError(
                "this file's status settings mix dotted assignments with keys "
                "that would have to be appended; edit the file by hand"
            )
        # No status shape at all: append the block the way the theme writer
        # appends its table — separator, blank line, then the table.
        separator = b"" if not content or content.endswith((b"\n", b"\r")) else b"\n"
        blank = b"" if not content or content.endswith((b"\n\n", b"\r\n\r\n")) else b"\n"
        block = b"[status]\n" + b"".join(
            key.encode("ascii") + b" = " + rendered[key] + b"\n" for key in rendered
        )
        return content + separator + blank + block

    replacements: list[tuple[int, int, bytes]] = []
    for key, match in dotted.items():
        if match is None:
            # Unreachable past the missing-branch return above; kept so the
            # refusal stands on its own if that branch ever moves.
            raise ConfigError(
                "this file's status settings mix dotted assignments with keys "
                "that would have to be appended; edit the file by hand"
            )
        span = _status_value_span(content, match)
        replacements.append(
            (*span, _status_replacement(content, span, rendered[key]))
        )
    for start, end, replacement in sorted(replacements, reverse=True):
        content = content[:start] + replacement + content[end:]
    return content


def _rewrite_status_settings(
    content: bytes, rendered: Mapping[str, bytes]
) -> bytes:
    """Replace exactly the rendered status values, preserving every other byte.

    A key missing from the existing ``[status]`` table is appended at the
    table's end; a file with no ``[status]`` table at all gains the documented
    block. Appends are computed *before* in-place replacements so every
    replacement span — all of which precede the insertion point — keeps its
    original position.
    """
    if not rendered:
        return content
    if _INLINE_STATUS_RE.search(content):
        raise ConfigError(
            "status is written as an inline table; Talaria will not reformat "
            "it — edit the file by hand"
        )

    header = _STATUS_HEADER_RE.search(content)
    if header is None:
        return _rewrite_dotted_or_append_block(content, rendered)

    next_header = _TABLE_HEADER_RE.search(content, header.end())
    table_end = next_header.start() if next_header is not None else len(content)
    replacements: list[tuple[int, int, bytes]] = []
    appends: list[tuple[str, bytes]] = []
    for key in rendered:
        match = _status_key_pattern(key).search(content, header.end(), table_end)
        if match is None:
            appends.append((key, rendered[key]))
            continue
        span = _status_value_span(content, match)
        replacements.append((*span, _status_replacement(content, span, rendered[key])))

    if appends:
        newline = b"\r\n" if b"\r\n" in content[header.end() : table_end] else b"\n"
        prefix = content[:table_end]
        if prefix and not prefix.endswith((b"\n", b"\r")):
            prefix += newline
        block = b"".join(
            key.encode("ascii") + b" = " + value_bytes + newline
            for key, value_bytes in appends
        )
        content = prefix + block + content[table_end:]

    for start, end, replacement in sorted(replacements, reverse=True):
        content = content[:start] + replacement + content[end:]
    return content


def _validate_status_change(key: str, value: Any) -> None:
    """Refuse a value the documented status contract would not accept."""
    if key == "command":
        # An empty command is allowed and is the honest "no status script"
        # state: the documented contract disables the region for an empty
        # value, and the write persists `command = ""` rather than deleting
        # the assignment (D8 recorded — one mechanism, not two).
        if not isinstance(value, str):
            raise ConfigError(f"status.command must be a string; refusing {value!r}")
        return
    if key == "interval_seconds":
        if isinstance(value, bool) or not isinstance(value, int):
            raise ConfigError(
                f"status.interval_seconds must be an integer; refusing {value!r}"
            )
        low, high = _STATUS_INTERVAL_BOUNDS
        if not low <= value <= high:
            raise ConfigError(
                f"status.interval_seconds must be between {low} and {high}; "
                f"refusing {value}"
            )
        return
    if isinstance(value, (str, bytes)) or not isinstance(value, Sequence):
        raise ConfigError(
            f"status.segments must be a list of segment names; refusing {value!r}"
        )
    seen: set[str] = set()
    for name in value:
        if not isinstance(name, str) or name not in KNOWN_STATUS_SEGMENTS:
            raise ConfigError(
                f"status.segments names an unknown segment; refusing {name!r}"
            )
        if name in seen:
            raise ConfigError(
                f"status.segments lists {name!r} twice; refusing the duplicate"
            )
        seen.add(name)


def save_status_settings(
    changes: Mapping[str, Any],
    scope: ConfigSaveScope = "user",
    *,
    config_dir: Path | None = None,
    cwd: Path | None = None,
) -> Path:
    """Persist only the changed ``[status]`` keys to the selected scope.

    The configuration view's apply path (issue #149, D8 recorded): the same
    narrow, byte-preserving write :func:`save_theme` uses, generalized to whole
    status assignments including the multi-line ``segments`` array. The parsed
    document is verified to differ from the original in exactly the requested
    keys before anything is written, so a rewrite that touched one neighbor
    byte too many refuses instead of persisting.
    """
    if not changes:
        raise ConfigError("save_status_settings needs at least one changed key")
    unknown = [key for key in changes if key not in STATUS_WRITE_KEYS]
    if unknown:
        raise ConfigError(
            "the configuration view writes only "
            "status.command, status.interval_seconds, and status.segments; "
            f"refusing {', '.join(repr(key) for key in unknown)}"
        )
    for key, value in changes.items():
        _validate_status_change(key, value)
    rendered = {
        key: _render_status_value(key, changes[key])
        for key in STATUS_WRITE_KEYS
        if key in changes
    }

    path = theme_config_path(scope, config_dir=config_dir, cwd=cwd)
    try:
        before_bytes = path.read_bytes() if path.is_file() else b""
    except OSError as exc:
        raise ConfigError(f"{path} could not be read: {exc}") from exc
    before = _parse_toml_bytes(path, before_bytes)
    existing_status = before.get("status")
    if existing_status is not None and not isinstance(existing_status, Mapping):
        raise ConfigError(f"{path} status must be a table before it can be saved")

    try:
        after_bytes = _rewrite_status_settings(before_bytes, rendered)
    except ConfigError as exc:
        raise ConfigError(f"{path}: {exc}") from exc
    try:
        after = _parse_toml_bytes(path, after_bytes)
    except ConfigError as exc:
        raise ConfigError(
            f"Talaria cannot safely rewrite the [status] keys in this form: {path}; "
            "edit the file by hand — no changes were written"
        ) from exc
    expected = deepcopy(before)
    expected_status = expected.setdefault("status", {})
    if not isinstance(expected_status, dict):  # guarded above; keeps the proof local
        raise ConfigError(f"{path} status must be a table before it can be saved")
    # TOML parses an array back as a list, so the expected document must carry
    # one too — a caller's tuple would compare unequal to its own reflection.
    expected_status.update(
        {key: list(value) if key == "segments" else value for key, value in changes.items()}
    )
    if after != expected:
        raise ConfigError(
            f"refusing to write {path}: the edit changed more than the requested keys"
        )

    try:
        atomic_replace_bytes(path, after_bytes, follow_symlinks=True)
    except OSError as exc:
        raise ConfigError(f"{path} could not be written: {exc}") from exc
    return path


# ── D10/D11 generalized surgical writer ──────────────────────────────────


_WRITABLE_TABLES = frozenset(
    {
        "theme",
        "ui",
        "status",
        "environment",
        "composer",
        "keys",
        "notifications",
    }
)
_TABLE_WRITE_KEYS: dict[str, frozenset[str]] = {
    "theme": frozenset({"name"}),
    "ui": frozenset(
        {
            "reduced_motion",
            "inspector_width",
            "inspector_open_at_start",
            "inspector_dock_min_columns",
            "diff_side_by_side_min_columns",
            "show_timestamps",
        }
    ),
    "status": frozenset(STATUS_WRITE_KEYS),
    "environment": frozenset({"allowlist"}),
    "composer": frozenset(
        {"paste_collapse_lines", "paste_collapse_bytes", "attachment_max_mb"}
    ),
    "keys": frozenset(KEY_SETTING_DEFAULTS),
    "notifications": frozenset({"transcript_line"}),
}


class _RenderedTable(dict[str, bytes]):
    """A key→TOML-value map that also names the table being rewritten."""

    def __init__(self, table: str, values: Mapping[str, bytes]) -> None:
        super().__init__(values)
        self.table = table


def _table_header_pattern(table: str) -> re.Pattern[bytes]:
    raw = re.escape(table.encode("ascii"))
    return re.compile(rb"(?m)^[ \t]*\[" + raw + rb"\][ \t]*(?:\#[^\r\n]*)?(?:\r?\n|$)")


def _inline_table_pattern(table: str) -> re.Pattern[bytes]:
    raw = re.escape(table.encode("ascii"))
    quoted = rb"(?:" + raw + rb"|\"" + raw + rb"\"|'" + raw + rb"')"
    return re.compile(
        rb"(?m)^[ \t]*"
        + quoted
        + rb"[ \t]*=[ \t]*\{[^\r\n]*\}[ \t]*(?:\#[^\r\n]*)?(?:\r?\n|$)"
    )


def _any_dotted_table_pattern(table: str) -> re.Pattern[bytes]:
    parts = [re.escape(part.encode("ascii")) for part in table.split(".")]
    dotted = rb"[ \t]*\.[ \t]*".join(parts)
    return re.compile(rb"(?m)^[ \t]*" + dotted + rb"[ \t]*\.")


def _dotted_table_key_pattern(table: str, key: str) -> re.Pattern[bytes]:
    pieces = [re.escape(part.encode("ascii")) for part in (*table.split("."), key)]
    quoted = [
        rb"(?:" + piece + rb"|\"" + piece + rb"\"|'" + piece + rb"')" for piece in pieces
    ]
    return re.compile(
        rb"(?m)^[ \t]*" + rb"[ \t]*\.[ \t]*".join(quoted) + rb"[ \t]*="
    )


def _table_key_pattern(key: str) -> re.Pattern[bytes]:
    raw = re.escape(key.encode("ascii"))
    return re.compile(
        rb"(?m)^[ \t]*(?:" + raw + rb"|\"" + raw + rb"\"|'" + raw + rb"')[ \t]*="
    )


def _render_toml_value(value: Any) -> bytes:
    if isinstance(value, bool):
        return b"true" if value else b"false"
    if isinstance(value, int):
        return str(value).encode()
    if isinstance(value, str):
        return _toml_basic_string(value).encode()
    if isinstance(value, (bytes, bytearray)) or not isinstance(value, Sequence):
        raise ConfigError(f"cannot render {value!r} as a TOML value")
    names = tuple(value)
    if not names:
        return b"[]"
    lines = [b"["]
    lines.extend(b"  " + _toml_basic_string(str(name)).encode() + b"," for name in names)
    lines.append(b"]")
    return b"\n".join(lines)


def _rewrite_dotted_or_append_named_table(
    content: bytes, table: str, rendered: Mapping[str, bytes]
) -> bytes:
    dotted = {key: _dotted_table_key_pattern(table, key).search(content) for key in rendered}
    missing = [key for key, match in dotted.items() if match is None]
    if missing:
        if any(match is not None for match in dotted.values()) or (
            _any_dotted_table_pattern(table).search(content) is not None
        ):
            raise ConfigError(
                f"this file's {table} settings mix dotted assignments with keys "
                "that would have to be appended; edit the file by hand"
            )
        separator = b"" if not content or content.endswith((b"\n", b"\r")) else b"\n"
        blank = b"" if not content or content.endswith((b"\n\n", b"\r\n\r\n")) else b"\n"
        block = f"[{table}]\n".encode("ascii") + b"".join(
            key.encode("ascii") + b" = " + rendered[key] + b"\n" for key in rendered
        )
        return content + separator + blank + block

    replacements: list[tuple[int, int, bytes]] = []
    for key, match in dotted.items():
        if match is None:
            raise ConfigError(
                f"this file's {table} settings mix dotted assignments with keys "
                "that would have to be appended; edit the file by hand"
            )
        span = _status_value_span(content, match)
        replacements.append((*span, _status_replacement(content, span, rendered[key])))
    for start, end, replacement in sorted(replacements, reverse=True):
        content = content[:start] + replacement + content[end:]
    return content


def _rewrite_named_table(
    content: bytes, table: str, rendered: Mapping[str, bytes]
) -> bytes:
    if not rendered:
        return content
    if _inline_table_pattern(table).search(content):
        raise ConfigError(
            f"{table} is written as an inline table; Talaria will not reformat "
            "it — edit the file by hand"
        )

    header = _table_header_pattern(table).search(content)
    if header is None:
        return _rewrite_dotted_or_append_named_table(content, table, rendered)

    next_header = _TABLE_HEADER_RE.search(content, header.end())
    table_end = next_header.start() if next_header is not None else len(content)
    replacements: list[tuple[int, int, bytes]] = []
    appends: list[tuple[str, bytes]] = []
    for key in rendered:
        match = _table_key_pattern(key).search(content, header.end(), table_end)
        if match is None:
            appends.append((key, rendered[key]))
            continue
        span = _status_value_span(content, match)
        replacements.append((*span, _status_replacement(content, span, rendered[key])))

    if appends:
        newline = b"\r\n" if b"\r\n" in content[header.end() : table_end] else b"\n"
        prefix = content[:table_end]
        if prefix and not prefix.endswith((b"\n", b"\r")):
            prefix += newline
        block = b"".join(
            key.encode("ascii") + b" = " + value_bytes + newline
            for key, value_bytes in appends
        )
        content = prefix + block + content[table_end:]

    for start, end, replacement in sorted(replacements, reverse=True):
        content = content[:start] + replacement + content[end:]
    return content


def _rewrite_settings_table(content: bytes, rendered: Mapping[str, bytes]) -> bytes:
    """Byte-preserving rewrite used by :func:`save_settings`.

    ``rendered`` is a :class:`_RenderedTable` (or any mapping with a ``table``
    attribute). The two-argument signature is the monkeypatch seam the
    semantic-diff guard test replaces.
    """
    table = getattr(rendered, "table", None)
    if not isinstance(table, str) or not table:
        raise ConfigError("internal rewrite is missing its table name")
    return _rewrite_named_table(content, table, rendered)


def _require_int(
    path: str,
    value: Any,
    *,
    positive: bool = False,
    bounds: tuple[int, int] | None = None,
) -> None:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ConfigError(f"{path} must be an integer; refusing {value!r}")
    if positive and value <= 0:
        raise ConfigError(f"{path} must be a positive integer; refusing {value}")
    if bounds is not None and not bounds[0] <= value <= bounds[1]:
        raise ConfigError(
            f"{path} must be between {bounds[0]} and {bounds[1]}; refusing {value}"
        )


def _require_bool(path: str, value: Any) -> None:
    if not isinstance(value, bool):
        raise ConfigError(f"{path} must be a boolean; refusing {value!r}")


def _validate_settings_change(table: str, key: str, value: Any) -> Any:
    """Strict write-time validation. Returns the value that will be persisted."""
    path = f"{table}.{key}"
    if table == "ui":
        if key == "inspector_width":
            _require_int(path, value, bounds=_INSPECTOR_WIDTH_BOUNDS)
            return value
        if key in {"inspector_dock_min_columns", "diff_side_by_side_min_columns"}:
            _require_int(path, value, positive=True)
            return value
        if key in {
            "reduced_motion",
            "inspector_open_at_start",
            "show_timestamps",
        }:
            _require_bool(path, value)
            return value
    if table == "composer":
        if key == "attachment_max_mb":
            _require_int(path, value, positive=True)
            return value
        if key in {"paste_collapse_lines", "paste_collapse_bytes"}:
            _require_int(path, value)
            return value
    if table == "notifications" and key == "transcript_line":
        _require_bool(path, value)
        return value
    if table == "keys":
        normalized = _valid_key(value)
        if normalized is None:
            raise ConfigError(f"{path} {value!r} is not a key Talaria recognizes")
        if normalized in RESERVED_KEYS:
            raise ConfigError(f"{path} {normalized!r} is reserved for quitting")
        return normalized
    if table == "environment" and key == "allowlist":
        if isinstance(value, (str, bytes)) or not isinstance(value, Sequence):
            raise ConfigError(f"{path} must be a list of strings; refusing {value!r}")
        if any(not isinstance(item, str) for item in value):
            raise ConfigError(f"{path} must be a list of strings; refusing {value!r}")
        return list(value)
    if table == "status":
        _validate_status_change(key, value)
        return value
    if table == "theme" and key == "name":
        if not isinstance(value, str) or not _THEME_SLUG_RE.fullmatch(value):
            raise ConfigError(f"invalid theme name: {value!r}")
        return value
    raise ConfigError(f"unknown key {path}")


def _validate_connection_changes(conn_id: str, changes: Mapping[str, Any]) -> dict[str, Any]:
    unknown = [key for key in changes if key not in _CONNECTION_FILE_KEYS]
    if unknown:
        raise ConfigError(
            f"connections.{conn_id}.{unknown[0]}"
        )
    validated: dict[str, Any] = {}
    if "url" in changes:
        url = changes["url"]
        if not isinstance(url, str) or not url.strip():
            raise ConfigError(f"connections.{conn_id}.url must be a string")
        if _url_has_userinfo(url):
            raise ConfigError(
                f"connections.{conn_id}.url carries a credential; "
                "refusing to write the secret"
            )
        validated["url"] = url
    if "auth" in changes:
        auth = changes["auth"]
        if auth not in _CONNECTION_AUTH_MODES:
            raise ConfigError(
                f"connections.{conn_id}.auth {auth!r} is not a recognized auth mode"
            )
        validated["auth"] = auth
    if "label" in changes:
        label = changes["label"]
        if not isinstance(label, str):
            raise ConfigError(f"connections.{conn_id}.label must be a string")
        validated["label"] = label
    return validated


def _expected_after_write(
    before: dict[str, Any], table: str, changes: Mapping[str, Any]
) -> dict[str, Any]:
    expected = deepcopy(before)
    if table.startswith("connections."):
        conn_id = table.split(".", 1)[1]
        inventory = expected.setdefault("connections", {})
        if not isinstance(inventory, dict):
            raise ConfigError("connections must be a table before it can be saved")
        entry = inventory.setdefault(conn_id, {})
        if not isinstance(entry, dict):
            raise ConfigError(f"{table} must be a table before it can be saved")
        entry.update(changes)
        return expected
    section = expected.setdefault(table, {})
    if not isinstance(section, dict):
        raise ConfigError(f"{table} must be a table before it can be saved")
    updated = dict(changes)
    if table == "status" and "segments" in updated:
        updated["segments"] = list(updated["segments"])
    if table == "environment" and "allowlist" in updated:
        updated["allowlist"] = list(updated["allowlist"])
    section.update(updated)
    return expected


def save_settings(
    table: str,
    changes: Mapping[str, Any],
    scope: ConfigSaveScope = "user",
    *,
    config_dir: Path | None = None,
    cwd: Path | None = None,
) -> Path:
    """Persist only the changed keys of one DEFAULTS table, surgically.

    ``table`` is a top-level DEFAULTS table or ``connections.<id>`` for one
    inventory entry. Validation is strict: loads fall back with a notice,
    writes refuse. Theme and status writes reuse the dedicated byte-preserving
    writers so an existing operator file does not change shape.
    """
    if not changes:
        raise ConfigError("save_settings needs at least one changed key")
    if table == "profiles" or table.startswith("profiles."):
        raise ConfigError(
            "profiles.endpoints is a compatibility alias; edit the file by hand"
        )
    if table.startswith("connections."):
        conn_id = table.split(".", 1)[1]
        if not conn_id or "." in conn_id:
            raise ConfigError(table)
        connection_changes = _validate_connection_changes(conn_id, changes)
        return _write_settings_table(
            table, connection_changes, scope, config_dir=config_dir, cwd=cwd
        )
    if table not in _WRITABLE_TABLES:
        raise ConfigError(table)
    allowed = _TABLE_WRITE_KEYS[table]
    unknown = [key for key in changes if key not in allowed]
    if unknown:
        raise ConfigError(f"{table}.{unknown[0]}")
    if table == "theme":
        return save_theme(str(changes["name"]), scope, config_dir=config_dir, cwd=cwd)
    if table == "status":
        return save_status_settings(changes, scope, config_dir=config_dir, cwd=cwd)

    validated: dict[str, Any] = {}
    for key, value in changes.items():
        validated[key] = _validate_settings_change(table, key, value)

    path = theme_config_path(scope, config_dir=config_dir, cwd=cwd)
    try:
        before_bytes = path.read_bytes() if path.is_file() else b""
    except OSError as exc:
        raise ConfigError(f"{path} could not be read: {exc}") from exc
    before = _parse_toml_bytes(path, before_bytes)

    if table == "keys":
        existing = before.get("keys")
        existing_map = dict(existing) if isinstance(existing, Mapping) else {}
        merged_keys = {**KEY_SETTING_DEFAULTS, **existing_map, **validated}
        by_chord: dict[str, list[str]] = {}
        for name, chord in merged_keys.items():
            if name not in KEY_SETTING_DEFAULTS:
                continue
            normalized = _valid_key(chord) or str(chord)
            by_chord.setdefault(normalized, []).append(name)
        for chord, names in by_chord.items():
            if len(names) > 1:
                raise ConfigError(
                    f"keys.{names[0]} and keys.{names[1]} are both {chord!r}"
                )

    return _write_settings_table(table, validated, scope, config_dir=config_dir, cwd=cwd)


def _write_settings_table(
    table: str,
    changes: Mapping[str, Any],
    scope: ConfigSaveScope,
    *,
    config_dir: Path | None,
    cwd: Path | None,
) -> Path:
    path = theme_config_path(scope, config_dir=config_dir, cwd=cwd)
    try:
        before_bytes = path.read_bytes() if path.is_file() else b""
    except OSError as exc:
        raise ConfigError(f"{path} could not be read: {exc}") from exc
    before = _parse_toml_bytes(path, before_bytes)
    existing = before.get(table.split(".", 1)[0] if table.startswith("connections.") else table)
    if existing is not None and table.startswith("connections."):
        inventory = existing
        if not isinstance(inventory, Mapping):
            raise ConfigError(f"{path} connections must be a table before it can be saved")
        conn_id = table.split(".", 1)[1]
        entry = inventory.get(conn_id)
        if entry is not None and not isinstance(entry, Mapping):
            raise ConfigError(f"{path} {table} must be a table before it can be saved")
    elif existing is not None and not isinstance(existing, Mapping):
        raise ConfigError(f"{path} {table} must be a table before it can be saved")

    rendered = _RenderedTable(
        table, {key: _render_toml_value(value) for key, value in changes.items()}
    )
    try:
        after_bytes = _rewrite_settings_table(before_bytes, rendered)
    except ConfigError as exc:
        raise ConfigError(f"{path}: {exc}") from exc
    try:
        after = _parse_toml_bytes(path, after_bytes)
    except ConfigError as exc:
        raise ConfigError(
            f"Talaria cannot safely rewrite the [{table}] keys in this form: {path}; "
            "edit the file by hand — no changes were written"
        ) from exc
    expected = _expected_after_write(before, table, changes)
    if after != expected:
        raise ConfigError(
            f"refusing to write {path}: the edit changed more than the requested keys"
        )
    try:
        atomic_replace_bytes(path, after_bytes, follow_symlinks=True)
    except OSError as exc:
        raise ConfigError(f"{path} could not be written: {exc}") from exc
    return path


def load_config(
    cli_overrides: Mapping[str, Any] | None = None,
    cwd: Path | None = None,
) -> Config:
    """Resolve KTD15's five-level precedence chain into a :class:`Config`.

    Highest first: ``cli_overrides``, ``TALARIA_*`` environment variables, a
    repo-local ``./.talaria/config.toml`` under ``cwd``, the global
    ``~/.talaria/config.toml`` (or its ``TALARIA_CONFIG_DIR`` redirection),
    and :data:`DEFAULTS`.

    Raises :class:`ConfigError` if a config file is unreadable or malformed, or
    if a ``TALARIA_*`` variable holds a value of the wrong type.
    """
    cwd = cwd if cwd is not None else Path.cwd()
    config_dir = global_config_dir()

    # deepcopy, not dict(): a shallow copy hands out the *same* nested section
    # objects DEFAULTS holds, so mutating a returned section would rewrite the
    # built-in defaults for the whole process.
    merged = deepcopy(DEFAULTS)
    merged = _deep_merge(merged, _read_toml(config_dir / "config.toml"))
    merged = _deep_merge(merged, _read_toml(cwd / ".talaria" / "config.toml"))
    merged = _deep_merge(merged, _env_overrides())
    if cli_overrides:
        # ``theme.name`` deliberately has no command-line override. Selection
        # belongs to the running application and persists to user configuration
        # automatically upon explicit selection in the UI.
        allowed_cli_overrides = dict(cli_overrides)
        allowed_cli_overrides.pop("theme", None)
        # Reduced motion stays restart-to-apply with no command-line alias.
        # D12's other ``[ui]`` keys may be supplied as launch overrides so
        # validation can run after precedence, the same way file values do.
        ui_cli = allowed_cli_overrides.get("ui")
        if isinstance(ui_cli, Mapping):
            filtered_ui = {
                key: value for key, value in ui_cli.items() if key != "reduced_motion"
            }
            if filtered_ui:
                allowed_cli_overrides["ui"] = filtered_ui
            else:
                allowed_cli_overrides.pop("ui", None)
        elif "ui" in allowed_cli_overrides:
            allowed_cli_overrides.pop("ui", None)
        merged = _deep_merge(merged, allowed_cli_overrides)

    user_theme_specs, user_theme_notices = load_user_theme_specs(
        config_dir=config_dir
    )
    user_theme_slugs = frozenset(spec.slug for spec in user_theme_specs)
    merged, notices = _normalize_config(
        merged,
        available_theme_slugs=_BUILTIN_THEME_SLUGS | user_theme_slugs,
    )
    return Config(
        values=_freeze(merged),
        config_dir=config_dir,
        notices=(*notices, *user_theme_notices),
    )


# ── which layer supplied each configuration-view key (issue #149) ────────

#: The provenance labels the configuration view displays. ``session`` is not
#: here because it is not a file layer: it is the view's own label for a
#: value the running process changed in memory — a ``/bar`` segment toggle —
#: and the view computes it by comparing the running set against this walk.
SettingScope = Literal["default", "user", "repository", "environment", "command line"]


def setting_scopes(
    cli_overrides: Mapping[str, Any] | None = None,
    *,
    cwd: Path | None = None,
    config_dir: Path | None = None,
) -> dict[tuple[str, str], str]:
    """Which precedence layer supplied each :data:`CONFIG_VIEW_KEYS` entry.

    Walks the same levels :func:`load_config` merges, in the same order, and
    records the last layer that carries each key. A key whose configured value
    was *invalid* still names the layer that supplied it: the view pairs that
    scope with the fallback notice ``load_config`` already produced, so the row
    reads as "invalid, using default" from the layer that caused it rather
    than hiding where the bad value came from.
    """
    cwd = cwd if cwd is not None else Path.cwd()
    root = config_dir if config_dir is not None else global_config_dir()
    scopes: dict[tuple[str, str], str] = {key: "default" for key in CONFIG_VIEW_KEYS}
    layers: list[tuple[str, Mapping[str, Any]]] = [
        ("user", _read_toml(root / "config.toml")),
        ("repository", _read_toml(cwd / ".talaria" / "config.toml")),
        ("environment", _env_overrides()),
    ]
    if cli_overrides:
        allowed = dict(cli_overrides)
        allowed.pop("theme", None)
        allowed.pop("ui", None)
        layers.append(("command line", allowed))
    for label, layer in layers:
        for section, key in CONFIG_VIEW_KEYS:
            node = layer.get(section)
            if isinstance(node, Mapping) and key in node:
                scopes[(section, key)] = label
    return scopes
