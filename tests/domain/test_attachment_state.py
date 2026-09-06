"""The attachment-state region: transitions, focus retention, projection (C9).

The ledger owns the record lifecycle; these tests pin the session state's
thin layer over it — staging, advancing, dropping, the submit-time omission
rule, survival across a session switch, and the focus-filtered projection
that lands as its own snapshot region ahead of C10's Mixture of Agents work.
"""

from __future__ import annotations

from dataclasses import replace

import pytest

from talaria.domain.attachments import AttachmentRecord
from talaria.domain.projection import (
    SNAPSHOT_REGIONS,
    attachment_view,
    project,
)
from talaria.domain.state import (
    SessionState,
    advance_attachment,
    drop_attachment,
    focus_session,
    reconcile_submitted_attachments,
    stage_attachment,
)


def file_record(
    attachment_id: str = "a-1",
    *,
    state: str = "attached",
    ref: str | None = "@file:notes.txt",
    session_id: str | None = "s1",
) -> AttachmentRecord:
    return AttachmentRecord(
        attachment_id=attachment_id,
        kind="file",
        source_path="/home/operator/docs/notes.txt",
        state=state,  # type: ignore[arg-type]
        gateway_ref=ref,
        session_id=session_id,
    )


def image_record(
    attachment_id: str = "i-1", *, session_id: str | None = "s1"
) -> AttachmentRecord:
    return AttachmentRecord(
        attachment_id=attachment_id,
        kind="image",
        source_path="/home/operator/shot.png",
        state="attached",
        gateway_ref="images/shot.png",
        session_id=session_id,
    )


def focused(session_id: str | None = "s1") -> SessionState:
    return replace(SessionState(), focused_session_id=session_id)


# ── transitions ──────────────────────────────────────────────────────────


def test_stage_files_and_drop_forgets() -> None:
    state = stage_attachment(focused(), file_record())
    assert len(state.attachments) == 1

    state = drop_attachment(state, "a-1")
    assert len(state.attachments) == 0


def test_stage_is_loud_on_a_duplicate_id() -> None:
    state = stage_attachment(focused(), file_record())
    with pytest.raises(ValueError):
        stage_attachment(state, file_record())


def test_drop_of_an_absent_id_is_a_no_op() -> None:
    state = stage_attachment(focused(), file_record())
    same = drop_attachment(state, "absent")
    assert len(same.attachments) == 1


def test_advance_writes_back_a_moved_record() -> None:
    state = stage_attachment(focused(), file_record(state="prepared", ref=None))
    prior = state.attachments.get("a-1")
    assert prior is not None
    state = advance_attachment(state, replace(prior, state="attached"))
    current = state.attachments.get("a-1")
    assert current is not None
    assert current.state == "attached"


# ── submit reconciliation ────────────────────────────────────────────────


def test_reconcile_drops_a_file_whose_token_left_the_text() -> None:
    state = stage_attachment(focused(), file_record())
    state = reconcile_submitted_attachments(state, "look at this instead")

    record = state.attachments.get("a-1")
    assert record is not None
    assert record.state == "detached"
    assert "omission" in record.detail


def test_reconcile_keeps_a_file_whose_token_was_submitted() -> None:
    state = stage_attachment(focused(), file_record())
    state = reconcile_submitted_attachments(state, "read @file:notes.txt please")

    current = state.attachments.get("a-1")
    assert current is not None
    assert current.state == "attached"


def test_reconcile_never_touches_images() -> None:
    state = stage_attachment(focused(), image_record())
    state = reconcile_submitted_attachments(state, "what is in this picture?")

    current = state.attachments.get("i-1")
    assert current is not None
    assert current.state == "attached"


def test_reconcile_ignores_records_that_are_not_attached() -> None:
    state = stage_attachment(focused(), file_record(state="prepared", ref=None))
    state = reconcile_submitted_attachments(state, "unrelated text")

    current = state.attachments.get("a-1")
    assert current is not None
    assert current.state == "prepared"


# ── focus ────────────────────────────────────────────────────────────────


def test_attachments_survive_a_session_switch_like_prompts() -> None:
    state = stage_attachment(focused("s1"), file_record())
    moved = focus_session(state, "s2")

    assert moved.attachments.get("a-1") is not None


def test_the_projection_hides_records_staged_for_another_session() -> None:
    state = stage_attachment(focused("s1"), file_record())
    other = attachment_view(focus_session(state, "s2"))

    assert other.chips == ()
    assert attachment_view(state).pending_count == 1


def test_session_less_records_show_regardless_of_focus() -> None:
    state = stage_attachment(focused("s1"), file_record(session_id=None))

    assert attachment_view(focus_session(state, "s2")).pending_count == 1


def test_chips_carry_display_names_never_local_paths() -> None:
    state = stage_attachment(focused(), file_record())
    (chip,) = attachment_view(state).chips

    assert chip.display_name == "notes.txt"
    assert chip.gateway_ref == "@file:notes.txt"
    assert chip.state == "attached"


# ── snapshot region ──────────────────────────────────────────────────────


def test_attachments_is_a_published_snapshot_region() -> None:
    assert "attachments" in SNAPSHOT_REGIONS


def test_the_first_snapshot_marks_attachments_changed() -> None:
    assert "attachments" in project(focused()).changed


def test_staging_an_attachment_marks_only_its_region() -> None:
    first = project(focused())
    second = project(stage_attachment(focused(), file_record()), previous=first)

    assert second.changed == {"attachments"}


def test_an_unchanged_ledger_marks_nothing() -> None:
    state = stage_attachment(focused(), file_record())
    first = project(state)

    assert project(state, previous=first).changed == frozenset()
