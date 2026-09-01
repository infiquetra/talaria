# ruff: noqa: E501
"""R22's rendering half: multi-row literal output, ANSI shown and not obeyed.

U6 owns the runner and its failure taxonomy; this suite owns what reaches the
screen. The distinction matters because the dangerous half is here: a runner
that correctly captures ``\\x1b[2J`` and a region that faithfully forwards it to
the terminal together produce a status command that can clear the screen.
"""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult

from talaria.status.contract import TRUNCATION_MARKER
from talaria.status.runner import StatusTickResult
from talaria.ui.literal import defang
from talaria.ui.status_region import StatusRegion
from talaria.ui.theme import BUILTIN_THEME_REGISTRY
from tests.ui.conftest import event, paused_app


class _StatusRegionHarness(App[None]):
    def __init__(self, initial_marker: str) -> None:
        super().__init__()
        BUILTIN_THEME_REGISTRY.register(self)
        self.theme = "refined-default"
        self._initial_marker = initial_marker

    def compose(self) -> ComposeResult:
        yield StatusRegion(initial_marker=self._initial_marker, id="status")

    @property
    def status_region(self) -> StatusRegion:
        return self.query_one("#status", StatusRegion)


@pytest.mark.asyncio
async def test_a_malformed_command_notice_is_visible_without_a_status_runner() -> None:
    notice = "status.command has invalid quoting; the status command is disabled"
    app = _StatusRegionHarness(notice)

    async with app.run_test(size=(100, 20)) as pilot:
        await pilot.pause()
        assert app.status_region.marker_text == f"[x] {notice}"


@pytest.mark.asyncio
async def test_running_app_routes_status_configuration_notices_to_status_region() -> None:
    notice = "status.command has invalid quoting; the status command is disabled"
    app, _ = paused_app(
        [event("gateway.ready", {})],
        startup_notices=(notice, "theme fallback stayed in the startup transcript"),
    )

    async with app.run_test(size=(100, 24)) as pilot:
        await pilot.pause()
        assert app.status_region.marker_text == f"[x] {notice}"


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
        assert app.status_region.row_texts[-1] == (
            f"[!] status truncated — {TRUNCATION_MARKER}"
        )
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
        assert app.status_region.marker_text == f"[x] status: {outcome}"
        assert app.status_region.row_texts == ()
        await app.shutdown_sources()


def test_defang_leaves_ordinary_text_alone() -> None:
    assert defang("branch: main · 3 ahead") == "branch: main · 3 ahead"
    assert defang("端末 é עברית 🜁") == "端末 é עברית 🜁"


# ── U6: the dedicated caret row is always mounted ────────────────────────


@pytest.mark.asyncio
async def test_status_region_geometry_is_invariant_across_focus_states() -> None:
    """Focus changes only repaint the dedicated row and never move a region."""
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
        baseline_empty = regions()

        # Focus moves must not change geometry when shell status output is empty.
        await pilot.press("tab")
        await pilot.pause()
        assert regions() == baseline_empty, "focus in the transcript moved a region"

        await pilot.press("tab")
        await pilot.pause()
        assert regions() == baseline_empty, "focus in the prompts container moved a region"

        app.composer.text_area.focus()
        await pilot.pause()
        assert regions() == baseline_empty, "returning focus to the composer moved a region"

        # Now with status rows showing — the same focus moves must still be invariant.
        # A dynamically mounted focus row would change the status region's height;
        # asserting equal geometry catches that exact regression.
        await app.status_region.apply(
            StatusTickResult(outcome="ok", rows=("branch: main", "tests: 296"))
        )
        await pilot.pause()
        baseline_with_rows = regions()

        app.transcript.focus()
        await pilot.pause()
        assert regions() == baseline_with_rows, "focus in transcript with rows moved a region"

        app.prompts.focus()
        await pilot.pause()
        assert regions() == baseline_with_rows, "focus in prompts container with rows moved a region"

        app.composer.text_area.focus()
        await pilot.pause()
        assert regions() == baseline_with_rows, "returning focus to composer with rows moved a region"

        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_never_probed_board_paints_no_seam_rows() -> None:
    """The render tick refreshes a painted seam board; it does not create one.

    An unprobed board is not empty — every seam in it reads never-observed — so
    a repaint that did not check whether anything had ever been painted would put
    four rows on screen in replay, where no probe runs and there is nothing to be
    never-observed *about* yet.

    This exists because the clause was already caught, but only by
    :func:`test_status_region_geometry_is_invariant_across_focus_states`, and only
    when the whole suite runs: that test reads its baseline geometry after the
    first pause, so it notices the extra rows only if a 50ms render tick happens
    to land between the baseline and a later assertion. Under load it fails every
    time; run alone it passes every time. A pin that needs a busy machine is not
    a pin — it is a test that will one day stop catching this and tell nobody. So
    the render tick is driven directly here, and the assertion is on the rows.
    """
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:
        await pilot.pause()
        assert app.status_region.seam_texts == ()
        await app._render_tick()
        await pilot.pause()
        assert app.status_region.seam_texts == (), "a render tick painted an unprobed board"
        await app.shutdown_sources()
