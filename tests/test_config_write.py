"""Issue #104's explicit, surgical ``[theme]`` persistence contract, and
issue #149's generalization of it to the configuration view's status keys."""

from __future__ import annotations

import os
import stat
import tomllib
from pathlib import Path

import pytest

from talaria import config as config_module
from talaria.config import ConfigError, save_status_settings, save_theme


def test_atomic_replace_of_symlink_uses_default_mode_and_leaves_target_untouched(
    tmp_path: Path,
) -> None:
    outside = tmp_path / "outside-theme.json"
    outside_bytes = b'{"name":"outside"}\n'
    outside.write_bytes(outside_bytes)
    outside.chmod(0o666)
    link = tmp_path / "stored-theme.json"
    link.symlink_to(outside)

    previous_umask = os.umask(0o022)
    try:
        config_module.atomic_replace_bytes(link, b'{"name":"replacement"}\n')
    finally:
        os.umask(previous_umask)

    assert not link.is_symlink()
    assert link.read_bytes() == b'{"name":"replacement"}\n'
    assert stat.S_IMODE(link.stat().st_mode) == 0o644
    assert outside.read_bytes() == outside_bytes


def test_user_save_creates_only_the_theme_table(tmp_path: Path) -> None:
    config_dir = tmp_path / "user"

    path = save_theme("neutral-dark", config_dir=config_dir)

    assert path == config_dir / "config.toml"
    assert path.read_bytes() == b'[theme]\nname = "neutral-dark"\n'


def test_repository_save_uses_the_explicit_repository_target(tmp_path: Path) -> None:
    path = save_theme(
        "accessible-high-contrast", "repository", cwd=tmp_path
    )

    assert path == tmp_path / ".talaria" / "config.toml"
    assert tomllib.loads(path.read_text(encoding="utf-8")) == {
        "theme": {"name": "accessible-high-contrast"}
    }


def test_replace_preserves_comments_sibling_tables_and_theme_neighbors(
    tmp_path: Path,
) -> None:
    path = tmp_path / "config.toml"
    before = (
        b"# operator comment\n"
        b"[status]\n"
        b'command = "git status"\n'
        b"\n"
        b"[theme] # theme comment\n"
        b"# keep this theme note\n"
        b'name = "refined-default"  # keep this inline comment\n'
        b"\n"
        b"[profiles.endpoints]\n"
        b'dev = "ws://127.0.0.1:9119/api/ws"\n'
    )
    path.write_bytes(before)

    save_theme("dark-green-terminal", config_dir=tmp_path)

    expected = before.replace(
        b'name = "refined-default"', b'name = "dark-green-terminal"'
    )
    assert path.read_bytes() == expected


def test_atomic_replace_failure_keeps_the_original_and_removes_the_temp(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "config.toml"
    original = b'[theme]\nname = "refined-default"\n'
    path.write_bytes(original)

    def refuse_replace(source: object, destination: object) -> None:
        del source, destination
        raise OSError("replace refused")

    monkeypatch.setattr(os, "replace", refuse_replace)

    with pytest.raises(ConfigError, match="replace refused"):
        save_theme("neutral-dark", config_dir=tmp_path)

    assert path.read_bytes() == original
    assert list(tmp_path.glob(".config.toml.*")) == []


def test_semantic_diff_guard_refuses_a_neighboring_change(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "config.toml"
    original = b'[status]\ninterval_seconds = 5\n'
    path.write_bytes(original)

    def corrupt_rewrite(content: bytes, name: str) -> bytes:
        del content, name
        return (
            b'[status]\ninterval_seconds = 9\n'
            b'[theme]\nname = "neutral-dark"\n'
        )

    monkeypatch.setattr(config_module, "_rewrite_theme_name", corrupt_rewrite)

    with pytest.raises(ConfigError, match="changed more than theme.name"):
        save_theme("neutral-dark", config_dir=tmp_path)

    assert path.read_bytes() == original


def test_invalid_existing_toml_is_never_replaced(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    original = b"[theme\n"
    path.write_bytes(original)

    with pytest.raises(ConfigError, match="not valid TOML"):
        save_theme("neutral-dark", config_dir=tmp_path)

    assert path.read_bytes() == original


# ── issue #149: the configuration view's status write (D8 recorded) ───────


def test_status_save_appends_the_documented_block_when_no_status_shape_exists(
    tmp_path: Path,
) -> None:
    path = save_status_settings(
        {
            "command": "git status --short",
            "interval_seconds": 30,
            "segments": ("cwd", "version"),
        },
        config_dir=tmp_path,
    )

    assert path == tmp_path / "config.toml"
    assert path.read_bytes() == (
        b'[status]\n'
        b'command = "git status --short"\n'
        b"interval_seconds = 30\n"
        b"segments = [\n"
        b'  "cwd",\n'
        b'  "version",\n'
        b"]\n"
    )


def test_status_save_replaces_values_and_appends_missing_keys_in_place(
    tmp_path: Path,
) -> None:
    path = tmp_path / "config.toml"
    before = (
        b"# operator comment\n"
        b"[status]\n"
        b'command = "git status"\n'
        b"interval_seconds = 5\n"
        b"\n"
        b"[theme]\n"
        b'name = "refined-default"\n'
    )
    path.write_bytes(before)

    save_status_settings(
        {"interval_seconds": 30, "segments": ("cwd", "version")}, config_dir=tmp_path
    )

    expected = before.replace(b"interval_seconds = 5", b"interval_seconds = 30")
    expected = expected.replace(
        b"\n\n[theme]",
        b'\n\nsegments = [\n  "cwd",\n  "version",\n]\n[theme]',
    )
    assert path.read_bytes() == expected
    assert tomllib.loads(path.read_text(encoding="utf-8")) == {
        "status": {
            "command": "git status",
            "interval_seconds": 30,
            "segments": ["cwd", "version"],
        },
        "theme": {"name": "refined-default"},
    }


def test_status_save_preserves_comments_around_a_multiline_array(
    tmp_path: Path,
) -> None:
    path = tmp_path / "config.toml"
    before = (
        b"[status]\n"
        b"# keep this note\n"
        b"segments = [\n"
        b'  "cwd",\n'
        b'  "version",\n'
        b"]  # keep this trailing note\n"
        b'command = "git status"  # keep this inline comment\n'
    )
    path.write_bytes(before)

    save_status_settings(
        {"segments": ("version", "cwd"), "command": "echo hi"}, config_dir=tmp_path
    )

    expected = (
        b"[status]\n"
        b"# keep this note\n"
        b"segments = [\n"
        b'  "version",\n'
        b'  "cwd",\n'
        b"]  # keep this trailing note\n"
        b'command = "echo hi"  # keep this inline comment\n'
    )
    assert path.read_bytes() == expected


def test_status_save_replaces_a_single_line_array_with_the_documented_multiline(
    tmp_path: Path,
) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b'[status]\nsegments = ["cwd", "version"]\n')

    save_status_settings({"segments": ("version",)}, config_dir=tmp_path)

    assert path.read_bytes() == b'[status]\nsegments = [\n  "version",\n]\n'


def test_status_save_preserves_crlf_line_endings(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b'[status]\r\nsegments = [\r\n  "cwd",\r\n]\r\n')

    save_status_settings({"segments": ("version",)}, config_dir=tmp_path)

    assert path.read_bytes() == b'[status]\r\nsegments = [\r\n  "version",\r\n]\r\n'


def test_status_save_rewrites_dotted_top_level_assignments(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(
        b'status.command = "old"\n'
        b"status.interval_seconds = 5\n"
        b'status.segments = ["cwd"]\n'
    )

    save_status_settings(
        {"command": "new", "interval_seconds": 30, "segments": ("version",)},
        config_dir=tmp_path,
    )

    assert path.read_bytes() == (
        b'status.command = "new"\n'
        b"status.interval_seconds = 30\n"
        b"status.segments = [\n"
        b'  "version",\n'
        b"]\n"
    )


def test_status_save_refuses_an_interior_comment_inside_the_replaced_array(
    tmp_path: Path,
) -> None:
    path = tmp_path / "config.toml"
    original = b'[status]\nsegments = [\n  "cwd",  # keep\n]\n'
    path.write_bytes(original)

    with pytest.raises(ConfigError, match="comment inside"):
        save_status_settings({"segments": ("version",)}, config_dir=tmp_path)

    assert path.read_bytes() == original


def test_status_save_refuses_an_inline_status_table(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    original = b'status = { command = "old" }\n'
    path.write_bytes(original)

    with pytest.raises(ConfigError, match="inline table"):
        save_status_settings({"command": "new"}, config_dir=tmp_path)

    assert path.read_bytes() == original


def test_status_save_refuses_the_dotted_and_append_mix(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    original = b'status.command = "old"\n'
    path.write_bytes(original)

    with pytest.raises(ConfigError, match="mix dotted assignments"):
        save_status_settings({"segments": ("cwd",)}, config_dir=tmp_path)

    assert path.read_bytes() == original


def test_status_save_refuses_an_append_beside_another_dotted_status_key(
    tmp_path: Path,
) -> None:
    path = tmp_path / "config.toml"
    original = b"status.cwd_max_columns = 30\n"
    path.write_bytes(original)

    with pytest.raises(ConfigError, match="mix dotted assignments"):
        save_status_settings({"command": "new"}, config_dir=tmp_path)

    assert path.read_bytes() == original


def test_status_save_refuses_an_invalid_existing_toml_file(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    original = b"[status\n"
    path.write_bytes(original)

    with pytest.raises(ConfigError, match="not valid TOML"):
        save_status_settings({"command": "new"}, config_dir=tmp_path)

    assert path.read_bytes() == original


def test_status_save_verifies_the_semantic_diff_and_refuses_a_neighbor_change(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "config.toml"
    original = b'[status]\ncommand = "old"\n'
    path.write_bytes(original)

    def corrupt_rewrite(content: bytes, rendered: object) -> bytes:
        del content, rendered
        return b'[status]\ncommand = "new"\n[theme]\nname = "stolen"\n'

    monkeypatch.setattr(config_module, "_rewrite_status_settings", corrupt_rewrite)

    with pytest.raises(ConfigError, match="changed more than the requested keys"):
        save_status_settings({"command": "new"}, config_dir=tmp_path)

    assert path.read_bytes() == original


def test_status_save_is_all_or_nothing_across_the_changed_keys(
    tmp_path: Path,
) -> None:
    path = tmp_path / "config.toml"
    original = b'[status]\ncommand = "old"\n'
    path.write_bytes(original)

    with pytest.raises(ConfigError, match="between 1 and 3600"):
        save_status_settings(
            {"command": "new", "interval_seconds": 9999}, config_dir=tmp_path
        )

    assert path.read_bytes() == original


@pytest.mark.parametrize(
    "changes",
    [
        {"name": "stolen-theme"},
        {"theme": {"name": "stolen-theme"}},
        {"allowlist": ["SECRET_TOKEN"]},
        {"endpoints": {"work": "ws://127.0.0.1:9119/api/ws"}},
        {"cwd_max_columns": 30},
        {"credentials": "should-never-exist"},
    ],
    ids=("theme-name", "theme-table", "allowlist", "endpoints", "columns", "credentials"),
)
def test_status_save_never_writes_a_key_outside_the_view_allowlist(
    tmp_path: Path, changes: dict[str, object]
) -> None:
    """The credential-like-keys guard: the refusal is by allowlist, so every
    non-status key falls through the same one clause."""
    path = tmp_path / "config.toml"
    path.write_bytes(b'[status]\ncommand = "old"\n')

    with pytest.raises(ConfigError, match="the configuration view writes only"):
        save_status_settings(changes, config_dir=tmp_path)  # type: ignore[arg-type]

    assert path.read_bytes() == b'[status]\ncommand = "old"\n'


@pytest.mark.parametrize(
    ("changes", "message"),
    [
        ({"command": 42}, "must be a string"),
        ({"interval_seconds": True}, "must be an integer"),
        ({"interval_seconds": 0}, "between 1 and 3600"),
        ({"interval_seconds": 3601}, "between 1 and 3600"),
        ({"interval_seconds": 5.5}, "must be an integer"),
        ({"segments": "cwd"}, "must be a list of segment names"),
        ({"segments": ("bogus",)}, "unknown segment"),
        ({"segments": ("cwd", "cwd")}, "twice"),
        ({}, "at least one changed key"),
    ],
    ids=(
        "command-not-a-string",
        "interval-bool",
        "interval-too-low",
        "interval-too-high",
        "interval-not-an-integer",
        "segments-not-a-list",
        "segments-unknown-name",
        "segments-duplicate",
        "nothing-changed",
    ),
)
def test_status_save_refuses_values_the_contract_would_not_accept(
    tmp_path: Path, changes: dict[str, object], message: str
) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b'[status]\ncommand = "old"\n')

    with pytest.raises(ConfigError, match=message):
        save_status_settings(changes, config_dir=tmp_path)  # type: ignore[arg-type]

    assert path.read_bytes() == b'[status]\ncommand = "old"\n'


def test_status_save_writes_an_empty_command_as_the_explicit_empty_value(
    tmp_path: Path,
) -> None:
    """D8 recorded: an empty command persists as ``command = ""`` — the
    documented contract disables the region for an empty value, and one
    mechanism for the outcome is enough (no byte-preserving key removal)."""
    path = tmp_path / "config.toml"
    path.write_bytes(b'[status]\ncommand = "git status"\n')

    save_status_settings({"command": ""}, config_dir=tmp_path)

    assert path.read_bytes() == b'[status]\ncommand = ""\n'
    assert tomllib.loads(path.read_text(encoding="utf-8"))["status"]["command"] == ""


def test_status_save_escapes_arbitrary_command_text(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b'[status]\ncommand = "old"\n')

    save_status_settings({"command": 'echo "hi" && echo C:\\path'}, config_dir=tmp_path)

    assert b'command = "echo \\"hi\\" && echo C:\\\\path"\n' in path.read_bytes()
    parsed = tomllib.loads(path.read_text(encoding="utf-8"))
    assert parsed["status"]["command"] == 'echo "hi" && echo C:\\path'


def test_status_save_writes_an_empty_segments_array_inline(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b'[status]\nsegments = ["cwd"]\n')

    save_status_settings({"segments": ()}, config_dir=tmp_path)

    assert path.read_bytes() == b"[status]\nsegments = []\n"


def test_status_save_targets_the_repository_scope_explicitly(tmp_path: Path) -> None:
    path = save_status_settings({"command": "repo-status"}, "repository", cwd=tmp_path)

    assert path == tmp_path / ".talaria" / "config.toml"
    assert tomllib.loads(path.read_text(encoding="utf-8")) == {
        "status": {"command": "repo-status"}
    }
