"""Attachment upload tests over a real stub gateway (C9).

Every test here dials :class:`StubGateway` with the real ``websockets``
client and drives :mod:`talaria.transport.attachments` through
:meth:`LiveSource.call`, so the asserted wire facts — the method names,
the ``data_url``/``content_base64`` param keys, the five gateway codes —
are proved against frames a server actually received, not against a
double's expectations. Gateway shapes mirror ``tui_gateway/server.py``
(Hermes ``8980b816f`` corroborating the I6 finding at ``63279301b``).
"""

from __future__ import annotations

import asyncio
import base64
from pathlib import Path
from typing import Any

import pytest
import pytest_asyncio

from talaria.recorder.framelog import FrameRecorder
from talaria.transport import attachments
from talaria.transport.attach import AttachTarget
from talaria.transport.credentials import Credential
from talaria.transport.source import LiveSource
from tests.transport.conftest import (
    STUB_TOKEN,
    StubGateway,
    err,
    ok,
)

FAST_RETRIES = (0.0, 0.01, 0.01)

#: Swept for across the recorded log file. A distinctive local path and
#: distinctive content, so a substring search over raw bytes is conclusive
#: for both.
CANARY_DIR = "canary-home-Rm4pZ9wx"
CANARY_TEXT = "canary-content-Rm4pZ9wx-must-not-be-recorded"


class StubProvider:
    """The loopback token, minted per dial as KTD11 requires."""

    async def acquire(self) -> Credential:
        return Credential("token", STUB_TOKEN, "file")


def attachment_responder(
    seen: list[tuple[str, dict[str, Any]]],
    *,
    file_error: tuple[int, str] | None = None,
    image_error: tuple[int, str] | None = None,
) -> Any:
    """Answer the three attachment methods the way the gateway does."""

    def respond(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        method = message.get("method")
        params = message.get("params", {})
        if method == "file.attach":
            seen.append((method, dict(params)))
            if file_error is not None:
                return err(message.get("id"), file_error[0], file_error[1])
            name = params.get("name") or Path(str(params.get("path", "f"))).name
            return ok(
                message.get("id"),
                {
                    "attached": True,
                    "name": name,
                    "path": f"/gateway/ws/.hermes/desktop-attachments/{name}",
                    "ref_path": f".hermes/desktop-attachments/{name}",
                    "ref_text": f"@file:.hermes/desktop-attachments/{name}",
                    "uploaded": True,
                },
            )
        if method == "image.attach_bytes":
            seen.append((method, dict(params)))
            if image_error is not None:
                return err(message.get("id"), image_error[0], image_error[1])
            return ok(
                message.get("id"),
                {
                    "attached": True,
                    "path": "/gateway/ws/.hermes/images/upload-ab12.png",
                    "count": 1,
                    "text": "[User attached image: upload-ab12.png]",
                    "bytes": 18,
                },
            )
        if method == "image.detach":
            seen.append((method, dict(params)))
            return ok(message.get("id"), {"detached": True, "count": 0})
        return None

    return respond


async def until(predicate: Any, *, timeout: float = 5.0) -> None:
    async def _poll() -> None:
        while not predicate():
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_poll(), timeout=timeout)


def connected_source(
    gateway: StubGateway, recorder: FrameRecorder | None = None
) -> LiveSource:
    return LiveSource(
        AttachTarget.from_url(gateway.url),
        StubProvider(),
        recorder=recorder,
        reconnect_delays=FAST_RETRIES,
    )


@pytest_asyncio.fixture
async def attachment_gateway() -> Any:
    seen: list[tuple[str, dict[str, Any]]] = []
    stub = StubGateway(responder=attachment_responder(seen))
    await stub.start()
    try:
        yield stub, seen
    finally:
        await stub.stop()


def live_source(
    gateway: StubGateway, recorder: FrameRecorder | None = None
) -> LiveSource:
    return connected_source(gateway, recorder=recorder)


def write_canary_file(root: Path) -> Path:
    home = root / CANARY_DIR
    home.mkdir()
    target = home / "notes.txt"
    target.write_text(CANARY_TEXT + "\n", encoding="utf-8")
    return target


@pytest.mark.asyncio
async def test_file_attach_uploads_bytes_and_returns_the_file_ref(
    attachment_gateway: tuple[StubGateway, list[tuple[str, dict[str, Any]]]], tmp_path: Path
) -> None:
    gateway, seen = attachment_gateway
    target = write_canary_file(tmp_path)
    source = live_source(gateway)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        outcome = await attachments.attach_file(source, target)
    finally:
        await source.close()

    assert outcome.status == "ok"
    assert outcome.kind == "attached"
    assert outcome.gateway_ref == "@file:.hermes/desktop-attachments/notes.txt"
    assert outcome.uploaded is True
    assert isinstance(outcome, attachments.FileAttachOutcome)
    assert outcome.name == "notes.txt"
    # The route layer places this string into the composer text itself.
    assert outcome.prompt_text == "@file:.hermes/desktop-attachments/notes.txt"
    assert "composer" in outcome.detail

    assert len(seen) == 1
    method, params = seen[0]
    assert method == "file.attach"
    # Corrected contract: {session_id, data_url, name} — notably no client
    # path, which would ask the gateway to read a disk it cannot see.
    assert "path" not in params
    assert "session_id" not in params
    assert params["name"] == "notes.txt"
    data_url = params["data_url"]
    assert data_url.startswith("data:text/plain;base64,")
    assert base64.b64decode(data_url.split(",", 1)[1]).decode() == CANARY_TEXT + "\n"


@pytest.mark.asyncio
async def test_recorded_log_never_holds_content_or_local_path(
    attachment_gateway: tuple[StubGateway, list[tuple[str, dict[str, Any]]]],
    tmp_path: Path,
) -> None:
    """The controller's bar: absence proved on the real recorded bytes."""
    gateway, _seen = attachment_gateway
    target = write_canary_file(tmp_path)
    log = tmp_path / "frames.jsonl"
    recorder = FrameRecorder(log, gateway.url)
    source = live_source(gateway, recorder=recorder)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        outcome = await attachments.attach_file(source, target)
        outcome_image = await attachments.attach_image_file(source, target)
        await attachments.detach_image(source, "/gateway/ws/img.png")
    finally:
        await source.close()

    assert outcome.status == "ok"
    assert outcome_image.status == "ok"
    raw = log.read_bytes()
    assert CANARY_TEXT.encode() not in raw
    assert CANARY_DIR.encode() not in raw


@pytest.mark.asyncio
async def test_image_attach_uploads_base64_and_returns_the_staged_path(
    attachment_gateway: tuple[StubGateway, list[tuple[str, dict[str, Any]]]], tmp_path: Path
) -> None:
    gateway, seen = attachment_gateway
    target = write_canary_file(tmp_path)
    source = live_source(gateway)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        outcome = await attachments.attach_image_file(source, target)
    finally:
        await source.close()

    assert outcome.status == "ok"
    assert outcome.kind == "attached"
    assert outcome.gateway_ref == "/gateway/ws/.hermes/images/upload-ab12.png"
    assert outcome.bytes_staged == 18

    assert outcome.prompt_text is None

    assert len(seen) == 1
    method, params = seen[0]
    assert method == "image.attach_bytes"
    # Corrected contract: {session_id, content_base64} — the gateway sniffs
    # the type itself. ``filename`` travels only when the caller passes it.
    assert set(params) == {"content_base64"}
    assert base64.b64decode(params["content_base64"]).decode() == CANARY_TEXT + "\n"


@pytest.mark.asyncio
async def test_image_attach_carries_the_filename_when_given(
    attachment_gateway: tuple[StubGateway, list[tuple[str, dict[str, Any]]]], tmp_path: Path
) -> None:
    """The turn contract's optional field (I6 addendum on #147): the route
    layer passes the base name so gateway errors can name the file. Absent
    by default, present when asked — never invented from the path."""
    gateway, seen = attachment_gateway
    target = write_canary_file(tmp_path)
    source = live_source(gateway)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        outcome = await attachments.attach_image_file(
            source, target, filename="canary.png"
        )
    finally:
        await source.close()

    assert outcome.status == "ok"
    assert len(seen) == 1
    _, params = seen[0]
    assert params["filename"] == "canary.png"
    assert set(params) == {"content_base64", "filename"}


@pytest.mark.asyncio
async def test_session_id_scopes_the_call_when_given(
    attachment_gateway: tuple[StubGateway, list[tuple[str, dict[str, Any]]]],
    tmp_path: Path,
) -> None:
    gateway, seen = attachment_gateway
    target = write_canary_file(tmp_path)
    source = live_source(gateway)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        outcome = await attachments.attach_file(
            source, target, session_id="sess-9"
        )
    finally:
        await source.close()

    assert outcome.status == "ok"
    assert seen[0][1]["session_id"] == "sess-9"


@pytest.mark.asyncio
async def test_detach_reports_idempotent_removal(
    attachment_gateway: tuple[StubGateway, list[tuple[str, dict[str, Any]]]],
) -> None:
    gateway, _seen = attachment_gateway
    source = live_source(gateway)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        outcome = await attachments.detach_image(source, "/gateway/ws/img.png")
    finally:
        await source.close()

    assert outcome.status == "ok"
    assert outcome.kind == "detached"
    assert "detached" in outcome.detail


@pytest.mark.asyncio
async def test_missing_file_never_reaches_the_wire(
    attachment_gateway: tuple[StubGateway, list[tuple[str, dict[str, Any]]]], tmp_path: Path
) -> None:
    gateway, seen = attachment_gateway
    source = live_source(gateway)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        outcome = await attachments.attach_file(
            source, tmp_path / "absent.txt"
        )
    finally:
        await source.close()

    assert outcome.status == "error"
    assert outcome.kind == "missing-file"
    assert "nothing was sent" in outcome.detail
    assert seen == []


@pytest.mark.asyncio
async def test_unreadable_path_is_explained_not_raised(
    attachment_gateway: tuple[StubGateway, list[tuple[str, dict[str, Any]]]], tmp_path: Path
) -> None:
    gateway, _seen = attachment_gateway
    source = live_source(gateway)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        outcome = await attachments.attach_file(source, tmp_path)
    finally:
        await source.close()

    assert outcome.status == "error"
    assert outcome.kind in ("missing-file", "unreadable-file")
    assert "nothing was sent" in outcome.detail


@pytest.mark.asyncio
async def test_gateway_type_refusal_maps_to_unsupported_type(
    tmp_path: Path,
) -> None:
    seen: list[tuple[str, dict[str, Any]]] = []
    stub = StubGateway(
        responder=attachment_responder(
            seen, image_error=(4016, "unsupported image extension: .pdf")
        )
    )
    await stub.start()
    # The image path carries no client-side refusal: these bytes reach the
    # gateway, which sniffs them and answers 4016 — Live 17's real case.
    target = tmp_path / "doc.pdf"
    target.write_bytes(b"%PDF-1.4\n" + b"\x00" * 64)
    source = connected_source(stub)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        outcome = await attachments.attach_image_file(source, target)
    finally:
        await source.close()
        await stub.stop()

    assert outcome.status == "error"
    assert outcome.kind == "unsupported-type"
    assert "unsupported image extension" in outcome.detail
    assert "convert" in outcome.detail


@pytest.mark.asyncio
async def test_gateway_size_cap_maps_to_too_large(tmp_path: Path) -> None:
    seen: list[tuple[str, dict[str, Any]]] = []
    stub = StubGateway(
        responder=attachment_responder(
            seen, image_error=(4018, "image too large (99999999 bytes; cap is 10 MB)")
        )
    )
    await stub.start()
    target = tmp_path / "big.png"
    target.write_bytes(b"\x89PNG" + b"\x00" * 64)
    source = connected_source(stub)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        outcome = await attachments.attach_image_file(source, target)
    finally:
        await source.close()
        await stub.stop()

    assert outcome.status == "error"
    assert outcome.kind == "too-large"
    assert "cap" in outcome.detail


@pytest.mark.asyncio
async def test_gateway_staging_failure_maps_to_staging_failed(tmp_path: Path) -> None:
    seen: list[tuple[str, dict[str, Any]]] = []
    stub = StubGateway(
        responder=attachment_responder(
            seen, file_error=(5028, "disk quota exceeded on gateway")
        )
    )
    await stub.start()
    target = tmp_path / "notes.txt"
    target.write_text("hello\n", encoding="utf-8")
    source = connected_source(stub)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        outcome = await attachments.attach_file(source, target)
    finally:
        await source.close()
        await stub.stop()

    assert outcome.status == "error"
    assert outcome.kind == "staging-failed"
    assert "nothing was attached" in outcome.detail


@pytest.mark.asyncio
async def test_interrupted_call_stays_unknown_and_warns_about_retry(
    tmp_path: Path,
) -> None:
    """An unknown outcome must never read as a failure — or a success.

    Calls through a source that never connected: the real
    :meth:`LiveSource.call` unknown path (no connection, nothing sent),
    with no network involved.
    """
    target = write_canary_file(tmp_path)
    source = live_source_stub()
    outcome = await attachments.attach_file(source, target)

    assert outcome.status == "unknown"
    assert outcome.kind == "unknown"
    assert "second" in outcome.detail


def live_source_stub() -> LiveSource:
    return LiveSource(
        AttachTarget.from_url("ws://example.invalid/api/ws"),
        StubProvider(),
        reconnect_delays=FAST_RETRIES,
    )


@pytest.mark.asyncio
async def test_excluded_pdf_is_refused_talaria_side_before_any_wire(
    attachment_gateway: tuple[StubGateway, list[tuple[str, dict[str, Any]]]],
    tmp_path: Path,
) -> None:
    """D6 excludes portable-document format for v0.6.1, and file.attach
    would accept one — so the refusal lives here, explicit about whose
    decision it is. Extension and magic bytes both trigger it, so a
    renamed PDF does not slip through as a text file."""
    gateway, seen = attachment_gateway
    deck = tmp_path / "deck.pdf"
    deck.write_bytes(b"%PDF-1.4\n" + b"\x00" * 64)
    renamed = tmp_path / "deck.txt"
    renamed.write_bytes(b"%PDF-1.4\n" + b"\x00" * 64)
    source = live_source(gateway)
    await source.start()
    try:
        await until(lambda: source.state == "connected")
        refused = await attachments.attach_file(source, deck)
        renamed_refused = await attachments.attach_file(source, renamed)
    finally:
        await source.close()

    for outcome in (refused, renamed_refused):
        assert outcome.status == "error"
        assert outcome.kind == "unsupported-type"
        assert "was not sent" in outcome.detail
        assert "excludes portable-document format" in outcome.detail
        assert outcome.prompt_text is None
    assert seen == []


def test_mime_labels_by_extension_without_accepting_anything() -> None:
    assert attachments.mime_for_filename("notes.txt") == "text/plain"
    assert attachments.mime_for_filename("photo.JPG") == "image/jpeg"
    assert attachments.mime_for_filename("deck.pdf") == "application/pdf"
    assert attachments.mime_for_filename("no-extension") == "application/octet-stream"


def test_data_url_shape_matches_the_gateway_decoder() -> None:
    url = attachments.build_data_url("text/plain", b"hello\n")
    assert url == "data:text/plain;base64,aGVsbG8K"
    payload = url.split(",", 1)[1]
    assert base64.b64decode(payload) == b"hello\n"


def test_taxonomy_quotes_the_gateway_and_names_retry() -> None:
    kind, detail = attachments.classify_gateway_error("file.attach", 4015, "path missing")
    assert kind == "invalid-request"
    assert "path missing" in detail
    kind, detail = attachments.classify_gateway_error(
        "image.attach_bytes", 4017, "data is not valid base64"
    )
    assert kind == "bad-payload"
    assert "data is not valid base64" in detail
    kind, detail = attachments.classify_gateway_error("file.attach", 5999, "boom")
    assert kind == "gateway-error"
    assert "5999" in detail
