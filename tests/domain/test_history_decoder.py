"""The history decoder's contract, pinned against replies a real Hermes sent.

``session.resume``'s ``messages`` array is **not** a uniform ``{role, text}``
list, and the whole point of a typed decoder is that the differences are pinned
rather than assumed. This module is where they are pinned.

**Provenance, stated per fixture.** Three live ``session.resume`` replies exist
in the local recording set, matched to their calls on the JSON-RPC ``id``, and
they carry three of the element shapes below. They are cited by digest and never
by path (R29):

* ``talaria-live-v1-21f-4d52b5b4a750`` — frame-log v1, recorded
  2026-08-07T00:13:34.796Z. Eight messages, ``messages_omitted: false``. Carries
  the text row and the reasoning-carrying assistant row.
* ``talaria-live-v1-17f-0c4c759ed094`` — frame-log v1, recorded
  2026-08-07T00:14:40.872Z. Eight messages, same two shapes.
* ``talaria-live-v1-19f-8ef7ecabfc6f`` — frame-log v1, recorded
  2026-08-07T15:53:30.946Z. Three messages, ``messages_omitted: false``.
  Carries the tool row.

The **display-metadata row** and the **``system`` role** are not in any of the
three. They are transcribed from the gateway's own builder,
``_history_to_messages`` (``tui_gateway/server.py:7078-7167``) and the
``model_switch`` writer (``server.py:3941-3990``), and are marked as
source-transcribed rather than recorded in :data:`ELEMENT_FIXTURES`. That
distinction is the honest part: a shape read from source could be misread, and
a fixture that hid its own provenance would make the misreading invisible. A
live capture that carries one of those two rows should replace the fixture and
move its provenance to ``recorded``.

**Kinds and structure, never recorded values.** Every fixture below is rebuilt
from the recorded key set with neutral placeholder text. No operator message,
path, or reasoning content is transcribed here — this is a public repository,
and a resume reply is the single richest carrier of private content the gateway
sends.
"""

from __future__ import annotations

from typing import Any, Literal

import pytest

from talaria.domain.history import (
    EVENT_DISPLAY_KINDS,
    HISTORY_ROLES,
    REASONING_KEYS,
    UNKNOWN_ROLE_PREFIX,
    UNREADABLE_ELEMENT_PREFIX,
    decode_history,
    element_count,
)

Provenance = Literal["recorded", "source-transcribed"]

#: ``(name, provenance, element, expected lines)``. The element is the recorded
#: *shape* filled with neutral values; the expectation is the whole decoded
#: output for that element, in order, so a decoder that emits an extra line or
#: reorders two fails rather than passing on a substring match.
Fixture = tuple[str, Provenance, dict[str, Any], tuple[tuple[str, str], ...]]

ELEMENT_FIXTURES: tuple[Fixture, ...] = (
    (
        # {"role": "user", "text": str, "row_id": int}
        "user-text-row",
        "recorded",
        {"role": "user", "text": "a question", "row_id": 41},
        (("user", "a question"),),
    ),
    (
        # {"role": "assistant", "text": str, "row_id": int}
        "assistant-text-row",
        "recorded",
        {"role": "assistant", "text": "an answer", "row_id": 42},
        (("assistant", "an answer"),),
    ),
    (
        # The recorded reasoning-carrying assistant row: text *and* reasoning,
        # with the codex payload alongside. Two lines, reasoning first, which is
        # the order the live path commits them in.
        "assistant-with-reasoning",
        "recorded",
        {
            "role": "assistant",
            "text": "an answer",
            "row_id": 43,
            "reasoning": "the thinking",
            "reasoning_content": "the thinking",
            "codex_reasoning_items": [
                {
                    "type": "reasoning",
                    "id": "rs-1",
                    "summary": [],
                    "encrypted_content": "AAAA",
                    "_issuer_kind": "codex",
                }
            ],
        },
        (("reasoning", "the thinking"), ("assistant", "an answer")),
    ),
    (
        # {"role": "tool", "name": str, "context": str} — no ``text``, no
        # ``row_id``. A decoder that reads ``text`` renders this as blank.
        "tool-row",
        "recorded",
        {"role": "tool", "name": "read_file", "context": "src/main.py"},
        (("tool", "⏺ read_file src/main.py"),),
    ),
    (
        # ``server.py:7120-7135`` keeps an assistant row whose visible text is
        # empty when it carries reasoning (#44022). Reasoning-only, one line.
        "reasoning-only-row",
        "source-transcribed",
        {"role": "assistant", "text": "", "row_id": 44, "reasoning": "the thinking"},
        (("reasoning", "the thinking"),),
    ),
    (
        # A model switch is persisted as a *user* row (``server.py:3951``)
        # because strict providers reject a late system message. Rendering it by
        # its role puts gateway scaffolding on screen as the operator's words.
        "display-kind-model-switch",
        "source-transcribed",
        {"role": "user", "text": "[Model switched to hermes-4]", "display_kind": "model_switch"},
        (("system", "[Model switched to hermes-4]"),),
    ),
    (
        # ``display_metadata``'s counts are the content of a delegation
        # completion (``server.py:9407-9430``), so they are rendered, not
        # dropped.
        "display-metadata-row",
        "source-transcribed",
        {
            "role": "user",
            "text": "delegation finished",
            "display_kind": "async_delegation_complete",
            "display_metadata": {"task_count": 3, "completed_count": 2, "failed_count": 1},
        },
        (
            (
                "system",
                "delegation finished completed_count=2 failed_count=1 task_count=3",
            ),
        ),
    ),
    (
        # ``skill_invocation`` is deliberately *not* an event kind: the row is
        # the operator's own line with the expanded body replaced by what they
        # typed (``server.py:7145-7154``).
        "skill-invocation-stays-a-user-line",
        "source-transcribed",
        {"role": "user", "text": "/deploy", "display_kind": "skill_invocation", "row_id": 45},
        (("user", "/deploy"),),
    ),
    (
        "system-row",
        "source-transcribed",
        {"role": "system", "text": "a system line"},
        (("system", "a system line"),),
    ),
)


@pytest.mark.parametrize(
    ("name", "provenance", "element", "expected"),
    ELEMENT_FIXTURES,
    ids=[fixture[0] for fixture in ELEMENT_FIXTURES],
)
def test_each_recorded_element_shape_decodes_to_its_pinned_lines(
    name: str,
    provenance: Provenance,
    element: dict[str, Any],
    expected: tuple[tuple[str, str], ...],
) -> None:
    assert decode_history([element]) == expected, f"{name} ({provenance}) drifted"


def test_at_least_three_fixtures_come_from_live_replies_rather_than_from_source() -> None:
    """The contract's own evidence check.

    Without this, every fixture could quietly become source-transcribed — a
    decoder pinned entirely against a reading of the gateway's code, which is
    the same reading that wrote the decoder, and therefore proves nothing about
    what the gateway sends.
    """
    recorded = [name for name, provenance, _, _ in ELEMENT_FIXTURES if provenance == "recorded"]
    assert len(recorded) >= 3, recorded


# ── the unknown-event posture: literal text, nothing dropped ─────────────


@pytest.mark.parametrize(
    "element",
    ["a bare string", 7, None, ["nested"], True],
    ids=["str", "int", "null", "list", "bool"],
)
def test_a_malformed_element_lands_as_an_unknown_event_carrying_its_literal(
    element: Any,
) -> None:
    lines = decode_history([element])
    assert len(lines) == 1
    kind, text = lines[0]
    assert kind == "unknown-event"
    assert text.startswith(UNREADABLE_ELEMENT_PREFIX)
    assert text != UNREADABLE_ELEMENT_PREFIX, "the literal was dropped"


def test_an_unknown_role_lands_as_an_unknown_event_keeping_its_text() -> None:
    """A role Talaria has not met is reported, not guessed at and not skipped."""
    lines = decode_history([{"role": "oracle", "text": "the prophecy"}])
    assert len(lines) == 1
    kind, text = lines[0]
    assert kind == "unknown-event"
    assert text.startswith(UNKNOWN_ROLE_PREFIX)
    assert "oracle" in text
    assert "the prophecy" in text, "the element's own text was dropped"


def test_an_element_with_no_role_at_all_is_reported_rather_than_skipped() -> None:
    lines = decode_history([{"text": "orphaned"}])
    assert [kind for kind, _ in lines] == ["unknown-event"]
    assert "orphaned" in lines[0][1]


def test_a_system_row_with_no_text_survives_as_unreadable_rather_than_vanishing() -> None:
    """CR6 finding 3: the module's own contract ('Nothing is dropped') was
    broken by exactly the case the ``user`` branch already handled correctly
    — a recognised role carrying no renderable ``text`` used to disappear
    (``return ()``) instead of surfacing as an unknown-event line."""
    lines = decode_history([{"role": "system", "details": "must survive"}])
    assert len(lines) == 1
    kind, text = lines[0]
    assert kind == "unknown-event"
    assert text.startswith(UNREADABLE_ELEMENT_PREFIX)
    assert "must survive" in text


def test_a_bare_tool_row_survives_as_unreadable_rather_than_inventing_a_line() -> None:
    """The companion half of the same finding: a tool row with neither
    ``name`` nor ``context`` used to render the invented line ``"⏺ tool"``,
    discarding every other key a malformed element carried."""
    lines = decode_history([{"role": "tool", "details": "must survive"}])
    assert len(lines) == 1
    kind, text = lines[0]
    assert kind == "unknown-event"
    assert text.startswith(UNREADABLE_ELEMENT_PREFIX)
    assert "must survive" in text
    assert "⏺ tool" not in text


def test_the_modelled_roles_are_exactly_the_ones_the_gateway_emits() -> None:
    """``server.py:7086`` filters the history to these four before sending."""
    assert HISTORY_ROLES == frozenset({"user", "assistant", "tool", "system"})


def test_the_reasoning_keys_are_the_four_the_gateway_forwards() -> None:
    """``server.py:7124-7129``. A key missing here makes a kept reasoning-only
    row decode as an unreadable element instead of as reasoning."""
    assert set(REASONING_KEYS) == {
        "reasoning",
        "reasoning_content",
        "reasoning_details",
        "codex_reasoning_items",
    }


def test_an_assistant_row_whose_only_reasoning_is_encrypted_still_shows_a_line() -> None:
    """The codex payload is not prose, so its content is withheld — but the row
    the gateway deliberately kept is not silently discarded."""
    lines = decode_history(
        [
            {
                "role": "assistant",
                "text": "",
                "codex_reasoning_items": [{"type": "reasoning", "encrypted_content": "AAAA"}],
            }
        ]
    )
    assert [kind for kind, _ in lines] == ["reasoning"]
    assert "AAAA" not in lines[0][1], "an encrypted blob reached the transcript"


# ── CR6 finding 2: resumed text must not be clipped where live text is not ─


def test_a_long_resumed_assistant_body_is_not_clipped() -> None:
    """The live ``message.complete`` reducer (``talaria/domain/state.py``)
    appends the whole assistant reply uncapped; the resumed path used to run
    the same text through ``clip_transcript_line`` (2,000 characters), so the
    same conversation read two different lengths depending on whether it
    arrived seeded or live."""
    body = "x" * 2500
    lines = decode_history([{"role": "assistant", "text": body}])
    assert lines == (("assistant", body),)
    assert "…" not in lines[0][1]


def test_a_long_resumed_reasoning_body_is_not_clipped() -> None:
    body = "y" * 2500
    lines = decode_history([{"role": "assistant", "text": "", "reasoning": body}])
    assert lines == (("reasoning", body),)
    assert "…" not in lines[0][1]


def test_the_unknown_event_literal_is_not_clipped() -> None:
    """The live ``_apply_unknown_event`` reducer appends ``frame.text``
    uncapped; ``_unreadable`` used to clip the same literal on the resumed
    path."""
    body = "z" * 2500
    lines = decode_history([body])
    assert len(lines) == 1
    kind, text = lines[0]
    assert kind == "unknown-event"
    assert body in text
    assert "…" not in text


def test_the_event_display_kinds_exclude_the_two_that_are_not_events() -> None:
    assert "skill_invocation" not in EVENT_DISPLAY_KINDS


# ── D1: the user line and the unknown-role literal were still clipped ────


def test_a_long_resumed_user_body_is_not_clipped() -> None:
    """The live ``record_submission`` writer (``talaria/domain/state.py``)
    appends the operator's own words with ``_append(state, "user", text)``,
    uncapped; the resumed ``role == "user"`` path still ran the same text
    through ``clip_transcript_line`` (2,000 characters), so a long prompt
    read two different lengths depending on whether it arrived seeded or
    live — the one clip the round-two unclipping pass (CR6 finding 2, the
    three tests above) missed."""
    body = "u" * 2500
    lines = decode_history([{"role": "user", "text": body}])
    assert lines == (("user", body),)
    assert "…" not in lines[0][1]


def test_the_unknown_role_literal_is_not_clipped() -> None:
    """The live ``_apply_unknown_event`` reducer appends ``frame.text``
    uncapped, and ``_unreadable`` already matches that for a shape this
    decoder cannot read at all — but an element with a *role* the decoder
    does not model took a different path, ``_unknown_role``, which still
    clipped the same "unknown-event" kind. Both must read alike."""
    body = "r" * 2500
    lines = decode_history([{"role": "nonsense", "text": body}])
    assert len(lines) == 1
    kind, text = lines[0]
    assert kind == "unknown-event"
    assert body in text
    assert "…" not in text
    assert "hidden" not in EVENT_DISPLAY_KINDS, (
        "the gateway drops hidden rows before the reply is built (server.py:7093)"
    )


# ── totality ─────────────────────────────────────────────────────────────


@pytest.mark.parametrize(
    "messages", [None, {}, "not a list", 7, True], ids=["null", "object", "str", "int", "bool"]
)
def test_a_messages_field_that_is_not_an_array_decodes_to_nothing(messages: Any) -> None:
    """Total by construction: a startup path that raises on a surprising reply
    is a client that will not open."""
    assert decode_history(messages) == ()
    assert element_count(messages) == 0


def test_element_count_counts_elements_not_decoded_lines() -> None:
    """The withheld arithmetic compares against ``message_count``, which counts
    elements — and one assistant element can decode to two lines."""
    messages = [
        {"role": "user", "text": "q"},
        {"role": "assistant", "text": "a", "reasoning": "r"},
    ]
    assert element_count(messages) == 2
    assert len(decode_history(messages)) == 3


def test_a_whole_recorded_reply_decodes_in_wire_order() -> None:
    """The three-message recording's shape end to end
    (``talaria-live-v1-19f-8ef7ecabfc6f``): user, tool, assistant."""
    lines = decode_history(
        [
            {"role": "user", "text": "q", "row_id": 1},
            {"role": "tool", "name": "grep", "context": "pattern"},
            {"role": "assistant", "text": "a", "row_id": 2},
        ]
    )
    assert [kind for kind, _ in lines] == ["user", "tool", "assistant"]
