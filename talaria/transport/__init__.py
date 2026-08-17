"""Transport-side packages: the frame-source seam and the live socket.

ADR-0002 keeps I/O out of ``talaria.domain``. KTD3 puts the seam here rather
than in the domain package precisely because the live implementation owns a
socket.

Five modules, in dependency order:

* :mod:`talaria.transport.source` — the ``FrameSource`` protocol plus both
  implementations' shared vocabulary, and ``LiveSource`` itself (U7).
* :mod:`talaria.transport.rpc` — request/response correlation, epoch-qualified,
  with ``unknown`` as a first-class outcome. Pure stdlib; no socket.
* :mod:`talaria.transport.credentials` — KTD11's per-dial credential chain,
  one entry per profile since v0.4's KTD5.
* :mod:`talaria.transport.attach` — the dial itself, reported as an outcome.
* :mod:`talaria.transport.connection_set` — v0.4's fleet root: one
  ``LiveSource`` per configured profile endpoint, merged into one frame stream
  whose every frame carries the connection it crossed. Live-only; the replay
  path never imports it.

``talaria.replay`` imports :mod:`talaria.transport.source` and nothing else from
this package. That module in turn imports only :mod:`talaria.transport.rpc` at
module scope; ``attach``, ``credentials``, and the ``websockets`` client library
are imported lazily inside the methods that dial. So the replay path — the one
R30 says runs the whole interface with no socket open — never loads a socket
library at all, which ``tests/transport/test_source_equivalence.py`` verifies in
a fresh interpreter rather than by inspection.
"""
