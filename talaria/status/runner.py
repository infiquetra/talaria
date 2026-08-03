"""The status-line runner: KTD5's external command contract end to end.

An asyncio service that owns at most one status child at a time. Every
failure mode (nonzero exit, timeout, overlap, empty/invalid/oversize output,
a missing executable) reports a categorical :class:`StatusTickResult` rather
than raising — the failure-taxonomy style ``src/record/`` already uses
(report outcomes, never throw). No failure here ever touches the session
loop (R21).
"""

from __future__ import annotations

import asyncio
import codecs
import contextlib
import os
import signal
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

from talaria.domain.projection import StatusPayload
from talaria.status.contract import (
    TRUNCATION_MARKER,
    ProcessLimits,
    build_child_env,
    encode_payload,
)

#: One tick's categorical outcome. ``ok`` is the only non-failure member;
#: every other member has a fixed, operator-facing marker string (below) and
#: never carries rendered rows.
TickOutcome = Literal[
    "ok",
    "empty_output",
    "invalid_output",
    "nonzero_exit",
    "timeout",
    "missing_executable",
    "spawn_error",
    "overlapped_skip",
    "disabled",
]

#: Categorical, operator-facing marker text per non-``ok`` outcome (R21: "each
#: yields its categorical marker"). ``nonzero_exit`` and ``timeout`` interpolate
#: extra detail at the call site; the entries here are the fixed prefix.
_MARKERS: dict[TickOutcome, str] = {
    "empty_output": "status: no output",
    "invalid_output": "status: invalid output (not UTF-8)",
    "nonzero_exit": "status: command failed",
    "timeout": "status: command timed out",
    "missing_executable": "status: command not found",
    "spawn_error": "status: could not start command",
    "overlapped_skip": "status: previous command still running",
    "disabled": "status: no command configured",
    "ok": "",
}


@dataclass(frozen=True)
class StatusTickResult:
    """One tick's outcome. ``rows`` is non-empty only when ``outcome == "ok"``.

    Row and byte truncation are both possible on a successful tick, so
    ``truncated`` is a separate flag rather than folded into ``outcome`` —
    truncation is not a failure, it is a bounded success (R22).
    """

    outcome: TickOutcome
    rows: tuple[str, ...] = ()
    truncated: bool = False
    exit_code: int | None = None
    marker: str | None = None

    @property
    def is_failure(self) -> bool:
        return self.outcome not in ("ok", "disabled")


def _decode_stdout(raw: bytes, *, byte_truncated: bool) -> str | None:
    """Decode captured stdout, tolerating a multi-byte codepoint split at the
    16 KiB truncation boundary without tolerating genuinely invalid UTF-8.

    Returns ``None`` when the bytes are not valid UTF-8 (the ``invalid_output``
    outcome), independent of truncation.
    """
    if not byte_truncated:
        try:
            return raw.decode("utf-8")
        except UnicodeDecodeError:
            return None

    # An incremental decoder buffers an incomplete trailing sequence instead
    # of raising, but still raises on a genuinely illegal byte, which is the
    # distinction the two failure categories need: "we truncated mid-character"
    # is tolerated, "the script emitted invalid UTF-8" is not.
    decoder = codecs.getincrementaldecoder("utf-8")()
    try:
        return decoder.decode(raw, final=False)
    except UnicodeDecodeError:
        return None


class StatusRunner:
    """Owns one status child at a time under KTD5's skip-while-running policy."""

    def __init__(
        self,
        *,
        argv: Sequence[str] | None,
        launch_cwd: Path,
        limits: ProcessLimits | None = None,
        allowlist: Sequence[str] = (),
        parent_env: Mapping[str, str] | None = None,
    ) -> None:
        self._argv = tuple(argv) if argv else None
        self._launch_cwd = launch_cwd
        self._limits = limits if limits is not None else ProcessLimits()
        self._allowlist = tuple(allowlist)
        self._parent_env: Mapping[str, str] = (
            dict(parent_env) if parent_env is not None else dict(os.environ)
        )
        self._running = False
        self._process: asyncio.subprocess.Process | None = None

    @property
    def enabled(self) -> bool:
        """False when no ``status.command`` is configured (KTD5: no child ever spawns)."""
        return self._argv is not None

    async def tick(self, payload: StatusPayload) -> StatusTickResult:
        """Run exactly one tick, or skip it under KTD5's overlap policy.

        Safe to call concurrently: a second call arriving while the first is
        still awaiting the child observes ``self._running`` and returns
        ``overlapped_skip`` immediately rather than spawning a second child —
        "at most one invocation" (R21) holds because asyncio is
        single-threaded and cooperative, so the flag check and set below never
        race.
        """
        if not self.enabled:
            return StatusTickResult(outcome="disabled", marker=_MARKERS["disabled"])
        if self._running:
            return StatusTickResult(outcome="overlapped_skip", marker=_MARKERS["overlapped_skip"])

        self._running = True
        try:
            return await self._run_once(payload)
        finally:
            self._running = False

    async def _run_once(self, payload: StatusPayload) -> StatusTickResult:
        if self._argv is None:
            # Unreachable via tick() (guarded by self.enabled), but a real
            # check rather than assert: bandit (B101) rightly flags assert as
            # something -O silently removes, and this guard is a real
            # invariant, not a debug-only sanity check.
            raise RuntimeError("_run_once called with no configured argv")
        env = build_child_env(parent_env=self._parent_env, allowlist=self._allowlist)

        try:
            process = await asyncio.create_subprocess_exec(
                *self._argv,
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                cwd=str(self._launch_cwd),
                env=env,
                start_new_session=True,  # KTD5: process-group-scoped termination.
            )
        except FileNotFoundError:
            return StatusTickResult(
                outcome="missing_executable", marker=_MARKERS["missing_executable"]
            )
        except OSError as exc:
            return StatusTickResult(
                outcome="spawn_error", marker=f"{_MARKERS['spawn_error']}: {exc}"
            )

        self._process = process
        try:
            return await self._communicate(process, payload)
        finally:
            self._process = None

    async def _communicate(
        self, process: asyncio.subprocess.Process, payload: StatusPayload
    ) -> StatusTickResult:
        payload_bytes = encode_payload(payload)
        try:
            stdout, stderr = await asyncio.wait_for(
                process.communicate(payload_bytes), timeout=self._limits.timeout_seconds
            )
        except TimeoutError:
            self._kill_process_group(process)
            # Reap: the group kill also takes any backgrounded grandchild
            # holding the stdout/stderr pipes open (R36) — without the
            # process-group scope, communicate() here would hang past this
            # second wait too, because a lone-process kill leaves the pipe
            # open in the surviving grandchild.
            with contextlib.suppress(Exception):
                await asyncio.wait_for(process.communicate(), timeout=self._limits.timeout_seconds)
            return StatusTickResult(outcome="timeout", marker=_MARKERS["timeout"])

        if process.returncode != 0:
            stderr_excerpt = stderr[: self._limits.stderr_limit_bytes].decode(
                "utf-8", errors="replace"
            )
            marker = f"{_MARKERS['nonzero_exit']} (exit {process.returncode})"
            if stderr_excerpt.strip():
                # Surfaced only in the categorical marker, never among the
                # rendered rows (KTD5's process contract).
                marker = f"{marker}: {stderr_excerpt.strip().splitlines()[0]}"
            return StatusTickResult(
                outcome="nonzero_exit", exit_code=process.returncode, marker=marker
            )

        return self._parse_stdout(stdout)

    def _parse_stdout(self, raw: bytes) -> StatusTickResult:
        byte_truncated = len(raw) > self._limits.stdout_limit_bytes
        if byte_truncated:
            raw = raw[: self._limits.stdout_limit_bytes]

        text = _decode_stdout(raw, byte_truncated=byte_truncated)
        if text is None:
            return StatusTickResult(outcome="invalid_output", marker=_MARKERS["invalid_output"])

        if text == "":
            return StatusTickResult(outcome="empty_output", marker=_MARKERS["empty_output"])

        # Rows are literal text (R22: ANSI is never interpreted, only split on
        # newlines). A single trailing newline is not itself an extra empty
        # row; interior blank lines are kept literal.
        lines = text.split("\n")
        if lines and lines[-1] == "" and text.endswith("\n"):
            lines = lines[:-1]

        if not lines:
            return StatusTickResult(outcome="empty_output", marker=_MARKERS["empty_output"])

        row_truncated = len(lines) > self._limits.row_limit
        if row_truncated:
            rows = tuple(lines[: self._limits.row_limit - 1]) + (
                f"{TRUNCATION_MARKER} ({len(lines)} rows)",
            )
        else:
            rows = tuple(lines)

        return StatusTickResult(outcome="ok", rows=rows, truncated=row_truncated or byte_truncated)

    def _kill_process_group(self, process: asyncio.subprocess.Process) -> None:
        """Signal the whole process group, not just ``process`` itself.

        ``start_new_session=True`` made ``process.pid`` the new group's
        leader, so its pid doubles as the group id — this is what makes
        R36's "stops the status child" true even when the script backgrounds
        a long-lived grandchild.
        """
        with contextlib.suppress(ProcessLookupError):
            os.killpg(process.pid, signal.SIGKILL)

    async def aclose(self) -> None:
        """Idempotent teardown hook (R36): kill any in-flight child's process group.

        Safe to call whether or not a tick is in flight, and safe to call more
        than once. Talaria's own teardown path calls this unconditionally so a
        status child is never left running after Talaria exits.
        """
        process = self._process
        if process is None or process.returncode is not None:
            return
        self._kill_process_group(process)
        with contextlib.suppress(Exception):
            await asyncio.wait_for(process.wait(), timeout=self._limits.timeout_seconds)


async def run_forever(
    runner: StatusRunner,
    *,
    get_payload: Callable[[], StatusPayload],
    on_result: Callable[[StatusTickResult], None],
    interval_seconds: float,
    stop_event: asyncio.Event | None = None,
) -> None:
    """Tick ``runner`` on a fixed interval until ``stop_event`` is set.

    R40: this loop is what the replay controller's pause/resume drives
    (mutation controls stay inert; only the tick cadence is affected) and
    what the status region subscribes to for rendering. It is deliberately
    a free function, not a method on :class:`StatusRunner`, so a caller that
    wants a different scheduling policy (e.g. driven by a test clock) can
    call :meth:`StatusRunner.tick` directly instead.
    """
    stop_event = stop_event if stop_event is not None else asyncio.Event()
    while not stop_event.is_set():
        result = await runner.tick(get_payload())
        on_result(result)
        try:
            await asyncio.wait_for(stop_event.wait(), timeout=interval_seconds)
        except TimeoutError:
            continue
