#!/usr/bin/env python3
"""Offline, deterministic bindings for the vendored Hermes OpenRPC contract."""
import argparse
import ast
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

TYPIFY_VERSION = "0.8.0"
ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / "crates/hermes-contract"
CONTRACT = CRATE / "vendor/gateway-contract.openrpc.json"
OUT = CRATE / "src/generated"
OWNER_FILE = ROOT / "tools/contract-gen/native-owner-methods.txt"
CONFIG_CHOICES_FILE = ROOT / "tools/contract-gen/config-choices.json"
CONFIG_DEFAULTS = "hermes_cli/config_defaults.py"
SCHEMA_SOURCE = "hermes_cli/web_server_config.py"
DESKTOP_CHOICES = "apps/desktop/src/app/settings/constants.ts"
REPO = "NousResearch/hermes-agent"
CONTRACT_PATH = "apps/shared/src/gateway-contract.openrpc.json"
CATALOGUES = ("methods", "x-canonical-methods", "x-notifications", "x-server-requests")
HAND_WRITTEN_EVENTS = {
    "approval.request": "ApprovalRequest", "approval.settled": "ApprovalSettled",
    "clarify.request": "ClarifyRequest", "clarify.settled": "ClarifySettled",
    "session.replay_gap": "SessionReplayGap",
}
# Each exception is deliberately local: unrelated defaults retain their semantics.
SCHEMA_REMOVALS = (
    ("DisplayLease", "viewer_id", "default", "typify rejects a null-only default"),
    ("MessageCompletePayload", "text", "default", "typify rejects a string-or-any default"),
    ("ProviderUsageWindow", "scope", "default", "Scope enum default conflicts in the full lifted document"),
    ("CanonicalCorrectionAck", "status", "title", "distinct inline Status enums must not collapse"),
    ("PromptResponseResult", "status", "title", "distinct inline Status enums must not collapse"),
    ("ProviderLimit", "scope", "title", "distinct inline Scope enums must not collapse"),
    ("ProviderUsageWindow", "scope", "title", "distinct inline Scope enums must not collapse"),
    ("SessionAutomationParams", "kind", "title", "distinct inline Kind enums must not collapse"),
    ("CanonicalPendingPrompt", "kind", "title", "distinct inline Kind enums must not collapse"),
    ("CanonicalBotCancelResult", "status", "title", "distinct inline Status enums must not collapse"),
    ("ImageRenderResult", "mode", "title", "distinct inline Mode enums must not collapse"),
)
KNOWN_DISCRIMINATORS = {
    "ConnectorPolicyLayer.body": "mode",
    "ConnectorPolicyGetResult.effective": "mode",
    "ConnectorPolicySetResult.effective": "mode",
    "ConnectionUpdatePayload.owner": "type",
    "GoalSnapshot.wait_barrier": "type",
    "ConnectionOperationParams.owner": "type",
    "ConnectionRespondParams.owner": "type",
    "ConnectorsConnectParams.owner": "type",
    "ConnectorsListParams.owner": "type",
    "ConnectorPolicySetParams.change": "type",
}
RECEIVED_CONSTRAINED_TYPES = {"SessionOwner"}
# Hermes v0.22.4 tui_gateway/ws_legacy_fallback.py:32-59, full interactive grant.
PROCESS_GLOBAL = {"process.stop", "delegation.pause", "reload.mcp", "reload.env", "agents.list"}
SESSION_NAMESPACES = ("session.", "prompt.", "message.")
SESSION_FALLBACK_ALLOWED = {
    "session.most_recent", "session.active_list", "session.events.stats",
    "session.foreign.list", "session.foreign.preview", "session.foreign.import",
}
KEYWORDS = set("as break const continue crate else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while async await dyn abstract become box do final macro override priv typeof unsized virtual yield try gen".split())


class ContractError(Exception):
    """An expected user-facing failure."""

    def __init__(self, message, stderr=""):
        super().__init__(message)
        self.stderr = stderr


class PinError(ContractError):
    """Invalid pinned version or commit."""


class CheckoutError(ContractError):
    """Checkout does not match the pin."""


class Pin:
    def __init__(self, tag, commit):
        self.tag, self.commit = tag, commit


def run(args, cwd=None, env=None):
    try:
        result = subprocess.run([str(a) for a in args], cwd=cwd, env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    except FileNotFoundError:
        raise ContractError("{} is not installed".format(args[0])) from None
    if result.returncode:
        # Collapse diagnostics to one line; paths and tracebacks are not CLI output.
        stderr = result.stderr.decode(errors="replace")
        detail = stderr.strip().splitlines()
        raise ContractError("{} failed: {}".format(args[0], " ".join(detail[-3:])), stderr=stderr)
    return result.stdout


def version(tag):
    match = re.fullmatch(r"v(\d+)\.(\d+)\.(\d+)", tag)
    if not match:
        raise ContractError("invalid stable tag {}".format(tag))
    return tuple(int(n) for n in match.groups())


def read_pin(path=None):
    pin = parse_pinned((Path(path) if path else CRATE / "PINNED").read_text())
    return pin.tag, pin.commit


def parse_pinned(text):
    lines = text.splitlines()
    if len(lines) != 2 or not lines[0].startswith("tag=") or not re.fullmatch(r"commit=[0-9a-f]{40}", lines[1]):
        raise PinError("invalid PINNED: expected tag and 40 lowercase hex commit")
    tag, commit = lines[0][4:], lines[1][7:]
    if version(tag) < (0, 22, 3):
        raise PinError("PINNED tag must be v0.22.3 or later")
    return Pin(tag, commit)


def validate_pinned(pin):
    parse_pinned("tag={}\ncommit={}\n".format(pin.tag, pin.commit))


def compare_versions(left, right):
    return (version(left) > version(right)) - (version(left) < version(right))


def snake_case(name):
    value = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name).replace(".", "_").lower()
    return value + "_" if value in KEYWORDS or value[:1].isdigit() else value


def pascal_case(name):
    value = "".join(part[:1].upper() + part[1:] for part in re.split(r"[._]", name))
    return value + "_" if value in KEYWORDS or value[:1].isdigit() else value


def walk(value):
    yield value
    if isinstance(value, dict):
        for child in value.values():
            yield from walk(child)
    elif isinstance(value, list):
        for child in value:
            yield from walk(child)


def schema_name(schema):
    return schema["$ref"].rsplit("/", 1)[1]


def rust_type_name(schema):
    return pascal_case(schema_name(schema))


def references(value):
    return {schema_name(node) for node in walk(value) if isinstance(node, dict) and "$ref" in node}


def validate(contract):
    schemas = contract["components"]["schemas"]
    for catalogue in CATALOGUES:
        seen = {}
        for entry in contract[catalogue]:
            name = entry["name"]
            if catalogue == "x-notifications" and name in HAND_WRITTEN_EVENTS:
                raise ContractError("the contract now declares {}; remove it from the hand-written list".format(name))
            converted = pascal_case(name) if catalogue == "x-notifications" else snake_case(name)
            if converted in seen:
                raise ContractError("name collision: {} and {} become {}".format(seen[converted], name, converted))
            seen[converted] = name
    for name, schema in schemas.items():
        for prop, value in schema.get("properties", {}).items():
            for node in walk(value):
                if isinstance(node, dict) and "discriminator" in node:
                    site = name + "." + prop
                    if KNOWN_DISCRIMINATORS.get(site) != node["discriminator"].get("propertyName"):
                        raise ContractError("unlisted discriminated union {}".format(site))
    received = set()
    for catalogue in CATALOGUES:
        for entry in contract[catalogue]:
            root = entry.get("result", {}) if catalogue in CATALOGUES[:2] else entry.get("params", [])
            received.update(references(root))
    pending = list(received)
    while pending:
        name = pending.pop()
        for ref in references(schemas[name]) - received:
            received.add(ref)
            pending.append(ref)
    for name in sorted(received):
        constrained = any(isinstance(n, dict) and any(k in n for k in ("pattern", "minLength", "maxLength")) for n in walk(schemas[name]))
        if constrained and name not in RECEIVED_CONSTRAINED_TYPES:
            raise ContractError("unlisted received constrained string {}".format(name))
    # The approved discriminator rule preserves const semantics in typify 0.8.0.
    for name, prop in discriminator_properties(contract):
        value = schemas.get(name, {}).get("properties", {}).get(prop, {})
        const, enum = "const" in value, "enum" in value
        valid_const = const and isinstance(value["const"], str)
        valid_enum = enum and isinstance(value["enum"], list) and bool(value["enum"]) and all(isinstance(v, str) for v in value["enum"])
        if const == enum or not (valid_const or valid_enum) or value.get("type") != "string":
            raise ContractError("invalid discriminator property {}.{}: expected a string const or string enum".format(name, prop))
    titles = {}
    for node in walk(preprocess_schemas(contract)["$defs"]):
        if isinstance(node, dict) and node.get("type") == "string" and "enum" in node and "title" in node:
            values = frozenset(node["enum"])
            title = node["title"]
            if title in titles and titles[title] != values:
                raise ContractError("inline string enum title collision: {}".format(title))
            titles[title] = values


def discriminator_properties(contract):
    schemas = contract["components"]["schemas"]
    properties = set()
    for site in KNOWN_DISCRIMINATORS:
        name, prop = site.split(".")
        value = schemas.get(name, {}).get("properties", {}).get(prop, {})
        for node in walk(value):
            if not isinstance(node, dict) or "discriminator" not in node:
                continue
            discriminator = node["discriminator"]
            key = discriminator["propertyName"]
            for ref in discriminator.get("mapping", {}).values():
                properties.add((ref.rsplit("/", 1)[1], key))
    return sorted(properties)


def lift_schemas(contract):
    schemas = copy.deepcopy(contract["components"]["schemas"])
    for node in walk(schemas):
        if isinstance(node, dict):
            if node.get("additionalProperties") is False:
                del node["additionalProperties"]
            if "$ref" in node:
                node["$ref"] = node["$ref"].replace("#/components/schemas/", "#/$defs/")
    return {"$schema": "https://json-schema.org/draft/2020-12/schema", "$defs": schemas}


def preprocess_schemas(contract):
    document = lift_schemas(contract)
    schemas = document["$defs"]
    for name, prop, keyword, _reason in SCHEMA_REMOVALS:
        if name in schemas and prop in schemas[name].get("properties", {}):
            schemas[name]["properties"][prop].pop(keyword, None)
    # This union admits every JSON value. Typify 0.8.0 instead emits flattened
    # optional fields that reject scalar text; normalize the equivalent schema
    # at the recorded received site, without changing unrelated unions.
    text = schemas.get("MessageCompletePayload", {}).get("properties", {}).get("text", {})
    if {} in text.get("anyOf", []):
        del text["anyOf"]
    # Hermes gateway/session_mutation_model.py:329 and session_mutations.py:316
    # at the pin return open mutation projections, beyond the declared receipt.
    if "SessionMutateResult" in schemas:
        schemas["SessionMutateResult"]["additionalProperties"] = {}
    # Architecture ruling for section 4.1: only discriminator const properties
    # become singleton enums, with local titles removed to avoid type collapse.
    for name, prop in discriminator_properties(contract):
        value = schemas[name]["properties"][prop]
        if "const" in value:
            value["enum"] = [value.pop("const")]
            value.pop("title", None)
    return document


def root_anyof_document(document):
    document = copy.deepcopy(document)
    document["anyOf"] = [{"$ref": "#/$defs/" + name} for name in sorted(document["$defs"])]
    return document


def lift(contract, root=False):
    document = preprocess_schemas(contract)
    return root_anyof_document(document) if root else document


def rejected_keyword_sites():
    return [row[:3] for row in SCHEMA_REMOVALS]


def known_union_sites():
    return sorted(KNOWN_DISCRIMINATORS)


def known_received_constrained_types():
    return sorted(RECEIVED_CONSTRAINED_TYPES)


validate_contract = validate
rust_module_name = snake_case
rust_variant_name = pascal_case


def refuse_identifier_collisions(names):
    for convert in (snake_case, pascal_case):
        seen = {}
        for name in names:
            identifier = convert(name)
            if identifier in seen:
                raise ContractError("name collision: {} and {}".format(seen[identifier], name))
            seen[identifier] = name


def method_catalogue(contract):
    return {transport: [{"name": entry["name"], "module": snake_case(entry["name"]),
                         "params": schema_name(entry["params"][0]["schema"]),
                         "result": schema_name(entry["result"]["schema"])} for entry in contract[cat]]
            for transport, cat in (("legacy", "methods"), ("canonical", "x-canonical-methods"))}


def notification_model(contract):
    notifications = [{"name": e["name"], "variant": pascal_case(e["name"]),
                      "dataless": "$ref" not in e["params"][0]["schema"]} for e in contract["x-notifications"]]
    notifications += [{"name": n, "variant": t, "dataless": False} for n, t in HAND_WRITTEN_EVENTS.items()]
    return {"notifications": notifications, "unknown_variant": "Unknown",
            "server_requests": [{"name": e["name"], "module": snake_case(e["name"]), "variant": pascal_case(e["name"])} for e in contract["x-server-requests"]]}


def native_method(name, owners):
    return name in owners or (name not in PROCESS_GLOBAL and
                             (not name.startswith(SESSION_NAMESPACES) or name in SESSION_FALLBACK_ALLOWED))


is_native_legacy = native_method


def is_native(transport, name, owner_methods):
    if transport == "canonical":
        return True
    canonical = {e["name"] for e in json.loads(CONTRACT.read_bytes())["x-canonical-methods"]}
    return transport == "legacy" and name not in canonical and native_method(name, owner_methods)


def non_native_legacy_names(contract):
    canonical = {e["name"] for e in contract["x-canonical-methods"]}
    owners = OWNER_FILE.read_text().splitlines()
    return sorted(e["name"] for e in contract["methods"] if e["name"] not in canonical and not native_method(e["name"], owners))


def require_pinned_checkout(checkout, commit):
    checkout = Path(checkout)
    if (checkout / ".git").exists():
        actual = run(["git", "-C", checkout, "rev-parse", "HEAD"]).decode().strip()
        if actual != commit:
            raise CheckoutError("Hermes checkout HEAD {} does not match PINNED {}".format(actual, commit))


def extract_owner_methods(checkout, commit=None):
    checkout = Path(checkout)
    if commit:
        require_pinned_checkout(checkout, commit)
    gateway = checkout / "gateway"
    names = set()

    def strings(node, bindings):
        if isinstance(node, ast.Name):
            return strings(bindings.get(node.id), bindings)
        if isinstance(node, ast.Dict):
            keys = {k.value for k in node.keys if isinstance(k, ast.Constant) and isinstance(k.value, str)}
            for key, value in zip(node.keys, node.values):
                if key is None:
                    keys.update(strings(value, bindings))
            return keys
        if isinstance(node, (ast.Tuple, ast.List, ast.Set)):
            return {v.value if isinstance(v, ast.Constant) else v.elts[0].value
                    for v in node.elts if (isinstance(v, ast.Constant) and isinstance(v.value, str))
                    or (isinstance(v, (ast.Tuple, ast.List)) and v.elts and isinstance(v.elts[0], ast.Constant) and isinstance(v.elts[0].value, str))}
        if isinstance(node, ast.DictComp):
            return set().union(*(strings(g.iter, bindings) for g in node.generators))
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr in ("items", "keys"):
            return strings(node.func.value, bindings)
        return set()

    modules = ["session_controls", "session_busy_controls", "session_queued_controls", "session_config",
               "session_tui_config", "session_ancillary", "session_shell", "session_tui_ops",
               "session_tui_side", "session_llm_oneshot", "session_connections", "session_group_controls"]
    for module in modules:
        file = gateway / (module + ".py")
        if not file.exists():
            continue
        tree = ast.parse(file.read_text())
        bindings = {target.id: node.value for node in tree.body if isinstance(node, ast.Assign)
                    for target in node.targets if isinstance(target, ast.Name)}
        for constant in ("GROUP_METHODS", "METHODS"):
            if (module == "session_group_controls" and constant == "GROUP_METHODS") or (module == "session_connections" and constant == "METHODS"):
                names.update(strings(bindings.get(constant), bindings))
        for function in ast.walk(tree):
            if not isinstance(function, (ast.FunctionDef, ast.AsyncFunctionDef)) or function.name != "handlers":
                continue
            local = dict(bindings)
            for node in ast.walk(function):
                if isinstance(node, ast.Assign):
                    for target in node.targets:
                        if isinstance(target, ast.Name):
                            local[target.id] = node.value
                        elif isinstance(target, ast.Subscript) and isinstance(target.value, ast.Name) and target.value.id == "handlers":
                            key = target.slice
                            if isinstance(key, ast.Constant) and isinstance(key.value, str):
                                names.add(key.value)
            for node in ast.walk(function):
                if isinstance(node, ast.Return):
                    names.update(strings(node.value, local))
    if (gateway / "session_controls.py").exists():
        names.add("profiles.list")
    return sorted(names)


owner_methods_from_checkout = extract_owner_methods


# The /config choice proof. Talaria's built-in choice table
# (crates/talaria-core/src/settings/choices.rs) offers closed value sets that the Hermes schema
# does not carry. Each recipe below says where the pinned Hermes source states that set:
#   constant  a module-level literal (select: "keys", "names" or one dict item)
#   function  string literals inside one function
#   comment   the comment block at a DEFAULT_CONFIG key, or at an anchor text
#   desktop   the desktop app's per-key suggestion list
# Every _select in the schema overrides is recorded too, so a test can tell when Hermes fixes a
# list that Talaria replaces. A missing constant, function, anchor or value fails generation.
CONFIG_CHOICE_SOURCES = [
    {"key": "agent.coding_context", "kind": "comment", "values": ["auto", "focus", "on", "off"]},
    {"key": "agent.image_input_mode", "kind": "comment", "values": ["auto", "native", "text"]},
    {"key": "agent.service_tier", "kind": "constant", "file": "agent/fast_mode.py", "name": "SERVICE_TIER_WORDS", "select": "keys", "match": "subset"},
    {"key": "agent.text_verbosity", "kind": "comment", "values": ["low", "medium", "high"]},
    {"key": "approvals.cron_mode", "kind": "comment", "anchor": "cron_mode / single_query_mode / unattended_mode: deny | approve", "values": ["deny", "approve"]},
    {"key": "approvals.mode", "kind": "constant", "file": "hermes_cli/approval_mode.py", "name": "VALID_APPROVAL_MODES"},
    {"key": "approvals.single_query_mode", "kind": "comment", "anchor": "cron_mode / single_query_mode / unattended_mode: deny | approve", "values": ["deny", "approve"]},
    {"key": "approvals.unattended_mode", "kind": "comment", "anchor": "cron_mode / single_query_mode / unattended_mode: deny | approve", "values": ["deny", "approve"]},
    {"key": "auth.codex_login_flow", "kind": "comment", "values": ["device_code", "browser"]},
    {"key": "bedrock.guardrail.stream_processing_mode", "kind": "comment", "values": ["async", "sync"]},
    {"key": "bedrock.guardrail.trace", "kind": "comment", "values": ["disabled", "enabled", "enabled_full"]},
    {"key": "bot_desktop.placement", "kind": "comment", "values": ["auto", "terminal", "gateway"]},
    {"key": "browser.dialog_policy", "kind": "comment", "values": ["must_respond", "auto_dismiss", "auto_accept"]},
    {"key": "browser.engine", "kind": "comment", "values": ["auto", "lightpanda"]},
    {"key": "code_execution.mode", "kind": "comment", "values": ["project", "strict"]},
    {"key": "compression.codex_app_server_auto", "kind": "comment", "values": ["native", "hermes", "off"]},
    {"key": "compression.tail_mode", "kind": "comment", "values": ["lean", "legacy"]},
    {"key": "computer_use.capture_after_mode", "kind": "comment", "values": ["som", "ax", "vision"]},
    {"key": "computer_use.no_overlay", "kind": "comment", "values": ["true", "false"]},
    {"key": "computer_use.permission_mode", "kind": "comment", "values": ["standard", "bounded"]},
    {"key": "context.engine", "kind": "comment", "anchor": "\"compressor\" =", "values": ["compressor"]},
    {"key": "dashboard.theme", "kind": "constant", "file": "hermes_cli/web_server_dashboard.py", "name": "_BUILTIN_DASHBOARD_THEMES", "select": "names"},
    {"key": "database.journal_mode", "kind": "function", "file": "hermes_state_wal.py", "function": "resolve_journal_mode", "values": ["wal", "delete"]},
    {"key": "desktop.disable_gpu", "kind": "comment", "values": ["auto", "true", "false"]},
    {"key": "desktop.ozone_platform_hint", "kind": "comment", "values": ["auto", "x11", "wayland"]},
    {"key": "desktop.password_store", "kind": "comment", "values": ["auto", "gnome-libsecret", "kwallet", "kwallet5", "kwallet6", "basic"]},
    {"key": "display.background_process_notifications", "kind": "comment", "values": ["concise", "all", "result", "error", "off"]},
    {"key": "display.cli_layout", "kind": "comment", "values": ["modern", "classic"]},
    {"key": "display.copy_shortcut", "kind": "comment", "values": ["auto", "ctrl_c", "ctrl_shift_c", "disabled"]},
    {"key": "display.final_response_markdown", "kind": "comment", "values": ["strip", "render", "raw"]},
    {"key": "display.focus_saved_tool_progress", "kind": "constant", "file": "tui_gateway/agent_factory.py", "name": "_TOOL_PROGRESS_MODES"},
    {"key": "display.inline_images", "kind": "comment", "values": ["auto", "kitty", "iterm", "unicode", "off"]},
    {"key": "display.interface", "kind": "comment", "values": ["cli", "tui"]},
    {"key": "display.memory_notifications", "kind": "comment", "values": ["on", "off", "verbose"]},
    {"key": "display.personality", "kind": "constant", "file": "hermes_cli/personality.py", "name": "BUILTIN_PERSONALITIES", "select": "keys"},
    {"key": "display.pet.render_mode", "kind": "constant", "file": "agent/pet/render.py", "name": "RENDER_MODES"},
    {"key": "display.reasoning_style", "kind": "comment", "values": ["code", "blockquote", "subtext"]},
    {"key": "display.skin", "kind": "constant", "file": "hermes_cli/skin_engine.py", "name": "_BUILTIN_SKINS", "select": "keys"},
    {"key": "display.tool_progress_grouping", "kind": "comment", "values": ["accumulate", "separate"]},
    {"key": "display.tui_status_indicator", "kind": "constant", "file": "hermes_constants.py", "name": "INDICATOR_STYLES"},
    {"key": "gateway.service_install_choice", "kind": "comment", "values": ["install", "decline"]},
    {"key": "human_delay.mode", "kind": "function", "file": "gateway/run_config_loaders.py", "function": "_human_delay_from_config", "values": ["off", "natural", "custom"]},
    {"key": "kanban.default_assignee", "kind": "comment", "values": []},
    {"key": "kanban.orchestrator_profile", "kind": "comment", "values": []},
    {"key": "local_runtime.backend", "kind": "comment", "values": ["auto", "cuda", "metal", "vulkan", "hip", "cpu"]},
    {"key": "lsp.install_strategy", "kind": "comment", "values": ["auto", "manual", "off"]},
    {"key": "lsp.package_manager", "kind": "comment", "values": ["npm", "pnpm", "yarn"]},
    {"key": "lsp.wait_mode", "kind": "comment", "values": ["document", "full"]},
    {"key": "nous.anthropic_wire", "kind": "comment", "values": ["chat", "native", "auto"]},
    {"key": "onboarding.profile_build", "kind": "comment", "values": ["ask", "off"]},
    {"key": "onboarding.run", "kind": "comment", "values": ["true", "false"]},
    {"key": "plugins.isolation", "kind": "comment", "values": ["in_process", "host"]},
    {"key": "prompt_caching.cache_ttl", "kind": "comment", "values": ["5m", "1h", "auto", "off"]},
    {"key": "proxy.credential_source", "kind": "comment", "values": ["env", "bitwarden"]},
    {"key": "security.approval.transport_fallback", "kind": "comment", "values": ["deny", "builtin"]},
    {"key": "sessions.fts_optimize_notice", "kind": "comment", "values": ["advise", "require", "off"]},
    {"key": "streaming.transport", "kind": "comment", "values": ["auto", "draft", "edit", "off"]},
    {"key": "stt.elevenlabs.model_id", "kind": "constant", "file": "tools/transcription_common.py", "name": "STT_MODEL_CATALOG", "select": "elevenlabs"},
    {"key": "stt.groq.model", "kind": "constant", "file": "tools/transcription_common.py", "name": "STT_MODEL_CATALOG", "select": "groq"},
    {"key": "stt.local.model", "kind": "constant", "file": "tools/transcription_common.py", "name": "STT_MODEL_CATALOG", "select": "local"},
    {"key": "stt.mistral.model", "kind": "constant", "file": "tools/transcription_common.py", "name": "STT_MODEL_CATALOG", "select": "mistral"},
    {"key": "stt.openai.model", "kind": "constant", "file": "tools/transcription_common.py", "name": "STT_MODEL_CATALOG", "select": "openai"},
    {"key": "stt.xai.model", "kind": "constant", "file": "tools/transcription_common.py", "name": "STT_MODEL_CATALOG", "select": "xai"},
    {"key": "terminal.degraded_mode", "kind": "comment", "values": ["warn", "fail"]},
    {"key": "terminal.home_mode", "kind": "comment", "values": ["auto", "real", "profile"]},
    {"key": "terminal.modal_mode", "kind": "constant", "file": "tools/tool_backend_helpers.py", "name": "_VALID_MODAL_MODES"},
    {"key": "terminal.vercel_runtime", "kind": "constant", "file": "tools/terminal_tool_backends.py", "name": "_SUPPORTED_VERCEL_RUNTIMES"},
    {"key": "tools.tool_search.enabled", "kind": "comment", "values": ["auto", "on", "off"]},
    {"key": "tools.tool_search.listing", "kind": "comment", "values": ["auto", "on", "off"]},
    {"key": "tts.edge.voice", "kind": "desktop"},
    {"key": "tts.elevenlabs.model_id", "kind": "comment", "values": ["eleven_multilingual_v2"]},
    {"key": "tts.gemini.voice", "kind": "desktop"},
    {"key": "tts.kittentts.model", "kind": "desktop"},
    {"key": "tts.minimax.model", "kind": "desktop"},
    {"key": "tts.neutts.device", "kind": "comment", "values": ["cpu", "cuda", "mps"]},
    {"key": "tts.neutts.model", "kind": "desktop"},
    {"key": "tts.openai.model", "kind": "constant", "file": "tools/tts_tool_openai.py", "name": "MANAGED_OPENAI_TTS_MODELS"},
    {"key": "tts.openai.voice", "kind": "comment", "values": ["alloy", "ash", "ballad", "cedar", "coral", "echo", "fable", "marin", "nova", "onyx", "sage", "shimmer", "verse"]},
    {"key": "tts.piper.voice", "kind": "desktop"},
    {"key": "tts.provider", "kind": "constant", "file": "tools/tts_command_provider.py", "name": "BUILTIN_TTS_PROVIDERS"},
    {"key": "updates.non_interactive_local_changes", "kind": "comment", "values": ["stash", "discard"]},
    {"key": "updates.parked_branch_strategy", "kind": "comment", "values": ["switch", "update_in_place"]},
    {"key": "updates.pre_update_backup", "kind": "comment", "values": ["quick", "full", "off"]},
    {"key": "vault.key_storage", "kind": "comment", "values": ["file", "keychain"]},
    {"key": "voice.gpt_live.voice", "kind": "constant", "file": "tools/voice_live.py", "name": "GPT_LIVE_VOICES"},
    {"key": "voice.submit_mode", "kind": "comment", "values": ["direct", "draft"]},
    {"key": "voice.voice_chat_mode", "kind": "comment", "values": ["chained", "gpt-live"]},
    {"key": "wake_word.capture", "kind": "comment", "values": ["auto", "local", "client"]},
    {"key": "wake_word.provider", "kind": "comment", "values": ["auto", "openwakeword", "sherpa", "porcupine"]},
    {"key": "wake_word.surface", "kind": "comment", "values": ["auto", "cli", "tui", "gui"]},
    {"key": "web.search_backend", "kind": "constant", "file": "agent/web_search_registry.py", "name": "_LEGACY_PREFERENCE"},
    {"key": "x_search.reasoning_effort", "kind": "constant", "file": "tools/x_search_tool.py", "name": "X_SEARCH_REASONING_EFFORTS"},
]


def _module(root, path):
    file = Path(root) / path
    if not file.exists():
        raise ContractError("config choices: {} is missing".format(path))
    return file.read_text(encoding="utf-8")


def _assignment(tree, name):
    for node in tree.body:
        targets = node.targets if isinstance(node, ast.Assign) else [node.target] if isinstance(node, ast.AnnAssign) else []
        if any(isinstance(t, ast.Name) and t.id == name for t in targets) and node.value is not None:
            return node.value
    return None


def _string_items(node):
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id in ("frozenset", "set", "tuple", "list") and node.args:
        return _string_items(node.args[0])
    if isinstance(node, (ast.Set, ast.Tuple, ast.List)):
        values = [e.value for e in node.elts if isinstance(e, ast.Constant) and isinstance(e.value, str)]
        return sorted(values) if isinstance(node, ast.Set) else values
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return [node.value]
    raise ContractError("config choices: not a literal list of strings")


def _dict_keys(node):
    if not isinstance(node, ast.Dict):
        raise ContractError("config choices: not a literal dict")
    return [k.value for k in node.keys if isinstance(k, ast.Constant) and isinstance(k.value, str)]


def _config_key_lines(text):
    lines = {}

    def walk(node, prefix):
        if isinstance(node, ast.Dict):
            for key, value in zip(node.keys, node.values):
                if isinstance(key, ast.Constant) and isinstance(key.value, str):
                    full = prefix + "." + key.value if prefix else key.value
                    lines[full] = key.lineno
                    walk(value, full)
    value = _assignment(ast.parse(text), "DEFAULT_CONFIG")
    if value is not None:
        walk(value, "")
    return lines


def _comment_block(lines, index):
    start = end = index
    while start > 0 and lines[start - 1].strip().startswith("#"):
        start -= 1
    while end + 1 < len(lines) and lines[end + 1].strip().startswith("#"):
        end += 1
    return "\n".join(lines[start:end + 1])


def _names(text, value):
    spellings = [value] + ([value.capitalize()] if value in ("true", "false") else [])
    return any(re.search(r"(?<![\w.-])" + re.escape(s) + r"(?![\w-])", text) for s in spellings)


def config_choice_entry(root, recipe):
    """One proof entry: the recipe plus the values read from the checkout at ``root``."""
    key, kind = recipe["key"], recipe["kind"]
    entry = {"key": key, "kind": kind}
    if kind == "constant":
        tree = ast.parse(_module(root, recipe["file"]))
        node = _assignment(tree, recipe["name"])
        if node is None:
            raise ContractError("config choices: {} has no {} for {}".format(recipe["file"], recipe["name"], key))
        select = recipe.get("select")
        if select == "keys":
            values = _dict_keys(node)
        elif select == "names":
            if not isinstance(node, ast.List):
                raise ContractError("config choices: {} is not a list".format(recipe["name"]))
            values = [v.value for item in node.elts if isinstance(item, ast.Dict)
                      for k, v in zip(item.keys, item.values)
                      if isinstance(k, ast.Constant) and k.value == "name" and isinstance(v, ast.Constant)]
        elif select:
            found = [v for k, v in zip(node.keys, node.values) if isinstance(k, ast.Constant) and k.value == select] if isinstance(node, ast.Dict) else []
            if not found:
                raise ContractError("config choices: {}[{}] is missing".format(recipe["name"], select))
            values = _string_items(found[0])
        else:
            values = _string_items(node)
        entry.update(file=recipe["file"], name=recipe["name"])
        if select:
            entry["select"] = select
    elif kind == "function":
        tree = ast.parse(_module(root, recipe["file"]))
        functions = [n for n in ast.walk(tree) if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef)) and n.name == recipe["function"]]
        if not functions:
            raise ContractError("config choices: {} has no function {}".format(recipe["file"], recipe["function"]))
        literals = {n.value for n in ast.walk(functions[0]) if isinstance(n, ast.Constant) and isinstance(n.value, str)}
        missing = [v for v in recipe["values"] if v not in literals]
        if missing:
            raise ContractError("config choices: {} does not name {} for {}".format(recipe["function"], missing, key))
        values = list(recipe["values"])
        entry.update(file=recipe["file"], function=recipe["function"])
    elif kind == "comment":
        path = recipe.get("file", CONFIG_DEFAULTS)
        text = _module(root, path)
        lines = text.splitlines()
        if "anchor" in recipe:
            index = next((i for i, line in enumerate(lines) if recipe["anchor"] in line), None)
            if index is None:
                raise ContractError("config choices: anchor {!r} for {} is missing".format(recipe["anchor"], key))
            entry["anchor"] = recipe["anchor"]
        else:
            line = _config_key_lines(text).get(key)
            if line is None:
                raise ContractError("config choices: {} is not in DEFAULT_CONFIG".format(key))
            index = line - 1
        block = _comment_block(lines, index)
        missing = [v for v in recipe["values"] if not _names(block, v)]
        if missing:
            raise ContractError("config choices: the comment for {} does not name {}".format(key, missing))
        values = list(recipe["values"])
        entry["file"] = path
    elif kind == "desktop":
        text = _module(root, DESKTOP_CHOICES)
        match = re.search(r"'" + re.escape(key) + r"'\s*:\s*\[(.*?)\]", text, re.S)
        if not match:
            raise ContractError("config choices: the desktop list for {} is missing".format(key))
        values = re.findall(r"'([^']*)'", match.group(1))
        entry["file"] = DESKTOP_CHOICES
    else:
        raise ContractError("config choices: unknown kind {} for {}".format(kind, key))
    if recipe.get("match"):
        entry["match"] = recipe["match"]
    entry["values"] = values
    return entry


def config_schema_selects(root):
    """Every ``_select`` in the schema overrides: its literal options, and whether more are added."""
    tree = ast.parse(_module(root, SCHEMA_SOURCE))
    node = _assignment(tree, "_SCHEMA_OVERRIDES")
    if not isinstance(node, ast.Dict):
        raise ContractError("config choices: _SCHEMA_OVERRIDES is missing")
    selects = []
    for key, value in zip(node.keys, node.values):
        if not (isinstance(key, ast.Constant) and isinstance(value, ast.Call)
                and isinstance(value.func, ast.Name) and value.func.id == "_select"):
            continue
        options = [a.value for a in value.args[1:] if isinstance(a, ast.Constant) and isinstance(a.value, str)]
        dynamic = any(isinstance(a, ast.Starred) for a in value.args[1:])
        selects.append({"key": key.value, "options": options, "dynamic": dynamic})
    return selects


def extract_config_choices(checkout, commit=None, tag=None, recipes=None):
    if commit:
        require_pinned_checkout(checkout, commit)
    return {
        "hermes": {"tag": tag, "commit": commit},
        "choices": [config_choice_entry(checkout, r) for r in (recipes or CONFIG_CHOICE_SOURCES)],
        "schema": config_schema_selects(checkout),
    }


def render_config_choices(data):
    return json.dumps(data, indent=1, ensure_ascii=False) + "\n"


def config_choices(checkout, check=False, out=None):
    tag, commit = read_pin()
    rendered = render_config_choices(extract_config_choices(checkout, commit, tag))
    target = Path(out or CONFIG_CHOICES_FILE)
    if check:
        if not target.exists() or target.read_text(encoding="utf-8") != rendered:
            print("config choices differ from the pinned Hermes source: " + str(target))
            return 1
        print("config choices are current")
        return 0
    target.write_text(rendered, encoding="utf-8")
    return 0


def config_choices_drift(checkout, commit):
    """Lines for the drift report: values that differ between the committed proof and a release."""
    old = json.loads(CONFIG_CHOICES_FILE.read_text(encoding="utf-8"))
    report = []
    try:
        new = extract_config_choices(checkout, commit)
    except ContractError as error:
        return ["Config choices could not be read: " + " ".join(str(error).splitlines())]
    before = {e["key"]: e["values"] for e in old["choices"]}
    for entry in new["choices"]:
        if before.get(entry["key"]) != entry["values"]:
            report.append("Config choices for {}: {} -> {}".format(entry["key"], before.get(entry["key"]), entry["values"]))
    schema = {e["key"]: e for e in old["schema"]}
    for entry in new["schema"]:
        if schema.get(entry["key"]) != entry:
            report.append("Schema select {} changed: {}".format(entry["key"], entry["options"]))
    return report or ["Config choices: no differences"]


def typify_available():
    try:
        return run(["cargo", "typify", "--version"]).decode().strip() == "cargo-typify " + TYPIFY_VERSION
    except ContractError:
        return False


def profile_methods(contract):
    def has_profile(schema, depth=0):
        if depth > 32 or not isinstance(schema, dict):
            return False
        properties = schema.get("properties")
        if isinstance(properties, dict) and "profile" in properties:
            return True
        reference = schema.get("$ref")
        if isinstance(reference, str):
            if not reference.startswith("#"):
                return False
            pointer = reference[1:]
            if pointer and not pointer.startswith("/"):
                return False
            target = contract
            for part in pointer.split("/")[1:]:
                part = part.replace("~1", "/").replace("~0", "~")
                if isinstance(target, dict):
                    target = target.get(part)
                elif isinstance(target, list) and re.fullmatch(r"0|[1-9][0-9]*", part) and int(part) < len(target):
                    target = target[int(part)]
                else:
                    return False
            return has_profile(target, depth + 1)
        return any(has_profile(part, depth + 1)
                   for keyword in ("allOf", "anyOf", "oneOf")
                   if isinstance(schema.get(keyword), list)
                   for part in schema[keyword])

    return sorted({entry["name"]
                   for catalogue in ("methods", "x-canonical-methods")
                   for entry in contract[catalogue]
                   if any(has_profile(param.get("schema", {})) for param in entry.get("params", []))})


def render_methods(entries):
    chunks = []
    for entry in sorted(entries, key=lambda e: e["name"]):
        name = entry["name"]
        chunks.append('pub mod %s {\npub const NAME: &str = %s;\npub type Params = crate::schemas::%s;\npub type Result = crate::schemas::%s;\npub struct Marker;\nimpl crate::method::Method for Marker {\nconst NAME: &\'static str = NAME;\ntype Params = Params;\ntype Result = Result;\n}\n}\n' %
                      (snake_case(name), json.dumps(name), rust_type_name(entry["params"][0]["schema"]), rust_type_name(entry["result"]["schema"])))
    return "".join(chunks)


def render_events(contract, server=False):
    entries = contract["x-server-requests" if server else "x-notifications"]
    variants, required = {}, {}
    for entry in entries:
        name, schema = entry["name"], entry["params"][0]["schema"]
        if "$ref" in schema:
            typ = schema_name(schema)
            variants[name] = "crate::schemas::" + typ
            required[name] = sorted(contract["components"]["schemas"][typ].get("required", []))
        else:
            variants[name], required[name] = None, []
    if not server:
        for name, typ in HAND_WRITTEN_EVENTS.items():
            variants[name] = "crate::canonical_events::" + typ
            required[name] = [] if name == "session.replay_gap" else ["prompt_id"]
    text = "#[derive(Clone, Debug, PartialEq, serde::Serialize)]\npub enum " + ("ServerRequest" if server else "Notification") + " {\n"
    for name, typ in sorted(variants.items()):
        text += pascal_case(name) + ("(" + typ + ")" if typ else "") + ",\n"
    text += "Unknown(serde_json::Value),\n}\npub const REQUIRED_FIELDS: &[(&str, &[&str])] = &[\n"
    for name, fields in sorted(required.items()):
        text += "(" + json.dumps(name) + ", &[" + ",".join(json.dumps(f) for f in fields) + "]),\n"
    text += "];\n"
    if server:
        text += render_methods(entries)
    return text


# Hermes gateway/session_local_recovery.py:313-314 at commit 4d04303818b61bf3aa09278af48356315515965b
# sends archived_truncated on a transcript row, but the published schema leaves it out.
ARCHIVED_TRUNCATED_EDITS = (
    (False, r"pub struct TranscriptMessage\s*\{",
     '#[serde(default, deserialize_with = "crate::overrides::deserialize_archived_truncated", '
     'skip_serializing_if = "::std::option::Option::is_none")]\n'
     "pub archived_truncated: ::std::option::Option<bool>,"),
    (True, r"pub struct TranscriptMessage\s*\{",
     "archived_truncated: ::std::result::Result<::std::option::Option<bool>, ::std::string::String>,"),
    (True, r"impl\s+::std::default::Default\s+for\s+TranscriptMessage\s*\{\s*fn\s+default\(\)\s*->\s*Self\s*\{\s*Self\s*\{",
     "archived_truncated: Ok(Default::default()),"),
    (True, r"impl\s+TranscriptMessage\s*\{",
     "pub fn archived_truncated<T>(mut self, value: T) -> Self\nwhere\n"
     "T: ::std::convert::TryInto<::std::option::Option<bool>>,\nT::Error: ::std::fmt::Display,\n{\n"
     'self.archived_truncated = value.try_into().map_err(|e| format!("error converting supplied value for archived_truncated: {e}"));\n'
     "self\n}"),
    (True, r"impl\s+::std::convert::TryFrom<TranscriptMessage>\s+for\s+super::TranscriptMessage\s*\{[^}]*?Ok\(Self\s*\{",
     "archived_truncated: value.archived_truncated?,"),
    (True, r"impl\s+::std::convert::From<super::TranscriptMessage>\s+for\s+TranscriptMessage\s*\{\s*fn\s+from\([^)]*\)\s*->\s*Self\s*\{\s*Self\s*\{",
     "archived_truncated: Ok(value.archived_truncated),"),
)


def postprocess_schemas_rs(schemas):
    """Apply the received transcript field and the catalogue's omission rule."""
    # model.options omits inactive flags so the profile stale check sends only
    # explicit_only. Keep this local to its params; other defaults stay put.
    if "pub struct ModelOptionsParams" in schemas:
        for field in ("include_unconfigured", "refresh"):
            pattern = (r'(pub struct ModelOptionsParams\s*\{[^}]*?)#\[serde\(default\)\]'
                       r'(\s*pub ' + field + r': bool,)')
            schemas, count = re.subn(
                pattern,
                r'\1#[serde(default, skip_serializing_if = "::std::ops::Not::not")]\2',
                schemas,
                count=1,
            )
            if count != 1:
                raise ContractError("expected one ModelOptionsParams.{} boolean default; update the override".format(field))
    split = schemas.find("pub mod builder {")
    if "pub struct TranscriptMessage" not in schemas or split < 0:
        return schemas
    parts = [schemas[:split], schemas[split:]]
    declared = re.search(r"pub struct TranscriptMessage\s*\{[^}]*\}", parts[0])
    if declared and "archived_truncated" in declared.group(0):
        raise ContractError("TranscriptMessage already declares archived_truncated; remove the override")
    for in_builder, pattern, insert in ARCHIVED_TRUNCATED_EDITS:
        text = parts[in_builder]
        matches = list(re.finditer(pattern, text))
        if len(matches) != 1:
            raise ContractError("expected one TranscriptMessage site for archived_truncated, found {}: {}".format(len(matches), pattern))
        end = matches[0].end()
        parts[in_builder] = text[:end] + "\n" + insert + text[end:]
    return "".join(parts)


def generate(contract_path=CONTRACT, out=OUT, hermes_checkout=None, pin=None, owner_file=OWNER_FILE):
    raw = Path(contract_path).read_bytes()
    contract = json.loads(raw.decode("utf-8-sig"))
    validate(contract)
    tag, commit = pin or read_pin()
    owners = extract_owner_methods(hermes_checkout, commit) if hermes_checkout else Path(owner_file).read_text().splitlines()
    if not typify_available():
        raise ContractError("cargo-typify {} is not installed; install it with the command in tools/contract-gen/README.md".format(TYPIFY_VERSION))
    header = "// Hermes {} commit {} contract sha256 {}\n".format(tag, commit, hashlib.sha256(raw).hexdigest())
    with tempfile.TemporaryDirectory(prefix="contract-gen-") as temp:
        temp = Path(temp)
        for name, rooted in (("definitions.json", False), ("all.json", True)):
            (temp / name).write_text(json.dumps(lift(contract, rooted), sort_keys=True, indent=2) + "\n")
        env = dict(os.environ)
        if "RUSTFMT" not in env and shutil.which("rustfmt"):
            env["RUSTFMT"] = shutil.which("rustfmt")
        run(["cargo", "typify", temp / "all.json", "-d", "PartialEq", "-o", temp / "schemas.rs"], env=env)
        schemas = (temp / "schemas.rs").read_text()
        schemas = postprocess_schemas_rs(schemas)
        if len(schemas.splitlines()) > 300000:
            raise ContractError("generated schemas exceed 300000 lines; evaluate --no-builder")
        canonical = {e["name"] for e in contract["x-canonical-methods"]}
        native = "pub trait NativeMethod: crate::method::Method {}\n"
        for transport, entries in (("canonical", contract["x-canonical-methods"]), ("legacy", contract["methods"])):
            for entry in sorted(entries, key=lambda e: e["name"]):
                name = entry["name"]
                if transport == "canonical" or (name not in canonical and native_method(name, owners)):
                    native += "impl NativeMethod for crate::methods::{}::{}::Marker {{}}\n".format(transport, snake_case(name))
        files = {
            "mod.rs": "pub mod schemas;\npub mod methods;\npub mod native;\npub mod events;\npub mod server_requests;\npub mod profile_methods;\n",
            "profile_methods.rs": "pub const PROFILE_METHODS: &[&str] = &[\n" + "".join("    {},\n".format(json.dumps(name)) for name in profile_methods(contract)) + "];\n",
            "schemas.rs": schemas,
            "methods.rs": "pub mod legacy {\n" + render_methods(contract["methods"]) + "}\npub mod canonical {\n" + render_methods(contract["x-canonical-methods"]) + "}\n",
            "events.rs": render_events(contract), "server_requests.rs": render_events(contract, True), "native.rs": native,
        }
        for name, contents in files.items():
            allowances = ""
            if name == "schemas.rs":
                allowances = "#![allow(dead_code, clippy::derivable_impls, clippy::should_implement_trait, clippy::unit_arg)]\n"
            elif name == "events.rs":
                allowances = "#![allow(clippy::large_enum_variant)]\n"
            (temp / name).write_text(header + allowances + contents)
        run(["rustfmt", "--edition", "2024"] + [temp / n for n in sorted(files)])
        Path(out).mkdir(parents=True, exist_ok=True)
        for name in sorted(files):
            (Path(out) / name).write_bytes((temp / name).read_bytes())
    if hermes_checkout:
        Path(owner_file).write_text("".join(n + "\n" for n in owners))


def check(require_typify=False):
    if not typify_available():
        print("cargo-typify {} is not installed; install it with the command in tools/contract-gen/README.md".format(TYPIFY_VERSION))
        return 1 if require_typify or os.environ.get("CONTRACT_GEN_REQUIRE_TYPIFY") == "1" else 0
    with tempfile.TemporaryDirectory(prefix="contract-check-") as temp:
        generate(out=Path(temp))
        names = ({p.name for p in Path(temp).glob("*.rs")} | {p.name for p in OUT.glob("*.rs")}) - {"compat.rs"}
        differences = sorted(n for n in names
                             if not (OUT / n).exists() or not (Path(temp) / n).exists() or (OUT / n).read_bytes() != (Path(temp) / n).read_bytes())
        for name in differences:
            print("generated file differs: " + name)
        if not differences:
            print("every generated file is current")
        return int(bool(differences))


COMPAT_KEYS = ("talaria", "floor", "pinned_tag", "pinned_commit", "newest_tested")
COMPAT_START = "<!-- hermes-compat:start -->"
COMPAT_END = "<!-- hermes-compat:end -->"


def read_compat():
    with (CRATE / "compat.toml").open("rb") as source:
        rows = tomllib.load(source).get("release")
    if not isinstance(rows, list) or not rows:
        raise ContractError("compat.toml is missing a non-empty [[release]] entry")
    for index, row in enumerate(rows, 1):
        if not isinstance(row, dict):
            raise ContractError("compat.toml release entry {} must be a table".format(index))
        for key in COMPAT_KEYS:
            if key not in row:
                raise ContractError("compat.toml release entry {} is missing key {}".format(index, key))
            if not isinstance(row[key], str):
                raise ContractError("compat.toml release entry {} key {} must be a string".format(index, key))
    return rows


def compat_line(row):
    return "hermes: pinned {} ({}), supported from {}, tested to {}".format(
        row["pinned_tag"], row["pinned_commit"][:8].lower(), row["floor"], row["newest_tested"])


def compat_readme(text, line):
    body = line + "\n[docs/compatibility.md](docs/compatibility.md)\n"
    if COMPAT_START not in text and COMPAT_END not in text:
        install = re.search(r"(?m)^## Install$", text)
        if install is None:
            raise ContractError("README.md is missing the ## Install heading")
        block = COMPAT_START + "\n" + body + COMPAT_END + "\n\n"
        return text[:install.start()] + block + text[install.start():]
    if text.count(COMPAT_START) != 1 or text.count(COMPAT_END) != 1:
        raise ContractError("README.md must have exactly one compatibility marker pair")
    start = text.index(COMPAT_START) + len(COMPAT_START)
    end = text.index(COMPAT_END)
    if start >= end or not text[end + len(COMPAT_END):].startswith("\n\n## Install"):
        raise ContractError("README.md compatibility block must precede ## Install")
    return text[:start] + "\n" + body + text[end:]


def compat(write=False):
    rows = read_compat()
    current = rows[-1]
    table = ["# Compatibility", "", "| Talaria | Floor | Pinned Hermes | Newest tested |",
             "| --- | --- | --- | --- |"]
    table.extend("| {} | {} | {} {} | {} |".format(
        row["talaria"], row["floor"], row["pinned_tag"], row["pinned_commit"][:8].lower(), row["newest_tested"])
        for row in rows)
    rust = ["// Generated from crates/hermes-contract/compat.toml. Do not edit.", "", "pub struct CompatRow {"]
    rust.extend("    pub {}: &'static str,".format(key) for key in COMPAT_KEYS)
    rust.extend(["}", "", "pub const CURRENT: CompatRow = CompatRow {"])
    rust.extend("    {}: {},".format(key, json.dumps(current[key])) for key in COMPAT_KEYS)
    rust.append("};")
    files = {
        ROOT / "docs/compatibility.md": "\n".join(table) + "\n",
        ROOT / "README.md": compat_readme((ROOT / "README.md").read_text(), compat_line(current)),
        OUT / "compat.rs": "\n".join(rust) + "\n",
    }
    differences = []
    for path, text in files.items():
        expected = text.encode("utf-8")
        if write:
            path.write_bytes(expected)
        elif not path.exists() or path.read_bytes() != expected:
            differences.append(path.relative_to(ROOT).as_posix())
    for path in differences:
        print("compatibility file differs: " + path)
    return int(bool(differences))


def gh_raw(path, ref):
    return run(["gh", "api", "-H", "Accept: application/vnd.github.raw+json", "repos/{}/contents/{}?ref={}".format(REPO, path, ref)])


def verify_vendor(contract_path=CONTRACT):
    _tag, commit = read_pin()
    failed = False
    for local, remote in ((Path(contract_path), CONTRACT_PATH), (CRATE / "vendor/LICENSE.hermes", "LICENSE")):
        actual, expected = local.read_bytes(), gh_raw(remote, commit)
        if actual != expected:
            print("{} mismatch: local sha256 {} upstream sha256 {}".format(local.name, hashlib.sha256(actual).hexdigest(), hashlib.sha256(expected).hexdigest()))
            failed = True
    if not failed:
        print("vendored contract and LICENSE match the pinned commit")
    return int(failed)


def resolve_tag(tag):
    if tag == "latest":
        # GitHub's stable release selection also handles the historical date-based
        # tag series, whose numeric major would otherwise outrank current releases.
        release = json.loads(run(["gh", "api", "repos/{}/releases/latest".format(REPO)]))
        tag = release["tag_name"]
        if release.get("draft") or release.get("prerelease") or version(tag) < (0, 22, 3):
            raise ContractError("no supported stable Hermes release was found")
    try:
        obj = json.loads(run(["gh", "api", "repos/{}/git/ref/tags/{}".format(REPO, tag)]))["object"]
        while obj["type"] == "tag":
            obj = json.loads(run(["gh", "api", "repos/{}/git/tags/{}".format(REPO, obj["sha"])]))["object"]
        if obj["type"] != "commit":
            raise ContractError("tag does not point to a commit")
        return tag, obj["sha"]
    except ContractError as error:
        if str(error) == "gh is not installed":
            raise
        raise ContractError("tag {} was not found in {}".format(tag, REPO)) from None


class OutputSnapshot:
    """Restore output bytes and path kinds, including an entire generated tree."""

    def __init__(self, paths):
        self.roots = tuple(paths)
        self.saved = {}
        for path in self.roots:
            self.remember(path, recursive=True)
            for parent in path.parents:
                if parent == ROOT:
                    break
                self.remember(parent)

    def remember(self, path, recursive=False):
        if path in self.saved:
            return
        if path.is_symlink():
            raise ContractError("cannot snapshot a symbolic-link output: {}".format(path))
        if path.is_dir():
            self.saved[path] = ("directory", None)
            if recursive:
                for child in path.iterdir():
                    self.remember(child, recursive=True)
        elif path.exists():
            self.saved[path] = ("file", path.read_bytes())
        else:
            self.saved[path] = ("missing", None)

    @staticmethod
    def remove(path):
        if path.is_symlink() or path.is_file():
            path.unlink()
        elif path.is_dir():
            shutil.rmtree(path)

    def restore(self):
        current = set()
        for root in self.roots:
            current.add(root)
            if root.is_dir() and not root.is_symlink():
                current.update(root.rglob("*"))
        for path in sorted(current - self.saved.keys(), key=lambda p: len(p.parts), reverse=True):
            self.remove(path)
        for path in sorted(self.saved, key=lambda p: len(p.parts), reverse=True):
            if self.saved[path][0] == "missing":
                self.remove(path)
        for path in sorted(self.saved, key=lambda p: len(p.parts)):
            kind, data = self.saved[path]
            if kind == "directory":
                if path.is_symlink() or not path.is_dir():
                    self.remove(path)
                    path.mkdir(parents=True, exist_ok=True)
            elif kind == "file":
                if path.is_symlink() or not path.is_file():
                    self.remove(path)
                    path.parent.mkdir(parents=True, exist_ok=True)
                if not path.exists() or path.read_bytes() != data:
                    path.write_bytes(data)


def update_compat_cells(changes):
    path = CRATE / "compat.toml"
    text = path.read_bytes().decode("utf-8")
    headers = list(re.finditer(r"(?m)^[ \t]*\[\[[ \t]*release[ \t]*\]\][^\r\n]*", text))
    if not headers:
        raise ContractError("compat.toml is missing a non-empty [[release]] entry")
    start = headers[-1].end()
    following = re.search(r"(?m)^[ \t]*\[", text[start:])
    end = start + following.start() if following else len(text)
    block = text[start:end]
    for key, value in changes.items():
        names = r'''(?:{0}|"{0}"|'{0}')'''.format(re.escape(key))
        pattern = r'''(?m)^([ \t]*{}[ \t]*=[ \t]*)("(?:\\.|[^"\\\r\n])*"|'[^'\r\n]*')'''.format(names)
        def replace(match):
            literal = json.dumps(value) if match[2].startswith('"') else "'" + value + "'"
            return match[1] + literal
        block, count = re.subn(pattern, replace, block)
        if count != 1:
            raise ContractError("compat.toml last release entry needs one {} cell".format(key))
    path.write_bytes((text[:start] + block + text[end:]).encode("utf-8"))


def bump_error(error):
    print(" ".join(str(error).splitlines()))
    detail = getattr(error, "stderr", "") or str(error)
    for needle, phrase in (
        ("inline string enum title collision", "a title collision in the generator"),
        ("crates/hermes-contract/src/events.rs", "a removed event still named in crates/hermes-contract/src/events.rs"),
        ("/tests/", "a recorded frame or a mock that rejects a new field"),
        ("error[E0063]", "a new field that a hand-written struct literal must name"),
    ):
        if needle in detail:
            print(phrase)
            break
    return 1


def older_recordings(tag):
    try:
        paths = run(["git", "ls-files", "-z", "--", "crates/", "fixtures/"], cwd=ROOT)
    except ContractError:
        return
    for name in paths.decode(errors="replace").split("\0"):
        if not name.endswith(".json") or not name.startswith(("crates/", "fixtures/")):
            continue
        try:
            value = json.loads((ROOT / name).read_bytes())
            release = value.get("recorded_against") if isinstance(value, dict) else None
            if isinstance(release, str) and compare_versions(release, tag) < 0:
                print(name)
        except (ContractError, OSError, ValueError):
            continue


def bump(tag):
    current = read_compat()[-1]
    tag, commit = resolve_tag(tag)
    commit = commit.lower()
    validate_pinned(Pin(tag, commit))
    raw = gh_raw(CONTRACT_PATH, commit)
    licence = gh_raw("LICENSE", commit)
    try:
        validate(json.loads(raw.decode("utf-8-sig")))
    except (ContractError, ValueError, KeyError) as error:
        return bump_error(error)
    changes = {"pinned_tag": tag, "pinned_commit": commit}
    if compare_versions(tag, current["newest_tested"]) > 0:
        changes["newest_tested"] = tag
    checksum = CONTRACT.with_name(CONTRACT.name + ".sha256")
    licence_path = CONTRACT.parent / "LICENSE.hermes"
    snapshot = OutputSnapshot([CRATE / "PINNED", CONTRACT, checksum, licence_path,
                               CRATE / "compat.toml", ROOT / "README.md", ROOT / "docs/compatibility.md", OUT])
    classify = False
    try:
        (CRATE / "PINNED").write_text("tag={}\ncommit={}\n".format(tag, commit))
        CONTRACT.write_bytes(raw)
        checksum.write_text(hashlib.sha256(raw).hexdigest() + "  " + CONTRACT.name + "\n")
        licence_path.write_bytes(licence)
        update_compat_cells(changes)
        classify = True
        generate()
        classify = False
        if main(["compat", "--write"]):
            snapshot.restore()
            return 1
        classify = True
        run(["cargo", "check", "--workspace", "--all-targets"], cwd=ROOT)
    except (ContractError, OSError, ValueError, KeyError) as error:
        snapshot.restore()
        if classify:
            return bump_error(error)
        raise
    older_recordings(tag)
    return 0


def tested(tag):
    version(tag)
    current = read_compat()[-1]
    if compare_versions(tag, current["newest_tested"]) <= 0:
        raise ContractError("newest tested is already {}".format(current["newest_tested"]))
    snapshot = OutputSnapshot([CRATE / "compat.toml", ROOT / "README.md", ROOT / "docs/compatibility.md", OUT / "compat.rs"])
    try:
        update_compat_cells({"newest_tested": tag})
        if main(["compat", "--write"]):
            snapshot.restore()
            return 1
    except (ContractError, OSError, ValueError, KeyError):
        snapshot.restore()
        raise
    return 0


def catalogue_diff(old, new, owners_before=(), owners_after=()):
    result = {}
    for cat in CATALOGUES:
        def indexed(contract):
            defs = lift(contract)["$defs"]
            rows = {}
            for entry in contract[cat]:
                roots = {k: entry[k] for k in ("params", "result") if k in entry}
                refs, pending = set(), list(references(roots))
                while pending:
                    name = pending.pop()
                    if name in refs:
                        continue
                    refs.add(name)
                    pending.extend(references(defs[name]) - refs)
                rows[entry["name"]] = (roots, {name: defs[name] for name in sorted(refs)})
            return rows
        before, after = indexed(old), indexed(new)
        label = dict(zip(CATALOGUES, ("legacy", "canonical", "notifications", "server_requests")))[cat]
        result[label] = {"added": sorted(after.keys() - before.keys()), "removed": sorted(before.keys() - after.keys()),
                       "changed": sorted(n for n in before.keys() & after.keys() if before[n] != after[n])}
    before, after = set(old["components"]["schemas"]), set(new["components"]["schemas"])
    result["schemas"] = {"added": sorted(after - before), "removed": sorted(before - after)}
    result["owner_methods"] = {"added": sorted(set(owners_after) - set(owners_before)), "removed": sorted(set(owners_before) - set(owners_after))}
    return result


def copy_workspace(destination):
    copy_workspace_for_drift(ROOT, destination)


def copy_workspace_for_drift(source, destination):
    source = Path(source)
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    manifest = (source / "Cargo.toml").read_text()
    manifest = re.sub(r"members\s*=\s*\[[\s\S]*?\]", 'members = ["crates/hermes-contract"]', manifest, count=1)
    (destination / "Cargo.toml").write_text(manifest)
    for name in ("Cargo.lock", "rust-toolchain.toml"):
        shutil.copy2(source / name, destination / name)
    shutil.copytree(source / "crates/hermes-contract", destination / "crates/hermes-contract")
    frames = source / "fixtures/frames"
    if frames.exists():
        shutil.copytree(frames, destination / "fixtures/frames")


def drift(tag, summary=None, hermes_checkout=None):
    report = ["# Hermes contract drift"]
    status = 1
    try:
        tag, commit = resolve_tag(tag)
        report.append("Release: {} ({})".format(tag, commit))
        raw = gh_raw(CONTRACT_PATH, commit)
        old, new = json.loads(CONTRACT.read_bytes()), json.loads(raw)
        report.append("```json\n" + json.dumps(catalogue_diff(old, new), indent=2, sort_keys=True) + "\n```")
        owners = OWNER_FILE.read_text().splitlines()
        if hermes_checkout:
            fresh = extract_owner_methods(hermes_checkout, commit)
            report.append("Owner methods added: {}; removed: {}".format(sorted(set(fresh) - set(owners)), sorted(set(owners) - set(fresh))))
            owners = fresh
            report.append("\n".join(config_choices_drift(hermes_checkout, commit)))
        else:
            note = "owner list was not refreshed; using the committed list"
            print(note)
            report.append(note)
        with tempfile.TemporaryDirectory(prefix="contract-drift-") as temp:
            temp = Path(temp)
            copy_workspace(temp)
            vendor = temp / "crates/hermes-contract/vendor/gateway-contract.openrpc.json"
            vendor.write_bytes(raw)
            owner = temp / "owners.txt"
            owner.write_text("\n".join(owners) + "\n")
            generate(vendor, temp / "crates/hermes-contract/src/generated", pin=(tag, commit), owner_file=owner)
            def step(label, args):
                result = subprocess.run(args, cwd=temp, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                output = result.stdout.decode(errors="replace")
                print("{} argv={} returncode={}".format(label, json.dumps(args), result.returncode), flush=True)
                if result.returncode:
                    print(output, end="" if output.endswith("\n") else "\n", flush=True)
                failures = [line for line in output.splitlines() if "FAILED" in line or "test result:" in line]
                report.append("{} ({}): {}\n{}".format(label, " ".join(args), "passed" if not result.returncode else "failed", "\n".join(failures)))
                return result.returncode == 0
            built = step("build", ["cargo", "build", "-p", "hermes-contract"])
            tests = sorted(p.stem for p in (temp / "crates/hermes-contract/tests").glob("*.rs") if p.stem not in ("frames", "responses"))
            if not tests:
                tests = ["canonical_events", "hygiene", "methods", "native", "overrides", "pinned", "unions"]
            args = ["cargo", "test", "-p", "hermes-contract", "--lib"]
            for name in tests:
                args.extend(["--test", name])
            bindings = step("binding tests", args)
            docs = step("doc tests", ["cargo", "test", "-p", "hermes-contract", "--doc"])
            step("recording tests", ["cargo", "test", "-p", "hermes-contract", "--test", "frames", "--test", "responses"])
            status = int(not (built and bindings and docs))
    except (ContractError, OSError, ValueError, KeyError) as error:
        message = " ".join(str(error).splitlines())
        report.append("failed: " + message)
        print(message)
    finally:
        rendered = "\n\n".join(report) + "\n"
        print(rendered, end="", flush=True)
        if summary:
            Path(summary).write_text(rendered)
    return status


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    gen = sub.add_parser("generate")
    gen.add_argument("--contract", type=Path, default=CONTRACT)
    gen.add_argument("--out", type=Path, default=OUT)
    gen.add_argument("--hermes-checkout", type=Path)
    gen.add_argument("--check", action="store_true")
    choices = sub.add_parser("config-choices", help="write or check config-choices.json from a pinned Hermes checkout")
    choices.add_argument("--hermes-checkout", type=Path, required=True)
    choices.add_argument("--check", action="store_true")
    chk = sub.add_parser("check")
    chk.add_argument("--require-typify", action="store_true")
    compat_parser = sub.add_parser("compat")
    compat_mode = compat_parser.add_mutually_exclusive_group(required=True)
    compat_mode.add_argument("--write", action="store_true")
    compat_mode.add_argument("--check", action="store_true")
    verify = sub.add_parser("verify-vendor")
    verify.add_argument("--contract", type=Path, default=CONTRACT)
    pin = sub.add_parser("pin", help="validate and print the current pin; does not bump it")
    pin.add_argument("--file", type=Path)
    drift_parser = sub.add_parser("drift")
    drift_parser.add_argument("--tag", required=True)
    drift_parser.add_argument("--summary", type=Path)
    drift_parser.add_argument("--hermes-checkout", type=Path)
    for command in ("bump", "tested"):
        update = sub.add_parser(command)
        update.add_argument("--tag", required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "generate":
            if args.check:
                return check(True)
            generate(args.contract, args.out, args.hermes_checkout)
        elif args.command == "config-choices":
            return config_choices(args.hermes_checkout, args.check)
        elif args.command == "check":
            return check(args.require_typify)
        elif args.command == "compat":
            return compat(args.write)
        elif args.command == "verify-vendor":
            return verify_vendor(args.contract)
        elif args.command == "pin":
            tag, commit = read_pin(args.file)
            print("tag={}\ncommit={}".format(tag, commit))
        elif args.command == "drift":
            return drift(args.tag, args.summary, args.hermes_checkout)
        elif args.command == "bump":
            return bump(args.tag)
        elif args.command == "tested":
            return tested(args.tag)
        return 0
    except (ContractError, OSError, ValueError, KeyError) as error:
        print(" ".join(str(error).splitlines()))
        return 1


if __name__ == "__main__":
    sys.exit(main())
