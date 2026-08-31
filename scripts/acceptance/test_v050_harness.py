"""Focused tests for the issue #110 acceptance harness itself."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

from scripts.acceptance.v050_common import (
    FALLBACK_MODEL_ROUTE,
    PRIMARY_MODEL_ROUTE,
    HarnessError,
    isolated_environment,
    probe_installed_artifact,
    validate_config_dir,
    validate_wheel_direct_url,
)
from scripts.acceptance.v050_install_probe import _probe_bare_launch, _probe_gate
from scripts.acceptance.v050_pty_driver import DriveEvent, parse_events, run_pty
from scripts.acceptance.v050_receipt import validate_receipt

_REPO_ROOT = Path(__file__).resolve().parents[2]


def _complete_gate_report(
    *,
    measured_ms: float = 54.45,
    known_check_passed: bool = False,
    other_check_passed: bool = True,
) -> dict[str, Any]:
    checks: dict[str, dict[str, Any]] = {
        "content_loss": {
            "measured": 0 if other_check_passed else 1,
            "threshold": 0,
            "comparison": "<=",
            "description": "content-loss fixture",
            "pass": other_check_passed,
        },
        "workload_latency_growing-one-column-table": {
            "measured": measured_ms,
            "threshold": 50.0,
            "comparison": "<=",
            "description": "streaming p99 fixture",
            "pass": known_check_passed,
        },
    }
    return {
        "verdict": "pass" if all(check["pass"] for check in checks.values()) else "fail",
        "matrix": {},
        "checks": checks,
        "stress": {},
        "cadence": {},
        "live": None,
        "feature": {},
        "determinism_identical": None,
        "sideband_determinism_identical": True,
        "sideband_structure_identical": True,
        "inert_controls_refused": [],
        "workloads": {},
    }


def _write_gate_program(path: Path, report: dict[str, Any], *, exit_code: int) -> None:
    encoded_report = json.dumps(report)
    path.write_text(
        f"""#!{sys.executable}
import json
import sys
from pathlib import Path

if sys.argv[1:4] != ["gate", "--deltas", "50000"]:
    raise SystemExit(9)
output = Path(sys.argv[sys.argv.index("--json") + 1])
report = json.loads({encoded_report!r})
output.write_text(json.dumps(report), encoding="utf-8")
print(json.dumps(report))
raise SystemExit({exit_code})
""",
        encoding="utf-8",
    )
    path.chmod(0o755)


@pytest.mark.skipif(sys.platform == "win32", reason="the acceptance terminal is POSIX-only")
def test_real_pty_preserves_ansi_keys_and_resize(tmp_path: Path) -> None:
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    environment = isolated_environment(
        config_dir=config_dir,
        term="xterm-256color",
        rows=30,
        columns=100,
    )
    result = run_pty(
        executable=Path(sys.executable),
        argv=[sys.executable, "-m", "scripts.acceptance.echo_terminal"],
        cwd=_REPO_ROOT,
        environment=environment,
        capture_path=tmp_path / "raw" / "echo.ansi",
        events=[
            DriveEvent(0.25, "text", "abc"),
            DriveEvent(0.50, "resize", (24, 80)),
            DriveEvent(0.75, "key", "CTRL_B"),
            DriveEvent(1.00, "text", "q"),
        ],
        expected_literals=["ECHO-TERMINAL", "READY:30x100", "RESIZE:24x80", "EXIT:q"],
        rows=30,
        columns=100,
        term="xterm-256color",
        terminal_program="harness echo terminal",
        timeout=5.0,
    )
    capture = result.capture_path.read_bytes()
    assert result.exit_code == 0
    assert not result.timed_out
    assert result.missing_literals == ()
    assert b"\x1b[31mECHO-TERMINAL\x1b[0m" in capture
    assert b"KEYS:616263" in capture
    assert b"KEYS:02" in capture
    assert result.rows == 24
    assert result.columns == 80


@pytest.mark.skipif(sys.platform == "win32", reason="the acceptance terminal is POSIX-only")
def test_timeout_kills_the_real_pty_child_loudly(tmp_path: Path) -> None:
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    environment = isolated_environment(
        config_dir=config_dir,
        term="xterm-256color",
        rows=20,
        columns=60,
    )
    result = run_pty(
        executable=Path(sys.executable),
        argv=[sys.executable, "-m", "scripts.acceptance.echo_terminal"],
        cwd=_REPO_ROOT,
        environment=environment,
        capture_path=tmp_path / "raw" / "timeout.ansi",
        events=[],
        expected_literals=["READY:20x60"],
        rows=20,
        columns=60,
        term="xterm-256color",
        terminal_program="harness echo terminal",
        timeout=0.5,
    )
    assert result.timed_out
    assert result.exit_code != 0
    assert result.capture_bytes > 0


@pytest.mark.skipif(sys.platform == "win32", reason="the acceptance terminal is POSIX-only")
def test_bare_launch_ends_blocking_worker_prompt_with_eof(tmp_path: Path) -> None:
    prompt_program = tmp_path / "blocking-worker-prompt"
    prompt_program.write_text(
        f"""#!{sys.executable}
import asyncio
import getpass


async def prompt() -> int:
    try:
        await asyncio.to_thread(getpass.getpass, "WORKER-PROMPT: ")
    except EOFError:
        print("PROMPT-EOF", flush=True)
        return 2
    return 0


raise SystemExit(asyncio.run(prompt()))
""",
        encoding="utf-8",
    )
    prompt_program.chmod(0o755)
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    environment = isolated_environment(
        config_dir=config_dir,
        term="xterm-256color",
        rows=20,
        columns=60,
    )

    result = _probe_bare_launch(
        prompt_program,
        work_dir=tmp_path,
        environment=environment,
        raw_dir=tmp_path / "raw",
        rows=20,
        columns=60,
        term="xterm-256color",
    )

    capture = Path(result["capture"]["path"]).read_bytes()
    assert result["exit_code"] == 2
    assert not result["timed_out"]
    assert b"WORKER-PROMPT: " in capture
    assert b"PROMPT-EOF" in capture


@pytest.mark.skipif(sys.platform == "win32", reason="the acceptance subprocess is POSIX-only")
def test_gate_probe_records_known_v040_exceedance_at_designed_scale(tmp_path: Path) -> None:
    gate_program = tmp_path / "gate-program"
    _write_gate_program(gate_program, _complete_gate_report(), exit_code=1)
    receipt_dir = tmp_path / "receipts"
    receipt_dir.mkdir()

    result = _probe_gate(
        gate_program,
        work_dir=tmp_path,
        environment={},
        receipt_dir=receipt_dir,
    )

    comparison = result["known_preexisting_exceedance"]
    assert result["argv"][2:4] == ["--deltas", "50000"]
    assert result["exit_code"] == 1
    assert result["report_verdict"] == "fail"
    assert result["accepted_failed_checks"] == [
        "workload_latency_growing-one-column-table"
    ]
    assert comparison == {
        "check": "workload_latency_growing-one-column-table",
        "measured_ms": 54.45,
        "threshold_ms": 50.0,
        "v0.4_baseline_ms": 61.988,
        "delta_from_v0.4_ms": -7.538,
        "direction_from_v0.4": "improved",
    }


@pytest.mark.skipif(sys.platform == "win32", reason="the acceptance subprocess is POSIX-only")
def test_gate_probe_refuses_any_other_failed_check(tmp_path: Path) -> None:
    gate_program = tmp_path / "gate-program"
    report = _complete_gate_report(other_check_passed=False)
    _write_gate_program(gate_program, report, exit_code=1)
    receipt_dir = tmp_path / "receipts"
    receipt_dir.mkdir()

    with pytest.raises(HarnessError, match="failed checks beyond the known v0.4 exceedance"):
        _probe_gate(
            gate_program,
            work_dir=tmp_path,
            environment={},
            receipt_dir=receipt_dir,
        )


@pytest.mark.skipif(sys.platform == "win32", reason="the acceptance subprocess is POSIX-only")
def test_gate_probe_refuses_latency_worse_than_v040_baseline(tmp_path: Path) -> None:
    gate_program = tmp_path / "gate-program"
    report = _complete_gate_report(measured_ms=62.0)
    _write_gate_program(gate_program, report, exit_code=1)
    receipt_dir = tmp_path / "receipts"
    receipt_dir.mkdir()

    with pytest.raises(HarnessError, match="regressed from the v0.4 baseline"):
        _probe_gate(
            gate_program,
            work_dir=tmp_path,
            environment={},
            receipt_dir=receipt_dir,
        )


@pytest.mark.skipif(sys.platform == "win32", reason="the acceptance subprocess is POSIX-only")
def test_gate_probe_refuses_an_incomplete_report(tmp_path: Path) -> None:
    gate_program = tmp_path / "gate-program"
    report = _complete_gate_report()
    del report["workloads"]
    _write_gate_program(gate_program, report, exit_code=1)
    receipt_dir = tmp_path / "receipts"
    receipt_dir.mkdir()

    with pytest.raises(HarnessError, match="report is incomplete; missing: workloads"):
        _probe_gate(
            gate_program,
            work_dir=tmp_path,
            environment={},
            receipt_dir=receipt_dir,
        )


def test_event_script_rejects_ambiguous_actions(tmp_path: Path) -> None:
    path = tmp_path / "events.json"
    path.write_text(
        json.dumps({"events": [{"at_seconds": 0, "text": "x", "key": "ENTER"}]}),
        encoding="utf-8",
    )
    with pytest.raises(HarnessError, match="exactly one action"):
        parse_events(path)


def test_real_config_directory_is_refused(tmp_path: Path) -> None:
    with pytest.raises(HarnessError, match="real Talaria config"):
        validate_config_dir(Path.home() / ".talaria", scratch_root=tmp_path)


def test_current_source_checkout_cannot_pose_as_an_installed_candidate(tmp_path: Path) -> None:
    """Exercise the actual checkout/venv boundary rather than a patched import."""
    venv = Path(sys.prefix)
    executable = venv / "bin" / "talaria"
    if not executable.exists():
        pytest.skip("the project console script is not installed in the test venv")
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    environment = isolated_environment(
        config_dir=config_dir,
        term="xterm-256color",
        rows=24,
        columns=80,
        venv_bin=venv / "bin",
    )
    from talaria import __version__

    with pytest.raises(HarnessError, match="source-checkout leakage"):
        probe_installed_artifact(
            venv=venv,
            executable=executable,
            work_dir=tmp_path,
            environment=environment,
            expected_version=__version__,
            integration_tree=_REPO_ROOT,
            wheel_sha256=None,
        )


def test_global_executable_is_refused_before_it_can_launch(tmp_path: Path) -> None:
    with pytest.raises(HarnessError, match="non-venv"):
        probe_installed_artifact(
            venv=tmp_path / "venv",
            executable=Path("/usr/bin/true"),
            work_dir=tmp_path,
            environment={},
            expected_version="0.5.0",
            integration_tree=_REPO_ROOT,
            wheel_sha256=None,
        )


def test_editable_install_provenance_is_refused() -> None:
    with pytest.raises(HarnessError, match="editable"):
        validate_wheel_direct_url(
            {"url": "file:///checkout", "dir_info": {"editable": True}},
            wheel_sha256=None,
        )


def _receipt(*, route: dict[str, Any], redaction: str = "passed") -> dict[str, Any]:
    return {
        "schema_version": "talaria-v0.5.0-receipt-v1",
        "release": "0.5.0",
        "checklist_item": 2,
        "owner": "shared",
        "tester": "talaria-t1",
        "verdict": "pass",
        "artifact": {
            "commit": "a" * 40,
            "wheel_filename": "talaria-0.5.0-py3-none-any.whl",
            "wheel_sha256": "b" * 64,
            "version": "0.5.0",
            "executable": "/scratch/venv/bin/talaria",
            "executable_sha256": "c" * 64,
            "installed_files_sha256": "c" * 64,
            "distribution_root": "/scratch/venv/lib/site-packages",
            "install_receipt_path": "/scratch/install-receipt.json",
            "install_receipt_sha256": "d" * 64,
        },
        "terminal": {
            "program": "terminal fixture",
            "term": "xterm-256color",
            "rows": 36,
            "columns": 132,
        },
        "session": {"mode": "live", "profile": "throwaway-profile", "model_route": route},
        "evidence": {
            "capture_path": "/scratch/raw/item-02.ansi",
            "capture_sha256": "e" * 64,
            "screenshot_path": "/scratch/screenshots/item-02.png",
            "screenshot_sha256": "f" * 64,
            "redaction_review": redaction,
        },
    }


def test_primary_route_is_a_valid_first_class_receipt_field() -> None:
    route = {
        "requested": PRIMARY_MODEL_ROUTE,
        "observed": PRIMARY_MODEL_ROUTE,
        "status": "used",
        "fallback_reason": None,
        "fallback_availability": "unavailable",
    }
    # Fallback unavailability does not invalidate a successful primary route.
    assert validate_receipt(_receipt(route=route), verify_files=False) == []


def test_unavailable_fallback_cannot_be_recorded_as_a_pass() -> None:
    route = {
        "requested": FALLBACK_MODEL_ROUTE,
        "observed": None,
        "status": "not-reached",
        "fallback_reason": {
            "code": "model-not-found",
            "detail": "Ollama listed no local model and the contracted GLM variant did not resolve",
        },
        "fallback_availability": "unavailable",
    }
    errors = validate_receipt(_receipt(route=route), verify_files=False)
    assert "a passing live leg must observe the model route it used" in errors
    assert "a leg cannot pass when the named fallback was required but unavailable" in errors


def test_unapproved_model_substitution_is_rejected() -> None:
    route = {
        "requested": "some other model",
        "observed": "some other model",
        "status": "used",
        "fallback_reason": None,
        "fallback_availability": "not-applicable",
    }
    errors = validate_receipt(_receipt(route=route), verify_files=False)
    assert any("unapproved route" in error for error in errors)


def test_pending_redaction_review_cannot_pass() -> None:
    route = {
        "requested": PRIMARY_MODEL_ROUTE,
        "observed": PRIMARY_MODEL_ROUTE,
        "status": "used",
        "fallback_reason": None,
        "fallback_availability": "not-checked",
    }
    errors = validate_receipt(_receipt(route=route, redaction="pending"), verify_files=False)
    assert "a receipt cannot pass before capture and screenshot redaction review" in errors
