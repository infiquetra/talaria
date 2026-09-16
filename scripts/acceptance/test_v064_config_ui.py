"""CFG-P3 Test Author Two: black-box driver harness.

Isolation, reject-once, redaction, and cleanup must pass. The v0.6.3
baseline run must fail because target selection cannot mount fixture-only
rows. Direct ``open_target_switch`` / ``open_reveal`` calls are forbidden.
"""

from __future__ import annotations

import ast
import inspect
from pathlib import Path

import pytest

from scripts.acceptance.v062_configuration import HarnessError, scan_for_canaries
from scripts.acceptance.v064_config_ui import (
    CLEANUP_SCHEMA_VERSION,
    FORBIDDEN_WRITE_TARGETS,
    INSTALLED_EXECUTABLE,
    LEGAL_LOCAL_A_ACTIVE,
    LEGAL_LOCAL_A_INSTALLED,
    LEGAL_LOCAL_E_ACTIVE,
    LEGAL_LOCAL_E_INSTALLED,
    OPERATOR_CONFIG,
    PLACEHOLDER_SCHEMA_KEY,
    REPO_ROOT,
    RejectOnceDashboard,
    RouteRecord,
    TargetMountObservation,
    assert_write_target_allowed,
    classify_body,
    cleanup_isolated_run,
    isolated_child_env,
    launch_supplied_executable,
    observe_installed_target_mount,
    redact_route_log,
    refuse_operator_config,
    require_legal_p3_name,
    require_outside_worktree,
    write_isolated_config,
)

_LEGAL = (
    LEGAL_LOCAL_A_ACTIVE,
    LEGAL_LOCAL_E_ACTIVE,
    LEGAL_LOCAL_A_INSTALLED,
    LEGAL_LOCAL_E_INSTALLED,
)


def test_legal_local_names_are_accepted_and_default_testb_hard_fail() -> None:
    for name in _LEGAL:
        assert require_legal_p3_name(name) == name
        assert "." not in name
        assert len(name) <= 64
    for name in FORBIDDEN_WRITE_TARGETS:
        with pytest.raises(HarnessError, match="hard failure"):
            assert_write_target_allowed(name)
    with pytest.raises(HarnessError, match="dot"):
        require_legal_p3_name("talaria-v0.6.2-cfg-t0-local-active-a")


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
    assert Path(env["HOME"]) == (tmp_path / "home").resolve()
    assert "PYTHONPATH" not in env
    child = launch_supplied_executable(exe, env=env, cwd=tmp_path)
    stdout, _stderr = child.communicate(timeout=5)
    assert child.returncode == 0
    assert stdout.strip() == "isolated-ok"


def test_reject_once_save_leaves_state_unchanged_then_retry_succeeds() -> None:
    with RejectOnceDashboard() as dashboard:
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
        log = redact_route_log(dashboard.records)
        assert all(item["body_class"] == "json-no-secret" for item in log)
        assert any(
            item["method"] == "PUT" and item["status"] == 409 for item in log
        )


def test_evidence_redaction_finds_canaries_and_keeps_route_log_clean() -> None:
    canary = "p3-canary-do-not-leak"
    assert scan_for_canaries({"value": canary}, [canary]) == [canary]
    assert classify_body({"is_set": True, "masked": "sk-…p3"}) == "json-no-secret"
    planted = classify_body({"value": canary}, canaries=(canary,))
    assert planted == "secret"
    log = redact_route_log(
        [
            RouteRecord(
                method="PUT",
                path="/api/config",
                profile=LEGAL_LOCAL_A_INSTALLED,
                status=409,
                body_class="json-no-secret",
            )
        ]
    )
    rendered = str(log)
    assert canary not in rendered
    assert "password" not in rendered
    assert "secret-value" not in rendered


def test_cleanup_removes_isolated_config_and_never_touches_operator_home(
    tmp_path: Path,
) -> None:
    config_dir = tmp_path / "config"
    write_isolated_config(config_dir, dashboard_origin="http://127.0.0.1:1")
    isolated_child_env(config_dir=config_dir, scratch=tmp_path)
    assert config_dir.is_dir()
    receipt = cleanup_isolated_run(config_dir=config_dir, scratch=tmp_path)
    assert receipt["schema_version"] == CLEANUP_SCHEMA_VERSION
    assert receipt["isolated_config_removed"] is True
    assert receipt["operator_config_untouched"] is True
    assert receipt["testB"] == {"unchanged": True}
    assert "safe_hash" not in receipt["testB"]
    assert not config_dir.exists()
    assert LEGAL_LOCAL_A_INSTALLED in receipt["absent_names"]
    assert scan_for_canaries(receipt, ("password", "token", "sk-")) == []


def test_harness_source_does_not_call_direct_openers() -> None:
    source = Path(inspect.getfile(observe_installed_target_mount)).read_text(
        encoding="utf-8"
    )
    tree = ast.parse(source)
    calls = {
        node.func.attr
        for node in ast.walk(tree)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
    }
    assert "open_target_switch" not in calls
    assert "open_reveal" not in calls


def test_v063_baseline_target_selection_mounts_fixture_only_rows(
    tmp_path: Path,
) -> None:
    """Against installed v0.6.3 this fails: apply_loaded_view cannot create rows."""
    observation = observe_installed_target_mount(
        INSTALLED_EXECUTABLE,
        worktree=REPO_ROOT,
        scratch=tmp_path,
    )
    assert observation.launched is True
    assert observation.selected_profile == LEGAL_LOCAL_A_INSTALLED
    assert PLACEHOLDER_SCHEMA_KEY not in observation.fixture_only_keys
    assert observation.mounted_fixture_only_rows, (
        "unimplemented: v0.6.3 apply_loaded_view cannot mount fixture-only "
        f"rows {observation.fixture_only_keys}; mounted {observation.mounted_keys}"
    )
    assert isinstance(observation, TargetMountObservation)
