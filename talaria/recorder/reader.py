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

from talaria.recorder.framelog import (
    FRAME_LOG_VERSION,
    FRAME_LOG_VERSION_MULTI_CONNECTION,
    Direction,
)
from talaria.recorder.redact import Redaction

#: Every frame-log version this build knows how to read. A header declaring
#: anything else is refused rather than parsed — see ``_parse_header``.
KNOWN_FRAME_LOG_VERSIONS: frozenset[int] = frozenset(
    {FRAME_LOG_VERSION, FRAME_LOG_VERSION_MULTI_CONNECTION}
)


class FrameLogError(Exception):
    """The file is not a frame-log recording this reader understands.

    Carries enough detail (path, line number where relevant) to point at the
    cause rather than a bare parse traceback. Raised for an unreadable file, and
    — since U8 — for a version this build does not know how to read.
    """


@dataclass(frozen=True)
class RecordedConnectionRow:
    """One member of a version-2 header's ``connections`` array.

    Named separately from the writer's ``RecordedConnection`` rather than
    imported: this is what was *read off a file somebody else may have written*,
    and the writer's type is what this process would emit. Sharing one class
    would make a reader's tolerance a property of the writer's current shape.
    """

    profile: str
    endpoint: str


@dataclass(frozen=True)
class FrameLogHeader:
    version: int
    started_at: str
    endpoint: str
    #: Every connection the run recorded, in the order the header lists them.
    #: Empty for a version-1 log, which is one connection by construction (KTD6)
    #: — not an unknown number of them.
    connections: tuple[RecordedConnectionRow, ...] = ()


@dataclass(frozen=True)
class FrameLogEntry:
    seq: int
    at: str
    dir: Direction
    frame: Any
    redactions: tuple[Redaction, ...] = ()
    parse_error: str | None = None
    #: Which connection this frame crossed. ``""`` in a version-1 log, where
    #: there is exactly one connection and no frame needs to name it.
    profile: str = ""


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
        version = obj["version"]
        # **The guard the format contract promised and nobody had written.**
        # ``docs/formats/frame-log.md`` states the rule this enforces: "a
        # version-1-only reader that skipped the unknown ``profile`` key would
        # merge equal ids from two different gateways into one session that never
        # existed, and would do it silently. The bump makes such a reader stop
        # instead of misread." Until U8 there was no check at all — a header
        # declaring ``version: 99`` parsed without complaint and every field a
        # future format added would have been dropped in exactly that silent way.
        #
        # Refusing forward versions is the whole point and is not a limitation to
        # apologise for: this build cannot know what a version 3 means, and the
        # failure mode of guessing is a merged session rather than an error.
        if version not in KNOWN_FRAME_LOG_VERSIONS:
            raise FrameLogError(
                f"{path}: frame log version {version!r} is not one this build can "
                f"read (known: {', '.join(str(v) for v in sorted(KNOWN_FRAME_LOG_VERSIONS))}) "
                "— a newer recording needs a newer Talaria"
            )
        rows = obj.get("connections", ())
        connections = tuple(
            RecordedConnectionRow(profile=row["profile"], endpoint=row["endpoint"])
            for row in rows
            if isinstance(row, dict)
        )
        return FrameLogHeader(
            version=version,
            started_at=obj["startedAt"],
            endpoint=obj["endpoint"],
            connections=connections,
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
            profile=obj.get("profile", "") or "",
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
