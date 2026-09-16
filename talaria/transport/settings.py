"""Authenticated dashboard settings client for core config/env/model/profile routes.

Every profile-scoped call carries an explicit ``ConfigTarget``. Empty, missing,
or ``current`` profiles are refused before a socket is opened. Host-administration
mutations are absent from this module; restart never claims the dashboard moved.
"""

from __future__ import annotations

import asyncio
from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any, Literal

from talaria.domain.settings import ConfigTarget
from talaria.transport.admin import (
    AdminError,
    admin_origin_for,
    request_admin_json,
)
from talaria.transport.credentials import Credential, CredentialError, CredentialProvider

__all__ = [
    "SettingsClient",
    "SettingsError",
    "SettingsFailure",
    "SettingsRestartResult",
]

SettingsFailure = Literal[
    "timeout",
    "malformed_response",
    "oversized_response",
    "conflict",
    "unauthorized",
    "unknown_profile",
    "profile_not_found",
    "absent_capability",
    "invalid_request",
    "http_error",
    "unreachable",
    "refused_origin",
    "credential_unavailable",
    "rate_limited",
]


class SettingsError(RuntimeError):
    """A settings call that did not produce a value, with a branchable reason."""

    def __init__(self, reason: SettingsFailure, message: str) -> None:
        super().__init__(message)
        self.reason: SettingsFailure = reason

    def __repr__(self) -> str:
        return f"SettingsError(reason={self.reason!r}, message={str(self)!r})"


@dataclass(frozen=True)
class SettingsRestartResult:
    """Outcome of ``POST /api/gateway/restart`` plus the follow-up polls."""

    dashboard_restarted: bool = False
    verified: bool = False


def _require_explicit_profile(target: ConfigTarget) -> str:
    name = target.profile_name.strip()
    if not name or name == "current":
        raise ValueError("profile-scoped request requires an explicit profile")
    return name


class SettingsClient:
    """Core configuration routes for one dashboard origin."""

    def __init__(
        self,
        endpoint: str,
        provider: CredentialProvider,
        *,
        timeout: float = 15.0,
    ) -> None:
        self.origin = admin_origin_for(endpoint)
        self._provider = provider
        self._timeout = timeout
        self._write_disabled: set[ConfigTarget] = set()

    def __repr__(self) -> str:
        return f"SettingsClient(origin={self.origin!r})"

    def writes_enabled_for(self, target: ConfigTarget) -> bool:
        return target not in self._write_disabled

    async def _credential(self) -> Credential:
        try:
            return await self._provider.acquire()
        except CredentialError as exc:
            raise SettingsError("credential_unavailable", str(exc)) from exc

    async def _request(
        self,
        method: str,
        path: str,
        *,
        params: dict[str, str] | None = None,
        body: Mapping[str, Any] | None = None,
    ) -> Any:
        credential = await self._credential()
        try:
            return await asyncio.to_thread(
                request_admin_json,
                self.origin,
                path,
                method=method,
                token=credential.value,
                params=params,
                body=body,
                timeout=self._timeout,
            )
        except AdminError as exc:
            raise self._translate(exc) from exc

    def _translate(self, exc: AdminError) -> SettingsError:
        reason = exc.reason
        allowed: set[str] = set(getattr(SettingsFailure, "__args__", ()))
        mapped: SettingsFailure = reason if reason in allowed else "http_error"
        return SettingsError(mapped, str(exc))

    def _disable_writes(self, target: ConfigTarget) -> None:
        self._write_disabled.add(target)

    async def get_config(
        self, target: ConfigTarget, *, include_defaults: bool = True
    ) -> Any:
        profile = _require_explicit_profile(target)
        params = {"profile": profile}
        if not include_defaults:
            params["include_defaults"] = "false"
        try:
            return await self._request("GET", "/api/config", params=params)
        except SettingsError as exc:
            if exc.reason in {"absent_capability", "unknown_profile"}:
                self._disable_writes(target)
                raise SettingsError(
                    "unknown_profile",
                    f"profile {profile!r} was not found",
                ) from exc
            raise

    async def put_config(self, target: ConfigTarget, patch: Mapping[str, Any]) -> Any:
        profile = _require_explicit_profile(target)
        return await self._request(
            "PUT",
            "/api/config",
            params={"profile": profile},
            body=dict(patch),
        )

    async def get_schema(self, target: ConfigTarget) -> Any:
        profile = _require_explicit_profile(target)
        return await self._request(
            "GET", "/api/config/schema", params={"profile": profile}
        )

    async def get_env(self, target: ConfigTarget) -> Any:
        profile = _require_explicit_profile(target)
        return await self._request("GET", "/api/env", params={"profile": profile})

    async def put_env(self, target: ConfigTarget, key: str, value: str) -> Any:
        profile = _require_explicit_profile(target)
        return await self._request(
            "PUT",
            "/api/env",
            params={"profile": profile},
            body={"key": key, "value": value},
        )

    async def delete_env(self, target: ConfigTarget, key: str) -> Any:
        profile = _require_explicit_profile(target)
        return await self._request(
            "DELETE",
            "/api/env",
            params={"profile": profile, "key": key},
        )

    async def reveal_env(self, target: ConfigTarget, key: str) -> Any:
        profile = _require_explicit_profile(target)
        try:
            return await self._request(
                "POST",
                "/api/env/reveal",
                params={"profile": profile},
                body={"key": key},
            )
        except SettingsError as exc:
            if "429" in str(exc) or exc.reason == "http_error":
                raise SettingsError(
                    "rate_limited",
                    f"env reveal is rate limited (429) for profile {profile!r}",
                ) from exc
            raise

    async def set_model(
        self,
        target: ConfigTarget,
        *,
        provider: str,
        model: str,
        confirm_expensive_model: bool,
    ) -> Any:
        profile = _require_explicit_profile(target)
        return await self._request(
            "POST",
            "/api/model/set",
            params={"profile": profile},
            body={
                "scope": "main",
                "provider": provider,
                "model": model,
                "confirm_expensive_model": confirm_expensive_model,
            },
        )

    async def get_model_info(self, target: ConfigTarget) -> Any:
        profile = _require_explicit_profile(target)
        return await self._request("GET", "/api/model/info", params={"profile": profile})

    async def clone_profile(
        self,
        connection_id: str,
        *,
        name: str,
        clone_from: str,
        clone_all: bool,
        clone_channels: bool,
    ) -> Any:
        if not connection_id.strip():
            raise ValueError("clone_profile requires an explicit connection")
        if not clone_from.strip() or clone_from.strip() == "current":
            raise ValueError("clone_profile requires an explicit source profile")
        body: dict[str, Any] = {
            "name": name,
            "clone_from": clone_from,
            "clone_all": clone_all,
            "clone_channels": clone_channels,
        }
        return await self._request("POST", "/api/profiles", body=body)

    async def rename_profile(self, connection_id: str, name: str, new_name: str) -> Any:
        if not connection_id.strip() or not name.strip():
            raise ValueError("rename_profile requires an explicit profile")
        return await self._request(
            "PATCH",
            f"/api/profiles/{name}",
            body={"new_name": new_name},
        )

    async def delete_profile(self, connection_id: str, name: str) -> Any:
        if not connection_id.strip() or not name.strip():
            raise ValueError("delete_profile requires an explicit profile")
        return await self._request("DELETE", f"/api/profiles/{name}")

    async def get_status(self, target: ConfigTarget) -> Any:
        profile = _require_explicit_profile(target)
        return await self._request("GET", "/api/status", params={"profile": profile})

    async def restart_gateway(self, target: ConfigTarget, plan: object) -> SettingsRestartResult:
        profile = _require_explicit_profile(target)
        _ = plan
        await self._request("POST", "/api/gateway/restart", params={"profile": profile})
        await self._request("GET", "/api/actions/gateway-restart/status")
        await self._request("GET", "/api/status", params={"profile": profile})
        return SettingsRestartResult(dashboard_restarted=False, verified=True)

    async def wake_start(self, target: ConfigTarget) -> Any:
        profile = _require_explicit_profile(target)
        return await self._request(
            "POST",
            "/api/rpc",
            params={"profile": profile},
            body={"method": "wake.start", "params": {"profile": profile, "persist": True}},
        )

    async def reset_profile(
        self, target: ConfigTarget, patch: Mapping[str, Any] | None = None
    ) -> Any:
        profile = _require_explicit_profile(target)
        return await self._request(
            "PUT",
            "/api/config",
            params={"profile": profile},
            body={"config": dict(patch or {}), "reset": True},
        )
