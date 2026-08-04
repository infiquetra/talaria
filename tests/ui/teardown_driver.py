"""A real Talaria run, on a real terminal, for the teardown assertions (R36).

Run as ``python -m tests.ui.teardown_driver <mode> <workdir>`` from the
repository root, with its standard streams attached to a pseudo-terminal. The
parent process — ``tests/ui/test_teardown.py`` — snapshots the terminal's
``termios`` attributes before, during and after, which is the only way to check
"the terminal was restored" that a mocked screen cannot fake.

**This is a test fixture and not part of the shipped client.** It exists so the
induced-failure case can be exercised against the real Textual driver without
putting a crash hook into ``talaria/``. The two modes are:

``normal``
    Replay a small corpus. The parent sends ``ctrl+q``.

``crash``
    Replay the same corpus from a source that raises part-way through. Nothing
    here catches it: :meth:`talaria.ui.app.TalariaApp._pump` is what turns a
    failed stream into an orderly shutdown, and that is the behaviour under
    test.

Both modes configure a status command that backgrounds a grandchild and records
both pids, so the parent can assert no child process outlives the run.
"""

from __future__ import annotations

import sys
from collections.abc import AsyncIterator
from pathlib import Path

from talaria.domain.commands import PasteThreshold
from talaria.replay.controls import ReplayControls
from talaria.replay.source import ReplaySource
from talaria.status.contract import ProcessLimits
from talaria.status.runner import StatusRunner
from talaria.transport.source import FrameRecord, FrameSource
from talaria.ui.app import TalariaApp

#: How long the status command's backgrounded grandchild would live if nothing
#: stopped it. Long enough that "it is gone" cannot be the child having finished.
GRANDCHILD_LIFETIME_SECONDS = 600


class FailingSource:
    """A frame source that streams, then fails — U7's ``LiveSource`` gone wrong.

    Yields real frames first so the app is genuinely running when the failure
    lands. A source that raised immediately would test the mount path rather
    than a mid-stream failure.
    """

    def __init__(self, inner: FrameSource, *, after: int) -> None:
        self.inner = inner
        self.after = after
        self.closed = False
        self.emitted = 0

    async def __aiter__(self) -> AsyncIterator[FrameRecord]:
        async for record in self.inner:
            self.emitted += 1
            yield record
            if self.emitted >= self.after:
                raise RuntimeError("induced mid-stream frame source failure")

    async def close(self) -> None:
        self.closed = True
        await self.inner.close()


def status_command(workdir: Path) -> list[str]:
    """A status command that leaves a trail: its own pid, and a grandchild's.

    ``sh -c`` rather than a Python helper so the backgrounded process is a real
    grandchild in the child's own process group, which is the arrangement
    KTD5's ``start_new_session=True`` and the runner's ``killpg`` exist for.
    """
    script = (
        f"echo $$ > {workdir / 'status-child.pid'}; "
        f"sleep {GRANDCHILD_LIFETIME_SECONDS} & "
        f"echo $! > {workdir / 'grandchild.pid'}; "
        "echo status-line-from-the-child"
    )
    return ["sh", "-c", script]


def build(mode: str, workdir: Path) -> TalariaApp:
    controls = ReplayControls(speed=8.0)
    corpus = ReplaySource.from_path(workdir / "corpus.jsonl", controls=controls)
    source: FrameSource = corpus if mode == "normal" else FailingSource(corpus, after=2)
    runner = StatusRunner(
        argv=status_command(workdir),
        launch_cwd=workdir,
        # The default two-second budget would reap the child before the parent
        # has looked at it. This makes the child outlive the observation, so
        # "it is gone afterwards" is a statement about teardown.
        limits=ProcessLimits(timeout_seconds=60.0),
    )
    return TalariaApp(
        source,
        mode="replay",
        controls=controls,
        status_runner=runner,
        status_interval=3600.0,
        paste_threshold=PasteThreshold(),
    )


def main(argv: list[str]) -> int:
    mode, workdir = argv[1], Path(argv[2])
    app = build(mode, workdir)
    app.run()
    (workdir / "exited").write_text(f"{app.return_code}\n", encoding="utf-8")
    return app.return_code or 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
