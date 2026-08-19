"""The approval path and the four blocking-prompt bridges (R7, R8, R9; PC9, F2).

Hermes blocks a turn on five questions and waits for a client to answer. Four of
them are what the gateway itself calls its "blocking bridges" — ``secret``,
``sudo``, ``clarify`` and ``terminal.read``, which share one lifecycle at
``tui_gateway/server.py:2958-2998`` (read at Hermes ``7f4d15515``): the tool
blocks on a threading event, the gateway emits ``<bridge>.expire`` when the wait
times out, and every ``*.respond`` handler is registered with
``allow_expired=True`` so a late answer resolves as ``{"status": "expired"}``
instead of erroring. The fifth is approval, which does not use that bridge at
all — it resolves by session key (``tui_gateway/methods_prompt.py:886-905``) and
carries no request id on the wire.

**Three rules hold across all five, and each one exists because the obvious
implementation breaks a different requirement.**

*The value never travels back the way it came.* Everything in a request payload
is gateway-authored and safe to render; everything in a respond payload was
typed by the operator or scraped off their screen. So this module builds respond
params and holds hidden inputs, and it has no path by which an answer reaches a
summary, a notice, a log line, or the status document (R9). The recorder's
deny-set covers the same four fields on disk (``docs/formats/frame-log.md``);
this is the same rule enforced one layer earlier, at the point the value exists.

*A control that is on screen is not necessarily a control the gateway is still
listening to.* Expiry clears the widget and leaves a permanent transcript trace,
and the answer path re-checks the registry, so a keystroke that lands a
microsecond after the expiry sends nothing (R8).

*Terminal-read is answered without a human.* F2 asks for the screen "without
creating a human overlay", so it never renders a control — the app answers it
from the KTD10 projection and, when the projection cannot answer, sends nothing
and says so locally rather than inventing a screen.

*An answer that cannot be aimed is not sent.* Approval is the one bridge with no
correlating id in either direction, and its resolver takes no discriminator: it
pops the oldest entry in the session's queue. While exactly one approval is
outstanding that is unambiguous. While more than one is, it is not — the gateway
also drops entries on timeout without emitting anything — so the card shows the
command and withholds the affirmative, and the only action offered is denying
the whole queue at once, which needs no correlation. The rule itself lives in
:func:`talaria.domain.state.respond_to_prompt` and the reasoning is recorded in
``docs/engineering-journal/DECISIONS.md``.

Nothing here decides *whether* an answer may be sent. That is the registry's
job, in :func:`talaria.domain.state.respond_to_prompt`, where it is testable
without a terminal (ADR-0002).
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any, ClassVar, Final

from rich.cells import chop_cells
from textual import events
from textual.app import ComposeResult
from textual.binding import Binding, BindingType
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.message import Message
from textual.widget import Widget
from textual.widgets import Button, Input, Static

from talaria.domain.models import PromptKind, TurnStatus
from talaria.domain.projection import PromptRow, PromptView
from talaria.ui.focus import CaretReleased, holds_caret
from talaria.ui.literal import defang, literal_text

__all__ = [
    "ANSWER_HINT",
    "CHOICE_HINT",
    "COMMAND_MIN_WIDTH",
    "COMMAND_PREVIEW_LINES",
    "CONTROL_OFFSCREEN_TITLE",
    "DECLINE_VALUES",
    "DENY_ALL_CHOICE",
    "DENY_ALL_HINT",
    "EMPTY_ANSWER",
    "GATEWAY_DISCARDED_ANSWER",
    "GATEWAY_HAD_NO_APPROVAL",
    "HIDDEN_KINDS",
    "NO_CHOICES_FALLBACK",
    "RESPOND_METHODS",
    "RESPOND_VALUE_FIELDS",
    "UNATTENDED_KINDS",
    "WAITING_TITLE",
    "WITHDRAWN_ACTIVITY_TEMPLATE",
    "CommandPanel",
    "PromptRegion",
    "activity_line",
    "attended_rows",
    "command_overflow_line",
    "decline_value",
    "echoable_answer",
    "gateway_refusal",
    "respond_params",
    "withdrawn_activity_line",
    "wrap_command",
]

#: One row per bridge: ``(kind, respond method, params key carrying the value)``.
#:
#: A single table rather than two dictionaries, so a bridge cannot be added to
#: one and forgotten in the other — a method with no field entry raises at the
#: moment the operator presses a key, which is the worst place to discover it.
#:
#: The methods are the gateway's own ``@method(...)`` registrations
#: (``tui_gateway/methods_prompt.py:858-905`` at Hermes ``7f4d15515``). Four of
#: the field names are the literal ``key`` argument passed to ``_respond``
#: (``:864``, ``:873``, ``:878``, ``:883``); approval's ``choice`` is what its
#: own handler reads at ``:895``. Those same four fields are what the recorder's
#: deny-set withholds (``talaria/recorder/redact.py``), and the test suite
#: asserts the two lists agree — a disagreement means Talaria sends a value the
#: frame log then writes in plaintext.
_BRIDGES: Final[tuple[tuple[PromptKind, str, str], ...]] = (
    ("approval", "approval.respond", "choice"),
    ("clarify", "clarify.respond", "answer"),
    ("secret", "secret.respond", "value"),
    ("sudo", "sudo.respond", "password"),
    ("terminal_read", "terminal.read.respond", "text"),
)

#: The JSON-RPC method each bridge is answered with.
RESPOND_METHODS: Final[dict[PromptKind, str]] = {
    kind: method for kind, method, _field in _BRIDGES
}

#: The ``params`` key each respond carries its value in.
RESPOND_VALUE_FIELDS: Final[dict[PromptKind, str]] = {
    kind: field for kind, _method, field in _BRIDGES
}

#: Bridges whose answer is a credential the operator types. Their input never
#: echoes, and no part of the interface ever holds the value after the call.
HIDDEN_KINDS: Final[frozenset[PromptKind]] = frozenset({"secret", "sudo"})

#: Bridges Talaria answers itself. F2: terminal-read is served from the
#: projection "without creating a human overlay", so it renders no control.
UNATTENDED_KINDS: Final[frozenset[PromptKind]] = frozenset({"terminal_read"})

#: What an approval offers when the gateway sent no ``choices`` list.
#:
#: The gateway normally fills one in (``tui_gateway/server.py:1663-1670``:
#: ``once``/``session``/``always``/``deny``, narrowed by ``smart_denied`` and
#: ``allow_permanent``), but only when ``allow_permanent`` is present in the
#: payload — so an approval with neither is reachable and would otherwise be
#: unanswerable. Deny is the only safe thing to synthesize: an affirmative the
#: gateway did not offer is a client inventing permission, and the shipping
#: terminal UI already treats deny as the choice that needs no offer (it sends
#: ``{choice: 'deny'}`` on escape, ``ui-tui/src/app/useInputHandlers.ts:182``).
NO_CHOICES_FALLBACK: Final[tuple[str, ...]] = ("deny",)


#: The only choice Talaria will ever apply to a whole approval queue at once.
#:
#: ``approval.respond`` accepts ``all: true`` and hands the choice to every
#: queued entry (``tools/approval.py:2219-2226``). Applied as a denial that is
#: the safe escape from a queue nobody can aim at; applied as an affirmative it
#: would grant permission for commands the operator has not read, which is the
#: exact harm the uncorrelated-approval rule exists to prevent. So the value is
#: a constant here rather than a parameter.
DENY_ALL_CHOICE: Final[str] = "deny"

#: What a **decline** puts on the wire, per bridge (R3, KTD4).
#:
#: These are not values invented here. They are what the gateway's own terminal
#: client sends down its escape path: the empty field value for clarify, sudo
#: and secret (``ui-tui/src/app/useInputHandlers.ts:119``, ``:128`` at Hermes
#: ``7f4d15515``) and the explicit ``deny`` choice for an approval (``:180``).
#: Declining therefore needs no new method and no protocol change — it is an
#: ordinary ``*.respond`` carrying the value that client already sends.
#:
#: **Approval's entry is the one that must never be the empty string, and the
#: reason is not symmetry.** The gateway's approval consumer blocks on ``None``
#: and on ``"deny"`` and returns *approved* for every other resolved choice
#: (``tools/approval.py:3584``, ``:3679``), so an approval "declined" with an
#: empty choice would authorize the command the operator was trying to refuse.
#: The value lives in this table rather than being derived at the call site so
#: there is exactly one place that decision is written down.
#:
#: ``terminal_read`` is deliberately absent rather than mapped to ``""``.
#: Talaria answers it itself and it renders no card
#: (:data:`UNATTENDED_KINDS`), so there is nothing for an operator to decline;
#: a missing key makes :func:`decline_value` return ``None`` and every caller
#: skip it, where an empty-string entry would have them send one.
#: The field value that declines every bridge except approval: nothing at all.
#:
#: Named rather than written as a bare ``""`` in the table below, and the name
#: is doing two jobs. It says what an empty string *means* here — "the operator
#: refused", not "the operator typed nothing" — and it keeps a string literal
#: out of a mapping whose keys are ``secret`` and ``sudo``, which bandit reads
#: as a hardcoded credential (B105). A suppression comment would have claimed
#: the finding was wrong; there is simply no credential to write down.
EMPTY_ANSWER: Final[str] = ""

DECLINE_VALUES: Final[dict[PromptKind, str]] = {
    "approval": DENY_ALL_CHOICE,
    "clarify": EMPTY_ANSWER,
    "secret": EMPTY_ANSWER,
    "sudo": EMPTY_ANSWER,
}

#: How many wrapped rows of an approval's command the card shows before it says
#: how many it is not showing.
#:
#: The shipping terminal UI uses ten (``CMD_PREVIEW_LINES``,
#: ``ui-tui/src/components/prompts.tsx:16``). Talaria uses fewer because its
#: card carries a header and a button row inside a border, and two approvals can
#: be on screen at once — ten rows each would push the second card, and the
#: transcript above it, off an 80x24 terminal. The number that matters for
#: safety is not this one: it is that whatever is *not* shown is announced, so
#: a truncated command is never mistaken for a short one.
COMMAND_PREVIEW_LINES: Final[int] = 6

#: The hint line under a card whose control is a set of buttons — an approval
#: or a multiple-choice clarify. Follows the picker's own hint convention
#: (``LIST_HINT``, ``talaria/domain/selection.py:40``): every key that does
#: something, named once, on the line the control's keys actually are (U1,
#: R1). ``esc decline`` names a key U2 binds; the hint is written now because
#: a control that is reachable in one press and answerable but silent about
#: how to say no is only half the requirement R1/R3 together describe.
CHOICE_HINT: Final[str] = "enter select · esc decline"

#: The hint line under a card whose control is free text — clarify, secret or
#: sudo with no gateway-offered choices. The control's own placeholder
#: (``"answer · Enter sends"``) already names the send key; this line adds
#: the one key the placeholder does not.
ANSWER_HINT: Final[str] = "enter send · esc decline"

#: The hint line under an unanswerable approval — a queue with more than one
#: entry offers only "deny all" (the uncorrelated-approval rule this module's
#: docstring names). No separate ``esc decline`` is listed here: denying the
#: whole queue already *is* the decline for every prompt on this card
#: (KTD8's per-kind sweep resolves outstanding approvals with the same
#: deny-all call), so naming a second key would promise a control the card
#: does not carry.
DENY_ALL_HINT: Final[str] = "enter deny all"

#: A card's border title while its answering control is on screen.
WAITING_TITLE: Final[str] = "waiting for you"

#: A card's border title while its answering control is *not* on screen.
#:
#: **The title is where this has to be said, because the title is what is still
#: visible.** A control pushed past the region's bottom edge takes everything
#: below it with it, so the only parts of the card left to read are the border
#: and the rows above the control — which is exactly why the card went on
#: reading as live. Replacing the phrase that made it read that way is the
#: whole fix: the card stops claiming to be waiting for an answer it has
#: nowhere to take, and says where its control went instead.
#:
#: "below" is always the true direction when this string is legible. The
#: control sits under the summary and the command, so a card whose title is on
#: screen while its control is not has that control beneath the fold; a card
#: scrolled off the top is not on screen to mislead anyone.
CONTROL_OFFSCREEN_TITLE: Final[str] = "answer below — scroll"

#: The narrowest the command body is ever wrapped to.
#:
#: Below this the wrap stops being readable and starts being a column of
#: fragments, so a terminal narrower than the card's chrome gets a body that
#: overflows its container rather than one chopped into two-character pieces.
#: An overflowing body is visible; a shredded one looks like a different
#: command.
COMMAND_MIN_WIDTH: Final[int] = 20

#: Announces the rows of the command the card is not showing.
#:
#: A truncation that does not say it truncated is the defect, not the
#: truncation: an operator reading ``rm -rf /home/build`` off a card that has
#: silently dropped ``&& rm -rf /`` approves the wrong thing. The whole command
#: is in the transcript, unclipped, which is what the second clause points at.
COMMAND_OVERFLOW_TEMPLATE: Final[str] = "… +{count} more line{plural} (whole command in transcript)"

#: The gateway accepted the frame and threw the answer away.
GATEWAY_DISCARDED_ANSWER: Final[str] = (
    "the gateway had already stopped waiting — the answer was discarded"
)

#: The gateway accepted the frame and had no approval left to resolve.
GATEWAY_HAD_NO_APPROVAL: Final[str] = (
    "the gateway had no approval waiting — nothing was resolved"
)

#: What the activity line says in place of ``working…`` after a withdrawal.
#:
#: ``{count}`` and ``{plural}`` are filled by :func:`withdrawn_activity_line`.
#:
#: The sentence asserts one thing and refuses another. It asserts the
#: withdrawal, which is an act of Talaria's and therefore knowable. It refuses
#: to say whether the agent resumed, because that is the one thing this state
#: exists to admit is unknown.
#:
#: Sixty-eight cells at ``count=1``, which is what keeps it inside an 80-column
#: terminal. The activity line is one row and does not wrap, so a longer
#: sentence loses its own operative clause — and the clause that would go is
#: ``is unknown``, which is the whole point of the line.
WITHDRAWN_ACTIVITY_TEMPLATE: Final[str] = (
    "{count} approval{plural} withdrawn — whether the agent is still blocked is unknown"
)


def withdrawn_activity_line(count: int) -> str:
    """The line that replaces ``working…`` while a withdrawal is unresolved."""
    return WITHDRAWN_ACTIVITY_TEMPLATE.format(count=count, plural="" if count == 1 else "s")


def command_overflow_line(hidden: int) -> str:
    """The marker that says a command is longer than the card is showing."""
    return COMMAND_OVERFLOW_TEMPLATE.format(count=hidden, plural="" if hidden == 1 else "s")


def wrap_command(
    command: str, width: int, *, limit: int = COMMAND_PREVIEW_LINES
) -> tuple[str, ...]:
    """Lay a command out as rows the card can render, truncating **visibly**.

    Three properties, and each one is a way the operator could otherwise be
    shown something other than what they are approving:

    *It wraps rather than clips.* A command longer than the terminal is wide is
    the ordinary case for the commands that need approving — a redirect, a pipe,
    a second statement after ``&&`` all live at the end. Clipping the tail hides
    exactly the part that makes a command dangerous, and it hides it *silently*,
    because a clipped row looks like a row that ended.

    *It wraps by cell, not by word.* ``chop_cells`` cuts at a fixed rendered
    width and keeps every character in order. Word wrapping would be prettier
    and would also be free to move whitespace, and whitespace is syntax in a
    shell command — a reader deciding about ``rm -rf /`` needs the characters,
    not the prose. Newlines in the command are honoured first, so a multi-line
    command stays multi-line and each of its lines wraps on its own.

    *What is not shown is counted and announced.* Anything past ``limit`` rows
    is replaced by :func:`command_overflow_line`, so the card can be short
    without ever being quietly incomplete.

    Tabs are expanded before wrapping. A tab is one character and eight cells,
    so a command containing one would wrap at the wrong column and the rows
    would not line up with what the terminal draws.
    """
    inner = max(COMMAND_MIN_WIDTH, width)
    rows: list[str] = []
    for source_line in defang(command).expandtabs(8).split("\n"):
        rows.extend(chop_cells(source_line, inner) or [""])
    if len(rows) <= limit:
        return tuple(rows)
    return (*rows[:limit], command_overflow_line(len(rows) - limit))


def respond_params(
    kind: PromptKind,
    *,
    request_id: str,
    session_id: str | None,
    value: str,
    all_approvals: bool = False,
    observed_request_id: str = "",
) -> dict[str, Any]:
    """Build the exact ``params`` one bridge's respond method reads.

    Approval is shaped differently from the other four and it is not an
    oversight in this function. ``approval.respond`` takes ``session_id`` and
    resolves the pending approval from the session's own queue
    (``methods_prompt.py:887-905``).

    **It also carries the gateway's own request id when one was observed** —
    R18 as amended on 2026-08-17, reversing this function's original
    unsent-always choice. Three facts moved it. The running revision synthesizes
    a ``request_id`` on every approval entry (``tools/approval.py:2596``) and
    emits it with the request event; ``approval.respond`` forwards the parameter
    at that revision; and the gateway removes queue heads on timeout and
    interrupt without emitting anything, so an uncorrelated answer can authorize
    a command the operator was never shown.

    ``observed_request_id`` is the gateway's id and never Talaria's synthesized
    registry key — see
    :attr:`~talaria.domain.models.PendingPrompt.observed_request_id`. That
    distinction is what the original docstring was protecting: a local key on the
    wire would be a field the gateway ignores, reading to the next person as
    though correlation were happening when it is not. A gateway that does not
    read the parameter ignores it — U1 verified live that a bogus id is accepted
    and disregarded rather than refused — so sending it degrades to today's
    behaviour rather than breaking it.

    ``all_approvals`` sets the gateway's own ``all`` flag, which resolves every
    queued approval in the session with one choice. It is the only answer that
    is correct without correlation, and :data:`DENY_ALL_CHOICE` is the only
    value it is ever sent with — so it is never aimed, and no id rides with it.
    """
    field = RESPOND_VALUE_FIELDS[kind]
    if kind == "approval":
        params: dict[str, Any] = {"session_id": session_id or "", field: value}
        if all_approvals:
            params["all"] = True
        elif observed_request_id:
            params["request_id"] = observed_request_id
        return params
    return {"request_id": request_id, field: value}


def decline_value(kind: PromptKind) -> str | None:
    """What declining this bridge sends, or ``None`` when it cannot be declined.

    A lookup with a name, so that no caller writes ``""`` for an approval by
    reaching for the obvious symmetry: an empty approval choice is *approved*
    by the gateway's consumer (:data:`DECLINE_VALUES` carries the citation).
    ``None`` means the kind renders no control for a human to escape from —
    today that is ``terminal_read`` alone.
    """
    return DECLINE_VALUES.get(kind)


def gateway_refusal(kind: PromptKind, result: Any) -> str | None:
    """Read the reply *body* for the two ways a success means "discarded".

    A JSON-RPC success is not the same claim as "the answer was used", and on
    this gateway the difference is carried in the body rather than in the
    envelope. Both cases below return ``_ok`` and would otherwise be written into
    the transcript as ``answered``:

    * Every bridge's ``*.respond`` is registered ``allow_expired=True``, and
      when the request is no longer pending ``_respond`` answers
      ``{"status": "expired"}`` (``tui_gateway/server.py:10228-10235``). The
      answer is dropped on the floor.
    * ``approval.respond`` returns ``{"resolved": n}`` — the number of queue
      entries it actually released (``methods_prompt.py:886-905`` over
      ``tools/approval.py:2214-2231``). Zero means the queue was empty: the
      approval had already timed out or been resolved elsewhere.

    Returns the operator-facing reason, or ``None`` when the gateway used the
    answer. Anything unrecognized is treated as used, because inventing a
    failure from a shape nobody has seen is the mirror of the defect this
    function fixes.
    """
    if not isinstance(result, Mapping):
        return None
    if result.get("status") == "expired":
        return GATEWAY_DISCARDED_ANSWER
    if kind == "approval":
        resolved = result.get("resolved")
        if isinstance(resolved, int) and not isinstance(resolved, bool) and resolved <= 0:
            return GATEWAY_HAD_NO_APPROVAL
    return None


def echoable_answer(row: PromptRow, value: str) -> str | None:
    """The answer's transcript form, or ``None`` when it must not be written.

    One rule covers all five bridges: **an answer is written to the transcript
    only when it is a member of the ``choices`` list the gateway itself sent.**

    A choice is gateway-authored, closed, and the single most useful thing in an
    audit trail — "did I allow that command" is the question an operator asks
    afterwards, and a transcript that only says "approval answered" cannot
    answer it. Everything else is operator-typed: a sudo password, a secret, a
    clarify reply to a question that may well have been "paste the token here".
    The recorder withholds exactly those from disk, and the transcript is the
    less obvious egress of the two — the terminal-read bridge serves it straight
    back to the agent, so a credential written into it leaves the machine by a
    door nobody was watching.

    Matching against the offered list rather than against the kind is what makes
    the rule one sentence instead of five, and it degrades in the safe
    direction: a clarify with no choices, or a free-typed answer that happens
    not to match, is withheld.
    """
    if value and value in row.choices:
        return value
    return None


def attended_rows(view: PromptView) -> tuple[PromptRow, ...]:
    """The prompts that actually put a control in front of the operator.

    The same filter :meth:`PromptRegion.apply` mounts by, exported so the
    activity line cannot disagree with the cards beneath it. It used to: the
    line read ``view.rows`` while ``apply`` filtered :data:`UNATTENDED_KINDS`,
    so a terminal-read rendered ``waiting for you — terminal read: terminal read
    requested`` above an empty region. Two readings of "which prompts need a
    human" is one reading too many, and the two drifted in the direction that
    invents work for the operator.
    """
    return tuple(row for row in view.rows if row.kind not in UNATTENDED_KINDS)


def activity_line(turn: TurnStatus, view: PromptView) -> str:
    """One line that never lets waiting-on-a-human look like working (R8).

    The distinction is the requirement, not the wording. A session blocked on an
    approval has an in-flight turn and a live socket, so every signal that says
    "busy" is still true — which is exactly why a busy indicator is the wrong
    one: the operator concludes the machine is thinking and waits for something
    that is waiting for them. So ``waiting`` names what is wanted and from whom,
    and it takes precedence over ``streaming`` because the projection already
    resolved that precedence once (``turn_status``) and two resolutions of one
    question drift.

    **A prompt Talaria answers itself is not something a human is waited on
    for, so it is not named here and it is not counted here.** ``turn_status``
    reports ``waiting`` for any outstanding prompt — correctly, because the
    agent's turn really is blocked — but "waiting" at the status layer and
    "waiting for you" on the screen are different claims. F2 asks that
    terminal-read be served "without creating a human overlay", and a line
    telling the operator they are being waited on for a question that has
    already been answered on the wire is a human overlay with no control under
    it. It was also the second-worst kind of wrong when an approval was queued
    beside it: the read has no control and got named, and the approval that has
    one got relegated to ``(+1 more)``.

    **A withdrawn approval takes the ``streaming`` slot, and only that slot.**
    Ageing an approval out empties the registry, which sends ``turn_status``
    back to ``streaming`` and put ``working…`` on screen for a session Talaria
    had just stopped offering the operator any way to unblock. Under the
    gateway's default wait the agent has most likely resumed and ``working…``
    is true; under a deployment whose approval timeout exceeds Talaria's own,
    the gateway is still holding and it is false. Talaria cannot tell those
    apart, so the line says so rather than picking the reading it prefers. It
    replaces only ``working…``: a live prompt outranks it because that is
    something the operator can act on, and an idle turn is not claiming
    anything that needs correcting — the withdrawal's permanent record is in
    the transcript either way.

    **The gateway's own wait note takes the same slot, below the withdrawal.**
    ``thinking.delta`` exists because a reasoning model can stall for minutes
    with nothing on the wire, and Hermes wrote it to replace a bare spinner with
    an account of what the stall is (``run_agent._emit_wait_notice``). That is
    strictly more than ``working…`` says, so it wins that slot — but only that
    one. It loses to the withdrawal line, which is not information but a
    correction: the withdrawal case is the one where ``working…`` may be
    *false*, and a note saying the gateway is indexing does nothing to stop an
    operator believing a session Talaria can no longer unblock is progressing.
    Losing to ``waiting for you`` needs no separate rule — the precedence above
    already decided it.
    """
    attended = attended_rows(view)
    if turn == "waiting" and attended:
        head = attended[0]
        suffix = f" (+{len(attended) - 1} more)" if len(attended) > 1 else ""
        return f"waiting for you — {head.kind.replace('_', ' ')}: {head.summary}{suffix}"
    if turn == "streaming":
        if view.withdrawn:
            return withdrawn_activity_line(view.withdrawn)
        return view.notice or "working…"
    if turn == "cancelled":
        return "interrupted"
    return ""


class CommandPanel(Static):
    """The command an approval is asking permission to run, rendered whole.

    A widget rather than a plain ``Static`` because the wrap depends on the
    width it is *actually rendered at*, and that is not known when the card
    composes. Textual delivers :class:`~textual.events.Resize` once layout has
    placed the widget and again whenever the terminal changes size, so the rows
    are recomputed there and the card stays honest at any width.

    Until that first resize arrives the content is the command as one piece of
    literal text, which soft-wraps. That fallback is deliberately the *safe*
    direction: it shows more than the cap rather than less, so no arrangement of
    events can produce a card that is silently missing part of a command.
    """

    DEFAULT_CSS = """
    CommandPanel {
        height: auto;
        width: 1fr;
        color: $text;
        padding: 0 0 0 1;
    }
    """

    class Rewrapped(Message):
        """The body re-laid itself out *after* its height had already been placed.

        Posted rather than handled here because the consequence is not local:
        the panel growing pushes the card's own controls down inside a
        :class:`PromptRegion` that has already decided how tall it is, and only
        the region can do anything about that. See
        :meth:`PromptRegion.on_command_panel_rewrapped`.
        """

    def __init__(self, command: str, **kwargs: object) -> None:
        super().__init__(literal_text(command), markup=False, **kwargs)  # type: ignore[arg-type]
        self.command = command
        self._width = 0

    @property
    def rendered_width(self) -> int:
        """The width the rows were last laid out for. ``0`` before first layout."""
        return self._width

    @property
    def rows(self) -> tuple[str, ...]:
        """The rows currently on screen, including any overflow marker."""
        if self._width <= 0:
            return tuple(defang(self.command).expandtabs(8).split("\n"))
        return wrap_command(self.command, self._width)

    def on_resize(self, event: events.Resize) -> None:
        # ``content_size``, not ``event.size``. ``event.size`` is the widget's
        # outer size and still contains this widget's own padding, so wrapping
        # to it produces rows one or two cells too long — and Rich then soft-
        # wraps each of those rows a second time. The visible result is not a
        # clipped command but a *shredded* one: every row broken again a few
        # characters from its end, so ``rm -rf /var`` appears with ``/var`` on
        # its own line and the cap of eight rows renders as fourteen, pushing
        # the truncation marker off the card it was there to keep honest.
        width = self.content_size.width or event.size.width
        if width <= 0 or width == self._width:
            return
        self._width = width
        self.update(literal_text("\n".join(wrap_command(self.command, width))))
        # The row count almost always *changes* here, and on a narrowing it
        # only ever grows — a 346-character command is four rows at 120 columns
        # and seven at 60. The card is taller than the layout that placed it
        # from this moment until the region does something about it.
        self.post_message(self.Rewrapped())


class PromptCard(Vertical):
    """One outstanding prompt, rendered in place rather than over the transcript.

    The card owns its ``request_id`` and hands it back with every answer. That
    is the client half of R9's correlation clause: the widget cannot answer a
    question other than the one it was mounted for, whatever else moved on
    screen in between.
    """

    #: **Every control here is built with ``compact=True``, and the card's own
    #: CSS never touches a control's border.** That is not a style preference —
    #: it is the only arrangement in which the controls have any height at all.
    #:
    #: Textual's ``Button`` carries its chrome as ``border-top: tall`` plus
    #: ``border-bottom: tall`` (two rows) declared inside its ``&.-style-default``
    #: block, and ``Input`` re-declares ``border: tall`` inside ``&:focus``.
    #: Both of those selectors carry a class or pseudo-class, so they outrank a
    #: plain descendant selector like ``PromptCard Button``: a ``border: none``
    #: written here loses the cascade and the tall border survives. Pair that
    #: with ``height: 1`` and the content box is ``1 - 2 = -1``, clamped to
    #: **zero rows** — the widget is laid out, focusable and reported as
    #: mounted, and renders no label, no placeholder, no caret and nothing the
    #: operator types. The card showed a question and two rows of border
    #: characters.
    #:
    #: ``compact=True`` is the framework's own answer: it sets
    #: ``-textual-compact``, whose rules use ``!important`` and therefore *do*
    #: win, on both widgets. So the sizing is left to the framework and this
    #: block only positions things.
    DEFAULT_CSS = """
    PromptCard {
        height: auto;
        border: round $warning;
        border-title-align: left;
        padding: 0 1;
    }
    PromptCard > Horizontal {
        height: auto;
    }
    PromptCard Button {
        min-width: 8;
        margin: 0 1 0 0;
    }
    PromptCard > .prompt--blocked {
        height: auto;
        color: $error;
    }
    PromptCard > .prompt--hint {
        height: auto;
        color: $text-muted;
    }
    /* R2: legible against the default terminal theme, and a *second* focus
       style from the two that already existed (Button's own reverse video,
       AgentRow.-interruptible:focus's tint, talaria/ui/agents.py:127-129) —
       neither told the operator which *card* the caret was on. Card-level
       rather than on the control itself, because the card carries the
       question and the command; a tint on the button alone leaves both of
       those reading as plain, unfocused text. ``:focus-within`` rather than
       ``:focus``: the card itself never receives the caret, its Input or its
       Button does, and the tint has to track the descendant. */
    PromptCard:focus-within {
        background: $accent 20%;
    }
    """

    #: ``escape`` while this card's own control holds the caret declines the
    #: prompt (R3, KTD4). The binding lives on the card rather than on the app
    #: because Textual resolves a key against the focused widget's ancestors
    #: (``Screen._modal_binding_chain`` walks ``focused.ancestors_with_self``),
    #: so a card-level binding fires exactly when one of *this* card's controls
    #: is focused and never when the caret is anywhere else — which is what
    #: KTD4 requires: ``escape`` in the composer stays unbound, so that a
    #: double-press can never be destructive.
    #:
    #: ``show=False`` because the card already names the key on its own hint
    #: line (:data:`CHOICE_HINT`, :data:`ANSWER_HINT`), where the operator is
    #: looking when the control is in front of them.
    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("escape", "decline", "decline", show=False),
    ]

    class Answered(Message):
        """An operator answered one prompt. Carries the value exactly once.

        The value rides a message rather than a shared attribute so nothing
        retains it: the app reads it, builds the respond params, and the message
        goes out of scope. A widget that kept the last answer would be a copy of
        a sudo password living in the DOM for the rest of the session.
        """

        def __init__(self, request_id: str, kind: PromptKind, value: str) -> None:
            super().__init__()
            self.request_id = request_id
            self.kind = kind
            self.value = value

    class DeniedAll(Message):
        """Deny every approval queued in one session, with one choice.

        A separate message from :class:`Answered` rather than a flag on it,
        because it is a different act: it carries no operator-typed value and no
        request id, it is the *only* answer that is correct without correlation,
        and it is only ever a denial. A boolean on ``Answered`` would put a code
        path one keystroke away from applying an affirmative to a queue nobody
        has read.
        """

        def __init__(self, session_id: str | None) -> None:
            super().__init__()
            self.session_id = session_id

    class Declined(Message):
        """The operator refused one prompt without waiting for it to expire (R3).

        **It carries no value, and that absence is the safety property.** The
        wire value is looked up from the prompt's kind by the app
        (:func:`decline_value`), so the widget layer has no way to post a
        decline carrying an empty approval choice — the one string the
        gateway's approval consumer reads as *approved*
        (``tools/approval.py:3679``). A ``declined=True`` flag on
        :class:`Answered` would have put that value one keystroke from a
        control the operator pressed to refuse.

        The kind rides along so the app can do that lookup without going back
        to the registry, which may already have moved on.
        """

        def __init__(self, request_id: str, kind: PromptKind) -> None:
            super().__init__()
            self.request_id = request_id
            self.kind = kind

    class DeclineRefused(Message):
        """Escape was pressed on the unanswerable (deny-all-only) card (CR4
        finding 6b).

        Its hint line names one key, ``enter deny all`` — escape is not one
        of them, and the card used to swallow it with no call, no notice and
        no caret movement. AE11's rule applies here exactly as it does to a
        control that sends nothing: a key that does not act must still say
        so, rather than read as identical to one that worked.
        """

        def __init__(self, request_id: str) -> None:
            super().__init__()
            self.request_id = request_id

    def __init__(self, row: PromptRow, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self.row = row
        self._input: Input | None = None
        self._actions: Widget | None = None

    @property
    def request_id(self) -> str:
        return self.row.request_id

    @property
    def action_widget(self) -> Widget | None:
        """The one thing on this card that answers — buttons, or the input.

        Named because it is the part of the card that has to be *reachable*
        rather than merely mounted. The summary and the command are the reasons
        to answer; this is the answering. A card showing both reasons with this
        scrolled off the region's bottom edge still reads as a live control,
        and nothing the operator can see will resolve it.
        """
        return self._actions

    @property
    def choices(self) -> tuple[str, ...]:
        if self.row.kind == "approval" and not self.row.choices:
            return NO_CHOICES_FALLBACK
        return self.row.choices

    def compose(self) -> ComposeResult:
        yield Static(
            literal_text(f"{self.row.kind.replace('_', ' ')}: {self.row.summary}"),
            markup=False,
            classes="prompt--question",
        )
        if self.row.command:
            # **The command, before any control that could grant it.** The line
            # above is the gateway's ``description``, which at the pin is the
            # joined pattern warnings rather than the command — "recursive
            # delete outside the workspace", never ``rm -rf /``. A card that
            # rendered only that line offered four buttons for a command that
            # was nowhere on the screen, and the shipping terminal UI is
            # explicit about why it does the opposite: "the full command must be
            # reviewable before approving"
            # (``ui-tui/src/components/prompts.tsx:97-99``).
            yield CommandPanel(self.row.command, classes="prompt--command")
        if not self.row.answerable:
            # The summary and the command above still render — the operator must
            # be able to read what the agent is asking for even when nothing can
            # be answered.
            # What is withheld is the affirmative, because pressing it would
            # release whichever entry the gateway's queue happens to have at its
            # head, which is not necessarily the one named above.
            yield Static(
                literal_text(self.row.blocked_reason),
                markup=False,
                classes="prompt--blocked",
            )
            self._actions = Horizontal()
            with self._actions:
                yield Button(
                    "deny all",
                    id="deny-all",
                    classes="prompt--deny-all",
                    compact=True,
                )
            yield Static(literal_text(DENY_ALL_HINT), markup=False, classes="prompt--hint")
            return
        if self.choices:
            # A gateway-supplied choice list turns any bridge into a closed
            # question, so approval and a multiple-choice clarify render the
            # same way. Free text is what a bridge gets when the gateway
            # offered nothing to pick from.
            self._actions = Horizontal()
            with self._actions:
                for index, choice in enumerate(self.choices):
                    # The label is defanged for display, and the button's *id*
                    # carries the index — so the value sent to the gateway is
                    # looked up from the original list rather than read back off
                    # the screen. A choice containing a control character would
                    # otherwise be answered with its printable stand-in, which
                    # the gateway has never heard of.
                    yield Button(
                        literal_text(choice).plain,
                        id=f"choice-{index}",
                        classes="prompt--choice",
                        compact=True,
                    )
            yield Static(literal_text(CHOICE_HINT), markup=False, classes="prompt--hint")
        else:
            self._input = Input(
                password=self.row.kind in HIDDEN_KINDS,
                placeholder="answer · Enter sends",
                id="answer",
                classes="prompt--answer",
                compact=True,
            )
            self._actions = self._input
            yield self._input
            yield Static(literal_text(ANSWER_HINT), markup=False, classes="prompt--hint")

    def on_mount(self) -> None:
        self.border_title = WAITING_TITLE

    def show_control_offscreen(self, offscreen: bool) -> None:
        """Say whether this card's control is somewhere the operator can see.

        Called by :meth:`PromptRegion.mark_unreachable_controls` after every
        recomputation, in both directions — a card that scrolls back into view
        gets its ordinary title again, so the warning cannot outlive the
        condition it describes.
        """
        title = CONTROL_OFFSCREEN_TITLE if offscreen else WAITING_TITLE
        if self.border_title != title:
            self.border_title = title

    def focus_answer(self) -> None:
        """Put the caret where the answer goes, without stealing it later.

        Extended for U1 (KTD1): a button-backed card — an approval, a
        multiple-choice clarify, or the unanswerable deny-all-only card — has
        no ``self._input`` to hand the caret to, and round 4 left those cards
        entirely out of the focus chain this method drives (R1's defect,
        ``talaria/ui/prompts.py:713-716`` pre-U1). The first mounted
        :class:`Button` is the first choice the card offers — the same
        ordering the gateway's own ``choices`` list carries, and the same
        button ``reveal_actions`` scrolls to — so focusing it puts the caret
        on the answer an operator reading top-to-bottom reaches first.
        """
        if self._input is not None:
            self._input.focus()
            return
        for button in self.query(Button):
            button.focus()
            return

    def action_decline(self) -> None:
        """Refuse this prompt now rather than waiting for it to expire (R3).

        A ``terminal_read`` never reaches this method — it renders no card at
        all (:data:`UNATTENDED_KINDS`) — and the kind check states that anyway,
        because :func:`decline_value` and this guard are the two places the
        "which bridges a human can refuse" rule is read.

        An **unanswerable** card is deliberate too, but it is not silent
        (CR4 finding 6b). Once a second approval queues, neither card can be
        aimed at (the uncorrelated-approval rule) and the only control either
        one carries is ``deny all`` — which is already the decline, for the
        whole queue, and is the same call KTD8's interrupt sweep makes.
        Binding ``escape`` to it as well would put a queue-wide safety action
        on a key the card's hint line (:data:`DENY_ALL_HINT`) deliberately
        does not name; the operator's way out of that card is the button in
        front of them. What changed is that pressing it anyway now says so
        (:class:`DeclineRefused`) instead of doing nothing that looks
        identical to having worked.

        **The ``Input`` is left alone here (CR4 finding 6a).** A successful
        decline unmounts the card, which takes its ``Input`` with it — an
        early clear bought nothing there. What it did buy, while the decline
        is still in flight or gets refused (replay mode, most concretely),
        was destroying the operator's half-typed answer under a notice that
        says the key did nothing: the text and the claim contradicted each
        other. Nothing here mirrors :meth:`on_input_submitted`'s own clear —
        that one guards a credential the ANSWER path is about to try sending;
        a decline never sends what is typed at all, so there is no value to
        protect from a slow reply.
        """
        if decline_value(self.row.kind) is None:
            return
        if not self.row.answerable:
            self.post_message(self.DeclineRefused(self.request_id))
            return
        self.post_message(self.Declined(self.request_id, self.row.kind))

    def on_button_pressed(self, message: Button.Pressed) -> None:
        message.stop()
        raw_id = message.button.id or ""
        if raw_id == "deny-all":
            self.post_message(self.DeniedAll(self.row.session_id))
            return
        index = int(raw_id.removeprefix("choice-")) if raw_id.startswith("choice-") else -1
        choices = self.choices
        if not 0 <= index < len(choices):  # pragma: no cover - ids are minted above
            return
        self.post_message(self.Answered(self.request_id, self.row.kind, choices[index]))

    def on_input_submitted(self, message: Input.Submitted) -> None:
        message.stop()
        value = message.value
        # Cleared before the message is posted, not after the answer is
        # delivered. An unconfirmed call can outlive the widget, and a sudo
        # password sitting in a mounted ``Input`` waiting for a reply that never
        # comes is a credential retained by an accident of timing.
        message.input.value = ""
        self.post_message(self.Answered(self.request_id, self.row.kind, value))


class PromptRegion(VerticalScroll):
    """The prompt controls plus the line that says a human is being waited on.

    Scrollable, and that is a correctness property rather than a convenience.
    An approval card now carries a wrapped command body, so two queued
    approvals can ask for more rows than the region is allowed to take from the
    transcript. A plain container would clip the overflow against its own edge
    with nothing on screen to say so — the same silent-truncation failure the
    command body's overflow marker exists to prevent, one level up. A scrollbar
    is the framework's visible statement that there is more below.
    """

    DEFAULT_CSS = """
    PromptRegion {
        height: auto;
        max-height: 75%;
        scrollbar-size-vertical: 1;
    }
    PromptRegion > .prompts--activity {
        height: 1;
        color: $text-muted;
    }
    PromptRegion.-waiting > .prompts--activity {
        color: $warning;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._activity: Static | None = None
        self._cards: dict[str, PromptCard] = {}

    def compose(self) -> ComposeResult:
        self._activity = Static(literal_text(""), markup=False, classes="prompts--activity")
        yield self._activity

    @property
    def activity_text(self) -> str:
        return "" if self._activity is None else str(self._activity.content)

    @property
    def card_ids(self) -> tuple[str, ...]:
        """Which request ids currently have a control on screen, in order."""
        return tuple(card.request_id for card in self.query(PromptCard))

    def card_for(self, request_id: str) -> PromptCard | None:
        return self._cards.get(request_id)

    def focus_first_unanswered(self) -> bool:
        """Jump the caret to the oldest outstanding card's control (R1, KTD1).

        "Oldest" is mount order — the same ordering :meth:`reveal_actions`
        already keeps visible, so the jump and the region's own scroll
        behaviour never disagree about which card is "the" one on screen.
        With two approvals queued that card's only action is "deny all",
        which is correct for the whole queue from either card, so landing on
        the oldest keeps the command the gateway resolves first in view.

        A no-op — returns ``False``, moves nothing — when no card is
        mounted. The jump is deliberate (KTD1): unlike mount-time auto-focus
        it does not check whether the composer holds text, because a
        keypress this explicit is allowed to move the caret even mid-word.
        """
        for card in self.query(PromptCard):
            target = card.action_widget
            if target is None:
                continue
            self.scroll_to_widget(target, animate=False)
            card.focus_answer()
            return True
        return False

    def on_resize(self, event: events.Resize) -> None:
        # Deferred, because the resize cascade has not finished when this
        # arrives: the region learns its new height first and the command
        # bodies inside it re-wrap afterwards, so a scroll computed here would
        # be computed against heights that are about to change.
        self.call_after_refresh(self.reveal_actions)

    def on_command_panel_rewrapped(self, message: CommandPanel.Rewrapped) -> None:
        """Recompute after a body re-wrapped, which is not always a resize.

        **Measured, because round 4 shipped this channel with nothing that
        could exercise it.** Instrumenting both triggers over a run shows one
        case where this is the *only* one that fires: a card mounting into a
        region that has already reached its ``max-height``. The region's own
        size does not change — it is already as tall as it may be — so no
        :class:`~textual.events.Resize` reaches it, while the new card's command
        body lays out and wraps like any other. That is the ordinary
        third-approval case, not a corner: the region caps at 70% of the screen
        somewhere around the third card at 120x40.

        With two approvals queued the region also holds the state where every
        card's only action is "deny all", so a card mounted below the fold with
        nothing recomputed is a card claiming to be waiting with no visible way
        to say no.
        """
        message.stop()
        self.call_after_refresh(self.reveal_actions)

    def reveal_actions(self) -> None:
        """Reveal one card's control, and stop every other card claiming one.

        **The defect this exists for is specific to the resize path, and it is
        stable rather than transient.** A card mounted at 60x20 puts its buttons
        inside the region and everything works. Narrow the terminal from 120x40
        to 60x20 with the card already up and the command body re-wraps from
        four rows to seven *after* the card's height was placed, so the buttons
        end up below the region's bottom edge — measured at row 17 against a
        region ending at row 14. Nothing put them back: this is a
        ``VerticalScroll``, so the rows exist and are reachable with three
        ``Tab`` presses, but the card keeps its "waiting for you" title and its
        command body, so it reads as a live control while every visible part of
        it is inert and a click on where the buttons *were* sends nothing.

        **One region cannot show every card's control, so the choice is which
        one it reveals — and the cards it does not reveal must stop looking
        live.** Round 4 made the first half of that choice and implemented the
        wrong thing: it returned after the first card, which is not "reveal the
        first card's control" but "look at the first card and give up". With a
        clarify parked above an approval — an ordinary state, because the
        gateway keys its pending map by request id and a clarify configured
        with a non-positive timeout waits forever — the clarify's input was
        already visible, the scroll was a no-op, and the approval below it kept
        its "waiting for you" title with its buttons off the bottom edge and
        every click on them landing nowhere.

        Revealing the **first** card is kept, with round 4's reason: with two
        approvals queued the only offered action is "deny all", which applies
        to the whole queue from whichever card carries it, so reaching for the
        first keeps the oldest command — the one the gateway resolves first —
        on screen instead of pushing it off. What is added is the other half:
        every card whose control did not survive that choice says so, on the
        one part of itself that is still legible. The reasoning and the
        alternatives are in ``docs/engineering-journal/DECISIONS.md``.

        Minimal scrolling, via :meth:`scroll_to_widget`: when the control is
        already visible this is a no-op, so an operator who has scrolled up to
        read a long command is not yanked back on every resize event.
        """
        for card in self.query(PromptCard):
            target = card.action_widget
            if target is not None and target.is_mounted:
                self.scroll_to_widget(target, animate=False)
                break
        # Deferred again, and for the same reason the caller deferred: the
        # scroll above moves every other card, so a reachability test run here
        # would be run against the arrangement this call just replaced.
        self.call_after_refresh(self.mark_unreachable_controls)

    def mark_unreachable_controls(self) -> None:
        """Retitle every card by whether its own control is on screen.

        ``scrollable_content_region`` rather than ``region``: the outer region
        includes the border and the scrollbar column, and a control the
        scrollbar is drawn over is not a control the operator can click. A
        control laid out at zero height fails this too — it is mounted,
        focusable, and draws nothing, which is the failure mode
        ``PromptCard.DEFAULT_CSS`` carries its own comment about.
        """
        viewport = self.scrollable_content_region
        for card in self.query(PromptCard):
            target = card.action_widget
            reachable = (
                target is not None
                and target.is_mounted
                and target.region.height > 0
                and viewport.contains_region(target.region)
            )
            card.show_control_offscreen(not reachable)

    async def apply(self, view: PromptView, turn: TurnStatus, *, focus_new: bool = True) -> None:
        """Mount, keep, and unmount controls to match the projection exactly.

        Reconciled by ``request_id`` rather than by position. A prompt answered
        or expired in the middle of a list must take *its own* control away, and
        an index-based pass takes the last one instead — which on these widgets
        means the operator's half-typed secret moves to a different question.

        ``focus_new`` is the caller's answer to "is the operator in the middle of
        something". A blocking prompt has a real claim on the caret, but taking
        it mid-word drops the keystrokes that were already in flight, so the app
        passes ``False`` while the composer holds text and the activity line
        does the asking instead.

        **A card whose row changed is rebuilt, not left alone.** The registry
        never edits a prompt once it is registered, so the only field that can
        move is ``answerable`` — and it moves on the *first* approval at the
        moment a second one arrives. Keeping that card because its id had not
        changed would leave the affirmative buttons on screen for a question
        that can no longer be aimed, which is the whole defect wearing a
        different hat.
        """
        wanted = {row.request_id: row for row in attended_rows(view)}

        released = False
        for request_id, card in tuple(self._cards.items()):
            wanted_row = wanted.get(request_id)
            if wanted_row is None or wanted_row != card.row:
                doomed = self._cards.pop(request_id)
                released = released or holds_caret(doomed)
                await doomed.remove()

        # Mounted at an explicit position, because a rebuilt card would
        # otherwise land at the end and silently reorder the queue. Order is the
        # one thing an approval queue must not lie about: it is the order the
        # gateway resolves in. Index 0 is the activity line, so row ``i`` sits
        # at ``i + 1``, and every earlier row is already mounted by the time
        # this loop reaches ``i``.
        # KTD1 (A1): the card takes focus when it mounts, but only through the
        # existing mid-word guard and only when no card already holds focus.
        # Extended from input-only (U1) to any focusable control (Input or first
        # Button). The first card of a batch takes focus when the guard allows;
        # every subsequent card in the same outstanding set does not steal it.
        # A local flag guards the batch case where Textual's focus has not yet
        # settled synchronously, so holds_caret would still read false for the
        # card just focused.
        already_holds = any(holds_caret(card) for card in self._cards.values())
        focused_this_apply = False
        for position, row in enumerate(wanted.values()):
            if row.request_id in self._cards:
                continue
            card = PromptCard(row)
            self._cards[row.request_id] = card
            await self.mount(card, before=position + 1)
            if focus_new and card.action_widget is not None:
                if not already_holds and not focused_this_apply:
                    if not any(
                        holds_caret(existing)
                        for cid, existing in self._cards.items()
                        if cid != row.request_id
                    ):
                        card.focus_answer()
                        focused_this_apply = True

        # ``wanted``, not ``view.rows``: the warning colour is the region
        # shouting for attention, and a terminal-read has no card here to look
        # at. It used to light the region up over a single empty activity row.
        waiting = turn == "waiting" and bool(wanted)
        self.set_class(waiting, "-waiting")
        if self._activity is not None:
            self._activity.update(literal_text(activity_line(turn, view)))

        # Announced last, so the mount loop above has already had its chance to
        # claim the caret. A card whose row changed is removed and replaced in
        # this same pass, and taking the caret out of the replacement would
        # answer one defect with another: the operator would be typing into the
        # composer while the question they are being asked sits unanswered.
        if released and not any(holds_caret(card) for card in self._cards.values()):
            self.post_message(CaretReleased())
