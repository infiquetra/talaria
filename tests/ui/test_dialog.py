"""The modal picker dialog: what each key does, and what ends up on screen.

The selection *rules* are asserted without a terminal in
``tests/domain/test_selection.py``. What is left for this file is the part that
genuinely needs a screen: that a key press reaches the right transition, that
the highlight the operator can see matches the one the model holds, that a
stage stack pushes and pops, and that dismissing returns the right payload.

The dialog is driven through a minimal host app rather than the real
:class:`~talaria.ui.app.TalariaApp`, so a failure here is a failure in the
dialog and not in six hundred lines of wiring. ``tests/ui/test_picker.py``
covers the wiring separately, against the real app.
"""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult
from textual.widgets import Static

from talaria.domain.selection import Choice, Selection, Stage
from talaria.ui.dialog import NO_MATCH, REFUSED_PREFIX, PickerDialog

# ── a source with two levels, one unselectable row, and a long list ───────

PROVIDERS = (
    Choice(key="alpha", label="Alpha", detail="(2 models)", marked=True),
    Choice(
        key="cold",
        label="Cold",
        detail="(1 model)",
        selectable=False,
        refusal="Cold is not authenticated",
    ),
    Choice(key="beta", label="Beta", detail="(3 models)"),
)

#: ``number`` is deliberately not the row's position on its stage — beta's
#: models are positions one through three and rows 41-43 of the listing the
#: composer's ``/models <n>`` takes. Conflating the two is the defect these
#: fixtures exist to keep caught.
MODELS = {
    "alpha": (
        Choice(key="alpha:one", label="one", payload="1", number=1),
        Choice(key="alpha:two", label="two", payload="2", number=2, marked=True),
    ),
    "beta": tuple(
        Choice(key=f"beta:{n}", label=f"b{n}", payload=str(n + 41), number=n + 41)
        for n in range(3)
    ),
}


#: A second provider listing whose marked row is deliberately *not* first, so a
#: test of "opens on the marked row" cannot pass by opening at the top.
LATE_MARK = (
    Choice(key="first", label="First"),
    Choice(key="second", label="Second"),
    Choice(key="third", label="Third", marked=True),
)


class TwoLevelSource:
    """Providers, then that provider's models — the shape ``/models`` uses.

    Builds both levels with :meth:`Selection.opened` because the real sources
    in ``talaria/ui/picker.py`` do; a stand-in that constructed a plain
    ``Selection`` would quietly stop exercising the opening rule.
    """

    def __init__(self, providers: tuple[Choice, ...] = PROVIDERS) -> None:
        self._providers = providers

    def root(self) -> Stage:
        return Stage(title="choose a provider", selection=Selection.opened(self._providers))

    def descend(self, depth: int, choice: Choice) -> Stage | str:
        if depth > 0:
            return choice.payload
        return Stage(
            title=f"models — {choice.label}",
            selection=Selection.opened(MODELS[choice.key]),
        )


class FlatSource:
    """One level, the shape ``/profiles`` uses."""

    def __init__(self, items: tuple[Choice, ...]) -> None:
        self._items = items

    def root(self) -> Stage:
        return Stage(title="choose one", selection=Selection.opened(self._items))

    def descend(self, depth: int, choice: Choice) -> Stage | str:
        return choice.payload


class Host(App[None]):
    """A bare app whose only job is to hold the dialog and keep its result."""

    def __init__(self, source: object) -> None:
        super().__init__()
        self._source = source
        self.result: str | None = None
        self.dismissed = False

    def compose(self) -> ComposeResult:
        yield Static("behind the dialog")

    def on_mount(self) -> None:
        def done(chosen: str | None) -> None:
            self.result = chosen
            self.dismissed = True

        self.push_screen(PickerDialog(self._source), done)  # type: ignore[arg-type]


def picker(app: Host) -> PickerDialog:
    screen = app.screen
    assert isinstance(screen, PickerDialog)
    return screen


# ── opening ───────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_the_dialog_opens_on_the_roots_rows_with_the_marked_one_first() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        dialog = picker(app)
        assert dialog.title_text == "choose a provider"
        assert len(dialog.row_texts) == 3
        assert "Alpha" in dialog.active_row_text
        assert dialog.depth == 0


@pytest.mark.asyncio
async def test_a_row_with_no_number_of_its_own_is_drawn_without_one() -> None:
    """Providers have no ``/models <n>`` number, so numbering them would invent one."""
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        rows = picker(app).row_texts
        assert rows[0] == "*Alpha  (2 models)"
        assert rows[1].strip().startswith("Cold")
        assert not any(row.lstrip("* ").startswith(("1.", "2.", "3.")) for row in rows)


@pytest.mark.asyncio
async def test_a_numbered_row_is_drawn_with_the_number_its_own_surface_takes() -> None:
    """Not its position on the stage — the two differ past the first provider."""
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("end")
        await pilot.press("enter")
        await pilot.pause()
        rows = picker(app).row_texts
        # Beta's models are rows 41-43 of the listing and positions 1-3 here.
        assert [row.strip() for row in rows] == ["41. b0", "42. b1", "43. b2"]


# ── navigation, and the highlight the operator can actually see ───────────


@pytest.mark.asyncio
async def test_the_arrows_move_the_visible_highlight() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        assert "Alpha" in picker(app).active_row_text

        await pilot.press("down")
        await pilot.pause()
        assert "Cold" in picker(app).active_row_text

        await pilot.press("up")
        await pilot.pause()
        assert "Alpha" in picker(app).active_row_text


@pytest.mark.asyncio
async def test_ctrl_p_is_not_a_movement_key_because_textual_owns_it() -> None:
    """The reason movement is arrows-only, asserted rather than remembered.

    ``ctrl+p`` is Textual's ``COMMAND_PALETTE_BINDING`` and opens its own
    palette over the dialog. Binding it to "move up" would have shipped a key
    that works in a unit test and not on a real terminal, so this asserts the
    collision is real and that the dialog does not pretend to handle it.
    """
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        assert "Alpha" in picker(app).active_row_text

        await pilot.press("ctrl+p")
        await pilot.pause()
        # Textual's palette took it; the picker is no longer the top screen.
        assert not isinstance(app.screen, PickerDialog)


@pytest.mark.asyncio
async def test_home_and_end_reach_the_ends_of_the_list() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("end")
        await pilot.pause()
        assert "Beta" in picker(app).active_row_text
        await pilot.press("home")
        await pilot.pause()
        assert "Alpha" in picker(app).active_row_text


@pytest.mark.asyncio
async def test_the_highlight_the_screen_shows_matches_the_one_the_model_holds() -> None:
    """The two disagreeing is exactly the defect this assertion is here for."""
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("down")
        await pilot.pause()
        dialog = picker(app)
        held = dialog.selection.active_choice
        assert held is not None
        assert held.label in dialog.active_row_text


# ── typing filters ────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_typing_filters_the_rows_and_shows_what_was_typed() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.press("b")
        await pilot.pause()
        dialog = picker(app)
        assert "b" in dialog.filter_text
        assert all("Alpha" not in row for row in dialog.row_texts)
        assert any("Beta" in row for row in dialog.row_texts)


@pytest.mark.asyncio
async def test_a_filter_matching_nothing_says_so_rather_than_going_blank() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.press("z")
        await pilot.press("z")
        await pilot.pause()
        assert picker(app).row_texts == (NO_MATCH,)


@pytest.mark.asyncio
async def test_backspace_widens_the_filter_again() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.press("b")
        await pilot.press("z")
        await pilot.pause()
        assert picker(app).row_texts == (NO_MATCH,)
        await pilot.press("backspace")
        await pilot.pause()
        assert any("Beta" in row for row in picker(app).row_texts)


@pytest.mark.asyncio
async def test_ctrl_u_drops_the_whole_filter() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.press("b")
        await pilot.press("e")
        await pilot.pause()
        await pilot.press("ctrl+u")
        await pilot.pause()
        dialog = picker(app)
        assert dialog.filter_text == ""
        assert len(dialog.row_texts) == 3


@pytest.mark.asyncio
async def test_a_letter_that_would_be_a_shortcut_elsewhere_goes_into_the_filter() -> None:
    """``q`` filters rather than quitting — Hermes makes the same call.

    A picker that swallows letters as commands cannot be filtered with them,
    and the operator has no way to know which letters are safe to type.
    """
    app = Host(FlatSource((Choice(key="q1", label="quick", payload="1"),)))
    async with app.run_test() as pilot:
        await pilot.press("q")
        await pilot.pause()
        assert picker(app).filter_text.endswith("q")
        assert app.dismissed is False


# ── the stage stack ───────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_enter_descends_into_the_next_level() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("enter")
        await pilot.pause()
        dialog = picker(app)
        assert dialog.depth == 1
        assert dialog.title_text == "models — Alpha"
        assert [row.strip().split(". ")[-1] for row in dialog.row_texts] == ["one", "two"]


@pytest.mark.asyncio
async def test_escape_pops_a_stage_before_it_closes_the_dialog() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("enter")
        await pilot.pause()
        assert picker(app).depth == 1

        await pilot.press("escape")
        await pilot.pause()
        assert picker(app).depth == 0
        assert app.dismissed is False

        await pilot.press("escape")
        await pilot.pause()
        assert app.dismissed is True
        assert app.result is None


@pytest.mark.asyncio
async def test_escape_clears_a_filter_before_it_pops_a_stage() -> None:
    """Each layer of escape undoes the most recent thing the operator did."""
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("enter")
        await pilot.press("t")
        await pilot.pause()
        assert picker(app).filter_text != ""

        await pilot.press("escape")
        await pilot.pause()
        dialog = picker(app)
        assert dialog.filter_text == ""
        # Still one level down: the filter absorbed this escape, not the stack.
        assert dialog.depth == 1


@pytest.mark.asyncio
async def test_the_highlight_survives_going_down_a_level_and_back_up() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("end")
        await pilot.pause()
        assert "Beta" in picker(app).active_row_text

        await pilot.press("enter")
        await pilot.pause()
        await pilot.press("escape")
        await pilot.pause()
        assert "Beta" in picker(app).active_row_text


# ── the horizontal arrows, which are second names for select and back ─────


@pytest.mark.asyncio
async def test_right_descends_a_level_exactly_as_enter_does() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("right")
        await pilot.pause()
        dialog = picker(app)
        assert dialog.depth == 1
        assert dialog.title_text == "models — Alpha"


@pytest.mark.asyncio
async def test_left_pops_a_stage_exactly_as_escape_does() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("right")
        await pilot.pause()
        assert picker(app).depth == 1

        await pilot.press("left")
        await pilot.pause()
        assert picker(app).depth == 0
        assert app.dismissed is False

        await pilot.press("left")
        await pilot.pause()
        assert app.dismissed is True
        assert app.result is None


@pytest.mark.asyncio
async def test_left_clears_a_filter_before_it_pops_a_stage() -> None:
    """``left`` is the same layered back that ``escape`` is, not a shortcut past it."""
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("right")
        await pilot.press("t")
        await pilot.pause()
        assert picker(app).filter_text != ""

        await pilot.press("left")
        await pilot.pause()
        dialog = picker(app)
        assert dialog.filter_text == ""
        assert dialog.depth == 1


@pytest.mark.asyncio
async def test_right_on_a_final_row_dismisses_with_the_payload_enter_would_send() -> None:
    app = Host(FlatSource((Choice(key="p", label="Prof", payload="7"),)))
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("right")
        await pilot.pause()
        assert app.dismissed is True
        assert app.result == "7"


@pytest.mark.asyncio
async def test_right_on_an_unselectable_row_refuses_exactly_as_enter_does() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("down")
        await pilot.press("right")
        await pilot.pause()
        dialog = picker(app)
        assert dialog.refusal_text.startswith(REFUSED_PREFIX)
        assert "not authenticated" in dialog.refusal_text
        assert app.dismissed is False


# ── opening on the row already in use ─────────────────────────────────────


@pytest.mark.asyncio
async def test_the_dialog_opens_on_the_marked_row_not_at_the_top() -> None:
    """The operator's report: re-opening ``/models`` did not land on the current model."""
    app = Host(TwoLevelSource(LATE_MARK))
    async with app.run_test() as pilot:
        await pilot.pause()
        assert "Third" in picker(app).active_row_text


@pytest.mark.asyncio
async def test_the_second_level_opens_on_its_marked_row_too() -> None:
    """Further than Hermes goes — its ``modelIdx`` resets to zero on descent."""
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("enter")
        await pilot.pause()
        dialog = picker(app)
        assert dialog.depth == 1
        assert dialog.active_row_text.strip().endswith("two")


# ── selection and refusal ─────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_choosing_a_leaf_dismisses_with_that_rows_payload() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("enter")  # into Alpha
        await pilot.press("down")  # onto "two"
        await pilot.press("enter")
        await pilot.pause()
        assert app.dismissed is True
        assert app.result == "2"


@pytest.mark.asyncio
async def test_enter_on_an_unselectable_row_names_the_reason_and_stays_open() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("down")  # onto the unauthenticated provider
        await pilot.press("enter")
        await pilot.pause()
        dialog = picker(app)
        assert dialog.refusal_text == f"{REFUSED_PREFIX}Cold is not authenticated"
        assert dialog.depth == 0
        assert app.dismissed is False


@pytest.mark.asyncio
async def test_the_refusal_clears_as_soon_as_the_operator_moves_on() -> None:
    """A stale refusal pointing at a row the operator has left is a lie."""
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("down")
        await pilot.press("enter")
        await pilot.pause()
        assert picker(app).refusal_text != ""

        await pilot.press("down")
        await pilot.pause()
        assert picker(app).refusal_text == ""


@pytest.mark.asyncio
async def test_enter_on_a_flat_source_dismisses_immediately() -> None:
    app = Host(FlatSource((Choice(key="a", label="alpha", payload="7"),)))
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("enter")
        await pilot.pause()
        assert app.result == "7"


@pytest.mark.asyncio
async def test_enter_with_nothing_matching_the_filter_does_nothing() -> None:
    app = Host(TwoLevelSource())
    async with app.run_test() as pilot:
        await pilot.press("z")
        await pilot.press("z")
        await pilot.pause()
        await pilot.press("enter")
        await pilot.pause()
        assert app.dismissed is False
        assert picker(app).row_texts == (NO_MATCH,)


# ── the long-list case the window exists for ──────────────────────────────


@pytest.mark.asyncio
async def test_a_list_longer_than_the_window_scrolls_the_highlight_into_view() -> None:
    many = tuple(Choice(key=f"k{i}", label=f"row{i:02d}", payload=str(i)) for i in range(40))
    app = Host(FlatSource(many))
    async with app.run_test() as pilot:
        await pilot.pause()
        assert len(picker(app).row_texts) < len(many)

        await pilot.press("end")
        await pilot.pause()
        dialog = picker(app)
        # The last row is both on screen and the one highlighted.
        assert "row39" in dialog.active_row_text
        assert any("row39" in row for row in dialog.row_texts)


@pytest.mark.asyncio
async def test_pagedown_moves_a_windowful_at_a_time() -> None:
    many = tuple(Choice(key=f"k{i}", label=f"row{i:02d}", payload=str(i)) for i in range(40))
    app = Host(FlatSource(many))
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("pagedown")
        await pilot.pause()
        moved = picker(app).selection.active
        assert moved > 1
        await pilot.press("pageup")
        await pilot.pause()
        assert picker(app).selection.active == 0
