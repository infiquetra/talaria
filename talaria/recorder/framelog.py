"""Append-only recorder for gateway traffic — frame-log v1 and v2.

Ported from ``src/record/recorder.ts``. Writes JSON Lines: one
self-describing JSON object per line, appended and never rewritten. The
format is documented in ``docs/formats/frame-log.md`` — the authority — rather
than being whatever a serializer happens to emit, so a corpus recorded today
can be replayed by a Talaria written in any language later.

Every frame passes through the redaction boundary (``talaria.recorder.redact``)
before it is written. That ordering is the point of this module and must not
be relaxed.

**One log, whatever the connection count (v0.4 KTD6).** Talaria can now hold
several gateway connections at once, one per configured profile, and they all
record into a single file in native arrival order — so replay needs no
cross-log merge rule and determinism is the arrival order itself.

The version follows from the connection count, and nothing else:

* **One connection — version 1, byte-identical to v0.1–v0.3.** No ``profile``
  key on any frame, no ``connections`` list in the header. This is what
  ``talaria record``, ``talaria --record``, and every single-gateway run emit,
  which is why ``tests/recorder/test_equivalence.py`` — the corpus that pins
  the Python recorder equivalent to the TypeScript reference, and with it the
  redaction guarantee — is unaffected by any of this.
* **Two or more — version 2.** Every frame carries a ``profile`` naming the
  connection it crossed, and the header carries a ``connections`` list. The
  bump is not ceremony: a version-1-only reader that ignored ``profile`` would
  merge equal session ids from *different* gateways into one wrong fleet, so
  the bump makes an old reader stop rather than misread.
"""

from __future__ import annotations

import json
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Literal, Protocol, runtime_checkable

from talaria.recorder.redact import redact_frame, redact_url

#: Bumped when the record shape changes in a way a reader must notice.
FRAME_LOG_VERSION = 1

#: The version a run with more than one gateway connection writes (KTD6). See
#: the module docstring for why the connection count is the only thing that
#: decides between the two.
FRAME_LOG_VERSION_MULTI_CONNECTION = 2

#: Which way a frame travelled.
Direction = Literal["in", "out"]


@dataclass(frozen=True)
class RecordedConnection:
    """One connection a multi-connection recording covers.

    ``endpoint`` is redacted on the way into the header, exactly as the
    single-connection ``endpoint`` field is, so the two cannot come to disagree
    about what a safe URL looks like.
    """

    profile: str
    endpoint: str


@runtime_checkable
class FrameSink(Protocol):
    """What :class:`~talaria.transport.source.LiveSource` needs of a recorder.

    Declared as a shape rather than as :class:`FrameRecorder` itself because a
    multi-connection run hands each source a *view* onto one shared file
    (:meth:`FrameRecorder.view`), and a source must not be able to tell the
    difference — the view is what stamps the connection identity, so a source
    that could reach the underlying recorder could write an untagged frame into
    a tagged log.
    """

    def record(self, direction: Direction, raw: str) -> None:
        """Record one frame as it crossed the wire."""
        ...

    def close(self) -> None:
        """Release this sink. Idempotent."""
        ...


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
        connections: Sequence[RecordedConnection] | None = None,
        clock: Callable[[], str] = _default_clock,
    ) -> None:
        self._path = Path(path)
        self._clock = clock
        self._seq = 0
        self._redaction_count = 0
        self._parse_error_count = 0
        self._closed = False
        #: The connections this log covers, or ``()`` when the caller named
        #: none. Fewer than two means a single-connection recording, which is
        #: version 1 and carries no connection identity anywhere — see the
        #: module docstring.
        self._connections: tuple[RecordedConnection, ...] = tuple(connections or ())
        self._multi = len(self._connections) > 1
        self._known_profiles = frozenset(row.profile for row in self._connections)
        #: Outstanding :meth:`view` handles. The file closes when the last one
        #: does, because N sources each closing their own view must not end the
        #: log the other N-1 are still writing to.
        self._open_views = 0
        #: Outstanding :meth:`hold` claims, counted separately from views.
        #:
        #: A view's lifetime is one *connection*; a hold's is the whole *run*.
        #: Conflating them was a real defect: reconnecting a profile retires
        #: its source before building the replacement, so in a fleet where the
        #: other profile never dialled, the retire crossed the view count to
        #: zero, closed the file, and the replacement's very first frame raised
        #: onto a closed log. The recording belongs to the run, so the run gets
        #: its own claim and view churn underneath it is free.
        self._holds = 0

        if self._multi and len(self._known_profiles) != len(self._connections):
            raise RecorderError(
                f"frame log {self._path}: two connections share one profile name; "
                "a per-frame profile key must identify exactly one connection"
            )

        try:
            self._path.parent.mkdir(parents=True, exist_ok=True)
            self._file = self._path.open("a", encoding="utf-8")
        except OSError as exc:
            raise RecorderError(f"could not open frame log {self._path}: {exc}") from exc

        header: dict[str, Any] = {
            "kind": "header",
            "version": (
                FRAME_LOG_VERSION_MULTI_CONNECTION if self._multi else FRAME_LOG_VERSION
            ),
            "startedAt": self._clock(),
            "endpoint": redact_url(endpoint),
        }
        if self._multi:
            # ``endpoint`` above is kept rather than dropped: a v2 header is a
            # superset of a v1 one, so a field never changes meaning by
            # disappearing. It names the run's first connection, and
            # ``connections`` names them all.
            header["connections"] = [
                {"profile": row.profile, "endpoint": redact_url(row.endpoint)}
                for row in self._connections
            ]
        self._write_line(header)

    @property
    def multi_connection(self) -> bool:
        """Whether this log stamps connection identity (KTD6's version-2 shape)."""
        return self._multi

    @property
    def connections(self) -> tuple[RecordedConnection, ...]:
        return self._connections

    def view(self, profile: str) -> ProfileFrameRecorder:
        """A sink that stamps every frame it records with ``profile``.

        This is how a connection gets a recorder it cannot mislabel: the
        profile is bound here, once, by whoever knows the connection's
        identity, rather than passed at every ``record`` call by a source that
        would have to be trusted to pass the right one.
        """
        if self._multi and profile not in self._known_profiles:
            raise RecorderError(
                f"frame log {self._path}: {profile!r} is not one of this "
                "recording's declared connections"
            )
        self._open_views += 1
        return ProfileFrameRecorder(self, profile)

    def record(self, direction: Direction, raw: str, *, profile: str | None = None) -> None:
        """Record one frame as received from, or sent to, the wire.

        Takes the raw text rather than a parsed object so that a frame
        Talaria cannot parse is still recorded — an unparseable frame is
        exactly the kind of protocol drift the corpus exists to catch.

        ``profile`` is written only on a multi-connection log, and is
        **required** there. A single-connection log drops it unwritten, which
        is what keeps a v0.1-format recording byte-identical: the caller does
        not have to know which shape it is writing into.
        """
        if self._closed:
            raise RecorderError(f"frame log {self._path} is already closed")
        if self._multi:
            if not profile:
                raise RecorderError(
                    f"frame log {self._path} covers {len(self._connections)} connections; "
                    "every frame must name the one it crossed"
                )
            if profile not in self._known_profiles:
                raise RecorderError(
                    f"frame log {self._path}: {profile!r} is not one of this "
                    "recording's declared connections"
                )

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
                self._tag(
                    {
                        "kind": "frame",
                        "seq": self._seq,
                        "at": at,
                        "dir": direction,
                        "frame": None,
                        "parseError": str(exc),
                    },
                    profile,
                )
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
        self._write_line(self._tag(record, profile))

    def _tag(self, record: dict[str, Any], profile: str | None) -> dict[str, Any]:
        """Stamp connection identity, on a multi-connection log and nowhere else."""
        if self._multi and profile:
            record["profile"] = profile
        return record

    def hold(self) -> None:
        """Keep the log open for the caller's whole lifetime, view churn aside.

        A connection set takes one of these at construction and drops it when
        it closes, so retiring the last connection mid-run cannot end a
        recording the set is about to write to again.
        """
        self._holds += 1

    def release_hold(self) -> None:
        """Drop a :meth:`hold`; close the file if nothing else claims it."""
        if self._holds > 0:
            self._holds -= 1
        self._close_if_unclaimed()

    def release_view(self) -> None:
        """One view is done; close the file when nothing claims it (KTD6).

        A view that closes when others are still open must not end the log —
        the remaining connections would then raise on their next frame, and one
        gateway hanging up would take the whole recording with it. A run-scoped
        :meth:`hold` counts as a claim for the same reason.
        """
        if self._open_views > 0:
            self._open_views -= 1
        self._close_if_unclaimed()

    def _close_if_unclaimed(self) -> None:
        if self._open_views == 0 and self._holds == 0:
            self.close()

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


class ProfileFrameRecorder:
    """One connection's :class:`FrameSink` onto a shared :class:`FrameRecorder`.

    Holds the profile so the source it is handed to never has to — a source
    that could choose the tag could mislabel a frame, and a mislabelled frame
    in a fleet recording is worse than an untagged one: it attributes a
    session to a gateway that never saw it.

    :meth:`close` releases this connection's hold on the file rather than
    closing it, so the log survives one gateway going away and ends when the
    last connection does.
    """

    def __init__(self, recorder: FrameRecorder, profile: str) -> None:
        self._recorder = recorder
        self._profile = profile
        self._closed = False

    @property
    def profile(self) -> str:
        return self._profile

    @property
    def file_path(self) -> Path:
        return self._recorder.file_path

    def record(self, direction: Direction, raw: str) -> None:
        self._recorder.record(direction, raw, profile=self._profile)

    def close(self) -> None:
        if self._closed:
            return
        self._closed = True
        self._recorder.release_view()


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
