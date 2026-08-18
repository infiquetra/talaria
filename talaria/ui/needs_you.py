"""The needs-you surface: one reserved row, and the drill-down behind ``/needs`` (v0.4 U7).

U6 built the queue and this module renders it. Nothing here decides *what* needs
a person — :func:`~talaria.domain.state.fleet_queue` does, purely, from the
registry — so a disagreement between what this bar says and what the answer path
will accept is not possible by construction. That is the whole reason the queue
is derived rather than stored.

**What this surface will not say.** It reports what Talaria has been told and
never what it has not asked. Two rules follow, and both are copy rules rather
than mechanism:

* The empty state is ``needs-you: none``, which is a statement about the queue.
  It is never "nothing needs you", "all clear", or anything else that would make
  a claim about the gateways. When a connection could not be asked,
  :func:`~talaria.domain.queue.summary_line` says so in the same row and the
  drill-down lists the reason per connection.
* An item whose connection dropped renders its age as it stood at the break,
  with the blind span named separately — see
  :func:`~talaria.domain.queue.wait_line`. Until U7 that field had no reader at
  all, so a frozen age was rendered as a live one.

**The freshness this surface actually has, stated because it is less than it
looks.** Sessions Talaria's own connections drive push their frames, so a driven
wait appears as it happens on any connection. A *foreign* session's wait — one
another client owns on a gateway Talaria is merely connected to — is visible only
through polling, and the KTD2 poll loop has no production caller yet (see the
v0.4 plan's U7 section). Its cadence today is therefore one sweep when a
connection comes up and one per seam revalidation. Nothing in this module claims
otherwise, which is why no row here says "now" or "current".
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Final, Literal

from textual.widgets import Static

from talaria.domain.models import QueueItem
from talaria.domain.queue import ItemKey, NeedsYouQueue, summary_line, wait_line
from talaria.domain.selection import Choice, Selection, Stage
from talaria.ui.literal import literal_text
from talaria.ui.prompts import NO_CHOICES_FALLBACK, decline_value

#: The drill-down's title. A question rather than a label, because the surface
#: answers exactly one and the operator arrived here asking it.
NEEDS_YOU_TITLE: Final[str] = "needs-you — what is waiting on you"

#: Shown on a notice row, which reports a connection rather than offering an item.
NOTICE_NOT_AN_ITEM: Final[str] = (
    "a report about a connection, not something to answer — nothing to select here"
)

#: What the second level is called once an approval row is descended into.
ANSWER_STAGE_TITLE: Final[str] = "answer this approval"

#: The decline row's label. Named as a refusal rather than shown as a bare
#: choice value, because ``deny`` in a list beside ``once`` reads as one more
#: option rather than as the escape.
DECLINE_LABEL: Final[str] = "decline — send an explicit deny"

#: Said when a chosen item is no longer in the queue by the time the dialog closes.
ITEM_NO_LONGER_WAITING: Final[str] = (
    "that item is no longer waiting — it resolved, expired, or its connection "
    "dropped while the list was open; nothing was sent"
)

#: Separates the three parts of an item's identity inside a selection payload.
#: The unit separator, because it is the one character in this position that
#: carries no meaning in a session id, a profile name, or a request id.
_PAYLOAD_SEP: Final[str] = "\x1f"


def encode_identity(identity: ItemKey) -> str:
    """Pack an item's identity into the single string a selection emits."""
    return _PAYLOAD_SEP.join(identity)


def decode_identity(payload: str) -> ItemKey | None:
    """Unpack a selection payload, or ``None`` when it is not one.

    ``None`` rather than an exception, and the difference matters at exactly one
    input: a gateway-supplied session id or request id that itself contains
    :data:`_PAYLOAD_SEP` would split into the wrong number of parts. The caller
    then reports :data:`ITEM_NO_LONGER_WAITING` and sends nothing, which is the
    safe reading of an identity that cannot be trusted to name one item. The
    alternative — splitting on the first two separators and hoping — would aim
    an approval at whatever row the mangled key happened to match.
    """
    parts = payload.split(_PAYLOAD_SEP)
    if len(parts) != 3:
        return None
    return (parts[0], parts[1], parts[2])


SelectionAction = Literal["go", "answer", "decline"]


@dataclass(frozen=True)
class NeedsSelection:
    """What the operator chose: an act, the item it applies to, and its value."""

    action: SelectionAction
    identity: ItemKey
    value: str = ""


def encode_selection(action: SelectionAction, identity: ItemKey, value: str = "") -> str:
    """Pack a choice into the single string a dialog selection emits."""
    return _PAYLOAD_SEP.join((action, *identity, value))


def decode_selection(payload: str) -> NeedsSelection | None:
    """Unpack a selection, or ``None`` when the payload is not one.

    ``value`` is split last and with a bound, so a gateway-supplied choice that
    itself contains :data:`_PAYLOAD_SEP` survives the round trip intact — the
    parts that must not be ambiguous are the four in front of it, and a
    separator inside any of *those* yields ``None`` rather than a partial match.
    Pinned by ``test_an_unparseable_payload_names_nothing_rather_than_guessing``.
    """
    parts = payload.split(_PAYLOAD_SEP, 4)
    if len(parts) != 5:
        return None
    action, profile, session_id, request_key, value = parts
    if action == "go":
        return NeedsSelection("go", (profile, session_id, request_key), value)
    if action == "answer":
        return NeedsSelection("answer", (profile, session_id, request_key), value)
    if action == "decline":
        return NeedsSelection("decline", (profile, session_id, request_key), value)
    return None


def answer_choices(item: QueueItem) -> tuple[Choice, ...]:
    """The explicit answers for one approval row — never an empty one.

    Every row here carries a value the gateway named, plus a decline whose value
    comes from :func:`~talaria.ui.prompts.decline_value` rather than from an
    empty string. That distinction is the whole reason this list is built rather
    than left to a free-text control: the gateway's consumer blocks only on
    ``None`` and ``"deny"`` and returns *approved* for anything else resolved, so
    an empty approval choice is an approval. A surface that offered "answer with
    nothing" as the escape would grant the command it looked like it refused.

    That claim about the consumer is not this module's: it is transcribed in
    :data:`~talaria.ui.prompts.DECLINE_VALUES`, which carries the citation
    (``tools/approval.py:3291`` and ``:3320``). ``NO_CHOICES_FALLBACK`` covers an
    approval the gateway described without listing choices; it is the same
    fallback the card uses, so the two paths cannot disagree about what an
    unlisted approval offers.

    Pinned by ``test_the_decline_row_sends_an_explicit_deny_and_never_an_empty_choice``
    and ``test_an_approval_the_gateway_listed_no_choices_for_still_offers_a_named_answer``.
    """
    offered = item.choices or NO_CHOICES_FALLBACK
    refusal = decline_value("approval")
    rows = [
        Choice(
            key=f"answer:{value}",
            label=value,
            payload=encode_selection("answer", item.identity, value),
        )
        for value in offered
        if value != refusal
    ]
    if refusal is not None:
        rows.append(
            Choice(
                key="decline",
                label=DECLINE_LABEL,
                payload=encode_selection("decline", item.identity, refusal),
            )
        )
    return tuple(rows)


def format_item_label(item: QueueItem, clock: float) -> str:
    """One drill-down row: what is waiting, how long, and where.

    Field order is the bar's, and for the bar's reason (R16): the age and the
    source are fixed-width-ish and come before the variable-width session title,
    so an 80-column clip takes the title rather than the facts.
    """
    title = item.session_title or item.session_id or "an unnamed session"
    parts = [item.kind, wait_line(item, clock), item.source, title]
    if item.possibly_duplicate:
        # Never silently merged: U6 shows both sightings and says the doubt out
        # loud, so the row has to carry it or the doubt dies at the boundary.
        parts.append("possibly the same as another row")
    return " · ".join(part for part in parts if part)


def format_item_detail(item: QueueItem) -> str:
    """The row's second line: the prompt itself, or why it cannot be answered."""
    if not item.answerable and item.blocked_reason:
        return item.blocked_reason
    return item.command or item.summary


class NeedsYouBar(Static):
    """One reserved row for the queue's summary (KTD7, R16).

    Composed at first mount and never unmounted, which is the whole of its
    geometry contract: a bar that appears when the queue fills and vanishes when
    it empties would move every region above it twice per approval, and the
    transcript would reflow underneath a reader. It renders ``needs-you: none``
    into the same row instead.

    Clipped rather than wrapped, like :class:`~talaria.ui.app.HelpBar`: a second
    line is a moved region by another name — and ``height: 1`` is what enforces
    that rather than the summary happening never to be long, which was measured
    rather than assumed. Pinned by
    ``test_the_needs_you_bar_holds_one_row_from_empty_to_many_and_back`` for the
    fill-and-empty transition and by
    ``test_the_row_is_reserved_by_the_stylesheet_not_by_the_summary_never_being_empty``
    for the declaration itself; only the second distinguishes ``height: 1`` from
    ``height: auto``.
    """

    DEFAULT_CSS = """
    NeedsYouBar {
        height: 1;
        color: $text-muted;
        text-wrap: nowrap;
        text-overflow: ellipsis;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__("", markup=False, **kwargs)  # type: ignore[arg-type]
        self._line = ""

    def update_queue(self, queue: NeedsYouQueue, clock: float) -> None:
        """Render this queue, repainting only when the text actually moved.

        The no-move guard is not an optimisation here so much as a rendering
        contract: the render tick calls this at its own cadence and a repaint per
        tick would fight the transcript for the terminal.
        """
        line = summary_line(queue, clock)
        if line == self._line:
            return
        self._line = line
        # ``literal_text`` for the same reason every other gateway-sourced string
        # gets it (R23): the oldest item's session title is the gateway's text,
        # and it arrives in a row that is otherwise Talaria's own words.
        self.update(literal_text(line))

    @property
    def line(self) -> str:
        """What the bar currently says, for assertions and for the palette."""
        return self._line


class NeedsYouPickerSource:
    """The ``/needs`` drill-down: every item, then every connection that could not
    be asked.

    One level, matching ``/sessions`` and for the same reason — there is nothing
    below an item to descend into, and the answer path lives on the card the
    selection navigates to rather than in the dialog.

    **Notices are rows, not a footnote.** A connection whose roster or approval
    detail could not be read contributes silence to the item list, and silence in
    a surface whose job is "is anything waiting on me" reads as "no". They are
    listed last and refuse selection, because each one reports a connection
    rather than offering something to answer. Pinned by
    ``test_the_drill_down_lists_items_then_the_connections_it_could_not_ask``.

    The payload is the item's *identity*, never its position: the queue is
    derived on every render, so a row index taken when the dialog opened may name
    a different item by the time it closes. The caller resolves the identity
    against the queue as it stands at that moment and reports
    :data:`ITEM_NO_LONGER_WAITING` when it has gone.

    **Answering inline is a descent, not a key, and the reason is this dialog's
    own design.** ``PickerDialog`` gives every printable key to the filter on
    purpose — a hundred-row list is navigated by typing — so an ``a``/``d`` pair
    for approve and decline would take two letters back out of the filter the
    plan also asks for ("typing filters"). Inventing control chords for a modal
    is the worse of the two remaining options, so ``enter`` on an answerable
    approval opens a second level of *named* choices, which is the descent
    ``/models`` already makes between a provider and its models. The plan's
    "explicit approve/decline keys" is met in substance — the answer is explicit
    and keyboard-only — by a mechanism the dialog already has. **A deliberate
    deviation from the plan's wording, recorded here rather than silently taken.**

    That descent also does something a key pair could not: every offered answer
    is a row with the gateway's own choice on it, so there is no path here that
    sends a value nobody read.

    ``inline_answerable`` is decided by the app, not here, and holds the
    identities whose prompt is still live in the focused engine's registry. The
    queue knows an item is *answerable in principle* (R18: it is the session's
    head approval and carries a request id); only the registry knows whether
    :meth:`~talaria.ui.app.TalariaApp.respond_live` would still accept it, and
    that is the function that will actually be called.
    """

    def __init__(
        self,
        queue: NeedsYouQueue,
        clock: float,
        *,
        title: str = "",
        inline_answerable: frozenset[ItemKey] = frozenset(),
    ) -> None:
        self._queue = queue
        self._clock = clock
        self._title = title or NEEDS_YOU_TITLE
        self._inline_answerable = inline_answerable

    def root(self) -> Stage:
        choices = [
            Choice(
                key=encode_identity(item.identity),
                label=format_item_label(item, self._clock),
                payload=encode_selection("go", item.identity),
                detail=format_item_detail(item),
                selectable=True,
            )
            for item in self._queue.items
        ]
        choices.extend(
            Choice(
                key=f"notice:{index}",
                label=notice,
                detail="",
                selectable=False,
                refusal=NOTICE_NOT_AN_ITEM,
            )
            for index, notice in enumerate(self._queue.notices)
        )
        return Stage(title=self._title, selection=Selection.opened(tuple(choices)))

    def descend(self, depth: int, choice: Choice) -> Stage | str:
        """Level one descends an answerable approval; everything else emits.

        A row that is not an answerable approval emits its ``go`` payload and the
        dialog closes on it, which is the navigation path. Level two always
        emits, because there is nothing below an answer. Pinned by
        ``test_an_answerable_row_descends_to_its_choices_rather_than_emitting``
        and, for the row the app declined to offer inline,
        ``test_a_row_whose_prompt_left_the_registry_is_not_offered_inline``.
        """
        selection = decode_selection(choice.payload)
        if depth > 0 or selection is None or selection.action != "go":
            return choice.payload
        if selection.identity not in self._inline_answerable:
            return choice.payload
        item = self._queue.item_for(selection.identity)
        if item is None:  # pragma: no cover - the queue this stage was built from
            return choice.payload
        # The title carries the wait as ``wait_line`` reports it, floor and blind
        # span included, so the age the operator answers against is the same
        # sentence the list showed rather than a fresher-looking restatement.
        return Stage(
            title=f"{ANSWER_STAGE_TITLE} — {wait_line(item, self._clock)}",
            selection=Selection.opened(answer_choices(item)),
        )


__all__ = [
    "ANSWER_STAGE_TITLE",
    "DECLINE_LABEL",
    "ITEM_NO_LONGER_WAITING",
    "NEEDS_YOU_TITLE",
    "NOTICE_NOT_AN_ITEM",
    "NeedsSelection",
    "NeedsYouBar",
    "NeedsYouPickerSource",
    "answer_choices",
    "decode_identity",
    "decode_selection",
    "encode_identity",
    "encode_selection",
    "format_item_detail",
    "format_item_label",
]
