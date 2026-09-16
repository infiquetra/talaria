"""D10/D11 — the generalized byte-preserving Talaria writer, and A1-6's local half.

Issue #149's writer covers ``[status]`` only; D10 extends the same surgical
rewrite to every ``DEFAULTS`` table, and D11 adds the ``[connections.<id>]``
inventory. The guarantees do not loosen with the generalization: comments,
layout, CRLF, and dotted shapes survive; unsupported shapes refuse rather
than reformat; and a write that would touch anything beyond the requested
keys refuses.

Interface under test (unimplemented until CFG B2, ``dev-3``):

* ``talaria.config.save_settings(table, changes, scope="user", *,
  config_dir=None, cwd=None)`` — table-scoped surgical write. ``table`` is a
  ``DEFAULTS`` table name, or ``connections.<id>`` for one inventory entry.
  Validation is strict (``ConfigError``), mirroring load-time normalization:
  loads fall back with a notice, writes refuse.

A1-6's "preserves unrelated values" holds on both sides of the ownership
line: the Hermes half is the sparse PUT (``tests/domain/
test_settings_target.py``); this file's half is the semantic-diff guard and
byte-exact preservation for Talaria's own file.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import pytest

from talaria import config as config_module
from talaria.config import ConfigError


def _save_settings() -> Any:
    save = getattr(config_module, "save_settings", None)
    if save is None:
        pytest.fail("unimplemented interface talaria.config.save_settings (CFG B2)")
    return save


# ── the generalization: every DEFAULTS table writes ──────────────────────


def test_a_ui_width_writes_surgically_beside_its_siblings(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b"[ui]\nreduced_motion = false\ninspector_width = 36\n")

    _save_settings()("ui", {"inspector_width": 40}, config_dir=tmp_path)

    assert path.read_bytes() == b"[ui]\nreduced_motion = false\ninspector_width = 40\n"


def test_a_missing_table_appends_without_touching_existing_tables(
    tmp_path: Path,
) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b"[status]\ninterval_seconds = 5\n")

    _save_settings()("ui", {"show_timestamps": True}, config_dir=tmp_path)

    assert path.read_bytes() == (
        b"[status]\ninterval_seconds = 5\n\n[ui]\nshow_timestamps = true\n"
    )


def test_a_new_notifications_table_writes_its_bool(tmp_path: Path) -> None:
    path = _save_settings()(
        "notifications", {"transcript_line": False}, config_dir=tmp_path
    )

    assert path == tmp_path / "config.toml"
    assert path.read_bytes() == b"[notifications]\ntranscript_line = false\n"


def test_a_keybinding_writes_and_normalizes_case(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b"[keys]\ninterrupt = \"ctrl+s\"\n")

    _save_settings()("keys", {"toggle_inspector": "Ctrl+X"}, config_dir=tmp_path)

    assert b'toggle_inspector = "ctrl+x"' in path.read_bytes()
    assert b'interrupt = "ctrl+s"' in path.read_bytes()


def test_the_composer_cap_writes_as_an_integer(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b"[composer]\npaste_collapse_lines = 6\n")

    _save_settings()("composer", {"attachment_max_mb": 32}, config_dir=tmp_path)

    assert b"attachment_max_mb = 32" in path.read_bytes()
    assert b"paste_collapse_lines = 6" in path.read_bytes()


def test_crlf_and_comments_survive_a_new_table_write(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(
        b"# operator comment\r\n[ui]\r\ninspector_width = 36  # keep\r\n"
    )

    _save_settings()("ui", {"inspector_width": 40}, config_dir=tmp_path)

    assert path.read_bytes() == (
        b"# operator comment\r\n[ui]\r\ninspector_width = 40  # keep\r\n"
    )


def test_dotted_assignments_rewrite_in_place(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b"ui.inspector_width = 36\n")

    _save_settings()("ui", {"inspector_width": 40}, config_dir=tmp_path)

    assert path.read_bytes() == b"ui.inspector_width = 40\n"


def test_a_repository_scoped_write_targets_the_repository_file(
    tmp_path: Path,
) -> None:
    path = _save_settings()(
        "ui", {"inspector_width": 40}, "repository", cwd=tmp_path
    )

    assert path == tmp_path / ".talaria" / "config.toml"
    assert b"inspector_width = 40" in path.read_bytes()


# ── D11: the connections inventory writes ────────────────────────────────


def test_a_connection_entry_writes_url_auth_and_label(tmp_path: Path) -> None:
    path = _save_settings()(
        "connections.office",
        {
            "url": "http://10.220.1.139:8765",
            "auth": "gated",
            "label": "office dashboard",
        },
        config_dir=tmp_path,
    )

    assert path == tmp_path / "config.toml"
    text = path.read_text(encoding="utf-8")
    assert "[connections.office]" in text
    assert 'url = "http://10.220.1.139:8765"' in text
    assert 'auth = "gated"' in text
    assert 'label = "office dashboard"' in text


def test_a_connection_write_preserves_sibling_entries(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(
        b"[connections.home]\n"
        b'url = "http://127.0.0.1:8765"\n'
        b'auth = "loopback"\n'
    )

    _save_settings()(
        "connections.office",
        {"url": "http://10.220.1.139:8765", "auth": "gated"},
        config_dir=tmp_path,
    )

    text = path.read_text(encoding="utf-8")
    assert "[connections.home]" in text
    assert 'url = "http://127.0.0.1:8765"' in text
    assert "[connections.office]" in text


def test_a_connection_url_carrying_a_credential_is_refused(tmp_path: Path) -> None:
    """``credentials.py`` refuses credentialed URLs at dial time; the writer
    refuses them at rest, so the secret never lands in the file at all."""
    path = tmp_path / "config.toml"
    path.write_bytes(b"")

    with pytest.raises(ConfigError, match="[Cc]redential"):
        _save_settings()(
            "connections.office",
            {"url": "http://user:pass@10.220.1.139:8765", "auth": "gated"},
            config_dir=tmp_path,
        )

    assert path.read_bytes() == b""


def test_an_unknown_connection_key_is_refused(tmp_path: Path) -> None:
    """Only ``url``/``auth``/``label`` live in the file; credential material
    belongs in the 0600 credentials file, never beside the URL."""
    path = tmp_path / "config.toml"
    path.write_bytes(b"")

    with pytest.raises(ConfigError, match="connections.office.token"):
        _save_settings()(
            "connections.office",
            {"url": "http://10.220.1.139:8765", "token": "secret-value"},
            config_dir=tmp_path,
        )

    assert path.read_bytes() == b""


def test_an_unknown_auth_mode_is_refused(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b"")

    with pytest.raises(ConfigError, match="auth"):
        _save_settings()(
            "connections.office",
            {"url": "http://10.220.1.139:8765", "auth": "kerberos"},
            config_dir=tmp_path,
        )

    assert path.read_bytes() == b""


def test_the_profiles_endpoint_map_stays_hand_edited(tmp_path: Path) -> None:
    """D11 keeps ``profiles.endpoints`` as a compatibility alias, not a UI
    write target: a whole-map value defeats the surgical rewrite (one entry
    changed, every entry re-rendered), so operators edit it by hand and the
    workspace manages ``[connections.*]`` instead."""
    path = tmp_path / "config.toml"
    original = b'[profiles.endpoints]\ndev = "ws://127.0.0.1:9119/api/ws"\n'
    path.write_bytes(original)

    with pytest.raises(ConfigError, match="by hand"):
        _save_settings()(
            "profiles",
            {"endpoints": {"dev": "ws://127.0.0.1:9119/api/ws"}},
            config_dir=tmp_path,
        )

    assert path.read_bytes() == original


# ── strict validation mirrors load-time normalization ────────────────────


@pytest.mark.parametrize(
    ("table", "changes", "message"),
    [
        ("ui", {"inspector_width": 27}, "between 28 and 48"),
        ("ui", {"inspector_width": 49}, "between 28 and 48"),
        ("ui", {"inspector_width": True}, "must be an integer"),
        ("ui", {"show_timestamps": "yes"}, "must be a boolean"),
        ("keys", {"toggle_inspector": "ctrl+banana"}, "not a key"),
        ("keys", {"toggle_inspector": "ctrl+q"}, "reserved for quitting"),
        ("composer", {"attachment_max_mb": 0}, "positive"),
        ("composer", {"attachment_max_mb": "lots"}, "must be an integer"),
        ("notifications", {"transcript_line": 1}, "must be a boolean"),
    ],
)
def test_invalid_values_are_refused_with_the_key_named(
    tmp_path: Path, table: str, changes: dict[str, Any], message: str
) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b"")

    with pytest.raises(ConfigError, match=message):
        _save_settings()(table, changes, config_dir=tmp_path)

    assert path.read_bytes() == b""


def test_a_duplicate_chord_across_keys_is_refused_with_both_named(
    tmp_path: Path,
) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b"[keys]\ninterrupt = \"ctrl+s\"\n")

    with pytest.raises(ConfigError, match="both"):
        _save_settings()("keys", {"toggle_inspector": "ctrl+s"}, config_dir=tmp_path)

    assert b"toggle_inspector" not in path.read_bytes()


def test_an_unknown_table_is_refused(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b"")

    with pytest.raises(ConfigError, match="unknown-table"):
        _save_settings()("unknown-table", {"key": "value"}, config_dir=tmp_path)

    assert path.read_bytes() == b""


def test_an_unknown_key_is_refused_with_table_and_key_named(
    tmp_path: Path,
) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b"[ui]\ninspector_width = 36\n")

    with pytest.raises(ConfigError, match="ui.font_size"):
        _save_settings()("ui", {"font_size": 14}, config_dir=tmp_path)

    assert path.read_bytes() == b"[ui]\ninspector_width = 36\n"


def test_an_empty_change_set_is_refused(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    path.write_bytes(b"")

    with pytest.raises(ConfigError, match="at least one changed key"):
        _save_settings()("ui", {}, config_dir=tmp_path)

    assert path.read_bytes() == b""


# ── the byte-safety rules generalize with the writer ─────────────────────


def test_an_inline_table_refuses_rather_than_reformats(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    original = b"ui = { inspector_width = 36 }\n"
    path.write_bytes(original)

    with pytest.raises(ConfigError, match="inline table"):
        _save_settings()("ui", {"inspector_width": 40}, config_dir=tmp_path)

    assert path.read_bytes() == original


def test_an_invalid_existing_file_is_never_replaced(tmp_path: Path) -> None:
    path = tmp_path / "config.toml"
    original = b"[ui\n"
    path.write_bytes(original)

    with pytest.raises(ConfigError, match="not valid TOML"):
        _save_settings()("ui", {"inspector_width": 40}, config_dir=tmp_path)

    assert path.read_bytes() == original


def test_the_write_is_all_or_nothing_across_tables_and_keys(
    tmp_path: Path,
) -> None:
    path = tmp_path / "config.toml"
    original = b"[ui]\ninspector_width = 36\n"
    path.write_bytes(original)

    with pytest.raises(ConfigError, match="between 28 and 48"):
        _save_settings()(
            "ui",
            {"inspector_width": 9999, "show_timestamps": True},
            config_dir=tmp_path,
        )

    assert path.read_bytes() == original


def test_the_semantic_diff_guard_watches_neighboring_tables(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A1-6's local half: the rewrite is verified to have changed only the
    requested keys, so an unrelated value cannot be silently rewritten."""
    path = tmp_path / "config.toml"
    original = b'[ui]\ninspector_width = 36\n[theme]\nname = "refined-default"\n'
    path.write_bytes(original)

    def corrupt_rewrite(content: bytes, rendered: object) -> bytes:
        del content, rendered
        return (
            b'[ui]\ninspector_width = 40\n[theme]\nname = "stolen"\n'
        )

    # raising=False: the seam under test does not exist until B2, and the
    # failure must read as missing behavior (below) rather than a broken
    # monkeypatch.
    monkeypatch.setattr(
        config_module, "_rewrite_settings_table", corrupt_rewrite, raising=False
    )

    with pytest.raises(ConfigError, match="changed more than the requested keys"):
        _save_settings()("ui", {"inspector_width": 40}, config_dir=tmp_path)

    assert path.read_bytes() == original
