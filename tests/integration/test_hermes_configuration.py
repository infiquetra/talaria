"""CFG-A1 11–20 against a real Hermes, plus remote reuse hooks for 27–28.

These cases are access-gated. CFG-T0 left authenticated local and remote
access absent, so the default run records them blocked rather than passed.
Stub-server coverage lives in ``tests/transport/`` and must not skip.
"""

from __future__ import annotations

import importlib
from typing import Any

import pytest

from scripts.acceptance.v062_configuration import (
    A1_LOCAL_CONTRACT_IDS,
    A1_REMOTE_REUSE_IDS,
    TEST_SECRET_KEY,
    access_limitation,
    multiplexer_preflight_is_batch_gate,
    remote_reuse_map,
    switch_pairs,
)

pytestmark = pytest.mark.real_hermes


def _settings_client() -> Any:
    try:
        module = importlib.import_module("talaria.transport.settings")
    except ImportError as exc:
        raise AssertionError("unimplemented interface: talaria.transport.settings") from exc
    client = getattr(module, "SettingsClient", None)
    if client is None:
        raise AssertionError("unimplemented interface: SettingsClient")
    return client


@pytest.mark.parametrize("test_id", A1_LOCAL_CONTRACT_IDS)
def test_a1_11_20_local_access_is_blocked_until_cfg_t0_is_closed(
    test_id: str, local_hermes_access: dict[str, Any]
) -> None:
    """Executed only after authenticated local access exists.

    The fixture skips with the CFG-T0 limitation when access is absent. A skip
    here is blocked, not passed.
    """
    assert local_hermes_access["connection"] == "local"
    client_cls = _settings_client()
    assert client_cls is not None
    raise AssertionError(f"{test_id} reached a live local session; implement the run body")


@pytest.mark.parametrize("test_id", A1_REMOTE_REUSE_IDS)
def test_a1_27_reuses_a1_11_19_on_remote(
    test_id: str, remote_hermes_access: dict[str, Any]
) -> None:
    assert remote_reuse_map()[test_id] == "A1-27"
    assert remote_hermes_access["connection"] == "remote"
    client_cls = _settings_client()
    assert client_cls is not None
    raise AssertionError(f"A1-27/{test_id} reached a live remote session; implement the run body")


def test_a1_28_switch_pairs_are_ready_when_both_connections_exist(
    local_hermes_access: dict[str, Any],
    remote_hermes_access: dict[str, Any],
) -> None:
    pairs = switch_pairs()
    assert len(pairs) == 12
    assert local_hermes_access["api_base"] != remote_hermes_access["api_base"]
    raise AssertionError("A1-28 reached live dual-connection access; implement the run body")


def test_a1_19_multiplexer_preflight_does_not_gate_the_batch() -> None:
    assert multiplexer_preflight_is_batch_gate() is False
    assert access_limitation("local")
    assert TEST_SECRET_KEY == "TALARIA_V062_TEST_SECRET"


def test_harness_forbids_host_admin_writes_on_the_live_path() -> None:
    from scripts.acceptance.v062_configuration import HarnessError, assert_request_allowed

    with pytest.raises(HarnessError):
        assert_request_allowed("POST", "/api/hermes/update")
    with pytest.raises(HarnessError):
        assert_request_allowed("POST", "/api/gateway/migrate")
    with pytest.raises(HarnessError):
        assert_request_allowed("POST", "/api/profiles/active")
