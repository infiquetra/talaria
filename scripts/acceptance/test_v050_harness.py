"""Focused tests for the issue #110 acceptance harness itself."""

from __future__ import annotations

import copy
import json
import re
import struct
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest
from jsonschema import Draft202012Validator

from scripts.acceptance.v050_common import (
    FALLBACK_MODEL_ROUTE,
    PRIMARY_MODEL_ROUTE,
    HarnessError,
    isolated_environment,
    probe_installed_artifact,
    sha256_file,
    validate_config_dir,
    validate_wheel_direct_url,
)
from scripts.acceptance.v050_install_probe import (
    _probe_bare_launch,
    _probe_gate,
    write_public_install_receipt,
)
from scripts.acceptance.v050_pty_driver import DriveEvent, parse_events, run_pty
from scripts.acceptance.v050_receipt import (
    _portable_json,
    _verify_evidence_file,
    publish_receipt,
    validate_receipt,
    validate_scratch_evidence_paths,
    verify_run,
)

_REPO_ROOT = Path(__file__).resolve().parents[2]

_TEXTUAL_RESIZE_PROGRAM = r"""
import os

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.events import Resize
from textual.widgets import Static

from talaria.ui.status_bar import BottomStatusBar, BottomStatusBarView


VIEW = BottomStatusBarView(
    cwd="/workspace/acceptance",
    git_branch="orch/talaria-v0-5-0",
    agent_provider="OpenCode",
    agent_model="Muse Spark 1.2 Contributor",
    input_tokens=32000,
    output_tokens=0,
    context_window=128000,
    tasks_completed=3,
    tasks_total=7,
    attention_count=1,
    connection="connected",
    version="0.5.0",
)


class ResizeProbe(App[None]):
    CSS = '''
    Screen { layout: vertical; }
    #body { height: 1fr; }
    '''
    BINDINGS = [Binding("ctrl+q", "quit", "Quit")]

    def compose(self) -> ComposeResult:
        yield Static("real Textual resize probe", id="body")
        yield BottomStatusBar(VIEW, id="bottom-status")

    def get_theme_variable_defaults(self) -> dict[str, str]:
        return {
            "talaria-status-background": "#010203",
            "talaria-status-text": "#111213",
            "talaria-status-muted": "#212223",
            "talaria-status-separator": "#313233",
            "talaria-status-success": "#41A243",
            "talaria-status-warning": "#B18223",
            "talaria-status-error": "#C14243",
            "talaria-status-attention": "#5182D3",
        }

    def on_resize(self, event: Resize) -> None:
        if event.size.width == 19:
            self.set_timer(0.20, self.report_narrow_form)

    def report_narrow_form(self) -> None:
        bar = self.query_one("#bottom-status", BottomStatusBar)
        rendered = bar.render()
        os.write(1, f"\r\nTEXTUAL-NARROW:{rendered.plain}:{bar.size.width}\r\n".encode())


ResizeProbe().run()
"""

_TEXTUAL_ENVIRONMENT_PROGRAM = r"""
import os

from textual import constants
from textual.app import App


class EnvironmentProbe(App[None]):
    def on_mount(self) -> None:
        message = (
            "TEXTUAL-ENV:"
            f"level={self.animation_level} "
            f"theme={self.theme} "
            f"smooth={constants.SMOOTH_SCROLL} "
            f"fps={constants.MAX_FPS} "
            f"terminal={os.environ['TERM_PROGRAM']}"
        )
        os.write(1, f"\r\n{message}\r\n".encode())
        self.exit()


EnvironmentProbe().run()
"""


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
def test_real_pty_preserves_ansi_keys_and_kernel_resize(tmp_path: Path) -> None:
    """The low-level probe proves TIOCGWINSZ changes, not Textual reflow."""
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
def test_real_textual_child_observes_pty_resize_and_renders_narrow_form(tmp_path: Path) -> None:
    """The environment must not hide a real pseudo-terminal resize from Textual."""
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    environment = isolated_environment(
        config_dir=config_dir,
        term="xterm-256color",
        rows=36,
        columns=144,
    )
    result = run_pty(
        executable=Path(sys.executable),
        argv=[sys.executable, "-c", _TEXTUAL_RESIZE_PROGRAM],
        cwd=_REPO_ROOT,
        environment=environment,
        capture_path=tmp_path / "raw" / "textual-resize.ansi",
        events=[
            DriveEvent(1.50, "resize", (36, 19)),
            DriveEvent(3.00, "key", "CTRL_Q"),
        ],
        expected_literals=["TEXTUAL-NARROW:[ok]:19"],
        rows=36,
        columns=144,
        term="xterm-256color",
        terminal_program="real Textual child through the acceptance pseudo-terminal",
        timeout=5.0,
    )

    assert result.exit_code == 0
    assert not result.timed_out
    assert result.missing_literals == ()
    assert "COLUMNS" not in environment
    assert "LINES" not in environment
    assert result.rows == 36
    assert result.columns == 19


@pytest.mark.skipif(sys.platform == "win32", reason="the acceptance terminal is POSIX-only")
def test_real_textual_child_uses_clean_defaults_and_declared_terminal(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A real Textual import must not observe framework controls from the parent."""
    for name, value in {
        "TEXTUAL_ANIMATIONS": "none",
        "TEXTUAL_THEME": "nord",
        "TEXTUAL_SMOOTH_SCROLL": "0",
        "TEXTUAL_FPS": "3",
        "TEXTUAL_DRIVER": "parent-driver",
        "TERM_PROGRAM": "parent-terminal",
    }.items():
        monkeypatch.setenv(name, value)
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    terminal_program = "declared acceptance terminal"
    environment = isolated_environment(
        config_dir=config_dir,
        term="xterm-256color",
        rows=24,
        columns=80,
        terminal_program=terminal_program,
    )

    result = run_pty(
        executable=Path(sys.executable),
        argv=[sys.executable, "-c", _TEXTUAL_ENVIRONMENT_PROGRAM],
        cwd=_REPO_ROOT,
        environment=environment,
        capture_path=tmp_path / "raw" / "textual-environment.ansi",
        events=[],
        expected_literals=[
            "TEXTUAL-ENV:level=full theme=textual-dark smooth=True fps=60 "
            f"terminal={terminal_program}"
        ],
        rows=24,
        columns=80,
        term="xterm-256color",
        terminal_program=terminal_program,
        timeout=5.0,
    )

    assert result.exit_code == 0
    assert not result.timed_out
    assert result.missing_literals == ()
    assert environment["TERM_PROGRAM"] == terminal_program
    assert not any(name.startswith("TEXTUAL_") for name in environment)


def test_isolated_environment_makes_colour_mode_explicit(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    monkeypatch.setenv("NO_COLOR", "1")
    monkeypatch.setenv("FORCE_COLOR", "0")
    monkeypatch.setenv("COLUMNS", "999")
    monkeypatch.setenv("LINES", "999")

    colour = isolated_environment(
        config_dir=config_dir,
        term="xterm-256color",
        rows=30,
        columns=100,
    )
    monochrome = isolated_environment(
        config_dir=config_dir,
        term="xterm-256color",
        rows=30,
        columns=100,
        monochrome=True,
    )

    assert "NO_COLOR" not in colour
    assert "FORCE_COLOR" not in colour
    assert "COLUMNS" not in colour
    assert "LINES" not in colour
    assert colour["COLORTERM"] == "truecolor"
    assert monochrome["NO_COLOR"] == "1"
    assert "COLORTERM" not in monochrome


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
    assert result["decision_verdict"] == "pass"
    assert result["excluded_failed_checks"] == [
        "workload_latency_growing-one-column-table"
    ]
    assert comparison == {
        "check": "workload_latency_growing-one-column-table",
        "current_sample_ms": 54.45,
        "threshold_ms": 50.0,
        "v0.4_sample_ms": 61.988,
        "block_markdown_reference_ms": 44.0,
        "v0.5_samples_ms": [54.45, 60.229, 68.861, 54.45],
        "v0.5_spread_ms": 14.411,
        "excluded_from_decision": True,
        "reason": (
            "known pre-existing exceedance excluded because its run-to-run variance on the "
            "unchanged v0.5 candidate exceeds the difference it would be used to detect"
        ),
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
def test_gate_probe_records_noisy_latency_above_v040_sample(tmp_path: Path) -> None:
    gate_program = tmp_path / "gate-program"
    report = _complete_gate_report(measured_ms=62.0)
    _write_gate_program(gate_program, report, exit_code=1)
    receipt_dir = tmp_path / "receipts"
    receipt_dir.mkdir()

    result = _probe_gate(
        gate_program,
        work_dir=tmp_path,
        environment={},
        receipt_dir=receipt_dir,
    )

    comparison = result["known_preexisting_exceedance"]
    assert result["decision_verdict"] == "pass"
    assert comparison["current_sample_ms"] == 62.0
    assert comparison["v0.4_sample_ms"] == 61.988
    assert comparison["v0.5_spread_ms"] == 14.411


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


def test_receipt_schema_uses_the_gateway_route_identifiers() -> None:
    schema = json.loads(
        (_REPO_ROOT / "docs" / "acceptance" / "v0.5.0" / "receipt.schema.json").read_text(
            encoding="utf-8"
        )
    )
    route_properties = schema["properties"]["session"]["properties"]["model_route"]["properties"]
    expected = [PRIMARY_MODEL_ROUTE, FALLBACK_MODEL_ROUTE, None]
    assert route_properties["requested"]["enum"] == expected
    assert route_properties["observed"]["enum"] == expected


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


def _primary_receipt() -> dict[str, Any]:
    return _receipt(
        route={
            "requested": PRIMARY_MODEL_ROUTE,
            "observed": PRIMARY_MODEL_ROUTE,
            "status": "used",
            "fallback_reason": None,
            "fallback_availability": "unavailable",
        }
    )


@pytest.mark.parametrize("label", ["capture", "screenshot"])
def test_evidence_file_verification_rejects_missing_and_changed_files(
    tmp_path: Path, label: str
) -> None:
    path = tmp_path / label
    expected = sha256_file(path) if path.exists() else "0" * 64
    assert _verify_evidence_file(path, expected, label=label) == [
        f"{label} file is missing: {path}"
    ]
    path.write_bytes(b"original")
    expected = sha256_file(path)
    assert _verify_evidence_file(path, expected, label=label) == []
    path.write_bytes(b"changed")
    assert _verify_evidence_file(path, expected, label=label) == [
        f"{label} hash does not match its file: {path}"
    ]


@pytest.mark.parametrize("missing_label", ["capture", "screenshot"])
def test_receipt_file_verification_accepts_bytes_then_rejects_missing_file(
    tmp_path: Path, missing_label: str
) -> None:
    capture = tmp_path / "raw" / "item-02.ansi"
    screenshot = tmp_path / "screenshots" / "item-02.png"
    capture.parent.mkdir()
    screenshot.parent.mkdir()
    capture.write_bytes(b"terminal bytes\x1b[32m")
    screenshot.write_bytes(b"portable network graphics bytes")
    receipt = _primary_receipt()
    evidence = receipt["evidence"]
    evidence["capture_path"] = str(capture)
    evidence["capture_sha256"] = sha256_file(capture)
    evidence["screenshot_path"] = str(screenshot)
    evidence["screenshot_sha256"] = sha256_file(screenshot)

    assert validate_receipt(receipt, verify_files=True) == []
    {"capture": capture, "screenshot": screenshot}[missing_label].unlink()

    errors = validate_receipt(receipt, verify_files=True)
    assert any(error.startswith(f"{missing_label} file is missing:") for error in errors)


@pytest.mark.parametrize("changed_label", ["capture", "screenshot"])
def test_receipt_file_verification_rejects_changed_file(
    tmp_path: Path, changed_label: str
) -> None:
    capture = tmp_path / "raw" / "item-02.ansi"
    screenshot = tmp_path / "screenshots" / "item-02.png"
    capture.parent.mkdir()
    screenshot.parent.mkdir()
    capture.write_bytes(b"terminal bytes")
    screenshot.write_bytes(b"portable network graphics bytes")
    receipt = _primary_receipt()
    evidence = receipt["evidence"]
    evidence["capture_path"] = str(capture)
    evidence["capture_sha256"] = sha256_file(capture)
    evidence["screenshot_path"] = str(screenshot)
    evidence["screenshot_sha256"] = sha256_file(screenshot)

    changed = {"capture": capture, "screenshot": screenshot}[changed_label]
    changed.write_bytes(changed.read_bytes() + b"changed")

    errors = validate_receipt(receipt, verify_files=True)
    assert any(
        error.startswith(f"{changed_label} hash does not match its file:")
        for error in errors
    )


@pytest.mark.parametrize(
    ("capture", "screenshot", "message"),
    [
        (Path("outside/capture.ansi"), Path("scratch/screenshots/item.png"), "raw capture escaped"),
        (
            Path("scratch/raw/item.ansi"),
            Path("outside/item.png"),
            "screenshot must remain",
        ),
    ],
)
def test_scratch_evidence_paths_reject_each_outside_source(
    tmp_path: Path, capture: Path, screenshot: Path, message: str
) -> None:
    with pytest.raises(HarnessError, match=message):
        validate_scratch_evidence_paths(
            capture=tmp_path / capture,
            screenshot=tmp_path / screenshot,
            scratch_root=tmp_path / "scratch",
        )


def test_receipt_commit_must_match_the_release_candidate() -> None:
    receipt = _primary_receipt()

    assert validate_receipt(
        receipt, verify_files=False, expected_commit="a" * 40
    ) == []
    errors = validate_receipt(receipt, verify_files=False, expected_commit="b" * 40)

    assert errors == ["artifact.commit does not match the release candidate"]


def test_receipt_rejects_the_current_operator_home_path() -> None:
    receipt = _primary_receipt()
    receipt["observations"] = [str(Path.home() / "private-workspace")]

    errors = validate_receipt(receipt, verify_files=False)

    assert "receipt contains the current user's home path" in errors


def test_public_install_receipt_scrubs_source_tree_and_operator_home(tmp_path: Path) -> None:
    receipt = {
        "schema_version": "talaria-v0.5.0-install-v1",
        "tester": "talaria-t2",
        "scratch_root": str(Path.home() / "acceptance"),
        "candidate": {
            "commit": "a" * 40,
            "integration_tree": str(Path.home() / "source"),
            "wheel_sha256": "b" * 64,
        },
    }
    destination = tmp_path / "install-receipt.json"

    write_public_install_receipt(receipt, destination)
    public = json.loads(destination.read_text(encoding="utf-8"))

    assert public["candidate"]["integration_tree"] == "<integration-tree>"
    assert public["scratch_root"] == "<scratch-root>"
    assert str(Path.home()) not in destination.read_text(encoding="utf-8")


def test_published_item_receipt_scrubs_the_tester_scratch_root(tmp_path: Path) -> None:
    repo_root = tmp_path / "repo"
    evidence_root = repo_root / "docs" / "acceptance" / "v0.5.0" / "evidence"
    public_install = evidence_root / "t1" / "install-receipt.json"
    public_install.parent.mkdir(parents=True)
    public_install.write_text(
        json.dumps(
            {
                "schema_version": "talaria-v0.5.0-install-v1",
                "tester": "talaria-t1",
                "candidate": {"commit": "a" * 40, "wheel_sha256": "b" * 64},
            }
        ),
        encoding="utf-8",
    )
    scratch = tmp_path / "talaria-v050-talaria-t1-private"
    capture = scratch / "raw" / "item-02.ansi"
    screenshot = scratch / "screenshots" / "item-02.png"
    pty_result = scratch / "receipts" / "item-02-pty.json"
    source_receipt = scratch / "receipts" / "item-02-talaria-t1.json"
    capture.parent.mkdir(parents=True)
    screenshot.parent.mkdir(parents=True)
    pty_result.parent.mkdir(parents=True)
    capture.write_bytes(b"terminal bytes")
    screenshot.write_bytes(b"png bytes")
    pty_result.write_text(
        json.dumps({"executable": str(scratch / "venv/bin/talaria")}),
        encoding="utf-8",
    )
    receipt = _primary_receipt()
    receipt["artifact"]["executable"] = str(scratch / "venv/bin/talaria")
    receipt["artifact"]["distribution_root"] = str(scratch / "venv/lib/site-packages")
    receipt["artifact"]["install_receipt_path"] = str(scratch / "install-receipt.json")
    receipt["evidence"].update(
        {
            "capture_path": str(capture),
            "capture_sha256": sha256_file(capture),
            "screenshot_path": str(screenshot),
            "screenshot_sha256": sha256_file(screenshot),
            "pty_result_path": str(pty_result),
            "pty_result_sha256": sha256_file(pty_result),
        }
    )
    source_receipt.write_text(json.dumps(receipt), encoding="utf-8")

    published_path = publish_receipt(
        source_receipt,
        public_install_receipt=public_install,
        evidence_root=evidence_root,
        repo_root=repo_root,
    )
    published = json.loads(published_path.read_text(encoding="utf-8"))

    assert published["artifact"]["executable"] == "<scratch-root>/venv/bin/talaria"
    assert published["artifact"]["distribution_root"] == (
        "<scratch-root>/venv/lib/site-packages"
    )
    assert str(scratch) not in published_path.read_text(encoding="utf-8")


def _write_verify_run_fixture(
    tmp_path: Path,
    *,
    manifest_commit: str,
    receipt_commit: str,
    include_receipt: bool = True,
    install_home_path: bool = False,
) -> tuple[Path, Path]:
    evidence_root = tmp_path / "docs" / "acceptance" / "v0.5.0" / "evidence"
    tester_root = evidence_root / "t1"
    receipt_path = tester_root / "receipts" / "item-02-talaria-t1.json"
    install_path = tester_root / "install-receipt.json"
    capture = tester_root / "raw" / "item-02.ansi"
    screenshot = tester_root / "screenshots" / "item-02.png"
    capture.parent.mkdir(parents=True)
    screenshot.parent.mkdir(parents=True)
    receipt_path.parent.mkdir(parents=True)
    capture.write_bytes(b"capture")
    screenshot.write_bytes(b"screenshot")
    receipt = _primary_receipt()
    receipt["artifact"]["commit"] = receipt_commit
    receipt["artifact"]["wheel_sha256"] = "b" * 64
    receipt["evidence"]["capture_path"] = capture.relative_to(tmp_path).as_posix()
    receipt["evidence"]["capture_sha256"] = sha256_file(capture)
    receipt["evidence"]["screenshot_path"] = screenshot.relative_to(tmp_path).as_posix()
    receipt["evidence"]["screenshot_sha256"] = sha256_file(screenshot)
    receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
    install = {
        "schema_version": "talaria-v0.5.0-install-v1",
        "tester": "talaria-t1",
        "candidate": {
            "commit": manifest_commit,
            "wheel_sha256": "b" * 64,
            "integration_tree": (
                str(Path.home() / "source") if install_home_path else "<integration-tree>"
            ),
        },
    }
    install_path.write_text(json.dumps(install), encoding="utf-8")
    receipt_relative = receipt_path.relative_to(tmp_path).as_posix()
    install_relative = install_path.relative_to(tmp_path).as_posix()
    manifest = {
        "status": "blocked",
        "current_candidate": {
            "commit": manifest_commit,
            "wheel_sha256": "b" * 64,
        },
        "receipts": (
            [
                {
                    "receipt_path": receipt_relative,
                    "receipt_sha256": sha256_file(receipt_path),
                    "checklist_item": 2,
                    "tester": "talaria-t1",
                    "verdict": "pass",
                }
            ]
            if include_receipt
            else []
        ),
        "install_receipts": [
            {
                "receipt_path": install_relative,
                "receipt_sha256": sha256_file(install_path),
            }
        ],
    }
    manifest_path = tmp_path / "artifact-manifest.json"
    manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
    return manifest_path, evidence_root


def test_verify_run_rejects_null_candidate_while_receipts_exist(tmp_path: Path) -> None:
    evidence_root = tmp_path / "evidence"
    receipt_path = evidence_root / "t1" / "receipts" / "item-02.json"
    receipt_path.parent.mkdir(parents=True)
    receipt_path.write_text("{}", encoding="utf-8")
    manifest_path = tmp_path / "artifact-manifest.json"
    manifest_path.write_text(
        json.dumps({"status": "not-run", "current_candidate": None, "receipts": []}),
        encoding="utf-8",
    )

    errors = verify_run(manifest_path, evidence_root=evidence_root, repo_root=tmp_path)

    assert errors == ["manifest candidate is null while receipts exist"]


def test_verify_run_rejects_receipt_from_another_candidate(tmp_path: Path) -> None:
    manifest_path, evidence_root = _write_verify_run_fixture(
        tmp_path, manifest_commit="b" * 40, receipt_commit="a" * 40
    )

    errors = verify_run(manifest_path, evidence_root=evidence_root, repo_root=tmp_path)

    assert any(
        error.endswith("artifact.commit does not match the release candidate")
        for error in errors
    )


def test_verify_run_rejects_receipt_absent_from_manifest(tmp_path: Path) -> None:
    manifest_path, evidence_root = _write_verify_run_fixture(
        tmp_path,
        manifest_commit="a" * 40,
        receipt_commit="a" * 40,
        include_receipt=False,
    )

    errors = verify_run(manifest_path, evidence_root=evidence_root, repo_root=tmp_path)

    assert any(error.startswith("receipt is absent from manifest:") for error in errors)


def test_verify_run_rejects_home_path_in_install_receipt(tmp_path: Path) -> None:
    manifest_path, evidence_root = _write_verify_run_fixture(
        tmp_path,
        manifest_commit="a" * 40,
        receipt_commit="a" * 40,
        install_home_path=True,
    )

    errors = verify_run(manifest_path, evidence_root=evidence_root, repo_root=tmp_path)

    assert any("install receipt contains the current user's home path" in error for error in errors)


def test_verify_run_rejects_superseded_origin_in_active_receipt(tmp_path: Path) -> None:
    manifest_path, evidence_root = _write_verify_run_fixture(
        tmp_path, manifest_commit="a" * 40, receipt_commit="a" * 40
    )
    receipt_path = next(evidence_root.glob("*/receipts/*.json"))
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    receipt["evidence"]["pty_result_path"] = (
        "docs/acceptance/v0.5.0/evidence/t1/superseded/old/pty-result.json"
    )
    receipt_path.write_text(json.dumps(receipt), encoding="utf-8")

    errors = verify_run(manifest_path, evidence_root=evidence_root, repo_root=tmp_path)

    assert any("superseded evidence origin" in error for error in errors)


def _git(repo: Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=repo, check=True, capture_output=True, text=True)


def test_verify_run_binds_manifest_candidate_to_released_commit(tmp_path: Path) -> None:
    _git(tmp_path, "init", "-q")
    _git(tmp_path, "config", "user.email", "acceptance@example.invalid")
    _git(tmp_path, "config", "user.name", "Acceptance")
    (tmp_path / "talaria").mkdir()
    (tmp_path / "talaria" / "module.py").write_text("value = 1\n")
    _git(tmp_path, "add", ".")
    _git(tmp_path, "commit", "-qm", "candidate")
    candidate = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=tmp_path, check=True, capture_output=True, text=True
    ).stdout.strip()
    manifest_path, evidence_root = _write_verify_run_fixture(
        tmp_path, manifest_commit=candidate, receipt_commit=candidate
    )
    (tmp_path / "docs-note.txt").write_text("non-release change\n")
    _git(tmp_path, "add", ".")
    _git(tmp_path, "commit", "-qm", "documentation only")
    equivalent = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=tmp_path, check=True, capture_output=True, text=True
    ).stdout.strip()

    assert not any(
        "released commit" in error
        for error in verify_run(
            manifest_path,
            evidence_root=evidence_root,
            repo_root=tmp_path,
            expected_candidate_commit=equivalent,
        )
    )

    (tmp_path / "talaria" / "module.py").write_text("value = 2\n")
    _git(tmp_path, "add", ".")
    _git(tmp_path, "commit", "-qm", "product change")
    divergent = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=tmp_path, check=True, capture_output=True, text=True
    ).stdout.strip()
    errors = verify_run(
        manifest_path,
        evidence_root=evidence_root,
        repo_root=tmp_path,
        expected_candidate_commit=divergent,
    )
    assert any("release-relevant files differ" in error for error in errors)


@pytest.mark.parametrize("suffix", [".ansi", ".jsonl"])
def test_verify_run_scans_superseded_raw_evidence_portably(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, suffix: str
) -> None:
    manifest_path, evidence_root = _write_verify_run_fixture(
        tmp_path, manifest_commit="a" * 40, receipt_commit="a" * 40
    )
    monkeypatch.setenv("HOME", "/different/operator")
    leaked = evidence_root / "t1" / "superseded" / f"leak{suffix}"
    leaked.parent.mkdir()
    leaked.write_bytes(b"private=/Users/other-operator/secret")

    errors = verify_run(manifest_path, evidence_root=evidence_root, repo_root=tmp_path)

    assert any("operator home path" in error and leaked.name in error for error in errors)


def test_verify_run_scans_png_ancillary_text(tmp_path: Path) -> None:
    manifest_path, evidence_root = _write_verify_run_fixture(
        tmp_path, manifest_commit="a" * 40, receipt_commit="a" * 40
    )
    payload = b"Comment\x00owner@example.com"
    chunk = struct.pack(">I", len(payload)) + b"tEXt" + payload + b"\x00\x00\x00\x00"
    leaked = evidence_root / "t1" / "superseded" / "leak.png"
    leaked.parent.mkdir()
    leaked.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk)

    errors = verify_run(manifest_path, evidence_root=evidence_root, repo_root=tmp_path)

    assert any("email address" in error and leaked.name in error for error in errors)


def test_portable_json_makes_repository_paths_relative_and_scrubs_home(
    tmp_path: Path,
) -> None:
    repository = tmp_path / "repo"
    document = {
        "event_script": str(repository / "docs" / "event.json"),
        "external_home_path": str(Path.home() / "private" / "capture.json"),
    }

    portable = _portable_json(document, repo_root=repository)

    assert portable == {
        "event_script": "docs/event.json",
        "external_home_path": "<home>/private/capture.json",
    }


def test_portable_json_replaces_the_tester_scratch_root(tmp_path: Path) -> None:
    repository = tmp_path / "repo"
    scratch = tmp_path / "talaria-v050-talaria-t1-secret"

    portable = _portable_json(
        {"path": str(scratch / "raw" / "capture.ansi")},
        repo_root=repository,
        scratch_root=scratch,
    )

    assert portable == {"path": "<scratch-root>/raw/capture.ansi"}


def test_acceptance_schemas_validate_current_manifest_and_active_receipts() -> None:
    acceptance_root = _REPO_ROOT / "docs" / "acceptance" / "v0.5.0"
    manifest_schema = json.loads(
        (acceptance_root / "artifact-manifest.schema.json").read_text(encoding="utf-8")
    )
    receipt_schema = json.loads(
        (acceptance_root / "receipt.schema.json").read_text(encoding="utf-8")
    )
    Draft202012Validator.check_schema(manifest_schema)
    Draft202012Validator.check_schema(receipt_schema)

    manifest = json.loads(
        (acceptance_root / "artifact-manifest.json").read_text(encoding="utf-8")
    )
    Draft202012Validator(manifest_schema).validate(manifest)
    receipt_paths = sorted((acceptance_root / "evidence").glob("*/receipts/*.json"))
    assert receipt_paths
    receipt_validator = Draft202012Validator(receipt_schema)
    for receipt_path in receipt_paths:
        receipt_validator.validate(json.loads(receipt_path.read_text(encoding="utf-8")))


def test_manifest_schema_names_its_cross_field_validator() -> None:
    schema_path = (
        _REPO_ROOT / "docs" / "acceptance" / "v0.5.0" / "artifact-manifest.schema.json"
    )
    schema = json.loads(schema_path.read_text(encoding="utf-8"))

    assert "scripts.acceptance.v050_records check" in schema["description"]


def test_workflow_actions_are_commit_pinned_and_managed() -> None:
    workflow_root = _REPO_ROOT / ".github" / "workflows"
    action_lines = [
        line.strip()
        for path in sorted(workflow_root.glob("*.yml"))
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip().startswith("uses:")
    ]
    assert action_lines
    for line in action_lines:
        reference = line.split("#", 1)[0].rsplit("@", 1)[-1].strip()
        assert re.fullmatch(r"[0-9a-f]{40}", reference), line
    dependabot = (_REPO_ROOT / ".github" / "dependabot.yml").read_text(encoding="utf-8")
    assert "package-ecosystem: github-actions" in dependabot


def test_workflows_keep_strict_product_bandit_and_covered_harness_bandit() -> None:
    for name in ("release.yml", "validate.yml"):
        workflow = (_REPO_ROOT / ".github" / "workflows" / name).read_text(
            encoding="utf-8"
        )
        assert "uv run bandit -r talaria -q" in workflow
        assert "uv run bandit -r scripts -q -ll" in workflow
        assert "bandit -r talaria scripts" not in workflow


def test_release_workflow_binds_acceptance_to_the_released_commit() -> None:
    workflow = (_REPO_ROOT / ".github" / "workflows" / "release.yml").read_text(
        encoding="utf-8"
    )
    assert '--expect-candidate "${{ github.sha }}"' in workflow


def test_committed_acceptance_evidence_passes_portable_privacy_scan() -> None:
    acceptance = _REPO_ROOT / "docs" / "acceptance" / "v0.5.0"
    assert verify_run(
        acceptance / "artifact-manifest.json",
        evidence_root=acceptance / "evidence",
        repo_root=_REPO_ROOT,
    ) == []


def test_manifest_schema_rejects_non_not_run_status_without_receipts() -> None:
    acceptance_root = _REPO_ROOT / "docs" / "acceptance" / "v0.5.0"
    schema = json.loads(
        (acceptance_root / "artifact-manifest.schema.json").read_text(encoding="utf-8")
    )
    manifest = json.loads(
        (acceptance_root / "artifact-manifest.json").read_text(encoding="utf-8")
    )
    invalid = copy.deepcopy(manifest)
    invalid["status"] = "blocked"
    invalid["receipts"] = []

    errors = list(Draft202012Validator(schema).iter_errors(invalid))

    assert any(error.validator == "minItems" for error in errors)


def test_manifest_schema_rejects_receipts_when_status_is_not_run() -> None:
    acceptance_root = _REPO_ROOT / "docs" / "acceptance" / "v0.5.0"
    schema = json.loads(
        (acceptance_root / "artifact-manifest.schema.json").read_text(encoding="utf-8")
    )
    manifest = json.loads(
        (acceptance_root / "artifact-manifest.json").read_text(encoding="utf-8")
    )
    invalid = copy.deepcopy(manifest)
    invalid["status"] = "not-run"

    errors = list(Draft202012Validator(schema).iter_errors(invalid))

    assert any(error.validator == "maxItems" for error in errors)
