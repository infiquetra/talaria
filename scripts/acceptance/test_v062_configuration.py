"""Harness-owned checks for the v0.6.2 configuration evidence contract."""

from __future__ import annotations

import ast
import inspect
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import pytest

from scripts.acceptance.v062_configuration import (
    A1_LOCAL_CONTRACT_IDS,
    A1_REMOTE_REUSE_IDS,
    CLEANUP_RECORD_TYPE,
    CLEANUP_SCHEMA_VERSION,
    FORBIDDEN_MUTATIONS,
    LEDGER_SCHEMA_PATH,
    LIVE_CREDENTIALS_PATH_FLAG,
    LIVE_HERMES_FLAG,
    P2_HERMES_NAME_RE,
    P2_PROFILE_NAME_RE,
    TEST_SECRET_KEY,
    HarnessError,
    access_limitation,
    assert_request_allowed,
    blocked_row,
    disposable_profile_name,
    live_hermes_enabled,
    multiplexer_preflight_is_batch_gate,
    product_gateway_lifecycle_accepted,
    readonly_host_status_allowed,
    remote_reuse_map,
    require_legal_p2_name,
    scan_for_canaries,
    sha256_text,
    stage_name_set,
    switch_pairs,
    validate_cleanup_receipt,
    validate_coverage_document,
    validate_ledger_document,
    validate_ledger_row,
)

_COMMIT = "c87c7901e563346913a2764a57f6c4e47bf45f8f"
_OBSERVED = datetime(2026, 9, 16, 12, 0, tzinfo=UTC).isoformat().replace("+00:00", "Z")


def _valid_row(**overrides: Any) -> dict[str, Any]:
    row: dict[str, Any] = {
        "test_id": "A1-11",
        "status": "blocked",
        "talaria_version": "0.6.2-unreleased",
        "talaria_commit": _COMMIT,
        "hermes_version": "unverified",
        "connection_label": "local",
        "profile_label": "testA",
        "observed_at": _OBSERVED,
        "evidence_refs": [],
        "limitation": access_limitation("local"),
    }
    row.update(overrides)
    return row


def test_the_ledger_schema_file_is_present() -> None:
    assert LEDGER_SCHEMA_PATH.is_file()


def test_a_complete_blocked_row_validates() -> None:
    validate_ledger_row(_valid_row())


def test_passed_without_evidence_refs_is_rejected() -> None:
    with pytest.raises(HarnessError, match="evidence_refs"):
        validate_ledger_row(_valid_row(status="passed", evidence_refs=[]))


def test_passed_with_evidence_refs_is_accepted() -> None:
    validate_ledger_row(
        _valid_row(
            status="passed",
            evidence_refs=["docs/acceptance/v0.6.2/configuration/evidence/local/A1-11.json"],
            limitation="",
            hermes_version="0.21.3",
        )
    )


def test_a_ledger_document_rejects_an_empty_row_list() -> None:
    with pytest.raises(HarnessError, match="rows"):
        validate_ledger_document(
            {"schema_version": "talaria-v0.6.2-configuration-ledger-v1", "rows": []}
        )


def test_a_ledger_document_accepts_the_blocked_a1_11_20_set() -> None:
    rows = [
        _valid_row(test_id=test_id, profile_label="testA")
        for test_id in A1_LOCAL_CONTRACT_IDS
    ]
    validate_ledger_document(
        {"schema_version": "talaria-v0.6.2-configuration-ledger-v1", "rows": rows}
    )


def test_disposable_names_are_stage_and_connection_specific() -> None:
    local_active = stage_name_set("local", "active")
    remote_installed = stage_name_set("remote", "installed")
    assert local_active["testA"] == "talaria-v062-cfg-p2-local-active-a"
    assert local_active["testE"] == "talaria-v062-cfg-p2-local-active-switch"
    assert local_active["testC"] == "talaria-v062-cfg-p2-local-active-clone"
    assert local_active["testD"] == "talaria-v062-cfg-p2-local-active-renamed"
    assert remote_installed["testA"] == "talaria-v062-cfg-p2-remote-installed-a"
    assert set(local_active.values()).isdisjoint(remote_installed.values())
    for name in (*local_active.values(), *remote_installed.values()):
        require_legal_p2_name(name)
        assert P2_PROFILE_NAME_RE.fullmatch(name)
        assert P2_HERMES_NAME_RE.fullmatch(name)
        assert "." not in name
        assert len(name) <= 64


def test_testb_and_default_are_not_disposable_write_aliases() -> None:
    for alias in ("testB", "default"):
        with pytest.raises(HarnessError):
            disposable_profile_name("local", "active", alias)


def test_a1_27_reuses_a1_11_through_19_and_not_20() -> None:
    mapping = remote_reuse_map()
    assert set(mapping) == set(A1_REMOTE_REUSE_IDS)
    assert "A1-20" not in mapping
    assert set(mapping.values()) == {"A1-27"}


def test_a1_28_enumerates_every_ordered_connection_and_mutable_profile_pair() -> None:
    pairs = switch_pairs()
    assert len(pairs) == 12
    assert ("local", "testA", "remote", "testE") in pairs
    assert ("local", "testA", "local", "testA") not in pairs
    assert all(pair[1] in {"testA", "testE"} and pair[3] in {"testA", "testE"} for pair in pairs)


def test_multiplexer_preflight_is_not_a_batch_gate() -> None:
    assert multiplexer_preflight_is_batch_gate() is False


def test_live_hermes_is_off_unless_the_non_talaria_flag_is_set(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv(LIVE_HERMES_FLAG, raising=False)
    assert live_hermes_enabled("local") is False
    assert live_hermes_enabled("remote") is False
    monkeypatch.setenv(LIVE_HERMES_FLAG, "local")
    assert live_hermes_enabled("local") is True
    assert live_hermes_enabled("remote") is False


def test_forbidden_mutations_are_refused_and_read_only_host_status_is_named() -> None:
    for method, path in sorted(FORBIDDEN_MUTATIONS):
        with pytest.raises(HarnessError, match="must not invoke"):
            assert_request_allowed(method, path)
    assert_request_allowed("GET", "/api/health")
    assert_request_allowed("GET", "/api/config/schema")
    assert readonly_host_status_allowed("/api/hermes/update/check")
    assert readonly_host_status_allowed("/api/gateway/migrate/plan")
    with pytest.raises(HarnessError, match="local-models"):
        assert_request_allowed("POST", "/api/local-models/download")
    with pytest.raises(HarnessError, match="open-terminal"):
        assert_request_allowed("POST", "/api/profiles/alpha-fixture/open-terminal")


def test_a_planted_canary_is_found_and_a_redacted_payload_is_clean() -> None:
    canary = "canary-v062-cfg-do-not-leak"
    assert scan_for_canaries({"env": {"is_set": True, "value": canary}}, [canary]) == [canary]
    assert scan_for_canaries({"env": {"is_set": True, "redacted_value": "***"}}, [canary]) == []


def test_coverage_rejects_an_unresolved_tier1_disposition() -> None:
    with pytest.raises(HarnessError, match="unresolved"):
        validate_coverage_document(
            {
                "tier1": [
                    {
                        "id": "display.resume_last_session",
                        "source_ref": "CFG-A1 §10",
                        "owner": "hermes-profile",
                        "scope": "profile",
                        "route": "PUT /api/config",
                        "control": "boolean",
                        "timing_class": "next-session",
                        "disposition": "unresolved",
                    }
                ],
                "live_schema": {
                    "field_count": 836,
                    "category_count": 43,
                    "category_order": ["general"],
                    "known_types": ["string", "number", "boolean", "list", "select"],
                    "unknown_types": 0,
                },
                "exclusions": ["pet.*"],
            }
        )


def test_coverage_accepts_a_resolved_denominator() -> None:
    validate_coverage_document(
        {
            "tier1": [
                {
                    "id": "display.resume_last_session",
                    "source_ref": "CFG-A1 §10",
                    "owner": "hermes-profile",
                    "scope": "profile",
                    "route": "PUT /api/config",
                    "control": "boolean",
                    "timing_class": "next-session",
                    "disposition": "implement/verify",
                }
            ],
            "live_schema": {
                "field_count": 836,
                "category_count": 43,
                "category_order": ["general", "agent"],
                "known_types": ["string", "number", "boolean", "list", "select"],
                "unknown_types": 0,
            },
            "exclusions": ["pet.*", "dashboard skin"],
        }
    )


def test_cleanup_receipt_requires_absent_names_and_unchanged_testb() -> None:
    names = list(stage_name_set("local", "active").values())
    validate_cleanup_receipt(
        {
            "schema_version": CLEANUP_SCHEMA_VERSION,
            "record_type": CLEANUP_RECORD_TYPE,
            "stage": "active",
            "absent_names": names,
            "testB": {"unchanged": True},
            "created_ids_deleted": True,
        },
        stage="active",
    )
    with pytest.raises(HarnessError, match="testB"):
        validate_cleanup_receipt(
            {
                "schema_version": CLEANUP_SCHEMA_VERSION,
                "record_type": CLEANUP_RECORD_TYPE,
                "stage": "active",
                "absent_names": names,
                "testB": {"unchanged": False},
                "created_ids_deleted": True,
            },
            stage="active",
        )
    with pytest.raises(HarnessError, match="digest"):
        validate_cleanup_receipt(
            {
                "schema_version": CLEANUP_SCHEMA_VERSION,
                "record_type": CLEANUP_RECORD_TYPE,
                "stage": "active",
                "absent_names": names,
                "testB": {"unchanged": True, "safe_hash": sha256_text("count-only")},
                "created_ids_deleted": True,
            },
            stage="active",
        )


def test_a_secret_value_in_a_ledger_row_is_rejected() -> None:
    with pytest.raises(HarnessError, match="token"):
        validate_ledger_row({**_valid_row(), "token": "not-a-real-secret"})


def test_blocked_row_helper_does_not_claim_a_pass() -> None:
    row = blocked_row(
        test_id="A1-20",
        connection_label="remote",
        profile_label="public-routes",
        talaria_commit=_COMMIT,
        limitation=access_limitation("remote"),
        observed_at=_OBSERVED,
    )
    assert row["status"] == "blocked"
    validate_ledger_row(row)


def test_p2_5_dotted_legacy_names_are_rejected_and_harness_post_is_not_p2_4() -> None:
    with pytest.raises(HarnessError, match="dot"):
        require_legal_p2_name("talaria-v0.6.2-cfg-t0-local-active-a")
    assert (
        product_gateway_lifecycle_accepted(
            ["POST /api/gateway/start"], harness_posted=True
        )
        is False
    )


def test_the_harness_does_not_import_or_spawn_host_admin_tools() -> None:
    source_path = Path(inspect.getfile(assert_request_allowed))
    tree = ast.parse(source_path.read_text(encoding="utf-8"))
    imported = {
        alias.name
        for node in ast.walk(tree)
        if isinstance(node, ast.Import)
        for alias in node.names
    }
    imported.update(
        node.module or ""
        for node in ast.walk(tree)
        if isinstance(node, ast.ImportFrom)
    )
    assert "subprocess" not in imported
    assert "paramiko" not in imported
    calls = [
        node.func.attr
        for node in ast.walk(tree)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
    ]
    assert "system" not in calls
    assert LIVE_HERMES_FLAG.startswith("V062_")
    assert not LIVE_CREDENTIALS_PATH_FLAG.startswith("TALARIA_")
    assert TEST_SECRET_KEY == "TALARIA_V062_TEST_SECRET"
