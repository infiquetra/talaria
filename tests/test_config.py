"""KTD15 precedence-chain tests for talaria/config.py.

The ``isolated_global_config_dir`` fixture these tests depend on is autouse and
lives in ``tests/conftest.py``, so the operator's real ``~/.talaria`` is never
read or written by any test in this suite.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from talaria import config as config_module
from talaria.config import (
    DEFAULTS,
    ConfigError,
    credentials_path,
    load_config,
    recordings_dir,
)


def test_defaults_apply_when_nothing_else_is_set(tmp_path: Path) -> None:
    cfg = load_config(cwd=tmp_path)
    assert cfg.get("status", "command") is None
    assert cfg.get("status", "interval_seconds") == 5
    assert cfg.get("composer", "paste_collapse_lines") == 6
    assert cfg.get("composer", "paste_collapse_bytes") == 512


def test_global_config_toml_overrides_default(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[status]\ncommand = "git status"\ninterval_seconds = 9\n'
    )
    cfg = load_config(cwd=tmp_path)
    assert cfg.get("status", "command") == "git status"
    assert cfg.get("status", "interval_seconds") == 9


def test_repo_local_config_overrides_global(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[status]\ncommand = "global-status"\n'
    )
    repo_local_dir = tmp_path / ".talaria"
    repo_local_dir.mkdir()
    (repo_local_dir / "config.toml").write_text('[status]\ncommand = "repo-status"\n')

    cfg = load_config(cwd=tmp_path)
    assert cfg.get("status", "command") == "repo-status"


def test_environment_variable_overrides_repo_local_config(
    isolated_global_config_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo_local_dir = tmp_path / ".talaria"
    repo_local_dir.mkdir()
    (repo_local_dir / "config.toml").write_text('[status]\ncommand = "repo-status"\n')
    monkeypatch.setenv("TALARIA_STATUS_COMMAND", "env-status")

    cfg = load_config(cwd=tmp_path)
    assert cfg.get("status", "command") == "env-status"


def test_cli_override_beats_environment_variable(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("TALARIA_STATUS_COMMAND", "env-status")

    cfg = load_config(cli_overrides={"status": {"command": "cli-status"}}, cwd=tmp_path)
    assert cfg.get("status", "command") == "cli-status"


def test_full_five_level_chain_resolves_to_the_highest_precedence(
    isolated_global_config_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[composer]\npaste_collapse_lines = 1\npaste_collapse_bytes = 1\n'
    )
    repo_local_dir = tmp_path / ".talaria"
    repo_local_dir.mkdir()
    (repo_local_dir / "config.toml").write_text(
        '[composer]\npaste_collapse_lines = 2\npaste_collapse_bytes = 2\n'
    )
    monkeypatch.setenv("TALARIA_COMPOSER_PASTE_COLLAPSE_LINES", "3")

    cfg = load_config(
        cli_overrides={"composer": {"paste_collapse_lines": 4}},
        cwd=tmp_path,
    )

    # cli_overrides wins outright for the key it sets.
    assert cfg.get("composer", "paste_collapse_lines") == 4
    # bytes is untouched by cli_overrides and the env var, so repo-local
    # (2) must win over global (1).
    assert cfg.get("composer", "paste_collapse_bytes") == 2


def test_talaria_config_dir_redirects_credentials_and_recordings_paths(
    isolated_global_config_dir: Path,
) -> None:
    assert credentials_path() == isolated_global_config_dir / "credentials"
    assert recordings_dir() == isolated_global_config_dir / "recordings"


def test_config_dir_defaults_to_home_talaria_without_the_env_override(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("TALARIA_CONFIG_DIR", raising=False)
    assert config_module.global_config_dir() == Path.home() / ".talaria"


def test_repo_local_level_is_isolated_to_tmp_path(tmp_path: Path) -> None:
    """A ``load_config()`` with no ``cwd`` must not reach the real repository.

    The repo-local level resolves against ``Path.cwd()``, so without the
    conftest fixture's ``chdir`` this call would read the repository's own
    git-ignored ``.talaria/config.toml`` and pass or fail on machine state.
    """
    repo_local = tmp_path / ".talaria"
    repo_local.mkdir()
    (repo_local / "config.toml").write_text('[status]\ncommand = "tmp-scoped"\n')

    cfg = load_config()  # deliberately no cwd argument

    assert Path.cwd().resolve() == tmp_path.resolve()
    assert cfg.get("status", "command") == "tmp-scoped"


def test_string_setting_keeps_a_numeric_looking_env_value_as_a_string(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """status.command is executed as an argument array (KTD5), never as an int."""
    monkeypatch.setenv("TALARIA_STATUS_COMMAND", "42")

    cfg = load_config(cwd=tmp_path)
    assert cfg.get("status", "command") == "42"
    assert isinstance(cfg.get("status", "command"), str)


def test_integer_setting_coerces_a_numeric_env_value(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("TALARIA_STATUS_INTERVAL_SECONDS", "11")

    cfg = load_config(cwd=tmp_path)
    assert cfg.get("status", "interval_seconds") == 11


def test_malformed_integer_env_value_names_the_variable(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("TALARIA_STATUS_INTERVAL_SECONDS", "--5")

    with pytest.raises(ConfigError) as excinfo:
        load_config(cwd=tmp_path)
    assert "TALARIA_STATUS_INTERVAL_SECONDS" in str(excinfo.value)


def test_malformed_toml_names_the_offending_file(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    bad = isolated_global_config_dir / "config.toml"
    bad.write_text("[status\ncommand = ")

    with pytest.raises(ConfigError) as excinfo:
        load_config(cwd=tmp_path)
    assert str(bad) in str(excinfo.value)


def test_loading_config_never_aliases_the_builtin_defaults(tmp_path: Path) -> None:
    """A shallow copy of DEFAULTS would hand out its own nested section objects."""
    cfg = load_config(cwd=tmp_path)
    assert cfg.get("environment", "allowlist") is not DEFAULTS["environment"]["allowlist"]
    assert DEFAULTS["environment"]["allowlist"] == []


def test_resolved_config_is_deeply_immutable(tmp_path: Path) -> None:
    """frozen=True is cosmetic unless the nested containers are read-only too."""
    cfg = load_config(cwd=tmp_path)

    with pytest.raises(TypeError):
        cfg.values["status"]["interval_seconds"] = 999
    with pytest.raises(AttributeError):
        cfg.get("environment", "allowlist").append("HOME")

    assert cfg.get("status", "interval_seconds") == 5
    assert DEFAULTS["status"]["interval_seconds"] == 5
