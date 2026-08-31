"""The inspector derives operations and files only from held transcript state."""

from __future__ import annotations

from talaria.domain.changes import DiffSelection, inspector_view
from talaria.domain.projection import entry_scoped_view
from tests.domain.conftest import raw_event, replay


def test_real_tool_rows_and_inline_diffs_keep_file_and_hunk_identity() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event(
                "tool.start",
                {"tool_id": "tool-7", "name": "edit_file", "context": "two files"},
            ),
            raw_event(
                "tool.complete",
                {
                    "tool_id": "tool-7",
                    "name": "edit_file",
                    "summary": "2 files changed",
                    "inline_diff": (
                        "┊ review diff\n"
                        "--- a/talaria/domain/changes.py\n"
                        "+++ b/talaria/domain/changes.py\n"
                        "@@ -1 +1 @@\n"
                        "-old\n"
                        "+new\n"
                        "@@ -8,0 +9 @@\n"
                        "+again\n"
                        "--- /dev/null\n"
                        "+++ b/tests/domain/test_changes.py\n"
                        "@@ -0,0 +1 @@\n"
                        "+test"
                    ),
                },
            ),
        ]
    )

    view = inspector_view(entry_scoped_view(state))

    assert len(view.operations) == 1
    operation = view.operations[0]
    assert operation.name == "edit_file"
    assert operation.context == "two files"
    assert operation.status == "completed"
    assert operation.details == ("2 files changed",)
    assert operation.changed_file_keys == (
        "talaria/domain/changes.py",
        "tests/domain/test_changes.py",
    )

    assert [(file.key, file.status, file.hunk_count) for file in view.changed_files] == [
        ("talaria/domain/changes.py", "M", 2),
        ("tests/domain/test_changes.py", "A", 1),
    ]
    first = view.document.file_for("talaria/domain/changes.py")
    assert first is not None
    assert first.operation_key == operation.key
    assert DiffSelection(first.key, 1) == DiffSelection(
        file_key="talaria/domain/changes.py", hunk_index=1
    )


def test_unclassified_tool_text_stays_literal_and_does_not_invent_a_file() -> None:
    literal_detail = (
        "not a unified diff\n--- unpaired\nliteral between headers\n"
        "+++ still unpaired later\n[bold]literal"
    )
    state = replay(
        [
            raw_event("message.start"),
            raw_event("tool.start", {"name": "read_file", "context": "AGENTS.md"}),
            raw_event(
                "tool.complete",
                {
                    "name": "read_file",
                    "summary": "read complete",
                    "inline_diff": literal_detail,
                },
            ),
        ]
    )

    view = inspector_view(entry_scoped_view(state))

    assert view.document.is_empty
    assert view.changed_files == ()
    assert view.operations[0].details == ("read complete", literal_detail)


def test_a_completion_without_a_held_start_is_an_honest_operation() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("tool.complete", {"name": "search", "error": "index unavailable"}),
        ]
    )

    view = inspector_view(entry_scoped_view(state))

    assert len(view.operations) == 1
    assert view.operations[0].name == "search"
    assert view.operations[0].status == "failed"
    assert view.operations[0].details == ("index unavailable",)


def test_empty_runtime_state_produces_four_empty_inspector_sources() -> None:
    view = inspector_view(entry_scoped_view(replay([])))

    assert view.tasks == ()
    assert view.context.is_empty
    assert view.changed_files == ()
    assert view.operations == ()
    assert view.selected_operation is None
