"""D6's two input routes and the C9 stage/remove flows (issue #147).

Pure route decisions first — drop detection, file/image routing, chip
splicing — then the live flows through a real app: ``/attach`` stages and
chips, a drop stages without inserting, PDFs are refused before any socket
write, gateway errors surface honestly, removal unchips and detaches, and
submit reconciles omitted tokens.
"""

from __future__ import annotations

import asyncio
from collections.abc import Callable, Mapping
from dataclasses import replace
from pathlib import Path
from typing import Any

import pytest
from textual.events import Paste

from talaria.domain.commands import LocalInvocation, resolve_command
from talaria.replay.controls import ReplayControls
from talaria.replay.source import ReplaySource
from talaria.transport.attach import AttachTarget
from talaria.transport.rpc import RpcOutcome
from talaria.transport.source import LiveSource
from talaria.ui.app import TalariaApp
from talaria.ui.attach import (
    IMAGE_EXTENSIONS,
    classify_route,
    clean_path_text,
    detect_dropped_path,
    new_attachment_id,
    place_ref_chip,
    remove_ref_chip,
    stat_for_confirm,
)
from talaria.ui.dialog import ConfirmDialog
from tests.transport.conftest import StubGateway
from tests.transport.test_compat_baseline import StubProvider
from tests.ui.conftest import event, paused_app, records


def ok_result(method: str, result: Mapping[str, Any]) -> RpcOutcome:
    return RpcOutcome(
        status="ok", method=method, request_id="1", epoch=1, result=result
    )


def error_result(method: str, code: int, message: str) -> RpcOutcome:
    return RpcOutcome(
        status="error",
        method=method,
        request_id="1",
        epoch=1,
        error_code=code,
        error_message=message,
    )


class ScriptedDispatcher:
    """Answers each method from a fixed table, recording every call."""

    def __init__(self, outcomes: Mapping[str, RpcOutcome] | None = None) -> None:
        self._outcomes = dict(outcomes or {})
        self.calls: list[tuple[str, Mapping[str, Any]]] = []

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome:
        self.calls.append((method, dict(params or {})))
        outcome = self._outcomes.get(method)
        if outcome is not None:
            return outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})

    @property
    def operator_calls(self) -> list[tuple[str, Mapping[str, Any]]]:
        return [call for call in self.calls if call[0] != "commands.catalog"]


def focused_live_app(dispatcher: ScriptedDispatcher) -> TalariaApp:
    controls = ReplayControls(paused=True)
    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    app = TalariaApp(source, mode="live", controls=controls, dispatcher=dispatcher)
    app.state = replace(app.state, focused_session_id="s1")
    return app


# ── path cleaning ────────────────────────────────────────────────────────


def test_bare_and_quoted_paths_clean_to_the_same_bare_form() -> None:
    assert clean_path_text("  notes.txt  ") == "notes.txt"
    assert clean_path_text("'my notes.txt'") == "my notes.txt"
    assert clean_path_text('"my notes.txt"') == "my notes.txt"
    assert clean_path_text("`my notes.txt`") == "my notes.txt"


def test_backslash_escapes_clean_without_quotes() -> None:
    assert clean_path_text("my\\ notes.txt") == "my notes.txt"


def test_a_leading_tilde_expands_before_any_reader() -> None:
    assert clean_path_text("~/notes.txt") == str(Path("~/notes.txt").expanduser())


def test_multiline_text_cleans_to_itself_because_it_is_not_a_path() -> None:
    assert clean_path_text("line one\nline two") == "line one\nline two"


# ── drop detection ───────────────────────────────────────────────────────


def test_a_paste_naming_an_existing_file_is_a_drop(tmp_path: Path) -> None:
    target = tmp_path / "dropped.txt"
    target.write_text("hello")
    assert detect_dropped_path(str(target)) == str(target)


def test_a_quoted_drop_unquotes(tmp_path: Path) -> None:
    target = tmp_path / "my notes.txt"
    target.write_text("hello")
    assert detect_dropped_path(f"'{target}'") == str(target)


def test_a_body_naming_nothing_stays_text(tmp_path: Path) -> None:
    assert detect_dropped_path(str(tmp_path / "absent.txt")) is None


def test_multiline_and_empty_bodies_are_never_drops(tmp_path: Path) -> None:
    target = tmp_path / "dropped.txt"
    target.write_text("hello")
    assert detect_dropped_path(f"{target}\nsecond line") is None
    assert detect_dropped_path("") is None
    assert detect_dropped_path("   ") is None


def test_a_directory_is_not_a_drop(tmp_path: Path) -> None:
    assert detect_dropped_path(str(tmp_path)) is None


# ── routing ──────────────────────────────────────────────────────────────


def test_every_gateway_image_extension_routes_image() -> None:
    assert IMAGE_EXTENSIONS == frozenset(
        {
            ".png",
            ".jpg",
            ".jpeg",
            ".gif",
            ".webp",
            ".bmp",
            ".tiff",
            ".tif",
            ".svg",
            ".ico",
        }
    )
    for extension in IMAGE_EXTENSIONS | {".PNG", ".JPG"}:
        assert classify_route(f"photo{extension}") == "image"


def test_text_code_and_unknown_route_file_including_pdfs() -> None:
    # PDFs route file-ward on purpose: the Talaria-side refusal lives in
    # the transport after the read (extension *and* magic bytes), so a
    # renamed PDF cannot slip through as text.
    assert classify_route("notes.txt") == "file"
    assert classify_route("main.py") == "file"
    assert classify_route("doc.pdf") == "file"
    assert classify_route("no-extension") == "file"


def test_stat_reports_kind_and_size_or_nothing(tmp_path: Path) -> None:
    target = tmp_path / "pic.png"
    target.write_bytes(b"\x89PNG\r\n\x1a\n" + b"0" * 100)
    assert stat_for_confirm(target) == ("image", target.stat().st_size)
    assert stat_for_confirm(tmp_path / "absent.txt") is None
    assert stat_for_confirm(tmp_path) is None


# ── chip splicing ────────────────────────────────────────────────────────


def test_placing_a_chip_joins_with_one_space() -> None:
    assert place_ref_chip("", "@file:a.txt") == "@file:a.txt"
    assert place_ref_chip("look ", "@file:a.txt") == "look @file:a.txt"
    assert place_ref_chip("look", "@file:a.txt") == "look @file:a.txt"
    assert place_ref_chip("a @file:x", "@file:a.txt") == "a @file:x @file:a.txt"


def test_removing_a_chip_tidies_only_its_joint() -> None:
    assert remove_ref_chip("look @file:a.txt please", "@file:a.txt") == "look please"
    assert remove_ref_chip("@file:a.txt please", "@file:a.txt") == "please"
    assert remove_ref_chip("look @file:a.txt", "@file:a.txt") == "look"
    assert remove_ref_chip("@file:a.txt", "@file:a.txt") == ""
    assert remove_ref_chip("look please", "@file:a.txt") == "look please"


def test_removal_does_not_corrupt_a_longer_token() -> None:
    # Glued on either side, the match is part of a longer token, not a chip.
    assert remove_ref_chip("x@file:a.txt b", "@file:a.txt") == "x@file:a.txt b"
    assert remove_ref_chip("a @file:a.txtx", "@file:a.txt") == "a @file:a.txtx"


def test_attachment_ids_are_short_unique_hex() -> None:
    ids = {new_attachment_id() for _ in range(100)}
    assert len(ids) == 100
    assert all(len(ticket) == 12 for ticket in ids)


# ── command resolution ───────────────────────────────────────────────────


def test_attach_resolves_local_with_its_argument() -> None:
    invocation = resolve_command("/attach notes.txt", None)
    assert isinstance(invocation, LocalInvocation)
    assert invocation.command.action == "attach"
    assert invocation.argument == "notes.txt"


def test_attach_without_an_argument_resolves_for_removal() -> None:
    invocation = resolve_command("/attach", None)
    assert isinstance(invocation, LocalInvocation)
    assert invocation.command.action == "attach"
    assert invocation.argument == ""


# ── live flows ───────────────────────────────────────────────────────────


async def confirm_open_dialog(pilot: Any) -> None:
    """Answer the attachment dialog on its proceed row (cancel opens first)."""
    await pilot.pause()
    await pilot.press("down")
    await pilot.press("enter")
    await pilot.pause()


def staged_records(app: TalariaApp) -> list[Any]:
    return [record for record in app.state.attachments]


@pytest.mark.asyncio
async def test_attach_stages_a_file_chips_the_composer_and_records(
    tmp_path: Path,
) -> None:
    dispatcher = ScriptedDispatcher(
        {
            "file.attach": ok_result(
                "file.attach",
                {
                    "attached": True,
                    "name": "notes.txt",
                    "ref_text": "@file:notes.txt",
                    "uploaded": True,
                },
            )
        }
    )
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        target = tmp_path / "notes.txt"
        target.write_text("live content")
        app.composer.text = f"/attach {target}"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        assert isinstance(app.screen, ConfirmDialog)
        assert "Attach this file?" in app.screen.title_text
        await confirm_open_dialog(pilot)
        await app.settle_live()
        await pilot.pause()

        methods = [call[0] for call in dispatcher.operator_calls]
        assert "file.attach" in methods
        params = dict(dispatcher.operator_calls[methods.index("file.attach")][1])
        assert params["session_id"] == "s1"
        assert params["name"] == "notes.txt"
        assert params["data_url"].startswith("data:text/plain;base64,")
        # The client places the reference; the gateway never injects it —
        # and the issued command line is consumed, not submitted as prose.
        assert app.composer.text == "@file:notes.txt"
        found = staged_records(app)
        assert len(found) == 1
        assert found[0].kind == "file"
        assert found[0].state == "attached"
        assert found[0].gateway_ref == "@file:notes.txt"
        assert found[0].session_id == "s1"
        # Staged, never delivered — the notice says exactly that.
        assert "staged" in app.composer.notice
        assert "not delivered" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_cancelling_the_confirm_stages_nothing(tmp_path: Path) -> None:
    dispatcher = ScriptedDispatcher()
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        target = tmp_path / "cancel.txt"
        target.write_text("cancel me")
        app.composer.text = f"/attach {target}"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        assert isinstance(app.screen, ConfirmDialog)
        await pilot.press("escape")
        await pilot.pause()
        await app.settle_live()

        assert "file.attach" not in [call[0] for call in dispatcher.operator_calls]
        assert len(staged_records(app)) == 0
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_attach_of_a_missing_path_notices_without_a_dialog() -> None:
    dispatcher = ScriptedDispatcher()
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        app.composer.text = "/attach /nonexistent-xyz-123.txt"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()

        assert not isinstance(app.screen, ConfirmDialog)
        assert "nothing was sent" in app.composer.notice
        assert dispatcher.operator_calls == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_multiline_text_is_a_message_never_an_attach() -> None:
    """``parse_command_line`` never resolves multi-line text as a command —
    a pasted script starting with a slash is a message. So this submits the
    text untouched: no dialog, no staging, no refusal either."""
    dispatcher = ScriptedDispatcher()
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        app.composer.text = "/attach one.txt\ntwo.txt"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert not isinstance(app.screen, ConfirmDialog)
        submits = [
            call for call in dispatcher.operator_calls if call[0] == "prompt.submit"
        ]
        assert len(submits) == 1
        assert submits[0][1]["text"] == "/attach one.txt\ntwo.txt"
        assert len(staged_records(app)) == 0
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_attach_in_replay_is_refused_like_every_mutation() -> None:
    app, controls = paused_app([event("gateway.ready", {})])
    async with app.run_test() as pilot:
        app.composer.text = "/attach notes.txt"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()

        assert [outcome.name for outcome in controls.refusals] == ["command-dispatch"]
        assert not isinstance(app.screen, ConfirmDialog)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_dropped_path_stages_without_inserting(tmp_path: Path) -> None:
    dispatcher = ScriptedDispatcher(
        {
            "file.attach": ok_result(
                "file.attach",
                {"attached": True, "name": "drop.txt", "ref_text": "@file:drop.txt"},
            )
        }
    )
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        target = tmp_path / "drop.txt"
        target.write_text("dropped content")
        app.composer.text_area.focus()
        app.post_message(Paste(str(target)))
        await pilot.pause()
        await pilot.pause()

        # Never inserted: the drop diverts before the literal insert.
        assert app.composer.text == ""
        assert isinstance(app.screen, ConfirmDialog)
        await confirm_open_dialog(pilot)
        await app.settle_live()
        await pilot.pause()

        assert "@file:drop.txt" in app.composer.text
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_paste_naming_nothing_stays_text() -> None:
    dispatcher = ScriptedDispatcher()
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        app.post_message(Paste("/nonexistent-xyz-123.txt"))
        await pilot.pause()
        await pilot.pause()

        assert app.composer.text == "/nonexistent-xyz-123.txt"
        assert not isinstance(app.screen, ConfirmDialog)
        assert dispatcher.operator_calls == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_attach_stages_an_image_with_no_chip(tmp_path: Path) -> None:
    dispatcher = ScriptedDispatcher(
        {
            "image.attach_bytes": ok_result(
                "image.attach_bytes",
                {"attached": True, "path": "images/shot.png", "bytes": 72},
            )
        }
    )
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        target = tmp_path / "shot.png"
        target.write_bytes(b"\x89PNG\r\n\x1a\n" + b"0" * 64)
        app.composer.text = f"/attach {target}"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        assert isinstance(app.screen, ConfirmDialog)
        await confirm_open_dialog(pilot)
        await app.settle_live()
        await pilot.pause()

        image_calls = [
            call
            for call in dispatcher.operator_calls
            if call[0] == "image.attach_bytes"
        ]
        assert len(image_calls) == 1
        assert image_calls[0][1]["filename"] == "shot.png"
        assert "content_base64" in image_calls[0][1]
        # Images take no @file: reference — submit drains the queue.
        assert "@file:" not in app.composer.text
        found = staged_records(app)
        assert len(found) == 1
        assert found[0].kind == "image"
        assert found[0].gateway_ref == "images/shot.png"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_pdf_is_refused_before_any_socket_write(tmp_path: Path) -> None:
    dispatcher = ScriptedDispatcher()
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        target = tmp_path / "doc.pdf"
        target.write_bytes(b"%PDF-1.7 fake")
        app.composer.text = f"/attach {target}"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        assert isinstance(app.screen, ConfirmDialog)
        await confirm_open_dialog(pilot)
        await app.settle_live()
        await pilot.pause()

        assert "file.attach" not in [call[0] for call in dispatcher.operator_calls]
        assert "image.attach_bytes" not in [
            call[0] for call in dispatcher.operator_calls
        ]
        assert "portable-document" in app.composer.notice
        assert len(staged_records(app)) == 0
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_gateway_4016_surfaces_honestly_with_nothing_staged(
    tmp_path: Path,
) -> None:
    dispatcher = ScriptedDispatcher(
        {
            "image.attach_bytes": error_result(
                "image.attach_bytes", 4016, "unsupported image extension: .png"
            )
        }
    )
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        target = tmp_path / "fake.png"
        target.write_text("not an image at all")
        app.composer.text = f"/attach {target}"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        await confirm_open_dialog(pilot)
        await app.settle_live()
        await pilot.pause()

        assert "unsupported image extension" in app.composer.notice
        assert len(staged_records(app)) == 0
        assert "@file:" not in app.composer.text
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_bare_attach_offers_removal_and_removes_the_chip(
    tmp_path: Path,
) -> None:
    dispatcher = ScriptedDispatcher(
        {
            "file.attach": ok_result(
                "file.attach",
                {"attached": True, "name": "gone.txt", "ref_text": "@file:gone.txt"},
            )
        }
    )
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        target = tmp_path / "gone.txt"
        target.write_text("soon removed")
        app.composer.text = f"/attach {target}"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        await confirm_open_dialog(pilot)
        await app.settle_live()
        await pilot.pause()
        assert "@file:gone.txt" in app.composer.text

        app.composer.text = "/attach"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        assert isinstance(app.screen, ConfirmDialog)
        assert "Remove this attachment?" in app.screen.title_text
        await confirm_open_dialog(pilot)
        await pilot.pause()

        assert "@file:gone.txt" not in app.composer.text
        assert len(staged_records(app)) == 0
        # Removal is local for files — no detach call exists to make.
        assert [call[0] for call in dispatcher.operator_calls].count("file.attach") == 1
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_bare_attach_with_nothing_staged_teaches_the_command() -> None:
    dispatcher = ScriptedDispatcher()
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        app.composer.text = "/attach"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()

        assert not isinstance(app.screen, ConfirmDialog)
        assert "/attach" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_removing_a_staged_image_detaches_gateway_side(tmp_path: Path) -> None:
    dispatcher = ScriptedDispatcher(
        {
            "image.attach_bytes": ok_result(
                "image.attach_bytes", {"attached": True, "path": "images/old.png"}
            ),
            "image.detach": ok_result("image.detach", {"detached": True}),
        }
    )
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        target = tmp_path / "old.png"
        target.write_bytes(b"\x89PNG\r\n\x1a\n")
        app.composer.text = f"/attach {target}"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        await confirm_open_dialog(pilot)
        await app.settle_live()
        await pilot.pause()
        assert len(staged_records(app)) == 1

        app.composer.text = "/attach"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        await confirm_open_dialog(pilot)
        await app.settle_live()
        await pilot.pause()

        detach_calls = [
            call for call in dispatcher.operator_calls if call[0] == "image.detach"
        ]
        assert len(detach_calls) == 1
        assert detach_calls[0][1]["path"] == "images/old.png"
        assert len(staged_records(app)) == 0
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_submit_drops_an_omitted_chip_and_keeps_a_carried_one(
    tmp_path: Path,
) -> None:
    dispatcher = ScriptedDispatcher(
        {
            "file.attach": ok_result(
                "file.attach",
                {"attached": True, "name": "kept.txt", "ref_text": "@file:kept.txt"},
            )
        }
    )
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        target = tmp_path / "kept.txt"
        target.write_text("kept content")
        app.composer.text = f"/attach {target}"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        await confirm_open_dialog(pilot)
        await app.settle_live()
        await pilot.pause()
        assert staged_records(app)[0].state == "attached"

        # The operator deletes the token before submitting: omission is
        # the detach, and the submit carries no reference.
        app.composer.text = "look at this instead"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        submits = [
            call for call in dispatcher.operator_calls if call[0] == "prompt.submit"
        ]
        assert len(submits) == 1
        assert submits[0][1]["text"] == "look at this instead"
        assert staged_records(app)[0].state == "detached"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_submit_carries_the_chip_the_client_placed(tmp_path: Path) -> None:
    """The point the superseded finding got wrong: the gateway never injects
    the reference, so the submitted text must already contain it."""
    dispatcher = ScriptedDispatcher(
        {
            "file.attach": ok_result(
                "file.attach",
                {
                    "attached": True,
                    "name": "carried.txt",
                    "ref_text": "@file:carried.txt",
                },
            )
        }
    )
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        target = tmp_path / "carried.txt"
        target.write_text("carried content")
        app.composer.text = f"/attach {target}"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        await confirm_open_dialog(pilot)
        await app.settle_live()
        await pilot.pause()

        app.composer.text = app.composer.text + " what does it say?"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        submits = [
            call for call in dispatcher.operator_calls if call[0] == "prompt.submit"
        ]
        assert len(submits) == 1
        assert "@file:carried.txt" in submits[0][1]["text"]
        assert staged_records(app)[0].state == "attached"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_focus_move_between_dialog_and_answer_stales_the_stage(
    tmp_path: Path,
) -> None:
    dispatcher = ScriptedDispatcher(
        {
            "file.attach": ok_result(
                "file.attach",
                {"attached": True, "name": "stale.txt", "ref_text": "@file:stale.txt"},
            )
        }
    )
    app = focused_live_app(dispatcher)
    async with app.run_test() as pilot:
        target = tmp_path / "stale.txt"
        target.write_text("stale content")
        app.composer.text = f"/attach {target}"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        assert isinstance(app.screen, ConfirmDialog)
        app.state = replace(app.state, focused_session_id="s2")
        await confirm_open_dialog(pilot)
        await app.settle_live()
        await pilot.pause()

        assert "file.attach" not in [call[0] for call in dispatcher.operator_calls]
        assert len(staged_records(app)) == 0
        assert "nothing was staged" in app.composer.notice
        await app.shutdown_sources()



@pytest.mark.asyncio
async def test_a_dropped_attach_explains_itself_past_the_reconnect(
    tmp_path: Path,
) -> None:
    """#161, from live-17's transfer rehearsal: the gateway dropped the
    session's ``file.attach`` before forwarding it, the link died, and
    Talaria reconnected. The transport resolves the call ``unknown`` the
    instant the socket dies and the failure text reaches the notice line
    milliseconds later — where the reconnect's completion used to erase it,
    so the operator learned nothing within 35 seconds. The connected
    transition's clear now takes only the lifecycle's own placeholder, so
    the explanation survives the reconnect: the operator reads the failure,
    keeps the recoverable command line, and nothing is recorded as staged.
    """
    saw_attach: asyncio.Event = asyncio.Event()

    def never_forwarded(
        query: dict[str, Any], gateway: StubGateway
    ) -> dict[str, Any] | None:
        if query.get("method") == "file.attach":
            saw_attach.set()
        return None

    gateway = StubGateway(responder=never_forwarded)
    await gateway.start()
    source = LiveSource(
        AttachTarget.from_url(gateway.url),
        StubProvider(),
        reconnect_delays=(0.0, 0.01, 0.01),
    )
    app = TalariaApp(source, mode="live", dispatcher=source)
    source.bind(on_connection=app.note_connection_state, on_reconnect=app.note_reconnect)

    async def wait_until(predicate: Callable[[], bool], *, timeout: float = 5.0) -> None:
        deadline = asyncio.get_running_loop().time() + timeout
        while not predicate():
            if asyncio.get_running_loop().time() > deadline:
                raise AssertionError("condition not reached in time")
            await asyncio.sleep(0.02)

    try:
        async with app.run_test(size=(120, 40)) as pilot:
            await gateway.wait_for_attach()
            # The lifecycle's own placeholder still clears on connect
            # (#161's second condition): after the startup connect the
            # line is empty, exactly as before.
            await wait_until(lambda: app.state.connection == "connected")
            for _ in range(5):
                await pilot.pause()
            assert app.composer.notice == ""

            target = tmp_path / "sample.txt"
            target.write_text("live content")
            app.composer.text = f"/attach {target}"
            app.composer.text_area.focus()
            await pilot.press("enter")
            await pilot.pause()
            assert isinstance(app.screen, ConfirmDialog)
            await confirm_open_dialog(pilot)

            await asyncio.wait_for(saw_attach.wait(), timeout=5)
            # The rehearsal's fault shape: the link closed with 1011 while
            # the request was in flight, so it is never forwarded, never
            # answered.
            await gateway.sessions[-1].connection.close(code=1011)

            await wait_until(lambda: len(gateway.sessions) == 2)
            await wait_until(lambda: app.state.connection == "connected")
            for _ in range(10):
                await pilot.pause()

            notice = app.composer.notice
            assert "file.attach call was interrupted" in notice
            assert "may or may not be staged" in notice
            # Recoverable input preserved; nothing claims delivery.
            assert app.composer.text.startswith("/attach")
            assert staged_records(app) == []
            await app.shutdown_sources()
    finally:
        await gateway.stop()
