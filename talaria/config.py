"""talaria/config.py — KTD15's configuration precedence chain.

The only module in this repository that reads settings from the filesystem,
and the owner of Talaria's narrow explicit theme-setting write.
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
``theme.name`` deliberately has no environment or command-line override; its
later session scope is in memory and only :func:`save_theme` persists it.

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
import tempfile
import tomllib
from collections.abc import Mapping
from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import Any, Literal

from talaria.status.contract import (
    DEFAULT_AGENT_MODEL_MAX_COLUMNS,
    DEFAULT_CWD_MAX_COLUMNS,
    DEFAULT_GIT_BRANCH_MAX_COLUMNS,
    DEFAULT_STATUS_SEGMENTS,
    normalize_status_settings,
    parse_command,
)
from talaria.themes.builtins import BUILTIN_THEMES, REFINED_DEFAULT


class ConfigError(Exception):
    """A configuration source or surgical configuration write is invalid.

    Carries the offending file or environment variable in its message so the
    operator is pointed at the cause rather than at a traceback inside this
    module.
    """


#: Built-in defaults. Every key a later unit reads must have an entry here so
#: that a missing config file never produces a missing setting. The type of the
#: value recorded here is also what environment-variable overrides are coerced
#: to, so a default of ``None`` means "string-valued, no default".
DEFAULTS: dict[str, Any] = {
    "theme": {"name": REFINED_DEFAULT.slug},
    "status": {
        "command": None,
        "interval_seconds": 5,
        "segments": list(DEFAULT_STATUS_SEGMENTS),
        "cwd_max_columns": DEFAULT_CWD_MAX_COLUMNS,
        "git_branch_max_columns": DEFAULT_GIT_BRANCH_MAX_COLUMNS,
        "agent_model_max_columns": DEFAULT_AGENT_MODEL_MAX_COLUMNS,
    },
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

ThemeSaveScope = Literal["user", "repository"]

_BUILTIN_THEME_SLUGS = frozenset(theme.slug for theme in BUILTIN_THEMES)
_THEME_SLUG_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
_THEME_HEADER_RE = re.compile(
    rb"(?m)^[ \t]*\[theme\][ \t]*(?:\#[^\r\n]*)?(?:\r?\n|$)"
)
_TABLE_HEADER_RE = re.compile(rb"(?m)^[ \t]*\[\[?[^\r\n]+\]\]?[ \t]*(?:\#[^\r\n]*)?(?:\r?\n|$)")
_THEME_NAME_RE = re.compile(
    rb"(?m)^[ \t]*(?:name|\"name\"|'name')[ \t]*=[^\r\n]*(?:\r?\n|$)"
)


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


def _normalize_config(merged: dict[str, Any]) -> tuple[dict[str, Any], tuple[str, ...]]:
    """Normalize winning configured values once, after every file layer merged."""
    notices: list[str] = []
    theme = merged.get("theme")
    requested = theme.get("name") if isinstance(theme, Mapping) else theme
    if not isinstance(requested, str):
        merged["theme"] = {"name": REFINED_DEFAULT.slug}
        notices.append(
            "theme.name must be a string; using Refined Default "
            f"({REFINED_DEFAULT.slug})"
        )
    elif requested not in _BUILTIN_THEME_SLUGS:
        merged["theme"] = {"name": REFINED_DEFAULT.slug}
        notices.append(
            f"theme {requested!r} is not available; using Refined Default "
            f"({REFINED_DEFAULT.slug})"
        )

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


def atomic_replace_bytes(path: Path, content: bytes) -> None:
    """Atomically replace ``path`` from a temporary file in the same directory."""
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, raw_temp = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temp_path = Path(raw_temp)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temp_path, path)
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

    after_bytes = _rewrite_theme_name(before_bytes, name)
    after = _parse_toml_bytes(path, after_bytes)
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
        atomic_replace_bytes(path, after_bytes)
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
        # ``theme.name`` deliberately has no command-line override. Session
        # selection belongs to the running application and persists only when
        # the operator explicitly invokes ``/theme save``.
        allowed_cli_overrides = dict(cli_overrides)
        allowed_cli_overrides.pop("theme", None)
        merged = _deep_merge(merged, allowed_cli_overrides)

    merged, notices = _normalize_config(merged)
    return Config(values=_freeze(merged), config_dir=config_dir, notices=notices)
