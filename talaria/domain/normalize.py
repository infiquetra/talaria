"""Deterministic normalization and the pure half of the reconciliation catalogue.

Every rule in ``docs/analysis/2026-08-02-hermes-reconciliation-rules.md`` lands in
one of two places. The ones that need no state — status coercion, identity,
session scoping, text hygiene — live here as small pure functions. The ones that
need the current session (terminal protection, resurrection, prompt expiry) live
in :mod:`talaria.domain.state`, because a guard that has to read what it is
guarding cannot be a pure function of its argument.

Splitting them this way is what makes "one rule, one test" achievable: a pure
rule gets a table-driven test with no fixture, and a stateful rule gets a
sequence test. A single ``normalize()`` god-function would have forced every rule
to be tested through a whole replay.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from datetime import datetime
from typing import Any

from talaria.domain.decode import DecodedFrame, decode_frame
from talaria.domain.models import (
    KNOWN_SUBAGENT_STATUSES,
    TERMINAL_SUBAGENT_STATUSES,
    GatewayEvent,
    SubagentStatus,
)

# ── Event dispositions ───────────────────────────────────────────────────

#: Known event types that produce a plain system line in the transcript rather
#: than driving turn, sub-agent, or prompt state. Keeping this explicit — rather
#: than "anything not otherwise handled" — means adding a gateway event type is a
#: decision about where it renders, not an accident of a fall-through branch.
SYSTEM_LINE_EVENTS: frozenset[str] = frozenset(
    {
        "background.complete",
        "browser.progress",
        "gateway.start_timeout",
        "gateway.stderr",
        "notification.show",
        "review.summary",
        "status.update",
    }
)

#: Known event types Talaria decodes and deliberately does not render in v0.1.
#: Each is out of scope for the prototype's surface (pet reactions, voice, skins,
#: mixture-of-agents phase chatter, billing device-flow links) and each is listed
#: rather than defaulted, so "we ignore this" stays a recorded decision.
AMBIENT_IGNORED_EVENTS: frozenset[str] = frozenset(
    {
        "billing.step_up.verification",
        "moa.aggregating",
        "moa.phase",
        "moa.progress",
        "notification.clear",
        "platforms.changed",
        "reaction",
        "skin.changed",
        "voice.status",
        "voice.transcript",
        "wake.detected",
    }
)

#: How long a sub-agent detail list may grow. Re-encoded from Hermes's
#: ``pushUnique`` bounds (``createGatewayEventHandler.ts:355-362``): thinking and
#: progress notes cap at 6, tool lines at 8. Talaria keeps one detail list per
#: row (KTD8's single ``detail`` field), so it takes the larger bound.
DETAIL_LIMIT = 8

#: Hermes clips a ``gateway.stderr`` line to 120 characters
#: (``createGatewayEventHandler.ts:880``, and again at ``:1015-1016``, whose
#: comment says it exists "to match the 120-char clip used for
#: ``gateway.stderr`` activity entries"). **Both sites are scoped to
#: ``gateway.stderr`` alone**, and both feed a one-row activity region, so the
#: bound is about the region's height rather than about the text being
#: gateway-authored. Re-encoded here for the one Talaria surface with the same
#: shape: a sub-agent row's single ``detail`` line.
DETAIL_LINE_CLIP = 120

#: What bounds a transcript entry. This is **not** a re-encoding of Hermes —
#: Hermes clips no transcript text at all; ``status.update`` reaches
#: ``pushActivity`` with ``p.text`` whole (``createGatewayEventHandler.ts:775``,
#: body at ``:811-817``). Talaria formerly reused the 120 above here on the
#: reasoning that "the same argument applies to any gateway-authored text". It
#: does not: the argument is that a runaway line must not own a *one-row*
#: region, and the transcript scrolls. What the reuse cost was measured on a
#: live gateway — a 157-character ``status.update`` reading "…trim the file, pin
#: a larger context_file_max_chars, or use a larger-context model!" reached the
#: screen cut mid-identifier at ``context_file_max_``, so the remedy it named
#: was unreadable and the setting could not even be copied.
#:
#: A bound still exists, because an unbounded line is a rendering hazard rather
#: than a diagnostic. It sits an order of magnitude above any human-readable
#: notice and below :data:`~talaria.domain.commands.COMMAND_OUTPUT_CLIP`, which
#: bounds text the operator explicitly asked to see.
TRANSCRIPT_LINE_CLIP = 2000


def normalize_frame(
    frame: Any,
    *,
    at: float,
    seq: int,
    parse_error: bool = False,
) -> DecodedFrame:
    """Decode one frame. A thin seam so callers do not import the decoder."""
    return decode_frame(frame, at=at, seq=seq, parse_error=parse_error)


def parse_frame_time(value: str) -> float:
    """Frame-log ISO time to epoch seconds, with a deterministic fallback.

    Frame-log v1 writes ``2026-08-02T12:21:35.016Z``. An unparseable value
    yields ``0.0`` rather than "now": a clock read here would make replay
    non-reproducible, which AE2 forbids, and a wrong-but-fixed number is
    strictly better than a right-but-varying one for a value used only to
    compute elapsed times.
    """
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()
    except (AttributeError, ValueError):
        return 0.0


# ── Rule: session cross-talk guard ───────────────────────────────────────


def applies_to_focused_session(
    event: GatewayEvent, focused_session_id: str | None
) -> bool:
    """Whether an event may mutate the focused session's state.

    Re-encodes ``createGatewayEventHandler.ts:720-722``. An event that names a
    different session is dropped, with one exception: types under the
    ``gateway.`` prefix describe the transport rather than a conversation, so
    they are never session-scoped and always apply.
    """
    if event.type.startswith("gateway."):
        return True
    if event.session_id is None or focused_session_id is None:
        return True
    return event.session_id == focused_session_id


# ── Rule: sub-agent status normalization and terminal protection ─────────


def normalize_subagent_status(value: Any, fallback: SubagentStatus) -> SubagentStatus:
    """Coerce a wire status into the frozen enum, else the caller's fallback.

    Re-encodes ``createGatewayEventHandler.ts:374-382``, including the
    lower-casing step — the gateway has emitted mixed case, and a row that says
    ``Completed`` would otherwise be indistinguishable from an unknown status.
    """
    if not isinstance(value, str):
        return fallback
    normalized = value.lower()
    if normalized in KNOWN_SUBAGENT_STATUSES:
        # Narrowed by the membership test; the Literal has exactly these members.
        return normalized  # type: ignore[return-value]
    return fallback


def is_terminal_status(status: str) -> bool:
    """Whether a status is one of the five a late event may not overwrite."""
    return status in TERMINAL_SUBAGENT_STATUSES


def keep_terminal_else(current: SubagentStatus, proposed: SubagentStatus) -> SubagentStatus:
    """Terminal wins over any later proposal.

    Re-encodes ``keepTerminalElseRunning`` (``:612``) generalized to an arbitrary
    proposed status, so the same one-line guard covers ``spawn_requested``
    proposing ``queued``, ``start`` proposing ``running``, and a progress event
    proposing ``running`` — the three call shapes Hermes writes out separately.
    """
    return current if is_terminal_status(current) else proposed


def subagent_identity(payload: Mapping[str, Any]) -> str:
    """Stable sub-agent id, server-issued where available.

    Re-encodes ``turnController.ts:1013-1016``: prefer ``subagent_id`` because it
    survives nested grandchildren and cross-tree joins; fall back to the
    composite key for gateways that omit it, which produces a flat list rather
    than a tree but never merges two distinct children into one row.
    """
    server_id = payload.get("subagent_id")
    if isinstance(server_id, str) and server_id:
        return server_id
    task_index = payload.get("task_index")
    index = task_index if isinstance(task_index, int) and not isinstance(task_index, bool) else 0
    goal = coerce_text(payload.get("goal")) or "subagent"
    return f"sa:{index}:{goal}"


def subagent_sort_key(depth: int, index: int, identity: str) -> tuple[int, int, str]:
    """Stable order by spawn position, not arrival.

    Re-encodes ``turnController.ts:1078-1083``. The trailing identity is
    Talaria's addition: Hermes's two-key sort is not total, so two children at
    the same depth and index keep insertion order — which under a reordered
    replay is not the same order twice. AE2 requires it to be.
    """
    return (depth, index, identity)


def preserved(value: Any, previous: Any) -> Any:
    """Nullish-preserving field merge for partial sub-agent payloads.

    Re-encodes the ``??`` chain at ``turnController.ts:1057-1076``: streaming
    sub-agent events carry partial payloads, so a field the event omits must
    keep its prior value rather than be overwritten with ``None``.
    """
    return previous if value is None else value


# ── Text hygiene ─────────────────────────────────────────────────────────


def coerce_text(value: Any) -> str:
    """A string or nothing — never ``str(value)`` on arbitrary wire content.

    Stripping stays here for **diagnostic** channels only — a tool name, an
    error message, a status note — where leading or trailing whitespace is
    never meaningful. Content-channel text (an assistant reply, a reasoning
    block) must not go through this: see :func:`coerce_text_exact` (KTD7).
    """
    return value.strip() if isinstance(value, str) else ""


def coerce_text_exact(value: Any) -> str:
    """A string or nothing, preserved byte-for-byte — the content channel's
    own coercion (KTD7).

    The only difference from :func:`coerce_text` is that this never strips.
    Content-channel text is markdown: four leading spaces are an indented
    code block, and a trailing blank line closes a construct — both are
    structure, not incidental whitespace, so a coercion that trims either one
    silently rewrites what the model said.
    """
    return value if isinstance(value, str) else ""


def _clip(text: str, limit: int) -> str:
    """Bound one line, marking the cut so it is not mistaken for the whole
    message."""
    if len(text) <= limit:
        return text
    return text[:limit] + "…"


def clip_detail_line(text: str, limit: int = DETAIL_LINE_CLIP) -> str:
    """Bound the single ``detail`` line of a sub-agent row.

    The row is one screen line, so the cut is a layout constraint and a tight
    bound is correct. Use :func:`clip_transcript_line` for anything that lands
    in the transcript, which scrolls.
    """
    return _clip(text, limit)


def clip_transcript_line(text: str, limit: int = TRANSCRIPT_LINE_CLIP) -> str:
    """Bound one transcript entry — a backstop against a runaway line, not a
    summary.

    Deliberately loose. A transcript entry is frequently the only account an
    operator gets of a failure, and a diagnostic cut before its remedy is worse
    than a long one: the reader cannot tell that advice was ever offered.
    """
    return _clip(text, limit)


def push_unique(items: Sequence[str], value: str, limit: int = DETAIL_LIMIT) -> tuple[str, ...]:
    """Append unless it repeats the tail, then keep only the last ``limit``.

    Re-encodes ``pushUnique`` (``createGatewayEventHandler.ts:355-362``). The
    tail check is what stops a sub-agent that reports the same note on every
    poll from filling its own row.
    """
    if items and items[-1] == value:
        return tuple(items)
    return tuple([*items, value])[-limit:]
