"""Attachment records: what is staged to send, route-independently (C9).

An attachment is a file or image the operator staged for the agent, however
it arrived — a typed path, a dropped path, or any route the operator later
selects (D6). This module deliberately records no route: the transport
contract (I6) is identical for every route once a path exists, and naming
one route here would hard-code D6's undecided choice into the substrate
every future route has to carry.

State is a small explicit lifecycle. ``prepared`` means read and
classified locally, nothing sent. ``attached`` means the gateway confirmed
staging and returned a reference. ``failed`` means an attempt ended without
one. ``detached`` means a confirmed attachment was removed gateway-side.
A record never claims delivery: the gateway reference proves staging, not
that the agent read anything.

:class:`AttachmentLedger` is the whole store: an immutable tuple of
records with add/discard/replace semantics. It lives here rather than in
:mod:`talaria.domain.state` because the composer surface that will own it
lands after D6, and the substrate must be usable — and testable — before
that surface exists.

Nothing here reads a clock, opens a file, or touches a socket. Pure data
and pure transitions, standard library only (ADR-0002).
"""

from __future__ import annotations

from collections.abc import Iterator
from dataclasses import dataclass, field
from pathlib import Path
from typing import Literal

__all__ = [
    "AttachmentKind",
    "AttachmentLedger",
    "AttachmentRecord",
    "AttachmentState",
]

#: What the gateway stages: a non-image file (``file.attach``) or an image
#: (``image.attach_bytes``). PDFs are not named here: whether a PDF travels
#: ``file.attach`` or the poppler-backed ``pdf.attach`` is a type-routing
#: decision D6 has not made, and this module does not pre-empt it.
AttachmentKind = Literal["file", "image"]

#: The record lifecycle. No ``sending`` state: the bytes-upload calls are
#: synchronous request/response pairs, so in-flight exists only on the
#: caller's stack, never in stored state worth re-reading.
AttachmentState = Literal["prepared", "attached", "failed", "detached"]


@dataclass(frozen=True)
class AttachmentRecord:
    """One staged attachment and what is known about it."""

    #: Route-layer identifier, unique within a ledger. Minted by the caller;
    #: this module assigns no identities, so two stages of the same file are
    #: two records and a retry never silently becomes a duplicate.
    attachment_id: str
    kind: AttachmentKind
    #: The operator-local path, as given. Needed to re-read the file on
    #: retry. Never written to recorded evidence: the redaction rule in
    #: :mod:`talaria.domain.redaction` withholds attachment paths from
    #: frames, so only the gateway-issued reference below is recordable.
    source_path: str
    #: Current lifecycle state. Only ever advanced by replacing the record;
    #: frozen, so no caller can half-update one.
    state: AttachmentState = "prepared"
    #: What the operator sees. Defaults to the file name; never the full
    #: local path, so display surfaces cannot leak directory structure.
    display_name: str = ""
    #: The gateway-issued reference (``@file:`` ref or staged image path).
    #: ``None`` until the gateway confirms staging. Safe to record.
    gateway_ref: str | None = None
    #: File size in bytes at read time, when known. Informational only;
    #: the gateway's size cap (4018) is the enforced bound, not this.
    size_bytes: int | None = None
    #: The last outcome detail, human-readable. Empty until something happens.
    detail: str = ""

    def __post_init__(self) -> None:
        if not self.attachment_id:
            raise ValueError("attachment_id must not be empty")
        if self.kind not in ("file", "image"):
            raise ValueError(f"unknown attachment kind: {self.kind!r}")
        if not self.source_path:
            raise ValueError("source_path must not be empty")
        if self.state not in ("prepared", "attached", "failed", "detached"):
            raise ValueError(f"unknown attachment state: {self.state!r}")
        if not self.display_name:
            object.__setattr__(
                self, "display_name", Path(self.source_path).name or self.source_path
            )
        if self.size_bytes is not None and self.size_bytes < 0:
            raise ValueError("size_bytes must not be negative")


@dataclass(frozen=True)
class AttachmentLedger:
    """An immutable set of attachment records, keyed by attachment id."""

    records: tuple[AttachmentRecord, ...] = field(default_factory=tuple)

    def __len__(self) -> int:
        return len(self.records)

    def __iter__(self) -> Iterator[AttachmentRecord]:
        return iter(self.records)

    def get(self, attachment_id: str) -> AttachmentRecord | None:
        """The record for one id, or ``None`` when no such record exists."""
        for record in self.records:
            if record.attachment_id == attachment_id:
                return record
        return None

    def add(self, record: AttachmentRecord) -> AttachmentLedger:
        """Stage one record. A duplicate id is a caller bug, reported loudly:
        silently replacing would let a retry masquerade as the original."""
        if self.get(record.attachment_id) is not None:
            raise ValueError(f"duplicate attachment_id: {record.attachment_id!r}")
        return AttachmentLedger(records=(*self.records, record))

    def discard(self, attachment_id: str) -> AttachmentLedger:
        """Remove one record. Removing an absent id is a no-op, because
        remove-then-remove-again is an ordinary operator gesture, not an error."""
        return AttachmentLedger(
            records=tuple(
                record
                for record in self.records
                if record.attachment_id != attachment_id
            )
        )

    def replace(self, record: AttachmentRecord) -> AttachmentLedger:
        """Advance one record (new state, reference, or detail) by id.

        Upserts: replacing a record the ledger has never seen stages it,
        so a route layer that builds the final record first needs no
        separate add step.
        """
        return AttachmentLedger(
            records=tuple(
                record if prior.attachment_id == record.attachment_id else prior
                for prior in self.records
            )
            + (
                ()
                if self.get(record.attachment_id) is not None
                else (record,)
            )
        )
