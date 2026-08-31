#!/usr/bin/env python3
"""Drive one installed Talaria process through a real POSIX pseudo-terminal.

The capture is written as bytes. It is deliberately not normalised, decoded,
or rendered here: ANSI colour and cursor/layout sequences are acceptance
evidence, and transforming them would test the transformer instead of Talaria.
"""

from __future__ import annotations

import argparse
import errno
import fcntl
import os
import pty
import select
import signal
import struct
import sys
import termios
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from scripts.acceptance.v050_common import (
    HarnessError,
    is_within,
    isolated_environment,
    probe_installed_artifact,
    read_json_object,
    sha256_file,
    validate_config_dir,
    validate_tester,
    write_json_object,
)

_KEY_BYTES = {
    "ENTER": b"\r",
    "ESCAPE": b"\x1b",
    "TAB": b"\t",
    "SHIFT_TAB": b"\x1b[Z",
    "UP": b"\x1b[A",
    "DOWN": b"\x1b[B",
    "RIGHT": b"\x1b[C",
    "LEFT": b"\x1b[D",
    "HOME": b"\x1b[H",
    "END": b"\x1b[F",
    "PAGE_UP": b"\x1b[5~",
    "PAGE_DOWN": b"\x1b[6~",
    "F5": b"\x1b[15~",
}
for _letter in "ABCDEFGHIJKLMNOPQRSTUVWXYZ":
    _KEY_BYTES[f"CTRL_{_letter}"] = bytes([ord(_letter) - ord("A") + 1])

_SIGNALS = {
    "SIGHUP": signal.SIGHUP,
    "SIGINT": signal.SIGINT,
    "SIGTERM": signal.SIGTERM,
    "SIGKILL": signal.SIGKILL,
}


@dataclass(frozen=True)
class DriveEvent:
    """One validated action scheduled from process start."""

    at_seconds: float
    kind: str
    value: str | tuple[int, int]


@dataclass(frozen=True)
class PtyResult:
    """Observed result of one real child process on a pseudo-terminal."""

    argv: tuple[str, ...]
    exit_code: int
    timed_out: bool
    duration_seconds: float
    capture_path: Path
    capture_sha256: str
    capture_bytes: int
    rows: int
    columns: int
    term: str
    terminal_program: str
    expected_literals: tuple[str, ...]
    missing_literals: tuple[str, ...]

    def as_json(self) -> dict[str, Any]:
        return {
            "argv": list(self.argv),
            "capture": {
                "bytes": self.capture_bytes,
                "path": str(self.capture_path),
                "sha256": self.capture_sha256,
            },
            "columns": self.columns,
            "duration_seconds": round(self.duration_seconds, 6),
            "exit_code": self.exit_code,
            "expected_literals": list(self.expected_literals),
            "missing_literals": list(self.missing_literals),
            "rows": self.rows,
            "term": self.term,
            "terminal_program": self.terminal_program,
            "timed_out": self.timed_out,
        }


def _positive_number(value: Any, *, field: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise HarnessError(f"event {field} must be a number")
    number = float(value)
    if number < 0:
        raise HarnessError(f"event {field} must be zero or greater")
    return number


def _positive_int(value: Any, *, field: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 1:
        raise HarnessError(f"event {field} must be a positive integer")
    return value


def parse_events(path: Path) -> list[DriveEvent]:
    """Read a timed event script and reject ambiguous or unknown actions."""
    document = read_json_object(path)
    raw_events = document.get("events")
    if not isinstance(raw_events, list):
        raise HarnessError(f"{path}: `events` must be an array")

    events: list[DriveEvent] = []
    for index, raw in enumerate(raw_events):
        if not isinstance(raw, dict):
            raise HarnessError(f"{path}: event {index} must be an object")
        at_seconds = _positive_number(raw.get("at_seconds"), field="at_seconds")
        actions = [name for name in ("text", "key", "hex_bytes", "resize", "signal") if name in raw]
        if len(actions) != 1:
            raise HarnessError(f"{path}: event {index} must declare exactly one action")
        action = actions[0]
        value = raw[action]
        if action == "text":
            if not isinstance(value, str):
                raise HarnessError(f"{path}: event {index} text must be a string")
            events.append(DriveEvent(at_seconds, action, value))
        elif action == "key":
            if not isinstance(value, str) or value not in _KEY_BYTES:
                allowed = ", ".join(sorted(_KEY_BYTES))
                raise HarnessError(f"{path}: event {index} has unknown key; expected: {allowed}")
            events.append(DriveEvent(at_seconds, action, value))
        elif action == "hex_bytes":
            if not isinstance(value, str):
                raise HarnessError(f"{path}: event {index} hex_bytes must be a string")
            try:
                bytes.fromhex(value)
            except ValueError as exc:
                raise HarnessError(f"{path}: event {index} hex_bytes is invalid") from exc
            events.append(DriveEvent(at_seconds, action, value))
        elif action == "resize":
            if not isinstance(value, dict):
                raise HarnessError(f"{path}: event {index} resize must be an object")
            rows = _positive_int(value.get("rows"), field="resize.rows")
            columns = _positive_int(value.get("columns"), field="resize.columns")
            events.append(DriveEvent(at_seconds, action, (rows, columns)))
        else:
            if not isinstance(value, str) or value not in _SIGNALS:
                allowed = ", ".join(sorted(_SIGNALS))
                raise HarnessError(
                    f"{path}: event {index} has unknown signal; expected: {allowed}"
                )
            events.append(DriveEvent(at_seconds, action, value))

    if events != sorted(events, key=lambda event: event.at_seconds):
        raise HarnessError(f"{path}: events must be ordered by nondecreasing at_seconds")
    return events


def _set_size(master: int, rows: int, columns: int) -> None:
    packed = struct.pack("HHHH", rows, columns, 0, 0)
    fcntl.ioctl(master, termios.TIOCSWINSZ, packed)


def _send_event(master: int, pid: int, event: DriveEvent) -> tuple[int, int] | None:
    if event.kind == "text":
        os.write(master, str(event.value).encode("utf-8"))
    elif event.kind == "key":
        os.write(master, _KEY_BYTES[str(event.value)])
    elif event.kind == "hex_bytes":
        os.write(master, bytes.fromhex(str(event.value)))
    elif event.kind == "resize":
        assert isinstance(event.value, tuple)
        rows, columns = event.value
        _set_size(master, rows, columns)
        return rows, columns
    else:
        os.killpg(pid, _SIGNALS[str(event.value)])
    return None


def _terminate_child(pid: int) -> int:
    """Terminate and reap the harness-owned process group after a timeout."""
    try:
        os.killpg(pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    deadline = time.monotonic() + 2.0
    while time.monotonic() < deadline:
        waited, status = os.waitpid(pid, os.WNOHANG)
        if waited == pid:
            return os.waitstatus_to_exitcode(status)
        time.sleep(0.02)
    try:
        os.killpg(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    _, status = os.waitpid(pid, 0)
    return os.waitstatus_to_exitcode(status)


def run_pty(
    *,
    executable: Path,
    argv: list[str],
    cwd: Path,
    environment: dict[str, str],
    capture_path: Path,
    events: list[DriveEvent],
    expected_literals: list[str],
    rows: int,
    columns: int,
    term: str,
    terminal_program: str,
    timeout: float,
) -> PtyResult:
    """Run and drive one child, preserving every byte read from the master."""
    if timeout <= 0:
        raise HarnessError("timeout must be positive")
    if capture_path.exists():
        raise HarnessError(f"refusing to replace existing raw capture: {capture_path}")
    capture_path.parent.mkdir(parents=True, exist_ok=True)

    started = time.monotonic()
    pid, master = pty.fork()
    if pid == 0:  # pragma: no cover - the child replaces this process
        try:
            os.chdir(cwd)
            os.execve(str(executable), argv, environment)
        except BaseException:
            os._exit(127)

    _set_size(master, rows, columns)
    os.set_blocking(master, False)
    pending = list(events)
    output = bytearray()
    status: int | None = None
    timed_out = False
    final_rows = rows
    final_columns = columns

    try:
        with capture_path.open("xb") as capture:
            while status is None:
                elapsed = time.monotonic() - started
                while pending and pending[0].at_seconds <= elapsed:
                    size = _send_event(master, pid, pending.pop(0))
                    if size is not None:
                        final_rows, final_columns = size

                wait_seconds = min(0.05, max(0.0, timeout - elapsed))
                readable, _, _ = select.select([master], [], [], wait_seconds)
                if readable:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError as exc:
                        if exc.errno != errno.EIO:
                            raise
                    else:
                        if chunk:
                            capture.write(chunk)
                            capture.flush()
                            output.extend(chunk)

                waited, child_status = os.waitpid(pid, os.WNOHANG)
                if waited == pid:
                    status = child_status
                    continue
                if elapsed >= timeout:
                    timed_out = True
                    break

            if timed_out:
                exit_code = _terminate_child(pid)
            else:
                assert status is not None
                exit_code = os.waitstatus_to_exitcode(status)

            # Drain bytes already queued by the slave before it closed.
            while True:
                readable, _, _ = select.select([master], [], [], 0)
                if not readable:
                    break
                try:
                    chunk = os.read(master, 65536)
                except OSError as exc:
                    if exc.errno == errno.EIO:
                        break
                    raise
                if not chunk:
                    break
                capture.write(chunk)
                output.extend(chunk)
    finally:
        os.close(master)

    decoded = bytes(output).decode("utf-8", "replace")
    missing = tuple(literal for literal in expected_literals if literal not in decoded)
    duration = time.monotonic() - started
    return PtyResult(
        argv=tuple(argv),
        exit_code=exit_code,
        timed_out=timed_out,
        duration_seconds=duration,
        capture_path=capture_path.resolve(),
        capture_sha256=sha256_file(capture_path),
        capture_bytes=capture_path.stat().st_size,
        rows=final_rows,
        columns=final_columns,
        term=term,
        terminal_program=terminal_program,
        expected_literals=tuple(expected_literals),
        missing_literals=missing,
    )


def _load_install_receipt(path: Path, tester: str) -> dict[str, Any]:
    receipt = read_json_object(path)
    if receipt.get("schema_version") != "talaria-v0.5.0-install-v1":
        raise HarnessError(f"{path}: not a Talaria v0.5.0 install receipt")
    if receipt.get("tester") != tester:
        raise HarnessError(f"{path}: receipt belongs to {receipt.get('tester')!r}, not {tester!r}")
    return receipt


def _path_field(document: dict[str, Any], *names: str) -> Path:
    value: Any = document
    for name in names:
        if not isinstance(value, dict):
            raise HarnessError(f"install receipt field {'.'.join(names)} is malformed")
        value = value.get(name)
    if not isinstance(value, str) or not value:
        raise HarnessError(f"install receipt field {'.'.join(names)} is missing")
    return Path(value).expanduser().resolve()


def _main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install-receipt", type=Path, required=True)
    parser.add_argument("--tester", required=True)
    parser.add_argument("--event-script", type=Path, required=True)
    parser.add_argument("--capture", type=Path, required=True)
    parser.add_argument("--result", type=Path, required=True)
    parser.add_argument("--cwd", type=Path)
    parser.add_argument("--term", default="xterm-256color")
    parser.add_argument(
        "--terminal-program",
        required=True,
        help="name/version of the tester terminal hosting this controlled pseudo-terminal",
    )
    parser.add_argument("--rows", type=int, default=36)
    parser.add_argument("--columns", type=int, default=132)
    parser.add_argument(
        "--monochrome",
        action="store_true",
        help="set NO_COLOR=1 explicitly for non-colour acceptance legs",
    )
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--expect", action="append", default=[])
    parser.add_argument("--accept-exit", action="append", type=int, default=[])
    parser.add_argument("command_args", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)

    tester = validate_tester(args.tester)
    receipt = _load_install_receipt(args.install_receipt, tester)
    scratch_root = _path_field(receipt, "scratch_root")
    executable = _path_field(receipt, "artifact", "executable")
    config_dir = validate_config_dir(
        _path_field(receipt, "config_dir"), scratch_root=scratch_root
    )
    venv = _path_field(receipt, "venv")
    expected_executable = (venv / "bin" / "talaria").resolve()
    if executable != expected_executable or not executable.is_file():
        raise HarnessError(
            f"installed executable no longer matches the tester venv: {executable}"
        )
    if not is_within(executable, venv):
        raise HarnessError(f"refusing executable outside tester venv: {executable}")

    capture = args.capture.expanduser().resolve()
    raw_root = (scratch_root / "raw").resolve()
    if not is_within(capture, raw_root):
        raise HarnessError(
            f"raw capture must stay under tester scratch until redaction review: {raw_root}"
        )
    result_path = args.result.expanduser().resolve()
    if not is_within(result_path, scratch_root / "receipts"):
        raise HarnessError(f"driver result must stay under {scratch_root / 'receipts'}")

    cwd = (args.cwd or (scratch_root / "work")).expanduser().resolve()
    if not is_within(cwd, scratch_root):
        raise HarnessError(f"drive cwd must stay under tester scratch: {cwd}")
    cwd.mkdir(parents=True, exist_ok=True)
    events = parse_events(args.event_script)
    command_args = list(args.command_args)
    if command_args and command_args[0] == "--":
        command_args.pop(0)
    child_argv = [str(executable), *command_args]
    environment = isolated_environment(
        config_dir=config_dir,
        term=args.term,
        rows=args.rows,
        columns=args.columns,
        venv_bin=venv / "bin",
        monochrome=args.monochrome,
    )
    artifact = receipt.get("artifact")
    candidate = receipt.get("candidate")
    if not isinstance(artifact, dict) or not isinstance(candidate, dict):
        raise HarnessError("install receipt has no candidate or artifact identity")
    version = artifact.get("version")
    wheel_sha256 = candidate.get("wheel_sha256")
    integration_tree_raw = candidate.get("integration_tree")
    if not isinstance(version, str) or not isinstance(wheel_sha256, str):
        raise HarnessError("install receipt has incomplete version or wheel identity")
    integration_tree = (
        Path(integration_tree_raw)
        if isinstance(integration_tree_raw, str) and integration_tree_raw
        else None
    )
    observed_identity = probe_installed_artifact(
        venv=venv,
        executable=executable,
        work_dir=cwd,
        environment=environment,
        expected_version=version,
        integration_tree=integration_tree,
        wheel_sha256=wheel_sha256,
    )
    if observed_identity.get("executable_sha256") != artifact.get("executable_sha256"):
        raise HarnessError("installed executable changed after the install probe")
    if observed_identity.get("installed_files_sha256") != artifact.get(
        "installed_files_sha256"
    ):
        raise HarnessError("installed Talaria files changed after the install probe")
    run = run_pty(
        executable=executable,
        argv=child_argv,
        cwd=cwd,
        environment=environment,
        capture_path=capture,
        events=events,
        expected_literals=list(args.expect),
        rows=args.rows,
        columns=args.columns,
        term=args.term,
        terminal_program=args.terminal_program,
        timeout=args.timeout,
    )
    result_document = {
        "schema_version": "talaria-v0.5.0-pty-v1",
        "tester": tester,
        **run.as_json(),
    }
    write_json_object(result_path, result_document)

    accepted_exits = set(args.accept_exit or [0])
    failures: list[str] = []
    if run.timed_out:
        failures.append(f"child exceeded {args.timeout:g}s and was killed")
    if run.exit_code not in accepted_exits:
        failures.append(
            f"child exit {run.exit_code} was not accepted ({sorted(accepted_exits)})"
        )
    if run.missing_literals:
        failures.append(f"missing expected literals: {list(run.missing_literals)!r}")
    if run.capture_bytes == 0:
        failures.append("raw capture is empty")
    if failures:
        for failure in failures:
            print(f"v050_pty_driver: {failure}", file=sys.stderr)
        return 1
    print(result_path)
    return 0


def main(argv: list[str] | None = None) -> int:
    try:
        return _main(argv)
    except HarnessError as exc:
        print(f"v050_pty_driver: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
