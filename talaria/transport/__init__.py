"""Transport-side packages: the frame-source seam and (milestone 2) the socket.

ADR-0002 keeps I/O out of ``talaria.domain``. KTD3 puts the seam here rather
than in the domain package precisely because the live implementation owns a
socket; ``talaria.replay`` imports :mod:`talaria.transport.source` and nothing
else from this package.
"""
