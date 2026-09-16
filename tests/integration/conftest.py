"""Access-gated fixtures for real Hermes configuration tests.

Authenticated local and remote access is still absent (CFG-T0). These fixtures
never invent a credential, never read the operator's default Talaria
credentials file, and never enable live mutation unless
``V062_CFG_LIVE_HERMES`` is set. Autouse config isolation clears ``TALARIA_*``,
so the opt-in flag deliberately does not use that prefix.
"""

from __future__ import annotations

from collections.abc import Iterator
from typing import Any, Literal

import pytest

from scripts.acceptance.v062_configuration import (
    LOCAL_API_BASE,
    REMOTE_API_BASE,
    access_limitation,
    live_credentials_path,
    live_hermes_enabled,
    stage_name_set,
)

ConnectionLabel = Literal["local", "remote"]


def pytest_configure(config: pytest.Config) -> None:
    config.addinivalue_line(
        "markers",
        "real_hermes: live Hermes configuration case; blocked when CFG-T0 access is absent",
    )


@pytest.fixture
def local_api_base() -> str:
    return LOCAL_API_BASE


@pytest.fixture
def remote_api_base() -> str:
    return REMOTE_API_BASE


@pytest.fixture
def active_local_names() -> dict[str, str]:
    return stage_name_set("local", "active")


@pytest.fixture
def active_remote_names() -> dict[str, str]:
    return stage_name_set("remote", "active")


def _require_live_access(connection: ConnectionLabel) -> None:
    if not live_hermes_enabled(connection):
        pytest.skip(access_limitation(connection))
    try:
        path = live_credentials_path()
    except Exception as exc:
        pytest.skip(f"blocked: {exc}")
    if path is None:
        pytest.skip(access_limitation(connection))


@pytest.fixture
def local_hermes_access() -> Iterator[dict[str, Any]]:
    _require_live_access("local")
    yield {"connection": "local", "api_base": LOCAL_API_BASE}


@pytest.fixture
def remote_hermes_access() -> Iterator[dict[str, Any]]:
    _require_live_access("remote")
    yield {"connection": "remote", "api_base": REMOTE_API_BASE}
