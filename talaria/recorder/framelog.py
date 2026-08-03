"""Append-only recorder for gateway traffic — frame-log v1.

Ported from ``src/record/recorder.ts``. Writes JSON Lines: one
self-describing JSON object per line, appended and never rewritten. The
format is documented in ``docs/formats/frame-log.md`` — the authority — rather
than being whatever a serializer happens to emit, so a corpus recorded today
can be replayed by a Talaria written in any language later.

Every frame passes through the redaction boundary (``talaria.recorder.redact``)
before it is written. That ordering is the point of this module and must not
be relaxed.
"""

from __future__ import annotations

import json
from collections.abc import Callable
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Literal

from talaria.recorder.redact import redact_frame, redact_url

#: Bumped when the record shape changes in a way a reader must notice.
FRAME_LOG_VERSION = 1

#: Which way a frame travelled.
Direction = Literal["in", "out"]


class RecorderError(Exception):
    """A create/write/flush/close failure at the recorder boundary (R25).

    Carries the offending path so a failure is named rather than surfaced as
    a bare traceback several frames away from the recorder that raised it.
    """


@dataclass(frozen=True)
class RecorderStats:
    frames: int
    redactions: int
    parse_errors: int


def _default_clock() -> str:
    """ISO-8601 with millisecond precision and a ``Z`` suffix, e.g.
    ``2026-08-02T12:21:35.016Z`` — matching ``Date.prototype.toISOString()``.
    """
    now = datetime.now(UTC)
    return now.strftime("%Y-%m-%dT%H:%M:%S.") + f"{now.microsecond // 1000:03d}Z"


class FrameRecorder:
    """Records frames to a JSON Lines file.

    Construction opens the file and writes the header immediately, so an
    empty recording is still self-describing and distinguishable from a
    missing one.
    """

    def __init__(
        self,
        path: str | Path,
        endpoint: str,
        *,
        clock: Callable[[], str] = _default_clock,
    ) -> None:
        self._path = Path(path)
        self._clock = clock
        self._seq = 0
        self._redaction_count = 0
        self._parse_error_count = 0
        self._closed = False

        try:
            self._path.parent.mkdir(parents=True, exist_ok=True)
            self._file = self._path.open("a", encoding="utf-8")
        except OSError as exc:
            raise RecorderError(f"could not open frame log {self._path}: {exc}") from exc

        header = {
            "kind": "header",
            "version": FRAME_LOG_VERSION,
            "startedAt": self._clock(),
            "endpoint": redact_url(endpoint),
        }
        self._write_line(header)

    def record(self, direction: Direction, raw: str) -> None:
        """Record one frame as received from, or sent to, the wire.

        Takes the raw text rather than a parsed object so that a frame
        Talaria cannot parse is still recorded — an unparseable frame is
        exactly the kind of protocol drift the corpus exists to catch.
        """
        if self._closed:
            raise RecorderError(f"frame log {self._path} is already closed")

        self._seq += 1
        at = self._clock()

        try:
            parsed = json.loads(raw)
        except json.JSONDecodeError as exc:
            self._parse_error_count += 1
            # The unparseable payload is withheld rather than stored: it
            # could not be walked by the redaction boundary, so it cannot be
            # shown to be safe.
            self._write_line(
                {
                    "kind": "frame",
                    "seq": self._seq,
                    "at": at,
                    "dir": direction,
                    "frame": None,
                    "parseError": str(exc),
                }
            )
            return

        result = redact_frame(parsed)
        self._redaction_count += len(result.redactions)

        record: dict[str, Any] = {
            "kind": "frame",
            "seq": self._seq,
            "at": at,
            "dir": direction,
            "frame": result.frame,
        }
        if result.redactions:
            record["redactions"] = [
                {"path": r.path, "reason": r.reason} for r in result.redactions
            ]
        self._write_line(record)

    def stats(self) -> RecorderStats:
        return RecorderStats(
            frames=self._seq,
            redactions=self._redaction_count,
            parse_errors=self._parse_error_count,
        )

    def close(self) -> None:
        """Flush and close. Safe to call more than once."""
        if self._closed:
            return
        self._closed = True
        try:
            self._file.flush()
            self._file.close()
        except OSError as exc:
            raise RecorderError(f"could not close frame log {self._path}: {exc}") from exc

    @property
    def file_path(self) -> Path:
        """Where this recording is being written."""
        return self._path

    def _write_line(self, record: dict[str, Any]) -> None:
        try:
            self._file.write(json.dumps(record) + "\n")
            self._file.flush()
        except (OSError, ValueError) as exc:
            raise RecorderError(f"could not write frame log {self._path}: {exc}") from exc


def default_log_path(recordings_dir: Path, now: datetime | None = None) -> Path:
    """Default recording path under KTD15's ``<config_dir>/recordings/``.

    Timestamped so successive runs never overwrite a corpus — the same
    pattern as the TypeScript reference's ``defaultLogPath``
    (``src/record/command.ts:24-27``), rebased onto the Python config
    surface's ``recordings_dir()`` (U1, KTD15) rather than the TypeScript
    tree's repo-local ``.talaria/frames/``.
    """
    dt = now if now is not None else datetime.now(UTC)
    iso = dt.strftime("%Y-%m-%dT%H:%M:%S.") + f"{dt.microsecond // 1000:03d}Z"
    stamp = iso.replace(":", "-").replace(".", "-")
    return recordings_dir / f"{stamp}.jsonl"
