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

TYPIFY_VERSION = "0.8.0"
ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / "crates/hermes-contract"
CONTRACT = CRATE / "vendor/gateway-contract.openrpc.json"
OUT = CRATE / "src/generated"
OWNER_FILE = ROOT / "tools/contract-gen/native-owner-methods.txt"
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
        detail = result.stderr.decode(errors="replace").strip().splitlines()
        raise ContractError("{} failed: {}".format(args[0], " ".join(detail[-3:])))
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


def typify_available():
    try:
        return run(["cargo", "typify", "--version"]).decode().strip() == "cargo-typify " + TYPIFY_VERSION
    except ContractError:
        return False


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
            "mod.rs": "pub mod schemas;\npub mod methods;\npub mod native;\npub mod events;\npub mod server_requests;\n",
            "schemas.rs": schemas,
            "methods.rs": "pub mod legacy {\n" + render_methods(contract["methods"]) + "}\npub mod canonical {\n" + render_methods(contract["x-canonical-methods"]) + "}\n",
            "events.rs": render_events(contract), "server_requests.rs": render_events(contract, True), "native.rs": native,
        }
        for name, contents in files.items():
            allowances = ""
            if name == "schemas.rs":
                allowances = "#![allow(clippy::derivable_impls, clippy::should_implement_trait, clippy::unit_arg)]\n"
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
        differences = sorted(n for n in {p.name for p in Path(temp).glob("*.rs")} | {p.name for p in OUT.glob("*.rs")}
                             if not (OUT / n).exists() or not (Path(temp) / n).exists() or (OUT / n).read_bytes() != (Path(temp) / n).read_bytes())
        for name in differences:
            print("generated file differs: " + name)
        if not differences:
            print("every generated file is current")
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
    except ContractError:
        raise ContractError("tag {} was not found in {}".format(tag, REPO)) from None


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
        if summary:
            Path(summary).write_text("\n\n".join(report) + "\n")
    return status


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    gen = sub.add_parser("generate")
    gen.add_argument("--contract", type=Path, default=CONTRACT)
    gen.add_argument("--out", type=Path, default=OUT)
    gen.add_argument("--hermes-checkout", type=Path)
    gen.add_argument("--check", action="store_true")
    chk = sub.add_parser("check")
    chk.add_argument("--require-typify", action="store_true")
    verify = sub.add_parser("verify-vendor")
    verify.add_argument("--contract", type=Path, default=CONTRACT)
    pin = sub.add_parser("pin", help="validate and print the current pin; does not bump it")
    pin.add_argument("--file", type=Path)
    drift_parser = sub.add_parser("drift")
    drift_parser.add_argument("--tag", required=True)
    drift_parser.add_argument("--summary", type=Path)
    drift_parser.add_argument("--hermes-checkout", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "generate":
            if args.check:
                return check(True)
            generate(args.contract, args.out, args.hermes_checkout)
        elif args.command == "check":
            return check(args.require_typify)
        elif args.command == "verify-vendor":
            return verify_vendor(args.contract)
        elif args.command == "pin":
            tag, commit = read_pin(args.file)
            print("tag={}\ncommit={}".format(tag, commit))
        elif args.command == "drift":
            return drift(args.tag, args.summary, args.hermes_checkout)
        return 0
    except (ContractError, OSError, ValueError, KeyError) as error:
        print(" ".join(str(error).splitlines()))
        return 1


if __name__ == "__main__":
    sys.exit(main())
