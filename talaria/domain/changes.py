"""Framework-free projections for the session inspector and diff handoff.

The gateway has no inspector endpoint.  Everything here is derived from the
entry-scoped transcript, needs-you queue, and sub-agent projection Talaria
already holds.  Parsing is deliberately conservative: only the normalized tool
rows written by :mod:`talaria.domain.state` and adjacent unified-diff headers
create structure.  Unrecognized tool text remains literal operation detail.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, replace
from typing import Literal

from talaria.domain.models import Usage
from talaria.domain.projection import EntryScopedView, SubagentView
from talaria.domain.queue import NeedsYouQueue

OperationStatus = Literal["running", "completed", "failed", "observed"]
ChangedFileStatus = Literal["M", "A", "D", "R"]
TaskSource = Literal["queue", "agent"]


@dataclass(frozen=True)
class OperationView:
    """One operation reconstructed from already-normalized transcript rows."""

    key: str
    entry_id: int
    name: str
    context: str = ""
    status: OperationStatus = "observed"
    details: tuple[str, ...] = ()
    changed_file_keys: tuple[str, ...] = ()


@dataclass(frozen=True)
class ChangedFileView:
    """One file body reported by a stored unified diff."""

    key: str
    path: str
    status: ChangedFileStatus
    old_path: str
    new_path: str
    unified_diff: str
    hunk_count: int
    operation_key: str | None = None


@dataclass(frozen=True)
class DiffDocument:
    """The immutable, session-reported file set consumed by the diff viewer."""

    files: tuple[ChangedFileView, ...] = ()

    def file_for(self, key: str) -> ChangedFileView | None:
        """Return the file named by a selection, if it is still held."""
        for changed_file in self.files:
            if changed_file.key == key:
                return changed_file
        return None

    @property
    def is_empty(self) -> bool:
        return not self.files


@dataclass(frozen=True)
class DiffSelection:
    """The complete immutable handoff from inspector to diff viewer."""

    file_key: str
    hunk_index: int = 0


@dataclass(frozen=True)
class InspectorTaskView:
    """One needs-you or sub-agent row shown in the Tasks section."""

    key: str
    label: str
    status: str
    source: TaskSource
    detail: str = ""


@dataclass(frozen=True)
class InspectorContextView:
    """Context facts the app already holds at the serialized render boundary."""

    session_id: str = ""
    profile: str = ""
    endpoint: str = ""
    model: str = ""
    input_tokens: int | None = None
    output_tokens: int | None = None

    @property
    def is_empty(self) -> bool:
        return not any(
            (
                self.session_id,
                self.profile,
                self.endpoint,
                self.model,
                self.input_tokens is not None,
                self.output_tokens is not None,
            )
        )


@dataclass(frozen=True)
class InspectorView:
    """All four inspector sections, computed once and handed to the UI."""

    tasks: tuple[InspectorTaskView, ...]
    context: InspectorContextView
    document: DiffDocument
    operations: tuple[OperationView, ...]
    selected_operation: OperationView | None = None

    @property
    def changed_files(self) -> tuple[ChangedFileView, ...]:
        return self.document.files


def inspector_view(
    entries: EntryScopedView,
    *,
    queue: NeedsYouQueue | None = None,
    agents: SubagentView | None = None,
    session_id: str = "",
    profile: str = "",
    endpoint: str = "",
    model: str = "",
    usage: Usage | None = None,
    selected_operation_key: str | None = None,
) -> InspectorView:
    """Build the four inspector sections exclusively from held runtime state."""
    operations, document = parse_changes(entries)
    selected = next(
        (operation for operation in operations if operation.key == selected_operation_key),
        operations[-1] if operations else None,
    )
    observed_usage = usage if usage is not None and usage.observed else None
    return InspectorView(
        tasks=_task_views(queue or NeedsYouQueue(), agents),
        context=InspectorContextView(
            session_id=session_id,
            profile=profile,
            endpoint=endpoint,
            model=model,
            input_tokens=(observed_usage.input_tokens if observed_usage is not None else None),
            output_tokens=(observed_usage.output_tokens if observed_usage is not None else None),
        ),
        document=document,
        operations=operations,
        selected_operation=selected,
    )


def parse_changes(entries: EntryScopedView) -> tuple[tuple[OperationView, ...], DiffDocument]:
    """Parse stored tool entries into operations and their reported file set.

    The reducer intentionally stores plain transcript text rather than the raw
    tool payload.  Completion rows are therefore matched to the most recent
    still-running operation with the same normalized name.  When no start row
    is held, the completion is still an honest standalone observation.
    """
    operations: list[OperationView] = []
    file_order: list[str] = []
    files_by_key: dict[str, ChangedFileView] = {}
    current_operation_index: int | None = None

    for entry in entries.entries:
        if entry.kind != "tool":
            continue
        parsed_row = _parse_tool_row(entry.raw_body)
        if parsed_row is not None:
            name, context, status, detail = parsed_row
            if status == "running":
                operations.append(
                    OperationView(
                        key=f"operation:{entry.entry_id}",
                        entry_id=entry.entry_id,
                        name=name,
                        context=context,
                        status=status,
                    )
                )
                current_operation_index = len(operations) - 1
                continue

            matching = next(
                (
                    index
                    for index in range(len(operations) - 1, -1, -1)
                    if operations[index].name == name and operations[index].status == "running"
                ),
                None,
            )
            if matching is None:
                operations.append(
                    OperationView(
                        key=f"operation:{entry.entry_id}",
                        entry_id=entry.entry_id,
                        name=name,
                        status=status,
                        details=(detail,) if detail else (),
                    )
                )
                current_operation_index = len(operations) - 1
            else:
                operation = operations[matching]
                operations[matching] = replace(
                    operation,
                    status=status,
                    details=(*operation.details, detail) if detail else operation.details,
                )
                current_operation_index = matching
            continue

        operation_key = (
            operations[current_operation_index].key if current_operation_index is not None else None
        )
        changed_files = _parse_diff(entry.raw_body, operation_key=operation_key)
        if changed_files:
            observed_keys: list[str] = []
            for changed_file in changed_files:
                if changed_file.key not in files_by_key:
                    file_order.append(changed_file.key)
                files_by_key[changed_file.key] = changed_file
                observed_keys.append(changed_file.key)
            if current_operation_index is not None:
                operation = operations[current_operation_index]
                operations[current_operation_index] = replace(
                    operation,
                    changed_file_keys=_ordered_unique(
                        (*operation.changed_file_keys, *observed_keys)
                    ),
                )
            continue

        if current_operation_index is None:
            operations.append(
                OperationView(
                    key=f"operation:{entry.entry_id}",
                    entry_id=entry.entry_id,
                    name="operation",
                    status="observed",
                    details=(entry.raw_body,),
                )
            )
            current_operation_index = len(operations) - 1
        else:
            operation = operations[current_operation_index]
            operations[current_operation_index] = replace(
                operation,
                details=(*operation.details, entry.raw_body),
            )

    return tuple(operations), DiffDocument(
        files=tuple(files_by_key[key] for key in file_order)
    )


def _parse_tool_row(
    text: str,
) -> tuple[str, str, OperationStatus, str] | None:
    if "\n" in text or not text.startswith("⏺ "):
        return None
    body = text.removeprefix("⏺ ")
    name, separator, remainder = body.partition(" ")
    if not name:
        return None
    rest = remainder if separator else ""
    if rest == "✓" or rest.startswith("✓ "):
        return name, "", "completed", rest.removeprefix("✓").lstrip()
    if rest == "✗" or rest.startswith("✗ "):
        return name, "", "failed", rest.removeprefix("✗").lstrip()
    return name, rest, "running", ""


def _parse_diff(text: str, *, operation_key: str | None) -> tuple[ChangedFileView, ...]:
    lines = text.splitlines()
    headers = [
        index
        for index in range(len(lines) - 1)
        if lines[index].startswith("--- ") and lines[index + 1].startswith("+++ ")
    ]
    changed_files: list[ChangedFileView] = []
    for position, start in enumerate(headers):
        end = headers[position + 1] if position + 1 < len(headers) else len(lines)
        old_path = _diff_header_path(lines[start], "--- ")
        new_path = _diff_header_path(lines[start + 1], "+++ ")
        if not old_path or not new_path:
            continue
        path = _display_path(new_path if new_path != "/dev/null" else old_path)
        if not path:
            continue
        patch_lines = lines[start:end]
        changed_files.append(
            ChangedFileView(
                key=path,
                path=path,
                status=_file_status(old_path, new_path),
                old_path=_display_path(old_path),
                new_path=_display_path(new_path),
                unified_diff="\n".join(patch_lines),
                hunk_count=sum(line.startswith("@@ ") for line in patch_lines),
                operation_key=operation_key,
            )
        )
    return tuple(changed_files)


def _diff_header_path(line: str, prefix: str) -> str:
    value = line.removeprefix(prefix).split("\t", 1)[0]
    if value.startswith('"') and value.endswith('"'):
        try:
            decoded = json.loads(value)
        except json.JSONDecodeError:
            return value
        return decoded if isinstance(decoded, str) else value
    return value


def _display_path(path: str) -> str:
    if path == "/dev/null":
        return path
    if path.startswith(("a/", "b/")):
        return path[2:]
    return path


def _file_status(old_path: str, new_path: str) -> ChangedFileStatus:
    if old_path == "/dev/null":
        return "A"
    if new_path == "/dev/null":
        return "D"
    if _display_path(old_path) != _display_path(new_path):
        return "R"
    return "M"


def _ordered_unique(values: tuple[str, ...]) -> tuple[str, ...]:
    return tuple(dict.fromkeys(values))


def _task_views(
    queue: NeedsYouQueue, agents: SubagentView | None
) -> tuple[InspectorTaskView, ...]:
    tasks: list[InspectorTaskView] = []
    for item in queue.items:
        status = "requested" if item.requested else ("waiting" if item.answerable else "blocked")
        tasks.append(
            InspectorTaskView(
                key=f"queue:{item.profile}:{item.session_id}:{item.request_key}",
                label=item.kind or "task",
                status=status,
                source="queue",
                detail=item.summary,
            )
        )
    if agents is not None:
        for row in agents.rows:
            tasks.append(
                InspectorTaskView(
                    key=f"agent:{row.id}",
                    label=row.name,
                    status=row.status,
                    source="agent",
                    detail=row.detail or "",
                )
            )
    return tuple(tasks)
