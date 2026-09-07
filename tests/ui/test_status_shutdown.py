"""The app-level half of the reaped-leader sweep: the tick unwinds before close.

``shutdown_sources`` cancels the status task and once relied on the event
loop's own shutdown to run that cancelled tick's ``finally`` — the only sweep
left once ``StatusRunner.aclose`` returned early on a reaped leader.
``aclose`` now sweeps the group itself (the runner-level proof is
``tests/status/test_runner.py``'s reaped-leader test), so survival no longer
depends on the loop. This file pins the second half of the repair: the
cancelled tick is *awaited* before ``self.source.close()``, so its unwind —
pipe release, runner state reset — completes before ``shutdown_sources``
returns rather than being left to whatever the loop does next. The source
double below records the status task's state at close-entry, which is the one
point where "the await ran first" is observable deterministically: without
the await, the tick is cancelled but not yet scheduled when close begins.
"""

from __future__ import annotations

import asyncio
import os
import signal
import time
from collections.abc import AsyncIterator, Callable
from contextlib import suppress
from pathlib import Path

import pytest
import pytest_asyncio

from talaria.status.contract import ProcessLimits
from talaria.status.runner import StatusRunner
from talaria.transport.attach import AttachTarget
from talaria.transport.source import FrameRecord, FrameSource, LiveSource
from talaria.ui.app import TalariaApp
from tests.transport.conftest import StubGateway
from tests.transport.test_compat_baseline import StubProvider

FAST_RETRIES = (0.0, 0.01, 0.01)


@pytest_asyncio.fixture
async def gateway() -> AsyncIterator[StubGateway]:
    stub = StubGateway(responder=lambda message, _stub: None)
    await stub.start()
    try:
        yield stub
    finally:
        await stub.stop()


class CloseProbe(FrameSource):
    """A source double that records the status task's state when close begins.

    Delegates everything to the real source. The recorded value is the ordering
    the awaited tick guarantees: ``shutdown_sources`` closes the source only
    after awaiting the cancelled status tick, so a ``True`` here is the await's
    work; a ``False`` means close began while the tick was still cancelled and
    unscheduled.
    """

    def __init__(self, inner: FrameSource, *, status_done: Callable[[], bool]) -> None:
        self.inner = inner
        self.status_done = status_done
        self.status_done_at_close: bool | None = None

    def __aiter__(self) -> AsyncIterator[FrameRecord]:
        return self.inner.__aiter__()

    async def close(self) -> None:
        self.status_done_at_close = self.status_done()
        await self.inner.close()


def _alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


@pytest.mark.asyncio
async def test_shutdown_sources_sweeps_and_unwinds_a_reaped_leaders_tick(
    gateway: StubGateway, tmp_path: Path
) -> None:
    """A reaped leader's worker dies at shutdown, and its tick is done by return.

    The status command backgrounds a worker and exits at once — the reaped
    leader shape — so at teardown the tick is parked on the worker's pipes
    with the leader already reaped. After ``shutdown_sources`` returns, with
    no yielding between it and the assertions: the worker is dead (``aclose``
    swept the group itself) and the source was closed only after the cancelled
    status tick had already unwound.
    """
    pidfile = tmp_path / "worker.pid"
    runner = StatusRunner(
        argv=["sh", "-c", f"sleep 60 & echo $! > {pidfile}; exit 0"],
        launch_cwd=tmp_path,
        limits=ProcessLimits(timeout_seconds=30.0),
    )
    source = LiveSource(
        AttachTarget.from_url(gateway.url), StubProvider(), reconnect_delays=FAST_RETRIES
    )
    probe = CloseProbe(
        source, status_done=lambda: app._status_task is not None and app._status_task.done()
    )
    app = TalariaApp(
        probe,
        mode="live",
        dispatcher=source,
        coalesce_interval=3600.0,
        status_runner=runner,
        status_interval=3600.0,
    )
    source.bind(on_connection=app.note_connection_state, on_reconnect=app.note_reconnect)

    async with app.run_test():
        await gateway.wait_for_attach()
        worker = 0
        deadline = time.monotonic() + 10.0
        while time.monotonic() < deadline:
            process = runner._process
            if (
                pidfile.exists()
                and pidfile.read_text().strip() != ""
                and process is not None
                and process.returncode is not None
            ):
                worker = int(pidfile.read_text().strip())
                break
            await asyncio.sleep(0.01)
        assert worker != 0, "the status leader never spawned and reaped"
        assert _alive(worker), "the backgrounded worker never ran"

        try:
            await app.shutdown_sources()

            gone = False
            deadline = time.monotonic() + 3.0
            while time.monotonic() < deadline:
                if not _alive(worker):
                    gone = True
                    break
                # Synchronous on purpose: no event-loop turn may run between
                # shutdown_sources and the assertions that pin its ordering.
                time.sleep(0.05)
            assert gone, (
                "a reaped leader's backgrounded worker outlived shutdown_sources"
            )
            # The discriminating assertion: it fails with the status-task await
            # removed even when everything else passes, because aclose on a
            # reaped leader never yields, so nothing between the cancel loop
            # and close() had scheduled the tick yet.
            assert probe.status_done_at_close is True, (
                "shutdown_sources closed the source before the cancelled "
                "status tick had unwound"
            )
            assert app._status_task is not None and app._status_task.done(), (
                "shutdown_sources returned with the status task not done"
            )
        finally:
            # Red-run hygiene: a failing assertion above must not leak the
            # worker this test backgrounds.
            with suppress(ProcessLookupError, PermissionError, ValueError):
                os.kill(worker, signal.SIGKILL)