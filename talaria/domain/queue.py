"""The needs-you queue: two feeds, one item identity, oldest wait first (U6).

Every outstanding human-facing blocking prompt from every registry session is
one typed :class:`~talaria.domain.models.QueueItem` in one flat queue ordered by
wait age (R13, R14, R15). This module is the *domain* half: pure functions over
data, no clock, no socket, no widget. :mod:`talaria.domain.state` adapts a
:class:`~talaria.domain.state.FleetState` onto it and
:mod:`talaria.domain.projection` publishes feed A's projection; the surface is
U7's.

**It is a leaf on purpose.** It imports models, registry, compat, normalize and
redaction — never ``state`` — so ``state`` can import it without a cycle, the
same discipline :mod:`talaria.domain.registry` documents at its own head.

Four rules run through everything below.

*Two feeds, one identity* (KTD2). Feed A is a session Talaria drives, whose
prompts arrive as events and land in the focused engine's prompt registry with
authoritative ``opened_at``/``seq`` stamps. Feed B is everyone else: roster rows
reporting a wait, plus per-approval detail fetched by the gated
``approval.pending`` call. Both resolve to ``(profile, durable session id,
request key)``, so a session Talaria attaches mid-wait never duplicates its item.

*The queue holds only resolvable items* (R14/R17). A blocking prompt of a kind
Talaria can render nowhere is named on its session's registry row and is never
queued — a queue row that cannot be resolved is a dead end wearing the clothes
of a task.

*Ages ride the frame clock, and a polled age is a floor* (KTD12/R20). Nothing
here reads a wall clock. A wait Talaria did not watch begin renders "waiting ≥
span" measured from the first sighting, because U1 verified that no row and no
approval payload carries a start stamp at any revision — the floor is not a
fallback, it is the only honest age any client can render.

*An empty queue must never mean "we could not ask"* (R24). A connection whose
roster or approval-detail probe did not answer contributes nothing, and nothing
is exactly what a quiet fleet contributes. :func:`connection_notices` is what
keeps those two apart, and :func:`summary_line` refuses to say "none" without
carrying the count of connections that could not be asked.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass, replace
from typing import Any, Final

from talaria.domain.compat import SeamBoard, SeamStatus
from talaria.domain.models import (
    SOURCE_APPROVAL_POLL,
    SOURCE_DRIVEN,
    SOURCE_ROSTER,
    PendingPrompt,
    QueueItem,
    QueuePrompt,
)
from talaria.domain.normalize import clip_detail_line, coerce_text
from talaria.domain.redaction import redact_probe_detail
from talaria.domain.registry import ConnectionChannel, RegistryRow, RowKey

# ── Vocabulary ───────────────────────────────────────────────────────────

#: The four kinds R14 enumerates: the prompts Talaria renders a card for, which
#: is what makes them resolvable and therefore queueable.
#:
#: ``terminal_read`` is deliberately absent though it is a fifth
#: :data:`~talaria.domain.models.PromptKind`. It is answered by machine from the
#: transcript projection with no human involved, so a queue row for it would ask
#: the operator for something they are not being asked for; when Talaria cannot
#: serve one it fails visibly on the row and settles (R14's terminal-read
#: clause), which is the opposite of entering a queue. That failure latch is the
#: focused half; a background one never registers a prompt at all, so its row is
#: named directly by the known-prompt branch in
#: :func:`talaria.domain.state._apply_event_to_row` — round nine found that
#: branch naming nothing, which left the suppression below silent, and round ten
#: found the branch itself reachable only from tests because the app still
#: folded inbound frames with ``apply_frame``. Both halves are wired now:
#: ``TalariaApp.ingest`` routes through ``route_frame``, pinned end to end by
#: ``test_a_background_terminal_read_names_its_row_through_the_running_app``.
QUEUEABLE_KINDS: Final[frozenset[str]] = frozenset(
    {"approval", "clarify", "secret", "sudo"}
)

#: The kind a foreign wait carries when the gateway has flattened it. A roster
#: row reports the word ``waiting`` and nothing finer — verified live: the row
#: is exactly ``{current, id, last_active, message_count, model, preview,
#: session_key, started_at, status, title}`` — so this is the honest name for a
#: wait whose kind no event has told Talaria.
UNOBSERVED_KIND: Final[str] = "unobserved"

#: The three kinds the running gateway blocks on that Talaria renders nowhere
#: (``methods_prompt.py:1412``, ``:1420``, ``:1429`` register their respond
#: bridges; the pinned read has none of them). They are **named on the registry
#: row and never queued** (KTD2): the queue holds only resolvable items, and an
#: item whose kind Talaria cannot render anywhere is not resolvable.
#:
#: The tuple is documentation and test material rather than the mechanism.
#: :func:`unresolvable_kind_of` decides by *absence* from
#: :data:`QUEUEABLE_KINDS`, so a fourth kind a later gateway grows is named on
#: its row too, without an edit here.
UNRESOLVABLE_KINDS: Final[tuple[str, ...]] = (
    "preview.read",
    "window.read",
    "mcp.setup",
)

#: The synthesized request key a flattened roster wait carries.
#:
#: Stable across polls **and across a later kind refinement**: an event that
#: names the kind must not mint a second identity for the same wait, so the key
#: says only that the roster reported one.
ROSTER_REQUEST_KEY: Final[str] = "roster:waiting"

# ── KTD11's gate, as amended by the operator ruling of 2026-08-17 ────────

#: The row statuses at which ``approval.pending`` may be called for a session.
#:
#: **Operator ruling, 2026-08-17, following U1's evidence.** KTD11's gate exists
#: to avoid warming lazy agent builds. U1 proved a pending approval does not
#: surface as ``waiting``, because approvals ride ``tools.approval``'s registry
#: rather than the ``_block()`` prompt registry that feeds status — and a
#: ``working`` row's agent is live by construction, because it is mid-turn. So
#: firing ``approval.pending`` against waiting-or-working rows preserves the
#: exact safety property — ``idle``, ``starting`` and lazy rows stay excluded and
#: are never polled — while keeping R14 true for foreign approvals. The cost is
#: bounded: only active rows, at the existing coalesce and backstop cadence.
#:
#: The property is checkable by pointing at this set and at
#: :func:`approval_detail_due`: ``idle`` excluded, ``starting`` excluded,
#: ``working`` included, ``waiting`` included, and a row of unknown status
#: (``""``, or a word this gateway invented) excluded because it is not a member.
APPROVAL_DETAIL_TRIGGER_STATUSES: Final[frozenset[str]] = frozenset(
    {"waiting", "working"}
)

# ── Approval correlation vocabulary (R18, amended 2026-08-17) ────────────
#
# These strings live here rather than in ``state`` because both readers are here:
# the queue decides which item may be answered, and ``state``'s registry decides
# which answer may be sent, and the two must say the same sentence. ``state``
# imports them and re-exports them, so every existing caller keeps its import.

#: Why a queued approval carries no answer control. Shown on the card by the
#: projection and repeated by the registry's refusal, from this one string, so
#: the screen and the registry cannot come to say different things.
UNCORRELATED_APPROVAL: Final[str] = (
    "more than one approval is waiting and this gateway sends no request id "
    "with an approval, so an answer cannot be aimed at one of them"
)

#: What the operator is told if an answer is attempted anyway.
REFUSED_UNCORRELATED_APPROVAL: Final[str] = (
    f"{UNCORRELATED_APPROVAL} — nothing was sent; deny them all, or let them expire"
)

#: Why a roster item is never answerable, whatever kind it claims.
#:
#: A roster item is not a prompt. It is the gateway's flattened word ``waiting``
#: about a session, carrying :data:`ROSTER_REQUEST_KEY` — a key Talaria minted,
#: which no gateway reply will ever name. There is nothing to aim an answer at.
#:
#: **It has to be said as its own rule, because "kind" is not enough.** An event
#: can teach a row that its wait is an ``approval``, and the roster item then
#: carries ``kind="approval"`` while being no approval entry at all. It was
#: offered as answerable with no command shown and no request id to aim with, and
#: answering it would have fired ``approval.respond`` bare, popping whatever the
#: gateway happened to have at its head: approving a command the operator never
#: saw. R18 and AE2 exist for exactly that.
#:
#: **The three sites that key on the kind are named here rather than counted, so
#: the claim can be checked by following them** — the first draft of this comment
#: said "everything downstream", listed two, and the third was found by review:
#:
#: 1. ``_with_head_of_queue_rules``'s head/count/feed pass, which decides which
#:    approval is next — pinned by
#:    ``test_a_roster_item_does_not_take_the_head_from_a_real_approval``.
#: 2. The same pass's answerability marking, which returns this string — pinned
#:    by ``test_a_roster_item_is_never_answerable_even_when_its_kind_says_approval``.
#: 3. ``_hide``'s settled-tombstone branch, where a roster item neither counts
#:    as a shadow (a shadow suppresses headship and inflates ``queued_count`` —
#:    pinned by ``test_a_settled_roster_item_shadows_no_later_approval``) nor
#:    hides (:data:`ROSTER_REQUEST_KEY` is one constant per session, so a
#:    tombstone on it would name every later wait there — pinned by
#:    ``test_a_settled_roster_wait_is_not_a_permanent_tombstone``).
#:
#: A fourth site, the ``uncorrelated_sessions`` test in ``build_queue``, needs no
#: exclusion: it runs inside the feed-A loop and a roster item comes from feed B,
#: so it is unreachable there by construction rather than by a guard.
#:
#: A roster item's resolution is *navigation* — open the session and see the real
#: prompt — which is AE11's shape, not an answer from here.
ROSTER_ITEM_NOT_ANSWERABLE: Final[str] = (
    "the roster reports this session waiting but Talaria has not seen the prompt "
    "itself, so there is nothing here to answer — open the session"
)

#: Why a session's second approval is shown but not offered (R18's head-of-queue
#: rule). The gateway holds approvals in a per-session **queue** — verified at
#: every revision U1 examined — and resolves from its head, so an answer aimed at
#: the second one would land on the first.
QUEUED_BEHIND_APPROVAL: Final[str] = (
    "an earlier approval in this session is still waiting; the gateway answers "
    "them in order, so this one becomes answerable when that one resolves"
)

#: What the registry says if such an answer is attempted anyway.
REFUSED_APPROVAL_NOT_HEAD: Final[str] = (
    f"{QUEUED_BEHIND_APPROVAL} — nothing was sent"
)

#: Why a **blind** approval — one Talaria holds no gateway request id for — is
#: refused while an unplaceable approval stands on its connection: the fold in
#: :func:`_with_head_of_queue_rules`, explaining itself.
#:
#: **The fold produces the refusal and this sentence from one computation**
#: (the blind branch), the same discipline as the shared displacement
#: predicate at ``registry.py``'s ``attach_displaces_client``: one spelling,
#: because "is this refused" and "why is it refused" answered by two separate
#: spellings is how CR6 round six found them disagreeing. Until then the fold
#: fed forged inputs to :func:`approval_block_reason` and every folded item
#: said :data:`QUEUED_BEHIND_APPROVAL` — "an earlier approval in this session
#: is still waiting" — which is false on any *other* session the fold reaches
#: (round six's repro: a sibling session whose sole polled approval has a
#: gateway id and no predecessor at all) and promises an in-session resolution
#: nothing in that session can deliver.
#:
#: **Narrowed to blind answers on the round-seven ruling (2026-08-18).** The
#: fold's premise was that answering any sibling could land on the phantom's
#: entry. That premise holds only for an answer carrying no gateway request
#: id: ``resolve_gateway_approval`` (``tools/approval.py:2655-2658``) selects
#: entries **by id** whenever a ``request_id`` is present and returns 0 on no
#: match — the head-pop (``queue.pop(0)``) is the ``else`` of an
#: if/elif/else, structurally unreachable while an id is present — and the
#: gateway's ``approval.respond`` handler (``tui_gateway/methods_prompt.py``)
#: passes ``request_id`` through verbatim and never retries without one. An
#: aimed answer therefore cannot land on the phantom's entry, and a stale id
#: is a no-op rather than a blind pop, so only blind approvals are refused
#: here — pinned by ``test_a_blind_sibling_session_stays_refused_beside_a_phantom``
#: (the refusal kept) and
#: ``test_an_id_carrying_sibling_session_stays_answerable_beside_a_phantom``
#: (the exemption).
#:
#: The clearing clause promises exactly what the registry delivers, and no
#: more — and since the round-eight split (2026-08-18) it delivers an
#: unconditional expiry: :func:`~talaria.domain.state.age_out_approvals`
#: withdraws EVERY session's stale approvals at
#: :data:`~talaria.domain.state.APPROVAL_STALE_AFTER` (300 seconds), focused
#: or not. Round seven's text routed the exit through "its own session's
#: view" because the age-out was focus-scoped then; round eight established
#: that a session whose runtime id the alias window trimmed can never be
#: focused again, so that route promised an exit no operator could take —
#: the age-out's removal was split from its focus-scoped presentation
#: instead. Pinned by
#: ``test_a_trimmed_id_phantom_off_focus_ages_out_at_threshold``.
UNPLACEABLE_APPROVAL_ON_CONNECTION: Final[str] = (
    "an approval on this connection has outlived the link to its session, so "
    "an answer naming no gateway request id has no provable place in the "
    "gateway's queues; this clears when that approval ages out — an "
    "unanswered approval is withdrawn 5 minutes after it arrived"
)

#: What the phantom item itself says — the one exit that is delivered.
#:
#: A phantom is an approval whose runtime session id names no registry row
#: (the four-slot alias window trimmed it, and no poll supplied a gateway id
#: to re-anchor through). Answering is closed to it, from anywhere: the
#: gateway's ``approval.respond`` resolves the session **before** touching
#: any queue (``_sess_nowait``, ``tui_gateway/server.py:2507-2509`` — an
#: exact ``_sessions`` lookup that errors ``session not found`` on a miss),
#: so an answer sent under the phantom's dead runtime id dies at session
#: resolution regardless of how well the request id aims — which is why the
#: id exemption above never applies to the phantom itself. And a session
#: whose runtime id was trimmed can never be focused again (the round-eight
#: finding), so the advice this constant carried through two rewrites —
#: "focus the session it arrived on" — was never followable, and the
#: focus-scoped age-out it relied on never fired for the phantom: the leak.
#:
#: What is promised now is only what the round-eight split delivers:
#: :func:`~talaria.domain.state.age_out_approvals` withdraws every session's
#: stale approvals — focused or not — at
#: :data:`~talaria.domain.state.APPROVAL_STALE_AFTER` (300 seconds), and the
#: prompt's removal withdraws this derived item with it. Pinned by
#: ``test_a_trimmed_id_phantom_off_focus_ages_out_at_threshold``.
PHANTOM_APPROVAL_AGES_OUT: Final[str] = (
    "this approval has outlived the link to its session, so no answer sent "
    "from here can be shown to reach it; unanswered, it ages out of this "
    "queue once its 5-minute stale window passes"
)

APPROVAL_ON_DOWN_CONNECTION: Final[str] = (
    "this connection is down — the approval cannot be answered or confirmed"
    " from here until it comes back"
)
"""Why a polled approval is refused while its connection is down.

Not dropped, and the difference is the whole point. A row that polls a status
outside :data:`APPROVAL_DETAIL_TRIGGER_STATUSES` gives *negative* evidence — the
gateway says the session is not blocked — so its stale detail is cleared where
the poll is folded. A disconnected row gives *no* evidence: the approval may
still be outstanding, and dropping it would be the silent loss R14 forbids,
while offering it would send an answer into a socket that is gone. So the item
stays, visible and unanswerable, and says which of the two it is.

Both halves are pinned rather than asserted:
``test_a_poll_outside_the_trigger_statuses_clears_stale_approval_detail`` for the
negative-evidence exits, ``test_an_approval_on_a_down_connection_is_refused_not_dropped``
for this one, and ``test_a_poll_inside_the_trigger_statuses_keeps_the_approval``
for the leg that must NOT clear.
"""

#: Why an item that already has an answer travelling offers no second one (R21).
#:
#: Re-attached 2026-08-18: inserting ``APPROVAL_ON_DOWN_CONNECTION`` above this
#: constant orphaned the line onto that one, so a docs build would have credited
#: the R21 sentence to the down-connection refusal and left this constant
#: undocumented.
ANSWER_ALREADY_TRAVELLING: Final[str] = (
    "an answer for this prompt is already travelling to the gateway"
)


def approval_block_reason(
    *, is_head: bool, queued_count: int, observed_request_id: str
) -> str:
    """Why this approval may not be answered right now, or ``""`` (R18 amended).

    **One function, three callers, and that is the point.** The card projection
    (:func:`~talaria.domain.projection.prompt_view`) asks it to decide whether a
    control is offered; the queue (:func:`_with_head_of_queue_rules`) asks it to
    decide whether an item is answerable; and
    :func:`~talaria.domain.state.respond_to_prompt` refuses in the same two
    cases, in the "nothing was sent" wording of the same two sentences. A screen
    that offers what the registry refuses — or, far worse, a registry that allows
    what the screen should never have offered — is what having more than one copy
    of this rule buys.

    **That sentence was false when it was written, on 2026-08-17.** There were
    two callers: ``respond_to_prompt`` had its own inline copy of these branches,
    and review found the two disagreeing — a settled head left the queue offering
    the second approval while the registry refused it. The three are named above
    rather than counted so that the claim can be checked by following them, and
    ``test_the_queue_and_the_registry_agree_on_a_session_the_poll_also_answers``
    drives two of them against one real fleet — a session holding a driven
    approval and a polled one, the case a count-only registry misreads — so
    drift fails a test rather than a reading.

    A lone approval is always answerable: with one entry in the queue the answer
    lands on it or on an empty queue, and the reply's own ``resolved`` count says
    which.
    """
    if queued_count <= 1:
        return ""
    if not is_head:
        return QUEUED_BEHIND_APPROVAL
    return "" if observed_request_id else UNCORRELATED_APPROVAL


def unresolvable_kind_of(row: RegistryRow) -> str:
    """The named kind this row is blocked on that Talaria cannot render, or ``""``.

    Membership-by-absence, deliberately: a kind is unresolvable when Talaria has
    no card for it, and the list of cards Talaria has is
    :data:`QUEUEABLE_KINDS`. :data:`UNRESOLVABLE_KINDS` names the three the
    running revision has today, but nothing here reads that tuple — a fourth
    kind a later gateway grows is named on its row on the day it arrives.

    ``UNOBSERVED_KIND`` is not unresolvable and never answers here: an
    unobserved wait is a wait whose kind is *unknown*, which AE11 resolves by
    navigation and a latched failure, not by naming a kind nobody has seen.
    """
    kind = row.waiting_kind
    if not kind or kind == UNOBSERVED_KIND or kind in QUEUEABLE_KINDS:
        return ""
    return kind


def unresolvable_kind_notice(kind: str) -> str:
    """The line a registry row carries for a wait Talaria cannot resolve (R14).

    Written to survive :func:`~talaria.domain.normalize.clip_detail_line`'s
    120-character bound whole, because both halves are load-bearing: what the
    session is blocked on, and the fact that no queue row will ever offer it.

    ``terminal_read`` gets its own sentence because its resolution model is the
    opposite of the other suppressed kinds: it is answered **by machine** from
    the transcript projection, and only for the session on screen — telling the
    operator to "answer it in its own client" would send a human to do a
    machine's job. A background one simply expires on the gateway's own 30-second
    timer (``tui_gateway/server.py``, ``read_terminal_callback``'s ``_block(...,
    timeout=30)``). Pinned by
    ``test_a_background_terminal_read_names_its_row_with_the_machine_wording``.
    """
    if kind == "terminal_read":
        return clip_detail_line(
            "waiting on terminal_read: answered by machine for the shown session"
            " only, not queued — expires unanswered here"
        )
    return clip_detail_line(
        f"waiting on {kind}: no card here, not queued — answer it in its own client"
    )


# ── Feed B's raw material: one ``approval.pending`` row ───────────────────


@dataclass(frozen=True)
class PolledApproval:
    """One row of an ``approval.pending`` reply, decoded and de-credentialled.

    **The shape is not fixed and this decoder does not pretend otherwise.** Four
    constructors feed the queue entry whose ``data`` dict is returned verbatim by
    ``list_gateway_approvals`` (``tools/approval.py:2679``), and they do not
    agree: the MCP-elicitation constructor (``:5141-5146``) builds only
    ``command``, ``description``, ``pattern_key`` and ``pattern_keys`` — no
    ``allow_permanent``, no ``allow_session``. So every field but
    :attr:`request_id` is absent-not-false here, and
    :meth:`allowed_answers` derives choices from what is present rather than
    indexing keys that may not exist.

    ``request_id`` is the one key all four share: ``_ApprovalEntry.__init__``
    sets it by ``setdefault`` (``tools/approval.py:2596``). A row without one
    names no request an answer could be aimed at and is dropped by
    :func:`decode_pending_approvals`.

    **The text is redacted here, and that is not belt-and-braces.** The evented
    path rewrites ``command`` through the gateway's *forced* redactor in its
    payload builder (``server.py:1928-1931``); ``list_gateway_approvals`` does
    not call that builder at all, three of the four constructors pre-redact with
    the *unforced* redactor, and the MCP-elicitation one stores the raw message
    with no redaction whatsoever. A polled row is therefore untrusted,
    unredacted gateway text, and R22 is Talaria's promise rather than the
    gateway's.
    """

    request_id: str
    command: str = ""
    description: str = ""
    choices: tuple[str, ...] = ()
    #: Frame-clock stamp of the first poll that saw this approval — KTD12's
    #: observation floor, never a start time. No row and no event payload
    #: carries a start stamp at any revision U1 examined.
    first_seen_at: float = 0.0

    def summary(self) -> str:
        """The one line the queue row shows for this approval.

        ``description`` first, because for an approval the gateway's description
        carries the *pattern warnings* that triggered the prompt rather than the
        command — the distinction :class:`~talaria.domain.models.PendingPrompt`
        documents at length — and the command travels in its own field so a
        renderer can wrap it whole.
        """
        return self.description or self.command or "approval requested"


@dataclass(frozen=True)
class PendingApprovalDirectory:
    """Every approval one ``approval.pending`` reply reported, plus whether it
    reported at all.

    ``answered`` is the R24 half and the reason this is not a bare tuple. A
    reply that does not carry the pinned ``approvals`` key told Talaria
    *nothing*, and an empty tuple read as "this session has no approvals" would
    be the fabricated zero R10 exists to prevent. The caller keeps whatever it
    knew and says the detail is unavailable.
    """

    approvals: tuple[PolledApproval, ...] = ()
    answered: bool = False


def decode_pending_approvals(result: Any, *, at: float) -> PendingApprovalDirectory:
    """Turn an ``approval.pending`` reply into rows, degrading rather than raising.

    The top-level key is ``approvals`` (pinned in
    :data:`~talaria.domain.compat.COMPAT_BASELINE` at ``RUNNING_PIN``). A reply
    that is not a mapping, or that carries no such list, answers
    ``answered=False`` — this runs on a poll loop, and a malformed reply must
    mark detail unavailable, never crash the poller and never read as zero.
    """
    if not isinstance(result, Mapping):
        return PendingApprovalDirectory()
    raw = result.get("approvals")
    if not isinstance(raw, list):
        return PendingApprovalDirectory()
    rows: list[PolledApproval] = []
    for row in raw:
        if not isinstance(row, Mapping):
            continue
        request_id = coerce_text(row.get("request_id"))
        if not request_id:
            continue
        raw_choices = row.get("choices")
        choices = (
            tuple(c for c in raw_choices if isinstance(c, str))
            if isinstance(raw_choices, list)
            else ()
        )
        rows.append(
            PolledApproval(
                request_id=request_id,
                command=redact_probe_detail(row.get("command")),
                description=redact_probe_detail(row.get("description")),
                choices=choices,
                first_seen_at=at,
            )
        )
    return PendingApprovalDirectory(approvals=tuple(rows), answered=True)


def merge_polled_approvals(
    known: Sequence[PolledApproval], fresh: Sequence[PolledApproval]
) -> tuple[PolledApproval, ...]:
    """Fold a fresh poll over what was already known, keeping first sightings.

    The gateway's order is kept — it is the queue order its resolver pops from
    (``tools/approval.py:2214-2222``, oldest first) and therefore the order R18's
    head-of-queue rule aims at. What survives from the previous poll is only
    :attr:`PolledApproval.first_seen_at`: re-stamping it every poll would reset
    the observation floor every two seconds and render a wait that has been
    outstanding for ten minutes as seconds old (KTD12).

    An approval the fresh reply does not list is **gone**: the gateway is no
    longer holding it, which is the confirmed resolution R18 waits for.
    """
    first_seen = {row.request_id: row.first_seen_at for row in known}
    return tuple(
        replace(row, first_seen_at=first_seen.get(row.request_id, row.first_seen_at))
        for row in fresh
    )


def approval_detail_due(row: RegistryRow, *, seam: SeamStatus | None) -> bool:
    """Whether ``approval.pending`` may be called for this row right now (KTD11).

    Four gates, and every one of them is checkable by reading this function.

    * The seam must be **present**. ``None`` is never-observed and every other
      verdict is a seam that did not answer; asking anyway would be asking a
      question whose absence R10 requires be named rather than retried blind.
    * The row's status must be in :data:`APPROVAL_DETAIL_TRIGGER_STATUSES` —
      ``waiting`` or ``working`` per the operator ruling of 2026-08-17. A row
      whose status is ``""`` (never reported) or a word this gateway invented is
      not a member, so no path here fires at a row of unknown status. The
      status set is driven exhaustively by
      ``test_approval_detail_fires_at_waiting_and_working_and_nothing_else`` and
      the seam set by
      ``test_approval_detail_is_never_asked_of_a_seam_that_did_not_answer``, so
      the claim fails a test rather than a reading.
    * A reclaimed row's session no longer exists.
    * A disconnected row has no live connection to ask on.

    Ownership deliberately does **not** gate this. KTD11's hazard is warming a
    *lazy* agent build, and every row this fires at is mid-turn or blocked by
    construction; whether Talaria drives it changes nothing about that, and a
    session this run once drove can have been taken by another client since
    (:func:`~talaria.domain.registry.attach_displaces_client` records why
    ownership does not expire). The duplicate detail that a driven session's
    poll returns costs one item identity, which both feeds already share.
    """
    if seam != "present":
        return False
    if row.reclaimed_reason is not None or row.disconnected:
        return False
    return row.status in APPROVAL_DETAIL_TRIGGER_STATUSES


# ── Feed A: the prompt registry, with its stamps kept ─────────────────────


def prompt_feed_rows(
    prompts: Sequence[PendingPrompt],
    answering: Sequence[PendingPrompt] = (),
) -> tuple[QueuePrompt, ...]:
    """Project the prompt registry into feed A's rows (the new projection, U6).

    ``opened_at`` and ``seq`` exist on every
    :class:`~talaria.domain.models.PendingPrompt` and are dropped by the card
    projection because a card does not need them. The queue is ordered by wait
    age and tie-broken by arrival, so it does.

    ``answering`` is included and flagged, not skipped. A prompt whose answer is
    on the wire is still outstanding *at the gateway* — the reply that would say
    otherwise has not arrived — and R18's whole rule is that a row clears on
    confirmation rather than on optimism.

    No filtering by focus happens here, and that is R14's instruction rather
    than an omission: the queue is the install's whole truth, and the card the
    operator is already looking at is one of its rows.
    """
    return tuple(
        QueuePrompt(
            request_id=prompt.request_id,
            kind=prompt.kind,
            summary=prompt.summary,
            opened_at=prompt.opened_at,
            seq=prompt.seq,
            choices=prompt.choices,
            session_id=prompt.session_id,
            command=prompt.command,
            observed_request_id=prompt.observed_request_id,
            in_flight=in_flight,
        )
        for source, in_flight in ((prompts, False), (answering, True))
        for prompt in source
    )


# ── The queue ────────────────────────────────────────────────────────────


ItemKey = tuple[str, str, str]


@dataclass(frozen=True)
class NeedsYouQueue:
    """One flat, wait-age-ordered queue, plus what could not be asked.

    ``notices`` is not decoration. A connection whose roster or approval-detail
    probe failed answers nothing, and from the items alone that is
    indistinguishable from a connection with nothing waiting on it — which for a
    surface whose entire job is telling the operator what needs them is the worst
    available error, because "we could not ask" would read as "you are free".
    """

    items: tuple[QueueItem, ...] = ()
    notices: tuple[str, ...] = ()

    @property
    def count(self) -> int:
        return len(self.items)

    @property
    def is_empty(self) -> bool:
        return not self.items

    @property
    def oldest(self) -> QueueItem | None:
        """The head of the queue — oldest wait first (R15)."""
        return self.items[0] if self.items else None

    def item_for(self, identity: ItemKey) -> QueueItem | None:
        for item in self.items:
            if item.identity == identity:
                return item
        return None


def _order_key(item: QueueItem) -> tuple[float, int, str, str, str]:
    """R15's ordering: oldest wait first, deterministic under equal ages.

    ``seq`` breaks a tie between two prompts stamped at the same frame time, and
    the identity triple breaks the rest — two polled items share a poll's single
    stamp and carry no frame sequence of their own, so without a total order the
    queue could reorder itself between two renders of identical state and AE8's
    replay determinism would be false.
    """
    return (item.opened_at, item.seq, item.profile, item.session_id, item.request_key)


def build_queue(
    *,
    rows: Mapping[RowKey, RegistryRow],
    aliases: Mapping[tuple[str, str], RowKey],
    focused_profile: str,
    focused_session_id: str | None = None,
    feed_a: Sequence[QueuePrompt] = (),
    approval_detail: Mapping[RowKey, tuple[PolledApproval, ...]] | None = None,
    channels: Mapping[str, ConnectionChannel] | None = None,
    boards: Mapping[str, SeamBoard] | None = None,
    answers: Mapping[ItemKey, float | None] | None = None,
    settled: frozenset[ItemKey] | None = None,
) -> NeedsYouQueue:
    """Build the queue from the fleet's parts. Pure, and the only builder.

    The two feeds are folded in order — driven first, then polled — and the
    dedupe is stated twice because one statement of it is not enough:

    1. **By identity.** ``(profile, durable session id, request key)`` is one
       item however many feeds saw it. Feed A's rows carry runtime session ids;
       they are resolved through ``aliases`` to the row's durable key here, which
       is what makes a driven sighting and a polled sighting of one approval
       collide rather than double.
    2. **By what can be shown to be the same thing, and never by guesswork.**
       Only one feed-B shape is ever dropped, and only where dropping it is
       provable:

       * A **roster** item is the flattened word ``waiting`` with no request key
         at all — its key is :data:`ROSTER_REQUEST_KEY`, minted by Talaria, a
         shape neither key form at ``models.py``'s ``PendingPrompt.request_id``
         takes — so it is never matched by identity. It is dropped only for
         a session that a **visible** feed-A item covers — visible, not merely
         held: a feed-A prompt hidden by AE2's settled latch covers nothing,
         because the latch withholds the settled item's *control* and a roster
         item carries no control to restore (:data:`ROSTER_ITEM_NOT_ANSWERABLE`),
         while the wait the roster reports is still the gateway's word. Pinned by
         ``test_a_settled_feed_a_item_does_not_cover_the_roster_wait``. An
         earlier draft dropped the roster item for any session feed A held at
         all and called that "safe" because the roster item carried "strictly
         less information" — cited nothing, and was false in the settled case:
         the one waiting session rendered as no item at all, which is the
         answer the module head forbids. The undercount stays on record here
         because it is the evidence that an uncited "safe" was not.
       * A **polled approval** leaves the list by exactly two routes, named
         here rather than counted because an earlier draft of this sentence said
         "only by rule 1" and there were two: rule 1's exact identity match, and
         a settled tombstone, which is AE2's latch and not a deduplication at
         all. It is **never** dropped because feed A happens to hold some
         approval for the same session — pinned by
         ``test_attaching_mid_wait_hides_no_approval_and_doubles_none`` and
         ``test_a_ghost_feed_a_approval_masks_no_live_polled_one``. When feed A
         holds an *uncorrelated* approval there (one the gateway sent no
         ``request_id`` for, leaving a locally synthesized key), the polled rows
         are kept and flagged
         :attr:`~talaria.domain.models.QueueItem.possibly_duplicate`, so the
         surface can say the two might be one thing rather than Talaria deciding
         it. That flag has a second writer on the driven side —
         :func:`_feed_a_items` sets it on any queueable-kind item whose row
         could not be derived (the aged-alias, no-gateway-id state), where the
         unplaceable copy is the one carrying the doubt.

       **Two wrong versions of this shipped before the third, and both wrong ones
       lost real work.** The first was a blanket per-session rule: a session
       where feed A held one approval and the poll returned two lost the second
       entirely, and a single *clarify* in feed A suppressed a polled approval,
       which is not even the same question. The second masked one polled approval
       per uncorrelated driven one — but nothing correlated *which*, so the mask
       ate the gateway's head. Attaching to a session mid-wait made the queue show
       one command twice and never show the other, which is both failures at once.

       The lesson is in rule 2's wording. Any positional or count-based guess
       about which polled row a keyless driven prompt refers to is a guess, and a
       wrong guess hides a human-blocking approval where nobody can find it.
       Showing one approval twice is visible and self-correcting; hiding one is
       neither. Over-report, and label the doubt.

       **Stated limit.** Feed A can also hold a prompt the gateway has already
       dropped — it removes a queue entry on timeout and on interrupt without
       emitting anything — so a ghost item can outlive the thing it names. That is
       an over-report in the same survivable direction: the operator answers it
       and AE2's settle-and-latch records that the gateway was not waiting.
    """
    items: list[QueueItem] = []
    seen: set[ItemKey] = set()
    covered_sessions: set[tuple[str, str]] = set()
    uncorrelated_sessions: set[tuple[str, str]] = set()
    shadowed_approvals: dict[tuple[str, str], int] = {}
    settled_seen: set[ItemKey] = set()
    settled_keys = settled or frozenset()
    answer_index = _resolve_answer_index(answers or {}, aliases)

    def _hide(item: QueueItem) -> bool:
        """Whether ``item`` is already held or settled — and count it if settled.

        A settled approval keeps its place in the head-of-queue accounting even
        though it leaves the list. Latching says the answer's outcome was
        *ambiguous*, so the gateway may still be holding that entry at the head
        of its own queue — and an answer aimed at the next one would land on it.
        Forgetting it here is what let the queue offer a second approval the
        registry refuses.

        **A settled roster item neither shadows nor hides, for the same reason it
        is excluded from the head accounting** (:data:`ROSTER_ITEM_NOT_ANSWERABLE`):
        it is a status word, not an entry the gateway holds, so there is no
        control a latch could suppress and nothing a shadow could stand for. Both
        halves were missed when that rule was written, and each omission was
        permanent in the worst direction. The shadow half: a shadow both
        suppresses headship and inflates ``queued_count``, so one settled roster
        item made every later approval on that session unanswerable for the life
        of the run — pinned by
        ``test_a_settled_roster_item_shadows_no_later_approval``. The hide half:
        :data:`ROSTER_REQUEST_KEY` is a module constant, one key for every wait a
        session ever has, so one tombstone named them all and a session that
        waited again later was absent from the queue for the life of the run —
        the "needs-you: none" answer the module head forbids — pinned by
        ``test_a_settled_roster_wait_is_not_a_permanent_tombstone``.
        """
        if item.identity in seen:
            return True
        if item.identity in settled_keys:
            if item.source == SOURCE_ROSTER:
                return False
            if item.kind == "approval" and item.identity not in settled_seen:
                settled_seen.add(item.identity)
                session = (item.profile, item.session_id)
                shadowed_approvals[session] = shadowed_approvals.get(session, 0) + 1
            return True
        return False

    feed_a_built, phantom_sessions = _feed_a_items(
        feed_a,
        rows=rows,
        aliases=aliases,
        profile=focused_profile,
        focused_session_id=focused_session_id,
        answers=answer_index,
        anchors=_polled_anchors(approval_detail or {}),
    )
    for item in feed_a_built:
        session = (item.profile, item.session_id)
        # Recorded *before* the hide, unlike coverage below: a settled
        # uncorrelated approval is latched-ambiguous, so the gateway may still
        # hold its entry, and the doubt it casts on the polled copies stands —
        # the same reasoning that keeps settled approvals in ``_hide``'s shadow
        # counting.
        if item.kind == "approval" and not item.observed_request_id:
            uncorrelated_sessions.add(session)
        if _hide(item):
            continue
        seen.add(item.identity)
        # Coverage is recorded *after* the hide (docstring rule 2, the roster
        # bullet): only an item the operator can see may stand in for the
        # roster's word that the session is waiting.
        covered_sessions.add(session)
        items.append(item)

    for item in _feed_b_items(
        rows=rows,
        approval_detail=approval_detail or {},
        answers=answer_index,
    ):
        session = (item.profile, item.session_id)
        if item.source == SOURCE_ROSTER and session in covered_sessions:
            continue
        if item.source == SOURCE_APPROVAL_POLL and session in uncorrelated_sessions:
            item = replace(item, possibly_duplicate=True)
        if _hide(item):
            continue
        seen.add(item.identity)
        items.append(item)

    ordered = tuple(sorted(items, key=_order_key))
    return NeedsYouQueue(
        items=_with_head_of_queue_rules(ordered, shadowed_approvals, phantom_sessions),
        notices=_all_connection_notices(
            channels=channels or {}, boards=boards or {}, focused_profile=focused_profile
        ),
    )


def _resolve_answer_index(
    answers: Mapping[ItemKey, float | None],
    aliases: Mapping[tuple[str, str], RowKey],
) -> dict[ItemKey, float | None]:
    """Re-key in-flight answers by the row each names, so both feeds find them.

    An answer is recorded under whatever session id its caller held — durable
    where the caller had one, a runtime alias otherwise (the contract
    :meth:`~talaria.domain.state.FleetState.protected_keys` states). An item is
    keyed by the row's durable id. Resolving one hop through the alias map here
    is what stops an answer sent against a runtime id from leaving its own item
    rendering as unanswered while it travels.
    """
    index: dict[ItemKey, float | None] = {}
    for (profile, session_id, request_key), at in answers.items():
        index[(profile, session_id, request_key)] = at
        row_key = aliases.get((profile, session_id))
        if row_key is not None:
            index.setdefault((row_key[0], row_key[1], request_key), at)
    return index


def _feed_a_items(
    feed_a: Sequence[QueuePrompt],
    *,
    rows: Mapping[RowKey, RegistryRow],
    aliases: Mapping[tuple[str, str], RowKey],
    profile: str,
    focused_session_id: str | None,
    answers: Mapping[ItemKey, float | None],
    anchors: Mapping[tuple[str, str], RowKey],
) -> tuple[list[QueueItem], frozenset[tuple[str, str]]]:
    """Feed A (KTD2): the sessions Talaria drives, from the prompt registry.

    The second return value is the phantom sessions: the ``(profile, session)``
    keys of **approval** items whose row could not be derived — an aged-out
    alias with no gateway id to recover through. They are returned whether or
    not the item later hides (a settled phantom is latched-ambiguous, so the
    gateway may still hold its entry), and :func:`_with_head_of_queue_rules`
    reads them as the unplaceable fold its docstring describes.
    """
    built: list[QueueItem] = []
    phantom_sessions: set[tuple[str, str]] = set()
    for prompt in feed_a:
        if prompt.kind not in QUEUEABLE_KINDS:
            # R14's terminal-read clause, and any future machine-answered
            # bridge: not a question for a person, so not a row in the queue.
            continue
        session_id = prompt.session_id or focused_session_id or ""
        row_key = _row_key_for(profile, session_id, rows=rows, aliases=aliases)
        unanchored = False
        if row_key not in rows:
            # The alias aged out, so this key names no row. A gateway id is the
            # same string a poll saw, so where the gateway sent one it names the
            # row this prompt belongs to — a derivation, not a guess.
            #
            # `observed_request_id` and not `request_id`, and the honest
            # account is that this is intent rather than behaviour: mutation
            # testing shows the two spellings are equivalent here. `request_id`
            # is the REGISTRY key — the gateway's id when one was observed, and a
            # locally synthesized `approval:<session>#<n>` otherwise (models.py,
            # the `PromptRequest.request_id` comment) — and a synthesized key
            # carries colons while every gateway id is uuid4 hex, so it can never
            # appear in a map built from gateway ids and the lookup always misses
            # to the same None. The spelling is kept because it states the
            # precondition instead of leaning on that non-collision, but no test
            # can distinguish it and none pretends to.
            #
            # The fix for the doubling is the `unanchored` branch below, not this
            # line. What was wrong before was `anchors.get(..., row_key)` — a
            # default that silently kept the phantom key and emitted the item
            # with no doubt attached.
            anchored = (
                anchors.get((profile, prompt.observed_request_id))
                if prompt.observed_request_id
                else None
            )
            if anchored is None:
                # No gateway id to derive from, so the two feeds cannot be shown
                # to name one thing. That is the state this unit answers by
                # over-reporting and labelling the doubt, never by hiding one of
                # them or guessing which row it belongs to. An unplaceable
                # *approval* is additionally shown-but-not-offered while any
                # other session holds approvals — the fold
                # ``_with_head_of_queue_rules`` documents, pinned by
                # ``test_a_phantom_blocks_itself_but_not_its_id_carrying_polled_copy``.
                unanchored = True
            else:
                row_key = anchored
        row = rows.get(row_key)
        if unanchored and prompt.kind == "approval":
            phantom_sessions.add((profile, row_key[1]))
        requested_at = answers.get((profile, row_key[1], prompt.request_id))
        built.append(
            QueueItem(
                profile=profile,
                session_id=row_key[1],
                request_key=prompt.request_id,
                source=SOURCE_DRIVEN,
                kind=prompt.kind,
                summary=prompt.summary,
                row_key=row_key,
                choices=prompt.choices,
                command=prompt.command,
                opened_at=prompt.opened_at,
                seq=prompt.seq,
                # Talaria watched the request frame arrive, so this is a start
                # stamp rather than a floor (KTD12's other half).
                age_is_floor=False,
                observed_request_id=prompt.observed_request_id,
                # The flag's second writer — the first is ``build_queue``'s
                # rule-2 polled branch, and until CR6 round six only that one
                # was documented. This one fires on a *driven* item of any
                # queueable kind whose row could not be derived: a poll may
                # already show the same wait under the durable row, so the
                # unplaceable copy carries the doubt itself. A driven clarify
                # with a trimmed alias comes back ``possibly_duplicate=True``,
                # not only approvals (U7 labels off this flag).
                possibly_duplicate=unanchored,
                requested=prompt.in_flight,
                requested_at=requested_at,
                session_title=row.title if row is not None else "",
                stale_since=row.stale_since if row is not None else None,
            )
        )
    return built, frozenset(phantom_sessions)


def _feed_b_items(
    *,
    rows: Mapping[RowKey, RegistryRow],
    approval_detail: Mapping[RowKey, tuple[PolledApproval, ...]],
    answers: Mapping[ItemKey, float | None],
) -> list[QueueItem]:
    """Feed B (KTD2): the sessions Talaria does not drive, from polls.

    Two shapes, and they can both be true of one session at once. The detailed
    shape is an ``approval.pending`` row, one item each. The flattened shape is a
    roster row reporting ``waiting`` with no kind, one ``unobserved`` item.

    **Both are emitted when both are present, and that is evidence-led rather
    than cautious.** The gateway's ``waiting`` status is computed from the
    ``_block()`` prompt registry alone (``server.py:8471`` at the pin), which
    holds clarify, sudo, secret and the three unrenderable kinds — approvals live
    in a different structure the status function never reads. So a row that is
    both ``waiting`` and holding approvals is blocked on two different things,
    and collapsing them would hide one. (Source-derived at the pin; U1 could not
    induce an approval on this install, and U9 confirms it on an
    approvals-enabled gateway. If it turns out wrong, the cost is one extra
    ``unobserved`` row beside a named approval — an over-report, which for this
    surface is the survivable direction.)
    """
    built: list[QueueItem] = []
    for row_key, row in rows.items():
        if row.reclaimed_reason is not None:
            # The session is gone. Its wait cannot be answered by anyone, and
            # the row itself latches ``reclaimed(reason)`` where the operator
            # can see it — a queue row would be a task nobody can finish.
            continue
        for index, approval in enumerate(approval_detail.get(row_key, ())):
            identity = (row_key[0], row_key[1], approval.request_id)
            built.append(
                QueueItem(
                    profile=row_key[0],
                    session_id=row_key[1],
                    request_key=approval.request_id,
                    source=SOURCE_APPROVAL_POLL,
                    kind="approval",
                    summary=approval.summary(),
                    row_key=row_key,
                    choices=approval.choices,
                    command=approval.command,
                    opened_at=approval.first_seen_at,
                    # The gateway's own queue order, which is the order its
                    # resolver pops from and therefore the order R18 aims at.
                    seq=index,
                    age_is_floor=True,
                    observed_request_id=approval.request_id,
                    requested=identity in answers,
                    requested_at=answers.get(identity),
                    session_title=row.title,
                    stale_since=row.stale_since,
                    # Visible and refused rather than dropped. A disconnected
                    # row cannot be refreshed (``approval_detail_due`` refuses
                    # it), so its detail can never be shown to have resolved —
                    # but unlike a row that polled a non-blocking status, the
                    # evidence here is MISSING rather than negative: the
                    # approval may well still be outstanding. Dropping it would
                    # be the silent loss R14 forbids; offering it would send an
                    # answer into a socket that is gone. Pinned by
                    # ``test_an_approval_on_a_down_connection_is_refused_not_dropped``.
                    answerable=not row.disconnected,
                    blocked_reason=(
                        APPROVAL_ON_DOWN_CONNECTION if row.disconnected else ""
                    ),
                )
            )
        if row.status != "waiting":
            continue
        kind = row.waiting_kind or UNOBSERVED_KIND
        if unresolvable_kind_of(row):
            # Named on the row by the router, never queued (KTD2/R14).
            continue
        identity = (row_key[0], row_key[1], ROSTER_REQUEST_KEY)
        built.append(
            QueueItem(
                profile=row_key[0],
                session_id=row_key[1],
                request_key=ROSTER_REQUEST_KEY,
                source=SOURCE_ROSTER,
                kind=kind,
                summary=_roster_summary(kind),
                row_key=row_key,
                # The roster carries no start stamp of any kind (verified live),
                # so the first poll that saw the wait is the floor and the only
                # honest age (KTD12).
                opened_at=row.status_floor_at,
                seq=0,
                age_is_floor=True,
                requested=identity in answers,
                requested_at=answers.get(identity),
                session_title=row.title,
                stale_since=row.stale_since,
            )
        )
    return built


def _roster_summary(kind: str) -> str:
    if kind == UNOBSERVED_KIND:
        return (
            "waiting on a prompt of an unknown kind — this gateway reports only "
            "that the session is waiting"
        )
    return f"waiting on a {kind} prompt"


def _row_key_for(
    profile: str,
    session_id: str,
    *,
    rows: Mapping[RowKey, RegistryRow],
    aliases: Mapping[tuple[str, str], RowKey],
) -> RowKey:
    """The registry key a session id names on one connection.

    Resolved rather than assumed, per the contract
    :meth:`~talaria.domain.state.FleetState.protected_keys` states for anything
    recording protection: a bare runtime id is re-anchored by nothing, so a queue
    item recorded under one loses its row's protection the moment that alias ages
    out of the row's four-slot window.
    """
    alias = aliases.get((profile, session_id))
    if alias is not None and alias in rows:
        return alias
    key = (profile, session_id)
    if key in rows:
        return key
    return key


def _polled_anchors(
    approval_detail: Mapping[RowKey, tuple[PolledApproval, ...]],
) -> dict[tuple[str, str], RowKey]:
    """Which row each polled approval's **gateway id** belongs to.

    The recovery route for a feed-A prompt whose session id no longer resolves.
    The runtime-alias index is bounded at four ids per row (U3's deliberate memory
    bound), so a prompt registered under a runtime id can outlive the alias that
    mapped it to its row — and then :func:`_row_key_for` returns a key naming no
    row, feed B keys the same approval by the durable row, the two identities
    differ, and one approval is emitted twice. (Until 2026-08-17 both copies were
    also *answerable* — the split put each in a session of its own, switching the
    cross-feed head rule off. The fold in :func:`_with_head_of_queue_rules`
    refused both until the round-seven ruling narrowed it, 2026-08-18: the
    keyless copy stays refused, while a copy carrying a gateway id is aimed by
    id — ``tools/approval.py:2655-2658`` selects by id, no fallthrough — and
    stays answerable.)

    A gateway-supplied ``request_id`` is the same string on both sides, so it
    correlates them exactly. This is a *derivation*, not the guess the coverage
    rule used to make: the id came from the gateway, and the row it was polled
    against is where it belongs.
    """
    anchors: dict[tuple[str, str], RowKey] = {}
    for row_key, approvals in approval_detail.items():
        for approval in approvals:
            anchors.setdefault((row_key[0], approval.request_id), row_key)
    return anchors


def _with_head_of_queue_rules(
    items: tuple[QueueItem, ...],
    shadowed: Mapping[tuple[str, str], int],
    phantom_sessions: frozenset[tuple[str, str]] = frozenset(),
) -> tuple[QueueItem, ...]:
    """Apply R18's head-of-queue rule to every session's approvals.

    The gateway holds approvals per session in a **queue** and resolves from its
    head; it also removes an entry on timeout and on interrupt without emitting
    anything. Two consequences, and they are the amended R18 exactly:

    * A session's second approval is shown but never offered. An answer aimed at
      it would land on the first — there is no aiming, only a head.
    * The head is answerable when the gateway supplied a request id for it, and
      only then. With an id the answer says which entry it means; without one it
      is the uncorrelated case the shipped refusal and the deny-all fallback
      already govern, unchanged, because a single-approval session is
      unambiguous by count while a two-approval session is not.

    An item with an answer already travelling offers no second answer either
    (R21), whatever its position.

    ``shadowed`` counts a session's **settled** approvals — ones latched after an
    ambiguous outcome, which leave the list but not the gateway's queue. They are
    counted here and they suppress headship, because latching means the outcome
    was never confirmed: the gateway may still hold that entry at the head, and
    an answer aimed at the next one would land on it. Leaving them out is what
    let this function offer an approval that
    :func:`~talaria.domain.state.respond_to_prompt` refuses.

    **Headship also needs the order to be knowable, and across the two feeds it
    is not.** A driven approval carries the moment its request frame arrived; a
    polled one carries only an observation floor, because no revision U1 read
    stamps an approval with a start time (KTD12). Sorting the two together
    produces an order, but not a *true* one — the polled entry may have been
    waiting long before the floor says. So a session holding approvals from both
    feeds has no provable head and none of them is offered. A session whose
    approvals all come from one feed does: driven ones are ordered by the frame
    sequence they arrived on, and polled ones by the gateway's own queue order,
    which is the order its resolver pops from.

    **A phantom approval widens that unprovability to every blind answer on
    its connection** (``phantom_sessions``: driven approvals whose row could
    not be derived — the aged-alias state :func:`_feed_a_items` records). Such
    an item's session key is a runtime id naming no row, so the gateway
    session it actually belongs to may be *any* session on its connection —
    including one whose approvals are accounted here under the durable key.
    Splitting one gateway session's accounting in two is exactly how the
    cross-feed refusal above was once switched off by the alias window's
    memory bound, offering two answers at one FIFO. The fold refuses in two
    branches, each producing its refusal and its sentence from one
    computation, so the two cannot disagree:

    * **The phantom itself, while any other session holds approvals** —
      refused with :data:`PHANTOM_APPROVAL_AGES_OUT`, which promises only the
      exit the round-eight split delivers: the unconditional age-out. An
      observed gateway id on the phantom does not
      exempt it: its answer travels under its dead runtime id, and the
      gateway's ``approval.respond`` resolves the session before any queue is
      touched (``_sess_nowait``, ``tui_gateway/server.py:2507-2509``), so the
      aim never gets to matter. Pinned by the phantom assertions of
      ``test_a_phantom_blocks_itself_but_not_its_id_carrying_polled_copy``
      and ``test_age_out_takes_the_phantom_with_the_live_approval``.
    * **A blind sibling of a phantom** — an approval with no observed gateway
      id, whose answer would be a bare head-pop at a queue the phantom's
      entry may head — refused with
      :data:`UNPLACEABLE_APPROVAL_ON_CONNECTION`. Pinned by
      ``test_a_blind_sibling_session_stays_refused_beside_a_phantom``.

    **An approval carrying an observed gateway request id is exempt from the
    fold** (the round-seven ruling, 2026-08-18): its answer is aimed by id —
    ``resolve_gateway_approval`` (``tools/approval.py:2655-2658``) selects
    entries by id and returns 0 on no match, the head-pop structurally
    unreachable — so it cannot land on the phantom's entry, and it falls
    through to the ordinary per-session rules below. Pinned by
    ``test_an_id_carrying_sibling_session_stays_answerable_beside_a_phantom``
    and ``test_an_id_carrying_anchored_sibling_stays_answerable_beside_a_phantom``.

    (Until CR6 round six the fold instead forged
    :func:`approval_block_reason` inputs and borrowed the per-session
    :data:`QUEUED_BEHIND_APPROVAL` sentence — true only inside the phantom's
    own session, false on every sibling session the fold reaches, whose sole
    anchored approval has no predecessor to wait for.) A phantom
    session that is the *only* approval-holding session on its connection keeps
    the ordinary same-session rules — matching what the registry's own
    session-scoped count allows — pinned by the pre-poll leg of
    ``test_a_phantom_blocks_itself_but_not_its_id_carrying_polled_copy``, where
    the lone phantom stays offered until a poll shows another session holding
    approvals. Stated limit: a
    phantom whose prompt the gateway has already dropped blocks its
    connection's blind approvals until the prompt leaves the registry — the
    refusal rides each blocked item's ``blocked_reason``, which is the
    survivable direction this module's history argues for, where hiding is
    not. The block is exactly as wide as the phantom's registry presence,
    and since the round-eight split that presence is bounded: the registry
    sheds every stale approval at
    :data:`~talaria.domain.state.APPROVAL_STALE_AFTER` whether or not its
    session is focused, so the fold over a dropped-prompt phantom lasts at
    most one stale window — pinned by
    ``test_the_fold_ends_when_the_phantom_ages_out`` (the exit) and
    ``test_a_trimmed_id_phantom_off_focus_ages_out_at_threshold``
    (round eight's inversion of round seven's no-exit finding).
    """
    heads: dict[tuple[str, str], QueueItem] = {}
    counts: dict[tuple[str, str], int] = {}
    feeds: dict[tuple[str, str], set[bool]] = {}
    for item in items:
        if item.kind != "approval" or item.source == SOURCE_ROSTER:
            continue
        session = (item.profile, item.session_id)
        counts[session] = counts.get(session, 0) + 1
        feeds.setdefault(session, set()).add(item.age_is_floor)
        if session not in heads:
            heads[session] = item

    # Every approval-holding session per profile: the visible ones (``counts``)
    # plus the settled-latched ones (``shadowed`` — the gateway may still hold
    # those entries, the same reasoning the shadow counting itself states).
    sessions_by_profile: dict[str, set[tuple[str, str]]] = {}
    for session in (*counts, *shadowed):
        sessions_by_profile.setdefault(session[0], set()).add(session)
    phantoms_by_profile: dict[str, set[tuple[str, str]]] = {}
    for session in phantom_sessions:
        phantoms_by_profile.setdefault(session[0], set()).add(session)

    marked: list[QueueItem] = []
    for item in items:
        if item.blocked_reason == APPROVAL_ON_DOWN_CONNECTION:
            # LOAD-BEARING for the refusal SENTENCE, which an earlier draft of
            # this comment denied. Removing the branch leaves the whole suite
            # green (round twelve's X4 mutation), and that survival was first
            # read as proof the branch was redundant. It is not: with two polled
            # approvals on one session and the connection then lost, the second
            # one falls through to ``approval_block_reason`` and comes back
            # "an earlier approval in this session is still waiting" — true of
            # its position and false about why it cannot be answered, which is
            # that the socket is gone. Measured both ways:
            #
            #   with this branch:    gw-1 down-connection, gw-2 down-connection
            #   without it:          gw-1 down-connection, gw-2 "an earlier …"
            #
            # ``answerable`` is False either way, so nothing here can be
            # answered wrongly; what the branch protects is the operator being
            # told the true reason. The green suite meant only that no test
            # looked, which is a coverage gap and not an equivalence — the
            # distinction the corrected rule in
            # ``docs/engineering-journal/LEARNINGS.md`` now turns on. Pinned by
            # the second-approval leg of
            # ``test_an_approval_on_a_down_connection_is_refused_not_dropped``.
            marked.append(item)
            continue
        if item.requested:
            marked.append(
                replace(item, answerable=False, blocked_reason=ANSWER_ALREADY_TRAVELLING)
            )
            continue
        if item.source == SOURCE_ROSTER:
            marked.append(
                replace(item, answerable=False, blocked_reason=ROSTER_ITEM_NOT_ANSWERABLE)
            )
            continue
        if item.kind != "approval":
            marked.append(item)
            continue
        session = (item.profile, item.session_id)
        hidden = shadowed.get(session, 0)
        # KEPT DELIBERATELY, CURRENTLY UNOBSERVABLE. The per-profile keying
        # here is the fold's stated scope — a phantom folds its own
        # connection, never the fleet — but since the round-seven narrowing
        # it is equivalent under mutation: the driver's round-eight mutant
        # (2026-08-18) replaced this lookup with the fleet-wide
        # ``phantom_sessions`` set and passed every test. That is structural,
        # not a coverage gap:
        #
        # * every phantom carries the focused profile —
        #   ``phantom_sessions.add((profile, row_key[1]))`` lives in
        #   :func:`_feed_a_items`, whose only caller passes
        #   ``profile=focused_profile``;
        # * every blind approval is focused-profile too — a polled approval
        #   always carries ``observed_request_id`` because
        #   :func:`decode_pending_approvals` drops keyless rows (its
        #   ``if not request_id: continue`` guard), and roster items leave
        #   through their own branch above, so the only blind approvals are
        #   feed A's, built under ``focused_profile``;
        # * and the membership branch below is keying-invariant outright,
        #   since the set holds ``(profile, session_id)`` tuples.
        #
        # So the blind branch only ever compares a focused-profile item
        # against focused-profile phantoms, and per-profile versus fleet-wide
        # keying cannot differ in any reachable state. It WAS observable
        # before the narrowing: the un-narrowed fold refused id-carrying
        # siblings, which do exist on other connections, and
        # ``test_a_phantom_on_one_connection_folds_nothing_on_another``
        # killed this same mutant in round six. The id-carrying exemption
        # removed that observability — the test now passes through the
        # exemption under either keying. Do not delete this keying as inert
        # (it is the rule's scope, defensive against any future blind item
        # arriving under another profile), and do not read it as test-pinned:
        # a test killing the fleet-wide mutant would need a state the
        # enumeration above shows unreachable.
        profile_phantoms = phantoms_by_profile.get(item.profile, set())
        held_elsewhere = sessions_by_profile.get(item.profile, set()) - {session}
        # The narrowed fold the docstring describes, two branches and two
        # sentences, each branch the refusal AND its reason in one computation
        # — the discipline ``attach_displaces_client`` states, applied to the
        # refusals this function does not delegate to ``approval_block_reason``.
        # (It used to delegate, with forged inputs, and the borrowed
        # per-session sentence was false on every *other* session the fold
        # reaches — CR6 round six, finding 1.)
        if session in profile_phantoms and bool(held_elsewhere):
            # The phantom itself, while any other session holds approvals.
            # Never exempted by an observed id: its answer travels under its
            # dead runtime id, and the gateway resolves the session before any
            # queue is touched (``_sess_nowait``, tui_gateway/server.py:2507-
            # 2509 — an exact lookup, "session not found" on a miss), so its
            # aim never gets to matter. Its sentence promises only the
            # unconditional age-out the round-eight split delivers.
            marked.append(
                replace(
                    item,
                    answerable=False,
                    blocked_reason=PHANTOM_APPROVAL_AGES_OUT,
                )
            )
            continue
        if bool(profile_phantoms - {session}) and not item.observed_request_id:
            # A blind sibling of a phantom: its bare ``approval.respond`` is a
            # head-pop, and the phantom's entry may head its session's queue.
            # An item with an observed gateway id falls through instead — the
            # round-seven narrowing: ``resolve_gateway_approval`` selects by
            # id and returns 0 on no match (tools/approval.py:2655-2658), so
            # an aimed answer cannot land on the phantom's entry.
            marked.append(
                replace(
                    item,
                    answerable=False,
                    blocked_reason=UNPLACEABLE_APPROVAL_ON_CONNECTION,
                )
            )
            continue
        ordered = len(feeds.get(session, set())) < 2
        reason = approval_block_reason(
            is_head=(
                not hidden and ordered and heads[session].identity == item.identity
            ),
            queued_count=counts[session] + hidden,
            observed_request_id=item.observed_request_id,
        )
        marked.append(
            replace(item, answerable=False, blocked_reason=reason) if reason else item
        )
    return tuple(marked)


# ── What could not be asked (R24, R10) ───────────────────────────────────


def connection_notices(
    *, profile: str, board: SeamBoard | None, channel: ConnectionChannel | None
) -> tuple[str, ...]:
    """Why this connection may be contributing less than the truth.

    One line per reason, each naming the connection, the missing capability, and
    the consequence *for the queue* — which is a different sentence from the seam
    line's consequence for the install, and the difference is the point: an
    operator reading the needs-you surface is asking "is anything waiting on me",
    and "we could not ask this gateway" is an answer to that question.

    A never-observed probe and an absent method are separate lines because they
    are separate facts (R24). A connection that is simply down gets its own,
    because a dropped source is neither.

    **A down connection is named whether or not it ever answered a roster poll**,
    and the two facts are two sentences rather than one silencing the other. This
    was guarded on ``last_poll_at is not None`` until 2026-08-17, on the reasoning
    that calling a connection's rows "stale" is meaningless when it has no rows —
    which is true of the *wording* and not of the *fact*. A connection that
    dropped before its first poll is the one contributing least of all, and it
    was the one saying nothing.
    """
    lines: list[str] = []
    if channel is not None and not channel.connected:
        if channel.last_poll_at is None:
            lines.append(
                f"{profile}: connection down before it was ever polled — nothing "
                "of this connection's is in the queue and nothing can be learned "
                "from it"
            )
        else:
            lines.append(
                f"{profile}: connection down — its rows are stale and no new item "
                "can be learned from it"
            )
    if board is None:
        return (
            *lines,
            f"{profile}: capabilities not probed — nothing is known about what "
            "this connection can answer, so its sessions are not in the queue",
        )
    lines.extend(_seam_notice(profile, board, "roster", _ROSTER_CONSEQUENCE))
    lines.extend(
        _seam_notice(profile, board, "approval-detail", _APPROVAL_DETAIL_CONSEQUENCE)
    )
    # **A present seam means "this gateway would answer", never "we asked".**
    # Nothing in production issues ``approval.pending`` as a data call — it is
    # registered as a presence probe only — so ``FleetState.approval_detail`` is
    # written by nothing in a live run and no foreign session's approval reaches
    # the queue. Until that changes, a probed-present seam silences the line
    # above and the silence reads as "everything of this connection's is in the
    # queue", which is R14's failure arriving through the door the seam was meant
    # to guard: not a queue that lost an item, but a queue that stopped saying it
    # had never looked.
    #
    # Stated unconditionally rather than gated on the seam, because the fact does
    # not depend on what the gateway can do: Talaria does not ask, whatever the
    # answer would have been. It is deliberately the last line, so a connection
    # that ALSO could not be probed says both things in the order they matter.
    #
    # **Delete this line when the poll lands, and not before.** It is not a
    # permanent caveat; it is the disclosure of a named, filed gap — the plan's
    # UNSLOTTED slice covering the KTD2 cadence, feed B's assembly, and AE2's
    # settle-and-latch. Pinned by
    # ``test_a_connection_says_its_foreign_approvals_are_unpolled_even_when_probed``.
    lines.append(f"{profile}: {_APPROVAL_DETAIL_UNPOLLED}")
    return tuple(lines)


_ROSTER_CONSEQUENCE: Final[str] = (
    "sessions this connection holds are not enumerated, so nothing of theirs "
    "reaches the queue"
)

_APPROVAL_DETAIL_CONSEQUENCE: Final[str] = (
    "approvals on this connection's foreign sessions are not fetched; a waiting "
    "row is shown without its prompt"
)

#: Said of every connection, whatever its seam probe found, for as long as
#: nothing calls ``approval.pending`` for data. See ``connection_notices``.
_APPROVAL_DETAIL_UNPOLLED: Final[str] = (
    "foreign approval detail is not polled on any connection — a session of "
    "someone else's that is waiting on an approval is not in this queue, whether "
    "or not this gateway would answer"
)


def _seam_notice(
    profile: str, board: SeamBoard, seam: str, consequence: str
) -> tuple[str, ...]:
    try:
        observation = board.observation_for(seam)
    except KeyError:  # pragma: no cover - a board built from the catalogue
        return ()
    if observation.status == "present":
        return ()
    if observation.status is None:
        return (f"{profile}: {seam} not probed yet — {consequence}",)
    return (f"{profile}: {seam} {observation.status} — {consequence}",)


def _all_connection_notices(
    *,
    channels: Mapping[str, ConnectionChannel],
    boards: Mapping[str, SeamBoard],
    focused_profile: str,
) -> tuple[str, ...]:
    """Every connection's notices, in a stable order.

    The union of the connections that have a channel, the connections that have
    a board, and the focused one — a connection that failed before it ever
    polled has neither, and it is exactly the connection whose silence must not
    be read as quiet.
    """
    profiles = sorted({*channels, *boards, focused_profile})
    lines: list[str] = []
    for profile in profiles:
        lines.extend(
            connection_notices(
                profile=profile,
                board=boards.get(profile),
                channel=channels.get(profile),
            )
        )
    return tuple(lines)


# ── Rendering helpers the surface reuses (U7 owns the widgets) ────────────


def format_age(seconds: float) -> str:
    """Whole seconds from the frame clock, so a replay renders identically."""
    return f"{max(0.0, seconds):.0f}s"


def wait_line(item: QueueItem, clock: float) -> str:
    """One item's age, saying which kind of age it is (KTD12, R18, R20).

    ``requested`` is an answer on the wire — R18's requested-with-age — and it
    reports the age of the *request*, not of the wait, because that is the number
    that tells the operator whether to worry. ``waiting ≥`` is a floor: Talaria
    saw the wait already in progress and no start stamp exists anywhere on the
    wire, so the span since the first sighting is all that can be claimed.
    ``waiting`` plain is a wait Talaria watched begin.

    **``stale_since`` is rendered here, and until U7 it was rendered nowhere.**
    The field was written by three call sites in this module and read by none,
    so a queue item whose connection dropped rendered exactly like a live one:
    an age that kept counting up off a clock that had stopped watching. Two
    corrections, both mechanical. Every age is measured **to the moment the
    stream broke** rather than to now, because nothing after that moment was
    observed; and the wait becomes a floor whatever kind of stamp it started as,
    because a wait Talaria watched begin and then stopped watching is a wait
    known only to have lasted *at least* that long — it may have been answered
    by someone at the gateway a second later. The blind span is then stated as
    its own number, so the operator can see how much of the silence is Talaria's
    rather than the session's. Pinned by
    ``test_a_polled_item_of_a_disconnected_connection_says_it_is_stale``.
    """
    # Nothing after the break was observed, so every span below is measured to
    # the break and never to ``clock``.
    observed_to = clock if item.stale_since is None else item.stale_since
    if item.requested:
        if item.requested_at is None:
            line = "requested, age not observed"
        else:
            line = f"requested {format_age(observed_to - item.requested_at)} ago"
    else:
        span = format_age(observed_to - item.opened_at)
        floor = item.age_is_floor or item.stale_since is not None
        line = f"waiting ≥ {span}" if floor else f"waiting {span}"
    if item.stale_since is None:
        return line
    return f"{line}, unobserved for {format_age(clock - item.stale_since)}"


#: What the summary says when nothing is waiting and every connection answered.
NEEDS_YOU_NONE: Final[str] = "needs-you: none"


def summary_line(queue: NeedsYouQueue, clock: float) -> str:
    """R16's glanceable summary: how many wait, and the oldest one's age and session.

    **The empty state is not unconditional.** "none" with a connection that could
    not be asked would be the one wrong answer this surface can give, so the
    count of unanswered connections rides along with it — see
    :class:`NeedsYouQueue`.
    """
    blind = len(queue.notices)
    if queue.is_empty:
        if not blind:
            return NEEDS_YOU_NONE
        plural = "notice" if blind == 1 else "notices"
        return f"{NEEDS_YOU_NONE} seen · {blind} {plural}: part of the fleet could not be asked"
    oldest = queue.items[0]
    title = oldest.session_title or oldest.session_id or "an unnamed session"
    line = f"needs-you: {queue.count} · {wait_line(oldest, clock)} · {oldest.source} · {title}"
    if blind:
        line = f"{line} (+{blind} unanswered)"
    return line


__all__ = [
    "ANSWER_ALREADY_TRAVELLING",
    "APPROVAL_DETAIL_TRIGGER_STATUSES",
    "NEEDS_YOU_NONE",
    "PHANTOM_APPROVAL_AGES_OUT",
    "QUEUEABLE_KINDS",
    "QUEUED_BEHIND_APPROVAL",
    "REFUSED_APPROVAL_NOT_HEAD",
    "REFUSED_UNCORRELATED_APPROVAL",
    "ROSTER_ITEM_NOT_ANSWERABLE",
    "ROSTER_REQUEST_KEY",
    "SOURCE_APPROVAL_POLL",
    "SOURCE_DRIVEN",
    "SOURCE_ROSTER",
    "UNCORRELATED_APPROVAL",
    "UNOBSERVED_KIND",
    "UNPLACEABLE_APPROVAL_ON_CONNECTION",
    "UNRESOLVABLE_KINDS",
    "ItemKey",
    "NeedsYouQueue",
    "PendingApprovalDirectory",
    "PolledApproval",
    "approval_detail_due",
    "build_queue",
    "connection_notices",
    "decode_pending_approvals",
    "format_age",
    "merge_polled_approvals",
    "prompt_feed_rows",
    "summary_line",
    "unresolvable_kind_notice",
    "unresolvable_kind_of",
    "wait_line",
]
