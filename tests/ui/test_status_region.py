"""R22's rendering half: multi-row literal output, ANSI shown and not obeyed.

U6 owns the runner and its failure taxonomy; this suite owns what reaches the
screen. The distinction matters because the dangerous half is here: a runner
that correctly captures ``\\x1b[2J`` and a region that faithfully forwards it to
the terminal together produce a status command that can clear the screen.
"""

from __future__ import annotations

import pytest

from talaria.status.contract import TRUNCATION_MARKER
from talaria.status.runner import StatusTickResult
from talaria.ui.literal import defang
from tests.ui.conftest import event, paused_app


@pytest.mark.asyncio
async def test_a_multi_row_payload_renders_one_row_per_line() -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:
        await app.status_region.apply(
            StatusTickResult(outcome="ok", rows=("branch: main", "tests: 296", "wip: 2"))
        )
        await pilot.pause()
        assert app.status_region.row_texts == ("branch: main", "tests: 296", "wip: 2")
        assert app.status_region.marker_text == ""
        await app.shutdown_sources()


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("\x1b[2Jcleared?", "␛[2Jcleared?"),
        ("\x1b]0;retitled\x07", "␛]0;retitled␇"),
        ("carriage\rreturn", "carriage␍return"),
        ("bell\x07", "bell␇"),
        ("keeps\ttabs", "keeps\ttabs"),
    ],
)
@pytest.mark.asyncio
async def test_escape_sequences_are_shown_not_obeyed(raw: str, expected: str) -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:
        await app.status_region.apply(StatusTickResult(outcome="ok", rows=(raw,)))
        await pilot.pause()
        assert app.status_region.row_texts == (expected,)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_rich_markup_is_not_parsed() -> None:
    """A status command printing ``[red]`` gets those six characters on screen."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:
        await app.status_region.apply(
            StatusTickResult(outcome="ok", rows=("[red]not a colour[/red]",))
        )
        await pilot.pause()
        assert app.status_region.row_texts == ("[red]not a colour[/red]",)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_truncation_is_visible_rather_than_silent() -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:
        await app.status_region.apply(
            StatusTickResult(outcome="ok", rows=tuple(f"row {n}" for n in range(8)), truncated=True)
        )
        await pilot.pause()
        assert app.status_region.row_texts[-1] == TRUNCATION_MARKER
        await app.shutdown_sources()


@pytest.mark.parametrize(
    "outcome",
    ["nonzero_exit", "timeout", "missing_executable", "empty_output", "invalid_output"],
)
@pytest.mark.asyncio
async def test_every_failure_shows_its_categorical_marker(outcome: str) -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:
        await app.status_region.apply(
            StatusTickResult(outcome=outcome, marker=f"status: {outcome}")  # type: ignore[arg-type]
        )
        await pilot.pause()
        assert app.status_region.marker_text == f"status: {outcome}"
        assert app.status_region.row_texts == ()
        await app.shutdown_sources()


def test_defang_leaves_ordinary_text_alone() -> None:
    assert defang("branch: main · 3 ahead") == "branch: main · 3 ahead"
    assert defang("端末 é עברית 🜁") == "端末 é עברית 🜁"


# ── U3: the caret slot (R5, KTD5) ─────────────────────────────────────────
#
# talaria/ui/app.py wires focus changes to StatusRegion.set_caret through
# _refresh_caret_slot; tests/ui/test_focus_returns.py covers that wiring end
# to end (tabbing into the transcript, the F1 jump). This section is what
# CR5 found missing: the slot's OWN two properties, independent of how the
# caret got there — that it survives a status tick sharing the region with
# it, and that naming the caret never moves a single row of anything else.


@pytest.mark.asyncio
async def test_the_caret_word_survives_a_status_tick_that_also_fails() -> None:
    """The caret slot is a dedicated ``Static``, deliberately never the
    shared ``.status--marker`` one — that Static is overwritten by every
    tick (KTD5's own reason for the split, see ``StatusRegion``'s
    docstring). A caret word set before a failing tick must still be there
    after it, and the failure marker must still be shown beside it.
    """
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:
        # Mount-time focus settles onto the composer asynchronously; waited
        # out first so that settling does not fire afterward and clobber the
        # caret word this test sets by hand.
        await pilot.pause()
        app.status_region.set_caret("transcript")
        await pilot.pause()
        assert app.status_region.caret_text == "caret: transcript"

        await app.status_region.apply(StatusTickResult(outcome="timeout", marker="status: timeout"))
        await pilot.pause()

        assert app.status_region.marker_text == "status: timeout"
        assert app.status_region.caret_text == "caret: transcript"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_status_region_geometry_is_invariant_across_caret_states() -> None:
    """R5: the caret slot is mounted unconditionally at a fixed height, so
    writing into it must never move a single row of the rest of the
    interface. Checked against every region's own geometry rather than
    screen height alone, which cannot see ``#body``'s rows move under it —
    the same falsifier ``talaria/ui/composer.py:181-189``'s regression
    needed, applied here to the caret slot instead of focus.
    """
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:

        def regions() -> dict[str, object]:
            return {
                "status": app.status_region.region,
                "body": app.query_one("#body").region,
                "prompts": app.prompts.region,
                "transcript": app.transcript.region,
                "composer": app.composer.region,
            }

        await pilot.pause()
        baseline = regions()

        app.status_region.set_caret("transcript")
        await pilot.pause()
        assert regions() == baseline, "naming the transcript moved a region"

        app.status_region.set_caret("prompts")
        await pilot.pause()
        assert regions() == baseline, "naming the prompts region moved a region"

        # The R5 falsifier: a caret word beside a failure marker, the one
        # state where the region is showing the most it ever shows at once.
        await app.status_region.apply(StatusTickResult(outcome="timeout", marker="status: timeout"))
        await pilot.pause()
        assert regions() == baseline, "a failure marker beside the caret word moved a region"

        app.status_region.set_caret("")
        await pilot.pause()
        assert regions() == baseline, "clearing the caret slot moved a region"
        await app.shutdown_sources()
