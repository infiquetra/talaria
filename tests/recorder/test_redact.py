"""Redaction boundary tests — ported 1:1 from ``src/record/redact.test.ts``
(R27), plus the Python-only superset extension required by KTD6/PC10/R28
(``ticket``/``internal`` query-parameter denial) and the divergence-pinning
test the plan calls for.

These frames mirror the shapes the Hermes gateway actually dispatches, read
from ``tui_gateway/methods_prompt.py`` at Hermes ``7f4d15515``. If a test here
fails, a credential is one commit away from being written to disk.
"""

from __future__ import annotations

import json
from typing import Any

import pytest

from talaria.recorder.redact import (
    REDACTED,
    is_suspicious_key,
    redact_frame,
    redact_url,
)

# ---------------------------------------------------------------------------
# Hermes's four "blocking bridges" and the params key each one carries the
# sensitive value in. Mirrors the parametrize list in the gateway's own
# `tests/tui_gateway/test_protocol.py`, which groups all four as sensitive
# prompts. Kept as data so adding a bridge here is the whole change.
# ---------------------------------------------------------------------------
BLOCKING_BRIDGES: tuple[tuple[str, str], ...] = (
    ("sudo.respond", "password"),
    ("secret.respond", "value"),
    ("terminal.read.respond", "text"),
    ("clarify.respond", "answer"),
)


@pytest.mark.parametrize("method,key", BLOCKING_BRIDGES)
def test_withholds_the_sensitive_value_on_blocking_bridge(method: str, key: str) -> None:
    canary = f"canary-for-{method}"
    result = redact_frame(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": {"request_id": "r-1", key: canary},
        }
    )

    assert canary not in json.dumps(result.frame)
    assert result.frame["params"][key] == REDACTED
    assert any(
        r.path == f"params.{key}" and r.reason == f"deny-set:{method}" for r in result.redactions
    )


# ---------------------------------------------------------------------------
# Real key names harvested from the Hermes gateway at `7f4d15515`, not
# invented examples. Seventeen distinct key names there contain "token" and
# exactly one is a credential, so this boundary is where over-redaction gets
# caught.
# ---------------------------------------------------------------------------
@pytest.mark.parametrize(
    "key", ["token", "access_token", "api_key", "password", "authorization"]
)
def test_key_name_net_withholds(key: str) -> None:
    assert is_suspicious_key(key) is True


@pytest.mark.parametrize(
    "key",
    [
        "max_tokens",
        "input_tokens",
        "output_tokens",
        "reasoning_tokens",
        "session_total_tokens",
        "tokens_per_delta",
        "token_estimate",
        "token_line",
        "session_key",
        "idempotency_key",
    ],
)
def test_key_name_net_preserves_accounting_data(key: str) -> None:
    assert is_suspicious_key(key) is False


def test_keeps_a_usage_frame_numerically_intact() -> None:
    result = redact_frame(
        {
            "method": "event",
            "params": {
                "type": "usage",
                "payload": {"input_tokens": 1200, "output_tokens": 340, "max_tokens": 4096},
            },
        }
    )

    assert result.redactions == []
    assert result.frame["params"]["payload"] == {
        "input_tokens": 1200,
        "output_tokens": 340,
        "max_tokens": 4096,
    }


def test_withholds_the_plaintext_sudo_password() -> None:
    result = redact_frame(
        {
            "jsonrpc": "2.0",
            "id": 7,
            "method": "sudo.respond",
            "params": {"request_id": "r-1", "password": "hunter2"},
        }
    )

    assert "hunter2" not in json.dumps(result.frame)
    assert result.frame["params"]["password"] == REDACTED
    assert len(result.redactions) == 1
    assert result.redactions[0].path == "params.password"
    assert result.redactions[0].reason == "deny-set:sudo.respond"


def test_withholds_a_secret_value() -> None:
    result = redact_frame(
        {
            "jsonrpc": "2.0",
            "method": "secret.respond",
            "params": {"request_id": "r-2", "value": "sk-live-abc123"},
        }
    )

    assert "sk-live-abc123" not in json.dumps(result.frame)
    assert len(result.redactions) == 1


def test_withholds_a_captured_terminal_buffer() -> None:
    result = redact_frame(
        {
            "method": "terminal.read.respond",
            "params": {"request_id": "r-3", "text": "$ cat ~/.env\nAPI_KEY=live"},
        }
    )

    assert "live" not in json.dumps(result.frame)


def test_keeps_non_credential_fields_on_a_denied_method_intact() -> None:
    result = redact_frame(
        {"method": "sudo.respond", "params": {"request_id": "r-4", "password": "s3cret"}}
    )

    assert result.frame["params"]["request_id"] == "r-4"


def test_leaves_ordinary_traffic_completely_untouched() -> None:
    original = {
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "type": "assistant_delta",
            "session_id": "s-1",
            "payload": {"text": "hello"},
        },
    }

    result = redact_frame(original)

    assert result.frame == original
    assert result.redactions == []


def test_does_not_mutate_the_frame_it_was_given() -> None:
    original: dict[str, Any] = {"method": "sudo.respond", "params": {"password": "hunter2"}}
    redact_frame(original)
    assert original["params"]["password"] == "hunter2"


def test_catches_a_credential_on_an_unknown_method_via_the_key_name_net() -> None:
    # Protocol drift: a method the deny-set has never heard of.
    result = redact_frame(
        {"method": "future.unknown.respond", "params": {"api_key": "should-not-survive"}}
    )

    assert "should-not-survive" not in json.dumps(result.frame)
    assert result.redactions[0].reason == "suspicious-key"


def test_catches_a_credential_nested_at_arbitrary_depth() -> None:
    result = redact_frame(
        {
            "method": "event",
            "params": {"payload": {"config": {"provider": {"token": "deep-secret"}}}},
        }
    )

    assert "deep-secret" not in json.dumps(result.frame)


def test_catches_a_credential_inside_an_array() -> None:
    result = redact_frame(
        {"method": "batch", "params": {"items": [{"ok": 1}, {"password": "in-an-array"}]}}
    )

    assert "in-an-array" not in json.dumps(result.frame)


def test_survives_frames_that_are_not_objects() -> None:
    assert redact_frame(None).frame is None
    assert redact_frame("plain string").frame == "plain string"
    assert redact_frame(42).frame == 42


def test_redact_url_withholds_the_gateway_attach_token() -> None:
    url = redact_url("ws://127.0.0.1:8765/api/ws?token=abc123&session=s-1")

    assert "abc123" not in url
    assert "session=s-1" in url


def test_redact_url_leaves_a_clean_url_alone() -> None:
    url = "ws://127.0.0.1:8765/api/ws"
    assert redact_url(url) == url


def test_redact_url_withholds_anything_it_cannot_parse_rather_than_guessing() -> None:
    assert redact_url("not a url") == REDACTED


def test_is_suspicious_key_matches_the_credential_shaped_names() -> None:
    for key in ["password", "passphrase", "apiKey", "api_key", "authToken", "secret"]:
        assert is_suspicious_key(key) is True


def test_is_suspicious_key_does_not_match_ordinary_protocol_fields() -> None:
    for key in ["session_id", "method", "params", "text", "type"]:
        assert is_suspicious_key(key) is False


@pytest.mark.parametrize("key", ["authToken", "apiKey", "accessToken", "privateKey"])
def test_camel_case_keys_withholds(key: str) -> None:
    assert is_suspicious_key(key) is True


@pytest.mark.parametrize("key", ["maxTokens", "inputTokens", "outputTokens", "sessionKey"])
def test_camel_case_keys_preserves(key: str) -> None:
    assert is_suspicious_key(key) is False


# ---------------------------------------------------------------------------
# Python-only superset (KTD6, PC10, R28). `redactUrl`'s TypeScript
# `SENSITIVE_KEY_PATTERNS` withhold the bare `token` query key but match
# neither `ticket` nor `internal` -- the other two credential forms the
# Hermes attach protocol accepts (KTD11). The Python port denies all three by
# name; this divergence is pinned by a test rather than left to drift.
# ---------------------------------------------------------------------------
@pytest.mark.parametrize("key", ["token", "ticket", "internal"])
def test_redact_url_withholds_every_attach_credential_form(key: str) -> None:
    """AE3/PC10: a synthetic endpoint URL bearing each of `?token=`, `?ticket=`,
    and `?internal=` is withheld in all three cases -- the enumerated
    Python-side superset."""
    url = redact_url(f"ws://127.0.0.1:8765/api/ws?{key}=super-secret-credential&session=s-1")

    assert "super-secret-credential" not in url
    assert "session=s-1" in url


def test_python_redactor_is_a_strict_superset_of_the_typescript_deny_set() -> None:
    """The TypeScript reference withholds only `token`; ticket and internal
    are additional Python-side denials, drawn from the enumerated set in
    `talaria/recorder/redact.py`'s module docstring (KTD6)."""
    from talaria.recorder.redact import URL_ONLY_DENIED_QUERY_KEYS

    assert URL_ONLY_DENIED_QUERY_KEYS == frozenset({"ticket", "internal"})

    # `token` is caught by the shared key-name net (both implementations);
    # `ticket` and `internal` are caught only by the Python-only addition.
    assert is_suspicious_key("token") is True
    assert is_suspicious_key("ticket") is False
    assert is_suspicious_key("internal") is False


def test_model_save_key_api_key_is_caught_by_the_key_name_net() -> None:
    """R28's outbound synthetic-credential evidence: `model.save_key` is not
    in the explicit deny-set (it needs no entry -- see redact.py's module
    docstring) but its `params.api_key` is still withheld, by the same net
    that catches protocol drift."""
    result = redact_frame(
        {
            "method": "model.save_key",
            "params": {"slug": "deepseek", "api_key": "sk-should-not-survive"},
        }
    )

    assert "sk-should-not-survive" not in json.dumps(result.frame)
    assert result.frame["params"]["slug"] == "deepseek"
    assert any(r.reason == "suspicious-key" for r in result.redactions)


def test_approval_respond_is_deliberately_not_in_the_deny_set() -> None:
    """`approval.respond` carries a choice, not a credential (U2 approach
    notes): it must not be redacted, or KTD6's exact-`redactions` equivalence
    with the TypeScript reference breaks."""
    result = redact_frame(
        {
            "method": "approval.respond",
            "params": {"request_id": "r-9", "approved": True, "reason": "looks fine"},
        }
    )

    assert result.redactions == []
    assert result.frame["params"]["approved"] is True


# ── Leaks found by adversarial probe, 2026-08-03 ──────────────────────
#
# Each of these three put a credential on disk through the writer. They are
# kept as separate named tests rather than one parametrized sweep because the
# three causes are unrelated and a future edit is likely to reopen exactly one.


def test_an_oddly_cased_key_is_still_caught() -> None:
    """``_normalize_key`` rewrites ``ApIkEy`` to ``ap_ik_ey`` and misses it.

    The camel-boundary normalizer inserts separators mid-word when a name is
    cased unusually, destroying the name it is meant to canonicalize.
    """
    assert is_suspicious_key("ApIkEy")
    assert is_suspicious_key("APIKey")
    assert is_suspicious_key("api_key")


def test_a_separator_other_than_dash_or_underscore_is_still_caught() -> None:
    """A space is not in the ``[-_]`` class the anchored patterns use."""
    assert is_suspicious_key("api key")
    assert is_suspicious_key("api.key")
    assert is_suspicious_key("api key")


def test_a_credential_url_under_an_innocent_key_is_withheld() -> None:
    """The key-name net reads names only; KTD11 puts the credential in a value."""
    for query_key in ("token", "ticket", "internal"):
        result = redact_frame(
            {"method": "x", "params": {"url": f"ws://h/api/ws?{query_key}=SECRET"}}
        )
        assert "SECRET" not in json.dumps(result.frame), query_key
        assert [r.reason for r in result.redactions] == ["url-credential"]


def test_a_harmless_url_is_recorded_untouched() -> None:
    """Blanket redaction of every string would make the corpus useless."""
    frame = {"method": "x", "params": {"url": "https://example.com/docs?page=2&sort=asc"}}
    result = redact_frame(frame)
    assert result.frame["params"]["url"] == "https://example.com/docs?page=2&sort=asc"
    assert result.redactions == []


def test_the_widened_net_still_preserves_the_usage_counters() -> None:
    """Squashing separators must not turn ``max_tokens`` into a credential.

    Over-redaction is a different failure, not the safe direction: these
    counters are what the corpus exists to study.
    """
    for name in (
        "max_tokens",
        "input_tokens",
        "output_tokens",
        "session_total_tokens",
        "tokens_per_delta",
        "maxTokens",
    ):
        assert not is_suspicious_key(name), name
