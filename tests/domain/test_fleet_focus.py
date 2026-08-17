"""U4's focus-movement domain: fleet-scoped answering, the steal test, the handover.

Three mechanisms, all of them decisions the UI merely reports:

*The answering set is fleet-scoped* (KTD9/R6). An answer travelling for a
session Talaria is not looking at occupies the switch-refusal window exactly as
a focused one does, and protects its registry row from retirement and eviction
for the length of the round trip. The protection half is written against the
contract in :meth:`~talaria.domain.state.FleetState.protected_keys`, including
the shape that docstring says a single-rebind test cannot catch.

*The confirm fires for live sessions this run did not open* (OP2/KTD8), and for
nothing else — a historical session, an already-driven session, and a session
with no row at all each resume without a dialog.

*An attach recovers the two hydratable kinds and visibly fails the rest*
(KTD8). The reply's ``pending_approval``/``pending_clarify`` become ordinary
request events; anything else the session was waiting on latches a
resolved-failed on its row.

Frames are built raw and pushed through :mod:`talaria.domain.decode`, the same
discipline ``tests/domain/test_registry.py`` keeps.
"""

from __future__ import annotations

from dataclasses import replace
from typing import Any

from talaria.domain.decode import decode_frame
from talaria.domain.models import PendingPrompt
from talaria.domain.registry import RegistryRow, attach_displaces_client
from talaria.domain.session_list import decode_active_list, decode_session_list
from talaria.domain.state import (
    REFUSED_SWITCH_WHILE_ANSWERING,
    REFUSED_SWITCH_WHILE_FLEET_ANSWERING,
    FleetState,
    activation_hydration_events,
    apply_active_list,
    apply_frame,
    attach_confirm_row,
    attach_residue_notice,
    begin_fleet_answer,
    end_fleet_answer,
    fleet_switch_refusal,
    latch_attach_residue,
    mark_we_drive,
    route_frame,
    route_frames,
    seed_from_listing,
)

from .conftest import BASE_TIME, raw_event

FOCUSED_PROFILE = "default"
OTHER_PROFILE = "beta"


def decoded(frame: Any, *, at: float, seq: int) -> Any:
    return decode_frame(frame, at=at, seq=seq)


def fleet_with_focused_session() -> FleetState:
    """A fleet whose focused engine adopted ``sess-focus`` through a real turn."""
    frames = [
        raw_event("message.start"),
        raw_event("message.delta", {"text": "Hello"}),
        raw_event("message.complete", {"text": "Hello"}),
    ]
    fleet = FleetState(focused_profile=FOCUSED_PROFILE)
    return route_frames(
        fleet,
        [decoded(f, at=BASE_TIME + i, seq=i + 1) for i, f in enumerate(frames)],
        profile=FOCUSED_PROFILE,
    )


def active_list(
    runtime: str,
    *,
    durable: str,
    status: str = "idle",
    at: float,
    title: str = "t",
) -> Any:
    return decode_active_list(
        {
            "sessions": [
                {
                    "id": runtime,
                    "session_key": durable,
                    "status": status,
                    "last_active": at,
                    "started_at": BASE_TIME,
                    "message_count": 1,
                    "model": "m",
                    "preview": "p",
                    "title": title,
                }
            ]
        }
    )


# ── the answering set is fleet-scoped (KTD9, R6) ─────────────────────────


def test_an_answer_for_a_background_session_refuses_a_switch() -> None:
    """R6's fleet half, and the reason KTD9 exists at all.

    The focused engine's ``answering`` tuple is the *focused* session's
    bookkeeping — an answer the queue aims at a session Talaria does not drive
    never enters it. Asserted with that tuple provably empty, so this cannot
    pass on the shipped focused-only rule.
    """
    fleet = fleet_with_focused_session()
    assert fleet.focused.answering == ()
    assert fleet_switch_refusal(fleet) == ""

    fleet = begin_fleet_answer(
        fleet, profile=OTHER_PROFILE, session_id="durable-1", request_key="req-1"
    )
    assert fleet.focused.answering == (), "the focused engine must not have moved"
    assert fleet_switch_refusal(fleet) == REFUSED_SWITCH_WHILE_FLEET_ANSWERING

    fleet = end_fleet_answer(
        fleet, profile=OTHER_PROFILE, session_id="durable-1", request_key="req-1"
    )
    assert fleet_switch_refusal(fleet) == ""


def test_the_focused_sentence_is_still_what_a_focused_answer_says() -> None:
    """The shipped refusal keeps its shipped words. Both are true when a
    focused answer is out; the operator is looking at that card, so that is
    the sentence they get."""
    fleet = fleet_with_focused_session()
    prompt = PendingPrompt(
        request_id="req-focused",
        kind="approval",
        summary="a dangerous command",
        opened_at=BASE_TIME,
        seq=4,
        session_id="sess-focus",
    )
    fleet = replace(fleet, focused=replace(fleet.focused, answering=(prompt,)))
    fleet = begin_fleet_answer(
        fleet, profile=FOCUSED_PROFILE, session_id="sess-focus", request_key="req-focused"
    )

    assert fleet_switch_refusal(fleet) == REFUSED_SWITCH_WHILE_ANSWERING


def test_two_answers_on_one_row_release_it_only_when_both_end() -> None:
    """A session can block on two approvals at once — the gateway's per-session
    structure is a queue, not a slot (U1). The protection is derived from what
    is still outstanding rather than from a counter, so the second answer
    ending is what releases the row."""
    fleet = fleet_with_focused_session()
    fleet = begin_fleet_answer(
        fleet, profile=OTHER_PROFILE, session_id="durable-1", request_key="req-1"
    )
    fleet = begin_fleet_answer(
        fleet, profile=OTHER_PROFILE, session_id="durable-1", request_key="req-2"
    )
    assert fleet.answering_keys == frozenset({(OTHER_PROFILE, "durable-1")})

    fleet = end_fleet_answer(
        fleet, profile=OTHER_PROFILE, session_id="durable-1", request_key="req-1"
    )
    assert fleet.answering_keys == frozenset({(OTHER_PROFILE, "durable-1")})

    fleet = end_fleet_answer(
        fleet, profile=OTHER_PROFILE, session_id="durable-1", request_key="req-2"
    )
    assert fleet.answering_keys == frozenset()
    assert fleet_switch_refusal(fleet) == ""


def test_an_in_flight_answer_survives_rebind_then_alias_churn() -> None:
    """The shape :meth:`FleetState.protected_keys` says a single-rebind test
    cannot catch — written for the answering set this unit feeds.

    An event creates the row under a runtime id; the answer is recorded there;
    the first poll teaches the durable identity and *moves* the row. Read-time
    resolution through the alias map carries the protection across that move —
    and then four more resumes age the original runtime id out of the row's
    four-slot window, the bound reclaims its alias entry, and resolution alone
    has nothing left to follow. Only the re-anchoring at the rebind keeps the
    row protected, which is why deleting it would reopen the defect.
    """
    fleet = fleet_with_focused_session()
    fleet = route_frame(
        fleet,
        decoded(raw_event("message.start", session_id="r1"), at=BASE_TIME + 5, seq=9),
        profile=OTHER_PROFILE,
        generation=1,
    )
    assert (OTHER_PROFILE, "r1") in fleet.rows

    fleet = begin_fleet_answer(
        fleet, profile=OTHER_PROFILE, session_id="r1", request_key="req-1"
    )
    fleet = apply_active_list(
        fleet,
        active_list("r1", durable="durable-1", at=BASE_TIME + 6),
        profile=OTHER_PROFILE,
        at=BASE_TIME + 6,
        poll_epoch=1,
    )
    durable_key = (OTHER_PROFILE, "durable-1")
    assert durable_key in fleet.rows and (OTHER_PROFILE, "r1") not in fleet.rows
    assert durable_key in fleet.protected_keys()

    for index, runtime in enumerate(("r2", "r3", "r4", "r5")):
        fleet = route_frame(
            fleet,
            decoded(
                raw_event("message.start", session_id=runtime),
                at=BASE_TIME + 10 + index,
                seq=20 + index,
            ),
            profile=OTHER_PROFILE,
            generation=1,
        )
        fleet = apply_active_list(
            fleet,
            active_list(runtime, durable="durable-1", at=BASE_TIME + 10 + index),
            profile=OTHER_PROFILE,
            at=BASE_TIME + 10 + index,
            poll_epoch=1,
        )

    assert "r1" not in fleet.rows[durable_key].runtime_ids
    assert (OTHER_PROFILE, "r1") not in fleet.aliases
    assert durable_key in fleet.protected_keys(), (
        "the answer's protection did not survive the alias trim"
    )

    # And the dual-listing sweep that mentions it in neither listing leaves it.
    fleet = seed_from_listing(
        fleet,
        decode_session_list({"sessions": []}),
        profile=OTHER_PROFILE,
        at=BASE_TIME + 40,
        poll_epoch=2,
    )
    fleet = apply_active_list(
        fleet,
        decode_active_list({"sessions": []}),
        profile=OTHER_PROFILE,
        at=BASE_TIME + 40,
        poll_epoch=2,
    )
    assert durable_key in fleet.rows, "a row with an answer in flight was retired"


# ── OP2: which attaches are confirmed, and which are not (KTD8) ───────────


def _fleet_with_rows() -> FleetState:
    """One historical row and one live row, both foreign, on one connection."""
    fleet = fleet_with_focused_session()
    fleet = seed_from_listing(
        fleet,
        decode_session_list(
            {
                "sessions": [
                    {"id": "hist-1", "title": "yesterday", "started_at": BASE_TIME},
                    {"id": "live-1", "title": "somebody else's", "started_at": BASE_TIME},
                ]
            }
        ),
        profile=FOCUSED_PROFILE,
        at=BASE_TIME + 50,
        poll_epoch=1,
    )
    return apply_active_list(
        fleet,
        active_list(
            "live-1-runtime",
            durable="live-1",
            status="waiting",
            at=BASE_TIME + 50,
            title="somebody else's",
        ),
        profile=FOCUSED_PROFILE,
        at=BASE_TIME + 50,
        poll_epoch=1,
    )


def test_a_live_session_this_run_did_not_open_is_confirmed() -> None:
    fleet = _fleet_with_rows()
    row = attach_confirm_row(fleet, profile=FOCUSED_PROFILE, session_id="live-1")
    assert row is not None
    assert row.ownership == "not_ours"
    assert row.status == "waiting"
    # The flattened wait is what the confirm's unknown-kind sentence is for.
    assert row.waiting_kind == "unobserved"


def test_a_historical_session_resumes_with_no_dialog() -> None:
    """KTD8's exemption, and the reason for it: nothing live is stolen, and
    always-confirming trains the operator to click through the one dialog that
    matters."""
    fleet = _fleet_with_rows()
    assert attach_confirm_row(fleet, profile=FOCUSED_PROFILE, session_id="hist-1") is None


def test_a_session_this_run_already_drives_is_not_confirmed() -> None:
    fleet = _fleet_with_rows()
    fleet = mark_we_drive(
        fleet, profile=FOCUSED_PROFILE, session_id="live-1", at=BASE_TIME + 51
    )
    assert attach_confirm_row(fleet, profile=FOCUSED_PROFILE, session_id="live-1") is None


def test_an_unknown_session_is_not_confirmed() -> None:
    """No row means no observation, and R24's rule is that an absence is named
    rather than acted on as though it were a sighting. The confirmation's whole
    premise — this session is live elsewhere — would be unchecked."""
    fleet = _fleet_with_rows()
    assert attach_confirm_row(fleet, profile=FOCUSED_PROFILE, session_id="never-seen") is None


def test_a_reclaimed_row_is_not_confirmed() -> None:
    fleet = _fleet_with_rows()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "session.reclaimed", {"reason": "idle_timeout"}, session_id="live-1-runtime"
            ),
            at=BASE_TIME + 52,
            seq=60,
        ),
        profile=FOCUSED_PROFILE,
    )
    assert attach_confirm_row(fleet, profile=FOCUSED_PROFILE, session_id="live-1") is None


# ── the attach handover: hydration, and visible residue (KTD8) ────────────


def test_the_reply_s_two_hydratable_kinds_become_request_events() -> None:
    events = activation_hydration_events(
        {
            "session_id": "s2",
            "pending_approval": {
                "request_id": "a-1",
                "command": "rm -rf /tmp/x",
                "description": "a dangerous command",
                "choices": ["once", "deny"],
            },
            "pending_clarify": {"request_id": "c-1", "question": "which branch?"},
        },
        session_id="s2",
        at=BASE_TIME,
        seq=7,
    )
    assert [event.type for event in events] == ["approval.request", "clarify.request"]
    assert all(event.session_id == "s2" for event in events)

    # Folded through the ordinary reducer, they register as ordinary prompts —
    # no second registration path, so no second set of rules to drift.
    state = replace(
        FleetState().focused, focused_session_id="s2", session_key="s2"
    )
    for event in events:
        state = apply_frame(state, event)
    assert sorted(prompt.kind for prompt in state.prompts) == ["approval", "clarify"]


def test_a_reply_carrying_neither_field_hydrates_nothing() -> None:
    """Day-one behaviour, not an edge case: the revision serving this machine
    today has no hydration fields in its payload builder at all (U1)."""
    assert (
        activation_hydration_events(
            {"session_id": "s2", "status": "waiting"}, session_id="s2", at=BASE_TIME, seq=1
        )
        == ()
    )


def test_a_wait_the_attach_did_not_recover_latches_on_the_row() -> None:
    """The residue latch. A sudo raised before the attach was announced to the
    transport this attach displaced; no method re-announces it, so inventing a
    card would be a claim and staying silent would leave the row saying it is
    waiting for something that will never arrive."""
    fleet = _fleet_with_rows()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("sudo.request", {"request_id": "s-1"}, session_id="live-1-runtime"),
            at=BASE_TIME + 53,
            seq=61,
        ),
        profile=FOCUSED_PROFILE,
    )
    assert fleet.rows[(FOCUSED_PROFILE, "live-1")].waiting_kind == "sudo"

    fleet, line = latch_attach_residue(
        fleet,
        profile=FOCUSED_PROFILE,
        session_id="live-1",
        hydrated=frozenset(),
        at=BASE_TIME + 54,
    )
    row = fleet.rows[(FOCUSED_PROFILE, "live-1")]
    assert line == attach_residue_notice("sudo")
    assert "sudo prompt" in row.last_notice and "resolved-failed" in row.last_notice
    assert row.waiting_kind == "", "the wait must settle, not stay outstanding"


def test_a_hydrated_kind_latches_nothing() -> None:
    fleet = _fleet_with_rows()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "clarify.request",
                {"request_id": "c-1", "question": "which branch?"},
                session_id="live-1-runtime",
            ),
            at=BASE_TIME + 53,
            seq=61,
        ),
        profile=FOCUSED_PROFILE,
    )
    fleet, line = latch_attach_residue(
        fleet,
        profile=FOCUSED_PROFILE,
        session_id="live-1",
        hydrated=frozenset({"clarify"}),
        at=BASE_TIME + 54,
    )
    assert line == ""
    assert fleet.rows[(FOCUSED_PROFILE, "live-1")].waiting_kind == "clarify"


def test_a_flattened_wait_latches_as_an_unknown_kind() -> None:
    """A polled row reports ``waiting`` and no kind — U1 verified the row
    carries none. The latch names the ignorance rather than guessing a kind."""
    fleet = _fleet_with_rows()
    fleet, line = latch_attach_residue(
        fleet,
        profile=FOCUSED_PROFILE,
        session_id="live-1",
        hydrated=frozenset(),
        at=BASE_TIME + 54,
    )
    assert "unknown kind" in line
    assert "unknown kind" in fleet.rows[(FOCUSED_PROFILE, "live-1")].last_notice


def test_a_flattened_wait_withholds_the_latch_when_a_card_did_come_back() -> None:
    """The one case decided by what the reply carried rather than by the kind.

    The row said only ``waiting``; the reply hydrated an approval. Latching as
    well would put "a prompt was lost" on the row while the recovered card is
    on screen — a contradiction the operator cannot resolve. Withholding it
    costs nothing durable: the next roster poll reports the session waiting
    again if it still is.
    """
    fleet = _fleet_with_rows()
    fleet, line = latch_attach_residue(
        fleet,
        profile=FOCUSED_PROFILE,
        session_id="live-1",
        hydrated=frozenset({"approval"}),
        at=BASE_TIME + 54,
    )
    assert line == ""
    assert fleet.rows[(FOCUSED_PROFILE, "live-1")].last_notice == ""


def test_a_named_wait_still_latches_when_a_different_kind_hydrated() -> None:
    """The flattened-wait leniency above does not extend to a kind Talaria
    actually observed. A sudo the gateway announced before the attach is
    unrecoverable whether or not an approval came back with the reply."""
    fleet = _fleet_with_rows()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("sudo.request", {"request_id": "s-1"}, session_id="live-1-runtime"),
            at=BASE_TIME + 53,
            seq=61,
        ),
        profile=FOCUSED_PROFILE,
    )
    fleet, line = latch_attach_residue(
        fleet,
        profile=FOCUSED_PROFILE,
        session_id="live-1",
        hydrated=frozenset({"approval"}),
        at=BASE_TIME + 54,
    )
    assert "sudo prompt" in line


def test_a_session_that_was_not_waiting_latches_nothing() -> None:
    fleet = _fleet_with_rows()
    fleet, line = latch_attach_residue(
        fleet,
        profile=FOCUSED_PROFILE,
        session_id="hist-1",
        hydrated=frozenset(),
        at=BASE_TIME + 54,
    )
    assert line == ""
    assert fleet.rows[(FOCUSED_PROFILE, "hist-1")].last_notice == ""


def test_a_reclaimed_row_displaces_nobody() -> None:
    """CR4 round 4: this clause of the predicate was pinned by nothing.

    ``session.reclaimed`` records the reap reason without clearing
    ``live_listed`` — the row keeps saying the roster last saw it live. Without
    this clause, attaching to a session the gateway has already reaped would ask
    the operator to confirm detaching a client that the gateway itself says is
    gone.
    """
    row = RegistryRow(
        profile="beta",
        durable_id="s2",
        ownership="not_ours",
        live_listed=True,
        reclaimed_reason="idle_timeout",
    )
    assert not attach_displaces_client(row)


def test_a_row_on_a_lost_connection_displaces_nobody() -> None:
    """The sibling clause, and reachable the same way (CR4 round 4).

    ``fleet_connection_lost`` marks rows disconnected without clearing
    ``live_listed``, because a connection Talaria lost is not evidence the
    session ended — the data is marked, never cleared (R5/R20). But a
    connection Talaria cannot see is one it cannot displace anybody on.
    """
    row = RegistryRow(
        profile="beta",
        durable_id="s2",
        ownership="not_ours",
        live_listed=True,
        disconnected=True,
    )
    assert not attach_displaces_client(row)


def test_a_live_foreign_row_does_displace_somebody() -> None:
    """The positive case, so the two tests above cannot pass by always-False."""
    row = RegistryRow(
        profile="beta", durable_id="s2", ownership="not_ours", live_listed=True
    )
    assert attach_displaces_client(row)
