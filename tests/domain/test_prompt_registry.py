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

from talaria.domain.projection import status_payload, turn_status
from talaria.domain.state import respond_to_prompt

from .conftest import raw_event, replay


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

    answered, accepted = respond_to_prompt(state, "req-1")
    assert accepted
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
    after, accepted = respond_to_prompt(state, "req-1")
    assert not accepted
    assert after.rejected_responses == 1
    assert after.prompts == ()


def test_a_respond_for_an_unknown_id_is_refused_before_the_socket() -> None:
    state = replay([raw_event("sudo.request", {"request_id": "req-live"})])
    after, accepted = respond_to_prompt(state, "req-never-existed")
    assert not accepted
    assert [p.request_id for p in after.prompts] == ["req-live"]


def test_approval_gets_a_synthesized_session_scoped_key() -> None:
    """``approval.request`` carries no ``request_id`` at the pin — its payload is
    ``{description, command, choices, allow_permanent, smart_denied}``
    (``createGatewayEventHandler.ts:1130-1147``) and ``approval.respond``
    resolves by session key instead. R8 still wants a keyed registry."""
    state = replay(
        [raw_event("approval.request", {"command": "rm -rf /", "description": "dangerous"})]
    )
    assert [p.request_id for p in state.prompts] == ["approval:sess-focus"]
    assert state.prompts[0].summary == "dangerous"


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
