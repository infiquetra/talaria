"""The picker's selection model, asserted without a screen.

That these run with no terminal at all is the point of splitting the state
machine out of the widget: every navigation, filter and restock rule below is
checkable as a value comparison. The dialog's own tests
(``tests/ui/test_dialog.py``) then only have to prove that key presses reach
these transitions and that the result is drawn.
"""

from __future__ import annotations

import pytest

from talaria.domain.selection import (
    VISIBLE_ROWS,
    Choice,
    Selection,
    subsequence_score,
)

MODELS = ("gpt-oss:20b", "qwen3.6:latest", "gemma3:27b", "deepseek-coder-v2")


def rows(*labels: str) -> tuple[Choice, ...]:
    return tuple(Choice(key=label, label=label, payload=label) for label in labels)


def listing() -> Selection:
    return Selection(items=rows(*MODELS))


# ── the unselectable guard ────────────────────────────────────────────────


def test_an_unselectable_choice_must_carry_a_reason() -> None:
    """The failure this module exists to prevent, asserted at construction.

    An unselectable row with nothing to say is a highlight that lands, an
    enter that does nothing, and no way for the operator to find out why.
    """
    with pytest.raises(ValueError, match="carries no refusal reason"):
        Choice(key="p", label="Provider", selectable=False)

    allowed = Choice(key="p", label="Provider", selectable=False, refusal="it is not authenticated")
    assert allowed.refusal


# ── where the list opens ──────────────────────────────────────────────────


def test_opened_starts_on_the_marked_row_rather_than_the_top() -> None:
    """The rule behind the operator's report that the picker forgot their model."""
    items = (
        Choice(key="a", label="Alpha"),
        Choice(key="b", label="Beta", marked=True),
        Choice(key="c", label="Gamma"),
    )
    assert Selection.opened(items).active == 1


def test_opened_falls_back_to_the_top_when_nothing_is_marked() -> None:
    assert Selection.opened(rows(*MODELS)).active == 0
    assert Selection.opened(()).active == 0


def test_opened_takes_the_first_mark_when_a_list_carries_more_than_one() -> None:
    """Not reachable through the real sources, and defined anyway.

    A list with two marks means two rows claiming to be current, which is a
    caller bug. Picking the first is arbitrary but it is *decided*: the
    alternative is an ordering-dependent answer that changes when an unrelated
    listing is re-sorted.
    """
    items = (
        Choice(key="a", label="Alpha"),
        Choice(key="b", label="Beta", marked=True),
        Choice(key="c", label="Gamma", marked=True),
    )
    assert Selection.opened(items).active == 1


# ── ranking ───────────────────────────────────────────────────────────────


def test_the_filter_matches_a_subsequence_not_only_a_substring() -> None:
    """``gpt20`` finds ``gpt-oss:20b`` — an operator types memorable characters."""
    assert subsequence_score("gpt-oss:20b", "gpt20") is not None
    assert subsequence_score("gpt-oss:20b", "gpt20b") is not None
    assert subsequence_score("gemma3:27b", "gpt20") is None


def test_a_tighter_match_outranks_a_scattered_one() -> None:
    tight = subsequence_score("abcdef", "abc")
    scattered = subsequence_score("axbxcx", "abc")
    assert tight is not None and scattered is not None
    assert tight < scattered


def test_filtering_orders_by_score_then_by_listing_order() -> None:
    selection = Selection(items=rows("alpha", "beta", "alpaca"))
    filtered = selection.typed("a").typed("l").visible
    # Both "alpha" and "alpaca" match "al"; the tighter span wins, and listing
    # order breaks any remaining tie rather than an arbitrary sort.
    assert [c.label for c in filtered] == ["alpha", "alpaca"]


def test_an_empty_filter_admits_every_row_in_listing_order() -> None:
    assert [c.label for c in listing().visible] == list(MODELS)


# ── navigation ────────────────────────────────────────────────────────────


def test_movement_stops_at_the_ends_rather_than_wrapping() -> None:
    """Not wrapping is deliberate: the operator can feel where the list ends."""
    selection = listing()
    assert selection.move(-1).active == 0
    assert selection.move(len(MODELS) * 2).active == len(MODELS) - 1


def test_movement_steps_one_row_at_a_time() -> None:
    selection = listing()
    assert selection.move(1).active == 1
    assert selection.move(1).move(1).active == 2
    assert selection.move(1).move(1).move(-1).active == 1


def test_navigation_visits_unselectable_rows_rather_than_skipping_them() -> None:
    """The documented disagreement with Qwen Code's ``findNextValidIndex``.

    Skipping would make an unauthenticated provider unreachable *and*
    unexplained. Landing on it is what lets the dialog say why enter refuses.
    """
    items = (
        Choice(key="a", label="Alpha"),
        Choice(key="b", label="Beta", selectable=False, refusal="not authenticated"),
        Choice(key="c", label="Gamma"),
    )
    selection = Selection(items=items)
    landed = selection.move(1).active_choice
    assert landed is not None
    assert landed.key == "b"
    assert landed.selectable is False


def test_to_index_ignores_a_row_number_outside_the_list() -> None:
    selection = listing()
    assert selection.to_index(2).active == 2
    assert selection.to_index(99).active == 0
    assert selection.to_index(-1).active == 0


# ── the filter's effect on the highlight ──────────────────────────────────


def test_typing_resets_the_highlight_because_the_ranking_changed() -> None:
    """Leaving it put would point it at a row that now means something else."""
    selection = listing().move(3)
    assert selection.active == 3
    assert selection.typed("e").active == 0


def test_backspace_shortens_the_filter() -> None:
    selection = listing().typed("g").typed("p")
    assert selection.filter_text == "gp"
    assert selection.backspaced().filter_text == "g"


def test_clearing_the_filter_keeps_the_highlighted_row_highlighted() -> None:
    """Hermes's ``providerIndexAfterClearingFilter``, and worth the bookkeeping.

    The operator filtered *to* find this row; dropping them at the top of the
    full list discards the work that got them there.
    """
    narrowed = listing().typed("g").typed("e").typed("m")
    held = narrowed.active_choice
    assert held is not None and held.label == "gemma3:27b"

    widened = narrowed.cleared()
    assert widened.filter_text == ""
    assert widened.active_choice is not None
    assert widened.active_choice.label == "gemma3:27b"
    # ...and it is genuinely the full list again, not a still-filtered one.
    assert len(widened.visible) == len(MODELS)


def test_a_filter_matching_nothing_reports_empty_rather_than_a_stale_row() -> None:
    selection = listing().typed("z").typed("z").typed("z")
    assert selection.empty is True
    assert selection.active_choice is None
    # Clearing recovers, and does not crash on the held-row lookup.
    assert selection.cleared().empty is False


# ── restocking ────────────────────────────────────────────────────────────


def test_restocking_keeps_the_highlight_on_the_same_row() -> None:
    """A refetch on reconnect must not move an operator who has scrolled."""
    selection = listing().move(2)
    held = selection.active_choice
    assert held is not None

    reordered = rows(*reversed(MODELS))
    assert selection.restocked(reordered).active_choice == Choice(
        key=held.key, label=held.label, payload=held.payload
    )


def test_restocking_a_genuinely_new_list_falls_back_to_the_marked_row() -> None:
    selection = listing().move(2)
    fresh = (
        Choice(key="x", label="x"),
        Choice(key="y", label="y", marked=True),
        Choice(key="z", label="z"),
    )
    assert selection.restocked(fresh).active == 1


def test_restocking_with_neither_the_held_nor_a_marked_row_opens_at_the_top() -> None:
    selection = listing().move(2)
    fresh = rows("x", "y", "z")
    assert selection.restocked(fresh).active == 0


def test_restocking_an_empty_list_leaves_nothing_highlighted() -> None:
    assert listing().move(2).restocked(()).active_choice is None


# ── the scrolling window ──────────────────────────────────────────────────


def test_the_window_centres_the_highlight_once_past_the_middle() -> None:
    many = Selection(items=rows(*[f"m{i}" for i in range(40)]))
    _, top_offset = many.window()
    assert top_offset == 0

    shown, offset = many.move(20).window()
    assert len(shown) == VISIBLE_ROWS
    assert offset == 20 - VISIBLE_ROWS // 2
    # The highlighted row is inside the slice that was returned.
    assert shown[20 - offset].label == "m20"


def test_the_window_stops_at_the_end_rather_than_scrolling_past_it() -> None:
    many = Selection(items=rows(*[f"m{i}" for i in range(40)]))
    shown, offset = many.move(39).window()
    assert offset == 40 - VISIBLE_ROWS
    assert len(shown) == VISIBLE_ROWS
    assert shown[-1].label == "m39"


def test_a_list_shorter_than_the_window_is_shown_whole() -> None:
    shown, offset = listing().window()
    assert offset == 0
    assert len(shown) == len(MODELS)


# ── immutability ──────────────────────────────────────────────────────────


def test_every_transition_returns_a_new_selection_and_mutates_nothing() -> None:
    """A test can hold before and after side by side because nothing is shared."""
    before = listing().move(1)
    after = before.move(1).typed("g").backspaced().cleared()
    assert before.active == 1
    assert before.filter_text == ""
    assert after is not before
