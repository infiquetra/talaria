"""KTD15 precedence-chain tests for talaria/config.py.

Every test redirects the global config directory via ``TALARIA_CONFIG_DIR``
(monkeypatched into the environment) so the operator's real ``~/.talaria`` is
never read or written by this suite.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from talaria import config as config_module
from talaria.config import credentials_path, load_config, recordings_dir


@pytest.fixture(autouse=True)
def isolated_global_config_dir(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    global_dir = tmp_path / "global-talaria"
    global_dir.mkdir()
    monkeypatch.setenv("TALARIA_CONFIG_DIR", str(global_dir))
    for env_name in config_module._ENV_KEY_MAP:
        monkeypatch.delenv(env_name, raising=False)
    return global_dir


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
