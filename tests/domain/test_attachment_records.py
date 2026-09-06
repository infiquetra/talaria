"""Attachment record and ledger tests (C9, route-independent substrate)."""

from __future__ import annotations

import pytest

from talaria.domain.attachments import (
    AttachmentLedger,
    AttachmentRecord,
)


def test_record_derives_a_basename_display_name() -> None:
    record = AttachmentRecord(
        attachment_id="a-1",
        kind="file",
        source_path="/home/operator/docs/notes.txt",
        state="prepared",
    )

    assert record.display_name == "notes.txt"
    assert record.gateway_ref is None
    assert record.detail == ""


def test_record_rejects_empty_identity_and_bad_enums() -> None:
    with pytest.raises(ValueError):
        AttachmentRecord(
            attachment_id="", kind="file", source_path="/tmp/x.txt", state="prepared"
        )
    with pytest.raises(ValueError):
        AttachmentRecord(
            attachment_id="a", kind="file", source_path="", state="prepared"
        )
    with pytest.raises(ValueError):
        AttachmentRecord(
            attachment_id="a",
            kind="video",  # type: ignore[arg-type]
            source_path="/tmp/x.mp4",
            state="prepared",
        )
    with pytest.raises(ValueError):
        AttachmentRecord(
            attachment_id="a",
            kind="file",
            source_path="/tmp/x.txt",
            state="sending",  # type: ignore[arg-type]
        )


def test_ledger_add_is_loud_on_duplicate_id() -> None:
    first = AttachmentRecord(
        attachment_id="a-1", kind="file", source_path="/tmp/a.txt", state="prepared"
    )
    ledger = AttachmentLedger().add(first)

    assert len(ledger) == 1
    assert ledger.get("a-1") == first
    assert ledger.get("absent") is None
    with pytest.raises(ValueError):
        ledger.add(first)


def test_ledger_discard_is_idempotent() -> None:
    record = AttachmentRecord(
        attachment_id="a-1", kind="image", source_path="/tmp/a.png", state="prepared"
    )
    ledger = AttachmentLedger().add(record)

    kept = ledger.discard("a-1")
    assert len(kept) == 0
    # Removing again is an ordinary gesture, not an error.
    assert len(kept.discard("a-1")) == 0
    # The original ledger is untouched: frozen means frozen.
    assert len(ledger) == 1


def test_ledger_replace_advances_state_and_upserts() -> None:
    record = AttachmentRecord(
        attachment_id="a-1", kind="file", source_path="/tmp/a.txt", state="prepared"
    )
    ledger = AttachmentLedger().add(record)

    attached = ledger.replace(
        AttachmentRecord(
            attachment_id="a-1",
            kind="file",
            source_path="/tmp/a.txt",
            state="attached",
            gateway_ref="@file:.hermes/a.txt",
            size_bytes=12,
            detail="staged",
        )
    )
    current = attached.get("a-1")
    assert current is not None
    assert current.state == "attached"
    assert current.gateway_ref == "@file:.hermes/a.txt"

    # A record the ledger never saw stages directly through replace.
    fresh = attached.replace(
        AttachmentRecord(
            attachment_id="a-2", kind="image", source_path="/tmp/b.png", state="failed",
            detail="too large",
        )
    )
    assert len(fresh) == 2
    assert [record.attachment_id for record in fresh] == ["a-1", "a-2"]


def test_same_path_restaged_is_a_new_record_not_a_duplicate() -> None:
    """Remove-then-re-add (Live 15) must work: identity is the id, and a
    retry mints a new one rather than silently reviving the old."""
    first = AttachmentRecord(
        attachment_id="a-1", kind="file", source_path="/tmp/a.txt", state="failed"
    )
    ledger = AttachmentLedger().add(first).discard("a-1")
    second = AttachmentRecord(
        attachment_id="a-2", kind="file", source_path="/tmp/a.txt", state="prepared"
    )

    restaged = ledger.add(second)
    assert len(restaged) == 1
    assert restaged.get("a-2") is not None
