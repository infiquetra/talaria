"""The ``session.list`` decoder's contract (R7, R10, U7).

``session.list``'s reply is pinned top-level in ``talaria/domain/compat.py``
as ``{"sessions": "list"}`` — the row shape one level down is this module's
job, mirroring ``tests/domain/test_history_decoder.py``'s provenance
discipline for U6's history decoder.

**Provenance.** No live ``session.list`` recording exists in the local corpus
at the time this test was written. The fixture below is transcribed from the
handler at the pin (``tui_gateway/methods_session.py:162-208``,
``BASELINE_PIN`` in ``talaria/domain/compat.py``) rather than recorded from a
live reply, and is labelled ``source-transcribed`` for that reason — a live
capture that carries this reply should replace it and move the label to
``recorded``.

**Kinds and structure, never real values.** The fixture below uses invented
placeholder text throughout (R12) — this is a public repository, and a session
title is exactly the kind of content that names what an operator is working
on.
"""

from __future__ import annotations

from typing import Any

import pytest

from talaria.domain.session_list import (
    SessionDirectory,
    SessionSummary,
    decode_session_list,
)

#: Transcribed from ``methods_session.py:195-207`` at the pin — every key the
#: handler's row comprehension sets, filled with invented placeholder values.
#: Provenance: source-transcribed.
SESSION_LIST_ROW: dict[str, Any] = {
    "id": "s-fixture-042",
    "title": "sketch the export format",
    "preview": "let's pick this back up",
    "started_at": 1785000100.0,
    "message_count": 6,
    "source": "cli",
}


def test_a_well_formed_row_decodes_every_field() -> None:
    directory = decode_session_list({"sessions": [SESSION_LIST_ROW]})
    assert directory.sessions == (
        SessionSummary(
            session_id="s-fixture-042",
            title="sketch the export format",
            preview="let's pick this back up",
            started_at=1785000100.0,
            message_count=6,
            source="cli",
        ),
    )


def test_row_order_is_preserved() -> None:
    first = {**SESSION_LIST_ROW, "id": "s-fixture-001"}
    second = {**SESSION_LIST_ROW, "id": "s-fixture-002"}
    directory = decode_session_list({"sessions": [first, second]})
    assert [row.session_id for row in directory.sessions] == [
        "s-fixture-001",
        "s-fixture-002",
    ]


@pytest.mark.parametrize(
    "result",
    [
        "not a mapping",
        None,
        [],
        7,
        {"sessions": "not a list"},
        {"sessions": None},
        {},
    ],
)
def test_a_malformed_reply_degrades_to_an_empty_directory_rather_than_raising(
    result: Any,
) -> None:
    directory = decode_session_list(result)
    assert directory == SessionDirectory()
    assert directory.is_empty


def test_a_row_missing_its_stored_id_is_dropped_not_kept_as_a_placeholder() -> None:
    """Unlike U6's history decoder, a row nobody could select is noise, not
    information the operator needs to see — there is no ``unknown-event``
    posture here."""
    good = SESSION_LIST_ROW
    no_id = {**SESSION_LIST_ROW, "id": ""}
    missing_id = {k: v for k, v in SESSION_LIST_ROW.items() if k != "id"}
    directory = decode_session_list({"sessions": [good, no_id, missing_id]})
    assert [row.session_id for row in directory.sessions] == ["s-fixture-042"]


def test_a_row_that_is_not_a_mapping_is_dropped_and_costs_nothing_else() -> None:
    directory = decode_session_list(
        {"sessions": [SESSION_LIST_ROW, "not a mapping", 7, None]}
    )
    assert len(directory.sessions) == 1


def test_missing_optional_fields_default_rather_than_raise() -> None:
    directory = decode_session_list({"sessions": [{"id": "s-fixture-bare"}]})
    assert directory.sessions == (SessionSummary(session_id="s-fixture-bare"),)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("title", 7),
        ("preview", None),
        ("started_at", "not a number"),
        ("message_count", "not a count"),
        ("message_count", True),
        ("source", []),
    ],
)
def test_a_field_of_the_wrong_kind_degrades_to_its_default(field: str, value: Any) -> None:
    row = {**SESSION_LIST_ROW, field: value}
    directory = decode_session_list({"sessions": [row]})
    decoded = directory.sessions[0]
    default = SessionSummary(session_id="ignored")
    assert getattr(decoded, field) == getattr(default, field)


def test_an_empty_but_well_formed_listing_is_empty_rather_than_a_failure() -> None:
    directory = decode_session_list({"sessions": []})
    assert directory == SessionDirectory()
    assert directory.is_empty


@pytest.mark.parametrize("started_at", [float("nan"), float("inf"), float("-inf"), 1e300, -1e300])
def test_a_timestamp_datetime_cannot_represent_degrades_to_the_missing_value(
    started_at: float,
) -> None:
    """P2, U7 round two. ``started_at`` reaches
    ``datetime.fromtimestamp`` in :func:`~talaria.ui.picker.format_session_detail`
    — NaN, an infinity, or a magnitude past ``datetime``'s year 1-9999 bound
    raised ``OverflowError``/``ValueError`` there before the picker dialog
    ever opened. Folded into the same 0.0 this decoder already returns for a
    missing or wrong-typed timestamp, not treated as a new failure shape."""
    row = {**SESSION_LIST_ROW, "started_at": started_at}
    directory = decode_session_list({"sessions": [row]})
    assert directory.sessions[0].started_at == 0.0
