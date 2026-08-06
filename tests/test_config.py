"""KTD15 precedence-chain tests for talaria/config.py.

The ``isolated_global_config_dir`` fixture these tests depend on is autouse and
lives in ``tests/conftest.py``, so the operator's real ``~/.talaria`` is never
read or written by any test in this suite.
"""

from __future__ import annotations

import os
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
from tests.conftest import HERMES_DASHBOARD_TOKEN_VAR


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


#: The names an operator's shell can hold that would change what the suite reads
#: or dials. Written out literally rather than read back from the modules that
#: use them: a list derived from the thing under test survives deleting an entry
#: from that thing.
LEAKABLE_ENV_NAMES = (
    "TALARIA_GATEWAY_URL",
    "TALARIA_PROFILE",
    "TALARIA_LOG_LEVEL",
    "TALARIA_STATUS_INTERVAL",
    "TALARIA_STATUS_COMMAND",
    "TALARIA_STATUS_INTERVAL_SECONDS",
    "TALARIA_COMPOSER_PASTE_COLLAPSE_LINES",
    "TALARIA_COMPOSER_PASTE_COLLAPSE_BYTES",
)


def test_no_talaria_variable_from_the_shell_is_visible_inside_a_test() -> None:
    """The state every other test in this suite is entitled to assume."""
    for name in LEAKABLE_ENV_NAMES:
        assert name not in os.environ, f"{name} leaked into the suite from the shell"
    # The positive half: the fixture does set the one variable it owns, so a
    # fixture that had simply emptied the environment could not pass this.
    assert os.environ.get("TALARIA_CONFIG_DIR"), "the isolation fixture ran no setup"
    assert HERMES_DASHBOARD_TOKEN_VAR not in os.environ


def test_the_isolation_fixture_clears_variables_the_shell_actually_exported(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The fixture is run *against a polluted environment*, which is the only
    way this claim can fail on a clean developer machine.

    The test above passes trivially here: nothing exports ``TALARIA_GATEWAY_URL``
    on this machine, so it would stay green with the sweep deleted entirely.
    This one sets every leakable name first and then calls the real fixture
    function, so the sweep is the thing under test rather than the shell.

    That matters because the sweep replaced a narrower one.
    ``tests/conftest.py`` used to iterate ``config._ENV_KEY_MAP`` — four names,
    not including ``TALARIA_GATEWAY_URL``, which is what decides where a live
    dial would attach. On a machine with a Hermes dashboard listening, that is
    the difference between a test and a real session; U10 came within one
    unexported variable of finding out.
    """
    import tests.conftest as suite_conftest

    for name in LEAKABLE_ENV_NAMES:
        monkeypatch.setenv(name, f"shell-value-for-{name}")
    monkeypatch.setenv(HERMES_DASHBOARD_TOKEN_VAR, "shell-token")
    # The positive control, in the same observation: the pollution is really
    # there before the fixture runs.
    assert all(name in os.environ for name in LEAKABLE_ENV_NAMES)
    assert HERMES_DASHBOARD_TOKEN_VAR in os.environ

    inner_root = tmp_path / "inner"
    inner_root.mkdir()
    inner = pytest.MonkeyPatch()
    try:
        # ``__wrapped__`` is the undecorated function pytest stores on the
        # fixture object; calling the fixture itself would raise, and
        # duplicating its body here would test a copy rather than the fixture
        # every other test in this suite actually uses.
        fixture_body = suite_conftest.isolated_global_config_dir.__wrapped__  # type: ignore[attr-defined]
        global_dir = fixture_body(inner_root, inner)
        leaked = [name for name in LEAKABLE_ENV_NAMES if name in os.environ]
        assert not leaked, f"the isolation fixture left these set: {leaked}"
        assert HERMES_DASHBOARD_TOKEN_VAR not in os.environ
        assert os.environ["TALARIA_CONFIG_DIR"] == str(global_dir)
    finally:
        inner.undo()


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


# ── U4: the profile endpoint map ──────────────────────────────────────────
#
# Hermes publishes no address for a profile's gateway, so this is where one
# comes from. Every profile name below is synthetic (R12): this is a public
# repository and the real inventory is the operator's.


def test_no_profile_endpoint_is_configured_by_default(tmp_path: Path) -> None:
    """The honest starting state: every profile reads as unaddressable."""
    cfg = load_config(cwd=tmp_path)
    assert config_module.profile_endpoints(cfg) == {}


def test_the_operator_supplies_profile_endpoints_in_config_toml(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[profiles.endpoints]\n"
        'alpha-fixture = "ws://127.0.0.1:9119/api/ws"\n'
        'beta-fixture = "ws://127.0.0.1:9120/api/ws"\n'
    )
    cfg = load_config(cwd=tmp_path)
    assert config_module.profile_endpoints(cfg) == {
        "alpha-fixture": "ws://127.0.0.1:9119/api/ws",
        "beta-fixture": "ws://127.0.0.1:9120/api/ws",
    }


def test_a_non_string_or_blank_endpoint_is_dropped_rather_than_coerced(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    """The value is a URL that will be dialled; ``str(7)`` is not one."""
    (isolated_global_config_dir / "config.toml").write_text(
        "[profiles.endpoints]\n"
        'alpha-fixture = "ws://127.0.0.1:9119/api/ws"\n'
        "beta-fixture = 9120\n"
        'gamma-fixture = "   "\n'
    )
    cfg = load_config(cwd=tmp_path)
    assert config_module.profile_endpoints(cfg) == {
        "alpha-fixture": "ws://127.0.0.1:9119/api/ws"
    }


def test_a_profiles_section_of_the_wrong_shape_yields_no_endpoints(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[profiles]\nendpoints = 7\n"
    )
    cfg = load_config(cwd=tmp_path)
    assert config_module.profile_endpoints(cfg) == {}
