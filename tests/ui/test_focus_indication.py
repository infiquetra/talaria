"""Issue #109 focus cues stay visible without moving any screen row."""

from __future__ import annotations

import pytest
from textual.color import Color
from textual.widgets import Static

from talaria.ui.focus import focused_region
from tests.ui.conftest import RecordingDispatcher, event, feed, live_app, settle


def _subagent_start() -> dict[str, object]:
    return event(
        "subagent.start",
        {"subagent_id": "a0", "goal": "review focus", "depth": 1, "task_index": 0},
    )


@pytest.mark.asyncio
async def test_focus_cues_change_text_and_colour_without_any_geometry_delta() -> None:
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(110, 34)) as pilot:
        feed(app, event("message.start", {}))
        feed(app, event("message.delta", {"text": "focus body"}), seq=100)
        feed(app, _subagent_start(), seq=101)
        feed(
            app,
            event(
                "approval.request",
                {
                    "description": "inspect the generated report",
                    "command": "open report.txt",
                    "choices": ["once", "deny"],
                },
            ),
            seq=102,
        )
        await settle(app, pilot)

        card = app.prompts.card_for("approval:s1#1")
        row = app.agents.row_for("a0")
        assert card is not None and card.action_widget is not None
        assert row is not None

        targets = (
            (app.composer.text_area, "composer"),
            (app.transcript, "transcript"),
            (card.action_widget, "prompts"),
            (row, "agents"),
        )

        def geometry() -> dict[str, object]:
            return {
                "status": app.status_region.region,
                "transcript": app.transcript.region,
                "prompts": app.prompts.region,
                "agents": app.agents.region,
                "composer": app.composer.region,
                "needs-you": app.needs_you_bar.region,
                "help": app.help_bar.region,
            }

        app.composer.text_area.focus()
        app.status_region.set_caret("composer")
        app.composer.show_caret_location(True)
        await pilot.pause()
        baseline = geometry()
        composer_top = app.composer.region.y
        transcript_anchor = app.transcript.capture_reading_anchor()

        for target, expected in targets:
            if expected == "prompts":
                card.focus_answer()
            else:
                target.focus()
            await pilot.pause()
            region = focused_region(app.focused)
            app.status_region.set_caret(region)
            app.composer.show_caret_location(region == "composer")
            await pilot.pause()

            assert region == expected
            assert app.status_region.focus_text == f"caret: {expected}"
            assert app.composer.border_title == (
                "compose [*] caret here"
                if expected == "composer"
                else "compose [ ] caret elsewhere"
            )
            assert geometry() == baseline
            assert app.composer.region.y == composer_top
            assert app.transcript.capture_reading_anchor() == transcript_anchor

            tokens = app.theme_registry.resolve(app.theme).tokens
            expected_border = (
                tokens["talaria.focus"]
                if expected == "composer"
                else tokens["talaria.border.muted"]
            )
            assert app.composer.styles.border_top[1] == Color.parse(expected_border)
            focus_row = app.status_region.query_one(".status--focus", Static)
            assert focus_row.styles.color == Color.parse(tokens["talaria.focus"])

        await app.shutdown_sources()


def test_an_unlisted_focusable_surface_is_named_from_its_real_id() -> None:
    widget = Static(id="operation-log")
    assert focused_region(widget) == "operation log"
