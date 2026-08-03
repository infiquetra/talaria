"""talaria/config.py — KTD15's configuration precedence chain.

The only module in this repository that reads settings from the filesystem.
Contents live under ``~/.talaria/`` (relocatable for tests via
``TALARIA_CONFIG_DIR``): ``config.toml`` for settings, ``credentials`` (mode
0600, KTD11) for the attach credential, and ``recordings/`` for frame logs.

Precedence, highest first: an explicit command-line override, a ``TALARIA_*``
environment variable, a repo-local ``./.talaria/config.toml``, the global
``~/.talaria/config.toml`` (or its ``TALARIA_CONFIG_DIR`` redirection), and the
built-in default. Consumers (U5's status runner, U6, U7's credential provider)
call :func:`load_config` rather than touching the filesystem themselves.
"""

from __future__ import annotations

import os
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

#: Built-in defaults. Every key a later unit reads must have an entry here so
#: that a missing config file never produces a missing setting.
DEFAULTS: dict[str, Any] = {
    "status": {"command": None, "interval_seconds": 5},
    "environment": {"allowlist": []},
    "composer": {"paste_collapse_lines": 6, "paste_collapse_bytes": 512},
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
    """The KTD11 credential file: ``<config_dir>/credentials``, mode 0600."""
    return (config_dir if config_dir is not None else global_config_dir()) / "credentials"


def recordings_dir(config_dir: Path | None = None) -> Path:
    """The frame-log directory: ``<config_dir>/recordings/`` (git-ignored, R29)."""
    return (config_dir if config_dir is not None else global_config_dir()) / "recordings"


def _read_toml(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {}
    with path.open("rb") as fh:
        return tomllib.load(fh)


def _deep_merge(base: dict[str, Any], override: dict[str, Any]) -> dict[str, Any]:
    """Merge ``override`` onto ``base``, recursing into nested dicts only."""
    result = dict(base)
    for key, value in override.items():
        if isinstance(value, dict) and isinstance(result.get(key), dict):
            result[key] = _deep_merge(result[key], value)
        else:
            result[key] = value
    return result


def _coerce_env_value(raw: str) -> Any:
    if raw.lstrip("-").isdigit():
        return int(raw)
    return raw


def _env_overrides() -> dict[str, Any]:
    overrides: dict[str, Any] = {}
    for env_name, (section, key) in _ENV_KEY_MAP.items():
        raw = os.environ.get(env_name)
        if raw is None:
            continue
        overrides.setdefault(section, {})[key] = _coerce_env_value(raw)
    return overrides


@dataclass(frozen=True)
class Config:
    """A fully resolved configuration snapshot."""

    values: dict[str, Any]
    config_dir: Path

    def get(self, *path: str, default: Any = None) -> Any:
        """Look up a dotted path, e.g. ``config.get("status", "command")``."""
        node: Any = self.values
        for part in path:
            if not isinstance(node, dict) or part not in node:
                return default
            node = node[part]
        return node


def load_config(
    cli_overrides: dict[str, Any] | None = None,
    cwd: Path | None = None,
) -> Config:
    """Resolve KTD15's five-level precedence chain into a :class:`Config`.

    Highest first: ``cli_overrides``, ``TALARIA_*`` environment variables, a
    repo-local ``./.talaria/config.toml`` under ``cwd``, the global
    ``~/.talaria/config.toml`` (or its ``TALARIA_CONFIG_DIR`` redirection),
    and :data:`DEFAULTS`.
    """
    cwd = cwd if cwd is not None else Path.cwd()
    config_dir = global_config_dir()

    merged = dict(DEFAULTS)
    merged = _deep_merge(merged, _read_toml(config_dir / "config.toml"))
    merged = _deep_merge(merged, _read_toml(cwd / ".talaria" / "config.toml"))
    merged = _deep_merge(merged, _env_overrides())
    if cli_overrides:
        merged = _deep_merge(merged, cli_overrides)

    return Config(values=merged, config_dir=config_dir)
