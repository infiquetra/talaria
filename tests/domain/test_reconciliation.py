"""R37 and AE2 — determinism under the six awkward input sequences, and the
catalogue's own completeness check.

AE2 names them: unknown event, malformed frame, duplicate, reordering, late
terminal update, missing start, another session's event. The requirement is not
that each is handled *well* — it is that each produces "the same deterministic
state on every replay". So the tests here replay each sequence twice and compare
the whole value, byte for byte.

The last test in this file is the one that keeps the catalogue honest. ADR-0003's
failure mode is silent: a rule that was read, understood, and then not re-encoded
produces a defect months later that Hermes fixed years earlier, and nothing in
the codebase points at the omission. So the catalogue document is parsed, and
every rule in it must name a test function that exists.
"""

from __future__ import annotations

import ast
from pathlib import Path
from typing import Any

import pytest

from talaria.domain.projection import project
from talaria.domain.state import SessionState, focus_session

from .conftest import raw_event, replay

REPO_ROOT = Path(__file__).resolve().parents[2]
CATALOGUE = REPO_ROOT / "docs" / "analysis" / "2026-08-02-hermes-reconciliation-rules.md"
DOMAIN_TESTS = Path(__file__).parent

#: Hermes revision every rule in the catalogue was read at. Changing the pin
#: means re-reading, so it is asserted rather than assumed.
EXPECTED_PIN = "7f4d15515"


# ── AE2: the same corpus replays to the same state ───────────────────────


def _awkward_corpus() -> list[dict[str, Any]]:
    """One corpus containing every sequence AE2 names."""
    return [
        raw_event("gateway.ready", {}, session_id=None),
        raw_event("cauldron.bubbled", {"unknown": True}),  # unknown type
        {"method": "event", "params": {"type": 7}},  # malformed frame
        raw_event("message.delta", {"text": "before any start"}),  # missing start
        raw_event("message.start"),
        raw_event("subagent.start", {"subagent_id": "sa-1", "goal": "read", "depth": 1}),
        # duplicate of the frame above
        raw_event("subagent.start", {"subagent_id": "sa-1", "goal": "read", "depth": 1}),
        raw_event("subagent.complete", {"subagent_id": "sa-1", "status": "failed"}),
        raw_event("subagent.progress", {"subagent_id": "sa-1", "text": "late"}),  # late terminal
        raw_event("message.delta", {"text": "hello"}, session_id="sess-other"),  # cross-talk
        raw_event("clarify.request", {"request_id": "req-1", "question": "Which?"}),
        raw_event("clarify.expire", {"request_id": "req-1"}),
        raw_event("message.complete", {"text": "hello", "usage": {"input_tokens": 4}}),
    ]


def test_replaying_one_corpus_twice_produces_identical_state() -> None:
    corpus = _awkward_corpus()
    first = replay(corpus)
    second = replay(corpus)
    assert repr(first) == repr(second)
    assert first == second


def test_replaying_one_corpus_twice_produces_identical_projections() -> None:
    corpus = _awkward_corpus()
    assert project(replay(corpus)) == project(replay(corpus))
    assert repr(project(replay(corpus))) == repr(project(replay(corpus)))


#: Clock reads that would make replay non-reproducible. Matched by call name
#: through the syntax tree rather than by substring: this module's own prose
#: mentions ``time.time`` several times, and a grep-based version of this check
#: fails on its own documentation.
_CLOCK_CALLS: frozenset[str] = frozenset(
    {
        "time.time",
        "time.monotonic",
        "time.perf_counter",
        "time.time_ns",
        "datetime.now",
        "datetime.utcnow",
        "date.today",
    }
)


def _dotted_name(node: ast.AST) -> str:
    if isinstance(node, ast.Attribute):
        return f"{_dotted_name(node.value)}.{node.attr}"
    if isinstance(node, ast.Name):
        return node.id
    return ""


def test_no_domain_module_reads_a_clock() -> None:
    """The cheapest way to break AE2 is a clock read below the transport
    boundary: everything still works, and every determinism test starts failing
    intermittently on a slow machine."""
    offenders: list[str] = []
    for path in sorted((REPO_ROOT / "talaria" / "domain").rglob("*.py")):
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if not isinstance(node, ast.Call):
                continue
            name = _dotted_name(node.func)
            if name in _CLOCK_CALLS or name.rsplit(".", 1)[-1] in {"utcnow", "monotonic"}:
                offenders.append(f"{path.name}:{node.lineno} calls {name}()")
    assert not offenders, f"the domain core must take time as an argument: {offenders}"


# ── The individual awkward sequences ─────────────────────────────────────


def test_another_sessions_event_mutates_nothing() -> None:
    """``createGatewayEventHandler.ts:720-722``, three lines that stop a second
    conversation's stream appearing in this one."""
    focused = replay([raw_event("message.start")])
    after = replay(
        [
            raw_event("message.delta", {"text": "not mine"}, session_id="sess-other"),
            raw_event("subagent.start", {"subagent_id": "x"}, session_id="sess-other"),
            raw_event("clarify.request", {"request_id": "r"}, session_id="sess-other"),
        ],
        focused,
    )
    assert after.transcript == focused.transcript
    assert after.streaming_text == focused.streaming_text
    assert after.subagents == () and after.prompts == ()
    assert after.cross_session_events_ignored == 3


def test_a_gateway_event_is_never_session_scoped() -> None:
    """Transport-level events describe the socket, not a conversation."""
    state = replay([raw_event("message.start")])
    after = replay([raw_event("gateway.ready", {}, session_id="sess-other")], state)
    assert after.connection == "connected"


def test_a_duplicated_frame_lands_in_the_catalogued_outcome() -> None:
    """Idempotent where the payload carries an identity, additive where it does
    not. A ``subagent.start`` names its child, so a repeat updates one row; a
    ``message.delta`` carries no identity, so a repeat is more text — which is
    correct, because the gateway does not retransmit deltas."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("subagent.start", {"subagent_id": "sa-1", "goal": "read"}),
            raw_event("subagent.start", {"subagent_id": "sa-1", "goal": "read"}),
            raw_event("message.delta", {"text": "ab"}),
            raw_event("message.delta", {"text": "ab"}),
        ]
    )
    assert len(state.subagents) == 1
    assert state.streaming_text == "abab"


def test_reordered_subagent_events_converge_on_the_same_rows() -> None:
    """Identity plus terminal protection make the fan-out order-insensitive for
    everything except which detail line landed last."""
    forward = replay(
        [
            raw_event("subagent.spawn_requested", {"subagent_id": "sa-1", "goal": "g", "depth": 0}),
            raw_event("subagent.start", {"subagent_id": "sa-1", "goal": "g", "depth": 0}),
            raw_event("subagent.complete", {"subagent_id": "sa-1", "status": "completed"}),
        ]
    )
    reordered = replay(
        [
            raw_event("subagent.spawn_requested", {"subagent_id": "sa-1", "goal": "g", "depth": 0}),
            raw_event("subagent.complete", {"subagent_id": "sa-1", "status": "completed"}),
            raw_event("subagent.start", {"subagent_id": "sa-1", "goal": "g", "depth": 0}),
        ]
    )
    assert [(r.id, r.status) for r in forward.subagents] == [
        (r.id, r.status) for r in reordered.subagents
    ]


def test_focusing_a_new_session_drops_the_previous_sessions_state() -> None:
    """``turnController.reset()`` (``:918-938``) — its comment names the failure
    it prevents, session A's state bleeding into session B."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "session A"}),
            raw_event("subagent.start", {"subagent_id": "sa-1", "goal": "g"}),
            raw_event("sudo.request", {"request_id": "req-1"}),
        ]
    )
    switched = focus_session(state, "sess-b")
    assert switched.subagents == ()
    assert switched.prompts == ()
    assert switched.streaming_text == ""
    assert switched.turn == "idle"
    assert switched.transcript == state.transcript, "history is kept; live state is not"


def test_a_malformed_element_inside_a_list_payload_is_skipped_not_fatal() -> None:
    """Re-encodes the shape of ``parseTodos`` (``turnController.ts:51-76``):
    reject the element, keep the frame. A frame-level rejection would lose the
    question along with the bad choice."""
    state = replay(
        [
            raw_event(
                "clarify.request",
                {"request_id": "req-1", "question": "Which?", "choices": ["a", 7, None, "b"]},
            )
        ]
    )
    assert state.prompts[0].choices == ("a", "b")
    assert state.prompts[0].summary == "Which?"


def test_a_notification_lands_as_a_plain_system_line() -> None:
    """Hermes runs a whole notice state machine — hold while busy, latest-wins,
    key-matched clear, TTL, session-boundary clear (``turnController.ts:173-265``,
    ``:934-936``). v0.1 has no notice surface to hold anything for, so the text
    is committed and the machine is dropped."""
    state = replay(
        [
            raw_event(
                "notification.show",
                {"key": "credits.warn90", "text": "90% of credits used"},
            ),
            raw_event("notification.clear", {"key": "credits.warn90"}),
        ]
    )
    assert [e.text for e in state.transcript if e.kind == "system"] == ["90% of credits used"]


def test_inline_diff_content_is_preserved_as_plain_text() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event(
                "tool.complete",
                {
                    "tool_id": "t1",
                    "name": "edit_file",
                    "summary": "1 hunk",
                    "inline_diff": "┊ review diff\n--- a/x\n+++ b/x\n-old\n+new",
                },
            ),
        ]
    )
    tool_text = "\n".join(e.text for e in state.transcript if e.kind == "tool")
    assert "┊" not in tool_text
    assert "-old" in tool_text and "+new" in tool_text


def test_ambient_events_are_decoded_and_deliberately_not_rendered() -> None:
    state = replay(
        [
            raw_event("reaction", {}),
            raw_event("skin.changed", {"accent": "#fff"}),
            raw_event("voice.status", {"state": "listening"}),
            raw_event("moa.progress", {"refs_done": 1, "refs_total": 3}),
        ]
    )
    assert state.transcript == ()
    assert state.unknown_event_types == (), "these are known, just not rendered"


# ── The catalogue keeps itself honest ────────────────────────────────────


def _catalogue_rows() -> list[tuple[str, str]]:
    """Every ``| RR-nn | … | `test_name` |`` row, as (rule id, test name)."""
    rows: list[tuple[str, str]] = []
    for line in CATALOGUE.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped.startswith("| RR-"):
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        rule_id = cells[0]
        test_name = cells[-1].strip("`")
        rows.append((rule_id, test_name))
    return rows


def _defined_test_names() -> set[str]:
    names: set[str] = set()
    for path in sorted(DOMAIN_TESTS.glob("test_*.py")):
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.startswith("def test_"):
                names.add(line[len("def ") :].split("(")[0])
    return names


def test_the_catalogue_exists_and_is_revision_pinned() -> None:
    assert CATALOGUE.is_file(), f"the reconciliation catalogue is missing: {CATALOGUE}"
    text = CATALOGUE.read_text(encoding="utf-8")
    assert EXPECTED_PIN in text, "the catalogue must name the revision it was read at"


def test_every_catalogued_rule_names_a_test_that_exists() -> None:
    rows = _catalogue_rows()
    assert len(rows) >= 30, f"the catalogue looks truncated: {len(rows)} rules parsed"

    defined = _defined_test_names()
    missing = [(rule, name) for rule, name in rows if name not in defined]
    assert not missing, (
        "every reconciliation rule must name a test that exists in tests/domain/; "
        f"unmatched: {missing}"
    )


def test_catalogued_rule_ids_are_unique_and_contiguous() -> None:
    ids = [rule for rule, _ in _catalogue_rows()]
    assert len(ids) == len(set(ids)), "duplicate rule id in the catalogue"
    numbers = sorted(int(rule.split("-")[1]) for rule in ids)
    assert numbers == list(range(1, len(numbers) + 1)), (
        "rule ids must run RR-01..RR-nn with no gaps, so a deleted rule is visible"
    )


@pytest.mark.parametrize("state", [SessionState()])
def test_an_empty_state_projects_without_error(state: SessionState) -> None:
    snapshot = project(state)
    assert snapshot.transcript.lines == ()
    assert snapshot.status.turn == "idle"
