"""R18: argv exec (no shell), stdin payload delivery, interval tick, overlap.

Every scenario spawns a tiny Python subprocess via ``python_argv`` (see
``conftest.py``) rather than depending on shell utilities, so the suite is
portable and each script fully controls its own behavior.
"""

from __future__ import annotations

import asyncio
from pathlib import Path

from talaria.domain.projection import StatusPayload
from talaria.status.contract import ProcessLimits
from talaria.status.runner import StatusRunner
from tests.status.conftest import python_argv


def _fast_limits(**overrides: object) -> ProcessLimits:
    fields: dict[str, object] = {
        "timeout_seconds": 2.0,
        "stdout_limit_bytes": 16 * 1024,
        "stderr_limit_bytes": 4 * 1024,
        "row_limit": 8,
    }
    fields.update(overrides)
    return ProcessLimits(**fields)  # type: ignore[arg-type]


def test_happy_path_three_rows_reach_the_projection(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    script = "import sys, json; json.load(sys.stdin); print('one'); print('two'); print('three')"

    async def scenario() -> None:
        runner = StatusRunner(argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits())
        result = await runner.tick(sample_payload)
        assert result.outcome == "ok"
        assert result.rows == ("one", "two", "three")
        assert not result.truncated

    asyncio.run(scenario())


def test_disabled_when_no_command_configured(tmp_path: Path, sample_payload: StatusPayload) -> None:
    async def scenario() -> None:
        runner = StatusRunner(argv=None, launch_cwd=tmp_path)
        assert not runner.enabled
        result = await runner.tick(sample_payload)
        assert result.outcome == "disabled"
        assert result.rows == ()

    asyncio.run(scenario())


def test_argv_is_exec_directly_no_shell(tmp_path: Path, sample_payload: StatusPayload) -> None:
    """A shell would expand ``$HOME``; direct exec must not."""

    async def scenario() -> None:
        runner = StatusRunner(
            argv=("/bin/echo", "$HOME"), launch_cwd=tmp_path, limits=_fast_limits()
        )
        result = await runner.tick(sample_payload)
        assert result.outcome == "ok"
        assert result.rows == ("$HOME",)

    asyncio.run(scenario())


def test_stdin_payload_is_the_ktd5_document(tmp_path: Path, sample_payload: StatusPayload) -> None:
    script = (
        "import sys, json\n"
        "doc = json.load(sys.stdin)\n"
        "print(doc['version'])\n"
        "print(doc['session']['id'])\n"
    )

    async def scenario() -> None:
        runner = StatusRunner(argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits())
        result = await runner.tick(sample_payload)
        assert result.outcome == "ok"
        assert result.rows == ("1", "sess-1")

    asyncio.run(scenario())


def test_ansi_escapes_arrive_literal_and_uninterpreted(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    script = "import sys, json; json.load(sys.stdin); print('\\x1b[31mred\\x1b[0m')"

    async def scenario() -> None:
        runner = StatusRunner(argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits())
        result = await runner.tick(sample_payload)
        assert result.outcome == "ok"
        assert result.rows == ("\x1b[31mred\x1b[0m",)

    asyncio.run(scenario())


def test_rows_past_the_bound_truncate_with_a_visible_marker(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    script = "import sys, json\njson.load(sys.stdin)\nfor i in range(12):\n    print(f'row-{i}')\n"

    async def scenario() -> None:
        runner = StatusRunner(
            argv=python_argv(script),
            launch_cwd=tmp_path,
            limits=_fast_limits(row_limit=8),
        )
        result = await runner.tick(sample_payload)
        assert result.outcome == "ok"
        assert len(result.rows) == 8
        assert result.rows[:7] == tuple(f"row-{i}" for i in range(7))
        assert "truncated" in result.rows[-1]
        assert result.truncated is True

    asyncio.run(scenario())


def test_overlap_second_tick_is_skipped_while_first_runs(
    tmp_path: Path, sample_payload: StatusPayload
) -> None:
    script = "import sys, json, time\njson.load(sys.stdin)\ntime.sleep(0.3)\nprint('done')\n"

    async def scenario() -> None:
        runner = StatusRunner(argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits())
        first, second = await asyncio.gather(
            runner.tick(sample_payload), runner.tick(sample_payload)
        )
        outcomes = {first.outcome, second.outcome}
        assert outcomes == {"ok", "overlapped_skip"}
        # At most one child ever ran concurrently: the overlapped call
        # produced no rows and no process was spawned for it.
        skipped = first if first.outcome == "overlapped_skip" else second
        assert skipped.rows == ()

    asyncio.run(scenario())


def test_overlap_recovers_on_the_next_tick(tmp_path: Path, sample_payload: StatusPayload) -> None:
    script = "import sys, json; json.load(sys.stdin); print('ok-row')"

    async def scenario() -> None:
        runner = StatusRunner(argv=python_argv(script), launch_cwd=tmp_path, limits=_fast_limits())
        await runner.tick(sample_payload)
        result = await runner.tick(sample_payload)
        assert result.outcome == "ok"

    asyncio.run(scenario())
