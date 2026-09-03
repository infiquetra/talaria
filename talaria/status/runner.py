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
    ScriptDocumentError,
    ScriptRow,
    build_child_env,
    encode_payload,
    parse_script_rows,
)

#: One tick's categorical outcome. ``ok`` is the only non-failure member;
#: every other member has a fixed, operator-facing marker string (below) and
#: never carries rendered rows.
TickOutcome = Literal[
    "ok",
    "empty_output",
    "invalid_output",
    "invalid_document",
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
#: Read granularity for the capped output reads. Small enough that the ceiling
#: is honoured closely, large enough that an ordinary status line is one read.
_READ_CHUNK_BYTES = 8192

_MARKERS: dict[TickOutcome, str] = {
    "empty_output": "status: no output",
    "invalid_output": "status: invalid output (not UTF-8)",
    "invalid_document": "status: invalid script document",
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

    ``script_rows`` carries a version-2 script document's rows, and only
    then: it is a tuple (possibly empty — an explicit ``"rows": []``
    clears the bar) when this tick's stdout was a valid v2 document, and
    ``None`` for every other tick, including v1 plain-text ticks and all
    failures. The bar replaces its script rows only on a non-``None``
    tick and otherwise keeps its previous good render; the in-body
    region never renders ``script_rows`` (ownership, not z-order).
    """

    outcome: TickOutcome
    rows: tuple[str, ...] = ()
    truncated: bool = False
    exit_code: int | None = None
    marker: str | None = None
    script_rows: tuple[ScriptRow, ...] | None = None

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
        #: Set once a spawn has settled — the child is recorded in
        #: ``_process``, or the spawn failed and there is nothing to record.
        #: ``None`` before the first spawn. :meth:`aclose` waits on it, because
        #: a child that exists but is not yet recorded is a child nothing sweeps.
        self._spawn_settled: asyncio.Event | None = None

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

        # ``create_subprocess_exec`` forks and execs the child, then *keeps
        # awaiting* while the subprocess transport is wired up. The child is
        # already running during that tail — it can do real work, write files,
        # and be seen by anything watching — while ``self._process`` is still
        # ``None``. A teardown landing in that window used to find nothing to
        # kill and return, leaving the child behind (R36). This event lets
        # :meth:`aclose` wait for the spawn to settle rather than conclude from
        # a ``None`` that no child exists.
        spawn_settled = asyncio.Event()
        self._spawn_settled = spawn_settled
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
            # Recorded here rather than after the ``except`` chain, so that
            # "the event is set" and "the process is recorded" cannot be
            # observed out of order.
            self._process = process
        except FileNotFoundError:
            return StatusTickResult(
                outcome="missing_executable", marker=_MARKERS["missing_executable"]
            )
        except OSError as exc:
            return StatusTickResult(
                outcome="spawn_error", marker=f"{_MARKERS['spawn_error']}: {exc}"
            )
        except (ValueError, TypeError) as exc:
            # Not every bad argv reaches the kernel. An embedded NUL raises
            # ValueError and a non-string element raises TypeError, both before
            # any OSError is possible, and both would escape tick() — which this
            # module's contract says never happens (R21: no status failure
            # touches the session loop).
            return StatusTickResult(
                outcome="spawn_error", marker=f"{_MARKERS['spawn_error']}: {exc}"
            )
        finally:
            # Every path out of the spawn settles it, including the three
            # failures above — an ``aclose`` waiting here must not hang because
            # the executable was missing.
            spawn_settled.set()

        try:
            return await self._communicate(process, payload)
        finally:
            # Unconditional, not just on the timeout path. Two things depend on
            # it. A command that backgrounds a worker with its pipes redirected
            # ("worker & echo ok") exits 0 immediately, so every non-timeout
            # path used to return leaving that worker running — one leaked
            # process per tick, forever, reparented to init. And because this
            # block runs when the tick is cancelled, Talaria's own shutdown
            # cannot leave a status child behind even if nothing calls aclose().
            # start_new_session=True at spawn is what makes the group the right
            # unit to kill: the child leads it, so this reaches its descendants.
            #
            # **Not guarded on returncode**, and that is a correction. The guard
            # that used to be here — sweep only while the leader is unreaped —
            # was written against pid recycling, and it silently disabled the
            # sweep for the one command shape the paragraph above says it exists
            # for. ``worker & echo ok`` exits the *leader* immediately while the
            # worker keeps the pipes open, so asyncio's child watcher reaps the
            # leader within milliseconds and the reads then block until the
            # timeout. By the time this block runs, ``returncode`` is 0 and the
            # sweep was skipped: measured at one surviving ``sleep`` per tick,
            # exactly the leak the comment claimed to have fixed.
            #
            # The recycling risk the guard was written for is real and is
            # accepted here, because it is bounded and the leak was not. This
            # runs inside one tick — at most ``timeout_seconds`` after the
            # leader exited — and ``self._process`` is cleared below, so
            # :meth:`aclose` can only ever signal a group whose tick is still in
            # flight. For the pid to have been recycled inside that window the
            # machine would have to allocate every pid in its range within a
            # couple of seconds, and the *certain* alternative is a leaked
            # process on every tick for as long as Talaria runs.
            self._kill_process_group(process)
            if process.returncode is None:
                with contextlib.suppress(Exception):
                    await asyncio.wait_for(process.wait(), timeout=self._limits.timeout_seconds)
            self._release_pipes(process)
            self._process = None

    async def _communicate(
        self, process: asyncio.subprocess.Process, payload: StatusPayload
    ) -> StatusTickResult:
        payload_bytes = encode_payload(payload)
        try:
            stdout, stderr, overflowed = await asyncio.wait_for(
                self._pump(process, payload_bytes), timeout=self._limits.timeout_seconds
            )
        except TimeoutError:
            # Teardown is handled unconditionally by _run_once's finally block,
            # which also reaps the group holding these pipes open. This path
            # used to run a second communicate() here, which both doubled the
            # configured timeout budget and leaked two file descriptors per
            # tick whenever the group kill did not free the pipes.
            return StatusTickResult(outcome="timeout", marker=_MARKERS["timeout"])

        if overflowed:
            # The child was cut off mid-stream once it passed the byte cap, so
            # its exit status is this runner's doing and says nothing about the
            # command. Oversize output is a bounded success (R22), so it is
            # reported as one rather than as the SIGKILL that stopped it.
            return self._parse_stdout(stdout)

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

        # A version-2 script document is detected on the whole decoded text
        # before any line splitting: a pretty-printed document spans lines,
        # and splitting first would mangle it into v1 rows. Byte-truncated
        # output skips this branch — a cut mid-document no longer parses,
        # and the v1 literal path below is the byte-identical fallback
        # rather than a guess about what the cut removed.
        if not byte_truncated:
            try:
                script_rows = parse_script_rows(text)
            except ScriptDocumentError as exc:
                return StatusTickResult(
                    outcome="invalid_document",
                    marker=f"{_MARKERS['invalid_document']}: {exc}",
                )
            if script_rows is not None:
                return self._script_rows_result(script_rows)

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

    def _script_rows_result(self, script_rows: tuple[ScriptRow, ...]) -> StatusTickResult:
        """Bound a version-2 document's rows the same way v1 text rows bind.

        The row limit and the visible truncation marker are the runner's,
        not the document's: a script cannot opt out of the bound by
        choosing a richer output shape. ``rows`` stays empty — the in-body
        region renders nothing for a v2 tick; the bar owns these rows.
        """
        row_truncated = len(script_rows) > self._limits.row_limit
        if row_truncated:
            bounded = tuple(script_rows[: self._limits.row_limit - 1]) + (
                ScriptRow(text=f"{TRUNCATION_MARKER} ({len(script_rows)} rows)"),
            )
        else:
            bounded = script_rows
        return StatusTickResult(outcome="ok", truncated=row_truncated, script_rows=bounded)

    async def _pump(
        self, process: asyncio.subprocess.Process, payload_bytes: bytes
    ) -> tuple[bytes, bytes, bool]:
        """Feed stdin and read both output streams under a hard byte ceiling.

        This exists because ``Process.communicate()`` reads until EOF. The byte
        limits were applied afterwards, when slicing what had already been read,
        so they bounded what Talaria *displayed* and not what it *held*: a
        command that floods stdout drove resident memory from 28 MB to over
        3 GB inside a single two-second tick, and it recurs every tick. The
        limits now bound the read itself.

        Reads one byte past the limit so that ``len(raw) > limit`` still
        distinguishes "exactly at the cap" from "truncated", which is the test
        the display path already applies. Once a stream is over its cap there is
        nothing further to learn from it, so the group is killed rather than
        drained — draining would be unbounded in time on an endless writer.
        """

        async def feed() -> None:
            stdin = process.stdin
            if stdin is None:
                return
            # A child that exits without reading its payload makes this a
            # broken pipe. That is an ordinary outcome, not a failure.
            with contextlib.suppress(BrokenPipeError, ConnectionResetError):
                stdin.write(payload_bytes)
                await stdin.drain()
            with contextlib.suppress(BrokenPipeError, ConnectionResetError):
                stdin.close()

        async def read_capped(stream: asyncio.StreamReader | None, limit: int) -> bytes:
            if stream is None:
                return b""
            chunks: list[bytes] = []
            seen = 0
            while seen <= limit:
                chunk = await stream.read(min(_READ_CHUNK_BYTES, limit + 1 - seen))
                if not chunk:
                    break
                chunks.append(chunk)
                seen += len(chunk)
            if seen > limit:
                # Kill here, the moment the cap is crossed, rather than after
                # both reads finish. The two reads run concurrently and the
                # caller needs both: if this one stops reading a flooding stdout
                # while the child still holds stderr open, the child blocks on a
                # full stdout pipe, stderr never reaches EOF, and the tick
                # deadlocks into a timeout instead of reporting the bounded
                # success it actually has. Killing the group ends the other read
                # promptly.
                self._kill_process_group(process)
            return b"".join(chunks)

        stdout, stderr, _ = await asyncio.gather(
            read_capped(process.stdout, self._limits.stdout_limit_bytes),
            read_capped(process.stderr, self._limits.stderr_limit_bytes),
            feed(),
        )
        overflowed = (
            len(stdout) > self._limits.stdout_limit_bytes
            or len(stderr) > self._limits.stderr_limit_bytes
        )
        if not overflowed:
            # Both streams reached EOF within budget, which means every writer
            # closed them — the child itself has exited and is now a zombie
            # holding its true exit status. Sweep the group *before* reaping,
            # never after: while the child is unreaped its pid cannot be
            # recycled, so the group id is provably still ours. A worker the
            # command backgrounded with its pipes redirected is what this
            # catches; SIGKILL to the zombie itself is a no-op and does not
            # disturb the exit status read below.
            self._kill_process_group(process)
            await process.wait()
        return stdout, stderr, overflowed

    def _release_pipes(self, process: asyncio.subprocess.Process) -> None:
        """Close the child's pipe transports rather than waiting for a collection.

        Without this, a timeout whose group kill did not free the pipes left two
        descriptors held per tick — measured at a steady 2.00 fd/tick over 100
        ticks, unaffected by an explicit ``gc.collect()``, because the event loop
        still references the transport. Reaches for the private transport
        deliberately: closing it closes stdin, stdout and stderr together, and
        asyncio exposes no public equivalent on ``Process``.
        """
        with contextlib.suppress(Exception):
            # getattr rather than attribute access: this is private asyncio
            # internals, so it is treated as something that may simply not be
            # there on another interpreter rather than as a typed attribute.
            transport = getattr(process, "_transport", None)
            if transport is not None:
                transport.close()

    def _kill_process_group(self, process: asyncio.subprocess.Process) -> None:
        """Signal the whole process group, not just ``process`` itself.

        ``start_new_session=True`` made ``process.pid`` the new group's
        leader, so its pid doubles as the group id — this is what makes
        R36's "stops the status child" true even when the script backgrounds
        a long-lived grandchild.

        PermissionError is suppressed alongside the expected
        ``ProcessLookupError``: a group whose members have all exited can leave
        this call landing on a pid the kernel has since reused, and on a
        recycled pid owned by another user the kernel answers ``EPERM`` rather
        than ``ESRCH``. Callers guard against reaching that state; this is the
        second line, and a status tick must not raise (R21).
        """
        with contextlib.suppress(ProcessLookupError, PermissionError):
            os.killpg(process.pid, signal.SIGKILL)

    async def aclose(self) -> None:
        """Idempotent teardown hook (R36): kill any in-flight child's process group.

        Safe to call whether or not a tick is in flight, and safe to call more
        than once. Talaria's own teardown path calls this unconditionally so a
        status child is never left running after Talaria exits.

        **This handles the still-running child only, and that is deliberate.**
        The reaped-leader case — a command that backgrounds a worker, exits at
        once, and leaves the worker holding the pipes — belongs to
        :meth:`_run_once`'s ``finally``, which sweeps the group whatever the
        leader's state. Widening this method to cover it as well was measured to
        add a branch nothing in the suite could reach: cancelling the status task
        runs that ``finally`` before this coroutine's first ``await`` returns, so
        the group is already gone by the time control arrives here. A guard
        nothing can exercise is a guard nobody can trust, so it is not here.

        **A spawn in flight is waited for, not treated as an absent child.**
        ``create_subprocess_exec`` forks the child and then keeps awaiting while
        the transport is set up, so there is a real window in which the child is
        running and ``self._process`` is still ``None``. Reading ``None`` and
        returning left that child alive — an R36 leak that reproduces
        deterministically if the window is widened, and that reached CI as an
        intermittent failure of
        ``tests/ui/test_teardown.py::test_teardown_stops_a_status_child_this_app_does_not_own``.
        """
        settled = self._spawn_settled
        if settled is not None and not settled.is_set():
            with contextlib.suppress(Exception):
                await asyncio.wait_for(
                    settled.wait(), timeout=self._limits.timeout_seconds
                )

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
