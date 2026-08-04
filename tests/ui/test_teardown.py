"""Teardown, from a normal exit and from an induced mid-stream failure (R36, F7).

Two layers, because the claim has two halves that no single harness can prove.

**On a real pseudo-terminal**, a subprocess runs the actual Talaria shell through
``tests/ui/teardown_driver.py`` and the parent reads the terminal's ``termios``
attributes before, during and after. This is the only way "the terminal was
restored" is checkable: an in-process Textual test drives a headless driver that
never touches a tty, so every screen-level restore assertion there is vacuous.
:func:`test_the_terminal_restore_assertion_can_fail` is the control that shows
the assertion is falsifiable — the same run killed with ``SIGKILL`` leaves the
terminal in raw mode, and the test asserts exactly that.

**In process, against a real loopback stub gateway**, the rest: Talaria-owned
tasks stopped, in-flight calls resolved rather than left hanging, the status
child's whole process group gone, and — F7's clause, the one that makes Talaria a
client rather than a supervisor — the gateway process still running and still
serving after Talaria exits.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import os
import pty
import signal
import subprocess
import sys
import termios
import threading
import time
from collections.abc import AsyncIterator, Callable
from pathlib import Path
from typing import Any

import pytest
import pytest_asyncio

from talaria.domain.projection import status_payload
from talaria.domain.startup import resolve_startup
from talaria.status.contract import ProcessLimits
from talaria.status.runner import StatusRunner
from talaria.transport.attach import AttachTarget
from talaria.transport.source import FrameRecord, LiveSource
from talaria.ui.app import STREAM_FAILED, STREAM_FAILURE_EXIT_CODE, TalariaApp
from tests.transport.conftest import READY_FRAME, STUB_TOKEN, StubGateway, event
from tests.transport.test_compat_baseline import StubProvider

REPO_ROOT = Path(__file__).resolve().parents[2]
FAST_RETRIES = (0.0, 0.01, 0.01)

#: How long a pseudo-terminal run is given to reach raw mode and then to exit.
#: Generous: the assertion is about what state is left behind, and a timeout
#: that fires early would report a slow machine as an unrestored terminal.
REACH_RAW_SECONDS = 20.0
EXIT_SECONDS = 40.0


# ── the pseudo-terminal harness ──────────────────────────────────────────


def write_corpus(path: Path, *, frames: int = 40) -> Path:
    """A frame-log v1 corpus, generated rather than committed (R29)."""
    lines = [
        json.dumps(
            {
                "kind": "header",
                "version": 1,
                "startedAt": "2026-08-03T00:00:00.000Z",
                "endpoint": "ws://127.0.0.1:9119/api/ws",
            }
        )
    ]
    for index in range(frames):
        lines.append(
            json.dumps(
                {
                    "kind": "frame",
                    "seq": index + 1,
                    "at": f"2026-08-03T00:00:{index % 60:02d}.000Z",
                    "dir": "in",
                    "frame": {
                        "jsonrpc": "2.0",
                        "method": "event",
                        "params": {
                            "type": "message.delta",
                            "session_id": "s1",
                            "payload": {"text": f"line {index}\n"},
                        },
                    },
                }
            )
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


class PtyRun:
    """One driver run on a pseudo-terminal, with the master drained.

    The drain thread is not a convenience. A pseudo-terminal's buffer is a few
    kilobytes; an undrained master blocks the child on its first full screen
    paint, after which it never reads a keystroke and never exits. That looked
    exactly like a hung application.
    """

    def __init__(self, mode: str, workdir: Path) -> None:
        self.mode = mode
        self.workdir = workdir
        self.output = bytearray()
        self._stop = threading.Event()
        self.master, slave = pty.openpty()
        self.before = termios.tcgetattr(self.master)
        env = dict(os.environ)
        env.update(
            TERM="xterm-256color",
            COLUMNS="100",
            LINES="30",
            PYTHONPATH=str(REPO_ROOT),
            # KTD15's global level, isolated the way tests/conftest.py isolates
            # it in process: this run is a *subprocess* and does not inherit the
            # autouse fixture's monkeypatching.
            TALARIA_CONFIG_DIR=str(workdir / "config"),
        )
        (workdir / "config").mkdir(exist_ok=True)
        self.process = subprocess.Popen(
            [sys.executable, "-m", "tests.ui.teardown_driver", mode, str(workdir)],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=env,
            cwd=str(REPO_ROOT),
            close_fds=True,
        )
        os.close(slave)
        self._drain = threading.Thread(target=self._drain_master, daemon=True)
        self._drain.start()

    def _drain_master(self) -> None:
        while not self._stop.is_set():
            try:
                chunk = os.read(self.master, 65536)
            except OSError:
                return
            if not chunk:
                return
            self.output.extend(chunk)

    def wait_for_raw_mode(self) -> bool:
        """Whether the app put the terminal into raw mode. The positive half."""
        deadline = time.monotonic() + REACH_RAW_SECONDS
        while time.monotonic() < deadline:
            if not termios.tcgetattr(self.master)[3] & termios.ECHO:
                return True
            time.sleep(0.02)
        return False

    def wait_for_file(self, name: str) -> Path | None:
        deadline = time.monotonic() + REACH_RAW_SECONDS
        path = self.workdir / name
        while time.monotonic() < deadline:
            if path.exists() and path.read_text(encoding="utf-8").strip():
                return path
            time.sleep(0.02)
        return None

    def send_quit(self) -> None:
        os.write(self.master, b"\x11")  # ctrl+q

    def wait(self) -> int:
        try:
            return self.process.wait(timeout=EXIT_SECONDS)
        except subprocess.TimeoutExpired:  # pragma: no cover - a hung app
            self.process.kill()
            self.process.wait()
            raise

    def terminal_restored(self) -> bool:
        # A short settle: the child's final ``tcsetattr`` and the parent's read
        # are in different processes, so the exit and the restore are not
        # ordered with respect to each other from here.
        time.sleep(0.4)
        return termios.tcgetattr(self.master) == self.before

    def close(self) -> None:
        self._stop.set()
        if self.process.poll() is None:  # pragma: no cover - cleanup path
            self.process.kill()
            self.process.wait()
        os.close(self.master)


def alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:  # pragma: no cover - a recycled pid owned elsewhere
        return True
    return True


def read_pid(path: Path) -> int:
    return int(path.read_text(encoding="utf-8").strip())


@pytest.fixture
def workdir(tmp_path: Path) -> Path:
    directory = tmp_path / "run"
    directory.mkdir()
    write_corpus(directory / "corpus.jsonl")
    return directory


# ── normal exit, on a real terminal ──────────────────────────────────────


def test_a_normal_exit_restores_the_terminal_modes(workdir: Path) -> None:
    """R36's first clause, measured on a pseudo-terminal rather than asserted.

    Three observations in one test, because the interesting one is only
    meaningful beside the other two: the app *drew* something, it *did* put the
    terminal into raw mode, and the attributes afterwards are byte-identical to
    the ones before. Without the middle observation, a client that never touched
    the terminal at all would pass.
    """
    run = PtyRun("normal", workdir)
    try:
        assert run.wait_for_raw_mode(), "the app never put the terminal into raw mode"
        assert run.wait_for_file("status-child.pid") is not None
        run.send_quit()
        assert run.wait() == 0
        assert len(run.output) > 2000, "the app exited without drawing an interface"
        assert run.terminal_restored(), (
            "the terminal was left in the mode the application set"
        )
    finally:
        run.close()


def test_the_terminal_restore_assertion_can_fail(workdir: Path) -> None:
    """The control. Same run, killed instead of quit — and the terminal stays raw.

    This exists because the restore assertion above is the kind that passes for
    the wrong reason: if the pseudo-terminal reset itself when the last writer
    closed, or if the app never changed anything, the test would be green and
    empty. It is green here only because *this* case is red.
    """
    run = PtyRun("normal", workdir)
    try:
        assert run.wait_for_raw_mode()
        run.process.send_signal(signal.SIGKILL)
        assert run.wait() == -signal.SIGKILL
        assert not run.terminal_restored(), (
            "a SIGKILLed application left the terminal restored, so the "
            "restore assertion above proves nothing"
        )
    finally:
        run.close()


def test_a_normal_exit_leaves_no_status_child_or_grandchild(workdir: Path) -> None:
    """R36's "stops the status child", including what the child backgrounded.

    The status command writes its own pid and backgrounds a ten-minute sleep,
    so both are demonstrably alive while Talaria runs — that is the positive
    half — and both must be gone once it has exited.
    """
    run = PtyRun("normal", workdir)
    try:
        assert run.wait_for_file("status-child.pid") is not None
        grandchild_file = run.wait_for_file("grandchild.pid")
        assert grandchild_file is not None
        grandchild = read_pid(grandchild_file)
        assert alive(grandchild), "the backgrounded grandchild was never running"

        run.send_quit()
        assert run.wait() == 0
        time.sleep(0.5)

        assert not alive(read_pid(workdir / "status-child.pid")), (
            "the status child outlived Talaria"
        )
        assert not alive(grandchild), (
            "a process the status command backgrounded outlived Talaria"
        )
    finally:
        run.close()


# ── induced mid-stream failure, on a real terminal ───────────────────────


def test_an_induced_mid_stream_failure_still_restores_the_terminal(workdir: Path) -> None:
    """The failure path reaches the same teardown as the ordinary exit.

    The driver's source raises after two frames and nothing in the driver
    catches it: :meth:`~talaria.ui.app.TalariaApp._pump` is what turns a failed
    stream into an orderly shutdown. Before that clause existed the exception
    died inside an unretrieved task, the interface stayed up over a stream that
    had ended, and this test hung until its own timeout.
    """
    run = PtyRun("crash", workdir)
    try:
        assert run.wait_for_raw_mode(), "the app never started"
        assert run.wait() == STREAM_FAILURE_EXIT_CODE
        assert run.terminal_restored(), (
            "a failed stream left the terminal in raw mode"
        )
    finally:
        run.close()


def test_an_induced_mid_stream_failure_still_stops_the_status_child(
    workdir: Path,
) -> None:
    run = PtyRun("crash", workdir)
    try:
        grandchild_file = run.wait_for_file("grandchild.pid")
        assert grandchild_file is not None
        grandchild = read_pid(grandchild_file)
        assert alive(grandchild)

        assert run.wait() == STREAM_FAILURE_EXIT_CODE
        time.sleep(0.5)
        assert not alive(grandchild), (
            "a crashed Talaria left a status grandchild running"
        )
    finally:
        run.close()


# ── in process: the gateway survives, and nothing is left waiting ────────


@pytest_asyncio.fixture
async def gateway() -> AsyncIterator[StubGateway]:
    stub = StubGateway(responder=lambda message, _stub: None)
    await stub.start()
    try:
        yield stub
    finally:
        await stub.stop()


def live_app(gateway: StubGateway, **kwargs: Any) -> tuple[TalariaApp, LiveSource]:
    source = LiveSource(
        AttachTarget.from_url(gateway.url),
        StubProvider(),
        reconnect_delays=FAST_RETRIES,
    )
    app = TalariaApp(
        source, mode="live", dispatcher=source, coalesce_interval=3600.0, **kwargs
    )
    source.bind(on_connection=app.note_connection_state, on_reconnect=app.note_reconnect)
    return app, source


async def until(predicate: Callable[[], bool], *, timeout: float = 10.0) -> None:
    async def _poll() -> None:
        while not predicate():
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_poll(), timeout=timeout)


@pytest.mark.asyncio
async def test_the_gateway_is_still_serving_after_talaria_exits(
    gateway: StubGateway,
) -> None:
    """F7's clause: Talaria dials a gateway it did not launch, and does not stop it.

    Proved by dialling again *after* the first source is torn down and receiving
    the greeting on a second connection. Asserting that the stub's task is still
    alive would test the fixture; asking it for a new session tests the server.
    """
    app, source = live_app(gateway)
    async with app.run_test():
        await gateway.wait_for_attach()
        await until(lambda: source.frames_received >= 1)
        await app.shutdown_sources()

    assert source.closed

    second = LiveSource(
        AttachTarget.from_url(gateway.url), StubProvider(), reconnect_delays=FAST_RETRIES
    )
    try:
        assert await second.start() == "connected", "the gateway stopped when Talaria did"
        greeting = await asyncio.wait_for(second.__aiter__().__anext__(), timeout=5.0)
        assert isinstance(greeting, FrameRecord)
        assert greeting.frame == READY_FRAME
        assert len(gateway.sessions) == 2
    finally:
        await second.close()


@pytest.mark.asyncio
async def test_the_gateway_process_survives_a_talaria_that_shares_its_process_group() -> None:
    """F7 at process granularity, which the in-process test above cannot see.

    The test above proves Talaria closed only its own socket. It cannot prove
    anything about processes, because there is only one: a teardown that
    signalled a process group, or killed something it wrongly believed it had
    launched, would leave that in-process server object untouched and the test
    green. Talaria *does* signal process groups — that is how KTD5 stops a
    status child — so "it aims that only at its own child" is a claim worth
    measuring rather than reasoning about.

    The gateway therefore runs as a separate OS process here, deliberately left
    in **Talaria's own process group** (no ``start_new_session``), so a
    mis-aimed group signal would take it down. Three observations after
    teardown, in order of strength: the process is alive, it is still *serving*
    (a second client dials it and receives the greeting), and it recorded two
    sessions rather than one.

    Talaria itself is torn down in process here rather than exiting as a
    process; the pseudo-terminal tests at the top of this file are what cover a
    real process exit. What this adds is the other side of the pair.
    """
    server = subprocess.Popen(
        [sys.executable, "-m", "tests.transport.stub_gateway_main"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        cwd=str(REPO_ROOT),
        env={**os.environ, "PYTHONPATH": str(REPO_ROOT)},
    )
    try:
        assert server.stdout is not None
        url = await asyncio.get_running_loop().run_in_executor(None, server.stdout.readline)
        url = url.strip()
        assert url.startswith("ws://127.0.0.1:"), f"the stub never announced a URL: {url!r}"

        source = LiveSource(
            AttachTarget.from_url(url), StubProvider(), reconnect_delays=FAST_RETRIES
        )
        app = TalariaApp(
            source, mode="live", dispatcher=source, coalesce_interval=3600.0
        )
        source.bind(
            on_connection=app.note_connection_state, on_reconnect=app.note_reconnect
        )
        async with app.run_test():
            await until(lambda: source.frames_received >= 1)
            await app.shutdown_sources()
        assert source.closed

        await asyncio.sleep(0.3)
        assert server.poll() is None, (
            f"the gateway process died when Talaria did (exit {server.returncode})"
        )

        second = LiveSource(
            AttachTarget.from_url(url), StubProvider(), reconnect_delays=FAST_RETRIES
        )
        try:
            assert await second.start() == "connected", (
                "the gateway process is alive but no longer accepting connections"
            )
            greeting = await asyncio.wait_for(second.__aiter__().__anext__(), timeout=5.0)
            assert isinstance(greeting, FrameRecord)
            assert greeting.frame == READY_FRAME
        finally:
            await second.close()
    finally:
        server.kill()
        server.wait()


@pytest.mark.asyncio
async def test_a_call_in_flight_at_teardown_resolves_instead_of_hanging(
    gateway: StubGateway,
) -> None:
    """"Local waiters resolved" (R36), against a gateway that never answers.

    The stub's responder returns ``None`` for everything, so this call has no
    reply coming. Teardown must resolve it — as ``unknown``, never as a
    success — rather than leave a coroutine awaiting a future nothing will set.
    """
    app, source = live_app(gateway)
    async with app.run_test():
        await gateway.wait_for_attach()
        # The server accepting and the client finishing its handshake are two
        # events. Calling on the first one returns ``not connected`` without
        # writing anything, and the assertion below would then be about the
        # wrong outcome entirely.
        await until(lambda: source.connected)
        pending = asyncio.create_task(source.call("agents.list", {}))
        # Waited for on the *server*, not on the correlator's in-flight count.
        # A live app fetches the command catalogue at connect, so that count
        # reaches one without this call ever having been minted — and teardown
        # would then resolve it ``not connected`` instead of abandoning it,
        # which is a different code path answering a different question.
        await gateway.wait_for_request("agents.list")

        await app.shutdown_sources()

        outcome = await asyncio.wait_for(pending, timeout=5.0)
        assert outcome.status == "unknown", "a lost call was reported as an answer"
        assert outcome.reason == "the transport was closed"
        assert source.correlator.in_flight == 0


class SilentDispatcher:
    """Accepts a call and never answers it, and never raises.

    Not the same as the stub gateway's silent responder: that one is silent on
    the *wire*, and closing the transport underneath it resolves the waiter.
    This one holds the coroutine open regardless of what the transport does, so
    a task awaiting it ends only if somebody cancels it.
    """

    async def call(self, method: str, params: Any = None, *, timeout: Any = None) -> Any:
        await asyncio.Event().wait()
        raise AssertionError("unreachable")  # pragma: no cover


@pytest.mark.asyncio
async def test_teardown_stops_every_task_talaria_started(gateway: StubGateway) -> None:
    """The pump, the status loop, the catalogue fetch **and** the startup sequence.

    Collected before the shutdown and inspected after, because a task set read
    afterwards is empty whether teardown cancelled them or nothing ever ran.

    The startup sequence is the reason this app is given a
    :class:`~talaria.domain.startup.StartupSelection`. An earlier version of this
    test named four tasks in its docstring and collected three: the app was built
    without a selection, so ``begin_live_startup`` returned immediately,
    ``_startup_task`` was always ``None``, and removing it from
    ``shutdown_sources``' cancel set left the whole suite green. All four are
    now required to exist, by name, before anything is torn down.

    **The dispatcher is a double that never answers, and that is what makes the
    cancellation observable.** Against the real
    :class:`~talaria.transport.source.LiveSource`, closing the transport resolves
    every call in flight as ``unknown`` — so the startup task and the catalogue
    task finish *by themselves* a moment after teardown, and removing them from
    the cancel set still left this test green. A dispatcher that returns nothing
    ever separates "teardown cancelled it" from "it happened to end".
    """
    runner = StatusRunner(
        argv=["sh", "-c", "echo one-status-line"],
        launch_cwd=Path.cwd(),
        limits=ProcessLimits(timeout_seconds=10.0),
    )
    app, _source = live_app(
        gateway,
        status_runner=runner,
        status_interval=3600.0,
        startup=resolve_startup(session="s-teardown", resume=False),
    )
    app.dispatcher = SilentDispatcher()

    async with app.run_test():
        await gateway.wait_for_attach()
        await until(lambda: app._pump_task is not None)
        # The startup sequence is kicked off from the ``connected`` callback,
        # so it appears a moment after the attach rather than at mount.
        await until(lambda: app._startup_task is not None)
        by_name = {
            "pump": app._pump_task,
            "status": app._status_task,
            "catalog": app._catalog_task,
            "startup": app._startup_task,
        }
        missing = sorted(name for name, task in by_name.items() if task is None)
        assert not missing, f"these tasks never started, so nothing was proved: {missing}"
        started = [task for task in by_name.values() if task is not None]
        assert not any(task.done() for task in started), (
            "a task had already finished before teardown, so its cancellation "
            "would not have been observable here"
        )

        await app.shutdown_sources()
        await asyncio.sleep(0.05)

        still_running = sorted(
            name for name, task in by_name.items() if task is not None and not task.done()
        )
        assert not still_running, f"teardown left a Talaria-owned task running: {still_running}"


@pytest.mark.asyncio
async def test_teardown_stops_a_status_child_this_app_does_not_own(
    gateway: StubGateway, tmp_path: Path
) -> None:
    """What ``shutdown_sources``' ``status_runner.aclose()`` uniquely covers.

    Cancelling ``_status_task`` already sweeps the child's process group — the
    cancellation unwinds into ``StatusRunner._run_once``'s ``finally``, which
    kills the group whatever the leader's state — so removing the ``aclose()``
    call leaves the pty teardown tests green. That made it an unpinned line
    under a comment claiming it was the only thing standing between R36 and a
    leaked process, which it is not.

    This is the case cancellation cannot reach: a tick driven by a task the app
    does not hold. ``aclose()`` is the runner's own teardown contract and does
    not care who started the tick, so it sweeps this child; ``_status_task``
    knows nothing about it.
    """
    marker = tmp_path / "foreign-child.pid"
    runner = StatusRunner(
        argv=["sh", "-c", f'echo $$ > "{marker}"; sleep 900'],
        launch_cwd=Path.cwd(),
        limits=ProcessLimits(timeout_seconds=60.0),
    )
    app, _source = live_app(gateway)

    async with app.run_test():
        app.status_runner = runner
        payload = status_payload(app.state, mode="live")
        foreign = asyncio.create_task(runner.tick(payload))
        try:
            await until(lambda: marker.exists() and marker.read_text().strip() != "")
            child = read_pid(marker)
            assert alive(child), "the foreign status child never ran"
            assert app._status_task is None, (
                "this app owns a status task after all, so the tick below is not "
                "foreign and this test proves nothing about aclose()"
            )

            await app.shutdown_sources()
            await asyncio.sleep(0.3)

            assert not alive(child), (
                "a status child started outside the app's own status task "
                "outlived teardown"
            )
        finally:
            foreign.cancel()
            with contextlib.suppress(asyncio.CancelledError, Exception):
                await foreign


@pytest.mark.asyncio
async def test_teardown_is_idempotent(gateway: StubGateway) -> None:
    """Textual calls ``on_unmount`` on the way out and tests call it explicitly.

    A second pass must not raise — closing an already-closed socket is the
    ordinary case at exit, not an incident.
    """
    app, source = live_app(gateway)
    async with app.run_test():
        await gateway.wait_for_attach()
        await app.shutdown_sources()
        await app.shutdown_sources()
        await app.shutdown_sources()
    assert source.closed
    assert source.close_errors == 0


# ── the in-process half of the induced failure ───────────────────────────


class BreakingSource:
    """Streams two frames, then raises. Records whether it was closed."""

    def __init__(self) -> None:
        self.closed = False
        self.emitted = 0

    async def __aiter__(self) -> AsyncIterator[FrameRecord]:
        for seq in (1, 2):
            self.emitted += 1
            yield FrameRecord(
                seq=seq,
                at=float(seq),
                direction="in",
                frame=event("message.delta", {"text": f"line {seq}\n"}),
            )
            await asyncio.sleep(0)
        raise RuntimeError("induced mid-stream frame source failure")

    async def close(self) -> None:
        self.closed = True


@pytest.mark.asyncio
async def test_a_failed_stream_is_named_and_closes_the_source() -> None:
    """The failure is reported, the source is closed, and the app asks to exit.

    Paired with the count of frames that arrived first: a source that raised
    before yielding anything would satisfy every other assertion here and would
    be testing the mount path instead of a mid-stream failure.
    """
    source = BreakingSource()
    app = TalariaApp(source, mode="replay", coalesce_interval=3600.0)

    async with app.run_test():
        await until(lambda: bool(app.stream_failure))
        assert app.frames_applied == 2, "the stream failed before it ever streamed"
        assert "induced mid-stream frame source failure" in app.stream_failure
        assert any(entry.text.startswith(STREAM_FAILED) for entry in app.state.transcript)
        await until(lambda: source.closed)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_ordinary_end_of_corpus_is_not_reported_as_a_failure() -> None:
    """Pairs the test above. A corpus that simply ends is the common case, and
    a client that announced "the frame stream failed" every time a replay
    finished would train the operator to ignore the line that matters."""

    class EndingSource:
        def __init__(self) -> None:
            self.closed = False

        async def __aiter__(self) -> AsyncIterator[FrameRecord]:
            yield FrameRecord(seq=1, at=1.0, direction="in", frame=event("message.delta", {}))

        async def close(self) -> None:
            self.closed = True

    source = EndingSource()
    app = TalariaApp(source, mode="replay", coalesce_interval=3600.0)
    async with app.run_test():
        await asyncio.wait_for(app.replay_complete.wait(), timeout=5.0)
        assert app.stream_failure == ""
        assert not any(
            entry.text.startswith(STREAM_FAILED) for entry in app.state.transcript
        )
        await app.shutdown_sources()


def test_the_pty_driver_dials_nothing_and_carries_no_credential() -> None:
    """A guard on this file's own hygiene, not on the product.

    The pseudo-terminal runs inherit the developer's real environment — that is
    the point of running them as subprocesses — so a driver that grew a live
    transport would dial whatever ``TALARIA_GATEWAY_URL`` resolves to on the
    machine running the suite. Checked by the names it *constructs* rather than
    by a substring sweep of the whole file, so the module docstring may keep
    explaining what it is not.
    """
    driver = (REPO_ROOT / "tests" / "ui" / "teardown_driver.py").read_text(encoding="utf-8")
    for constructed in ("LiveSource(", "AttachTarget", "TokenProvider", "websockets"):
        assert constructed not in driver, f"the pty driver reaches for {constructed}"
    assert STUB_TOKEN not in driver
    assert "ReplaySource.from_path" in driver, "the driver stopped replaying a corpus"
