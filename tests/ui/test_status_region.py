# ruff: noqa: E501
"""R22's rendering half: multi-row literal output, ANSI shown and not obeyed.

U6 owns the runner and its failure taxonomy; this suite owns what reaches the
screen. The distinction matters because the dangerous half is here: a runner
that correctly captures ``\\x1b[2J`` and a region that faithfully forwards it to
the terminal together produce a status command that can clear the screen.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import replace
from typing import cast

import pytest
from textual.app import App, ComposeResult
from textual.widgets import Static

from talaria.domain.compat import (
    SeamBoard,
    SeamObservation,
    SeamStatus,
    apply_probe_round,
    empty_board,
)
from talaria.status.contract import TRUNCATION_MARKER, ScriptRow
from talaria.status.runner import StatusTickResult
from talaria.ui.app import TalariaApp
from talaria.ui.inspector import EMPTY_SECTION
from talaria.ui.literal import defang
from talaria.ui.status_region import StatusRegion
from talaria.ui.theme import BUILTIN_THEME_REGISTRY
from tests.ui.conftest import event, feed, paused_app, screen_text, streaming_turn


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


# ── #122/#144: the seam board lives in the inspector alone ──────────────
#
# #122 moved routine rows into the inspector and kept the actionable subset
# above the composer; #144 closed that second surface. What these tests pin
# now: every seam row — clean, actionable, or stale — renders in the
# inspector's diagnostics section and nowhere above the composer, with the
# inspector open or closed.


def _paint_board(
    app: TalariaApp, statuses: Mapping[str, str], *, at: float | None = None
) -> SeamBoard:
    """Fold fabricated probe results into the focused board at the frame clock.

    Seams omitted from ``statuses`` stay never-observed, the way
    ``kanban-dispatcher`` — and an ``http-runner`` seam with no HTTP probe
    behind it — always are after a real round.
    """
    clock = app.state.last_observed_at if at is None else at
    results = tuple(
        SeamObservation(
            seam=name,
            status=cast(SeamStatus, status),
            source=f"probe {name}",
            trigger="attach",
        )
        for name, status in statuses.items()
    )
    board = apply_probe_round(empty_board(app.fleet_profile), results, at=clock)
    app.fleet = replace(app.fleet, seam_boards={app.fleet_profile: board})
    return board


def _diag_names(app: TalariaApp) -> tuple[str, ...]:
    """The seam each inspector diagnostics row carries, in section order."""
    return tuple(line.strip().split(":")[0] for line in app.inspector.diag_texts)


def _region_texts(app: TalariaApp) -> str:
    """Everything the status region renders, joined for one substring check.

    The region has no seam surface any more, so the honest pin is on its whole
    rendered content: nothing it shows may name a seam.
    """
    return "\n".join((*app.status_region.row_texts, app.status_region.marker_text))


@pytest.mark.asyncio
async def test_a_clean_board_renders_in_the_inspector_alone() -> None:
    """A clean board fills the inspector and leaves the region empty."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.pause()
        _paint_board(app, {"roster": "present", "approval-detail": "present"})
        await app._render_seams()
        await pilot.pause()

        assert _diag_names(app) == (
            "roster",
            "approval-detail",
            "http-runner",
            "kanban-dispatcher",
        )
        assert screen_text(app).count("roster:") == 1, "the inspector copy is the only one"
        assert _region_texts(app) == ""
        await app.shutdown_sources()


@pytest.mark.parametrize(
    "status", ["absent", "incompatible", "degraded", "parameter-invalid"]
)
@pytest.mark.asyncio
async def test_actionable_seam_rows_never_return_above_the_composer(status: str) -> None:
    """#144: a seam naming a lost capability is inspector-only, in full."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.pause()
        _paint_board(app, {"roster": status, "approval-detail": "present"})
        await app._render_seams()
        await pilot.pause()

        assert _diag_names(app) == (
            "roster",
            "approval-detail",
            "http-runner",
            "kanban-dispatcher",
        )
        roster = next(line for line in app.inspector.diag_texts if "roster:" in line)
        assert status in roster
        assert screen_text(app).count("roster:") == 1, (
            "an actionable row must not duplicate above the composer"
        )
        assert _region_texts(app) == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_stale_present_seam_grows_stale_in_the_inspector_only() -> None:
    """An un-revalidated verdict is ambiguous currency — and #144 keeps even
    the stale copy off the composer, which is the duplicate the operator saw."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.pause()
        _paint_board(app, {"roster": "present"}, at=app.state.last_observed_at - 400.0)
        await app._render_seams()
        await pilot.pause()

        roster = next(line for line in app.inspector.diag_texts if "roster:" in line)
        assert "stale" in roster
        assert screen_text(app).count("roster:") == 1
        assert _region_texts(app) == ""

        # The age refresh is a display transition on existing data (I4's
        # trigger bound): it repaints the inspector's row and nothing else.
        await app._refresh_seam_ages()
        await pilot.pause()
        assert screen_text(app).count("roster:") == 1
        assert _region_texts(app) == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_with_the_inspector_closed_an_actionable_board_renders_nowhere_inline() -> (
    None
):
    """#144's rule holds with the inspector hidden: the rows do not fall back
    above the composer, they wait in state for the inspector to reopen."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.pause()
        app.inspector.toggle()
        await pilot.pause()

        _paint_board(app, {"roster": "absent", "approval-detail": "present"})
        await app._render_seams()
        await pilot.pause()

        assert "roster:" not in screen_text(app), (
            "a closed inspector must not push diagnostics above the composer"
        )
        assert _region_texts(app) == ""
        assert _diag_names(app) == (
            "roster",
            "approval-detail",
            "http-runner",
            "kanban-dispatcher",
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_unprobed_board_keeps_the_inspector_honestly_empty() -> None:
    """No probe yet is an empty sentence in the inspector, never a zero."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.pause()
        assert app.inspector.diag_texts == ()
        assert _region_texts(app) == ""
        empty = app.inspector.query_one(".inspector--diag-empty", Static)
        assert str(empty.content) == f"  {EMPTY_SECTION}"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_seam_failing_mid_move_surfaces_in_the_inspector_alone() -> None:
    """A seam that turns against the interface mid-move updates its inspector
    row — it does not return above the composer to be seen."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.pause()
        clock = app.state.last_observed_at
        board = _paint_board(
            app, {"roster": "present", "approval-detail": "present"}, at=clock
        )
        await app._render_seams()
        await pilot.pause()
        assert screen_text(app).count("roster:") == 1

        board = apply_probe_round(
            board,
            (
                SeamObservation(
                    seam="roster",
                    status="absent",
                    source="probe roster",
                    trigger="revalidation",
                    detail="session.active_list answered 404",
                ),
            ),
            at=clock,
        )
        app.fleet = replace(app.fleet, seam_boards={app.fleet_profile: board})
        await app._render_seams()
        await pilot.pause()

        roster = next(line for line in app.inspector.diag_texts if "roster:" in line)
        assert roster.startswith("  roster: absent")
        assert screen_text(app).count("roster:") == 1
        assert _region_texts(app) == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_marker_and_notice_paths_survive_the_seam_move() -> None:
    """The alert paths the seam move did not touch still reach the marker, and
    the caret row in the inspector's context is not clobbered by a seam paint."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.pause()
        context_before = app.inspector.context_text
        assert "caret" in context_before
        _paint_board(app, {"roster": "present", "approval-detail": "present"})
        await app._render_seams()
        await pilot.pause()
        assert app.inspector.context_text == context_before

        await app.status_region.apply(
            StatusTickResult(outcome="timeout", marker="status: slow command")
        )
        await pilot.pause()
        assert app.status_region.marker_text == "[x] status: slow command"
        assert app.status_region.row_texts == ()
        assert len(app.inspector.diag_texts) == 4

        app.status_region.show_configuration_notice(
            "status.command has invalid quoting; the status command is disabled"
        )
        assert app.status_region.marker_text == (
            "[x] status.command has invalid quoting; the status command is disabled"
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_failed_move_drops_the_board_until_the_next_paint(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """#144 removed the region fallback: an inspector that cannot take the rows
    leaves them nowhere on screen, and the unset cache retries on the next paint
    rather than remembering a refusal as done."""

    async def _refused(self: TalariaApp, lines: tuple[str, ...]) -> bool:
        return False

    original = TalariaApp._render_inspector_diagnostics
    monkeypatch.setattr(TalariaApp, "_render_inspector_diagnostics", _refused)
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.pause()
        _paint_board(app, {"roster": "present", "approval-detail": "present"})
        await app._render_seams()
        await pilot.pause()

        assert _diag_names(app) == ()
        assert "roster:" not in screen_text(app), (
            "a refused move must not fall back to the retired region copy"
        )
        assert _region_texts(app) == ""

        monkeypatch.setattr(TalariaApp, "_render_inspector_diagnostics", original)
        await app._render_seams()
        await pilot.pause()
        assert _diag_names(app) == (
            "roster",
            "approval-detail",
            "http-runner",
            "kanban-dispatcher",
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_clean_flow_snapshot_keeps_routine_rows_out_of_the_composer_area() -> None:
    """Steady-state ticks render the chat with nowhere routine leaking."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.pause()
        for seq, frame in enumerate(streaming_turn(["hello"]), start=101):
            feed(app, frame, seq=seq)
        # Paint after the frames: the board is stamped on the frame clock, and
        # a board stamped before them would be honestly stale by render time.
        _paint_board(app, {"roster": "present", "approval-detail": "present"})
        await app._render_seams()
        await app.render_snapshot()
        await pilot.pause()

        text = screen_text(app)
        assert "hello" in text
        assert text.count("roster:") == 1  # moved, not gone: it lives in the inspector
        assert app.status_region.row_texts == ()
        assert app.status_region.marker_text == ""
        assert not any(
            "roster:" in entry.text or "approval-detail:" in entry.text
            for entry in app.state.transcript
        )
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
        assert app.inspector.diag_texts == ()
        await app._render_tick()
        await pilot.pause()
        assert app.inspector.diag_texts == (), "a render tick painted an unprobed board"
        await app.shutdown_sources()


# ── #125 coexistence: the bar owns script rows, the region keeps the marker ──
#
# Ownership, asserted from the region side: a version-2 tick renders
# nothing here (not even when it carries script rows), while a bad
# document still surfaces its notice through this widget's marker.


@pytest.mark.asyncio
async def test_a_version_two_tick_renders_no_region_rows() -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:
        await app.status_region.apply(
            StatusTickResult(
                outcome="ok",
                script_rows=(ScriptRow("bar-row"), ScriptRow("wip: 2", "warning")),
            )
        )
        await pilot.pause()
        assert app.status_region.row_texts == ()
        assert app.status_region.marker_text == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_invalid_document_tick_marks_the_region_without_rows() -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:
        await app.status_region.apply(
            StatusTickResult(
                outcome="invalid_document",
                marker="status: invalid script document: unknown script document version 9",
            )
        )
        await pilot.pause()
        assert app.status_region.row_texts == ()
        assert app.status_region.marker_text == (
            "[x] status: invalid script document: unknown script document version 9"
        )
        await app.shutdown_sources()
