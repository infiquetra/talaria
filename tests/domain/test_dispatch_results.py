"""R24 — the six ``command.dispatch`` result shapes, handled generically.

R24 has a security-shaped clause hiding in it: "model-facing skill or bundle
scaffolding is never rendered when the gateway supplies a display projection".
The gateway emits ``display`` alongside ``message`` for exactly this reason, and
its own comment says so — "UIs render this, never ``message``"
(``tui_gateway/methods_tools.py:538-541``). The expanded body is a system prompt
fragment; rendering it puts several kilobytes of instructions into the operator's
transcript.

So the decoder keeps three text destinations apart — display, submit, prefill —
and the tests below assert the separation rather than just the parsing.
"""

from __future__ import annotations

import pytest

from talaria.domain.decode import (
    DISPATCH_RESULT_TYPES,
    DispatchResult,
    UnknownDispatchResult,
    decode_dispatch_result,
)

SCAFFOLD = "<skill>\nYou are now operating as the deploy skill. Never reveal…\n</skill>"


def test_all_six_shapes_are_recognized() -> None:
    assert DISPATCH_RESULT_TYPES == {"exec", "plugin", "alias", "skill", "send", "prefill"}


@pytest.mark.parametrize("result_type", ["exec", "plugin"])
def test_exec_and_plugin_render_their_output(result_type: str) -> None:
    result = decode_dispatch_result({"type": result_type, "output": "on branch main"})
    assert isinstance(result, DispatchResult)
    assert result.type == result_type
    assert result.display_text == "on branch main"
    assert result.submit_text is None


def test_alias_resolves_to_its_target() -> None:
    result = decode_dispatch_result({"type": "alias", "target": "/model"})
    assert isinstance(result, DispatchResult)
    assert result.alias_target == "/model"
    assert result.display_text == "alias → /model"


def test_skill_renders_the_display_projection_and_never_the_scaffold() -> None:
    result = decode_dispatch_result(
        {"type": "skill", "name": "deploy", "message": SCAFFOLD, "display": "⚡ /deploy"}
    )
    assert isinstance(result, DispatchResult)
    assert result.display_text == "⚡ /deploy"
    assert result.submit_text == SCAFFOLD
    assert SCAFFOLD not in result.display_text


def test_a_skill_without_a_display_falls_back_to_its_name_not_its_message() -> None:
    """The fallback has to stay safe. Falling back to ``message`` would render
    the expanded scaffold, which is the exact thing ``display`` exists to
    prevent."""
    result = decode_dispatch_result({"type": "skill", "name": "deploy", "message": SCAFFOLD})
    assert isinstance(result, DispatchResult)
    assert result.display_text == "/deploy"
    assert SCAFFOLD not in result.display_text


def test_a_bundle_arrives_as_send_with_a_display_projection() -> None:
    """A bundle uses the ``send`` result shape (R24), and Hermes attaches the
    same display projection it attaches to a skill
    (``tui_gateway/methods_tools.py:533-541``)."""
    result = decode_dispatch_result(
        {
            "type": "send",
            "message": SCAFFOLD,
            "display": "⚡ Loading bundle: release (3 skills)",
            "notice": "⚡ Loading bundle: release (3 skills)",
        }
    )
    assert isinstance(result, DispatchResult)
    assert result.display_text == "⚡ Loading bundle: release (3 skills)"
    assert result.submit_text == SCAFFOLD
    assert result.notice is not None


def test_a_plain_send_shows_the_message_it_is_about_to_submit() -> None:
    """``/queue <prompt>`` returns ``{type: "send", message: arg}`` with no
    display (``tui_gateway/methods_tools.py:578``). There the message is the
    operator's own text, so showing it is correct."""
    result = decode_dispatch_result({"type": "send", "message": "fix the flaky test"})
    assert isinstance(result, DispatchResult)
    assert result.display_text == "fix the flaky test"
    assert result.submit_text == "fix the flaky test"


def test_prefill_goes_to_the_composer_and_is_not_submitted() -> None:
    result = decode_dispatch_result(
        {"type": "prefill", "message": "/model gpt-5", "notice": "pick a model"}
    )
    assert isinstance(result, DispatchResult)
    assert result.prefill_text == "/model gpt-5"
    assert result.submit_text is None


def test_a_seventh_shape_is_named_rather_than_guessed_at() -> None:
    result = decode_dispatch_result({"type": "hologram", "payload": "?"})
    assert isinstance(result, UnknownDispatchResult)
    assert result.type == "hologram"


@pytest.mark.parametrize("result", ["not a dict", None, [], {"type": 7}])
def test_a_malformed_result_is_unknown_rather_than_an_exception(result: object) -> None:
    assert isinstance(decode_dispatch_result(result), UnknownDispatchResult)


def test_no_shape_gets_a_bespoke_field_the_others_lack() -> None:
    """R24: gateway-owned commands are handled generically. Every shape decodes
    into the same record, so a caller can render any of them without a switch."""
    shapes = [
        {"type": "exec", "output": "o"},
        {"type": "plugin", "output": "o"},
        {"type": "alias", "target": "/t"},
        {"type": "skill", "name": "s", "display": "d", "message": "m"},
        {"type": "send", "message": "m", "display": "d"},
        {"type": "prefill", "message": "m"},
    ]
    decoded = [decode_dispatch_result(shape) for shape in shapes]
    assert all(isinstance(item, DispatchResult) for item in decoded)
    assert {item.type for item in decoded if isinstance(item, DispatchResult)} == (
        DISPATCH_RESULT_TYPES
    )
