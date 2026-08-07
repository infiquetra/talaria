"""A gating document cannot go stale in silence.

DRIFT-04 was a release-gate document that named two conditions which would change
its verdict, both of which were met by later work two days on, with nothing in the
repository pointing back at the document to say so. The convention this module
enforces — and the two shapes rejected in favour of it — is recorded in
``docs/engineering-journal/DECISIONS.md``.

A gating document declares a fenced ``gate`` block: its identifier, the verdict it
currently states, the date by which it must be re-read, and one ``blocks-on`` line
per condition holding that verdict in place. Every assertion here reads that block
against the prose it claims to summarize, so the block cannot drift from the table
it describes, and the block cannot outlive the table's own grades.

The loop closes without anyone remembering a convention. Editing an evidence-table
row is the natural act when a condition clears;
:func:`test_every_condition_matches_the_evidence_row_it_names` then forces the gate
block to follow, and
:func:`test_no_declared_condition_has_already_been_cleared` refuses the result — so
the verdict has to be restated rather than left contradicting its own table.

Those two read from the block towards the table, so both go quiet for a gate that
declares no conditions at all — which is what a gate looks like the moment it
clears. :func:`test_every_uncleared_row_declares_itself_as_a_condition` reads the
other way, from the table towards the block, and is the only check here that says
anything about a cleared gate's rows. It was added on 2026-08-07, when
`v0-1-daily-driver` lost its last condition and a mutation putting an unmet row
back into a silent gate stayed green.

The backlink and horizon checks below cover the two cases that escape all of them:
work that clears a condition without touching the gating document at all, and a
gate nobody has looked at in a while.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from datetime import date
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parents[2]
_DOCS = _REPO_ROOT / "docs"

# A grade meaning the condition no longer blocks. A gate still listing a condition
# that carries one of these is a verdict that has outlived its own evidence table.
_CLEARED_GRADES = frozenset({"met", "measured"})

_REQUIRED_FIELDS = ("id", "verdict", "review-by")

_GATE_BLOCK = re.compile(r"^```gate\n(.*?)^```", re.MULTILINE | re.DOTALL)
_EVIDENCE_TABLE = re.compile(r"^## Evidence table\s*?$(.*?)^## ", re.MULTILINE | re.DOTALL)
_EMPHASIZED = re.compile(r"\*\*(.+?)\*\*")
# The bracketed placeholder form used when *documenting* this convention
# (``Clears: <gate-id>#<condition-id>``) deliberately does not match: an example
# in prose must not read as a live claim.
_BACKLINK = re.compile(r"Clears:\s*([A-Za-z0-9][\w.-]*)#([A-Za-z0-9][\w.-]*)")
# An evidence-table row label: ``12``, or ``6a`` for a row that qualifies another.
_ROW_LABEL = re.compile(r"\d+[a-z]?")


@dataclass(frozen=True)
class Gate:
    """One ``gate`` block, parsed."""

    path: Path
    identifier: str
    verdict: str
    review_by: date
    conditions: tuple[tuple[str, str], ...]


def _parse_gate(path: Path, body: str) -> Gate:
    """Parse a gate block, or fail the calling test with the reason.

    The format is deliberately flat ``key: value`` lines with a repeated
    ``blocks-on`` key rather than nested YAML, so that reading it needs no
    dependency and no indentation rules — the parser stays small enough to be
    obviously correct, which matters for a check whose whole job is to be trusted.
    """
    fields: dict[str, str] = {}
    conditions: list[tuple[str, str]] = []
    for raw_line in body.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        key, separator, value = line.partition(":")
        assert separator, f"{path}: gate line is not `key: value`: {raw_line!r}"
        key, value = key.strip(), value.strip()
        if key == "blocks-on":
            condition_id, _, grade = value.partition(" ")
            assert grade.strip(), (
                f"{path}: `blocks-on` needs a condition id and a grade: {value!r}"
            )
            conditions.append((condition_id.strip(), grade.strip()))
        else:
            assert key not in fields, f"{path}: gate declares {key!r} twice"
            fields[key] = value

    for field_name in _REQUIRED_FIELDS:
        assert field_name in fields, f"{path}: gate block is missing `{field_name}`"

    raw_date = fields["review-by"]
    try:
        review_by = date.fromisoformat(raw_date)
    except ValueError as error:  # pragma: no cover - message is the point
        raise AssertionError(f"{path}: `review-by` is not an ISO date: {raw_date!r}") from error

    return Gate(
        path=path,
        identifier=fields["id"],
        verdict=fields["verdict"],
        review_by=review_by,
        conditions=tuple(conditions),
    )


def _discover_gate_sources() -> tuple[tuple[Path, str], ...]:
    found: list[tuple[Path, str]] = []
    for path in sorted(_DOCS.rglob("*.md")):
        for match in _GATE_BLOCK.finditer(path.read_text(encoding="utf-8")):
            found.append((path, match.group(1)))
    return tuple(found)


_GATE_SOURCES = _discover_gate_sources()
_GATE_IDS = [str(path.relative_to(_REPO_ROOT)) for path, _ in _GATE_SOURCES]

_over_each_gate = pytest.mark.parametrize(("path", "body"), _GATE_SOURCES, ids=_GATE_IDS)


def _evidence_table(path: Path) -> str:
    """The ``## Evidence table`` section alone.

    Scoped rather than searched whole because the method table further down the
    same document also numbers its rows, and a condition must not be able to match
    the wrong table's row 6.
    """
    match = _EVIDENCE_TABLE.search(path.read_text(encoding="utf-8"))
    assert match is not None, f"{path}: declares a gate block but has no `## Evidence table`"
    return match.group(1)


def _row_status(table: str, row_number: str) -> str | None:
    """The Status cell of one evidence-table row, with its emphasis markers stripped."""
    for line in table.splitlines():
        if not line.startswith("|"):
            continue
        cells = line.split("|")
        if len(cells) < 4 or cells[1].strip() != row_number:
            continue
        return cells[3].strip().strip("*").strip()
    return None


def _graded_rows(table: str) -> tuple[tuple[str, str], ...]:
    """Every evidence-table row as ``(row number, status)``.

    Only rows whose first cell is a row label — ``12``, ``6a`` — are returned, so
    the header and the alignment rule drop out without needing to be recognised
    by name.
    """
    rows: list[tuple[str, str]] = []
    for line in table.splitlines():
        if not line.startswith("|"):
            continue
        cells = line.split("|")
        if len(cells) < 4:
            continue
        number = cells[1].strip()
        if not _ROW_LABEL.fullmatch(number):
            continue
        rows.append((number, cells[3].strip().strip("*").strip()))
    return tuple(rows)


def _grade_of(status: str) -> str:
    """The grade word a status cell opens with, lowercased.

    Compared against :data:`_CLEARED_GRADES` by **exact membership**, never by
    containment, because ``"met" in "partially unmet"`` is ``True``. That is the
    same substring trap that let a mutation flip this module's verdict check from
    ``NOT READY`` to ``READY`` and stay green; English negates by prefix, so any
    containment test over a grade passes for the negation of the grade.
    """
    return status.split()[0].strip("*").lower() if status.split() else ""


def _backlinks() -> tuple[tuple[Path, str, str], ...]:
    """Every ``Clears: <gate>#<condition>`` claim written anywhere under ``docs/``."""
    found: list[tuple[Path, str, str]] = []
    for path in sorted(_DOCS.rglob("*.md")):
        for match in _BACKLINK.finditer(path.read_text(encoding="utf-8")):
            found.append((path, match.group(1), match.group(2)))
    return tuple(found)


def test_the_repository_has_at_least_one_gating_document() -> None:
    """Guards every other test in this module against being vacuously green.

    The rest are parametrized over discovered blocks, so a regex that quietly stops
    matching — or a document that loses its block in a rewrite — would collect zero
    cases and report success, which is the shape this repository has already been
    burned by twice.
    """
    assert _GATE_SOURCES, (
        "no `gate` block found under docs/. If gating documents were retired, retire "
        "this check with them rather than leaving it collecting nothing."
    )


@_over_each_gate
def test_gate_block_declares_every_required_field(path: Path, body: str) -> None:
    gate = _parse_gate(path, body)
    assert gate.identifier, f"{path}: gate `id` is empty"
    assert gate.verdict, f"{path}: gate `verdict` is empty"


@_over_each_gate
def test_gate_identifiers_are_unique_across_the_repository(path: Path, body: str) -> None:
    """Backlinks address a gate by identifier, so two gates cannot share one."""
    identifier = _parse_gate(path, body).identifier
    matches = [
        other_path
        for other_path, other_body in _GATE_SOURCES
        if _parse_gate(other_path, other_body).identifier == identifier
    ]
    assert len(matches) == 1, (
        f"gate id {identifier!r} is declared by more than one document: {matches}"
    )


@_over_each_gate
def test_declared_verdict_matches_what_the_document_emphasizes(path: Path, body: str) -> None:
    """The block cannot state a verdict the document itself does not.

    Matched **exactly** against the emphasized span of a heading, not as a substring
    of one. Substring matching is vacuous in the direction that matters here: a block
    claiming ``READY`` passes a substring check against the heading "Talaria v0.1 is
    **NOT READY** as a daily driver", which inverts the verdict and reports success.
    That is not hypothetical — this test was written the substring way first, and a
    mutation flipping the block to ``READY`` stayed green.

    The convention it imposes is small and already how these documents are written:
    a gating document emphasizes its verdict where it states it.
    """
    gate = _parse_gate(path, body)
    emphasized = [
        span
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.startswith("#")
        for span in _EMPHASIZED.findall(line)
    ]
    assert gate.verdict in emphasized, (
        f"{path}: the gate block states the verdict {gate.verdict!r}, which is not the "
        f"verdict any heading of this document emphasizes (found: {emphasized})"
    )


@_over_each_gate
def test_every_condition_matches_the_evidence_row_it_names(path: Path, body: str) -> None:
    """A declared condition is grounded in a graded row, not in the block's own say-so.

    A condition with no row behind it is a condition with no grade anyone can check,
    which is the defect this whole module exists to prevent — so the block names an
    evidence-table row and the row's grade is the authority.
    """
    gate = _parse_gate(path, body)
    table = _evidence_table(path)
    for condition_id, grade in gate.conditions:
        row_number = condition_id.removeprefix("row-")
        assert row_number != condition_id, (
            f"{path}: condition id must name an evidence-table row as `row-<n>`, "
            f"got {condition_id!r}"
        )
        status = _row_status(table, row_number)
        assert status is not None, f"{path}: gate names {condition_id!r}, which is not a table row"
        assert grade in status, (
            f"{path}: the gate block grades {condition_id} as {grade!r}, but its "
            f"evidence-table row now reads {status!r}. Restate the gate against the table."
        )


@_over_each_gate
def test_no_declared_condition_has_already_been_cleared(path: Path, body: str) -> None:
    """The loop-closer, and the one check that needs nobody to remember anything.

    Editing the evidence-table row is what an author naturally does when a condition
    clears. The previous test then forces this block to follow the row, and this one
    refuses a block that still holds a verdict up on a condition the table now grades
    as settled — so the verdict gets restated instead of silently outliving its table.
    """
    gate = _parse_gate(path, body)
    for condition_id, grade in gate.conditions:
        assert grade not in _CLEARED_GRADES, (
            f"{path}: the verdict {gate.verdict!r} still blocks on {condition_id}, but the "
            f"evidence table now grades it {grade!r}. Restate the verdict and drop the "
            f"condition, rather than leaving the gate contradicting its own table."
        )


@_over_each_gate
def test_every_uncleared_row_declares_itself_as_a_condition(path: Path, body: str) -> None:
    """The inverse direction, and the one that starts mattering once a gate clears.

    Every other condition check here loops over the block's *declared* conditions,
    so all of them are vacuous for a gate that declares none. That was harmless
    while `v0-1-daily-driver` listed three; it stopped being harmless on
    2026-08-07, when the last condition cleared and the whole condition-checking
    half of this module began iterating an empty tuple for that document.

    The gap is not theoretical. It was found by mutating the verdict document in
    the direction the existing checks do not read: putting row 13 back to
    ``partially unmet`` while leaving the block with no ``blocks-on`` line at all.
    Nine tests stayed green on a gate that claimed READY over an unmet row --
    precisely the drift this module exists to refuse, arriving from the side it
    was not watching.

    So the authority runs both ways. A declared condition must name a row that
    still blocks (above), and a row that still blocks must be declared (here).
    """
    gate = _parse_gate(path, body)
    declared = {condition_id.removeprefix("row-") for condition_id, _ in gate.conditions}
    for row_number, status in _graded_rows(_evidence_table(path)):
        if _grade_of(status) in _CLEARED_GRADES:
            continue
        assert row_number in declared, (
            f"{path}: evidence-table row {row_number} is graded {status!r}, which is not a "
            f"cleared grade, but the gate block declares no `blocks-on: row-{row_number}` "
            f"for it. A verdict cannot outrun its own table by staying silent about a row."
        )


@_over_each_gate
def test_gate_has_been_re_read_within_its_declared_horizon(path: Path, body: str) -> None:
    """Deliberately time-dependent: this is the check that fires with no code change.

    Every other assertion here needs an edit to trigger it, so a gating document that
    simply sits while the world moves — exactly what DRIFT-04 did — trips none of them.
    Failing on the calendar is the cost of catching that case. The fix is to re-read
    the document against current evidence and then move `review-by`; moving the date
    without the re-read is a visible act in a diff, which silence was not.
    """
    gate = _parse_gate(path, body)
    assert date.today() <= gate.review_by, (
        f"{path}: gate {gate.identifier!r} was due for re-reading by {gate.review_by}. "
        f"Re-read its conditions against current evidence, restate what moved, then set a "
        f"new `review-by`."
    )


def test_every_backlink_names_a_gate_that_exists() -> None:
    """A misspelled backlink is worse than no backlink: it reads as a closed loop."""
    known = {_parse_gate(path, body).identifier for path, body in _GATE_SOURCES}
    for path, gate_id, condition_id in _backlinks():
        assert gate_id in known, (
            f"{path}: claims to clear {gate_id}#{condition_id}, but no document declares a "
            f"gate called {gate_id!r}. Fix the reference, or retire the claim with the gate."
        )


def test_no_backlink_claims_to_clear_a_condition_its_gate_still_blocks_on() -> None:
    """The contradiction detector: the exact shape DRIFT-04 took, made loud.

    A backlink naming a condition the gate no longer lists is the normal end state —
    the condition cleared and the verdict was restated — so only the live
    contradiction fails here.
    """
    blocking = {
        (_parse_gate(path, body).identifier, condition_id)
        for path, body in _GATE_SOURCES
        for condition_id, _ in _parse_gate(path, body).conditions
    }
    for path, gate_id, condition_id in _backlinks():
        assert (gate_id, condition_id) not in blocking, (
            f"{path}: records that {gate_id}#{condition_id} has been cleared, but that gate "
            f"still holds its verdict up on it. One of the two documents is wrong, and this "
            f"is the contradiction that went unnoticed for a day as DRIFT-04."
        )
