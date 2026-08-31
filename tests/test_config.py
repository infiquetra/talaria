"""KTD15 precedence-chain tests for talaria/config.py.

The ``isolated_global_config_dir`` fixture these tests depend on is autouse and
lives in ``tests/conftest.py``, so the operator's real ``~/.talaria`` is never
read or written by any test in this suite.
"""

from __future__ import annotations

import os
import re
import stat
import tomllib
from pathlib import Path

import pytest

from talaria import config as config_module
from talaria.config import (
    DEFAULTS,
    ConfigError,
    credentials_path,
    load_config,
    recordings_dir,
    save_theme,
)
from talaria.themes import ThemeSpec
from talaria.themes.builtins import REFINED_DEFAULT
from talaria.themes.storage import serialize_user_theme
from tests.conftest import HERMES_DASHBOARD_TOKEN_VAR


def test_defaults_apply_when_nothing_else_is_set(tmp_path: Path) -> None:
    cfg = load_config(cwd=tmp_path)
    assert cfg.get("theme", "name") == "refined-default"
    assert cfg.notices == ()
    assert cfg.get("status", "command") is None
    assert cfg.get("status", "interval_seconds") == 5
    assert cfg.get("status", "segments") == (
        "cwd",
        "git_branch",
        "agent_model",
        "context",
        "task_progress",
        "connection",
        "version",
    )
    assert cfg.get("status", "cwd_max_columns") == 24
    assert cfg.get("status", "git_branch_max_columns") == 18
    assert cfg.get("status", "agent_model_max_columns") == 24
    assert cfg.get("ui", "reduced_motion") is False
    assert cfg.get("composer", "paste_collapse_lines") == 6
    assert cfg.get("composer", "paste_collapse_bytes") == 512


def test_theme_precedence_is_default_then_user_then_repository(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[theme]\nname = "dark-green-terminal"\n', encoding="utf-8"
    )
    repository = tmp_path / ".talaria"
    repository.mkdir()
    (repository / "config.toml").write_text(
        '[theme]\nname = "neutral-dark"\n', encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("theme", "name") == "neutral-dark"
    assert cfg.notices == ()


def test_invalid_winning_theme_falls_to_default_not_a_weaker_scope(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[theme]\nname = "dark-green-terminal"\n', encoding="utf-8"
    )
    repository = tmp_path / ".talaria"
    repository.mkdir()
    (repository / "config.toml").write_text(
        '[theme]\nname = "not-installed"\n', encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("theme", "name") == "refined-default"
    assert len(cfg.notices) == 1
    assert "not-installed" in cfg.notices[0]
    assert "Refined Default" in cfg.notices[0]


def test_non_string_theme_falls_back_with_an_immutable_notice(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[theme]\nname = 7\n", encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("theme", "name") == "refined-default"
    assert isinstance(cfg.notices, tuple)
    assert "must be a string" in cfg.notices[0]


def test_scalar_theme_is_normalized_to_a_table_with_a_shape_notice(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        'theme = "refined-default"\n', encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("theme", "name") == "refined-default"
    assert isinstance(cfg.get("theme", "name"), str)
    assert cfg.notices == (
        "theme must be a table with a name key; using Refined Default "
        "(refined-default)",
    )


def test_save_theme_rewrites_a_dotted_key_without_changing_neighbors(
    isolated_global_config_dir: Path,
) -> None:
    path = isolated_global_config_dir / "config.toml"
    before = (
        b"# operator comment\n"
        b'theme.name = "midnight-ink"  # keep this inline comment\n'
        b"[status]\n"
        b"interval_seconds = 7\n"
    )
    path.write_bytes(before)

    saved = save_theme("aurora-slate", config_dir=isolated_global_config_dir)

    expected = before.replace(b'"midnight-ink"', b'"aurora-slate"')
    assert saved == path
    assert path.read_bytes() == expected
    assert tomllib.loads(path.read_text(encoding="utf-8")) == {
        "theme": {"name": "aurora-slate"},
        "status": {"interval_seconds": 7},
    }


def test_save_theme_through_a_symlink_preserves_link_target_and_mode(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    dotfiles = tmp_path / "dotfiles"
    dotfiles.mkdir()
    target = dotfiles / "talaria.toml"
    target.write_text('[theme]\nname = "refined-default"\n', encoding="utf-8")
    target.chmod(0o640)
    original_mode = stat.S_IMODE(target.stat().st_mode)
    link = isolated_global_config_dir / "config.toml"
    link.symlink_to(target)

    save_theme("neutral-dark", config_dir=isolated_global_config_dir)

    assert link.is_symlink()
    assert tomllib.loads(target.read_text(encoding="utf-8")) == {
        "theme": {"name": "neutral-dark"}
    }
    assert stat.S_IMODE(target.stat().st_mode) == original_mode


@pytest.mark.parametrize(
    ("state", "content"),
    [
        (
            "extra-field",
            '{"dark":true,"extra":1,"name":"Broken","slug":"broken","tokens":{}}',
        ),
        ("unrelated-json", '{"bogus":1}'),
        ("truncated-json", '{"dark":'),
        ("empty-file", ""),
        (
            "missing-tokens",
            '{"dark":true,"name":"Broken","slug":"broken","tokens":{}}',
        ),
    ],
)
def test_broken_stored_themes_are_skipped_without_hiding_valid_themes(
    isolated_global_config_dir: Path,
    state: str,
    content: str,
) -> None:
    themes = isolated_global_config_dir / "themes"
    themes.mkdir()
    broken = themes / f"broken-{state}.json"
    broken.write_text(content, encoding="utf-8")
    valid = ThemeSpec(
        slug="valid-user",
        name="Valid User",
        dark=REFINED_DEFAULT.dark,
        tokens=REFINED_DEFAULT.tokens,
    )
    (themes / "valid-user.json").write_bytes(serialize_user_theme(valid))
    (isolated_global_config_dir / "config.toml").write_text(
        '[theme]\nname = "valid-user"\n',
        encoding="utf-8",
    )

    cfg = load_config()

    assert cfg.get("theme", "name") == "valid-user"
    assert len(cfg.notices) == 1
    assert str(broken) in cfg.notices[0]
    assert "skipped" in cfg.notices[0]


def test_a_stored_imported_theme_slug_is_accepted_at_startup(
    isolated_global_config_dir: Path,
) -> None:
    themes = isolated_global_config_dir / "themes"
    themes.mkdir()
    imported = ThemeSpec(
        slug="stored-import",
        name="Stored Import",
        dark=REFINED_DEFAULT.dark,
        tokens=REFINED_DEFAULT.tokens,
    )
    (themes / "stored-import.json").write_bytes(serialize_user_theme(imported))
    (isolated_global_config_dir / "config.toml").write_text(
        '[theme]\nname = "stored-import"\n',
        encoding="utf-8",
    )

    cfg = load_config()

    assert cfg.get("theme", "name") == "stored-import"
    assert cfg.notices == ()


def test_theme_has_no_command_line_override(tmp_path: Path) -> None:
    cfg = load_config(
        cli_overrides={"theme": {"name": "neutral-dark"}}, cwd=tmp_path
    )

    assert cfg.get("theme", "name") == "refined-default"


def test_reduced_motion_precedence_is_default_then_user_then_repository(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[ui]\nreduced_motion = true\n", encoding="utf-8"
    )
    repository = tmp_path / ".talaria"
    repository.mkdir()
    (repository / "config.toml").write_text(
        "[ui]\nreduced_motion = false\n", encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("ui", "reduced_motion") is False
    assert cfg.notices == ()


def test_invalid_winning_reduced_motion_uses_false_not_a_weaker_scope(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[ui]\nreduced_motion = true\n", encoding="utf-8"
    )
    repository = tmp_path / ".talaria"
    repository.mkdir()
    (repository / "config.toml").write_text(
        '[ui]\nreduced_motion = "yes"\n', encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("ui", "reduced_motion") is False
    assert isinstance(cfg.notices, tuple)
    assert cfg.notices == ("ui.reduced_motion must be a boolean; using false",)


def test_reduced_motion_has_no_environment_or_command_line_alias(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("TALARIA_UI_REDUCED_MOTION", "true")

    cfg = load_config(
        cli_overrides={"ui": {"reduced_motion": True}},
        cwd=tmp_path,
    )

    assert cfg.get("ui", "reduced_motion") is False
    assert cfg.notices == ()


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


def test_malformed_status_interval_env_value_falls_back_visibly(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("TALARIA_STATUS_INTERVAL_SECONDS", "--5")

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("status", "interval_seconds") == 5
    assert any(
        "status.interval_seconds" in notice and "using 5" in notice
        for notice in cfg.notices
    )


@pytest.mark.parametrize("bad", (-1, 0, 3601, "5", 2.5, True))
def test_invalid_winning_status_interval_uses_default_with_a_notice(
    tmp_path: Path, bad: object
) -> None:
    cfg = load_config(cli_overrides={"status": {"interval_seconds": bad}}, cwd=tmp_path)

    assert cfg.get("status", "interval_seconds") == 5
    assert any(
        "status.interval_seconds" in notice and "using 5" in notice
        for notice in cfg.notices
    )


@pytest.mark.parametrize("valid", (1, 17, 3600))
def test_valid_status_interval_counterexamples_survive_normalization(
    tmp_path: Path, valid: int
) -> None:
    cfg = load_config(
        cli_overrides={"status": {"interval_seconds": valid}}, cwd=tmp_path
    )

    assert cfg.get("status", "interval_seconds") == valid
    assert not any("status.interval_seconds" in notice for notice in cfg.notices)


@pytest.mark.parametrize(
    ("key", "bad", "fallback", "valid"),
    [
        ("cwd_max_columns", 7, 24, 48),
        ("cwd_max_columns", "24", 24, 8),
        ("git_branch_max_columns", 41, 18, 40),
        ("git_branch_max_columns", False, 18, 8),
        ("agent_model_max_columns", 9, 24, 48),
        ("agent_model_max_columns", 24.0, 24, 10),
    ],
)
def test_status_width_caps_validate_type_and_range_after_precedence(
    tmp_path: Path,
    key: str,
    bad: object,
    fallback: int,
    valid: int,
) -> None:
    invalid = load_config(cli_overrides={"status": {key: bad}}, cwd=tmp_path)
    accepted = load_config(cli_overrides={"status": {key: valid}}, cwd=tmp_path)

    assert invalid.get("status", key) == fallback
    assert any(f"status.{key}" in notice for notice in invalid.notices)
    assert accepted.get("status", key) == valid
    assert not any(f"status.{key}" in notice for notice in accepted.notices)


def test_status_segment_order_is_normalized_once_and_frozen(tmp_path: Path) -> None:
    cfg = load_config(
        cli_overrides={
            "status": {
                "segments": ["version", "unknown", "connection", "version"]
            }
        },
        cwd=tmp_path,
    )

    assert cfg.get("status", "segments") == ("version", "connection")
    assert any("unknown segment" in notice for notice in cfg.notices)
    assert any("duplicate" in notice for notice in cfg.notices)

    fallback = load_config(
        cli_overrides={"status": {"segments": ["unknown"]}}, cwd=tmp_path
    )
    assert fallback.get("status", "segments") == ("connection",)
    assert any("connection only" in notice for notice in fallback.notices)


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


def test_v050_user_guide_toml_examples_parse_and_match_runtime_defaults() -> None:
    """Every TOML fence added with the v0.5 guides is executable documentation."""
    repository = Path(__file__).resolve().parents[1]
    fence = re.compile(r"(?ms)^```toml\n(.*?)^```[ \t]*$")
    documents = (
        repository / "docs" / "themes.md",
        repository / "docs" / "configuration.md",
        repository / "docs" / "terminal-ui.md",
    )

    examples = [
        tomllib.loads(source)
        for document in documents
        for source in fence.findall(document.read_text(encoding="utf-8"))
    ]

    assert len(examples) == 1
    example = examples[0]
    assert example["theme"] == DEFAULTS["theme"]
    assert example["ui"] == DEFAULTS["ui"]
    assert example["status"] == {
        key: value for key, value in DEFAULTS["status"].items() if key != "command"
    }
    assert example["environment"] == DEFAULTS["environment"]
    assert example["composer"] == DEFAULTS["composer"]
    assert example["profiles"] == DEFAULTS["profiles"]
