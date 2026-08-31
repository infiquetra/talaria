#!/usr/bin/env python3
"""Build, freeze, install, and probe the Talaria v0.5.0 candidate wheel.

The build and install are separate subcommands so both testers install the same
frozen wheel digest. The install subcommand creates a random scratch root, a
fresh virtual environment, and a dedicated ``TALARIA_CONFIG_DIR``. It then
proves that imports and the console script come from that non-editable wheel
before running version, bare-launch, and gate probes.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path
from typing import Any

from scripts.acceptance.v050_common import (
    RELEASE_VERSION,
    HarnessError,
    command_output,
    is_within,
    isolated_environment,
    probe_installed_artifact,
    read_json_object,
    run_command,
    sha256_file,
    validate_config_dir,
    validate_tester,
    write_json_object,
)
from scripts.acceptance.v050_pty_driver import DriveEvent, run_pty


def _utc_now() -> str:
    return dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat()


def _wheel_metadata(path: Path) -> dict[str, str]:
    try:
        with zipfile.ZipFile(path) as wheel:
            metadata_names = [
                name for name in wheel.namelist() if name.endswith(".dist-info/METADATA")
            ]
            if len(metadata_names) != 1:
                raise HarnessError(
                    f"candidate wheel has {len(metadata_names)} METADATA files, expected one"
                )
            metadata = wheel.read(metadata_names[0]).decode("utf-8")
    except (OSError, UnicodeDecodeError, zipfile.BadZipFile) as exc:
        raise HarnessError(f"cannot inspect candidate wheel {path}: {exc}") from exc

    fields: dict[str, str] = {}
    for line in metadata.splitlines():
        name, separator, value = line.partition(":")
        if separator and name in {"Name", "Version"}:
            fields[name.lower()] = value.strip()
    if fields.get("name") != "talaria" or not fields.get("version"):
        raise HarnessError(f"{path}: wheel metadata does not identify Talaria and its version")
    return fields


def build_candidate(
    *, integration_tree: Path, candidate_dir: Path, expected_version: str
) -> Path:
    """Build one clean-tree wheel and write its immutable candidate manifest."""
    integration_tree = integration_tree.expanduser().resolve()
    candidate_dir = candidate_dir.expanduser().resolve()
    if not integration_tree.is_dir() or not (integration_tree / "pyproject.toml").is_file():
        raise HarnessError(f"not a Talaria integration tree: {integration_tree}")
    root = Path(
        command_output(["git", "rev-parse", "--show-toplevel"], cwd=integration_tree)
    ).resolve()
    if root != integration_tree:
        raise HarnessError(f"integration tree must be its Git root: {integration_tree} != {root}")
    if is_within(candidate_dir, integration_tree):
        raise HarnessError("candidate output must be outside the integration tree")
    if candidate_dir.exists():
        raise HarnessError(f"candidate directory must be fresh and absent: {candidate_dir}")

    dirty = command_output(
        ["git", "status", "--porcelain", "--untracked-files=all"], cwd=integration_tree
    )
    if dirty:
        message = (
            "integration tree is not clean; a commit identifier would not identify "
            "the built bytes:\n"
        )
        raise HarnessError(
            f"{message}{dirty}"
        )
    commit = command_output(["git", "rev-parse", "HEAD"], cwd=integration_tree)
    short_commit = command_output(
        ["git", "rev-parse", "--short=9", "HEAD"], cwd=integration_tree
    )
    branch = command_output(["git", "branch", "--show-current"], cwd=integration_tree)
    if not branch:
        raise HarnessError("integration tree is detached; candidate branch provenance is absent")

    uv = shutil.which("uv")
    if uv is None:
        raise HarnessError("uv is required to build the candidate wheel")
    wheel_dir = candidate_dir / "wheel"
    wheel_dir.mkdir(parents=True)
    build = run_command(
        [uv, "build", "--wheel", "--out-dir", str(wheel_dir)],
        cwd=integration_tree,
        timeout=300.0,
    )
    (candidate_dir / "build.stdout.log").write_text(build.stdout, encoding="utf-8")
    (candidate_dir / "build.stderr.log").write_text(build.stderr, encoding="utf-8")
    if build.returncode != 0:
        raise HarnessError(
            f"wheel build failed ({build.returncode}); logs remain under {candidate_dir}"
        )
    wheels = sorted(wheel_dir.glob("*.whl"))
    if len(wheels) != 1:
        raise HarnessError(f"build produced {len(wheels)} wheels under {wheel_dir}, expected one")
    wheel = wheels[0].resolve()
    metadata = _wheel_metadata(wheel)
    if metadata["version"] != expected_version:
        raise HarnessError(
            f"candidate wheel version is {metadata['version']!r}, expected {expected_version!r}"
        )

    manifest = {
        "schema_version": "talaria-v0.5.0-candidate-v1",
        "built_at": _utc_now(),
        "branch": branch,
        "commit": commit,
        "commit_short": short_commit,
        "integration_tree": str(integration_tree),
        "version": metadata["version"],
        "wheel_filename": wheel.name,
        "wheel_path": str(wheel),
        "wheel_sha256": sha256_file(wheel),
    }
    manifest_path = candidate_dir / "candidate.json"
    write_json_object(manifest_path, manifest)
    return manifest_path


def _candidate(path: Path, expected_version: str) -> tuple[dict[str, Any], Path]:
    candidate = read_json_object(path)
    if candidate.get("schema_version") != "talaria-v0.5.0-candidate-v1":
        raise HarnessError(f"{path}: not a Talaria v0.5.0 candidate manifest")
    if candidate.get("version") != expected_version:
        raise HarnessError(
            f"candidate version is {candidate.get('version')!r}, expected {expected_version!r}"
        )
    wheel_raw = candidate.get("wheel_path")
    digest = candidate.get("wheel_sha256")
    if not isinstance(wheel_raw, str) or not isinstance(digest, str):
        raise HarnessError(f"{path}: candidate wheel path or digest is missing")
    wheel = Path(wheel_raw).expanduser().resolve()
    if not wheel.is_file():
        raise HarnessError(f"candidate wheel is missing: {wheel}")
    observed = sha256_file(wheel)
    if observed != digest:
        raise HarnessError(f"candidate wheel changed: {observed} != frozen {digest}")
    metadata = _wheel_metadata(wheel)
    if metadata["version"] != expected_version:
        raise HarnessError(f"candidate wheel metadata changed to {metadata['version']!r}")
    return candidate, wheel


def _probe_version(
    executable: Path, *, work_dir: Path, environment: dict[str, str], expected_version: str
) -> dict[str, Any]:
    result = run_command(
        [str(executable), "--version"], cwd=work_dir, environment=environment, timeout=30.0
    )
    expected = f"talaria {expected_version}"
    if result.returncode != 0 or result.stdout.strip() != expected:
        raise HarnessError(
            f"version probe failed: exit={result.returncode}, stdout={result.stdout!r}, "
            f"stderr={result.stderr!r}, expected={expected!r}"
        )
    return {"argv": [str(executable), "--version"], "exit_code": 0, "stdout": expected}


def _probe_bare_launch(
    executable: Path,
    *,
    work_dir: Path,
    environment: dict[str, str],
    raw_dir: Path,
    rows: int,
    columns: int,
    term: str,
) -> dict[str, Any]:
    # A fresh scratch config may reach either the interactive credential prompt
    # or the interface. ctrl+d ends getpass's blocking read; ctrl+q exits the
    # interface.
    run = run_pty(
        executable=executable,
        argv=[str(executable)],
        cwd=work_dir,
        environment=environment,
        capture_path=raw_dir / "install-bare-launch.ansi",
        events=[
            DriveEvent(1.0, "key", "CTRL_D"),
            DriveEvent(2.0, "key", "CTRL_Q"),
        ],
        expected_literals=[],
        rows=rows,
        columns=columns,
        term=term,
        terminal_program="Python pty.fork install probe",
        timeout=10.0,
    )
    if run.timed_out:
        raise HarnessError("bare installed launch hung and was killed after 10 seconds")
    if run.exit_code == 127:
        raise HarnessError("bare installed launch could not execute")
    if run.capture_bytes == 0:
        raise HarnessError("bare installed launch produced an empty terminal capture")
    document = run.as_json()
    document["probe_note"] = (
        "Fresh isolated config: reaching either the credential prompt or the interface is valid; "
        "the scripted ctrl+d/ctrl+q exits the reached surface."
    )
    return document


def _probe_gate(
    executable: Path,
    *,
    work_dir: Path,
    environment: dict[str, str],
    receipt_dir: Path,
) -> dict[str, Any]:
    gate_json = receipt_dir / "install-gate.json"
    argv = [str(executable), "gate", "--deltas", "5000", "--json", str(gate_json)]
    result = run_command(argv, cwd=work_dir, environment=environment, timeout=300.0)
    (receipt_dir / "install-gate.stdout.log").write_text(result.stdout, encoding="utf-8")
    (receipt_dir / "install-gate.stderr.log").write_text(result.stderr, encoding="utf-8")
    if result.returncode != 0:
        raise HarnessError(
            f"installed gate probe failed ({result.returncode}); logs remain under {receipt_dir}"
        )
    if not gate_json.is_file():
        raise HarnessError("installed gate probe exited zero without writing its JSON result")
    try:
        parsed: Any = json.loads(gate_json.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise HarnessError("installed gate probe wrote invalid JSON") from exc
    if not isinstance(parsed, dict):
        raise HarnessError("installed gate probe JSON root is not an object")
    return {
        "argv": argv,
        "exit_code": result.returncode,
        "json_path": str(gate_json.resolve()),
        "json_sha256": sha256_file(gate_json),
        "stderr_path": str((receipt_dir / "install-gate.stderr.log").resolve()),
        "stdout_path": str((receipt_dir / "install-gate.stdout.log").resolve()),
    }


def install_candidate(
    *,
    candidate_manifest: Path,
    tester: str,
    scratch_parent: Path,
    expected_version: str,
    term: str,
    rows: int,
    columns: int,
) -> Path:
    """Install and probe the frozen wheel in one randomly named tester root."""
    tester = validate_tester(tester)
    candidate, wheel = _candidate(candidate_manifest, expected_version)
    scratch_parent = scratch_parent.expanduser().resolve()
    scratch_parent.mkdir(parents=True, exist_ok=True)
    scratch_root = Path(
        tempfile.mkdtemp(prefix=f"talaria-v050-{tester}-", dir=scratch_parent)
    ).resolve()
    scratch_root.chmod(0o700)
    config_dir = scratch_root / "config"
    config_dir.mkdir(mode=0o700)
    raw_dir = scratch_root / "raw"
    raw_dir.mkdir()
    receipt_dir = scratch_root / "receipts"
    receipt_dir.mkdir()
    (scratch_root / "screenshots").mkdir()
    work_dir = scratch_root / "work"
    work_dir.mkdir()
    validate_config_dir(config_dir, scratch_root=scratch_root)

    marker = {
        "schema_version": "talaria-v0.5.0-config-marker-v1",
        "tester": tester,
        "wheel_sha256": candidate["wheel_sha256"],
    }
    write_json_object(config_dir / ".acceptance-config.json", marker)

    venv = scratch_root / "venv"
    create = run_command(
        [sys.executable, "-m", "venv", str(venv)], cwd=work_dir, timeout=120.0
    )
    (receipt_dir / "venv.stdout.log").write_text(create.stdout, encoding="utf-8")
    (receipt_dir / "venv.stderr.log").write_text(create.stderr, encoding="utf-8")
    if create.returncode != 0:
        raise HarnessError(f"fresh venv creation failed; scratch remains at {scratch_root}")

    venv_python = venv / "bin" / "python"
    install = run_command(
        [str(venv_python), "-m", "pip", "install", str(wheel)],
        cwd=work_dir,
        timeout=300.0,
    )
    (receipt_dir / "pip.stdout.log").write_text(install.stdout, encoding="utf-8")
    (receipt_dir / "pip.stderr.log").write_text(install.stderr, encoding="utf-8")
    if install.returncode != 0:
        raise HarnessError(f"wheel installation failed; scratch remains at {scratch_root}")

    executable = venv / "bin" / "talaria"
    environment = isolated_environment(
        config_dir=config_dir,
        term=term,
        rows=rows,
        columns=columns,
        venv_bin=venv / "bin",
    )
    integration_tree_raw = candidate.get("integration_tree")
    if not isinstance(integration_tree_raw, str):
        raise HarnessError("candidate manifest has no integration-tree provenance")
    identity = probe_installed_artifact(
        venv=venv,
        executable=executable,
        work_dir=work_dir,
        environment=environment,
        expected_version=expected_version,
        integration_tree=Path(integration_tree_raw),
        wheel_sha256=str(candidate["wheel_sha256"]),
    )
    version_probe = _probe_version(
        executable, work_dir=work_dir, environment=environment, expected_version=expected_version
    )
    bare_probe = _probe_bare_launch(
        executable,
        work_dir=work_dir,
        environment=environment,
        raw_dir=raw_dir,
        rows=rows,
        columns=columns,
        term=term,
    )
    gate_probe = _probe_gate(
        executable, work_dir=work_dir, environment=environment, receipt_dir=receipt_dir
    )

    receipt = {
        "schema_version": "talaria-v0.5.0-install-v1",
        "installed_at": _utc_now(),
        "tester": tester,
        "scratch_root": str(scratch_root),
        "config_dir": str(config_dir.resolve()),
        "venv": str(venv.resolve()),
        "candidate": {
            "branch": candidate["branch"],
            "commit": candidate["commit"],
            "commit_short": candidate["commit_short"],
            "integration_tree": candidate["integration_tree"],
            "wheel_filename": candidate["wheel_filename"],
            "wheel_path": candidate["wheel_path"],
            "wheel_sha256": candidate["wheel_sha256"],
        },
        "artifact": {
            "direct_url": identity["direct_url"],
            "distribution_root": identity["distribution_root"],
            "executable": identity["executable"],
            "executable_sha256": identity["executable_sha256"],
            "installed_file_count": identity["installed_file_count"],
            "installed_files_sha256": identity["installed_files_sha256"],
            "package_file": identity["package_file"],
            "version": identity["distribution_version"],
        },
        "terminal": {
            "program": "POSIX pseudo-terminal",
            "term": term,
            "rows": rows,
            "columns": columns,
        },
        "probes": {"version": version_probe, "bare_launch": bare_probe, "gate": gate_probe},
    }
    receipt_path = scratch_root / "install-receipt.json"
    write_json_object(receipt_path, receipt)
    return receipt_path


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    build = subparsers.add_parser("build", help="build and freeze the clean integration tree")
    build.add_argument("--integration-tree", type=Path, required=True)
    build.add_argument("--candidate-dir", type=Path, required=True)
    build.add_argument("--expected-version", default=RELEASE_VERSION)

    install = subparsers.add_parser("install", help="install the frozen wheel for one tester")
    install.add_argument("--candidate-manifest", type=Path, required=True)
    install.add_argument("--tester", required=True)
    install.add_argument("--scratch-parent", type=Path, default=Path(tempfile.gettempdir()))
    install.add_argument("--expected-version", default=RELEASE_VERSION)
    install.add_argument("--term", default="xterm-256color")
    install.add_argument("--rows", type=int, default=36)
    install.add_argument("--columns", type=int, default=132)
    return parser


def _main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.command == "build":
        path = build_candidate(
            integration_tree=args.integration_tree,
            candidate_dir=args.candidate_dir,
            expected_version=args.expected_version,
        )
    else:
        path = install_candidate(
            candidate_manifest=args.candidate_manifest,
            tester=args.tester,
            scratch_parent=args.scratch_parent,
            expected_version=args.expected_version,
            term=args.term,
            rows=args.rows,
            columns=args.columns,
        )
    print(path)
    return 0


def main(argv: list[str] | None = None) -> int:
    try:
        return _main(argv)
    except HarnessError as exc:
        print(f"v050_install_probe: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
