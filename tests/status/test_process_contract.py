"""R21/R22/PC2/AE1: KTD5's frozen process contract and the full failure matrix.

Every scenario yields a categorical marker and the loop is expected to
continue (the caller — not this module — is the loop; each test here proves
one tick's outcome is categorical rather than an exception).
"""

from __future__ import annotations

import asyncio
import os
import time
from pathlib import Path

from talaria.domain.projection import StatusPayload
from talaria.status.contract import ProcessLimits
from talaria.status.runner import StatusRunner
from tests.status.conftest import python_argv


def _fast_limits(**overrides: object) -> ProcessLimits:
    fields: dict[str, object] = {
        "timeout_seconds": 0.3,
        "stdout_limit_bytes": 16 * 1024,
        "stderr_limit_bytes": 4 * 1024,
        "row_limit": 8,
    }
    fields.update(overrides)
    return ProcessLimits(**fields)  # type: ignore[arg-type]


# ── Failure matrix ──────────────────────────────────────────────────────


def test_nonzero_exit_yields_categorical_marker(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    script = "import sys, json; json.load(sys.stdin); sys.exit(3)"

    async def scenario() -> None:
        runner = StatusRunner(argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits())
        result = await runner.tick(sample_payload)
        assert result.outcome == "nonzero_exit"
        assert result.exit_code == 3
        assert result.marker is not None and "failed" in result.marker
        assert result.rows == ()

    asyncio.run(scenario())


def test_hang_past_timeout_is_killed_and_marked(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    script = "import sys, json, time; json.load(sys.stdin); time.sleep(30)"

    async def scenario() -> None:
        runner = StatusRunner(argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits())
        start = time.monotonic()
        result = await runner.tick(sample_payload)
        elapsed = time.monotonic() - start
        assert result.outcome == "timeout"
        # Bounded well under the 30s sleep -- proves the kill actually fired.
        assert elapsed < 5.0
        assert result.rows == ()

    asyncio.run(scenario())


def test_overlap_at_most_one_child_ever(tmp_path: Path, sample_payload: StatusPayload) -> None:
    script = "import sys, json, time; json.load(sys.stdin); time.sleep(0.2); print('x')"

    async def scenario() -> None:
        # Not _fast_limits()'s 0.3s: this test is about the overlap guard, not
        # the timeout, and 0.3s leaves 0.1s for an entire interpreter spawn once
        # the child's own 0.2s sleep is paid. That margin holds on a developer
        # machine and loses on a loaded CI runner, where the surviving tick
        # times out and the "exactly one ran" assertion fails while the guard
        # itself is working correctly. The timeout is given room so the outcome
        # under test is the one being measured.
        runner = StatusRunner(
            argv=python_argv(script),
            launch_cwd=tmp_path,
            limits=_fast_limits(timeout_seconds=30.0),
        )
        results = await asyncio.gather(
            runner.tick(sample_payload),
            runner.tick(sample_payload),
            runner.tick(sample_payload),
        )
        assert sum(1 for r in results if r.outcome == "overlapped_skip") == 2
        assert sum(1 for r in results if r.outcome == "ok") == 1

    asyncio.run(scenario())


def test_empty_output_yields_categorical_marker(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    script = "import sys, json; json.load(sys.stdin)"

    async def scenario() -> None:
        runner = StatusRunner(argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits())
        result = await runner.tick(sample_payload)
        assert result.outcome == "empty_output"
        assert result.rows == ()

    asyncio.run(scenario())


def test_invalid_utf8_output_yields_categorical_marker(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    script = (
        "import sys, json\n"
        "json.load(sys.stdin)\n"
        "sys.stdout.buffer.write(b'\\xff\\xfe not valid utf-8\\n')\n"
    )

    async def scenario() -> None:
        runner = StatusRunner(argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits())
        result = await runner.tick(sample_payload)
        assert result.outcome == "invalid_output"
        assert result.rows == ()

    asyncio.run(scenario())


def test_output_past_the_byte_limit_truncates_visibly(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    script = "import sys, json\njson.load(sys.stdin)\nsys.stdout.write('a' * 40)\n"

    async def scenario() -> None:
        runner = StatusRunner(
            argv=python_argv(script),
            launch_cwd=tmp_path,
            limits=_fast_limits(stdout_limit_bytes=10),
        )
        result = await runner.tick(sample_payload)
        assert result.outcome == "ok"
        assert result.truncated is True
        assert sum(len(row) for row in result.rows) <= 10

    asyncio.run(scenario())


def test_truncation_boundary_mid_multibyte_character_is_not_invalid_output(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    """A byte-limit cut lands mid-codepoint here (each euro sign is 3 bytes);
    that must be tolerated as a truncation, not reported as invalid UTF-8."""
    script = (
        "import sys, json\n"
        "json.load(sys.stdin)\n"
        "sys.stdout.write('\\u20ac' * 10)\n"  # 30 bytes of '€'
    )

    async def scenario() -> None:
        runner = StatusRunner(
            argv=python_argv(script),
            launch_cwd=tmp_path,
            # 11 bytes cuts the 4th euro sign (bytes 9-11) right in half.
            limits=_fast_limits(stdout_limit_bytes=11),
        )
        result = await runner.tick(sample_payload)
        assert result.outcome == "ok"
        assert result.truncated is True

    asyncio.run(scenario())


def test_missing_executable_yields_categorical_marker(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    async def scenario() -> None:
        runner = StatusRunner(
            argv=("/definitely/not/a/real/executable-xyz",),
            launch_cwd=tmp_path,
            limits=_fast_limits(),
        )
        result = await runner.tick(sample_payload)
        assert result.outcome == "missing_executable"
        assert result.rows == ()

    asyncio.run(scenario())


# ── Process contract ─────────────────────────────────────────────────────


def test_cwd_is_the_launch_directory(tmp_path: Path, sample_payload: StatusPayload) -> None:
    marker_dir = tmp_path / "launch-here"
    marker_dir.mkdir()
    script = "import sys, json, os; json.load(sys.stdin); print(os.getcwd())"

    async def scenario() -> None:
        runner = StatusRunner(
            argv=python_argv(script), launch_cwd=marker_dir, limits=_fast_limits()
        )
        result = await runner.tick(sample_payload)
        assert result.outcome == "ok"
        assert result.rows == (str(marker_dir.resolve()),) or result.rows == (str(marker_dir),)

    asyncio.run(scenario())


def test_script_reading_stdin_to_eof_exits_without_hanging_into_timeout(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    script = "import sys, json, time; json.load(sys.stdin); print('read-to-eof-ok')"

    async def scenario() -> None:
        runner = StatusRunner(
            argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits(timeout_seconds=5.0)
        )
        start = time.monotonic()
        result = await runner.tick(sample_payload)
        elapsed = time.monotonic() - start
        assert result.outcome == "ok"
        # Well under the 5s timeout: stdin closing after the payload write is
        # what let this exit promptly instead of blocking to the deadline.
        assert elapsed < 2.0

    asyncio.run(scenario())


def test_stderr_never_appears_among_rows_only_in_the_failure_marker(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    script = (
        "import sys, json\n"
        "json.load(sys.stdin)\n"
        "print('this-is-stdout-and-should-be-a-row')\n"
        "sys.stderr.write('this-is-stderr-diagnostic\\n')\n"
        "sys.exit(1)\n"
    )

    async def scenario() -> None:
        runner = StatusRunner(argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits())
        result = await runner.tick(sample_payload)
        assert result.outcome == "nonzero_exit"
        # nonzero exit discards stdout rows entirely -- a chatty script
        # cannot impersonate status rows via stdout on a failing run either.
        assert result.rows == ()
        assert result.marker is not None
        assert "this-is-stderr-diagnostic" in result.marker

    asyncio.run(scenario())


def test_backgrounded_grandchild_is_reaped_on_timeout_kill(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    pidfile = tmp_path / "grandchild.pid"
    script = (
        "import json, subprocess, sys, time\n"
        "json.load(sys.stdin)\n"
        "gc = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])\n"
        f"open({str(pidfile)!r}, 'w').write(str(gc.pid))\n"
        "time.sleep(30)\n"
    )

    async def scenario() -> None:
        runner = StatusRunner(argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits())
        result = await runner.tick(sample_payload)
        assert result.outcome == "timeout"

        # Grandchild's pid was written before the parent (and its process
        # group) were killed.
        deadline = time.monotonic() + 3.0
        grandchild_pid = int(pidfile.read_text())
        gone = False
        while time.monotonic() < deadline:
            try:
                os.kill(grandchild_pid, 0)
            except ProcessLookupError:
                gone = True
                break
            time.sleep(0.1)
        assert gone, "grandchild survived the process-group kill"

    asyncio.run(scenario())


def test_aclose_kills_an_in_flight_child(tmp_path: Path, sample_payload: StatusPayload) -> None:
    """R36: teardown stops the status child even mid-tick."""
    script = "import sys, json, time; json.load(sys.stdin); time.sleep(30)"

    async def scenario() -> None:
        runner = StatusRunner(
            argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits(timeout_seconds=30.0)
        )
        tick_task = asyncio.create_task(runner.tick(sample_payload))
        # Let the child actually spawn before tearing down.
        await asyncio.sleep(0.2)
        await runner.aclose()
        result = await asyncio.wait_for(tick_task, timeout=2.0)
        assert result.outcome in ("timeout", "nonzero_exit")

    asyncio.run(scenario())
