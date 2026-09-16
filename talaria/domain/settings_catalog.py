"""Finite CFG-A1 §10 coverage manifest.

Tier 1 is the curated Desktop inventory: every class-A ``implement/verify``
control named in CFG-A1 §10. Uncurated live-schema keys fall through to
Tier 2. ``pet.*`` is an explicit exclusion (KTD11), not an omitted row.
"""

from __future__ import annotations

from dataclasses import dataclass

__all__ = [
    "CatalogEntry",
    "TIER1",
    "coverage_denominator_keys",
    "surface_disposition",
    "tier_of",
]


@dataclass(frozen=True)
class CatalogEntry:
    """One curated coverage row. Omission is not an exclusion."""

    key: str
    owner: str
    route: str
    mechanism: str
    scope: str
    control: str
    effect: str
    disposition: str
    rationale: str


def _entry(
    key: str,
    *,
    owner: str = "hermes-profile",
    route: str = "PUT /api/config",
    mechanism: str = "query",
    scope: str = "profile",
    control: str,
    effect: str = "next-session",
    rationale: str,
) -> CatalogEntry:
    return CatalogEntry(
        key=key,
        owner=owner,
        route=route,
        mechanism=mechanism,
        scope=scope,
        control=control,
        effect=effect,
        disposition="implement",
        rationale=rationale,
    )


TIER1: tuple[CatalogEntry, ...] = (
    # Model — CFG-A1 §10.1
    _entry(
        "model_context_length",
        control="number field",
        rationale="Desktop Model context window (0 = auto).",
    ),
    _entry(
        "fallback_providers",
        control="list editor",
        rationale="Desktop Model fallback provider list.",
    ),
    _entry(
        "model.main",
        route="POST /api/model/set",
        control="model picker overlay",
        rationale="Main provider/model assignment via the model route.",
    ),
    _entry(
        "model.auxiliary",
        route="POST /api/model/set",
        control="auxiliary slot overlay",
        rationale="Eleven auxiliary task slots use the same model-set route.",
    ),
    _entry(
        "model.moa",
        route="PUT /api/model/moa",
        control="MoA preset overlay",
        rationale="Mixture-of-Agents presets are a first-class model control.",
    ),
    _entry(
        "agent.service_tier",
        control="select",
        rationale="Desktop Model fast-tier control.",
    ),
    # Chat
    _entry(
        "display.personality",
        control="string field",
        effect="live",
        rationale="Chat personality; also live via config.set.",
    ),
    _entry(
        "timezone",
        control="string field",
        rationale="Chat timezone for timestamps.",
    ),
    _entry(
        "display.show_reasoning",
        control="boolean field",
        rationale="Chat reasoning-block visibility.",
    ),
    _entry(
        "agent.image_input_mode",
        control="select",
        rationale="Chat image-attachment mode.",
    ),
    # Appearance — resume last session is Tier 1 even though the rest of
    # the Desktop Appearance page is Desktop-local.
    _entry(
        "display.resume_last_session",
        control="boolean field",
        rationale="Appearance resume-last-session; first-class, not generic fallback.",
    ),
    # Workspace
    _entry(
        "terminal.cwd",
        control="string field",
        effect="live",
        rationale="Workspace working directory; live via config.set cwd.",
    ),
    _entry(
        "desktop.repo_scan_enabled",
        control="boolean field",
        rationale="Workspace repo discovery toggle (Hermes config consumed by Desktop).",
    ),
    _entry(
        "desktop.repo_scan_roots",
        control="list editor",
        rationale="Workspace repo discovery roots.",
    ),
    _entry(
        "desktop.repo_scan_exclude_paths",
        control="list editor",
        rationale="Workspace repo discovery excludes.",
    ),
    _entry(
        "code_execution.mode",
        control="select",
        rationale="Workspace code-execution mode.",
    ),
    _entry(
        "terminal.persistent_shell",
        control="boolean field",
        rationale="Workspace persistent shell.",
    ),
    _entry(
        "terminal.env_passthrough",
        control="boolean field",
        rationale="Workspace environment passthrough.",
    ),
    _entry(
        "file_read_max_chars",
        control="number field",
        rationale="Workspace file-read limit.",
    ),
    # Safety
    _entry(
        "approvals.mode",
        control="select",
        effect="live",
        rationale="Safety approval mode; live via session.info broadcast.",
    ),
    _entry(
        "approvals.timeout",
        control="number field",
        rationale="Safety approval timeout.",
    ),
    _entry(
        "approvals.mcp_reload_confirm",
        control="boolean field",
        rationale="Safety confirm-MCP-reloads.",
    ),
    _entry(
        "command_allowlist",
        control="list editor",
        rationale="Safety command allowlist.",
    ),
    _entry(
        "security.redact_secrets",
        control="boolean field",
        rationale="Safety redact-secrets.",
    ),
    _entry(
        "security.allow_private_urls",
        control="boolean field",
        rationale="Safety allow-private-URLs.",
    ),
    _entry(
        "checkpoints.enabled",
        control="boolean field",
        rationale="Safety file checkpoints.",
    ),
    # Browser
    _entry(
        "browser.use_real_profile",
        control="boolean field",
        rationale="Browser use-real-profile.",
    ),
    _entry(
        "browser.allow_private_urls",
        control="boolean field",
        rationale="Browser private URLs.",
    ),
    _entry(
        "browser.auto_local_for_private_urls",
        control="boolean field",
        rationale="Browser local-for-private.",
    ),
    _entry(
        "vault.sources",
        route="RPC vault.*",
        mechanism="params",
        control="vault overlay",
        rationale="Browser vault sources; secrets only in vault.add.",
    ),
    # Memory & Context
    _entry(
        "memory.memory_enabled",
        control="boolean field",
        rationale="Memory on/off.",
    ),
    _entry(
        "memory.user_profile_enabled",
        control="boolean field",
        rationale="Memory user-profile toggle.",
    ),
    _entry(
        "memory.memory_char_limit",
        control="number field",
        rationale="Memory budget.",
    ),
    _entry(
        "memory.user_char_limit",
        control="number field",
        rationale="User-profile memory budget.",
    ),
    _entry(
        "memory.provider",
        control="select",
        rationale="Memory provider.",
    ),
    _entry(
        "memory.providers",
        route="PUT /api/memory/providers/{name}/config",
        control="memory-provider overlay",
        rationale="Declared memory-provider config; OAuth blocked when callback is unreachable.",
    ),
    _entry(
        "context.engine",
        control="select",
        rationale="Context engine.",
    ),
    _entry(
        "compression.enabled",
        control="boolean field",
        rationale="Context compression toggle from Memory & Context.",
    ),
    # Voice
    _entry(
        "voice.voice_chat_mode",
        control="select",
        effect="live",
        rationale="Voice chat mode; live via config.set.",
    ),
    _entry(
        "tts.provider",
        control="select",
        rationale="TTS provider; first-class Voice control.",
    ),
    _entry(
        "stt.enabled",
        control="boolean field",
        rationale="STT enable.",
    ),
    _entry(
        "stt.echo_transcripts",
        control="boolean field",
        rationale="STT echo transcripts.",
    ),
    _entry(
        "stt.provider",
        control="select",
        rationale="STT provider.",
    ),
    _entry(
        "voice.auto_tts",
        control="boolean field",
        rationale="Read-aloud / auto TTS.",
    ),
    _entry(
        "voice.record_key",
        control="string field",
        rationale="Voice record shortcut.",
    ),
    _entry(
        "voice.max_recording_seconds",
        control="number field",
        rationale="Voice max recording length.",
    ),
    _entry(
        "voice.client_direct",
        control="boolean field",
        rationale="Voice client-direct.",
    ),
    _entry(
        "wake.control",
        route="RPC wake.start|stop|status",
        mechanism="params",
        control="wake-word toggle",
        effect="gateway-restart",
        rationale="Wake-word RPC with explicit profile; schema wake_word.* stays Tier 2.",
    ),
    _entry(
        "audio.elevenlabs.voices",
        route="GET /api/audio/elevenlabs/voices",
        control="voice-id catalog select",
        rationale="ElevenLabs voice catalog feeds the voice-id select.",
    ),
    # Advanced
    _entry(
        "toolsets",
        control="list editor",
        rationale="Advanced toolsets.",
    ),
    _entry(
        "terminal.backend",
        route="GET /api/tools/terminal-backends",
        control="terminal backend picker",
        rationale="Advanced terminal backend; options from the tools route.",
    ),
    _entry(
        "terminal.timeout",
        control="number field",
        rationale="Advanced terminal timeout.",
    ),
    _entry(
        "tool_output.max_bytes",
        control="number field",
        rationale="Advanced tool-output cap.",
    ),
    _entry(
        "checkpoints.max_snapshots",
        control="number field",
        rationale="Advanced checkpoint snapshot cap.",
    ),
    _entry(
        "agent.max_turns",
        control="number field",
        rationale="Advanced agent max turns.",
    ),
    _entry(
        "agent.api_max_retries",
        control="number field",
        rationale="Advanced agent retries.",
    ),
    _entry(
        "agent.tool_use_enforcement",
        control="select",
        rationale="Advanced tool-use enforcement.",
    ),
    _entry(
        "delegation.model",
        control="string field",
        rationale="Advanced delegation model.",
    ),
    _entry(
        "updates.non_interactive_local_changes",
        control="select",
        rationale="Advanced update local-change policy.",
    ),
    # Sessions
    _entry(
        "sessions.auto_archive",
        control="boolean field",
        rationale="Sessions auto-archive.",
    ),
    _entry(
        "sessions.auto_archive_days",
        control="number field",
        rationale="Sessions auto-archive days.",
    ),
    # Bespoke pages — CFG-A1 §10.2
    _entry(
        "providers.accounts",
        route="POST /api/providers/oauth/{id}/start",
        control="provider overlay",
        rationale="Provider OAuth connect/poll/submit/disconnect.",
    ),
    _entry(
        "env.keys",
        route="PUT /api/env",
        control="key rows",
        rationale="Provider and tool API keys: set/clear/reveal.",
    ),
    _entry(
        "providers.custom_endpoints",
        route="/api/providers/custom-endpoints",
        control="custom-endpoint overlay",
        rationale="Custom endpoint CRUD/activate/validate.",
    ),
    _entry(
        "config.reset",
        route="PUT /api/config",
        control="profile-explicit Reset overlay",
        rationale="Reset names its target; never the ambient launch profile.",
    ),
    _entry(
        "config.export",
        route="GET /api/config",
        control="loopback host-path export",
        rationale="Profile export is loopback-only with explicit host-path wording.",
    ),
    _entry(
        "config.import",
        route="PUT /api/config",
        control="loopback host-path import",
        rationale="Profile import is loopback-only with explicit host-path wording.",
    ),
    # Related surfaces — CFG-A1 §10.3
    _entry(
        "messaging.platforms",
        route="PUT /api/messaging/platforms/{id}",
        control="messaging overlay",
        effect="gateway-restart",
        rationale="Messaging platform enable/env; WhatsApp onboarding is host-side.",
    ),
    _entry(
        "skills",
        route="/api/skills",
        control="skills overlay",
        rationale="Skills list/toggle/hub install.",
    ),
    _entry(
        "toolsets.catalog",
        route="/api/tools",
        control="toolsets overlay",
        rationale="Toolset enable/config/models.",
    ),
    _entry(
        "mcp.servers",
        route="/api/mcp/servers",
        control="MCP overlay",
        rationale="MCP server CRUD/enable/test/catalog.",
    ),
    _entry(
        "cron.jobs",
        route="/api/cron",
        control="cron overlay",
        effect="gateway-restart",
        rationale="Cron jobs fire from the gateway process.",
    ),
    _entry(
        "profiles.clone",
        route="POST /api/profiles",
        mechanism="body",
        control="clone overlay",
        rationale="Clone with explicit source and destination names.",
    ),
    _entry(
        "profiles.rename",
        route="PATCH /api/profiles/{name}",
        mechanism="path",
        control="rename overlay",
        rationale="Rename uses the path identity.",
    ),
    _entry(
        "profiles.delete",
        route="DELETE /api/profiles/{name}",
        mechanism="path",
        control="delete overlay",
        rationale="Delete uses the path identity; default stays server-protected.",
    ),
    _entry(
        "agent.plugins",
        owner="hermes-host",
        route="/api/dashboard/agent-plugins",
        mechanism="host",
        scope="host",
        control="host plugin status",
        effect="dashboard-restart-no-api",
        rationale="Agent plugins are host-owned dashboard controls.",
    ),
)

_TIER1_KEYS: frozenset[str] = frozenset(entry.key for entry in TIER1)


def tier_of(key: str) -> int:
    """1 when curated, 2 when a remaining live-schema key."""
    if key in _TIER1_KEYS:
        return 1
    return 2


def surface_disposition(key: str) -> str:
    """Explicit exclusion or implement; uncurated keys are not omitted."""
    if key == "pet.enabled" or key.startswith("pet."):
        return "excluded"
    if key in _TIER1_KEYS:
        return "implement"
    return "tier2"


def coverage_denominator_keys(schema_keys: tuple[str, ...]) -> frozenset[str]:
    """Curated manifest plus every supplied live-schema key."""
    return _TIER1_KEYS | frozenset(schema_keys)
