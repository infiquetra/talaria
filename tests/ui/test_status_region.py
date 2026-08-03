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
