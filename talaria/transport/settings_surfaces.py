"""Typed adapters for remaining Desktop-parity Hermes settings routes.

Every profile-scoped call takes a ``ConfigTarget`` and refuses an empty or
``current`` profile before a socket opens. Host routes take :class:`HostScope`.
Forbidden host-administration mutations are omitted; read-only host status is
assembled so those write paths never appear in this module.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any
from urllib.parse import urlsplit

from talaria.domain.settings import ConfigTarget
from talaria.domain.settings_commands import WakeWord, rpc_request
from talaria.transport.settings import SettingsClient, _require_explicit_profile

__all__ = [
    "EXPORT_HOST_PATH_NOTE",
    "IMPORT_HOST_PATH_NOTE",
    "MEMORY_OAUTH_BLOCKED",
    "WHATSAPP_HOST_NOTE",
    "HostPathResult",
    "HostScope",
    "SettingsSurfaces",
    "SurfaceNote",
]

WHATSAPP_HOST_NOTE = (
    "WhatsApp onboarding is host-side; the dashboard process owns the bridge."
)
EXPORT_HOST_PATH_NOTE = (
    "Profile export writes a path on the Hermes host and is loopback-only."
)
IMPORT_HOST_PATH_NOTE = (
    "Profile import reads a path on the Hermes host and is loopback-only."
)
MEMORY_OAUTH_BLOCKED = (
    "Memory-provider OAuth needs a reachable loopback callback; "
    "this origin cannot host it."
)

_LOOPBACK_HOSTS = frozenset({"127.0.0.1", "::1", "localhost"})


@dataclass(frozen=True)
class HostScope:
    """Dashboard-process scope. Host routes never accept a profile name."""

    connection_id: str

    def __post_init__(self) -> None:
        if not self.connection_id.strip():
            raise ValueError("host-scoped request requires an explicit connection")


@dataclass(frozen=True)
class SurfaceNote:
    """A visible capability caveat; never treated as an empty success."""

    available: bool
    reason: str
    detail: str
    payload: Any = None


@dataclass(frozen=True)
class HostPathResult:
    """Loopback host-path import/export, or a remote-unavailable note."""

    available: bool
    reason: str
    detail: str
    archive: str | None = None
    name: str | None = None
    payload: Any = None


def _host_path(*parts: str) -> str:
    return "/" + "/".join(parts)


def _origin_is_loopback(origin: str) -> bool:
    host = (urlsplit(origin).hostname or "").lower()
    return host in _LOOPBACK_HOSTS


class SettingsSurfaces:
    """Remaining Desktop-parity routes on top of :class:`SettingsClient`."""

    def __init__(self, client: SettingsClient) -> None:
        self._client = client

    def __repr__(self) -> str:
        return f"SettingsSurfaces(client={self._client!r})"

    def _profile(self, target: ConfigTarget) -> str:
        return _require_explicit_profile(target)

    def _require_host(self, scope: HostScope) -> str:
        return scope.connection_id.strip()

    def _loopback(self) -> bool:
        return _origin_is_loopback(self._client.origin)

    async def _scoped(
        self,
        method: str,
        path: str,
        target: ConfigTarget,
        *,
        body: Mapping[str, Any] | None = None,
        extra_params: Mapping[str, str] | None = None,
    ) -> Any:
        params = {"profile": self._profile(target)}
        if extra_params:
            params.update(extra_params)
        return await self._client._request(method, path, params=params, body=body)

    async def _host(
        self,
        scope: HostScope,
        method: str,
        path: str,
        *,
        body: Mapping[str, Any] | None = None,
        params: dict[str, str] | None = None,
    ) -> Any:
        self._require_host(scope)
        return await self._client._request(method, path, params=params, body=body)

    async def _rpc(
        self,
        target: ConfigTarget,
        method: str,
        extra: Mapping[str, Any] | None = None,
    ) -> Any:
        profile = self._profile(target)
        params: dict[str, Any] = {"profile": profile}
        if extra:
            params.update(extra)
        return await self._client._request(
            "POST",
            "/api/rpc",
            params={"profile": profile},
            body={"method": method, "params": params},
        )

    async def reset_profile(
        self, target: ConfigTarget, patch: Mapping[str, Any] | None = None
    ) -> Any:
        return await self._client.reset_profile(target, patch)

    async def wake(self, command: WakeWord) -> Any:
        spec = rpc_request(command)
        profile = self._profile(command.target)
        return await self._client._request(
            "POST",
            "/api/rpc",
            params={"profile": profile},
            body={"method": spec.method, "params": dict(spec.params)},
        )

    async def wake_start(self, target: ConfigTarget) -> Any:
        return await self.wake(WakeWord(target=target, action="start"))

    async def wake_stop(self, target: ConfigTarget) -> Any:
        return await self.wake(WakeWord(target=target, action="stop"))

    async def wake_status(self, target: ConfigTarget) -> Any:
        return await self.wake(WakeWord(target=target, action="status"))

    async def list_oauth_providers(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/providers/oauth", target)

    async def start_oauth(self, target: ConfigTarget, provider_id: str) -> Any:
        return await self._scoped(
            "POST", f"/api/providers/oauth/{provider_id}/start", target
        )

    async def poll_oauth(
        self, target: ConfigTarget, provider_id: str, session_id: str
    ) -> Any:
        return await self._scoped(
            "GET",
            f"/api/providers/oauth/{provider_id}/poll/{session_id}",
            target,
        )

    async def submit_oauth(
        self,
        target: ConfigTarget,
        provider_id: str,
        body: Mapping[str, Any],
    ) -> Any:
        return await self._scoped(
            "POST",
            f"/api/providers/oauth/{provider_id}/submit",
            target,
            body=dict(body),
        )

    async def disconnect_oauth(self, target: ConfigTarget, provider_id: str) -> Any:
        return await self._scoped(
            "DELETE", f"/api/providers/oauth/{provider_id}", target
        )

    async def cancel_oauth_session(self, target: ConfigTarget, session_id: str) -> Any:
        return await self._scoped(
            "DELETE", f"/api/providers/oauth/sessions/{session_id}", target
        )

    async def validate_provider(self, target: ConfigTarget, key: str, value: str) -> Any:
        return await self._scoped(
            "POST",
            "/api/providers/validate",
            target,
            body={"key": key, "value": value},
        )

    async def list_custom_endpoints(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/providers/custom-endpoints", target)

    async def upsert_custom_endpoint(
        self, target: ConfigTarget, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "POST",
            "/api/providers/custom-endpoints",
            target,
            body=dict(body),
        )

    async def activate_custom_endpoint(
        self, target: ConfigTarget, endpoint_id: str
    ) -> Any:
        return await self._scoped(
            "POST",
            f"/api/providers/custom-endpoints/{endpoint_id}/activate",
            target,
        )

    async def delete_custom_endpoint(
        self, target: ConfigTarget, endpoint_id: str
    ) -> Any:
        return await self._scoped(
            "DELETE",
            f"/api/providers/custom-endpoints/{endpoint_id}",
            target,
        )

    async def validate_custom_endpoint(
        self, target: ConfigTarget, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "POST",
            "/api/providers/custom-endpoints/validate",
            target,
            body=dict(body),
        )

    async def get_voice_config(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/audio/voice-config", target)

    async def get_voice_live_status(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/audio/voice-live/status", target)

    async def get_elevenlabs_voices(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/audio/elevenlabs/voices", target)

    async def list_messaging_platforms(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/messaging/platforms", target)

    async def put_messaging_platform(
        self,
        target: ConfigTarget,
        platform_id: str,
        body: Mapping[str, Any],
    ) -> Any:
        return await self._scoped(
            "PUT",
            f"/api/messaging/platforms/{platform_id}",
            target,
            body=dict(body),
        )

    async def test_messaging_platform(
        self, target: ConfigTarget, platform_id: str
    ) -> Any:
        return await self._scoped(
            "POST", f"/api/messaging/platforms/{platform_id}/test", target
        )

    async def start_telegram_onboarding(
        self, target: ConfigTarget, body: Mapping[str, Any] | None = None
    ) -> Any:
        return await self._scoped(
            "POST",
            "/api/messaging/telegram/onboarding/start",
            target,
            body=dict(body or {}),
        )

    async def get_telegram_onboarding(
        self, target: ConfigTarget, pairing_id: str
    ) -> Any:
        return await self._scoped(
            "GET",
            f"/api/messaging/telegram/onboarding/{pairing_id}",
            target,
        )

    async def apply_telegram_onboarding(
        self, target: ConfigTarget, pairing_id: str
    ) -> Any:
        return await self._scoped(
            "POST",
            f"/api/messaging/telegram/onboarding/{pairing_id}/apply",
            target,
        )

    async def delete_telegram_onboarding(
        self, target: ConfigTarget, pairing_id: str
    ) -> Any:
        return await self._scoped(
            "DELETE",
            f"/api/messaging/telegram/onboarding/{pairing_id}",
            target,
        )

    async def start_whatsapp_onboarding(
        self, target: ConfigTarget, body: Mapping[str, Any] | None = None
    ) -> SurfaceNote:
        payload = await self._scoped(
            "POST",
            "/api/messaging/whatsapp/onboarding/start",
            target,
            body=dict(body or {}),
        )
        return SurfaceNote(
            available=True,
            reason="host_side_bridge",
            detail=WHATSAPP_HOST_NOTE,
            payload=payload,
        )

    async def get_whatsapp_onboarding(
        self, target: ConfigTarget, pairing_id: str
    ) -> SurfaceNote:
        payload = await self._scoped(
            "GET",
            f"/api/messaging/whatsapp/onboarding/{pairing_id}",
            target,
        )
        return SurfaceNote(
            available=True,
            reason="host_side_bridge",
            detail=WHATSAPP_HOST_NOTE,
            payload=payload,
        )

    async def apply_whatsapp_onboarding(
        self, target: ConfigTarget, pairing_id: str
    ) -> SurfaceNote:
        payload = await self._scoped(
            "POST",
            f"/api/messaging/whatsapp/onboarding/{pairing_id}/apply",
            target,
        )
        return SurfaceNote(
            available=True,
            reason="host_side_bridge",
            detail=WHATSAPP_HOST_NOTE,
            payload=payload,
        )

    async def delete_whatsapp_onboarding(
        self, target: ConfigTarget, pairing_id: str
    ) -> SurfaceNote:
        payload = await self._scoped(
            "DELETE",
            f"/api/messaging/whatsapp/onboarding/{pairing_id}",
            target,
        )
        return SurfaceNote(
            available=True,
            reason="host_side_bridge",
            detail=WHATSAPP_HOST_NOTE,
            payload=payload,
        )

    async def get_pairing(self, scope: HostScope) -> Any:
        return await self._host(scope, "GET", "/api/pairing")

    async def approve_pairing(self, scope: HostScope, body: Mapping[str, Any]) -> Any:
        return await self._host(
            scope, "POST", "/api/pairing/approve", body=dict(body)
        )

    async def revoke_pairing(self, scope: HostScope, body: Mapping[str, Any]) -> Any:
        return await self._host(
            scope, "POST", "/api/pairing/revoke", body=dict(body)
        )

    async def clear_pending_pairing(self, scope: HostScope) -> Any:
        return await self._host(scope, "POST", "/api/pairing/clear-pending")

    async def list_skills(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/skills", target)

    async def toggle_skill(
        self, target: ConfigTarget, name: str, *, enabled: bool
    ) -> Any:
        return await self._scoped(
            "PUT",
            "/api/skills/toggle",
            target,
            body={"name": name, "enabled": enabled},
        )

    async def get_skill_content(self, target: ConfigTarget, name: str) -> Any:
        return await self._scoped(
            "GET",
            "/api/skills/content",
            target,
            extra_params={"name": name},
        )

    async def create_skill(self, target: ConfigTarget, body: Mapping[str, Any]) -> Any:
        payload = dict(body)
        payload.setdefault("profile", self._profile(target))
        return await self._scoped("POST", "/api/skills", target, body=payload)

    async def update_skill_content(
        self, target: ConfigTarget, body: Mapping[str, Any]
    ) -> Any:
        payload = dict(body)
        payload.setdefault("profile", self._profile(target))
        return await self._scoped("PUT", "/api/skills/content", target, body=payload)

    async def list_skill_hub_official(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/skills/hub/official", target)

    async def list_skill_hub_sources(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/skills/hub/sources", target)

    async def search_skill_hub(self, target: ConfigTarget, query: str) -> Any:
        return await self._scoped(
            "GET",
            "/api/skills/hub/search",
            target,
            extra_params={"q": query},
        )

    async def preview_skill_hub(self, target: ConfigTarget, slug: str) -> Any:
        return await self._scoped(
            "GET",
            "/api/skills/hub/preview",
            target,
            extra_params={"slug": slug},
        )

    async def scan_skill_hub(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/skills/hub/scan", target)

    async def install_skill_hub(
        self, target: ConfigTarget, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "POST", "/api/skills/hub/install", target, body=dict(body)
        )

    async def uninstall_skill_hub(
        self, target: ConfigTarget, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "POST", "/api/skills/hub/uninstall", target, body=dict(body)
        )

    async def update_skill_hub(
        self, target: ConfigTarget, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "POST", "/api/skills/hub/update", target, body=dict(body)
        )

    async def list_toolsets(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/tools/toolsets", target)

    async def put_toolset(
        self, target: ConfigTarget, name: str, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "PUT", f"/api/tools/toolsets/{name}", target, body=dict(body)
        )

    async def get_toolset_config(self, target: ConfigTarget, name: str) -> Any:
        return await self._scoped("GET", f"/api/tools/toolsets/{name}/config", target)

    async def get_toolset_models(self, target: ConfigTarget, name: str) -> Any:
        return await self._scoped("GET", f"/api/tools/toolsets/{name}/models", target)

    async def put_toolset_model(
        self, target: ConfigTarget, name: str, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "PUT",
            f"/api/tools/toolsets/{name}/model",
            target,
            body=dict(body),
        )

    async def put_toolset_provider(
        self, target: ConfigTarget, name: str, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "PUT",
            f"/api/tools/toolsets/{name}/provider",
            target,
            body=dict(body),
        )

    async def put_toolset_env(
        self, target: ConfigTarget, name: str, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "PUT", f"/api/tools/toolsets/{name}/env", target, body=dict(body)
        )

    async def post_toolset_setup(self, target: ConfigTarget, name: str) -> Any:
        return await self._scoped(
            "POST", f"/api/tools/toolsets/{name}/post-setup", target
        )

    async def get_terminal_backends(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/tools/terminal/backends", target)

    async def put_terminal_backend(
        self, target: ConfigTarget, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "PUT", "/api/tools/terminal/backend", target, body=dict(body)
        )

    async def get_computer_use_status(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/tools/computer-use/status", target)

    async def list_mcp_servers(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/mcp/servers", target)

    async def add_mcp_server(self, target: ConfigTarget, body: Mapping[str, Any]) -> Any:
        return await self._scoped(
            "POST", "/api/mcp/servers", target, body=dict(body)
        )

    async def replace_mcp_servers(
        self, target: ConfigTarget, servers: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "PUT", "/api/mcp/servers", target, body={"servers": dict(servers)}
        )

    async def delete_mcp_server(self, target: ConfigTarget, name: str) -> Any:
        return await self._scoped("DELETE", f"/api/mcp/servers/{name}", target)

    async def test_mcp_server(self, target: ConfigTarget, name: str) -> Any:
        return await self._scoped("POST", f"/api/mcp/servers/{name}/test", target)

    async def auth_mcp_server(self, target: ConfigTarget, name: str) -> Any:
        return await self._scoped("POST", f"/api/mcp/servers/{name}/auth", target)

    async def enable_mcp_server(
        self, target: ConfigTarget, name: str, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "PUT",
            f"/api/mcp/servers/{name}/enabled",
            target,
            body=dict(body),
        )

    async def get_mcp_catalog(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/mcp/catalog", target)

    async def install_mcp_catalog(
        self, target: ConfigTarget, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "POST", "/api/mcp/catalog/install", target, body=dict(body)
        )

    async def get_mcp_oauth_flow(self, scope: HostScope, flow_id: str) -> Any:
        return await self._host(scope, "GET", f"/api/mcp/oauth/flows/{flow_id}")

    async def delete_mcp_oauth_flow(self, scope: HostScope, flow_id: str) -> Any:
        return await self._host(scope, "DELETE", f"/api/mcp/oauth/flows/{flow_id}")

    async def list_cron_jobs(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/cron/jobs", target)

    async def get_cron_job(self, target: ConfigTarget, job_id: str) -> Any:
        return await self._scoped("GET", f"/api/cron/jobs/{job_id}", target)

    async def get_cron_job_runs(
        self, target: ConfigTarget, job_id: str, *, limit: int = 20
    ) -> Any:
        return await self._scoped(
            "GET",
            f"/api/cron/jobs/{job_id}/runs",
            target,
            extra_params={"limit": str(limit)},
        )

    async def create_cron_job(
        self, target: ConfigTarget, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped("POST", "/api/cron/jobs", target, body=dict(body))

    async def update_cron_job(
        self, target: ConfigTarget, job_id: str, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "PUT", f"/api/cron/jobs/{job_id}", target, body=dict(body)
        )

    async def pause_cron_job(self, target: ConfigTarget, job_id: str) -> Any:
        return await self._scoped("POST", f"/api/cron/jobs/{job_id}/pause", target)

    async def resume_cron_job(self, target: ConfigTarget, job_id: str) -> Any:
        return await self._scoped("POST", f"/api/cron/jobs/{job_id}/resume", target)

    async def trigger_cron_job(self, target: ConfigTarget, job_id: str) -> Any:
        return await self._scoped("POST", f"/api/cron/jobs/{job_id}/trigger", target)

    async def delete_cron_job(self, target: ConfigTarget, job_id: str) -> Any:
        return await self._scoped("DELETE", f"/api/cron/jobs/{job_id}", target)

    async def get_cron_delivery_targets(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/cron/delivery-targets", target)

    async def list_cron_blueprints(self, target: ConfigTarget) -> Any:
        return await self._scoped("GET", "/api/cron/blueprints", target)

    async def instantiate_cron_blueprint(
        self, target: ConfigTarget, body: Mapping[str, Any]
    ) -> Any:
        return await self._scoped(
            "POST", "/api/cron/blueprints/instantiate", target, body=dict(body)
        )

    async def get_memory_provider_config(
        self, target: ConfigTarget, name: str, *, surface: str | None = None
    ) -> Any:
        extra = {"surface": surface} if surface else None
        return await self._scoped(
            "GET",
            f"/api/memory/providers/{name}/config",
            target,
            extra_params=extra,
        )

    async def put_memory_provider_config(
        self,
        target: ConfigTarget,
        name: str,
        body: Mapping[str, Any],
        *,
        surface: str | None = None,
    ) -> Any:
        extra = {"surface": surface} if surface else None
        return await self._scoped(
            "PUT",
            f"/api/memory/providers/{name}/config",
            target,
            body=dict(body),
            extra_params=extra,
        )

    async def setup_memory_provider(
        self, target: ConfigTarget, name: str, body: Mapping[str, Any] | None = None
    ) -> Any:
        return await self._scoped(
            "POST",
            f"/api/memory/providers/{name}/setup",
            target,
            body=dict(body or {}),
        )

    async def start_memory_oauth(
        self, target: ConfigTarget, provider: str
    ) -> SurfaceNote:
        self._profile(target)
        if not self._loopback():
            return SurfaceNote(
                available=False,
                reason="callback_unreachable",
                detail=MEMORY_OAUTH_BLOCKED,
            )
        payload = await self._scoped(
            "POST",
            f"/api/memory/providers/{provider}/oauth/start",
            target,
        )
        return SurfaceNote(
            available=True,
            reason="ok",
            detail="Memory-provider OAuth started on a loopback callback.",
            payload=payload,
        )

    async def memory_oauth_status(
        self, target: ConfigTarget, provider: str
    ) -> SurfaceNote:
        self._profile(target)
        if not self._loopback():
            return SurfaceNote(
                available=False,
                reason="callback_unreachable",
                detail=MEMORY_OAUTH_BLOCKED,
            )
        payload = await self._scoped(
            "GET",
            f"/api/memory/providers/{provider}/oauth/status",
            target,
        )
        return SurfaceNote(
            available=True,
            reason="ok",
            detail="Memory-provider OAuth status from a loopback callback.",
            payload=payload,
        )

    async def vault_list(self, target: ConfigTarget) -> Any:
        return await self._rpc(target, "vault.list")

    async def vault_sources(self, target: ConfigTarget) -> Any:
        return await self._rpc(target, "vault.sources")

    async def vault_source_set(
        self, target: ConfigTarget, name: str, *, enabled: bool
    ) -> Any:
        return await self._rpc(
            target, "vault.source.set", extra={"name": name, "enabled": enabled}
        )

    async def vault_unlock(self, target: ConfigTarget, name: str, password: str) -> Any:
        return await self._rpc(
            target,
            "vault.unlock",
            extra={"name": name, "password": password},
        )

    async def vault_lock(self, target: ConfigTarget, name: str | None = None) -> Any:
        extra = {"name": name} if name else None
        return await self._rpc(target, "vault.lock", extra=extra)

    async def vault_add(self, target: ConfigTarget, extra: Mapping[str, Any]) -> Any:
        return await self._rpc(target, "vault.add", extra=extra)

    async def vault_remove(self, target: ConfigTarget, item_id: str) -> Any:
        return await self._rpc(target, "vault.remove", extra={"id": item_id})

    async def list_dashboard_plugins(self, scope: HostScope) -> SurfaceNote:
        payload = await self._host(scope, "GET", "/api/dashboard/plugins")
        return SurfaceNote(
            available=True,
            reason="dashboard_restart_no_api",
            detail=(
                "Agent plugins are host-owned; a dashboard restart is required "
                "and no API performs it."
            ),
            payload=payload,
        )

    async def install_agent_plugin(
        self, scope: HostScope, body: Mapping[str, Any]
    ) -> SurfaceNote:
        payload = await self._host(
            scope,
            "POST",
            "/api/dashboard/agent-plugins/install",
            body=dict(body),
        )
        return SurfaceNote(
            available=True,
            reason="dashboard_restart_no_api",
            detail=(
                "Agent plugins are host-owned; a dashboard restart is required "
                "and no API performs it."
            ),
            payload=payload,
        )

    async def enable_agent_plugin(self, scope: HostScope, name: str) -> SurfaceNote:
        return await self._agent_plugin_action(scope, "POST", name, "enable")

    async def disable_agent_plugin(self, scope: HostScope, name: str) -> SurfaceNote:
        return await self._agent_plugin_action(scope, "POST", name, "disable")

    async def refresh_agent_plugin(self, scope: HostScope, name: str) -> SurfaceNote:
        return await self._agent_plugin_action(scope, "POST", name, "update")

    async def delete_agent_plugin(self, scope: HostScope, name: str) -> SurfaceNote:
        return await self._agent_plugin_action(scope, "DELETE", name, None)

    async def _agent_plugin_action(
        self,
        scope: HostScope,
        method: str,
        name: str,
        action: str | None,
    ) -> SurfaceNote:
        suffix = f"/{action}" if action else ""
        payload = await self._host(
            scope,
            method,
            f"/api/dashboard/agent-plugins/{name}{suffix}",
        )
        return SurfaceNote(
            available=True,
            reason="dashboard_restart_no_api",
            detail=(
                "Agent plugins are host-owned; a dashboard restart is required "
                "and no API performs it."
            ),
            payload=payload,
        )

    async def get_setup_command(self, target: ConfigTarget) -> Any:
        name = self._profile(target)
        return await self._client._request(
            "GET", f"/api/profiles/{name}/setup-command"
        )

    async def get_profile_soul(self, target: ConfigTarget) -> Any:
        name = self._profile(target)
        return await self._client._request("GET", f"/api/profiles/{name}/soul")

    async def put_profile_soul(self, target: ConfigTarget, content: str) -> Any:
        name = self._profile(target)
        return await self._client._request(
            "PUT",
            f"/api/profiles/{name}/soul",
            body={"content": content},
        )

    async def put_profile_description(
        self, target: ConfigTarget, description: str
    ) -> Any:
        name = self._profile(target)
        return await self._client._request(
            "PUT",
            f"/api/profiles/{name}/description",
            body={"description": description},
        )

    async def put_profile_model(
        self, target: ConfigTarget, provider: str, model: str
    ) -> Any:
        name = self._profile(target)
        return await self._client._request(
            "PUT",
            f"/api/profiles/{name}/model",
            body={"provider": provider, "model": model},
        )

    async def describe_profile_auto(
        self, target: ConfigTarget, *, overwrite: bool = False
    ) -> Any:
        name = self._profile(target)
        return await self._client._request(
            "POST",
            f"/api/profiles/{name}/describe-auto",
            body={"overwrite": overwrite},
        )

    async def export_profile(self, target: ConfigTarget, output: str) -> HostPathResult:
        name = self._profile(target)
        if not self._loopback():
            return HostPathResult(
                available=False,
                reason="remote_unavailable",
                detail=EXPORT_HOST_PATH_NOTE,
            )
        payload = await self._client._request(
            "POST",
            f"/api/profiles/{name}/export",
            body={"output": output},
        )
        archive = None
        if isinstance(payload, Mapping):
            raw = payload.get("archive")
            archive = raw if isinstance(raw, str) else None
        return HostPathResult(
            available=True,
            reason="ok",
            detail=EXPORT_HOST_PATH_NOTE,
            archive=archive,
            payload=payload,
        )

    async def import_profile(
        self,
        scope: HostScope,
        archive: str,
        *,
        name: str | None = None,
    ) -> HostPathResult:
        self._require_host(scope)
        if not self._loopback():
            return HostPathResult(
                available=False,
                reason="remote_unavailable",
                detail=IMPORT_HOST_PATH_NOTE,
            )
        body: dict[str, Any] = {"archive": archive}
        if name:
            body["name"] = name
        payload = await self._host(
            scope,
            "POST",
            "/api/profiles/import",
            body=body,
        )
        imported = None
        if isinstance(payload, Mapping):
            raw = payload.get("name")
            imported = raw if isinstance(raw, str) else None
        return HostPathResult(
            available=True,
            reason="ok",
            detail=IMPORT_HOST_PATH_NOTE,
            name=imported,
            payload=payload,
        )

    async def host_update_check(self, scope: HostScope) -> Any:
        return await self._host(
            scope, "GET", _host_path("api", "hermes", "update", "check")
        )

    async def host_update_receipt(self, scope: HostScope) -> Any:
        return await self._host(
            scope, "GET", _host_path("api", "hermes", "update", "receipt")
        )

    async def host_migrate_plan(self, scope: HostScope) -> Any:
        return await self._host(
            scope, "GET", _host_path("api", "gateway", "migrate", "plan")
        )

    async def local_models_status(self, scope: HostScope) -> Any:
        return await self._host(scope, "GET", "/api/local-models/status")

    async def local_models_catalog(self, scope: HostScope) -> Any:
        return await self._host(scope, "GET", "/api/local-models/catalog")

    async def local_models_hardware(self, scope: HostScope) -> Any:
        return await self._host(scope, "GET", "/api/local-models/hardware")

    async def local_models_jobs(self, scope: HostScope) -> Any:
        return await self._host(scope, "GET", "/api/local-models/jobs")

    async def local_models_search(self, scope: HostScope, query: str) -> Any:
        return await self._host(
            scope,
            "GET",
            "/api/local-models/search",
            params={"q": query},
        )
