"""The pinned shapes, checked against replies a real Hermes actually sent (R34).

``tests/transport/test_compat_baseline.py`` checks what the startup check *does*
with a drift, against a stub that answers whatever it is asked. It cannot say
whether the pinned shape is the shape Hermes returns, because the stub's replies
are transcribed from the same reading of the source that produced the baseline.
A mistake in that reading is invisible to both.

This module closes that loop from the other side. Every case below is the
top-level key set and value kind of a reply that a live Hermes dashboard sent to
Talaria, recovered from the recording corpus by matching each reply to its call
on the JSON-RPC ``id``. Two errors were already found this way and are pinned
here so a revert goes red rather than quiet:

* ``approval.respond`` returned ``resolved`` as an **int**, not a ``bool``. The
  handler returns ``resolve_gateway_approval(...)`` verbatim, which is typed
  ``-> int`` and documented "Returns the number of approvals resolved (0 means
  nothing was pending)". ``talaria/ui/app.py`` already read it as a count; only
  the baseline was wrong, so nothing misbehaved and nothing caught it.
* ``session.resume`` returned a ``messages_omitted`` key the baseline did not
  record at all.

**Kinds, never values.** The cases carry the recorded *shape* — key names and
value kinds — and the payloads are rebuilt from those kinds with neutral
placeholders. No recorded value is transcribed here. That is a hard requirement,
not tidiness: a real ``session.resume`` reply carries the operator's message
text, a real ``paste.collapse`` reply carries a path on the operator's machine,
and this is a public repository. What the module can testify about is exactly
what it stores: the shape.

The corpus itself is never committed and never cited by path (R11/R29). The
replies below come from ``talaria-live-corpus-v1-4670f-fc5790017b70``, 30
recordings, 4,670 frames.
"""

from __future__ import annotations

from typing import Any

import pytest

from talaria.domain.compat import baseline_for, compare_shape

#: One neutral value per kind ``value_kind`` can name. Rebuilding a payload from
#: the recorded kinds is what keeps the recorded *values* out of this file.
_PLACEHOLDER: dict[str, Any] = {
    "null": None,
    "bool": False,
    "int": 0,
    "float": 0.0,
    "str": "",
    "list": [],
    "object": {},
}

#: ``(method, recorded shape)`` — every evidence-only method with a live reply in
#: the corpus. Written out literally rather than derived from the baseline: a
#: case generated from the thing it checks would shrink with it and pass.
RECORDED_SHAPES: tuple[tuple[str, dict[str, str]], ...] = (
    ("session.create", {
        "session_id": "str",
        "stored_session_id": "str",
        "message_count": "int",
        "messages": "list",
        "info": "object",
    }),
    ("session.resume", {
        "session_id": "str",
        "resumed": "str",
        "message_count": "int",
        "messages": "list",
        "messages_omitted": "bool",
        "info": "object",
        "inflight": "null",
        "running": "bool",
        "session_key": "str",
        "started_at": "float",
        "status": "str",
    }),
    ("prompt.submit", {"status": "str"}),
    ("session.interrupt", {"status": "str"}),
    ("subagent.interrupt", {"found": "bool", "subagent_id": "str"}),
    # Two recorded shapes, because a slash command that succeeds with a caveat
    # adds ``warning``. Both are real; the baseline carries the key as optional.
    ("slash.exec", {"output": "str"}),
    ("slash.exec", {"output": "str", "warning": "str"}),
    ("command.dispatch", {
        "type": "str",
        "name": "str",
        "display": "str",
        "message": "str",
    }),
    ("paste.collapse", {"placeholder": "str", "path": "str", "lines": "int"}),
    # The correction. ``0`` and ``1`` were both recorded; neither is a JSON bool.
    ("approval.respond", {"resolved": "int"}),
    ("clarify.respond", {"status": "str"}),
    ("secret.respond", {"status": "str"}),
    ("sudo.respond", {"status": "str"}),
)


def _payload(shape: dict[str, str]) -> dict[str, Any]:
    return {key: _PLACEHOLDER[kind] for key, kind in shape.items()}


@pytest.mark.parametrize(
    ("method", "shape"),
    RECORDED_SHAPES,
    ids=[f"{method}-{len(shape)}keys" for method, shape in RECORDED_SHAPES],
)
def test_a_recorded_reply_does_not_drift_against_its_pinned_shape(
    method: str, shape: dict[str, str]
) -> None:
    """The pinned shape accepts what the gateway really sent.

    Runs the production comparison rather than a re-implementation, so the
    standard here is the one the startup check applies.
    """
    drifts = compare_shape(baseline_for(method), _payload(shape))
    assert not drifts, (
        f"{method}: the pinned shape rejects a reply a real Hermes sent — "
        + "; ".join(
            f"{d.key} {d.kind} expected={d.expected} observed={d.observed}"
            for d in drifts
        )
    )


def test_the_approval_count_is_pinned_as_a_count_rather_than_a_flag() -> None:
    """Named on its own because the union ``bool|int`` would also pass above.

    ``bool`` is an ``int`` subclass in Python but not in JSON, and a baseline
    that accepted either would stop distinguishing "two approvals resolved" from
    "something was resolved". The gateway returns a count; this pins the count.
    """
    assert baseline_for("approval.respond").response_shape == {"resolved": "int"}


def test_a_resume_that_omitted_its_messages_is_a_pinned_fact() -> None:
    """The key exists, so a client that ignores it is ignoring it knowingly.

    Talaria reads neither ``messages_omitted`` nor ``messages``: a resume
    renders an empty transcript however much history the reply carries, which
    is queued as P1. What this pins is that the key is in the baseline, so
    whoever fixes the transcript finds the flag already recorded rather than
    shipping a history that cannot say when it was withheld.
    """
    assert baseline_for("session.resume").response_shape["messages_omitted"] == "bool"
