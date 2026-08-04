"""Run the loopback stub gateway as a **separate operating-system process**.

F7 says Talaria dials a gateway it did not launch and must not stop it. The
in-process version of that assertion — tear the client down, then dial the same
server object again — proves the client closed only its own socket, and it is
the cheaper test, so it stays. What it cannot see is anything that happens at
process granularity: a teardown that signalled a process group, killed a child
it thought it owned, or took the whole session down with it would leave that
in-process server object perfectly healthy, because the server *is* the test.

So this module exists to put the gateway somewhere Talaria's teardown could
actually damage it. It starts a :class:`~tests.transport.conftest.StubGateway`,
prints the URL it is listening on as the first line of standard output, and then
serves until it is killed.

Not Hermes. The bodies it answers with are the ones this repository transcribed
by hand from the pinned handlers; no Hermes gateway is involved in any test that
uses it.
"""

from __future__ import annotations

import asyncio
import sys

from tests.transport.conftest import StubGateway


async def _serve_forever() -> None:
    stub = StubGateway()
    await stub.start()
    # The port is chosen by the kernel, so the parent learns it from here rather
    # than from a number written down in two places.
    print(stub.url, flush=True)
    try:
        await asyncio.Event().wait()
    finally:  # pragma: no cover - only on an orderly signal
        await stub.stop()


if __name__ == "__main__":
    try:
        asyncio.run(_serve_forever())
    except KeyboardInterrupt:  # pragma: no cover - the parent's teardown
        sys.exit(0)
