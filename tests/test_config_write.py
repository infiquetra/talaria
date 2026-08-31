"""Issue #104's explicit, surgical ``[theme]`` persistence contract."""

from __future__ import annotations

import os
import tomllib
from pathlib import Path

import pytest

from talaria import config as config_module
from talaria.config import ConfigError, save_theme


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
