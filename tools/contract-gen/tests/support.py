"""Shared helpers for the generator tests. Not collected by unittest."""

from __future__ import annotations

import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Tuple

REPO = Path(__file__).resolve().parents[3]
CONTRACT_PATH = (
    REPO / "crates" / "hermes-contract" / "vendor" / "gateway-contract.openrpc.json"
)
VENDOR = REPO / "crates" / "hermes-contract" / "vendor"
PINNED_PATH = REPO / "crates" / "hermes-contract" / "PINNED"
SCRIPT_PATH = REPO / "tools" / "contract-gen" / "contract_gen.py"

GENERATED_FILES = (
    "mod.rs",
    "schemas.rs",
    "methods.rs",
    "events.rs",
    "server_requests.rs",
    "native.rs",
)

REF_PREFIX = "#/components/schemas/"
CONSTRAINTS = ("pattern", "minLength", "maxLength")


def load_contract(path: Optional[Path] = None) -> dict:
    source = CONTRACT_PATH if path is None else path
    return json.loads(source.read_text(encoding="utf-8"))


def ref_name(node: object) -> Optional[str]:
    if not isinstance(node, dict):
        return None
    ref = node.get("$ref")
    if isinstance(ref, str) and ref.startswith(REF_PREFIX):
        return ref[len(REF_PREFIX) :]
    return None


def count_additional(node: object) -> Dict[str, int]:
    counts = {"false": 0, "true": 0, "schema": 0}

    def walk(value: object) -> None:
        if isinstance(value, dict):
            if "additionalProperties" in value:
                marker = value["additionalProperties"]
                if marker is False:
                    counts["false"] += 1
                elif marker is True:
                    counts["true"] += 1
                elif isinstance(marker, dict):
                    counts["schema"] += 1
            for child in value.values():
                walk(child)
        elif isinstance(value, list):
            for child in value:
                walk(child)

    walk(node)
    return counts


def discriminator_sites(contract: dict) -> List[str]:
    sites = []
    schemas = contract["components"]["schemas"]

    def walk(node: object, schema_name: str, prop: Optional[str]) -> None:
        if isinstance(node, dict):
            if "discriminator" in node and prop:
                sites.append(schema_name + "." + prop)
            properties = node.get("properties")
            if isinstance(properties, dict):
                for name, child in properties.items():
                    walk(child, schema_name, name)
            for key, child in node.items():
                if key == "properties":
                    continue
                walk(child, schema_name, prop)
        elif isinstance(node, list):
            for child in node:
                walk(child, schema_name, prop)

    for name, schema in schemas.items():
        walk(schema, name, None)
    return sorted(set(sites))


def _inline_constrained(node: object) -> bool:
    if isinstance(node, dict):
        if "$ref" in node:
            return False
        if any(key in node for key in CONSTRAINTS):
            return True
        return any(_inline_constrained(child) for child in node.values())
    if isinstance(node, list):
        return any(_inline_constrained(child) for child in node)
    return False


def received_constrained_types(contract: dict) -> set:
    """Named schemas that constrain a string reachable from received data."""
    schemas = contract["components"]["schemas"]
    roots = []
    for catalogue in ("methods", "x-canonical-methods"):
        for method in contract.get(catalogue, []):
            roots.append(method.get("result", {}).get("schema", {}))
    for catalogue in ("x-notifications", "x-server-requests"):
        for entry in contract.get(catalogue, []):
            params = entry.get("params") or []
            if params:
                roots.append(params[0].get("schema", {}))

    found = set()
    seen = set()

    def walk_refs(node: object) -> None:
        if isinstance(node, dict):
            name = ref_name(node)
            if name:
                visit(name)
            for child in node.values():
                walk_refs(child)
        elif isinstance(node, list):
            for child in node:
                walk_refs(child)

    def visit(name: str) -> None:
        if name in seen or name not in schemas:
            return
        seen.add(name)
        schema = schemas[name]
        if _inline_constrained(schema):
            found.add(name)
        walk_refs(schema)

    for root in roots:
        name = ref_name(root)
        if name:
            visit(name)
        else:
            walk_refs(root)
    return found


def method_ref_pair(method: dict) -> Tuple[str, str]:
    params = method["params"][0]["schema"]["$ref"]
    result = method["result"]["schema"]["$ref"]
    return params, result


def shared_method_names(contract: dict) -> List[str]:
    legacy = {method["name"] for method in contract["methods"]}
    canonical = {method["name"] for method in contract["x-canonical-methods"]}
    return sorted(legacy & canonical)


def run_script(script: Path, args: Iterable[str], env: Optional[dict] = None):
    merged = os.environ.copy()
    if env:
        merged.update(env)
    completed = subprocess.run(
        [sys.executable, "-I", str(script), *list(args)],
        cwd=str(script.resolve().parents[2]),
        env=merged,
        capture_output=True,
        text=True,
    )
    return completed.returncode, completed.stdout, completed.stderr


def copy_generator(root: Path) -> Path:
    dest = root / "tools" / "contract-gen" / "contract_gen.py"
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(SCRIPT_PATH, dest)
    return dest


def write_executable(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IEXEC)


def fake_gh_script() -> str:
    """Offline gh. File bytes and the pinned commit come from the environment.

    Responses:
    - argv containing the missing tag: exit 1
    - releases/latest: JSON tag_name v0.22.4
    - commits/ or git/ref/tags: JSON sha from GH_COMMIT
    - argv containing LICENSE: bytes from GH_LICENSE
    - argv containing the contract file name: bytes from GH_CONTRACT
    """
    return """#!/usr/bin/env python3
import os
import sys

argv = sys.argv[1:]
joined = " ".join(argv)
log_path = os.environ.get("GH_LOG")
if log_path:
    with open(log_path, "a", encoding="utf-8") as handle:
        handle.write(joined + "\\n")
missing = os.environ.get("GH_MISSING_TAG", "")
if missing and missing in joined:
    sys.stderr.write("not found\\n")
    sys.exit(1)
if "releases/latest" in joined:
    sys.stdout.write('{"tag_name":"v0.22.4"}\\n')
    sys.exit(0)
if "commits/" in joined or "git/ref/tags" in joined or "git/refs/tags" in joined:
    sha = os.environ["GH_COMMIT"]
    sys.stdout.write(
        '{"sha":"%s","object":{"type":"commit","sha":"%s"}}\\n' % (sha, sha)
    )
    sys.exit(0)
if "LICENSE" in joined:
    sys.stdout.buffer.write(open(os.environ["GH_LICENSE"], "rb").read())
    sys.exit(0)
if "gateway-contract.openrpc.json" in joined:
    sys.stdout.buffer.write(open(os.environ["GH_CONTRACT"], "rb").read())
    sys.exit(0)
sys.stderr.write("fake gh: unhandled\\n")
sys.exit(1)
"""


def fake_cargo_script() -> str:
    """Offline cargo. typify writes the -o file. test exits 1 only for FAKE_CARGO_FAIL."""
    return """#!/usr/bin/env python3
import os
import sys

argv = sys.argv[1:]
log_path = os.environ.get("CARGO_LOG")
if log_path:
    with open(log_path, "a", encoding="utf-8") as handle:
        handle.write(" ".join(argv) + "\\n")
if "--version" in argv:
    sys.stdout.write("cargo-typify 0.8.0\\n")
    sys.exit(0)
if "typify" in argv:
    if "-o" in argv:
        index = argv.index("-o")
        dest = argv[index + 1]
        with open(dest, "w", encoding="utf-8") as handle:
            handle.write("// typify\\n")
    sys.exit(0)
if "test" in argv:
    fail = os.environ.get("FAKE_CARGO_FAIL", "")
    if fail and fail in argv:
        sys.stderr.write("failed %s\\n" % fail)
        sys.exit(1)
    sys.exit(0)
sys.exit(0)
"""


def install_fakes(bin_dir: Path, include_cargo: bool) -> None:
    write_executable(bin_dir / "gh", fake_gh_script())
    if include_cargo:
        text = fake_cargo_script()
        write_executable(bin_dir / "cargo", text)
        write_executable(bin_dir / "cargo-typify", text)


def prepend_path(bin_dir: Path) -> str:
    return str(bin_dir) + os.pathsep + os.environ.get("PATH", "")


def temporary_directory():
    return tempfile.TemporaryDirectory()
