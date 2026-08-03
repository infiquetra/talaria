"""Read frame-log v1 files written by :mod:`talaria.recorder.framelog`.

Feeds U3's normalization fixtures and the equivalence harness
(``tests/recorder/test_equivalence.py``, AE6). Read-only and format-only: this
module has no opinion about what a frame *means*, only about the JSON Lines
shape ``docs/formats/frame-log.md`` specifies.
"""

from __future__ import annotations

import json
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from talaria.recorder.framelog import Direction
from talaria.recorder.redact import Redaction


class FrameLogError(Exception):
    """The file is not a valid frame-log v1 recording.

    Carries enough detail (path, line number where relevant) to point at the
    cause rather than a bare parse traceback.
    """


@dataclass(frozen=True)
class FrameLogHeader:
    version: int
    started_at: str
    endpoint: str


@dataclass(frozen=True)
class FrameLogEntry:
    seq: int
    at: str
    dir: Direction
    frame: Any
    redactions: tuple[Redaction, ...] = ()
    parse_error: str | None = None


@dataclass(frozen=True)
class FrameLog:
    header: FrameLogHeader
    entries: tuple[FrameLogEntry, ...]


def _parse_header(line: str, path: Path) -> FrameLogHeader:
    try:
        obj = json.loads(line)
    except json.JSONDecodeError as exc:
        raise FrameLogError(f"{path}: header line is not valid JSON: {exc}") from exc
    if not isinstance(obj, dict) or obj.get("kind") != "header":
        raise FrameLogError(f"{path}: first line is not a header record")
    try:
        return FrameLogHeader(
            version=obj["version"],
            started_at=obj["startedAt"],
            endpoint=obj["endpoint"],
        )
    except KeyError as exc:
        raise FrameLogError(f"{path}: header is missing required field {exc}") from exc


def _parse_entry(line: str, path: Path, lineno: int) -> FrameLogEntry:
    try:
        obj = json.loads(line)
    except json.JSONDecodeError as exc:
        raise FrameLogError(f"{path}:{lineno}: not valid JSON: {exc}") from exc
    if not isinstance(obj, dict) or obj.get("kind") != "frame":
        raise FrameLogError(
            f"{path}:{lineno}: expected a frame record, got kind={obj.get('kind')!r}"
        )
    try:
        redactions = tuple(
            Redaction(path=r["path"], reason=r["reason"]) for r in obj.get("redactions", [])
        )
        return FrameLogEntry(
            seq=obj["seq"],
            at=obj["at"],
            dir=obj["dir"],
            frame=obj.get("frame"),
            redactions=redactions,
            parse_error=obj.get("parseError"),
        )
    except KeyError as exc:
        raise FrameLogError(f"{path}:{lineno}: frame record missing required field {exc}") from exc


def iter_frame_log(path: str | Path) -> Iterator[FrameLogEntry]:
    """Stream entries from a frame log without holding the whole file in memory.

    The header is validated but not yielded — call :func:`read_header` first
    if the caller needs it. Raises :class:`FrameLogError` on the first
    malformed line encountered.
    """
    path = Path(path)
    with path.open("r", encoding="utf-8") as fh:
        first = fh.readline()
        if not first:
            raise FrameLogError(f"{path} is empty: a frame log always has a header")
        _parse_header(first, path)
        for lineno, raw_line in enumerate(fh, start=2):
            line = raw_line.strip()
            if not line:
                continue
            yield _parse_entry(line, path, lineno)


def read_header(path: str | Path) -> FrameLogHeader:
    """Read just the header line, without materializing the frame body."""
    path = Path(path)
    with path.open("r", encoding="utf-8") as fh:
        first = fh.readline()
        if not first:
            raise FrameLogError(f"{path} is empty: a frame log always has a header")
        return _parse_header(first, path)


def read_frame_log(path: str | Path) -> FrameLog:
    """Read an entire frame-log v1 file into memory."""
    path = Path(path)
    header: FrameLogHeader | None = None
    entries: list[FrameLogEntry] = []

    with path.open("r", encoding="utf-8") as fh:
        first = fh.readline()
        if not first:
            raise FrameLogError(f"{path} is empty: a frame log always has a header")
        header = _parse_header(first, path)
        for lineno, raw_line in enumerate(fh, start=2):
            line = raw_line.strip()
            if not line:
                continue
            entries.append(_parse_entry(line, path, lineno))

    return FrameLog(header=header, entries=tuple(entries))
