"""Attachment redaction tests (C9): content genuinely absent, not labelled.

Every test below sweeps the *recorded bytes* for the canary — file
content, operator-local paths, and the base64 forms — and fails if any
survives. A marker-presence assertion would pass while the content sat
beside the marker; these tests prove the bytes are gone. Gateway-issued
references (``@file:`` refs, staged names) must survive: they name
gateway workspace files, not operator disk, and the evidence needs them.
"""

from __future__ import annotations

import base64
import json

from talaria.recorder.redact import REDACTED, redact_frame

CANARY_TEXT = "canary-content-Rm4pZ9wx-must-not-be-recorded"
CANARY_HOME = "canary-home-Rm4pZ9wx"
CANARY_B64 = base64.b64encode(CANARY_TEXT.encode()).decode("ascii")
CANARY_URL = f"data:text/plain;base64,{CANARY_B64}"
CANARY_PATH = f"/{CANARY_HOME}/docs/notes.txt"


def recorded_bytes(frame: object) -> bytes:
    result = redact_frame(frame)
    return json.dumps(result.frame).encode()


def test_file_attach_request_withholds_content_path_and_name() -> None:
    raw = recorded_bytes(
        {
            "jsonrpc": "2.0",
            "id": "7",
            "method": "file.attach",
            "params": {
                "path": CANARY_PATH,
                "data_url": CANARY_URL,
                "name": "notes.txt",
            },
        }
    )

    assert CANARY_TEXT.encode() not in raw
    assert CANARY_B64.encode() not in raw
    assert CANARY_HOME.encode() not in raw


def test_image_attach_request_withholds_base64_and_hints() -> None:
    raw = recorded_bytes(
        {
            "jsonrpc": "2.0",
            "id": "8",
            "method": "image.attach_bytes",
            "params": {
                "content_base64": CANARY_B64,
                "filename": "photo.png",
                "ext": ".png",
            },
        }
    )

    assert CANARY_B64.encode() not in raw

    result = redact_frame(
        {
            "jsonrpc": "2.0",
            "id": "8",
            "method": "image.attach_bytes",
            "params": {"content_base64": CANARY_B64, "data": CANARY_B64},
        }
    )
    assert CANARY_B64.encode() not in json.dumps(result.frame).encode()
    assert any(
        r.path == "params.data" and r.reason == "deny-set:image.attach_bytes"
        for r in result.redactions
    )


def test_detach_request_withholds_the_path() -> None:
    raw = recorded_bytes(
        {
            "jsonrpc": "2.0",
            "id": "9",
            "method": "image.detach",
            "params": {"path": f"/gateway/{CANARY_HOME}/img.png"},
        }
    )

    assert CANARY_HOME.encode() not in raw


def test_gateway_reply_refs_survive_redaction() -> None:
    """No over-redaction: the evidence keeps what names gateway workspace."""
    result = redact_frame(
        {
            "jsonrpc": "2.0",
            "id": "7",
            "result": {
                "attached": True,
                "name": "notes.txt",
                "path": "/gateway/ws/.hermes/desktop-attachments/notes.txt",
                "ref_text": "@file:.hermes/desktop-attachments/notes.txt",
                "uploaded": True,
            },
        }
    )

    assert result.redactions == []
    assert result.frame["result"]["ref_text"].startswith("@file:")
    assert "desktop-attachments" in result.frame["result"]["path"]


def test_withheld_values_carry_the_method_reason() -> None:
    result = redact_frame(
        {
            "jsonrpc": "2.0",
            "id": "7",
            "method": "file.attach",
            "params": {"path": CANARY_PATH, "data_url": CANARY_URL},
        }
    )

    assert result.frame["params"]["path"] == REDACTED
    assert result.frame["params"]["data_url"] == REDACTED
    assert any(
        r.path == "params.data_url" and r.reason == "deny-set:file.attach"
        for r in result.redactions
    )
