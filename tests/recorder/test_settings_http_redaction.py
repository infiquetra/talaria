"""A1-9 — secret handling across the HTTP recording boundary.

``PUT /api/env`` carries a plaintext credential, ``POST /api/env/reveal``
returns one, and provider/vault/OAuth/token payloads do the same. Key-name
heuristics are not enough on this boundary: the reveal response is
``{"value": ...}`` under an innocent key, so denial is by route (KTD8),
independent of key names — with the existing JSON-RPC/frame redaction kept
as the defense-in-depth layer underneath.

Interface under test (unimplemented until CFG B2, ``dev-3``):

* ``talaria.domain.redaction.is_sensitive_http_route(method, path)`` —
  route denial independent of key-name heuristics.
* ``talaria.recorder.redact.redact_http_body(*, method, path, body)`` —
  route-denied bodies withheld whole with a recorded reason; all other
  bodies walked with the frame key-name net. Never mutates its input.
* ``talaria.recorder.redact.http_response_persistable(*, method, path)`` —
  ``False`` for reveal and token/ticket responses, which must never reach
  disk even in redacted form.
* ``talaria.domain.settings.project_secret_row`` — the write-only secret
  row: its inputs admit no plaintext, so summaries cannot leak one.

Deliberately *not* widened here: ``redact_url``'s query-key set stays
exactly ``token``/``ticket``/``internal``, pinned by the frozen
TypeScript-equivalence harness. Route denial is new API with no equivalence
surface.

Every canary below is an in-memory random value per the secret evidence
policy; none is written to disk outside the test process.
"""

from __future__ import annotations

import importlib
import inspect
import json
import secrets
from typing import Any

import pytest


def _require_module(name: str) -> Any:
    try:
        return importlib.import_module(name)
    except ImportError as exc:
        pytest.fail(f"unimplemented interface {name} (CFG v0.6.2 B2): {exc}")


def _require_attr(module: Any, name: str) -> Any:
    try:
        return getattr(module, name)
    except AttributeError:
        pytest.fail(
            f"unimplemented interface {module.__name__}.{name} (CFG v0.6.2 B2)"
        )


def _redaction_policy() -> Any:
    return _require_module("talaria.domain.redaction")


def _recorder_redact() -> Any:
    return _require_module("talaria.recorder.redact")


def _canary() -> str:
    return f"canary-{secrets.token_hex(8)}"


# ── route denial is by route, not by key name ────────────────────────────


@pytest.mark.parametrize(
    ("method", "path"),
    [
        ("PUT", "/api/env"),
        ("DELETE", "/api/env"),
        ("POST", "/api/env/reveal"),
        ("POST", "/api/providers/validate"),
        ("POST", "/api/providers/custom-endpoints/validate"),
        ("POST", "/api/providers/oauth/example/start"),
        ("POST", "/auth/native/token"),
        ("POST", "/auth/native/refresh"),
        ("POST", "/api/auth/ws-ticket"),
        ("POST", "/api/credentials/pool/rotate"),
    ],
)
def test_credential_routes_are_denied_independent_of_key_names(
    method: str, path: str
) -> None:
    policy = _redaction_policy()

    assert _require_attr(policy, "is_sensitive_http_route")(method, path) is True


@pytest.mark.parametrize(
    ("method", "path"),
    [
        ("GET", "/api/env"),
        ("GET", "/api/config"),
        ("PUT", "/api/config"),
        ("GET", "/api/config/schema"),
        ("POST", "/api/model/set"),
        ("GET", "/api/model/info"),
        ("GET", "/api/profiles"),
        ("POST", "/api/gateway/restart"),
        ("GET", "/api/health"),
        ("GET", "/api/status"),
    ],
)
def test_ordinary_routes_are_not_denied(method: str, path: str) -> None:
    """Denial is not a prefix on ``/api``: config/model/profile/gateway
    traffic stays recordable (with the key-name net still applied)."""
    policy = _redaction_policy()

    assert _require_attr(policy, "is_sensitive_http_route")(method, path) is False


# ── env PUT and reveal responses through the recorder ────────────────────


def test_an_env_put_body_is_withheld_whole_with_a_recorded_reason() -> None:
    recorder = _recorder_redact()
    redacted_marker = _require_attr(recorder, "REDACTED")
    canary = _canary()
    body = {"key": "EXAMPLE_API_KEY", "value": canary, "profile": "alpha-fixture"}

    result = _require_attr(recorder, "redact_http_body")(
        method="PUT", path="/api/env", body=body
    )

    assert canary not in json.dumps(result.frame)
    assert result.frame == redacted_marker
    assert any(
        entry.reason == "deny-route:PUT /api/env" for entry in result.redactions
    )
    # The input is never mutated: the caller still holds the live body.
    assert body["value"] == canary


def test_a_reveal_response_is_withheld_despite_its_innocent_key() -> None:
    """``{"value": ...}`` defeats the key-name net by design, which is why
    this boundary denies by route."""
    recorder = _recorder_redact()
    redacted_marker = _require_attr(recorder, "REDACTED")
    canary = _canary()

    result = _require_attr(recorder, "redact_http_body")(
        method="POST", path="/api/env/reveal", body={"value": canary}
    )

    assert canary not in json.dumps(result.frame)
    assert result.frame == redacted_marker
    assert any(
        entry.reason == "deny-route:POST /api/env/reveal"
        for entry in result.redactions
    )


def test_a_config_put_keeps_its_keys_but_loses_a_credential_shaped_value() -> None:
    """The non-denied path still walks the key-name net: ordinary config
    values stay readable while a credential-shaped key is withheld."""
    recorder = _recorder_redact()
    redacted_marker = _require_attr(recorder, "REDACTED")
    canary = _canary()

    result = _require_attr(recorder, "redact_http_body")(
        method="PUT",
        path="/api/config",
        body={"config": {"agent": {"max_turns": 40}, "model": {"api_key": canary}}},
    )

    assert canary not in json.dumps(result.frame)
    assert result.frame["config"]["agent"] == {"max_turns": 40}
    assert result.frame["config"]["model"] == {"api_key": redacted_marker}


def test_a_token_response_body_is_withheld_whole() -> None:
    recorder = _recorder_redact()
    canary = _canary()

    result = _require_attr(recorder, "redact_http_body")(
        method="POST",
        path="/auth/native/token",
        body={"access_token": canary, "refresh_token": canary, "expires_in": 900},
    )

    assert canary not in json.dumps(result.frame)
    assert any(
        entry.reason == "deny-route:POST /auth/native/token"
        for entry in result.redactions
    )


# ── reveal and token responses are never persisted ───────────────────────


@pytest.mark.parametrize(
    ("method", "path"),
    [
        ("POST", "/api/env/reveal"),
        ("POST", "/auth/native/token"),
        ("POST", "/auth/native/refresh"),
        ("POST", "/api/auth/ws-ticket"),
    ],
)
def test_reveal_and_token_responses_are_never_persisted(
    method: str, path: str
) -> None:
    """A reveal response contains only the secret: even a redacted record
    carries no information, and any redaction bug would leak. The recording
    layer drops the frame instead of writing a marker."""
    recorder = _recorder_redact()

    assert (
        _require_attr(recorder, "http_response_persistable")(
            method=method, path=path
        )
        is False
    )


@pytest.mark.parametrize(
    ("method", "path"),
    [
        ("PUT", "/api/env"),
        ("DELETE", "/api/env"),
        ("GET", "/api/env"),
        ("PUT", "/api/config"),
        ("GET", "/api/status"),
    ],
)
def test_denied_request_bodies_still_persist_as_markers(
    method: str, path: str
) -> None:
    """The positive control: route denial withholds the *body* but the
    redacted record persists, so the corpus shows the request happened."""
    recorder = _recorder_redact()

    assert (
        _require_attr(recorder, "http_response_persistable")(
            method=method, path=path
        )
        is True
    )


# ── the write-only secret row and summary ────────────────────────────────


def test_a_secret_row_admits_no_plaintext_and_renders_masked() -> None:
    """D6: secrets are write-only in Talaria. The projector's inputs are
    ``is_set`` + the server's redacted value — there is no parameter a
    plaintext could arrive through, so rows and summaries cannot leak one."""
    settings = _require_module("talaria.domain.settings")
    canary = _canary()

    row = _require_attr(settings, "project_secret_row")(
        key="EXAMPLE_API_KEY", is_set=True, redacted_value="sk-…abcd"
    )

    assert canary not in repr(row)
    assert row.key == "EXAMPLE_API_KEY"
    assert row.is_set is True
    assert row.masked == "sk-…abcd"
    assert row.provenance == "saved"
    assert row.read_only is False


def test_an_unset_secret_row_says_unset_without_a_value() -> None:
    settings = _require_module("talaria.domain.settings")

    row = _require_attr(settings, "project_secret_row")(
        key="EXAMPLE_API_KEY", is_set=False, redacted_value=""
    )

    assert row.is_set is False
    assert row.masked == ""
    assert row.provenance == "default"


def test_a_save_summary_names_the_secret_key_and_changed_never_the_value() -> None:
    settings = _require_module("talaria.domain.settings")
    project = _require_attr(settings, "project_save_summary")
    result_type = _require_attr(settings, "FieldSaveResult")
    canary = _canary()

    summary = project(
        results=(
            result_type(key="EXAMPLE_API_KEY", outcome="saved"),
            result_type(key="agent.max_turns", outcome="saved"),
        )
    )

    assert canary not in repr(summary)
    saved = next(
        group for group in summary.groups if group.effect == "saved"
    )
    assert "EXAMPLE_API_KEY" in saved.keys
    assert "agent.max_turns" in saved.keys
    assert saved.details == ()


# ── P2-2: rate-limit error records keep status, never values ──────────────
#
# A 429 reveal error carries no secret — only rate metadata the evidence
# rules explicitly permit ("route/status metadata"). Denying it whole would
# erase the rate-limit signal P2-2 must prove; recording it raw risks a
# value-shaped field. Status-aware redaction preserves the status wording
# while withholding any value, and only error metadata persists.


def _require_status_support(recorder: Any) -> None:
    for name in ("redact_http_body", "http_response_persistable"):
        params = inspect.signature(getattr(recorder, name)).parameters
        if "status" not in params:
            pytest.fail(
                f"unimplemented interface talaria.recorder.redact.{name}"
                "(status=...) (P2-2)"
            )


def test_a_429_reveal_error_preserves_status_wording_without_values() -> None:
    recorder = _recorder_redact()
    _require_status_support(recorder)
    canary = _canary()

    result = _require_attr(recorder, "redact_http_body")(
        method="POST",
        path="/api/env/reveal",
        body={"detail": "Too many reveal requests. Try again shortly."},
        status=429,
    )

    assert "Too many reveal requests" in json.dumps(result.frame)
    assert canary not in json.dumps(result.frame)
    assert any(
        entry.reason == "deny-route:POST /api/env/reveal"
        for entry in result.redactions
    )


def test_a_value_shaped_field_in_an_error_body_is_still_withheld() -> None:
    """Status-awareness must not become a bypass: value-shaped content in
    an error body is withheld even while the status wording survives."""
    recorder = _recorder_redact()
    _require_status_support(recorder)
    redacted_marker = _require_attr(recorder, "REDACTED")
    canary = _canary()

    result = _require_attr(recorder, "redact_http_body")(
        method="POST",
        path="/api/env/reveal",
        body={"detail": "Too many reveal requests.", "value": canary},
        status=429,
    )

    assert canary not in json.dumps(result.frame)
    assert result.frame.get("value") == redacted_marker
    assert "Too many reveal requests" in json.dumps(result.frame)


@pytest.mark.parametrize(
    ("status", "persistable"),
    [(429, True), (200, False), (None, False)],
)
def test_only_error_metadata_persists_for_reveal_responses(
    status: int | None, persistable: bool
) -> None:
    """Successful reveal responses never reach disk (A1-9, unchanged);
    429 error metadata persists so the rate-limit proof is recordable."""
    recorder = _recorder_redact()
    _require_status_support(recorder)

    assert (
        _require_attr(recorder, "http_response_persistable")(
            method="POST", path="/api/env/reveal", status=status
        )
        is persistable
    )


# ── P3-2: denial is per exchange, never a sticky state ─────────────────────
#
# Target switches interleave denied reveal traffic with recordable config
# traffic for the newly selected profile. Denial must be decided fresh per
# exchange: a denied reveal must not poison the redactor into withholding
# the next target's ordinary bodies.


def test_denial_does_not_leak_across_exchanges() -> None:
    recorder = _recorder_redact()
    redacted_marker = _require_attr(recorder, "REDACTED")
    canary = _canary()

    denied = _require_attr(recorder, "redact_http_body")(
        method="POST", path="/api/env/reveal", body={"value": canary}
    )
    assert denied.frame == redacted_marker

    kept = _require_attr(recorder, "redact_http_body")(
        method="GET",
        path="/api/config",
        body={"agent": {"max_turns": 40}, "model": {"name": "example-large"}},
    )

    assert kept.frame == {
        "agent": {"max_turns": 40},
        "model": {"name": "example-large"},
    }
    assert kept.redactions == []
