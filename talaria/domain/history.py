"""Decode a ``session.resume`` reply's ``messages`` into transcript lines (KTD2).

The gateway builds this array in ``_history_to_messages``
(``tui_gateway/server.py:7078-7167``) and it is **not** a uniform
``{role, text}`` list. Four row shapes reach the wire, and a bare
``role``-to-kind cast mis-renders three of them:

* **Text rows** — ``{role, text, row_id}``, where ``row_id`` is the durable
  persisted-row identity and carries no transcript content.
* **Tool rows** — ``{role: "tool", name, context}``. These carry *no* ``text``
  key at all, so a decoder that reads ``text`` renders a tool call as a blank
  line.
* **Reasoning-carrying assistant rows** — ``{role: "assistant", text,
  reasoning, reasoning_content, codex_reasoning_items}``. The gateway keeps an
  assistant row whose visible text is empty when it carries reasoning
  (``server.py:7120-7135``, ``#44022``), so "assistant with empty text" is a
  *reasoning* line, not a row to drop.
* **Display-metadata rows** — a row typed by ``display_kind``. A model switch
  is persisted as ``{role: "user", ...}`` with ``display_kind:
  "model_switch"`` (``server.py:3951``) purely because strict providers reject
  a late system message; rendering it by its role puts gateway scaffolding on
  screen as though the operator had typed it. The gateway's own comment at
  ``server.py:7156-7159`` says these exist "so the TUI can render model
  switches and delegation completions as events instead of opaque user
  messages".

**Nothing is dropped.** A malformed element and an unrecognised role both take
the ``unknown-event`` posture the live path already uses for an event type
Talaria does not model (:func:`~talaria.domain.state._apply_unknown_event`):
one entry, kind ``unknown-event``, carrying the element's literal text. A
history the client cannot fully interpret must still show the operator that
something was there.

The element shapes above are pinned against replies a real Hermes sent in
``tests/domain/test_history_decoder.py``. They are not in ``MethodBaseline``
(``talaria/domain/compat.py:379``) because that structure is top-level by
design and these shapes are nested one level down.
"""

from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from typing import Any, Final

from talaria.domain.models import TranscriptKind
from talaria.domain.normalize import clip_transcript_line, coerce_text

#: One decoded line: the kind it renders as and the text it renders.
HistoryLine = tuple[TranscriptKind, str]

#: The roles ``_history_to_messages`` will emit (``server.py:7086``). Anything
#: else is a gateway Talaria has not met, and takes the unknown-event posture
#: rather than being guessed at.
HISTORY_ROLES: Final[frozenset[str]] = frozenset({"user", "assistant", "tool", "system"})

#: The assistant reasoning keys the gateway forwards (``server.py:7124-7129``),
#: in the order they are preferred as *renderable* text.
#:
#: ``reasoning_details`` and ``codex_reasoning_items`` are deliberately absent.
#: Both are structured provider payloads rather than prose, and the codex one
#: carries an ``encrypted_content`` blob that is meaningless on screen and
#: costs a screenful of base64 to show. Their presence still keeps the row —
#: see :data:`REASONING_KEYS` — it is only their *content* that is not text.
REASONING_TEXT_KEYS: Final[tuple[str, ...]] = ("reasoning", "reasoning_content")

#: Every key whose presence makes an otherwise-empty assistant row a reasoning
#: row rather than an empty one. Mirrors ``server.py:7124-7129`` exactly.
REASONING_KEYS: Final[tuple[str, ...]] = (
    "reasoning",
    "reasoning_content",
    "reasoning_details",
    "codex_reasoning_items",
)

#: ``display_kind`` values that mean "this row is a timeline event, not a
#: conversational turn". Taken from the gateway's own writers: ``model_switch``
#: (``server.py:3951``), ``auto_continue`` (``server.py:7442``, and the legacy
#: sniff at ``:7073``), ``async_delegation_complete`` (``server.py:9308``).
#:
#: ``skill_invocation`` (``server.py:7152``) is **not** here. That row is the
#: operator's own line with the expanded skill body replaced by the invocation
#: they typed, so it belongs on screen as a user line — which is what it is.
#: ``hidden`` is not here either: the gateway drops those rows before the reply
#: is built (``server.py:7093``), so a client never sees one.
EVENT_DISPLAY_KINDS: Final[frozenset[str]] = frozenset(
    {"model_switch", "auto_continue", "async_delegation_complete"}
)

#: What an element Talaria could not read at all is labelled as.
UNREADABLE_ELEMENT_PREFIX: Final[str] = "unreadable history element:"

#: What an element with a role Talaria does not model is labelled as.
UNKNOWN_ROLE_PREFIX: Final[str] = "unknown history role"


def _literal(value: Any) -> str:
    """The element's own text, never ``str()`` on an arbitrary object.

    A string is itself. Anything else is re-encoded as JSON so what appears on
    screen is the wire content rather than a Python repr, and a value JSON
    cannot encode falls back to its type name — which says *something was
    here* without inventing what.
    """
    if isinstance(value, str):
        return value
    try:
        return json.dumps(value, ensure_ascii=False, sort_keys=True)
    except (TypeError, ValueError):  # pragma: no cover - defensive
        return f"<{type(value).__name__}>"


def _unreadable(value: Any) -> HistoryLine:
    # Deliberately unclipped, like the assistant and reasoning lines below —
    # see the module docstring's "seeded history and the turns that follow it
    # are the same conversation" and the live unknown-event path this mirrors
    # (``_apply_unknown_event``, ``talaria/domain/state.py``), which never
    # clips ``frame.text`` either.
    return ("unknown-event", f"{UNREADABLE_ELEMENT_PREFIX} {_literal(value)}")


def _unknown_role(element: Mapping[str, Any]) -> HistoryLine:
    # Unclipped (D1), like `_unreadable` right above it and for the identical
    # reason: this is the same "unknown-event" kind, and its live counterpart
    # (`_apply_unknown_event`, `talaria/domain/state.py`) never clips
    # `frame.text` either. A clip here was the one place this decoder still
    # broke its own "content is never dropped" contract for a row the live
    # path shows in full.
    role = _literal(element.get("role"))
    return (
        "unknown-event",
        f"{UNKNOWN_ROLE_PREFIX} {role}: {_literal(dict(element))}",
    )


def _metadata_summary(value: Any) -> str:
    """Flatten ``display_metadata``'s scalars into one readable trailer.

    Rendered rather than discarded because the counts are the whole content of
    a delegation-completion row: the row's text names the delegation, and the
    metadata says how many tasks finished and how many failed
    (``server.py:9407-9430``). Nested values are re-encoded as JSON by
    :func:`_literal` rather than walked — this is a trailer, not a renderer.
    """
    if not isinstance(value, Mapping) or not value:
        return ""
    parts = [f"{key}={_literal(value[key])}" for key in sorted(map(str, value.keys()))]
    return " ".join(parts)


def _tool_line(element: Mapping[str, Any]) -> HistoryLine | None:
    """Format a tool row the way the live ``tool.start`` handler formats one.

    Identical to :func:`~talaria.domain.state._on_tool_start`'s line by
    intent: seeded history and the turns that follow it are the same
    conversation, and a tool call that changes shape at the seam would read as
    two different clients rendering the same session.

    ``None`` when the element carries neither a ``name`` nor a ``context`` —
    the caller falls back to :func:`_unreadable` rather than inventing the
    bare line ``"⏺ tool"``, which discards every other key a malformed
    element carried while looking like a genuine, contentless tool call.
    """
    name = coerce_text(element.get("name"))
    context = coerce_text(element.get("context"))
    if not name and not context:
        return None
    # Clipped to match its live counterpart, `_on_tool_start`
    # (`talaria/domain/state.py`), which formats the identical line and clips
    # it the same way.
    return ("tool", clip_transcript_line(f"⏺ {name or 'tool'} {context}".rstrip()))


def _reasoning_text(element: Mapping[str, Any]) -> str:
    for key in REASONING_TEXT_KEYS:
        text = coerce_text(element.get(key))
        if text:
            return text
    return ""


def _has_reasoning(element: Mapping[str, Any]) -> bool:
    return any(element.get(key) for key in REASONING_KEYS)


def _decode_element(element: Any) -> tuple[HistoryLine, ...]:
    if not isinstance(element, Mapping):
        return (_unreadable(element),)

    role = element.get("role")
    if not isinstance(role, str) or role not in HISTORY_ROLES:
        return (_unknown_role(element),)

    if role == "tool":
        tool_line = _tool_line(element)
        return (tool_line,) if tool_line is not None else (_unreadable(dict(element)),)

    text = coerce_text(element.get("text"))
    display_kind = element.get("display_kind")
    if isinstance(display_kind, str) and display_kind in EVENT_DISPLAY_KINDS:
        trailer = _metadata_summary(element.get("display_metadata"))
        body = f"{text} {trailer}".strip() if trailer else text
        # An event row with neither text nor metadata still gets a line: the
        # kind is the only thing it says, and saying nothing would be dropping
        # it.
        #
        # Clipped to match its live counterpart, `_apply_system_line`
        # (`talaria/domain/state.py`), which writes both this row's kind and
        # the plain "system" row below through the identical clip.
        return (("system", clip_transcript_line(body or display_kind)),)

    if role == "system":
        # Clipped to match `_apply_system_line` — see the comment above.
        return (("system", clip_transcript_line(text)),) if text else (_unreadable(dict(element)),)

    if role == "assistant":
        lines: list[HistoryLine] = []
        reasoning = _reasoning_text(element)
        if reasoning:
            # Unclipped, like the assistant text below — see the module
            # docstring: seeded history and live turns are the same
            # conversation, and the live ``message.complete`` reducer
            # (``talaria/domain/state.py``) never clips either.
            lines.append(("reasoning", reasoning))
        if text:
            lines.append(("assistant", text))
        if lines:
            return tuple(lines)
        # Empty text, and the only reasoning it carries is a structured
        # payload with no prose in it (an encrypted codex item, typically).
        # The row is real — the gateway kept it deliberately — so it is shown
        # as one rather than silently skipped.
        if _has_reasoning(element):
            return (("reasoning", "*[reasoning withheld by the provider]*"),)
        return (_unreadable(dict(element)),)

    # role == "user"
    #
    # Unclipped (D1): the live counterpart, `record_submission`
    # (`talaria/domain/state.py`), writes the operator's own words with
    # `_append(state, "user", text)` and never clips them — the same
    # unclipped treatment the assistant and reasoning lines above already
    # get, for the same "seeded history and live turns are the same
    # conversation" reason.
    return (("user", text),) if text else (_unreadable(dict(element)),)


def decode_history(messages: Any) -> tuple[HistoryLine, ...]:
    """Turn a resume reply's ``messages`` array into ordered transcript lines.

    Total: every input produces a value, and a shape this has never seen
    produces an ``unknown-event`` line rather than an exception. The reply is
    untrusted wire content on a startup path, and a decoder that can raise
    turns a surprising history into a client that will not open.
    """
    if not isinstance(messages, Sequence) or isinstance(messages, (str, bytes)):
        return ()
    lines: list[HistoryLine] = []
    for element in messages:
        lines.extend(_decode_element(element))
    return tuple(lines)


def element_count(messages: Any) -> int:
    """How many *elements* the reply delivered, which is not how many lines.

    One assistant element carrying both reasoning and text decodes to two
    lines. The withheld-history arithmetic compares delivered elements against
    the gateway's ``message_count``, which counts elements, so it has to ask
    this rather than measure :func:`decode_history`'s output.
    """
    if not isinstance(messages, Sequence) or isinstance(messages, (str, bytes)):
        return 0
    return len(messages)


__all__ = [
    "EVENT_DISPLAY_KINDS",
    "HISTORY_ROLES",
    "HistoryLine",
    "REASONING_KEYS",
    "REASONING_TEXT_KEYS",
    "UNKNOWN_ROLE_PREFIX",
    "UNREADABLE_ELEMENT_PREFIX",
    "decode_history",
    "element_count",
]
