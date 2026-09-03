"""R19/R20: the payload contains exactly KTD5's frozen v1 field set.

Also the "contract doc and contract.py agree field for field" check the U6
Verification clause names.
"""

from __future__ import annotations

import re
from pathlib import Path

import pytest

from talaria.domain.projection import StatusPayload
from talaria.status.contract import (
    FROZEN_TOP_LEVEL_FIELDS,
    SCRIPT_OUTPUT_VERSION,
    ScriptDocumentError,
    ScriptRow,
    assert_frozen_shape,
    encode_payload,
    parse_script_rows,
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


def test_frozen_shape_still_rejects_actually_unknown_shapes_loudly() -> None:
    """The v1 stdin guard is not loosened by the v2 output intake.

    Neither a drifted field set nor a document naming the *output*
    version passes the frozen shape: the two version namespaces (stdin
    payload vs script-output document) must never be confused.
    """
    drifted = _payload().to_json_dict()
    drifted["rows"] = []
    with pytest.raises(AssertionError, match="field drift"):
        assert_frozen_shape(drifted)

    output_versioned = _payload().to_json_dict()
    output_versioned["version"] = SCRIPT_OUTPUT_VERSION
    with pytest.raises(AssertionError, match="version drift"):
        assert_frozen_shape(output_versioned)


# ── #125 U1: the versioned script-output intake ──────────────────────────


def test_v1_plain_text_is_not_a_document_and_renders_through_the_text_path() -> None:
    """Non-JSON stdout returns ``None``: the v1 literal-row path, unchanged."""
    assert parse_script_rows("branch: main\ntests: 296\n") is None
    assert parse_script_rows("") is None


@pytest.mark.parametrize("scalar", ["123", "null", "true", "[1, 2]", '"just a string"'])
def test_scalar_and_array_json_stay_literal_v1_rows(scalar: str) -> None:
    """Only a JSON *object* claims structure. A v1 script printing a bare
    number keeps rendering that number rather than tripping a document
    protocol it never opted into."""
    assert parse_script_rows(scalar) is None


def test_valid_version_two_document_yields_script_rows() -> None:
    rows = parse_script_rows(
        '{"version": 2, "rows": ["plain", {"text": "colored", "color": "warning"}]}'
    )
    assert rows == (ScriptRow(text="plain"), ScriptRow(text="colored", color="warning"))


def test_row_color_defaults_to_text_and_empty_shapes_are_kept() -> None:
    assert parse_script_rows('{"version": 2, "rows": [{"text": "x"}]}') == (
        ScriptRow(text="x", color="text"),
    )
    # An explicit empty list is a good render (clear the bar), not an error.
    assert parse_script_rows('{"version": 2, "rows": []}') == ()
    # An empty-string row is literal content, not a missing row.
    assert parse_script_rows('{"version": 2, "rows": [""]}') == (ScriptRow(text=""),)


def test_pretty_printed_multiline_document_parses() -> None:
    """The intake runs on the whole stdout text, so a document spanning
    lines is one document — the runner must not split it into v1 rows."""
    rows = parse_script_rows('{\n  "version": 2,\n  "rows": ["a", "b"]\n}\n')
    assert rows == (ScriptRow(text="a"), ScriptRow(text="b"))


@pytest.mark.parametrize(
    "document",
    [
        '{"rows": []}',
        '{"version": 1, "rows": []}',
        '{"version": 3, "rows": []}',
        '{"version": "2", "rows": []}',
        '{"version": null, "rows": []}',
        '{"version": 2}',
        '{"version": 2, "rows": null}',
        '{"version": 2, "rows": "nope"}',
        '{"version": 2, "rows": [null]}',
        '{"version": 2, "rows": [42]}',
        '{"version": 2, "rows": [{}]}',
        '{"version": 2, "rows": [{"text": 42}]}',
        '{"version": 2, "rows": [{"text": "x", "color": 42}]}',
        '{"version": 2, "rows": [], "extra": 1}',
        '{"version": 2, "rows": [{"text": "x", "bogus": 1}]}',
    ],
)
def test_unknown_shapes_raise_rather_than_render_junk(document: str) -> None:
    """Missing/jumped versions, null or wrong-typed fields, and unknown
    keys at either level are all loud — junk never reaches the bar."""
    with pytest.raises(ScriptDocumentError):
        parse_script_rows(document)
