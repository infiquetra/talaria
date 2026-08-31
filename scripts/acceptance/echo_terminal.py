#!/usr/bin/env python3
"""Small real-terminal child used only to prove the pseudo-terminal harness."""

from __future__ import annotations

import fcntl
import os
import signal
import struct
import sys
import termios
import tty


def _size() -> tuple[int, int]:
    packed = fcntl.ioctl(sys.stdin.fileno(), termios.TIOCGWINSZ, b"\0" * 8)
    rows, columns, _, _ = struct.unpack("HHHH", packed)
    return rows, columns


def _write(label: str) -> None:
    os.write(sys.stdout.fileno(), f"{label}\r\n".encode())


def _resized(_signum: int, _frame: object) -> None:
    rows, columns = _size()
    _write(f"RESIZE:{rows}x{columns}")


def main() -> int:
    tty.setraw(sys.stdin.fileno())
    signal.signal(signal.SIGWINCH, _resized)
    rows, columns = _size()
    os.write(sys.stdout.fileno(), b"\x1b[31mECHO-TERMINAL\x1b[0m\r\n")
    _write(f"READY:{rows}x{columns}")
    while True:
        chunk = os.read(sys.stdin.fileno(), 4096)
        if not chunk:
            return 1
        if b"q" in chunk:
            _write("EXIT:q")
            return 0
        _write(f"KEYS:{chunk.hex()}")


if __name__ == "__main__":
    raise SystemExit(main())
