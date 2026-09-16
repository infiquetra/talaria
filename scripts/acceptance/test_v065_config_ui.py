"""CFG-P4 Test Author Two: black-box diagnosis driver.

Isolation, identity, reject-once, redaction, cleanup, and the fixture
remount control must pass. Live diagnosis uses the T3R object-fields
shape against the candidate tree. Direct opener calls are forbidden.
"""

from __future__ import annotations

import ast
import inspect
from pathlib import Path

import pytest

from scripts.acceptance.v062_configuration import HarnessError, scan_for_canaries
from scripts.acceptance.v064_config_ui import (
    FORBIDDEN_WRITE_TARGETS,
    INSTALLED_EXECUTABLE,
    LEGAL_LOCAL_A_ACTIVE,
    LEGAL_LOCAL_A_INSTALLED,
    LEGAL_LOCAL_E_ACTIVE,
    LEGAL_LOCAL_E_INSTALLED,
    OPERATOR_CONFIG,
    PLACEHOLDER_SCHEMA_KEY,
    REPO_ROOT,
    isolated_child_env,
    launch_supplied_executable,
    refuse_operator_config,
    require_outside_worktree,
    write_isolated_config,
)
from scripts.acceptance.v065_config_ui import (
    CLEANUP_SCHEMA_VERSION,
    ORACLE_KEYS,
    LiveShapeDashboard,
    cleanup_p4_run,
    collect_installed_identity,
    observe_fixture_remount_control,
    observe_live_diagnosis,
    require_legal_p4_name,
)

_LEGAL = (
    LEGAL_LOCAL_A_ACTIVE,
    LEGAL_LOCAL_E_ACTIVE,
    LEGAL_LOCAL_A_INSTALLED,
    LEGAL_LOCAL_E_INSTALLED,
)


def test_legal_local_names_are_accepted_and_default_testb_hard_fail() -> None:
    for name in _LEGAL:
        assert require_legal_p4_name(name) == name
        assert "." not in name
        assert len(name) <= 64
    for name in FORBIDDEN_WRITE_TARGETS:
        with pytest.raises(HarnessError, match="hard failure"):
            require_legal_p4_name(name)
    with pytest.raises(HarnessError, match="dot"):
        require_legal_p4_name("talaria-v0.6.2-cfg-t0-local-active-a")


def test_process_isolation_refuses_operator_config_and_worktree_executable(
    tmp_path: Path,
) -> None:
    with pytest.raises(HarnessError, match="real Talaria config"):
        refuse_operator_config(OPERATOR_CONFIG)
    with pytest.raises(HarnessError, match="worktree"):
        require_outside_worktree(REPO_ROOT / "talaria" / "cli.py", worktree=REPO_ROOT)

    stub_root = tmp_path / "outside"
    stub_root.mkdir()
    stub = stub_root / "talaria"
    stub.write_text("#!/bin/sh\necho isolated-ok\n", encoding="utf-8")
    stub.chmod(0o755)
    exe = require_outside_worktree(stub, worktree=REPO_ROOT)
    config_dir = tmp_path / "config"
    env = isolated_child_env(config_dir=config_dir, scratch=tmp_path)
    assert Path(env["TALARIA_CONFIG_DIR"]) == config_dir.resolve()
    assert Path(env["TALARIA_CONFIG_DIR"]) != OPERATOR_CONFIG.resolve()
    assert "PYTHONPATH" not in env
    child = launch_supplied_executable(exe, env=env, cwd=tmp_path)
    stdout, _stderr = child.communicate(timeout=5)
    assert child.returncode == 0
    assert stdout.strip() == "isolated-ok"


@pytest.mark.skipif(
    not INSTALLED_EXECUTABLE.is_file(),
    reason="installed Talaria executable is absent",
)
def test_installed_identity_uses_clean_pythonpath(tmp_path: Path) -> None:
    record = collect_installed_identity(
        INSTALLED_EXECUTABLE, worktree=REPO_ROOT, scratch=tmp_path
    )
    assert record["pythonpath_set"] is False
    assert record["version"]
    assert record["talaria_file_kind"] in {"uv-tool", "other", "worktree-contaminated"}
    assert scan_for_canaries(record, ("password", "token", "sk-")) == []


def test_reject_once_save_leaves_state_unchanged_then_retry_succeeds() -> None:
    with LiveShapeDashboard() as dashboard:
        before = dashboard.saved_config(LEGAL_LOCAL_A_INSTALLED)
        status, body = dashboard.put(
            LEGAL_LOCAL_A_INSTALLED, {"agent": {"max_turns": 99}}
        )
        assert status == 409
        assert body["detail"] == "rejected-once"
        assert dashboard.saved_config(LEGAL_LOCAL_A_INSTALLED) == before
        status, body = dashboard.put(
            LEGAL_LOCAL_A_INSTALLED, {"agent": {"max_turns": 99}}
        )
        assert status == 200
        assert body["ok"] is True
        assert dashboard.saved_config(LEGAL_LOCAL_A_INSTALLED)["agent"]["max_turns"] == 99
        forbidden_status, _ = dashboard.put("default", {"agent": {"max_turns": 1}})
        assert forbidden_status == 400
        assert "default" in dashboard.forbidden_writes
        assert all(item["body_class"] == "json-no-secret" for item in dashboard.records)


def test_oracle_and_cleanup_are_canary_free(tmp_path: Path) -> None:
    config_dir = tmp_path / "config"
    write_isolated_config(config_dir, dashboard_origin="http://127.0.0.1:1")
    isolated_child_env(config_dir=config_dir, scratch=tmp_path)
    receipt = cleanup_p4_run(config_dir=config_dir, scratch=tmp_path)
    assert receipt["schema_version"] == CLEANUP_SCHEMA_VERSION
    assert receipt["isolated_config_removed"] is True
    assert receipt["operator_config_untouched"] is True
    assert not config_dir.exists()
    assert scan_for_canaries(receipt, ("password", "token", "sk-")) == []
    assert set(ORACLE_KEYS) == {
        "target_connection",
        "target_profile",
        "schema_route_status_or_reason",
        "schema_response_bytes",
        "schema_top_level_keys",
        "schema_field_count",
        "schema_category_count",
        "schema_decode_result",
        "config_load_result",
        "env_route_status_or_reason",
        "env_row_count",
        "generation",
        "selected_at_reconcile",
        "projected_group_titles",
    }


def test_harness_source_does_not_call_direct_openers() -> None:
    source = Path(inspect.getfile(observe_live_diagnosis)).read_text(encoding="utf-8")
    tree = ast.parse(source)
    calls = {
        node.func.attr
        for node in ast.walk(tree)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
    }
    assert "open_target_switch" not in calls
    assert "open_reveal" not in calls


def test_fixture_remount_control_stays_green(tmp_path: Path) -> None:
    observation = observe_fixture_remount_control(scratch=tmp_path)
    assert observation.launched is True
    assert observation.selected_profile == LEGAL_LOCAL_A_INSTALLED
    assert PLACEHOLDER_SCHEMA_KEY not in observation.fixture_only_keys
    assert observation.mounted_fixture_only_rows, (
        f"fixture remount control lost rows {observation.fixture_only_keys}; "
        f"mounted {observation.mounted_keys}"
    )


def test_live_diagnosis_mounts_selected_schema_and_env(tmp_path: Path) -> None:
    """T3R: object fields on the candidate tree; selected schema/env mount."""
    observation = observe_live_diagnosis(scratch=tmp_path, worktree=REPO_ROOT)
    assert observation.launched is True
    record = observation.oracle.as_record()
    assert set(record) == set(ORACLE_KEYS)
    assert record["target_profile"] == LEGAL_LOCAL_A_INSTALLED
    assert record["schema_field_count"] >= 1
    assert record["schema_decode_result"] == "ok"
    assert "fields" in record["schema_top_level_keys"]
    assert "category_order" in record["schema_top_level_keys"]
    assert scan_for_canaries(record, ("password", "token", "sk-", "secret-value")) == []
    assert observation.selected_schema_mounted, (
        "candidate live loader left selected schema absent; "
        f"placeholder={observation.placeholder_schema_present} "
        f"decode={observation.oracle.schema_decode_result} "
        f"titles={observation.oracle.projected_group_titles}"
    )
    assert observation.selected_env_mounted, (
        "candidate live loader left selected env absent; "
        f"env={observation.oracle.env_route_status_or_reason} "
        f"rows={observation.oracle.env_row_count}"
    )
    assert observation.placeholder_schema_present is False
