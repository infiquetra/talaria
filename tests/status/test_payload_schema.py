"""R19/R20: the payload contains exactly KTD5's frozen v1 field set.

Also the "contract doc and contract.py agree field for field" check the U6
Verification clause names.
"""

from __future__ import annotations

import re
from pathlib import Path

from talaria.domain.projection import StatusPayload
from talaria.status.contract import (
    FROZEN_TOP_LEVEL_FIELDS,
    assert_frozen_shape,
    encode_payload,
)

_REPO_ROOT = Path(__file__).resolve().parents[2]
_CONTRACT_DOC = _REPO_ROOT / "docs" / "formats" / "status-line.md"


def _payload(**overrides: object) -> StatusPayload:
    fields: dict[str, object] = {
        "version": 1,
        "mode": "replay",
        "connection": "connected",
        "session_id": "sess-1",
        "session_title": "a title",
        "turn": "idle",
        "pending_prompts": 0,
        "subagents_active": 0,
        "subagents_terminal": 0,
        "input_tokens": None,
        "output_tokens": None,
    }
    fields.update(overrides)
    return StatusPayload(**fields)  # type: ignore[arg-type]


def test_document_has_exactly_the_frozen_field_set() -> None:
    document = _payload().to_json_dict()
    assert_frozen_shape(document)  # raises AssertionError on any drift


def test_version_field_is_first_and_equal_to_one() -> None:
    document = _payload().to_json_dict()
    assert next(iter(document)) == "version"
    assert document["version"] == 1


def test_no_extra_fields_even_with_a_pending_secret_prompt() -> None:
    """A pending sudo/secret prompt must not leak a credential-bearing field.

    ``pending_prompts`` is a bare count (R20) — the payload has no field that
    *could* carry the prompt's own content, so this asserts the shape stays
    exactly the frozen set even when the domain state that produced it is
    mid-credential-prompt.
    """
    document = _payload(pending_prompts=1, turn="waiting").to_json_dict()
    assert_frozen_shape(document)
    assert document["pending_prompts"] == 1
    # No key anywhere in the document contains a credential-shaped name.
    assert "password" not in document
    assert "secret" not in document
    assert "token" not in document


def test_usage_null_when_unobserved_and_nested_when_present() -> None:
    unobserved = _payload().to_json_dict()
    assert unobserved["usage"] is None

    observed = _payload(input_tokens=10, output_tokens=20).to_json_dict()
    assert observed["usage"] == {"input_tokens": 10, "output_tokens": 20}


def test_encode_payload_is_utf8_json_with_trailing_newline() -> None:
    raw = encode_payload(_payload())
    assert raw.endswith(b"\n")
    import json

    decoded = json.loads(raw.decode("utf-8"))
    assert_frozen_shape(decoded)


def test_contract_doc_field_list_matches_the_serializer() -> None:
    """`docs/formats/status-line.md` and `contract.py` must name the same fields.

    Cheap, deliberately non-exhaustive: the doc must at least name every
    top-level field once (as an inline-code token), so a field rename in
    ``contract.py``/``projection.py`` without a doc update fails this test
    instead of shipping silently.
    """
    text = _CONTRACT_DOC.read_text(encoding="utf-8")
    for field_name in FROZEN_TOP_LEVEL_FIELDS:
        pattern = rf"`{re.escape(field_name)}`"
        assert re.search(pattern, text), f"{field_name!r} not documented in {_CONTRACT_DOC}"
