"""talaria/config.py — KTD15's configuration precedence chain.

The only module in this repository that reads settings from the filesystem.
Contents live under ``~/.talaria/`` (relocatable for tests via
``TALARIA_CONFIG_DIR``): ``config.toml`` for settings, ``credentials`` for the
attach credential, and ``recordings/`` for frame logs.

This module only *computes* the credential path. Creating that file, enforcing
its mode 0600, and reading it belong to U7's credential provider (KTD11) — no
code here opens it, so nothing here can leak its contents.

Precedence, highest first: an explicit command-line override, a ``TALARIA_*``
environment variable, a repo-local ``./.talaria/config.toml``, the global
``~/.talaria/config.toml`` (or its ``TALARIA_CONFIG_DIR`` redirection), and the
built-in default. Consumers (U5's status runner, U6, U7's credential provider)
call :func:`load_config` rather than touching the filesystem themselves.

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
import tomllib
from collections.abc import Mapping
from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import Any


class ConfigError(Exception):
    """A configuration source is unreadable or holds a value of the wrong type.

    Carries the offending file or environment variable in its message so the
    operator is pointed at the cause rather than at a traceback inside this
    module.
    """


#: Built-in defaults. Every key a later unit reads must have an entry here so
#: that a missing config file never produces a missing setting. The type of the
#: value recorded here is also what environment-variable overrides are coerced
#: to, so a default of ``None`` means "string-valued, no default".
DEFAULTS: dict[str, Any] = {
    "status": {"command": None, "interval_seconds": 5},
    "environment": {"allowlist": []},
    "composer": {"paste_collapse_lines": 6, "paste_collapse_bytes": 512},
    # U4's profile endpoints: a name-to-gateway-URL map the operator writes.
    # It has no environment-variable override and never will — a map cannot be
    # expressed as one ``TALARIA_*`` scalar, and inventing an encoding for it
    # would be a second config syntax. Empty by default, which means every
    # listed profile renders as "no endpoint configured" until the operator
    # says otherwise; that is the honest starting state rather than a guess.
    "profiles": {"endpoints": {}},
}

#: Maps a TALARIA_* environment variable to its (section, key) location in the
#: config tree. Extend this alongside DEFAULTS when a later unit adds a
#: setting; do not invent a parallel env-reading path elsewhere (KTD15).
_ENV_KEY_MAP: dict[str, tuple[str, str]] = {
    "TALARIA_STATUS_COMMAND": ("status", "command"),
    "TALARIA_STATUS_INTERVAL_SECONDS": ("status", "interval_seconds"),
    "TALARIA_COMPOSER_PASTE_COLLAPSE_LINES": ("composer", "paste_collapse_lines"),
    "TALARIA_COMPOSER_PASTE_COLLAPSE_BYTES": ("composer", "paste_collapse_bytes"),
}

_TRUE_LITERALS = frozenset({"1", "true", "yes", "on"})
_FALSE_LITERALS = frozenset({"0", "false", "no", "off"})


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
        overrides.setdefault(section, {})[key] = _coerce_env_value(env_name, raw, default)
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

    def get(self, *path: str, default: Any = None) -> Any:
        """Look up a dotted path, e.g. ``config.get("status", "command")``."""
        node: Any = self.values
        for part in path:
            if not isinstance(node, Mapping) or part not in node:
                return default
            node = node[part]
        return node


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
        merged = _deep_merge(merged, cli_overrides)

    return Config(values=_freeze(merged), config_dir=config_dir)
