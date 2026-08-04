"""Immutable snapshot view models with explicit change markers.

ADR-0002 left the view-model shape open and assigned it to "the first vertical
slice's re-render-cost evidence". That evidence belongs to U5, but U5 cannot
exist without this module, so U3 chooses and U5 measures.

**The choice: a frozen snapshot plus a set naming what changed since the last
one.** The UI can skip untouched regions without the domain knowing what a widget
is, and — the deciding argument — a snapshot is comparable. AE2 requires that
replaying one corpus twice produces an identical projection, and comparing two
values is trivial while comparing two mutation histories is not. The cost is one
allocation per emission, which is what U5 is asked to measure; if it measures
badly, the ADR records the number rather than the guess.

Nothing here reads a clock either. ``project()`` takes the observation time
explicitly, defaulting to the last frame time the state saw, so a sub-agent's
elapsed seconds are a function of the corpus rather than of when the test ran.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
from typing import Any

from talaria.domain.models import (
    ConnectionStatus,
    PromptKind,
    RunMode,
    SubagentRow,
    TranscriptEntry,
    TranscriptKind,
    TurnStatus,
)
from talaria.domain.state import UNCORRELATED_APPROVAL, SessionState

#: KTD5 freezes the status contract's field set at version 1. Adding a field is
#: a ``version: 2`` change that still emits the v1 shape on request.
STATUS_PAYLOAD_VERSION = 1

#: Rendered when the transcript region's height is unknown. Only used as a
#: default argument; a live UI passes its real height (KTD10 serves
#: ``viewport_rows`` truthfully because it is a number the UI already knows).
DEFAULT_VIEWPORT_ROWS = 24


class ProjectionUnavailableError(RuntimeError):
    """The projection cannot answer right now — teardown, or a projection fault.

    KTD10 is explicit that this must not become a fabricated response: the
    gateway's blocking bridge tolerates a late or absent answer and expires after
    30 seconds (``tui_gateway/server.py:2981-2998``), so saying nothing is a
    supported outcome and inventing a plausible screen is not.
    """


@dataclass(frozen=True)
class TranscriptView:
    """The transcript as plain text lines — the buffer terminal-read serves.

    One transcript entry can produce several lines; ``entry_count`` keeps the
    two countable independently so a test can assert content completeness (R6)
    without depending on how many newlines an entry happened to contain.

    ``committed_lines`` says where the provisional region begins, and it exists
    because a consumer cannot work it out. Lines ``[0, committed_lines)`` come
    from committed entries: entries are immutable and only ever appended, so
    those lines are fixed forever and a diffing renderer may skip them. The rest
    is in-flight streaming text, which is rewritten in place *and moves* — when
    an entry is appended mid-stream it lands before the streaming block, so
    every provisional line shifts down. A renderer that treats a matching
    provisional line as settled will desynchronize the first time that happens,
    which is exactly the defect this field was added to make impossible.

    The default is ``0`` — "assume nothing is settled" — so a hand-built view
    that omits it makes a consumer do more work rather than less.

    ``kinds`` names the entry each line came from, one member per line. It
    exists because a renderer that treats every line alike has no way to tell
    agent prose from a tool's output, and the two want different presentation:
    inline markdown belongs on the first and would silently restyle the second.
    Deciding it here rather than in the widget keeps the rule testable without a
    screen (ADR-0002) and stops the flattening in :func:`transcript_view` from
    being the only place that knows.

    It defaults to empty rather than to a guess, and empty means "unknown" — a
    consumer that cannot identify a line must fall back to the plainest
    rendering it has, which is the same thing R6 already asks for.
    """

    lines: tuple[str, ...]
    entry_count: int
    committed_lines: int = 0
    kinds: tuple[TranscriptKind, ...] = ()

    def kind_at(self, index: int) -> TranscriptKind | None:
        """The entry kind behind line ``index``, or ``None`` when unknown."""
        if index < 0 or index >= len(self.kinds):
            return None
        return self.kinds[index]

    @property
    def total_lines(self) -> int:
        return len(self.lines)

    @property
    def text(self) -> str:
        return "\n".join(self.lines)


@dataclass(frozen=True)
class SubagentView:
    """KTD8's rows plus the count that stays visible when they collapse (R16)."""

    rows: tuple[SubagentRow, ...]
    active_count: int
    terminal_count: int

    @property
    def collapsed_label(self) -> str:
        """What the collapsed form shows. Never empty while any row exists."""
        return f"{self.active_count} active · {self.terminal_count} finished"


@dataclass(frozen=True)
class PromptRow:
    """Everything a prompt control needs, and nothing an answer would carry.

    The four request-side fields the gateway sends outbound plus the session the
    prompt belongs to. No value field exists here in either direction: the
    answer travels the other way (R9), and a view model with somewhere to put it
    is a view model somebody will eventually put it in.
    """

    request_id: str
    kind: PromptKind
    summary: str
    choices: tuple[str, ...] = ()
    session_id: str | None = None
    #: approval only: the command permission is being asked for, whole. Carried
    #: separately from ``summary`` because the control renders them differently
    #: — the summary is a header line, the command is a wrapped body with a
    #: visible truncation marker — and because ``summary`` for an approval is
    #: the gateway's ``description``, which describes the *warnings* rather than
    #: the command.
    command: str = ""
    read_start: int | None = None
    read_count: int | None = None
    #: False when Talaria cannot name the request its answer would land on, so
    #: the control must not offer one. Today that is approval alone, and only
    #: while more than one is queued in a session — see
    #: :func:`~talaria.domain.state.respond_to_prompt`. Decided here rather than
    #: in the widget so the rule is testable without a screen (ADR-0002), and so
    #: the widget cannot disagree with the registry that will refuse it.
    answerable: bool = True
    #: Why not, in the operator's words. Empty when ``answerable``.
    blocked_reason: str = ""


@dataclass(frozen=True)
class PromptView:
    """Outstanding prompts, so a waiting session cannot look like a busy one."""

    rows: tuple[PromptRow, ...] = ()

    #: How many approvals were withdrawn locally with their fate still unknown.
    #:
    #: A withdrawal empties ``rows``, and an empty ``rows`` is what makes
    #: ``turn_status`` fall back to ``streaming`` — so without this the view
    #: that exists to stop a waiting session looking busy is the exact view
    #: that lets a possibly-blocked one look busy. Projected as a count rather
    #: than as a turn value because KTD5 freezes the turn field at four members;
    #: see :attr:`~talaria.domain.state.SessionState.withdrawn_approvals`.
    withdrawn: int = 0

    #: The gateway's live wait-state note, or ``""``.
    #:
    #: It rides this view because the activity line is its only reader, and the
    #: activity line is a function of this view. Hermes emits it so a long
    #: provider stall can say what it is waiting on; carrying it here is what
    #: lets ``working…`` be replaced by that explanation instead of sitting on
    #: screen next to a transcript that has stopped moving.
    notice: str = ""

    @property
    def request_ids(self) -> tuple[str, ...]:
        return tuple(row.request_id for row in self.rows)

    @property
    def summaries(self) -> tuple[str, ...]:
        return tuple(row.summary for row in self.rows)

    @property
    def pending_count(self) -> int:
        return len(self.rows)


@dataclass(frozen=True)
class StatusPayload:
    """KTD5's frozen v1 status document.

    The field set, the names, and the value types are all frozen — an external
    consumer depends on this shape from the first commit (R19). ``version`` is
    the compatibility signal, and it is the first field so a consumer can branch
    on it before parsing anything else.
    """

    version: int
    mode: RunMode
    connection: ConnectionStatus
    session_id: str
    session_title: str | None
    turn: TurnStatus
    pending_prompts: int
    subagents_active: int
    subagents_terminal: int
    input_tokens: int | None
    output_tokens: int | None

    def to_json_dict(self) -> dict[str, Any]:
        """Serialize to exactly KTD5's document, nesting included."""
        usage: dict[str, int] | None = None
        if self.input_tokens is not None and self.output_tokens is not None:
            usage = {
                "input_tokens": self.input_tokens,
                "output_tokens": self.output_tokens,
            }
        return {
            "version": self.version,
            "mode": self.mode,
            "connection": self.connection,
            "session": {"id": self.session_id, "title": self.session_title},
            "turn": self.turn,
            "pending_prompts": self.pending_prompts,
            "subagents": {
                "active": self.subagents_active,
                "terminal": self.subagents_terminal,
            },
            "usage": usage,
        }


@dataclass(frozen=True)
class Snapshot:
    """One immutable emission plus the names of the regions that moved."""

    transcript: TranscriptView
    subagents: SubagentView
    prompts: PromptView
    status: StatusPayload
    changed: frozenset[str]


#: The regions ``changed`` can name. A UI keyed off a typo would silently never
#: re-render, so the set is published and asserted against.
SNAPSHOT_REGIONS: frozenset[str] = frozenset(
    {"transcript", "subagents", "prompts", "status"}
)


def transcript_view(state: SessionState) -> TranscriptView:
    """Flatten the transcript into the plain-text line buffer (R6, KTD10).

    In-flight streaming text is included as a provisional trailing block. That
    is deliberate: a terminal-read arriving mid-stream should describe the screen
    the operator is looking at, and the alternative — serving only committed
    entries — would answer with a screen that is visibly not the one on display.
    """
    lines: list[str] = []
    kinds: list[TranscriptKind] = []
    for entry in state.transcript:
        entry_lines = _entry_lines(entry)
        lines.extend(entry_lines)
        kinds.extend([entry.kind] * len(entry_lines))
    committed = len(lines)
    if state.streaming_text:
        streamed = state.streaming_text.splitlines() or [""]
        lines.extend(streamed)
        # The in-flight block is the assistant's reply being written. It has no
        # entry yet — that is what "provisional" means — so the kind is stated
        # here rather than read off one, and it is the kind the same text will
        # carry the moment ``message.complete`` commits it.
        kinds.extend(["assistant"] * len(streamed))
    return TranscriptView(
        lines=tuple(lines),
        entry_count=len(state.transcript),
        committed_lines=committed,
        kinds=tuple(kinds),
    )


def _entry_lines(entry: TranscriptEntry) -> list[str]:
    prefix = _ENTRY_PREFIX.get(entry.kind, "")
    body = f"{prefix}{entry.text}" if prefix else entry.text
    return body.split("\n")


#: Plain-text speaker markers. Markdown, diff, and reasoning-block presentation
#: are out of scope for v0.1 (R6) — these prefixes are the whole of the
#: rendering, and they exist so a reader can tell an agent line from a tool line
#: in a buffer that has no colour.
_ENTRY_PREFIX: dict[str, str] = {
    "user": "› ",
    "assistant": "",
    "reasoning": "· ",
    "tool": "",
    "subagent": "  ",
    "system": "— ",
    "prompt": "? ",
    "prompt-expired": "? ",
    "cancelled": "",
    "error": "! ",
    "protocol-error": "! ",
    "unknown-event": "! ",
}


def subagent_view(state: SessionState, *, now: float | None = None) -> SubagentView:
    """Project sub-agent state into KTD8's five-field rows.

    ``elapsed`` is measured to ``now`` for a live row and frozen at its last
    update for a terminal one — a finished child's elapsed time is a property of
    the work it did, not of how long the operator has since been looking at it.
    """
    observed = state.last_observed_at if now is None else now
    rows: list[SubagentRow] = []
    active = 0
    terminal = 0
    for item in state.subagents:
        end = item.updated_at if item.is_terminal else observed
        rows.append(
            SubagentRow(
                id=item.id,
                name=item.name,
                status=item.status,
                elapsed=max(0.0, end - item.started_at),
                detail=item.detail[-1] if item.detail else None,
            )
        )
        if item.is_terminal:
            terminal += 1
        else:
            active += 1
    return SubagentView(rows=tuple(rows), active_count=active, terminal_count=terminal)


def prompt_view(state: SessionState) -> PromptView:
    """Project the registry, marking any prompt whose answer cannot be aimed.

    The only such prompt is an approval sharing its session with another
    outstanding approval. ``approval.respond`` carries no discriminator and
    resolves the oldest queue entry, and the gateway drops entries on timeout
    without telling anyone, so with two on screen the operator cannot know which
    command their answer reaches. The projection marks both, and the card turns
    into a read-only summary plus the one action that needs no correlation.

    "Outstanding" here means outstanding *at the gateway*, which is why
    :meth:`~talaria.domain.state.SessionState.outstanding_approvals` searches
    the in-flight set too. An approval answered a fraction of a second ago is
    still in the gateway's queue until the reply lands, so a second approval
    arriving inside that window is exactly as unaimable as one arriving beside
    it — and it is the case the operator is *most* likely to hit, because the
    interface invites the second press the moment the first one is sent.
    """
    ambiguous: set[str] = set()
    for session in {p.session_id for p in state.prompts if p.kind == "approval"}:
        queued = state.outstanding_approvals(session)
        if len(queued) > 1:
            ambiguous.update(p.request_id for p in queued)
    return PromptView(
        rows=tuple(
            PromptRow(
                request_id=p.request_id,
                kind=p.kind,
                summary=p.summary,
                choices=p.choices,
                session_id=p.session_id,
                command=p.command,
                read_start=p.read_start,
                read_count=p.read_count,
                answerable=p.request_id not in ambiguous,
                blocked_reason=(
                    UNCORRELATED_APPROVAL if p.request_id in ambiguous else ""
                ),
            )
            for p in state.prompts
        ),
        withdrawn=state.withdrawn_approvals,
        notice=state.thinking_notice,
    )


def turn_status(state: SessionState) -> TurnStatus:
    """Derive KTD5's four-value turn field.

    A streaming turn with an outstanding prompt reports ``waiting``, not
    ``streaming``. R8 asks that a session waiting on the operator never look
    like a session that is working, and this is the one field an external status
    consumer reads to tell the difference.
    """
    if state.prompts and state.turn != "cancelled":
        return "waiting"
    if state.turn == "streaming":
        return "streaming"
    if state.turn == "cancelled":
        return "cancelled"
    return "idle"


def status_payload(state: SessionState, *, mode: RunMode) -> StatusPayload:
    """Project domain state into KTD5's v1 document.

    No terminal-framework type and no credential-bearing value can appear here,
    because every field is either a counter, an enum member, or a session
    identifier the gateway already published (R20).
    """
    view = subagent_view(state)
    return StatusPayload(
        version=STATUS_PAYLOAD_VERSION,
        mode=mode,
        connection=state.connection,
        session_id=state.focused_session_id or "",
        session_title=state.session_title,
        turn=turn_status(state),
        pending_prompts=len(state.prompts),
        subagents_active=view.active_count,
        subagents_terminal=view.terminal_count,
        input_tokens=state.usage.input_tokens if state.usage.observed else None,
        output_tokens=state.usage.output_tokens if state.usage.observed else None,
    )


def project(
    state: SessionState,
    *,
    mode: RunMode = "replay",
    now: float | None = None,
    previous: Snapshot | None = None,
) -> Snapshot:
    """Emit one immutable snapshot and name the regions that changed."""
    transcript = transcript_view(state)
    subagents = subagent_view(state, now=now)
    prompts = prompt_view(state)
    status = status_payload(state, mode=mode)

    if previous is None:
        changed = frozenset(SNAPSHOT_REGIONS)
    else:
        changed = frozenset(
            name
            for name, current, before in (
                ("transcript", transcript, previous.transcript),
                ("subagents", subagents, previous.subagents),
                ("prompts", prompts, previous.prompts),
                ("status", status, previous.status),
            )
            if current != before
        )

    return Snapshot(
        transcript=transcript,
        subagents=subagents,
        prompts=prompts,
        status=status,
        changed=changed,
    )


# ── Terminal read (KTD10 / PC9) ──────────────────────────────────────────


@dataclass(frozen=True)
class TerminalReadResponse:
    """The gateway's ``read_terminal`` response shape, served truthfully.

    Field names follow the contract exactly, including its asymmetry: the
    *request* names the first line ``start_line`` while the *response* names it
    ``start`` (``tools/read_terminal_tool.py:28-31``, ``:64``).

    ``cursor_row`` is always ``None``. Talaria's transcript genuinely has no
    caret — it is a scrolled, read-only region, and the only caret on screen
    belongs to the composer, which is not part of what the agent asked to read.
    Serving the composer's caret, or synthesising a plausible row, would hand
    the agent a confident wrong answer.
    """

    total_lines: int
    start: int
    end: int
    viewport_rows: int
    text: str
    cursor_row: None = None

    def to_json_dict(self) -> dict[str, Any]:
        return asdict(self)


def terminal_read(
    view: TranscriptView | None,
    *,
    viewport_rows: int = DEFAULT_VIEWPORT_ROWS,
    start_line: int | None = None,
    count: int | None = None,
) -> TerminalReadResponse:
    """Select lines from the transcript buffer per the gateway's own contract.

    Two semantics come straight from that contract and are not open to
    reinterpretation (``tools/read_terminal_tool.py:18-31``, ``:58-70``):

    * **Omitting both arguments means the visible screen**, not the whole
      transcript. An agent that asks with no arguments is asking what is on
      screen; answering with 40,000 lines of scrollback is a different question.
    * **Valid lines are the half-open range ``[0, total_lines)``**, and the
      source clamps with a floor of ``0`` for ``start`` and ``1`` for ``count``
      (``:29-30``).

    An empty transcript answers honestly with ``total_lines: 0`` and empty text.
    A missing view raises :class:`ProjectionUnavailableError` so the caller sends
    nothing at all rather than a fabricated screen.
    """
    if view is None:
        raise ProjectionUnavailableError(
            "the transcript projection is unavailable; no terminal-read response is sent"
        )

    rows = max(1, viewport_rows)
    total = view.total_lines

    if start_line is None and count is None:
        start = max(0, total - rows)
        window = rows
    else:
        start = max(0, start_line) if start_line is not None else 0
        window = max(1, count) if count is not None else rows

    start = min(start, total)
    end = min(total, start + window)
    text = "\n".join(view.lines[start:end])

    return TerminalReadResponse(
        total_lines=total,
        start=start,
        end=end,
        viewport_rows=rows,
        text=text,
    )
