#!/usr/bin/env python3
"""Create and validate one evidence receipt per v0.5.0 checklist item and tester."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import shutil
import struct
import subprocess
import sys
import urllib.parse
import zlib
from dataclasses import dataclass, field
from enum import StrEnum
from pathlib import Path
from typing import Any

from scripts.acceptance.v050_common import (
    FALLBACK_MODEL_ROUTE,
    FALLBACK_REASON_CODES,
    ORDERED_VERDICTS,
    PRIMARY_MODEL_ROUTE,
    RELEASE_VERSION,
    TERMINAL_VERDICTS,
    TESTERS,
    VERDICTS,
    HarnessError,
    active_receipt_paths,
    is_quarantined_receipt,
    is_within,
    read_json_object,
    receipt_paths,
    repository_head,
    sha256_file,
    validate_tester,
    write_json_object,
)
from scripts.acceptance.v050_common import (
    require_object as _object,
)
from scripts.acceptance.v050_common import (
    require_string as _string,
)

_REPO_ROOT = Path(__file__).resolve().parents[2]
_ACCEPTANCE_ROOT = _REPO_ROOT / "docs" / "acceptance" / "v0.5.0"
_CHECKLIST_PATH = _ACCEPTANCE_ROOT / "checklist-items.json"
_MANIFEST_PATH = _ACCEPTANCE_ROOT / "artifact-manifest.json"
_EVIDENCE_ROOT = _ACCEPTANCE_ROOT / "evidence"
def _public_evidence_roots(repo_root: Path = _REPO_ROOT) -> tuple[Path, ...]:
    """Return every acceptance version directory matching docs/acceptance/v* plus docs/evidence."""
    acceptance_dir = repo_root / "docs" / "acceptance"
    roots: list[Path] = []
    if acceptance_dir.is_dir():
        roots.extend(
            sorted(
                path.relative_to(repo_root)
                for path in acceptance_dir.glob("v*")
                if path.is_dir()
            )
        )
    evidence_dir = repo_root / "docs" / "evidence"
    if evidence_dir.exists():
        roots.append(Path("docs/evidence"))
    return tuple(roots)


_ROUTE_ALIASES: dict[str, str | None] = {
    "primary": PRIMARY_MODEL_ROUTE,
    "fallback": FALLBACK_MODEL_ROUTE,
    "none": None,
}
_RELEASE_RELEVANT_PATHS = ("talaria", "pyproject.toml", "uv.lock", "src")


@dataclass(frozen=True)
class PrivacyPatternDef:
    pattern: re.Pattern[bytes]
    label: str
    rule_name: str


#: Centralized privacy pattern registry. Designed so any amendment from the
#: surveyor/architect is a localized data change rather than an architectural rewrite.
PRIVACY_PATTERNS: tuple[PrivacyPatternDef, ...] = (
    PrivacyPatternDef(
        pattern=re.compile(rb"/(?:Users|home)/[A-Za-z0-9._-]+"),
        label="operator home path",
        rule_name="operator-home-path",
    ),
    PrivacyPatternDef(
        pattern=re.compile(rb"/-(?:Users|home)-[A-Za-z0-9._-]+-"),
        label="encoded operator home path",
        rule_name="encoded-home-path",
    ),
    PrivacyPatternDef(
        pattern=re.compile(rb"/private/var/folders/"),
        label="operator temporary-directory identifier",
        rule_name="temporary-directory",
    ),
    PrivacyPatternDef(
        pattern=re.compile(rb"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"),
        label="email address",
        rule_name="email-address",
    ),
    PrivacyPatternDef(
        pattern=re.compile(rb"Authorization:\s*Bearer\s+\S+", re.IGNORECASE),
        label="bearer credential",
        rule_name="bearer-credential",
    ),
    PrivacyPatternDef(
        pattern=re.compile(rb"(?:token|credential)=[^&\s]+", re.IGNORECASE),
        label="credential query value",
        rule_name="credential-query-value",
    ),
    PrivacyPatternDef(
        pattern=re.compile(rb"\bw[A-Za-z0-9]+:[pt][A-Za-z0-9]+\b"),
        label="terminal pane identifier",
        rule_name="pane-coordinate",
    ),
    PrivacyPatternDef(
        pattern=re.compile(
            rb"\b(?:worker|controller|reviewer|architect|investigator|tester|operator)-\d+(?:-\d+)*\b"
        ),
        label="session name",
        rule_name="session-name",
    ),
)

_PRIVATE_PATTERNS = tuple((p.pattern, p.label) for p in PRIVACY_PATTERNS)

FORBIDDEN_KEY_NAMES: frozenset[str] = frozenset({
    "tester_pane",
    "pane_id",
    "pane",
    "session_name",
    "goal_id",
    "workdir_hash",
    "working_directory_hash",
    "cwd_hash",
    "workdir_sha256",
    "working_directory_sha256",
    "path_hash",
})


def is_forbidden_key(key: str) -> bool:
    return (
        key in FORBIDDEN_KEY_NAMES
        or key.endswith("_pane")
        or key.endswith("_workdir_hash")
        or key.endswith("_workdir_sha256")
        or key.endswith("_cwd_hash")
    )


def is_absolute_filesystem_path(tok: str) -> bool:
    if not isinstance(tok, str):
        return False
    if tok.startswith(("<scratch-root>", "<candidate-root>", "<integration-tree>")):
        return False
    if tok.startswith("file://"):
        target = tok[7:]
        if target.startswith(("<scratch-root>", "<candidate-root>", "<integration-tree>")):
            return False
        return is_absolute_filesystem_path(target)
    if tok.startswith(("http://", "https://", "//")):
        return False
    if tok.startswith("~/") or re.match(r"^~[A-Za-z0-9._-]+/", tok):
        return True
    if tok in ("/dev/null", "/dev/ptmx", "/dev/tty"):
        return False
    # Reversible transformation: URL-encoding (e.g. %2FUsers%2F... or %2e%2e%2e%2ftalaria)
    if "%2f" in tok.lower() or "%2e" in tok.lower():
        unquoted = urllib.parse.unquote(tok)
        if unquoted != tok and is_absolute_filesystem_path(unquoted):
            return True
    # Reversible transformation: escaped path separators (\/ or \\)
    if r"\/" in tok or r"\\" in tok:
        unescaped = tok.replace(r"\/", "/").replace(r"\\", "/")
        if unescaped != tok and is_absolute_filesystem_path(unescaped):
            return True
    # Encoded path prefixes
    if tok.startswith(("/-Users-", "/-home-", "/-tmp-", "/-private-", "/-var-", "/-opt-")):
        return True
    # Windows absolute path prefixes
    windows_prefixes = (
        "\\tmp\\",
        "\\temp\\",
        "\\Users\\",
        "\\home\\",
        "C:\\",
        "c:\\",
        "D:\\",
        "d:\\",
    )
    if tok.startswith(windows_prefixes):
        return True
    # Truncation or abbreviation of refused paths (.../talaria, …/talaria)
    if tok.startswith((".../", "…/")):
        return True
    if tok.startswith("/") and not tok.startswith("//"):
        if tok.startswith(("/v1/", "/api/")):
            return False
        if tok.count("/") >= 2:
            return True
        if tok.endswith("/") and len(tok) > 1:
            return True
        first_segment = tok[1:].split("/")[0]
        if first_segment in (
            "tmp",
            "private",
            "var",
            "opt",
            "Users",
            "home",
            "root",
            "etc",
            "usr",
            "Volumes",
            "srv",
            "mnt",
        ):
            return True
        if "." in first_segment and not first_segment.startswith("."):
            return True
    return False


def find_absolute_paths_in_text(text: str) -> list[str]:
    violations: list[str] = []
    raw_tokens = re.findall(r'[^\s"\'\(\)\[\]{}`]+', text)
    for raw in raw_tokens:
        tok = raw.strip("\"'()[]{}`<>").rstrip(",.;:!?")
        if not tok:
            continue
        if "=" in tok:
            val = tok.split("=", 1)[1].strip("\"'()[]{}`<>").rstrip(",.;:!?")
            if is_absolute_filesystem_path(val):
                violations.append(val)
                continue
        if is_absolute_filesystem_path(tok):
            violations.append(tok)
    return violations


class ValueCategory(StrEnum):
    COUNT = "count"
    DIGEST = "digest"
    TIMESTAMP = "timestamp"
    GATEWAY_SESSION_ID = "gateway-session-id"
    CLOSED_VOCABULARY = "closed-vocabulary"
    OBJECT = "object"
    LIST = "list"
    BOOLEAN = "boolean"
    STRING = "string"
    PATH = "path"
    HARNESS_IDENTITY = "harness-identity"
    HARNESS_LABEL = "harness-label"
    URL = "url"
    MAP = "map"
    FRAME_LABEL = "frame-label"


ALLOWED_PREIMAGE_CLASSES: frozenset[str] = frozenset({
    "git-commit",
    "commit",
    "wheel",
    "artifact",
    "evidence-file",
    "file",
    "rendered-frame",
    "text-twin",
    "source-capture",
    "step-payload",
    "receipt",
})

ALLOWED_URL_SCHEMES: frozenset[str] = frozenset({"http", "https", "ws", "wss", "file"})
ALLOWED_URL_HOSTS: frozenset[str] = frozenset({
    "127.0.0.1",
    "localhost",
    "::1",
    "[::1]",
    "<gateway>",
    "<candidate-root>",
})
ALLOWED_URL_PORTS: frozenset[int | None] = frozenset({None, 80, 443, 8000, 8080, 8765})

REFUSED_CLASSES: frozenset[str] = frozenset({
    "absolute-filesystem-path",
    "pane-tab-workspace-coordinate",
    "role-digit-session-name",
    "non-loopback-host",
    "operator-identity",
    "non-default-profile-name",
})


V050_INSTALL_SCHEMA = "talaria-v0.5.0-install-v1"
V061_ITEM_SCHEMA = "talaria-v0.6.1-receipt-v1"
V061_INSTALL_SCHEMA = "talaria-v0.6.1-install-v1"
V061_RELEASE = "0.6.1"
V061_ROLE_LABELS = ("dedicated-tester", "worker-lane-a", "worker-lane-b", "controller")


@dataclass(frozen=True)
class RecordSchema:
    name: str
    declared_keys: dict[str, ValueCategory]
    nested_schemas: dict[str, dict[str, ValueCategory]] = field(default_factory=dict)
    map_schemas: dict[str, tuple[str, ValueCategory]] = field(default_factory=dict)
    nullable_keys: frozenset[str] = field(default_factory=frozenset)
    digest_preimages: dict[str, str] = field(default_factory=dict)
    vocabularies: dict[str, frozenset[str]] = field(default_factory=dict)

    def _lookup_preimage(self, loc: str, key: str) -> str | None:
        candidates = [loc]
        parts = loc.split(".")
        for i in range(len(parts)):
            candidates.append(".".join(parts[i:]))
        name_parts = self.name.split(".")
        for i in range(len(name_parts)):
            prefix = ".".join(name_parts[i:])
            candidates.append(f"{prefix}.{key}")
            candidates.append(f"{prefix}.{loc}")
        candidates.append(key)
        for cand in candidates:
            if cand in self.digest_preimages:
                return self.digest_preimages[cand]
        return None

    def _lookup_vocabulary(self, loc: str, key: str) -> frozenset[str] | None:
        candidates = [loc]
        parts = loc.split(".")
        for i in range(len(parts)):
            candidates.append(".".join(parts[i:]))
        name_parts = self.name.split(".")
        for i in range(len(name_parts)):
            prefix = ".".join(name_parts[i:])
            candidates.append(f"{prefix}.{key}")
            candidates.append(f"{prefix}.{loc}")
        candidates.append(key)
        for cand in candidates:
            if cand in self.vocabularies:
                return self.vocabularies[cand]
        return None

    def __post_init__(self) -> None:
        for target, preimage_cls in self.digest_preimages.items():
            if preimage_cls not in ALLOWED_PREIMAGE_CLASSES:
                raise ValueError(
                    f"Schema {self.name!r}: preimage class {preimage_cls!r} for {target!r} "
                    f"is not an allowed preimage class ({sorted(ALLOWED_PREIMAGE_CLASSES)})"
                )
        for k, cat in self.declared_keys.items():
            if cat == ValueCategory.DIGEST:
                preimage = self._lookup_preimage(k, k)
                if not preimage:
                    raise ValueError(
                        f"Schema {self.name!r}: digest field {k!r} has no declared preimage class"
                    )
            elif cat == ValueCategory.CLOSED_VOCABULARY:
                vocab = self._lookup_vocabulary(k, k)
                if not vocab:
                    raise ValueError(
                        f"Schema {self.name!r}: closed-vocabulary field {k!r} "
                        "has no declared vocabulary"
                    )
        if "." not in self.name:
            for nk, n_keys in self.nested_schemas.items():
                for field_name, cat in n_keys.items():
                    if cat == ValueCategory.DIGEST:
                        loc = f"{nk}.{field_name}"
                        preimage = self._lookup_preimage(loc, field_name)
                        if not preimage:
                            raise ValueError(
                                f"Schema {self.name!r}: nested digest field {loc!r} "
                                "has no declared preimage class"
                            )
                    elif cat == ValueCategory.CLOSED_VOCABULARY:
                        loc = f"{nk}.{field_name}"
                        vocab = self._lookup_vocabulary(loc, field_name)
                        if not vocab:
                            raise ValueError(
                                f"Schema {self.name!r}: nested closed-vocabulary field {loc!r} "
                                "has no declared vocabulary"
                            )
            for mk, (_key_type, val_cat) in self.map_schemas.items():
                if val_cat == ValueCategory.DIGEST:
                    preimage = self._lookup_preimage(mk, mk)
                    if not preimage:
                        raise ValueError(
                            f"Schema {self.name!r}: digest map {mk!r} "
                            "has no declared preimage class"
                        )

    def validate(
        self, doc: dict[str, Any], *, path: Path, prefix: str = ""
    ) -> list[str]:
        errors: list[str] = []
        for k, v in doc.items():
            loc = f"{prefix}.{k}" if prefix else str(k)
            if is_forbidden_key(k):
                errors.append(f"{path}: contains forbidden key {k!r} at {loc}")
                continue
            if k not in self.declared_keys:
                errors.append(
                    f"{path}: {self.name} contains undeclared key {k!r} at {loc}"
                )
                continue
            if v is None and (k in self.nullable_keys or loc.split(".")[-1] in self.nullable_keys):
                continue
            cat = self.declared_keys[k]
            if cat == ValueCategory.COUNT:
                if not isinstance(v, (int, float)) or v < 0:
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be a non-negative number"
                    )
            elif cat == ValueCategory.DIGEST:
                preimage = self._lookup_preimage(loc, k)
                if not preimage or preimage not in ALLOWED_PREIMAGE_CLASSES:
                    errors.append(
                        f"{path}: {self.name} field {loc!r} has undeclared or disallowed "
                        f"digest preimage class ({preimage!r})"
                    )
                elif isinstance(v, list):
                    if not all(
                        isinstance(x, str)
                        and len(x) in (40, 64)
                        and all(c in "0123456789abcdefABCDEF" for c in x)
                        for x in v
                    ):
                        errors.append(
                            f"{path}: {self.name} field {loc!r} must be a list of hex digests"
                        )
                elif not (
                    isinstance(v, str)
                    and len(v) in (40, 64)
                    and all(c in "0123456789abcdefABCDEF" for c in v)
                ):
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be a hex digest"
                    )
            elif cat == ValueCategory.TIMESTAMP:
                if not isinstance(v, str) or not v.strip():
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be a timestamp string"
                    )
            elif cat == ValueCategory.GATEWAY_SESSION_ID:
                if not isinstance(v, str) or not v.strip():
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be a gateway session id string"
                    )
            elif cat == ValueCategory.CLOSED_VOCABULARY:
                vocab = self._lookup_vocabulary(loc, k)
                if not isinstance(v, str) or not v.strip():
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be a non-empty string"
                    )
                elif not vocab:
                    errors.append(
                        f"{path}: {self.name} field {loc!r} has no registered closed vocabulary"
                    )
                elif v not in vocab:
                    errors.append(
                        f"{path}: {self.name} field {loc!r} value {v!r} not in registered "
                        f"closed vocabulary ({sorted(vocab)})"
                    )
            elif cat == ValueCategory.HARNESS_IDENTITY:
                if v is not None:
                    if not isinstance(v, str) or not v.strip():
                        errors.append(
                            f"{path}: {self.name} field {loc!r} must be a non-empty string or null"
                        )
                    elif find_absolute_paths_in_text(v):
                        errors.append(
                            f"{path}: {self.name} field {loc!r} must not contain an absolute "
                            f"filesystem path ({v!r})"
                        )
            elif cat == ValueCategory.HARNESS_LABEL:
                if not isinstance(v, str) or not v.strip():
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be a "
                        "non-empty harness label string"
                    )
                elif not (v.startswith("<") and v.endswith(">") and len(v) > 2):
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be a placeholder harness label "
                        f"enclosed in '<...>' ({v!r})"
                    )
            elif cat == ValueCategory.PATH:
                if not isinstance(v, str) or not v.strip():
                    errors.append(f"{path}: {self.name} field {loc!r} must be a path string")
                elif "..." in v or "…" in v:
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must not be an abbreviated path ({v!r})"
                    )
                elif is_absolute_filesystem_path(v) or find_absolute_paths_in_text(v):
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must not be an absolute "
                        f"filesystem path ({v!r})"
                    )
                elif v.startswith("/") or v.startswith("~"):
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be repository-relative or "
                        f"placeholder ({v!r})"
                    )
            elif cat == ValueCategory.LIST:
                if not isinstance(v, list):
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be a list"
                    )
                elif k == "redactions":
                    surf_w = (
                        doc.get("width")
                        if isinstance(doc, dict) and isinstance(doc.get("width"), int)
                        else None
                    )
                    surf_h = (
                        doc.get("height")
                        if isinstance(doc, dict) and isinstance(doc.get("height"), int)
                        else None
                    )
                    errors.extend(
                        validate_redactions_list(
                            v, path=path, prefix=loc, surface_width=surf_w, surface_height=surf_h
                        )
                    )
                elif k in ("read_confirmations", "redaction_confirmations"):
                    for c_idx, item in enumerate(v):
                        if isinstance(item, dict):
                            errors.extend(
                                validate_read_confirmation_record(
                                    item, path=path, prefix=f"{loc}[{c_idx}]"
                                )
                            )
                        else:
                            errors.append(
                                f"{path}: {self.name} field {loc}[{c_idx}] must be an object"
                            )
            elif cat == ValueCategory.BOOLEAN:
                if not isinstance(v, bool):
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be a boolean"
                    )
            elif cat == ValueCategory.OBJECT:
                if isinstance(v, dict):
                    if k == "read_confirmation":
                        errors.extend(validate_read_confirmation_record(v, path=path, prefix=loc))
                    elif k in self.nested_schemas:
                        sub_schema = RecordSchema(
                            f"{self.name}.{k}",
                            self.nested_schemas[k],
                            nested_schemas=self.nested_schemas,
                            map_schemas=self.map_schemas,
                            nullable_keys=self.nullable_keys,
                            digest_preimages=self.digest_preimages,
                            vocabularies=self.vocabularies,
                        )
                        errors.extend(sub_schema.validate(v, path=path, prefix=loc))
                    elif k in self.map_schemas:
                        key_type, val_cat = self.map_schemas[k]
                        for mk, mv in v.items():
                            mloc = f"{loc}.{mk}"
                            if is_forbidden_key(mk):
                                errors.append(
                                    f"{path}: contains forbidden key {mk!r} at {mloc}"
                                )
                            if key_type == "path" and (
                                is_absolute_filesystem_path(mk)
                                or mk.startswith("/")
                                or ".." in mk
                                or "..." in mk
                                or "…" in mk
                            ):
                                errors.append(
                                    f"{path}: {self.name} map key {mk!r} at {mloc} "
                                    "must be a valid relative path"
                                )
                            if val_cat == ValueCategory.DIGEST:
                                map_preimage = (
                                    self._lookup_preimage(mloc, mk)
                                    or self._lookup_preimage(loc, k)
                                )
                                if (
                                    not map_preimage
                                    or map_preimage not in ALLOWED_PREIMAGE_CLASSES
                                ):
                                    errors.append(
                                        f"{path}: {self.name} map value at {mloc} has undeclared "
                                        f"or disallowed digest preimage class ({map_preimage!r})"
                                    )
                                elif not (
                                    isinstance(mv, str)
                                    and len(mv) in (40, 64)
                                    and all(c in "0123456789abcdefABCDEF" for c in mv)
                                ):
                                    errors.append(
                                        f"{path}: {self.name} map value at {mloc} "
                                        "must be a hex digest"
                                    )
                elif isinstance(v, str):
                    if find_absolute_paths_in_text(v):
                        errors.append(
                            f"{path}: {self.name} field {loc!r} must not contain an absolute "
                            f"filesystem path ({v!r})"
                        )
                else:
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be an object"
                    )
            elif cat == ValueCategory.URL:
                if not isinstance(v, str) or not v.strip():
                    errors.append(f"{path}: {self.name} field {loc!r} must be a url string")
                elif find_absolute_paths_in_text(v):
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must not contain an absolute "
                        f"filesystem path ({v!r})"
                    )
                elif v.startswith("<") and v.endswith(">"):
                    if v not in ALLOWED_URL_HOSTS:
                        errors.append(
                            f"{path}: {self.name} field {loc!r} has unapproved placeholder {v!r}"
                        )
                else:
                    try:
                        parsed = urllib.parse.urlsplit(v)
                    except Exception as exc:
                        errors.append(
                            f"{path}: {self.name} field {loc!r} is not a valid url: {exc}"
                        )
                        continue
                    if not parsed.scheme or parsed.scheme not in ALLOWED_URL_SCHEMES:
                        errors.append(
                            f"{path}: {self.name} field {loc!r} has unapproved url scheme "
                            f"{parsed.scheme!r} ({sorted(ALLOWED_URL_SCHEMES)})"
                        )
                    elif parsed.username or parsed.password or ("@" in parsed.netloc):
                        errors.append(
                            f"{path}: {self.name} field {loc!r} must not contain "
                            "userinfo or credentials"
                        )
                    elif parsed.scheme == "file":
                        clean_netloc = parsed.netloc.strip("/")
                        if clean_netloc and clean_netloc not in ALLOWED_URL_HOSTS:
                            errors.append(
                                f"{path}: {self.name} file url {loc!r} has unapproved host "
                                f"{parsed.netloc!r}"
                            )
                    else:
                        host = parsed.hostname
                        if not host or host not in ALLOWED_URL_HOSTS:
                            approved_hosts = sorted(ALLOWED_URL_HOSTS)
                            errors.append(
                                f"{path}: {self.name} endpoint url {loc!r} has unapproved host "
                                f"{host!r} (must be loopback or approved placeholder: "
                                f"{approved_hosts})"
                            )
                        elif parsed.port not in ALLOWED_URL_PORTS:
                            approved_ports = sorted(p for p in ALLOWED_URL_PORTS if p is not None)
                            errors.append(
                                f"{path}: {self.name} endpoint url {loc!r} has unapproved port "
                                f"{parsed.port!r} (must be standard or gateway port: "
                                f"{approved_ports})"
                            )
            elif cat == ValueCategory.STRING:
                if isinstance(v, list):
                    if not all(isinstance(x, str) for x in v):
                        errors.append(
                            f"{path}: {self.name} field {loc!r} must be a string or list of strings"
                        )
                elif not isinstance(v, str):
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be a string"
                    )
                elif loc == "self_check.expected_rejection" or k == "expected_rejection":
                    masked_v = _SENTINEL_CANDIDATE_PATTERN.sub(
                        lambda m: " " * len(m.group(0)), v
                    )
                    unmasked_paths = find_absolute_paths_in_text(masked_v)
                    if unmasked_paths:
                        errors.append(
                            f"{path}: {self.name} field {loc!r} contains absolute filesystem path "
                            f"{unmasked_paths[0]!r} (must be sanitized or redacted)"
                        )
                    val_bytes = masked_v.encode("utf-8")
                    for pattern, label in _PRIVATE_PATTERNS:
                        if pattern.search(val_bytes):
                            errors.append(
                                f"{path}: {self.name} field {loc!r} contains a private {label} "
                                "(must be sanitized or redacted)"
                            )
            elif cat == ValueCategory.FRAME_LABEL:
                if isinstance(v, int):
                    if v < 0:
                        errors.append(
                            f"{path}: {self.name} field {loc!r} must be a "
                            "non-negative number or string label"
                        )
                elif isinstance(v, str):
                    if not v.strip():
                        errors.append(
                            f"{path}: {self.name} field {loc!r} must be a non-empty string label"
                        )
                    elif find_absolute_paths_in_text(v):
                        errors.append(
                            f"{path}: {self.name} field {loc!r} must not contain an absolute "
                            f"filesystem path ({v!r})"
                        )
                else:
                    errors.append(
                        f"{path}: {self.name} field {loc!r} must be a string label "
                        "or non-negative number"
                    )
        return errors


REDACTION_ITEM_SCHEMA = RecordSchema(
    name="redaction-item",
    declared_keys={
        "index": ValueCategory.COUNT,
        "covered_class": ValueCategory.CLOSED_VOCABULARY,
        "twin_span": ValueCategory.STRING,
        "region": ValueCategory.OBJECT,
    },
    nested_schemas={
        "region": {
            "x": ValueCategory.COUNT,
            "y": ValueCategory.COUNT,
            "width": ValueCategory.COUNT,
            "height": ValueCategory.COUNT,
        },
    },
    vocabularies={
        "covered_class": REFUSED_CLASSES,
    },
)

REDACTION_CONFIRMATION_ITEM_SCHEMA = RecordSchema(
    name="redaction-confirmation-item",
    declared_keys={
        "index": ValueCategory.COUNT,
        "covered_class": ValueCategory.CLOSED_VOCABULARY,
        "region_matches_twin_span": ValueCategory.BOOLEAN,
    },
    vocabularies={
        "covered_class": REFUSED_CLASSES,
    },
)

READ_CONFIRMATION_RECORD_SCHEMA = RecordSchema(
    name="read-confirmation-record",
    declared_keys={
        "image": ValueCategory.STRING,
        "read_by": ValueCategory.CLOSED_VOCABULARY,
        "read_at": ValueCategory.TIMESTAMP,
        "redactions_confirmed": ValueCategory.LIST,
        "witnessed_element": ValueCategory.STRING,
        "nothing_else_masked": ValueCategory.BOOLEAN,
    },
    vocabularies={
        "read_by": frozenset(V061_ROLE_LABELS),
    },
)

_SENTINEL_PATTERN = re.compile(r"\[redacted:([a-z0-9_-]+):(\d+)\]")
_SENTINEL_CANDIDATE_PATTERN = re.compile(r"\[redacted:[^\]]*\]")


def validate_redactions_list(
    redactions: Any,
    *,
    path: Path,
    prefix: str = "redactions",
    surface_width: int | None = None,
    surface_height: int | None = None,
) -> list[str]:
    """Validate a capture-metadata redactions list against the third amendment contract.

    Enforces:
    - Must be a list of objects.
    - Each entry must conform to REDACTION_ITEM_SCHEMA.
    - covered_class must be in REFUSED_CLASSES (an allowed class is refused).
    - index must be a non-negative integer, unique within the image.
    - twin_span must strictly match '[redacted:<covered_class>:<index>]'.
    - region must specify non-negative integers x, y, width, height with width > 0, height > 0.
    - blanket redactions covering entire surface are refused.
    """
    if not isinstance(redactions, list):
        return [f"{path}: {prefix} must be a list"]
    errors: list[str] = []
    seen_indices: set[int] = set()
    for idx, entry in enumerate(redactions):
        loc = f"{prefix}[{idx}]"
        if not isinstance(entry, dict):
            errors.append(f"{path}: {loc} must be an object")
            continue
        errors.extend(REDACTION_ITEM_SCHEMA.validate(entry, path=path, prefix=loc))
        entry_idx = entry.get("index")
        if isinstance(entry_idx, int) and entry_idx >= 0:
            if entry_idx in seen_indices:
                errors.append(f"{path}: duplicate redaction index {entry_idx} at {loc}")
            seen_indices.add(entry_idx)
        else:
            errors.append(f"{path}: {loc}.index must be a non-negative integer")
        covered_class = entry.get("covered_class")
        if isinstance(covered_class, str):
            if covered_class not in REFUSED_CLASSES:
                errors.append(
                    f"{path}: redaction at {loc} declared covered_class {covered_class!r} "
                    "is not a refused class (allowed classes may not be masked)"
                )
            expected_span = f"[redacted:{covered_class}:{entry_idx}]"
            actual_span = entry.get("twin_span")
            if actual_span != expected_span:
                errors.append(
                    f"{path}: redaction at {loc} twin_span {actual_span!r} "
                    f"must match expected sentinel {expected_span!r}"
                )
        region = entry.get("region")
        if isinstance(region, dict):
            for coord in ("x", "y", "width", "height"):
                val = region.get(coord)
                if not isinstance(val, (int, float)) or val < 0:
                    errors.append(
                        f"{path}: {loc}.region.{coord} must be a non-negative number"
                    )
            reg_w = region.get("width")
            reg_h = region.get("height")
            reg_x = region.get("x")
            reg_y = region.get("y")
            if isinstance(reg_w, (int, float)) and reg_w <= 0:
                errors.append(f"{path}: {loc}.region.width must be positive")
            if isinstance(reg_h, (int, float)) and reg_h <= 0:
                errors.append(f"{path}: {loc}.region.height must be positive")
            if (
                surface_width is not None
                and surface_height is not None
                and isinstance(reg_x, (int, float))
                and isinstance(reg_y, (int, float))
                and isinstance(reg_w, (int, float))
                and isinstance(reg_h, (int, float))
                and reg_x == 0
                and reg_y == 0
                and reg_w >= surface_width
                and reg_h >= surface_height
            ):
                errors.append(
                    f"{path}: {loc}.region covers entire surface (blanket redactions are refused)"
                )
        else:
            errors.append(f"{path}: {loc}.region must be an object")
    return errors


def validate_read_confirmation_record(
    doc: dict[str, Any], *, path: Path, prefix: str = ""
) -> list[str]:
    """Validate a single read-confirmation record schema and field semantics."""
    errors: list[str] = []
    errors.extend(READ_CONFIRMATION_RECORD_SCHEMA.validate(doc, path=path, prefix=prefix))
    if doc.get("nothing_else_masked") is not True:
        loc = f"{prefix}.nothing_else_masked" if prefix else "nothing_else_masked"
        errors.append(f"{path}: {loc} must be true")
    rc_list = doc.get("redactions_confirmed")
    if isinstance(rc_list, list):
        for idx, rc in enumerate(rc_list):
            r_loc = (
                f"{prefix}.redactions_confirmed[{idx}]"
                if prefix
                else f"redactions_confirmed[{idx}]"
            )
            if isinstance(rc, dict):
                errors.extend(
                    REDACTION_CONFIRMATION_ITEM_SCHEMA.validate(rc, path=path, prefix=r_loc)
                )
                if not isinstance(rc.get("region_matches_twin_span"), bool):
                    errors.append(f"{path}: {r_loc}.region_matches_twin_span must be a boolean")
                c_class = rc.get("covered_class")
                if isinstance(c_class, str) and c_class not in REFUSED_CLASSES:
                    errors.append(
                        f"{path}: {r_loc}.covered_class {c_class!r} is not a refused class"
                    )
            else:
                errors.append(f"{path}: {r_loc} must be an object")
    return errors


def validate_twin_redactions(
    twin_path: Path, twin_text: str, redactions: list[dict[str, Any]]
) -> list[str]:
    """Enforce the two-way sentinel match between text twin and capture metadata redactions.

    Checks:
    1. Every sentinel [redacted:<covered_class>:<index>] in twin matches an entry in redactions.
    2. Every entry in redactions has its sentinel present in the twin.
    3. Any malformed sentinel shape ([redacted:...]) is refused.
    4. Sentinels in the twin must not duplicate the same index.
    5. Sentinel covered_class must be in REFUSED_CLASSES.
    """
    errors: list[str] = []
    all_candidates = _SENTINEL_CANDIDATE_PATTERN.findall(twin_text)
    for cand in all_candidates:
        if not _SENTINEL_PATTERN.fullmatch(cand):
            errors.append(
                f"{twin_path}: malformed redaction sentinel {cand!r} in text twin"
            )

    twin_matches: list[tuple[str, int]] = []
    seen_twin_indices: set[int] = set()
    for m in _SENTINEL_PATTERN.finditer(twin_text):
        c_class, idx_str = m.group(1), m.group(2)
        idx = int(idx_str)
        if idx in seen_twin_indices:
            errors.append(
                f"{twin_path}: duplicate redaction sentinel for index {idx} in text twin"
            )
        seen_twin_indices.add(idx)
        twin_matches.append((c_class, idx))

    redaction_map: dict[int, dict[str, Any]] = {}
    for r in redactions:
        if isinstance(r, dict) and isinstance(r.get("index"), int):
            redaction_map[r["index"]] = r

    for c_class, idx in twin_matches:
        if c_class not in REFUSED_CLASSES:
            errors.append(
                f"{twin_path}: redaction sentinel '[redacted:{c_class}:{idx}]' "
                f"covered_class {c_class!r} is not a refused class"
            )
        if idx not in redaction_map:
            errors.append(
                f"{twin_path}: unmatched redaction sentinel '[redacted:{c_class}:{idx}]' "
                "in twin has no matching entry in capture-metadata redactions"
            )
        else:
            entry = redaction_map[idx]
            if entry.get("covered_class") != c_class:
                exp_class = entry.get("covered_class")
                errors.append(
                    f"{twin_path}: redaction sentinel '[redacted:{c_class}:{idx}]' "
                    f"in twin has mismatched covered_class (expected {exp_class!r})"
                )

    for idx, entry in redaction_map.items():
        expected_span = entry.get("twin_span") or f"[redacted:{entry.get('covered_class')}:{idx}]"
        if idx not in seen_twin_indices or expected_span not in twin_text:
            errors.append(
                f"{twin_path}: capture-metadata redaction entry index {idx} "
                f"({expected_span}) not found in text twin"
            )

    return errors


def mask_matched_sentinels(text: str, redactions: list[dict[str, Any]]) -> str:
    """Replace interior of matched redaction sentinels with spaces of equal length.

    Preserves character offsets and line structure while preventing the masked span
    from matching filesystem paths or private identifier patterns.
    """
    valid_spans: set[str] = set()
    for r in redactions:
        if isinstance(r, dict):
            span = r.get("twin_span")
            if isinstance(span, str):
                valid_spans.add(span)
            elif "covered_class" in r and "index" in r:
                valid_spans.add(f"[redacted:{r['covered_class']}:{r['index']}]")

    def _replace_sentinel(m: re.Match[str]) -> str:
        sentinel = m.group(0)
        if sentinel in valid_spans:
            return " " * len(sentinel)
        return sentinel

    return _SENTINEL_PATTERN.sub(_replace_sentinel, text)


def validate_image_read_confirmations(
    png_name: str,
    *,
    twin_file: str | None,
    twin_text: str | None,
    redactions: list[dict[str, Any]],
    confirmations: list[dict[str, Any]],
    receipt_or_path: Path | str,
) -> list[str]:
    """Enforce Section 3 checkable read rules for a redacted image.

    Checks:
    1. A confirmation record must exist for this image ({image, read_by, read_at, ...}).
    2. read_by must be in V061_ROLE_LABELS.
    3. read_at must be an ISO 8601 timestamp.
    4. nothing_else_masked must be True.
    5. every redactions entry has a matching line in redactions_confirmed.
    6. each confirmed covered_class must be in REFUSED_CLASSES (an allowed class is refused).
    7. region_matches_twin_span must be True when twin is present, False when absent.
    8. witnessed_element must appear in the twin outside every sentinel span.
    """
    errors: list[str] = []
    def _matches_image(rec_img: Any, target_name: str) -> bool:
        if not isinstance(rec_img, str):
            return False
        if rec_img == target_name:
            return True
        rec_p = Path(rec_img)
        tgt_p = Path(target_name)
        if rec_p == tgt_p:
            return True
        if rec_p.parent != Path(".") or tgt_p.parent != Path("."):
            return False
        return rec_p.name == tgt_p.name

    matching_records = [
        rec for rec in confirmations
        if isinstance(rec, dict) and _matches_image(rec.get("image"), png_name)
    ]
    if not redactions:
        for rec in matching_records:
            if rec.get("redactions_confirmed"):
                errors.append(
                    f"{receipt_or_path}: read confirmation for unredacted image '{png_name}' "
                    "cannot declare redactions_confirmed"
                )
        return errors

    if not matching_records:
        errors.append(
            f"{receipt_or_path}: redacted screenshot '{png_name}' "
            "has no recorded read confirmation"
        )
        return errors

    for rec in matching_records:
        errors.extend(
            validate_read_confirmation_record(rec, path=Path(str(receipt_or_path)))
        )
        read_by = rec.get("read_by")
        if read_by not in V061_ROLE_LABELS:
            errors.append(
                f"{receipt_or_path}: read confirmation for '{png_name}' read_by {read_by!r} "
                f"must be in V061_ROLE_LABELS ({', '.join(V061_ROLE_LABELS)})"
            )
        read_at = rec.get("read_at")
        if not isinstance(read_at, str) or not read_at.strip():
            errors.append(
                f"{receipt_or_path}: read confirmation for '{png_name}' read_at "
                "must be an ISO 8601 timestamp"
            )
        else:
            try:
                dt.datetime.fromisoformat(read_at)
            except ValueError:
                errors.append(
                    f"{receipt_or_path}: read confirmation for '{png_name}' read_at "
                    "must be an ISO 8601 timestamp"
                )
        if rec.get("nothing_else_masked") is not True:
            errors.append(
                f"{receipt_or_path}: read confirmation for '{png_name}' "
                "nothing_else_masked must be true"
            )

        witnessed_element = rec.get("witnessed_element")
        if not isinstance(witnessed_element, str) or not witnessed_element.strip():
            errors.append(
                f"{receipt_or_path}: read confirmation for '{png_name}' witnessed_element "
                "must be a non-empty string"
            )
        elif twin_text is not None:
            unmasked_twin = _SENTINEL_CANDIDATE_PATTERN.sub(
                lambda m: " " * len(m.group(0)), twin_text
            )
            if witnessed_element not in unmasked_twin:
                if witnessed_element in twin_text:
                    errors.append(
                        f"{receipt_or_path}: read confirmation for '{png_name}' witnessed_element "
                        f"{witnessed_element!r} was found inside a mask "
                        "(a mask may never cover what the case exists to witness)"
                    )
                else:
                    errors.append(
                        f"{receipt_or_path}: read confirmation for '{png_name}' witnessed_element "
                        f"{witnessed_element!r} does not appear in text twin '{twin_file}'"
                    )

        rc_list = rec.get("redactions_confirmed")
        if not isinstance(rc_list, list):
            errors.append(
                f"{receipt_or_path}: read confirmation for '{png_name}' "
                "redactions_confirmed must be a list"
            )
            continue

        confirmed_by_idx: dict[int, dict[str, Any]] = {}
        for rc in rc_list:
            if isinstance(rc, dict):
                i = rc.get("index")
                if isinstance(i, int):
                    confirmed_by_idx[i] = rc
                c_class = rc.get("covered_class")
                if isinstance(c_class, str) and c_class not in REFUSED_CLASSES:
                    errors.append(
                        f"{receipt_or_path}: read confirmation for '{png_name}' index {i} "
                        f"confirmed covered_class {c_class!r} is not a refused class"
                    )
                if twin_file is not None:
                    if rc.get("region_matches_twin_span") is not True:
                        errors.append(
                            f"{receipt_or_path}: read confirmation for '{png_name}' index {i} "
                            "region_matches_twin_span must be true"
                        )
                else:
                    if rc.get("region_matches_twin_span") is not False:
                        errors.append(
                            f"{receipt_or_path}: read confirmation for '{png_name}' index {i} "
                            "region_matches_twin_span must be false"
                        )

        for r_entry in redactions:
            r_idx = r_entry.get("index")
            if r_idx not in confirmed_by_idx:
                errors.append(
                    f"{receipt_or_path}: redacted screenshot '{png_name}' redaction span "
                    f"index {r_idx} is unconfirmed (missing from redactions_confirmed)"
                )
            else:
                conf_entry = confirmed_by_idx[r_idx]
                if conf_entry.get("covered_class") != r_entry.get("covered_class"):
                    errors.append(
                        f"{receipt_or_path}: read confirmation for '{png_name}' index {r_idx} "
                        f"covered_class {conf_entry.get('covered_class')!r} does not match "
                        f"redaction {r_entry.get('covered_class')!r}"
                    )

        actual_indices = {r.get("index") for r in redactions if isinstance(r, dict)}
        for c_i in confirmed_by_idx:
            if c_i not in actual_indices:
                errors.append(
                    f"{receipt_or_path}: read confirmation for '{png_name}' confirms index {c_i} "
                    "which does not exist in capture metadata redactions"
                )

    return errors


RECEIPT_SCHEMA = RecordSchema(
    name="receipt",
    declared_keys={
        "schema_version": ValueCategory.CLOSED_VOCABULARY,
        "release": ValueCategory.CLOSED_VOCABULARY,
        "checklist_item": ValueCategory.CLOSED_VOCABULARY,
        "title": ValueCategory.STRING,
        "issue": ValueCategory.STRING,
        "tester": ValueCategory.CLOSED_VOCABULARY,
        "verdict": ValueCategory.CLOSED_VOCABULARY,
        "candidate_commit_sha": ValueCategory.DIGEST,
        "candidate_branch": ValueCategory.STRING,
        "recorded_at": ValueCategory.TIMESTAMP,
        "install": ValueCategory.OBJECT,
        "harness": ValueCategory.OBJECT,
        "gateway": ValueCategory.OBJECT,
        "session": ValueCategory.OBJECT,
        "terminal": ValueCategory.OBJECT,
        "actions": ValueCategory.STRING,
        "actual": ValueCategory.STRING,
        "expected": ValueCategory.OBJECT,
        "evidence": ValueCategory.OBJECT,
        "supersedes": ValueCategory.OBJECT,
        "screenshots_read_by": ValueCategory.CLOSED_VOCABULARY,
        "screenshots_read_at": ValueCategory.TIMESTAMP,
        "twin_path": ValueCategory.PATH,
        "twin_digest": ValueCategory.DIGEST,
        "twin_sha256": ValueCategory.DIGEST,
        "read_confirmations": ValueCategory.LIST,
        "redaction_review": ValueCategory.CLOSED_VOCABULARY,
    },
    nested_schemas={
        "install": {
            "kind": ValueCategory.CLOSED_VOCABULARY,
            "commit": ValueCategory.DIGEST,
            "basis": ValueCategory.STRING,
            "filename": ValueCategory.STRING,
            "sha256": ValueCategory.DIGEST,
            "url": ValueCategory.URL,
            "wheel_path": ValueCategory.PATH,
            "version_reported": ValueCategory.CLOSED_VOCABULARY,
            "help_ok": ValueCategory.BOOLEAN,
        },
        "harness": {
            "kind": ValueCategory.CLOSED_VOCABULARY,
            "commit": ValueCategory.DIGEST,
            "identity": ValueCategory.HARNESS_IDENTITY,
        },
        "evidence": {
            "narrative": ValueCategory.OBJECT,
            "files": ValueCategory.OBJECT,
            "files_listed_at": ValueCategory.TIMESTAMP,
            "screenshots_read_by": ValueCategory.CLOSED_VOCABULARY,
            "screenshots_read_at": ValueCategory.TIMESTAMP,
            "frames": ValueCategory.LIST,
            "wire": ValueCategory.OBJECT,
            "screenshot_path": ValueCategory.PATH,
            "screenshot_sha256": ValueCategory.DIGEST,
            "twin_path": ValueCategory.PATH,
            "twin_digest": ValueCategory.DIGEST,
            "twin_sha256": ValueCategory.DIGEST,
            "pty_result_path": ValueCategory.PATH,
            "pty_result_sha256": ValueCategory.DIGEST,
            "source": ValueCategory.STRING,
            "observation": ValueCategory.STRING,
            "read_confirmations": ValueCategory.LIST,
            "redaction_review": ValueCategory.CLOSED_VOCABULARY,
        },
        "supersedes": {
            "receipt_sha256": ValueCategory.DIGEST,
            "candidate_commit_sha": ValueCategory.DIGEST,
        },
        "narrative": {
            "kind": ValueCategory.CLOSED_VOCABULARY,
            "method": ValueCategory.STRING,
            "observation": ValueCategory.STRING,
            "source": ValueCategory.STRING,
        },
        "wire": {
            "events": ValueCategory.COUNT,
        },
        "gateway": {
            "url": ValueCategory.URL,
            "session_id": ValueCategory.GATEWAY_SESSION_ID,
            "token": ValueCategory.STRING,
            "profile": ValueCategory.STRING,
        },
        "session": {
            "session_id": ValueCategory.GATEWAY_SESSION_ID,
            "title": ValueCategory.STRING,
            "mode": ValueCategory.STRING,
            "profile": ValueCategory.STRING,
        },
        "terminal": {
            "columns": ValueCategory.COUNT,
            "rows": ValueCategory.COUNT,
            "cell_width": ValueCategory.COUNT,
            "cell_height": ValueCategory.COUNT,
            "width": ValueCategory.COUNT,
            "height": ValueCategory.COUNT,
            "dpi": ValueCategory.COUNT,
            "scale": ValueCategory.COUNT,
            "term": ValueCategory.STRING,
            "program": ValueCategory.STRING,
        },
        "expected": {
            "source": ValueCategory.STRING,
            "text": ValueCategory.STRING,
        },
    },
    map_schemas={
        "files": ("path", ValueCategory.DIGEST),
    },
    nullable_keys=frozenset({"commit", "identity", "supersedes"}),
    digest_preimages={
        "candidate_commit_sha": "git-commit",
        "install.commit": "git-commit",
        "install.sha256": "wheel",
        "harness.commit": "git-commit",
        "evidence.screenshot_sha256": "rendered-frame",
        "evidence.twin_digest": "text-twin",
        "evidence.twin_sha256": "text-twin",
        "twin_digest": "text-twin",
        "twin_sha256": "text-twin",
        "evidence.pty_result_sha256": "step-payload",
        "evidence.files": "evidence-file",
        "files": "evidence-file",
        "supersedes.receipt_sha256": "receipt",
        "supersedes.candidate_commit_sha": "git-commit",
    },
    vocabularies={
        "schema_version": frozenset({
            V061_ITEM_SCHEMA,
            "talaria-v0.5.0-receipt-v1",
            "talaria-v0.6.0-receipt-v1",
        }),
        "release": frozenset({V061_RELEASE, "0.5.0", "0.6.0"}),
        "checklist_item": frozenset(f"live-{i:02d}" for i in range(1, 24)),
        "tester": frozenset(V061_ROLE_LABELS),
        "verdict": frozenset(VERDICTS),
        "install.kind": frozenset({"source-checkout", "wheel"}),
        "install.version_reported": frozenset({V061_RELEASE, "0.5.0", "0.6.0"}),
        "harness.kind": frozenset({"repository-tooling", "scratch-capture", "manual"}),
        "evidence.screenshots_read_by": frozenset(V061_ROLE_LABELS),
        "screenshots_read_by": frozenset(V061_ROLE_LABELS),
        "evidence.redaction_review": frozenset({"passed", "withheld", "pending"}),
        "redaction_review": frozenset({"passed", "withheld", "pending"}),
        "narrative.kind": frozenset({
            "prose",
            "manual",
            "reported-live-matrix",
            "step-sequence",
            "reported-live-dispatch",
        }),
    },
)

INSTALL_RECEIPT_SCHEMA = RecordSchema(
    name="install-receipt",
    declared_keys={
        "schema_version": ValueCategory.CLOSED_VOCABULARY,
        "tester": ValueCategory.CLOSED_VOCABULARY,
        "candidate": ValueCategory.OBJECT,
        "install": ValueCategory.OBJECT,
        "recorded_at": ValueCategory.TIMESTAMP,
    },
    nested_schemas={
        "candidate": {
            "commit": ValueCategory.DIGEST,
            "wheel_sha256": ValueCategory.DIGEST,
            "version": ValueCategory.CLOSED_VOCABULARY,
            "branch": ValueCategory.STRING,
        },
        "install": {
            "version_reported": ValueCategory.CLOSED_VOCABULARY,
            "help_ok": ValueCategory.BOOLEAN,
            "filename": ValueCategory.STRING,
            "sha256": ValueCategory.DIGEST,
            "url": ValueCategory.URL,
            "wheel_path": ValueCategory.PATH,
        },
    },
    digest_preimages={
        "candidate.commit": "git-commit",
        "candidate.wheel_sha256": "wheel",
        "install.sha256": "wheel",
    },
    vocabularies={
        "schema_version": frozenset({
            V050_INSTALL_SCHEMA,
            "talaria-v0.6.0-install-v1",
            V061_INSTALL_SCHEMA,
        }),
        "tester": frozenset(V061_ROLE_LABELS) | frozenset(TESTERS),
        "candidate.version": frozenset({V061_RELEASE, "0.6.0", "0.5.0"}),
        "install.version_reported": frozenset({V061_RELEASE, "0.6.0", "0.5.0"}),
    },
)

CAPTURE_METADATA_SCHEMA = RecordSchema(
    name="capture-metadata",
    declared_keys={
        "columns": ValueCategory.COUNT,
        "rows": ValueCategory.COUNT,
        "cell_width": ValueCategory.COUNT,
        "cell_height": ValueCategory.COUNT,
        "width": ValueCategory.COUNT,
        "height": ValueCategory.COUNT,
        "dpi": ValueCategory.COUNT,
        "scale": ValueCategory.COUNT,
        "frame": ValueCategory.FRAME_LABEL,
        "frame_digest": ValueCategory.DIGEST,
        "frame_digests": ValueCategory.DIGEST,
        "digests": ValueCategory.DIGEST,
        "sha256": ValueCategory.DIGEST,
        "twin_digest": ValueCategory.DIGEST,
        "twin_sha256": ValueCategory.DIGEST,
        "twin_path": ValueCategory.PATH,
        "recorded_at": ValueCategory.TIMESTAMP,
        "captured_at": ValueCategory.TIMESTAMP,
        "timestamp": ValueCategory.TIMESTAMP,
        "session_id": ValueCategory.GATEWAY_SESSION_ID,
        "format": ValueCategory.CLOSED_VOCABULARY,
        "schema_version": ValueCategory.CLOSED_VOCABULARY,
        "title": ValueCategory.STRING,
        "geometry": ValueCategory.OBJECT,
        "terminal": ValueCategory.OBJECT,
        "source_digest_sha256": ValueCategory.DIGEST,
        "source": ValueCategory.STRING,
        "view_id": ValueCategory.STRING,
        "capture_kind": ValueCategory.CLOSED_VOCABULARY,
        "candidate": ValueCategory.OBJECT,
        "case": ValueCategory.CLOSED_VOCABULARY,
        "record_type": ValueCategory.CLOSED_VOCABULARY,
        "schema": ValueCategory.CLOSED_VOCABULARY,
        "format_version": ValueCategory.CLOSED_VOCABULARY,
        "purpose": ValueCategory.CLOSED_VOCABULARY,
        "session": ValueCategory.OBJECT,
        "first_ansi_offset": ValueCategory.COUNT,
        "final_ansi_offset": ValueCategory.COUNT,
        "frame_sha256": ValueCategory.DIGEST,
        "first_frame_sha256": ValueCategory.DIGEST,
        "png_sha256": ValueCategory.DIGEST,
        "gateway": ValueCategory.URL,
        "event_log": ValueCategory.PATH,
        "tester": ValueCategory.CLOSED_VOCABULARY,
        "scope": ValueCategory.STRING,
        "settling": ValueCategory.OBJECT,
        "self_check": ValueCategory.OBJECT,
        "text_twin": ValueCategory.OBJECT,
        "diagnostics_cells": ValueCategory.LIST,
        "diagnostics_crop_error": ValueCategory.STRING,
        "redactions": ValueCategory.LIST,
    },
    nested_schemas={
        "geometry": {
            "columns": ValueCategory.COUNT,
            "rows": ValueCategory.COUNT,
            "cell_width": ValueCategory.COUNT,
            "cell_height": ValueCategory.COUNT,
            "width": ValueCategory.COUNT,
            "height": ValueCategory.COUNT,
            "dpi": ValueCategory.COUNT,
            "scale": ValueCategory.COUNT,
        },
        "terminal": {
            "columns": ValueCategory.COUNT,
            "rows": ValueCategory.COUNT,
            "cell_width": ValueCategory.COUNT,
            "cell_height": ValueCategory.COUNT,
        },
        "candidate": {
            "commit_sha": ValueCategory.DIGEST,
            "wheel_sha256": ValueCategory.DIGEST,
            "entry_point": ValueCategory.PATH,
            "source_module": ValueCategory.PATH,
            "binary_sha256": ValueCategory.DIGEST,
        },
        "session": {
            "durable_id": ValueCategory.GATEWAY_SESSION_ID,
            "runtime_id": ValueCategory.GATEWAY_SESSION_ID,
            "request_id": ValueCategory.STRING,
            "reply_seq": ValueCategory.COUNT,
            "profile": ValueCategory.CLOSED_VOCABULARY,
            "title": ValueCategory.STRING,
            "mode": ValueCategory.STRING,
        },
        "settling": {
            "quiet_seconds_per_window": ValueCategory.COUNT,
            "timeout_seconds": ValueCategory.COUNT,
            "windows": ValueCategory.COUNT,
        },
        "self_check": {
            "algorithm": ValueCategory.CLOSED_VOCABULARY,
            "expected_rejection": ValueCategory.STRING,
            "stable_control": ValueCategory.CLOSED_VOCABULARY,
            "status": ValueCategory.CLOSED_VOCABULARY,
        },
        "text_twin": {
            "file": ValueCategory.PATH,
            "path": ValueCategory.PATH,
            "sha256": ValueCategory.DIGEST,
            "twin_digest": ValueCategory.DIGEST,
            "digest": ValueCategory.DIGEST,
            "frame_sha256": ValueCategory.DIGEST,
            "frame_digest": ValueCategory.DIGEST,
        },
    },
    digest_preimages={
        "frame_digest": "rendered-frame",
        "frame_digests": "rendered-frame",
        "digests": "rendered-frame",
        "sha256": "source-capture",
        "twin_digest": "text-twin",
        "twin_sha256": "text-twin",
        "source_digest_sha256": "source-capture",
        "frame_sha256": "rendered-frame",
        "first_frame_sha256": "rendered-frame",
        "png_sha256": "source-capture",
        "candidate.commit_sha": "git-commit",
        "candidate.binary_sha256": "artifact",
        "candidate.wheel_sha256": "wheel",
        "commit_sha": "git-commit",
        "binary_sha256": "artifact",
        "wheel_sha256": "wheel",
        "text_twin.sha256": "text-twin",
        "text_twin.twin_digest": "text-twin",
        "text_twin.digest": "text-twin",
        "text_twin.frame_sha256": "rendered-frame",
        "text_twin.frame_digest": "rendered-frame",
    },
    vocabularies={
        "format": frozenset({"ansi", "text", "png", "json", "jsonl", "svg", "binary"}),
        "schema_version": frozenset({
            "talaria-v0.6.1-capture-v1",
            "talaria-capture-metadata-v1",
            "talaria-v0.6.0-capture-v1",
            "talaria-v0.5.0-capture-v1",
        }),
        "capture_kind": frozenset({
            "screenshot",
            "terminal-frame",
            "wire-slice",
            "pty-result",
            "probe",
            "trace",
        }),
        "case": frozenset(f"live-{i:02d}" for i in range(1, 24)) | frozenset({
            "probe-1",
            "probe-2",
            "matrix",
            "harness-self-check",
        }),
        "record_type": frozenset({
            "v061-item",
            "v060-item",
            "v050-item",
            "receipt",
            "capture-metadata",
            "pixel-measurements",
            "host-sentinel",
            "sentinel-witness",
            "host-sentinel-witness",
            "directory-equality-derivation",
        }),
        "schema": frozenset({
            "talaria-v0.6.1-capture-v1",
            "talaria-capture-metadata-v1",
            "talaria-v0.6.0-capture-v1",
            "talaria-v0.5.0-capture-v1",
        }),
        "format_version": frozenset({
            "talaria-live-capture-v2",
            "talaria-v0.6.1-capture-v1",
            "talaria-capture-metadata-v1",
            "talaria-v0.6.0-capture-v1",
            "talaria-v0.5.0-capture-v1",
        }),
        "purpose": frozenset({
            "acceptance",
            "workflow-demonstration",
            "visual-inspection",
            "regression-check",
            "timing-verification",
            "probe",
            "self-check",
        }),
        "tester": frozenset(V061_ROLE_LABELS),
        "session.profile": frozenset({"default", "talaria", "hermes", "minimal", "debug"}),
        "self_check.algorithm": frozenset({
            "sha256",
            "md5",
            "exact-bytes",
            "hash",
            "cell-frame-equality-v1",
            "cell-frame-equality",
        }),
        "self_check.stable_control": frozenset({
            "ok",
            "passed",
            "pass",
            "stable",
            "verified",
            "none",
        }),
        "self_check.status": frozenset({
            "passed",
            "pass",
            "failed",
            "refused",
            "skipped",
            "ok",
        }),
        "covered_class": REFUSED_CLASSES,
    },
)

PIXEL_MEASUREMENTS_SCHEMA = RecordSchema(
    name="pixel-measurements",
    declared_keys={
        "columns": ValueCategory.COUNT,
        "rows": ValueCategory.COUNT,
        "cell_width": ValueCategory.COUNT,
        "cell_height": ValueCategory.COUNT,
        "width": ValueCategory.COUNT,
        "height": ValueCategory.COUNT,
        "dpi": ValueCategory.COUNT,
        "scale": ValueCategory.COUNT,
        "x": ValueCategory.COUNT,
        "y": ValueCategory.COUNT,
        "recorded_at": ValueCategory.TIMESTAMP,
        "timestamp": ValueCategory.TIMESTAMP,
        "schema_version": ValueCategory.CLOSED_VOCABULARY,
        "measurements": ValueCategory.OBJECT,
        "geometry": ValueCategory.OBJECT,
        "regions": ValueCategory.LIST,
        "sha256": ValueCategory.DIGEST,
        "title": ValueCategory.STRING,
        "box": ValueCategory.OBJECT,
        "bounds": ValueCategory.OBJECT,
    },
    digest_preimages={
        "sha256": "rendered-frame",
    },
    vocabularies={
        "schema_version": frozenset({
            "talaria-v0.6.1-measurements-v1",
            "talaria-pixel-measurements-v1",
            "talaria-v0.6.0-measurements-v1",
        }),
    },
)

STEP_LOG_SCHEMA = RecordSchema(
    name="step-log",
    declared_keys={
        "step": ValueCategory.COUNT,
        "seq": ValueCategory.COUNT,
        "sourceSeq": ValueCategory.COUNT,
        "timestamp": ValueCategory.TIMESTAMP,
        "recorded_at": ValueCategory.TIMESTAMP,
        "action": ValueCategory.STRING,
        "status": ValueCategory.STRING,
        "kind": ValueCategory.STRING,
        "elapsed_ms": ValueCategory.COUNT,
        "duration_ms": ValueCategory.COUNT,
        "result": ValueCategory.STRING,
        "details": ValueCategory.OBJECT,
        "digest": ValueCategory.DIGEST,
        "sha256": ValueCategory.DIGEST,
        "error": ValueCategory.STRING,
        "command": ValueCategory.STRING,
        "exit_code": ValueCategory.COUNT,
        "env": ValueCategory.OBJECT,
    },
    digest_preimages={
        "digest": "step-payload",
        "sha256": "step-payload",
    },
)

ATTESTATION_ITEM_SCHEMA = RecordSchema(
    name="attestation",
    declared_keys={
        "tester": ValueCategory.CLOSED_VOCABULARY,
        "capturing_role": ValueCategory.CLOSED_VOCABULARY,
        "attested_at": ValueCategory.TIMESTAMP,
        "install_kind": ValueCategory.CLOSED_VOCABULARY,
        "harness_kind": ValueCategory.CLOSED_VOCABULARY,
        "harness_commit": ValueCategory.DIGEST,
        "harness_identity": ValueCategory.HARNESS_IDENTITY,
        "expected": ValueCategory.STRING,
        "gateway": ValueCategory.OBJECT,
        "session": ValueCategory.OBJECT,
        "terminal": ValueCategory.OBJECT,
        "screenshots_read_by": ValueCategory.CLOSED_VOCABULARY,
        "screenshots_read_at": ValueCategory.TIMESTAMP,
        "notes": ValueCategory.STRING,
        "checklist_item": ValueCategory.CLOSED_VOCABULARY,
        "commit": ValueCategory.DIGEST,
        "read_confirmations": ValueCategory.LIST,
        "read_confirmation": ValueCategory.OBJECT,
        "redaction_review": ValueCategory.CLOSED_VOCABULARY,
    },
    nullable_keys=frozenset({"harness_commit", "harness_identity"}),
    digest_preimages={
        "harness_commit": "git-commit",
        "commit": "git-commit",
    },
    vocabularies={
        "tester": frozenset(V061_ROLE_LABELS),
        "capturing_role": frozenset(V061_ROLE_LABELS),
        "install_kind": frozenset({"source-checkout", "wheel"}),
        "harness_kind": frozenset({"repository-tooling", "scratch-capture", "manual"}),
        "screenshots_read_by": frozenset(V061_ROLE_LABELS),
        "checklist_item": frozenset(f"live-{i:02d}" for i in range(1, 24)),
        "redaction_review": frozenset({"passed", "withheld", "pending"}),
    },
)


class AttestationMapSchema:
    name = "attestation-map"

    def validate(self, doc: dict[str, Any], *, path: Path, prefix: str = "") -> list[str]:
        errors: list[str] = []
        for k, v in doc.items():
            loc = f"{prefix}.{k}" if prefix else str(k)
            if is_forbidden_key(k):
                errors.append(f"{path}: contains forbidden key {k!r} at {loc}")
                continue
            if re.match(r"^live-\d+$", k):
                if isinstance(v, dict):
                    errors.extend(ATTESTATION_ITEM_SCHEMA.validate(v, path=path, prefix=loc))
                else:
                    errors.append(f"{path}: attestation item {k!r} must be an object")
            elif k in ATTESTATION_ITEM_SCHEMA.declared_keys:
                return ATTESTATION_ITEM_SCHEMA.validate(doc, path=path, prefix=prefix)
            else:
                errors.append(
                    f"{path}: attestation-map contains undeclared key {k!r} at {loc}"
                )
        return errors


ATTESTATION_MAP_SCHEMA = AttestationMapSchema()

SENTINEL_ALLOWED_TOKENS: frozenset[str] = frozenset({
    "sentinel-idle",
    "sentinel-fired",
    "idle",
    "fired",
})
SENTINEL_IDLE_TOKENS: frozenset[str] = frozenset({"sentinel-idle", "idle"})
SENTINEL_FIRED_TOKENS: frozenset[str] = frozenset({"sentinel-fired", "fired"})

HOST_SENTINEL_ITEM_SCHEMA = RecordSchema(
    name="host-sentinel-witness",
    declared_keys={
        "key": ValueCategory.STRING,
        "consumed_before": ValueCategory.STRING,
        "consumed_after": ValueCategory.STRING,
        "control_before": ValueCategory.STRING,
        "control_after": ValueCategory.STRING,
    },
)

HOST_SENTINEL_RECORD_SCHEMA = RecordSchema(
    name="host-sentinel",
    declared_keys={
        "schema_version": ValueCategory.CLOSED_VOCABULARY,
        "format_version": ValueCategory.CLOSED_VOCABULARY,
        "record_type": ValueCategory.CLOSED_VOCABULARY,
        "checklist_item": ValueCategory.CLOSED_VOCABULARY,
        "case": ValueCategory.CLOSED_VOCABULARY,
        "candidate_commit": ValueCategory.STRING,
        "read_by": ValueCategory.CLOSED_VOCABULARY,
        "read_at": ValueCategory.TIMESTAMP,
        "keys": ValueCategory.LIST,
        "positive_controls_confirmed": ValueCategory.BOOLEAN,
        "consumed_keys_stayed_idle": ValueCategory.BOOLEAN,
    },
    vocabularies={
        "schema_version": frozenset({
            "talaria-v0.6.1-sentinel-v1",
            "talaria-sentinel-v1",
            "talaria-host-sentinel-v1",
        }),
        "format_version": frozenset({
            "talaria-v0.6.1-sentinel-v1",
            "talaria-sentinel-v1",
            "talaria-host-sentinel-v1",
        }),
        "record_type": frozenset({"host-sentinel", "sentinel-witness", "host-sentinel-witness"}),
        "checklist_item": frozenset(f"live-{i:02d}" for i in range(1, 24)),
        "case": frozenset(f"live-{i:02d}" for i in range(1, 24)),
        "read_by": frozenset(V061_ROLE_LABELS),
    },
)


def validate_host_sentinel_record(
    doc: dict[str, Any], *, path: Path, prefix: str = ""
) -> list[str]:
    """Validate host-sentinel evidence against the content-free non-leak contract."""
    errors: list[str] = []
    errors.extend(HOST_SENTINEL_RECORD_SCHEMA.validate(doc, path=path, prefix=prefix))

    keys_list = doc.get("keys")
    if keys_list is None:
        errors.append(f"{path}: host-sentinel record requires 'keys' (quartet observations)")
    elif not isinstance(keys_list, list):
        errors.append(f"{path}: 'keys' must be a list")
    elif not keys_list:
        errors.append(f"{path}: 'keys' list must not be empty")

    cand = doc.get("candidate_commit")
    if cand is None:
        errors.append(f"{path}: host-sentinel record requires 'candidate_commit'")
    elif not isinstance(cand, str) or not re.fullmatch(r"[0-9a-f]{40}", cand):
        errors.append(
            f"{path}: 'candidate_commit' must be a 40-character hexadecimal git commit SHA "
            f"(got {cand!r})"
        )

    if doc.get("positive_controls_confirmed") is not True:
        errors.append(f"{path}: 'positive_controls_confirmed' must be explicitly true")
    if doc.get("consumed_keys_stayed_idle") is not True:
        errors.append(f"{path}: 'consumed_keys_stayed_idle' must be explicitly true")

    doc_read_by = doc.get("read_by")
    doc_read_at = doc.get("read_at")
    read_dt: dt.datetime | None = None
    if not doc_read_by:
        errors.append(f"{path}: missing recorded read (read_by is required)")
    elif doc_read_by not in V061_ROLE_LABELS:
        errors.append(f"{path}: read_by {doc_read_by!r} not in role labels")
    if not doc_read_at:
        errors.append(f"{path}: missing recorded read (read_at is required)")
    else:
        try:
            parsed_read = dt.datetime.fromisoformat(str(doc_read_at).replace("Z", "+00:00"))
            if parsed_read.utcoffset() is None:
                errors.append(f"{path}: read_at must have timezone qualification")
            else:
                read_dt = parsed_read
        except (ValueError, TypeError):
            errors.append(f"{path}: read_at must be an ISO 8601 timestamp")

    if not isinstance(keys_list, list) or not keys_list:
        return errors

    root = path.parent
    seen_observations: set[str] = set()
    found_keys: set[str] = set()
    prev_captured_dt: dt.datetime | None = None
    quartet_phases = {
        "consumed_before": "sentinel-idle",
        "consumed_after": "sentinel-idle",
        "control_before": "sentinel-idle",
        "control_after": "sentinel-fired",
    }

    for idx, item in enumerate(keys_list):
        loc = f"keys[{idx}]"
        if not isinstance(item, dict):
            errors.append(f"{path}: {loc} must be an object")
            continue
        errors.extend(HOST_SENTINEL_ITEM_SCHEMA.validate(item, path=path, prefix=loc))
        k = item.get("key")
        if not k:
            errors.append(f"{path}: {loc} missing mandatory field 'key'")
        else:
            found_keys.add(k.lower())

        for phase, expected_token in quartet_phases.items():
            obs = item.get(phase)
            if not obs or not isinstance(obs, str):
                errors.append(
                    f"{path}: {loc}: incomplete quartet: missing observation '{phase}'"
                )
                continue
            if obs in seen_observations:
                errors.append(
                    f"{path}: {loc}: reused observation reference {obs!r} "
                    "across phases or keys"
                )
            seen_observations.add(obs)

            # Cross-file checks against sibling capture artifacts
            png_file = root / obs
            if not png_file.is_file():
                errors.append(f"{path}: referenced frame {obs!r} does not exist")
                continue

            # 1. Validate PNG chunk and embedded metadata
            png_data = png_file.read_bytes()
            embedded: dict[str, Any] | None = None
            if not png_data.startswith(b"\x89PNG\r\n\x1a\n"):
                errors.append(f"{path}: {obs!r} is not a valid PNG file")
            else:
                embedded = _extract_png_capture_metadata(png_data)
                if embedded is None:
                    errors.append(f"{path}: {obs!r} missing talaria-evidence chunk in PNG")
                else:
                    if embedded.get("record_type") != "capture-metadata":
                        errors.append(
                            f"{path}: {obs!r} PNG chunk record_type is "
                            f"{embedded.get('record_type')!r}, expected 'capture-metadata'"
                        )
                    if embedded.get("format_version") != "talaria-live-capture-v2":
                        errors.append(
                            f"{path}: {obs!r} PNG chunk format_version is "
                            f"{embedded.get('format_version')!r}, "
                            "expected 'talaria-live-capture-v2'"
                        )
                    if embedded.get("case") != "live-22":
                        errors.append(
                            f"{path}: {obs!r} PNG chunk case is "
                            f"{embedded.get('case')!r}, expected 'live-22'"
                        )
                    if cand and embedded.get("candidate", {}).get("commit_sha") != cand:
                        errors.append(
                            f"{path}: {obs!r} PNG chunk candidate commit "
                            f"{embedded.get('candidate', {}).get('commit_sha')!r} "
                            f"does not match witness candidate {cand!r}"
                        )
                    if embedded.get("redactions"):
                        errors.append(
                            f"{path}: {obs!r} PNG chunk contains redactions "
                            "on content-free host surface"
                        )

            # 2. Text twin validation
            txt_file = png_file.with_suffix(".txt")
            if not txt_file.is_file():
                errors.append(
                    f"{path}: referenced frame {obs!r} missing text twin {txt_file.name!r}"
                )
            else:
                txt_bytes = txt_file.read_bytes()
                actual_token = txt_bytes.decode("utf-8", errors="replace").strip()
                if actual_token != expected_token:
                    errors.append(
                        f"{path}: {loc}.{phase}: text twin {txt_file.name!r} contains "
                        f"{actual_token!r}, expected exact sentinel token "
                        f"{expected_token!r}"
                    )
                twin_sha = hashlib.sha256(txt_bytes).hexdigest()
                if embedded is not None:
                    chunk_twin = (
                        embedded.get("twin_digest")
                        or embedded.get("text_twin", {}).get("sha256")
                    )
                    if chunk_twin and chunk_twin != twin_sha:
                        errors.append(
                            f"{path}: {obs!r} PNG chunk twin_digest ({chunk_twin}) "
                            f"does not match text twin digest ({twin_sha})"
                        )

            # 3. Capture metadata JSON sidecar
            json_file = png_file.with_suffix(".json")
            if not json_file.is_file():
                errors.append(
                    f"{path}: referenced frame {obs!r} missing capture metadata "
                    f"{json_file.name!r}"
                )
            else:
                sidecar: dict[str, Any] | None = None
                try:
                    parsed_json = json.loads(json_file.read_text(encoding="utf-8"))
                    if isinstance(parsed_json, dict):
                        sidecar = parsed_json
                except Exception as exc:
                    errors.append(f"{path}: {json_file.name!r} is not valid JSON: {exc}")

                if sidecar is not None:
                    if sidecar.get("record_type") != "capture-metadata":
                        errors.append(
                            f"{path}: {json_file.name!r} record_type is "
                            f"{sidecar.get('record_type')!r}, expected 'capture-metadata'"
                        )
                    if sidecar.get("format_version") != "talaria-live-capture-v2":
                        errors.append(
                            f"{path}: {json_file.name!r} format_version is "
                            f"{sidecar.get('format_version')!r}, "
                            "expected 'talaria-live-capture-v2'"
                        )
                    if sidecar.get("case") != "live-22":
                        errors.append(
                            f"{path}: {json_file.name!r} case is "
                            f"{sidecar.get('case')!r}, expected 'live-22'"
                        )
                    if cand and sidecar.get("candidate", {}).get("commit_sha") != cand:
                        errors.append(
                            f"{path}: {json_file.name!r} candidate commit "
                            f"{sidecar.get('candidate', {}).get('commit_sha')!r} "
                            f"does not match witness candidate {cand!r}"
                        )
                    if sidecar.get("frame") != png_file.stem:
                        errors.append(
                            f"{path}: {json_file.name!r} frame is "
                            f"{sidecar.get('frame')!r}, expected {png_file.stem!r}"
                        )
                    if sidecar.get("redactions"):
                        errors.append(
                            f"{path}: {json_file.name!r} contains redactions "
                            "on content-free host surface"
                        )

                    if embedded is not None:
                        sidecar_cmp = dict(sidecar)
                        sidecar_cmp.pop("png_sha256", None)
                        embedded_cmp = dict(embedded)
                        embedded_cmp.pop("png_sha256", None)
                        if sidecar_cmp != embedded_cmp:
                            errors.append(
                                f"{path}: {obs!r} sidecar metadata and "
                                "embedded PNG chunk disagree"
                            )

                    cap_raw = sidecar.get("captured_at")
                    if not cap_raw:
                        errors.append(f"{path}: {json_file.name!r} missing 'captured_at'")
                    else:
                        try:
                            cap_dt = dt.datetime.fromisoformat(
                                str(cap_raw).replace("Z", "+00:00")
                            )
                            if cap_dt.utcoffset() is None:
                                errors.append(
                                    f"{path}: {json_file.name!r} captured_at "
                                    "must have timezone qualification"
                                )
                            else:
                                if read_dt is not None and cap_dt > read_dt:
                                    errors.append(
                                        f"{path}: {json_file.name!r} captured_at "
                                        f"({cap_raw}) is after read_at ({doc_read_at})"
                                    )
                                if (
                                    prev_captured_dt is not None
                                    and cap_dt <= prev_captured_dt
                                ):
                                    errors.append(
                                        f"{path}: {json_file.name!r} observations are not "
                                        f"chronologically ordered ({cap_raw} <= previous)"
                                    )
                                prev_captured_dt = cap_dt
                        except (ValueError, TypeError):
                            errors.append(
                                f"{path}: {json_file.name!r} captured_at "
                                "must be an ISO 8601 timestamp"
                            )

    if (
        doc.get("checklist_item") == "live-22"
        or doc.get("case") == "live-22"
        or "live-22" in str(path)
    ):
        required_keys = {"ctrl+o", "ctrl+s", "f1", "f2"}
        missing_keys = required_keys - found_keys
        if missing_keys:
            errors.append(
                f"{path}: Live 22 host-sentinel evidence requires all 4 default keys "
                f"{sorted(required_keys)}, missing: {sorted(missing_keys)}"
            )

    return errors


class HostSentinelSchema:
    name = "host-sentinel"
    declared_keys = HOST_SENTINEL_RECORD_SCHEMA.declared_keys
    vocabularies = HOST_SENTINEL_RECORD_SCHEMA.vocabularies

    def validate(self, doc: dict[str, Any], *, path: Path, prefix: str = "") -> list[str]:
        return validate_host_sentinel_record(doc, path=path, prefix=prefix)


HOST_SENTINEL_SCHEMA = HostSentinelSchema()


DIRECTORY_EQUALITY_STATUS_VOCABULARY: frozenset[str] = frozenset({
    "pass",
    "failed",
    "blocked",
    "qualified",
})

DIRECTORY_EQUALITY_MANDATORY_SOURCES: tuple[str, ...] = (
    "session-a-init",
    "session-b-init",
    "session-a-fresh",
    "session-a-resume",
)

DIRECTORY_EQUALITY_MANDATORY_STAGES: tuple[str, ...] = (
    "project-a-adoption",
    "project-a-tool",
    "project-b-adoption",
    "project-b-tool",
    "a-tool-override",
    "b-after-a-override",
    "a-after-reconnect",
    "fresh-a",
    "resumed-a",
    "resumed-a-tool",
)

DIRECTORY_EQUALITY_ADOPTION_STAGES: frozenset[str] = frozenset({
    "project-a-adoption",
    "project-b-adoption",
    "fresh-a",
    "resumed-a",
})

DIRECTORY_EQUALITY_TOOL_STAGES: frozenset[str] = frozenset({
    "project-a-tool",
    "project-b-tool",
    "a-tool-override",
    "b-after-a-override",
    "a-after-reconnect",
    "resumed-a-tool",
})

DIRECTORY_EQUALITY_DERIVATION_RECORD_SCHEMA = RecordSchema(
    name="directory-equality-derivation",
    declared_keys={
        "record_type": ValueCategory.CLOSED_VOCABULARY,
        "format_version": ValueCategory.CLOSED_VOCABULARY,
        "case": ValueCategory.CLOSED_VOCABULARY,
        "checklist_item": ValueCategory.CLOSED_VOCABULARY,
        "candidate_commit": ValueCategory.STRING,
        "derived_by": ValueCategory.CLOSED_VOCABULARY,
        "derived_at": ValueCategory.TIMESTAMP,
        "status": ValueCategory.CLOSED_VOCABULARY,
        "permission_semantics": ValueCategory.STRING,
        "sources": ValueCategory.OBJECT,
        "observations": ValueCategory.OBJECT,
    },
    vocabularies={
        "record_type": frozenset({"directory-equality-derivation"}),
        "format_version": frozenset({"talaria-directory-equality-v1"}),
        "case": frozenset(f"live-{i:02d}" for i in range(1, 24)),
        "checklist_item": frozenset(f"live-{i:02d}" for i in range(1, 24)),
        "status": DIRECTORY_EQUALITY_STATUS_VOCABULARY,
        "derived_by": frozenset(V061_ROLE_LABELS),
    },
)

DIRECTORY_EQUALITY_SOURCE_ITEM_SCHEMA = RecordSchema(
    name="directory-equality-source",
    declared_keys={
        "source_file": ValueCategory.STRING,
        "source_sha256": ValueCategory.DIGEST,
        "derived_file": ValueCategory.STRING,
        "derived_sha256": ValueCategory.DIGEST,
    },
    digest_preimages={
        "source_sha256": "source-capture",
        "derived_sha256": "evidence-file",
    },
)

DIRECTORY_EQUALITY_ADOPTION_OBSERVATION_SCHEMA = RecordSchema(
    name="directory-equality-adoption-observation",
    declared_keys={
        "source_id": ValueCategory.STRING,
        "request_seq": ValueCategory.COUNT,
        "reply_seq": ValueCategory.COUNT,
        "request_source_seq": ValueCategory.COUNT,
        "reply_source_seq": ValueCategory.COUNT,
        "requested_equals_launch": ValueCategory.BOOLEAN,
        "reported_equals_expected": ValueCategory.BOOLEAN,
    },
)

DIRECTORY_EQUALITY_TOOL_OBSERVATION_SCHEMA = RecordSchema(
    name="directory-equality-tool-observation",
    declared_keys={
        "source_id": ValueCategory.STRING,
        "tool_start_seq": ValueCategory.COUNT,
        "tool_complete_seq": ValueCategory.COUNT,
        "pwd_equals_expected": ValueCategory.BOOLEAN,
        "fixture_content_matches": ValueCategory.BOOLEAN,
        "override_supplied": ValueCategory.BOOLEAN,
    },
)

DIRECTORY_EQUALITY_INVARIANT_OBSERVATION_SCHEMA = RecordSchema(
    name="directory-equality-invariant-observation",
    declared_keys={
        "source_id": ValueCategory.STRING,
        "tool_start_seq": ValueCategory.COUNT,
        "tool_complete_seq": ValueCategory.COUNT,
        "pwd_equals_expected": ValueCategory.BOOLEAN,
        "fixture_content_matches": ValueCategory.BOOLEAN,
        "override_supplied": ValueCategory.BOOLEAN,
        "bounded_absence_session_cwd_set": ValueCategory.BOOLEAN,
        "b_reported_cwd_unchanged": ValueCategory.BOOLEAN,
    },
)

DIRECTORY_EQUALITY_STAGE_SOURCES: dict[str, str] = {
    "project-a-adoption": "session-a-init",
    "project-a-tool": "session-a-init",
    "project-b-adoption": "session-b-init",
    "project-b-tool": "session-b-init",
    "a-tool-override": "session-a-init",
    "b-after-a-override": "session-b-init",
    "a-after-reconnect": "session-a-init",
    "fresh-a": "session-a-fresh",
    "resumed-a": "session-a-resume",
    "resumed-a-tool": "session-a-resume",
}


def _commit_resolves(commit: str, *, repo_root: Path) -> bool:
    """Whether ``commit`` names an object that exists in this repository."""
    result = subprocess.run(
        ["git", "cat-file", "-e", f"{commit}^{{commit}}"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode == 0


def _find_repo_for_path(path: Path) -> Path | None:
    """Find the root of the Git repository containing path, if any."""
    try:
        cur = path.resolve()
        if cur.is_file():
            cur = cur.parent
        for p in [cur] + list(cur.parents):
            if (p / ".git").exists():
                return p
    except Exception:
        pass
    return None


def validate_directory_equality_derivation(
    doc: dict[str, Any],
    *,
    path: Path,
    prefix: str = "",
    expected_commit: str | None = None,
    repo_root: Path = _REPO_ROOT,
) -> list[str]:
    """Validate Live 13 directory-equality derivation against privacy and completeness contracts."""
    errors: list[str] = []
    errors.extend(
        DIRECTORY_EQUALITY_DERIVATION_RECORD_SCHEMA.validate(doc, path=path, prefix=prefix)
    )

    active_repo = repo_root
    if active_repo == _REPO_ROOT:
        discovered = _find_repo_for_path(path)
        if discovered is not None:
            active_repo = discovered

    # 0. Identification and versioning
    if doc.get("record_type") != "directory-equality-derivation":
        errors.append(f"{path}: 'record_type' must be 'directory-equality-derivation'")
    if doc.get("format_version") != "talaria-directory-equality-v1":
        errors.append(f"{path}: 'format_version' must be 'talaria-directory-equality-v1'")
    if not doc.get("case"):
        errors.append(f"{path}: missing mandatory field 'case'")
    if not doc.get("checklist_item"):
        errors.append(f"{path}: missing mandatory field 'checklist_item'")
    if (
        doc.get("case")
        and doc.get("checklist_item")
        and doc.get("case") != doc.get("checklist_item")
    ):
        errors.append(
            f"{path}: 'case' ({doc.get('case')}) does not match "
            f"'checklist_item' ({doc.get('checklist_item')})"
        )
    if "status" not in doc:
        errors.append(f"{path}: missing mandatory field 'status'")
    elif doc.get("status") not in DIRECTORY_EQUALITY_STATUS_VOCABULARY:
        errors.append(f"{path}: status {doc.get('status')!r} not in status vocabulary")

    # 1. Candidate commit format and resolution
    cand = doc.get("candidate_commit")
    if cand is None:
        errors.append(f"{path}: directory-equality derivation requires 'candidate_commit'")
    elif not isinstance(cand, str) or not re.fullmatch(r"[0-9a-f]{40}", cand):
        errors.append(
            f"{path}: 'candidate_commit' must be a 40-character hexadecimal git commit SHA "
            f"(got {cand!r})"
        )
    else:
        if expected_commit is None:
            sibling_receipt = path.parent / "receipt.json"
            if sibling_receipt.is_file():
                try:
                    receipt_obj = json.loads(sibling_receipt.read_text(encoding="utf-8"))
                    if isinstance(receipt_obj, dict):
                        cand_sha = receipt_obj.get("candidate_commit_sha")
                        if isinstance(cand_sha, str) and cand_sha:
                            expected_commit = cand_sha
                        else:
                            errors.append(
                                f"{path}: sibling receipt.json missing 'candidate_commit_sha'"
                            )
                    else:
                        errors.append(
                            f"{path}: sibling receipt.json is not a valid JSON object"
                        )
                except Exception as exc:
                    errors.append(f"{path}: sibling receipt.json is malformed: {exc}")
            else:
                errors.append(
                    f"{path}: missing sibling receipt.json for candidate_commit verification"
                )

        if expected_commit is not None and cand != expected_commit:
            errors.append(
                f"{path}: candidate_commit ({cand}) does not match "
                f"receipt candidate_commit_sha ({expected_commit})"
            )

        if not _commit_resolves(cand, repo_root=active_repo):
            errors.append(
                f"{path}: candidate_commit {cand[:12]} does not resolve in this repository"
            )

    # 2. Derived by & derived at
    derived_by = doc.get("derived_by")
    if not derived_by:
        errors.append(f"{path}: missing recorded derivation role ('derived_by' is required)")
    elif derived_by not in V061_ROLE_LABELS:
        errors.append(f"{path}: derived_by {derived_by!r} not in role labels")

    derived_at = doc.get("derived_at")
    if not derived_at:
        errors.append(f"{path}: missing recorded derivation timestamp ('derived_at' is required)")
    else:
        try:
            parsed_dt = dt.datetime.fromisoformat(str(derived_at).replace("Z", "+00:00"))
            if parsed_dt.utcoffset() is None:
                errors.append(f"{path}: derived_at must have timezone qualification")
        except (ValueError, TypeError):
            errors.append(f"{path}: derived_at must be an ISO 8601 timestamp")

    # 3. Permission semantics
    perm = doc.get("permission_semantics")
    if perm is None:
        errors.append(f"{path}: missing mandatory field 'permission_semantics'")
    elif not isinstance(perm, str) or not perm.strip():
        errors.append(f"{path}: 'permission_semantics' must be a non-empty string")

    # 4. Strict privacy check: no absolute filesystem paths in the record
    try:
        doc_serialized = json.dumps(doc)
        for p in find_absolute_paths_in_text(doc_serialized):
            errors.append(f"{path}: directory-equality derivation discloses absolute path {p!r}")
    except Exception:
        pass

    # 5. Sources validation
    sources = doc.get("sources")
    sources_dict: dict[str, Any] = {}
    if sources is None:
        errors.append(f"{path}: directory-equality derivation requires 'sources'")
    elif not isinstance(sources, dict):
        errors.append(f"{path}: 'sources' must be an object")
    else:
        sources_dict = sources
        missing_sources = set(DIRECTORY_EQUALITY_MANDATORY_SOURCES) - set(sources.keys())
        if missing_sources:
            errors.append(
                f"{path}: Live 13 derivation requires all 4 mandatory sources "
                f"{sorted(DIRECTORY_EQUALITY_MANDATORY_SOURCES)}, "
                f"missing: {sorted(missing_sources)}"
            )
        unexpected_sources = set(sources.keys()) - set(DIRECTORY_EQUALITY_MANDATORY_SOURCES)
        if unexpected_sources:
            errors.append(
                f"{path}: Live 13 derivation contains undeclared sources "
                f"{sorted(unexpected_sources)}"
            )

        root = path.parent
        for s_id, s_entry in sources.items():
            s_loc = f"sources.{s_id}"
            if not isinstance(s_entry, dict):
                errors.append(f"{path}: {s_loc} must be an object")
                continue
            errors.extend(
                DIRECTORY_EQUALITY_SOURCE_ITEM_SCHEMA.validate(s_entry, path=path, prefix=s_loc)
            )

            for f_key in ("source_file", "derived_file"):
                f_val = s_entry.get(f_key)
                if not f_val or not isinstance(f_val, str):
                    errors.append(f"{path}: {s_loc} missing mandatory field {f_key!r}")
                elif Path(f_val).is_absolute() or f_val.startswith("/"):
                    errors.append(
                        f"{path}: {s_loc}.{f_key} must be a relative filename (got {f_val!r})"
                    )

            for h_key in ("source_sha256", "derived_sha256"):
                h_val = s_entry.get(h_key)
                if not h_val or not isinstance(h_val, str):
                    errors.append(f"{path}: {s_loc} missing mandatory field {h_key!r}")
                elif not re.fullmatch(r"[0-9a-f]{64}", h_val):
                    errors.append(
                        f"{path}: {s_loc}.{h_key} must be a 64-character hexadecimal "
                        f"SHA-256 digest (got {h_val!r})"
                    )

            derived_name = s_entry.get("derived_file")
            if derived_name and isinstance(derived_name, str):
                derived_path = root / derived_name
                if not derived_path.is_file():
                    errors.append(
                        f"{path}: referenced derived wire capture {derived_name!r} does not exist"
                    )
                else:
                    derived_bytes = derived_path.read_bytes()
                    actual_sha = hashlib.sha256(derived_bytes).hexdigest()
                    expected_sha = s_entry.get("derived_sha256")
                    if expected_sha and actual_sha != expected_sha:
                        errors.append(
                            f"{path}: {derived_name!r} SHA-256 digest ({actual_sha}) "
                            f"does not match declared derived_sha256 ({expected_sha})"
                        )
                    wire_errs = _wire_capture_errors(derived_path, derived_bytes)
                    if wire_errs:
                        errors.extend(wire_errs)

    # 6. Observations validation
    observations = doc.get("observations")
    if observations is None:
        errors.append(f"{path}: directory-equality derivation requires 'observations'")
        return errors

    if not isinstance(observations, dict):
        errors.append(f"{path}: 'observations' must be an object")
        return errors

    missing_stages = set(DIRECTORY_EQUALITY_MANDATORY_STAGES) - set(observations.keys())
    if missing_stages:
        errors.append(
            f"{path}: Live 13 derivation requires all 10 mandatory scenario stages "
            f"{sorted(DIRECTORY_EQUALITY_MANDATORY_STAGES)}, missing: {sorted(missing_stages)}"
        )
    unexpected_stages = set(observations.keys()) - set(DIRECTORY_EQUALITY_MANDATORY_STAGES)
    if unexpected_stages:
        errors.append(
            f"{path}: Live 13 derivation contains undeclared scenario stages "
            f"{sorted(unexpected_stages)}"
        )

    all_predicates_passed = True

    for stage_name, obs in observations.items():
        loc = f"observations.{stage_name}"
        if not isinstance(obs, dict):
            errors.append(f"{path}: {loc} must be an object")
            continue

        if stage_name in DIRECTORY_EQUALITY_ADOPTION_STAGES:
            errors.extend(
                DIRECTORY_EQUALITY_ADOPTION_OBSERVATION_SCHEMA.validate(
                    obs, path=path, prefix=loc
                )
            )
        elif stage_name in DIRECTORY_EQUALITY_TOOL_STAGES:
            if stage_name == "b-after-a-override":
                errors.extend(
                    DIRECTORY_EQUALITY_INVARIANT_OBSERVATION_SCHEMA.validate(
                        obs, path=path, prefix=loc
                    )
                )
            else:
                errors.extend(
                    DIRECTORY_EQUALITY_TOOL_OBSERVATION_SCHEMA.validate(
                        obs, path=path, prefix=loc
                    )
                )
        else:
            errors.append(f"{path}: undeclared scenario stage {stage_name!r} at {loc}")
            continue

        s_id = obs.get("source_id")
        if not s_id:
            errors.append(f"{path}: {loc} missing mandatory field 'source_id'")
        elif s_id not in sources_dict:
            errors.append(f"{path}: {loc}.source_id {s_id!r} not declared in 'sources'")
        else:
            expected_source = DIRECTORY_EQUALITY_STAGE_SOURCES.get(stage_name)
            if expected_source and s_id != expected_source:
                errors.append(
                    f"{path}: {loc}.source_id {s_id!r} does not match "
                    f"expected source {expected_source!r} for stage {stage_name!r}"
                )

        if stage_name in DIRECTORY_EQUALITY_ADOPTION_STAGES:
            for seq_key in ("request_seq", "reply_seq", "request_source_seq", "reply_source_seq"):
                val = obs.get(seq_key)
                if val is None:
                    errors.append(f"{path}: {loc} missing mandatory sequence field {seq_key!r}")
                elif not isinstance(val, int) or val < 1:
                    errors.append(
                        f"{path}: {loc}.{seq_key} must be a positive integer (got {val!r})"
                    )

            req_seq = obs.get("request_seq")
            rep_seq = obs.get("reply_seq")
            if isinstance(req_seq, int) and isinstance(rep_seq, int) and rep_seq < req_seq:
                errors.append(
                    f"{path}: {loc}: reply_seq ({rep_seq}) cannot precede request_seq ({req_seq})"
                )

            has_launch = "requested_equals_launch" in obs
            has_reported = "reported_equals_expected" in obs
            if has_launch and not has_reported:
                errors.append(
                    f"{path}: {loc}: incomplete comparison: 'requested_equals_launch' present "
                    "without 'reported_equals_expected'"
                )
            elif has_reported and not has_launch:
                errors.append(
                    f"{path}: {loc}: incomplete comparison: 'reported_equals_expected' present "
                    "without 'requested_equals_launch'"
                )
            elif not has_launch and not has_reported:
                errors.append(
                    f"{path}: {loc}: incomplete comparison: missing adoption equality predicates"
                )
            else:
                for b_key in ("requested_equals_launch", "reported_equals_expected"):
                    b_val = obs.get(b_key)
                    if not isinstance(b_val, bool):
                        errors.append(f"{path}: {loc}.{b_key} must be a boolean (got {b_val!r})")
                    elif b_val is not True:
                        if stage_name == "resumed-a" and b_key == "requested_equals_launch":
                            continue
                        all_predicates_passed = False

        elif stage_name in DIRECTORY_EQUALITY_TOOL_STAGES:
            for seq_key in ("tool_start_seq", "tool_complete_seq"):
                val = obs.get(seq_key)
                if val is None:
                    errors.append(f"{path}: {loc} missing mandatory sequence field {seq_key!r}")
                elif not isinstance(val, int) or val < 1:
                    errors.append(
                        f"{path}: {loc}.{seq_key} must be a positive integer (got {val!r})"
                    )

            start_seq = obs.get("tool_start_seq")
            comp_seq = obs.get("tool_complete_seq")
            if isinstance(start_seq, int) and isinstance(comp_seq, int) and comp_seq < start_seq:
                errors.append(
                    f"{path}: {loc}: tool_complete_seq ({comp_seq}) cannot precede "
                    f"tool_start_seq ({start_seq})"
                )

            has_pwd = "pwd_equals_expected" in obs
            has_fixture = "fixture_content_matches" in obs
            if has_pwd and not has_fixture:
                errors.append(
                    f"{path}: {loc}: incomplete comparison: 'pwd_equals_expected' present "
                    "without 'fixture_content_matches'"
                )
            elif has_fixture and not has_pwd:
                errors.append(
                    f"{path}: {loc}: incomplete comparison: 'fixture_content_matches' present "
                    "without 'pwd_equals_expected'"
                )
            elif not has_pwd and not has_fixture:
                errors.append(
                    f"{path}: {loc}: incomplete comparison: missing tool equality predicates"
                )
            else:
                for b_key in ("pwd_equals_expected", "fixture_content_matches"):
                    b_val = obs.get(b_key)
                    if not isinstance(b_val, bool):
                        errors.append(f"{path}: {loc}.{b_key} must be a boolean (got {b_val!r})")
                    elif b_val is not True:
                        all_predicates_passed = False

            if "override_supplied" not in obs:
                errors.append(f"{path}: {loc} missing mandatory boolean 'override_supplied'")
            elif not isinstance(obs.get("override_supplied"), bool):
                errors.append(f"{path}: {loc}.override_supplied must be a boolean")

            if stage_name == "b-after-a-override":
                for inv_key in ("bounded_absence_session_cwd_set", "b_reported_cwd_unchanged"):
                    if inv_key not in obs:
                        errors.append(f"{path}: {loc} missing mandatory invariant '{inv_key}'")
                    else:
                        inv_val = obs.get(inv_key)
                        if not isinstance(inv_val, bool):
                            errors.append(f"{path}: {loc}.{inv_key} must be a boolean")
                        elif inv_val is not True:
                            all_predicates_passed = False

                s_entry = sources_dict.get(s_id, {}) if isinstance(s_id, str) else {}
                derived_name = s_entry.get("derived_file") if isinstance(s_entry, dict) else None
                if derived_name:
                    derived_path = path.parent / derived_name
                    if (
                        derived_path.is_file()
                        and isinstance(start_seq, int)
                        and isinstance(comp_seq, int)
                    ):
                        try:
                            lines = [
                                line_text
                                for line_text in derived_path.read_text(
                                    encoding="utf-8"
                                ).splitlines()
                                if line_text.strip()
                            ]
                            for line in lines[1:]:
                                rec = json.loads(line)
                                r_seq = rec.get("seq")
                                if isinstance(r_seq, int) and start_seq <= r_seq <= comp_seq:
                                    frame_data = rec.get("frame", {})
                                    candidates: list[Any] = [
                                        frame_data.get("method"),
                                        frame_data.get("type"),
                                    ]
                                    params = frame_data.get("params", {})
                                    if isinstance(params, dict):
                                        candidates.extend([
                                            params.get("method"),
                                            params.get("type"),
                                        ])
                                    if any(m == "session.cwd.set" for m in candidates):
                                        errors.append(
                                            f"{path}: {loc}: wire log {derived_name!r} contains "
                                            f"'session.cwd.set' at seq {r_seq} within "
                                            f"bounded window [{start_seq}, {comp_seq}]"
                                        )
                        except Exception:
                            pass

    # 7. Outcome coherence check
    doc_status = doc.get("status")
    if doc_status == "pass" and not all_predicates_passed:
        errors.append(
            f"{path}: status cannot be 'pass' when one or more equality predicates are false"
        )

    return errors


class DirectoryEqualityDerivationSchema:
    name = "directory-equality-derivation"
    declared_keys = DIRECTORY_EQUALITY_DERIVATION_RECORD_SCHEMA.declared_keys
    vocabularies = DIRECTORY_EQUALITY_DERIVATION_RECORD_SCHEMA.vocabularies

    def validate(
        self,
        doc: dict[str, Any],
        *,
        path: Path,
        prefix: str = "",
        expected_commit: str | None = None,
        repo_root: Path = _REPO_ROOT,
    ) -> list[str]:
        return validate_directory_equality_derivation(
            doc,
            path=path,
            prefix=prefix,
            expected_commit=expected_commit,
            repo_root=repo_root,
        )


DIRECTORY_EQUALITY_DERIVATION_SCHEMA = DirectoryEqualityDerivationSchema()


class SchemaRegistry:
    """Registry of authored record schemas under the amended privacy contract."""

    @classmethod
    def lookup(
        cls, path: Path, doc: Any
    ) -> (
        RecordSchema
        | AttestationMapSchema
        | HostSentinelSchema
        | DirectoryEqualityDerivationSchema
        | None
    ):
        if path.name == "receipt.json" or path.name.endswith("-receipt.json"):
            if isinstance(doc, dict) and (
                doc.get("schema_version") == V061_INSTALL_SCHEMA
                or "candidate" in doc
            ):
                return INSTALL_RECEIPT_SCHEMA
            return RECEIPT_SCHEMA
        if isinstance(doc, dict):
            if "redactions_confirmed" in doc and "witnessed_element" in doc:
                return READ_CONFIRMATION_RECORD_SCHEMA
            if "covered_class" in doc and "twin_span" in doc and "region" in doc:
                return REDACTION_ITEM_SCHEMA
            if (
                doc.get("schema_version") == V061_INSTALL_SCHEMA
                or ("candidate" in doc and "install" in doc and "tester" in doc)
            ):
                return INSTALL_RECEIPT_SCHEMA
            if "checklist_item" in doc and "verdict" in doc:
                return RECEIPT_SCHEMA
            if (
                doc.get("record_type") in (
                    "host-sentinel", "sentinel-witness", "host-sentinel-witness"
                )
                or doc.get("schema_version") in (
                    "talaria-v0.6.1-sentinel-v1",
                    "talaria-sentinel-v1",
                    "talaria-host-sentinel-v1",
                )
                or doc.get("format_version") in (
                    "talaria-v0.6.1-sentinel-v1",
                    "talaria-sentinel-v1",
                    "talaria-host-sentinel-v1",
                )
                or "sentinel" in path.name.lower()
                or ("keys" in doc and "positive_controls_confirmed" in doc)
            ):
                return HOST_SENTINEL_SCHEMA
            if (
                doc.get("record_type") == "directory-equality-derivation"
                or doc.get("format_version") == "talaria-directory-equality-v1"
                or path.name == "directory-equality-derivation.json"
                or "directory-equality" in path.name.lower()
                or ("stages" in doc and "sources" in doc and "derivation" in path.name.lower())
            ):
                return DIRECTORY_EQUALITY_DERIVATION_SCHEMA
            if any(
                k in doc for k in ("frame_digest", "frame_digests", "twin_digest", "redactions")
            ) or (
                {"columns", "rows", "cell_width"}.issubset(doc.keys())
                and not ("measurements" in path.name.lower() or "measurements" in doc)
            ) or (
                (
                    doc.get("format_version")
                    in CAPTURE_METADATA_SCHEMA.vocabularies.get("format_version", ())
                )
                or (
                    doc.get("schema_version")
                    in CAPTURE_METADATA_SCHEMA.vocabularies.get("schema_version", ())
                )
                or (
                    doc.get("schema")
                    in CAPTURE_METADATA_SCHEMA.vocabularies.get("schema", ())
                )
                or doc.get("record_type") == "capture-metadata"
            ):
                return CAPTURE_METADATA_SCHEMA
            if (
                "measurements" in path.name.lower()
                or "pixel" in path.name.lower()
                or "measurements" in doc
                or "regions" in doc
            ):
                return PIXEL_MEASUREMENTS_SCHEMA
            if (
                "step" in path.name.lower()
                or "step" in doc
                or ("seq" in doc and "action" in doc)
                or ("sourceSeq" in doc)
            ):
                return STEP_LOG_SCHEMA
            if (
                "attestation" in path.name.lower()
                or any(k.startswith("live-") for k in doc.keys())
            ):
                return ATTESTATION_MAP_SCHEMA
        return None

_MACOS_ACCEPTANCE_SCRATCH_PATH = re.compile(
    rb"/private/var/folders/[A-Za-z0-9._/-]+/T/"
    rb"talaria-v050-[A-Za-z0-9._-]+(?:\xe2\x80\xa6)?"
)
_COMMIT = re.compile(r"^[0-9a-f]{40}$")


def _utc_now() -> str:
    return dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat()


def _contains_home_path(value: Any, *, home: str | None = None) -> bool:
    """Return whether any nested string exposes the current operator home path."""
    home_path = home or str(Path.home().resolve())
    if isinstance(value, str):
        return home_path in value
    if isinstance(value, dict):
        return any(_contains_home_path(item, home=home_path) for item in value.values())
    if isinstance(value, list):
        return any(_contains_home_path(item, home=home_path) for item in value)
    return False


def _evidence_path(value: Any, *, field: str, repo_root: Path) -> Path:
    path = Path(_string(value, field=field)).expanduser()
    if not path.is_absolute():
        path = repo_root / path
    return path.resolve()


def _checklist() -> dict[int, dict[str, Any]]:
    document = read_json_object(_CHECKLIST_PATH)
    raw_items = document.get("items")
    if not isinstance(raw_items, list):
        raise HarnessError(f"{_CHECKLIST_PATH}: `items` must be an array")
    items: dict[int, dict[str, Any]] = {}
    for raw in raw_items:
        if not isinstance(raw, dict):
            raise HarnessError(f"{_CHECKLIST_PATH}: every item must be an object")
        number = raw.get("number")
        owner = raw.get("owner")
        title = raw.get("title")
        if (
            isinstance(number, bool)
            or not isinstance(number, int)
            or number < 1
            or owner not in {"shared", *TESTERS}
            or not isinstance(title, str)
            or not title
        ):
            raise HarnessError(f"{_CHECKLIST_PATH}: malformed checklist item {raw!r}")
        if number in items:
            raise HarnessError(f"{_CHECKLIST_PATH}: duplicate item {number}")
        items[number] = raw
    if set(items) != set(range(1, 37)):
        raise HarnessError(f"{_CHECKLIST_PATH}: checklist must contain exactly items 1 through 36")
    return items


def _tester_owns(owner: str, tester: str) -> bool:
    return owner == "shared" or owner == tester


def validate_receipt(
    receipt: dict[str, Any],
    *,
    verify_files: bool = True,
    expected_commit: str | None = None,
    repo_root: Path = _REPO_ROOT,
) -> list[str]:
    """Return every receipt defect; an empty list means the receipt is coherent."""
    errors: list[str] = []
    if _contains_home_path(receipt):
        errors.append("receipt contains the current user's home path")
    items = _checklist()
    if receipt.get("schema_version") != "talaria-v0.5.0-receipt-v1":
        errors.append("schema_version is not talaria-v0.5.0-receipt-v1")
    if receipt.get("release") != RELEASE_VERSION:
        errors.append(f"release is not {RELEASE_VERSION}")
    harness_commit = receipt.get("harness_commit")
    if not isinstance(harness_commit, str) or not _COMMIT.fullmatch(harness_commit):
        errors.append("harness_commit must be a full lowercase 40-character Git commit")
    number = receipt.get("checklist_item")
    tester = receipt.get("tester")
    owner = receipt.get("owner")
    verdict = receipt.get("verdict")
    item = (
        items.get(number) if isinstance(number, int) and not isinstance(number, bool) else None
    )
    if item is None:
        errors.append("checklist_item is not an integer from 1 through 36")
    if tester not in TESTERS:
        errors.append("tester is not talaria-t1 or talaria-t2")
    if item is not None and owner != item["owner"]:
        errors.append(f"owner does not match checklist item {number}: expected {item['owner']!r}")
    if (
        item is not None
        and isinstance(tester, str)
        and not _tester_owns(str(item["owner"]), tester)
    ):
        errors.append(f"tester {tester} does not own checklist item {number}")
    # Receipt validation accumulates defects for the operator; record generation
    # fails immediately at its trust boundary, but both use this same vocabulary.
    if verdict not in VERDICTS:
        ordered = ", ".join(
            (*ORDERED_VERDICTS[:-1], f"or {ORDERED_VERDICTS[-1]}")
        )
        errors.append(f"verdict must be {ordered}")

    try:
        artifact = _object(receipt.get("artifact"), field="artifact")
        for field in (
            "commit",
            "wheel_filename",
            "wheel_sha256",
            "version",
            "executable",
            "executable_sha256",
            "installed_files_sha256",
            "distribution_root",
            "install_receipt_path",
            "install_receipt_sha256",
        ):
            _string(artifact.get(field), field=f"artifact.{field}")
        if artifact.get("version") != RELEASE_VERSION:
            errors.append(f"artifact.version is not {RELEASE_VERSION}")
        if expected_commit is not None and artifact.get("commit") != expected_commit:
            errors.append("artifact.commit does not match the release candidate")
    except HarnessError as exc:
        errors.append(str(exc))

    try:
        terminal = _object(receipt.get("terminal"), field="terminal")
        _string(terminal.get("program"), field="terminal.program")
        _string(terminal.get("term"), field="terminal.term")
        for field in ("rows", "columns"):
            value = terminal.get(field)
            if isinstance(value, bool) or not isinstance(value, int) or value < 1:
                errors.append(f"terminal.{field} must be a positive integer")
    except HarnessError as exc:
        errors.append(str(exc))

    try:
        session = _object(receipt.get("session"), field="session")
        mode = session.get("mode")
        if mode not in {"live", "replay", "install-probe", "failure"}:
            errors.append("session.mode must be live, replay, install-probe, or failure")
        _string(session.get("profile"), field="session.profile")
        route = _object(session.get("model_route"), field="session.model_route")
        requested = route.get("requested")
        observed = route.get("observed")
        route_status = route.get("status")
        availability = route.get("fallback_availability")
        allowed_routes = {None, PRIMARY_MODEL_ROUTE, FALLBACK_MODEL_ROUTE}
        if requested not in allowed_routes:
            errors.append(f"model_route.requested is an unapproved route: {requested!r}")
        if observed not in allowed_routes:
            errors.append(f"model_route.observed is an unapproved route: {observed!r}")
        if route_status not in {"used", "not-reached", "not-applicable"}:
            errors.append("model_route.status must be used, not-reached, or not-applicable")
        if availability not in {"available", "unavailable", "not-checked", "not-applicable"}:
            errors.append("model_route.fallback_availability has an unknown value")

        reason = route.get("fallback_reason")
        fallback_involved = FALLBACK_MODEL_ROUTE in {requested, observed}
        if fallback_involved:
            if not isinstance(reason, dict):
                errors.append("a fallback request or use requires fallback_reason")
            else:
                if reason.get("code") not in FALLBACK_REASON_CODES:
                    errors.append(
                        "fallback_reason.code is not one of the four permitted reasons"
                    )
                try:
                    _string(
                        reason.get("detail"),
                        field="session.model_route.fallback_reason.detail",
                    )
                except HarnessError as exc:
                    errors.append(str(exc))
            if availability == "not-applicable":
                errors.append("fallback involvement requires a checked fallback availability")
        elif reason is not None:
            errors.append(
                "fallback_reason must be null when fallback was neither requested nor used"
            )

        if route_status == "used" and observed not in {PRIMARY_MODEL_ROUTE, FALLBACK_MODEL_ROUTE}:
            errors.append("model_route.status used requires an observed approved route")
        if route_status == "not-reached" and observed is not None:
            errors.append("model_route.status not-reached requires observed to be null")
        if route_status == "not-applicable" and (requested is not None or observed is not None):
            errors.append("model_route.status not-applicable requires both route fields to be null")
        if verdict == "pass" and mode == "live" and route_status != "used":
            errors.append("a passing live leg must observe the model route it used")
        if verdict == "pass" and observed == FALLBACK_MODEL_ROUTE and availability != "available":
            errors.append("a fallback leg cannot pass unless the named fallback was available")
        if verdict == "pass" and fallback_involved and availability == "unavailable":
            errors.append("a leg cannot pass when the named fallback was required but unavailable")
    except HarnessError as exc:
        errors.append(str(exc))

    try:
        evidence = _object(receipt.get("evidence"), field="evidence")
        redaction = evidence.get("redaction_review")
        if redaction not in {"passed", "withheld", "pending"}:
            errors.append("evidence.redaction_review must be passed, withheld, or pending")
        capture_path = _evidence_path(
            evidence.get("capture_path"), field="evidence.capture_path", repo_root=repo_root
        )
        capture_hash = _string(evidence.get("capture_sha256"), field="evidence.capture_sha256")
        screenshot_path = _evidence_path(
            evidence.get("screenshot_path"),
            field="evidence.screenshot_path",
            repo_root=repo_root,
        )
        screenshot_hash = _string(
            evidence.get("screenshot_sha256"), field="evidence.screenshot_sha256"
        )
        if verify_files:
            for path, expected_hash, label in (
                (capture_path, capture_hash, "capture"),
                (screenshot_path, screenshot_hash, "screenshot"),
            ):
                errors.extend(_verify_evidence_file(path, expected_hash, label=label))
        if verdict == "pass" and redaction != "passed":
            errors.append("a receipt cannot pass before capture and screenshot redaction review")
    except HarnessError as exc:
        errors.append(str(exc))
    return errors


def _verify_evidence_file(path: Path, expected_hash: str, *, label: str) -> list[str]:
    """Return missing or digest errors for any receipt-bound evidence file."""
    if not path.is_file():
        return [f"{label} file is missing: {path}"]
    if sha256_file(path) != expected_hash:
        return [f"{label} hash does not match its file: {path}"]
    return []


def _install_receipt(path: Path, tester: str) -> dict[str, Any]:
    receipt = read_json_object(path)
    if receipt.get("schema_version") != "talaria-v0.5.0-install-v1":
        raise HarnessError(f"{path}: not a Talaria v0.5.0 install receipt")
    if receipt.get("tester") != tester:
        raise HarnessError(f"{path}: install receipt belongs to a different tester")
    return receipt


def _path(value: Any, *, field: str) -> Path:
    return Path(_string(value, field=field)).expanduser().resolve()


def validate_scratch_evidence_paths(
    *, capture: Path, screenshot: Path, scratch_root: Path
) -> None:
    """Refuse evidence selected from outside the tester-owned scratch tree."""
    if not is_within(capture, scratch_root / "raw"):
        raise HarnessError("raw capture escaped the tester scratch raw directory")
    if not is_within(screenshot, scratch_root / "screenshots"):
        raise HarnessError(
            "screenshot must remain in the tester scratch screenshots directory"
        )
    if not capture.is_file():
        raise HarnessError(f"capture does not exist: {capture}")
    if not screenshot.is_file():
        raise HarnessError(f"screenshot does not exist: {screenshot}")


def record_receipt(args: argparse.Namespace) -> Path:
    tester = validate_tester(args.tester)
    item = _checklist().get(args.item)
    if item is None:
        raise HarnessError("checklist item must be from 1 through 36")
    owner = str(item["owner"])
    if not _tester_owns(owner, tester):
        raise HarnessError(f"{tester} does not own checklist item {args.item}")

    install_path = args.install_receipt.expanduser().resolve()
    install = _install_receipt(install_path, tester)
    scratch_root = _path(install.get("scratch_root"), field="install.scratch_root")
    pty_path = args.pty_result.expanduser().resolve()
    pty_result = read_json_object(pty_path)
    if pty_result.get("schema_version") != "talaria-v0.5.0-pty-v1":
        raise HarnessError(f"{pty_path}: not a v0.5.0 pseudo-terminal result")
    if pty_result.get("tester") != tester:
        raise HarnessError(f"{pty_path}: pseudo-terminal result belongs to a different tester")
    capture = _object(pty_result.get("capture"), field="pty.capture")
    capture_path = _path(capture.get("path"), field="pty.capture.path")
    screenshot = args.screenshot.expanduser().resolve()
    validate_scratch_evidence_paths(
        capture=capture_path,
        screenshot=screenshot,
        scratch_root=scratch_root,
    )
    candidate = _object(install.get("candidate"), field="install.candidate")
    artifact = _object(install.get("artifact"), field="install.artifact")
    requested = _ROUTE_ALIASES[args.route_requested]
    observed = _ROUTE_ALIASES[args.route_observed]
    fallback_reason: dict[str, str] | None = None
    if args.fallback_reason_code is not None or args.fallback_reason_detail is not None:
        if args.fallback_reason_code is None or args.fallback_reason_detail is None:
            raise HarnessError("fallback reason code and exact detail must be supplied together")
        fallback_reason = {
            "code": args.fallback_reason_code,
            "detail": args.fallback_reason_detail,
        }

    terminal_program = _string(
        pty_result.get("terminal_program"), field="pty.terminal_program"
    )
    term = _string(pty_result.get("term"), field="pty.term")
    rows = pty_result.get("rows")
    columns = pty_result.get("columns")
    receipt = {
        "schema_version": "talaria-v0.5.0-receipt-v1",
        "release": RELEASE_VERSION,
        "harness_commit": _string(
            pty_result.get("harness_commit"), field="pty.harness_commit"
        ),
        "recorded_at": _utc_now(),
        "checklist_item": args.item,
        "title": item["title"],
        "owner": owner,
        "tester": tester,
        "verdict": args.verdict,
        "artifact": {
            "commit": candidate["commit"],
            "wheel_filename": candidate["wheel_filename"],
            "wheel_sha256": candidate["wheel_sha256"],
            "version": artifact["version"],
            "executable": artifact["executable"],
            "executable_sha256": artifact["executable_sha256"],
            "installed_files_sha256": artifact["installed_files_sha256"],
            "distribution_root": artifact["distribution_root"],
            "install_receipt_path": str(install_path),
            "install_receipt_sha256": sha256_file(install_path),
        },
        "terminal": {
            "program": terminal_program,
            "term": term,
            "rows": rows,
            "columns": columns,
        },
        "session": {
            "mode": args.session_mode,
            "profile": args.session_profile,
            "model_route": {
                "requested": requested,
                "observed": observed,
                "status": args.route_status,
                "fallback_reason": fallback_reason,
                "fallback_availability": args.fallback_availability,
            },
        },
        "evidence": {
            "capture_path": str(capture_path),
            "capture_sha256": capture["sha256"],
            "screenshot_path": str(screenshot),
            "screenshot_sha256": sha256_file(screenshot),
            "redaction_review": args.redaction_review,
            "pty_result_path": str(pty_path),
            "pty_result_sha256": sha256_file(pty_path),
        },
        "observations": args.observation,
    }
    errors = validate_receipt(receipt)
    if errors:
        raise HarnessError("receipt is invalid:\n- " + "\n- ".join(errors))
    output = Path(args.output).expanduser().resolve()
    if not is_within(output, scratch_root / "receipts"):
        raise HarnessError("receipt output must stay in the tester scratch receipts directory")
    write_json_object(output, receipt)
    return output


def _repo_relative(path: Path, *, repo_root: Path) -> str:
    try:
        return path.resolve().relative_to(repo_root.resolve()).as_posix()
    except ValueError as exc:
        raise HarnessError(f"published evidence escaped the repository: {path}") from exc


def _copy_new(source: Path, destination: Path) -> None:
    if not source.is_file():
        raise HarnessError(f"evidence source does not exist: {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        raise HarnessError(f"refusing to replace published evidence: {destination}")
    shutil.copyfile(source, destination)


def _copy_public_capture(
    source: Path, destination: Path, *, scratch_root: Path
) -> None:
    """Copy terminal bytes while replacing only private scratch-root identifiers."""
    if not source.is_file():
        raise HarnessError(f"evidence source does not exist: {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        raise HarnessError(f"refusing to replace published evidence: {destination}")
    data = source.read_bytes().replace(
        str(scratch_root.resolve()).encode(), b"<scratch-root>"
    )
    destination.write_bytes(
        _MACOS_ACCEPTANCE_SCRATCH_PATH.sub(b"<scratch-root>", data)
    )


def _portable_json(
    value: Any, *, repo_root: Path, scratch_root: Path | None = None
) -> Any:
    """Return public JSON with checkout paths made relative and home paths redacted."""
    if isinstance(value, dict):
        return {
            key: _portable_json(item, repo_root=repo_root, scratch_root=scratch_root)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [
            _portable_json(item, repo_root=repo_root, scratch_root=scratch_root)
            for item in value
        ]
    if not isinstance(value, str):
        return value
    repository = str(repo_root.resolve())
    home = str(Path.home().resolve())
    scratch = str(scratch_root.resolve()) if scratch_root is not None else None
    if scratch is not None and value == scratch:
        return "<scratch-root>"
    if scratch is not None and value.startswith(f"{scratch}/"):
        return f"<scratch-root>/{value.removeprefix(f'{scratch}/')}"
    if value == repository:
        return "."
    if value.startswith(f"{repository}/"):
        return value.removeprefix(f"{repository}/")
    return value.replace(home, "<home>")


_ALLOWED_CRITICAL_CHUNKS: frozenset[bytes] = frozenset({b"IHDR", b"PLTE", b"IDAT", b"IEND"})
_ALLOWED_NON_TEXT_ANCILLARY_CHUNKS: frozenset[bytes] = frozenset({
    b"pHYs",
    b"gAMA",
    b"sRGB",
    b"tIME",
    b"iCCP",
})
_TEXT_CHUNK_TYPES: frozenset[bytes] = frozenset({b"tEXt", b"zTXt", b"iTXt"})

_CAPTURE_METADATA_DECLARED_KEYS: dict[str, str] = {
    k: v.value for k, v in CAPTURE_METADATA_SCHEMA.declared_keys.items()
}


def _validate_capture_metadata(doc: dict[str, Any], *, path: Path) -> list[str]:
    """Validate capture metadata against registered schema and value categories."""
    return CAPTURE_METADATA_SCHEMA.validate(doc, path=path)


def _parse_png_text_chunk(chunk_type: bytes, chunk_data: bytes) -> tuple[str, bytes, str | None]:
    """Parse keyword and text from tEXt, zTXt, or iTXt chunk."""
    if chunk_type == b"tEXt":
        if b"\x00" not in chunk_data:
            return "", b"", "tEXt chunk missing null separator"
        kw, text = chunk_data.split(b"\x00", 1)
        return kw.decode("latin-1", errors="replace"), text, None
    elif chunk_type == b"zTXt":
        if b"\x00" not in chunk_data:
            return "", b"", "zTXt chunk missing null separator"
        kw, rest = chunk_data.split(b"\x00", 1)
        if len(rest) < 1:
            return (
                kw.decode("latin-1", errors="replace"),
                b"",
                "zTXt chunk missing compression method",
            )
        comp_method = rest[0]
        if comp_method != 0:
            return (
                kw.decode("latin-1", errors="replace"),
                b"",
                f"zTXt unknown compression method {comp_method}",
            )
        try:
            decompressed = zlib.decompress(rest[1:])
            return kw.decode("latin-1", errors="replace"), decompressed, None
        except Exception as exc:
            return kw.decode("latin-1", errors="replace"), b"", f"zTXt decompression failed: {exc}"
    elif chunk_type == b"iTXt":
        if b"\x00" not in chunk_data:
            return "", b"", "iTXt chunk malformed"
        kw_raw, rest = chunk_data.split(b"\x00", 1)
        kw_str = kw_raw.decode("latin-1", errors="replace")
        if len(rest) < 2:
            return kw_str, b"", "iTXt chunk missing compression headers"
        comp_flag = rest[0]
        after_headers = rest[2:]
        parts = after_headers.split(b"\x00", 2)
        if len(parts) < 3:
            return kw_str, b"", "iTXt chunk missing null separators for tags"
        _lang_tag, _trans_kw, text_raw = parts
        if comp_flag == 1:
            try:
                decompressed = zlib.decompress(text_raw)
                return kw_str, decompressed, None
            except Exception as exc:
                return kw_str, b"", f"iTXt decompression failed: {exc}"
        else:
            return kw_str, text_raw, None
    return "", b"", f"unrecognized text chunk type {chunk_type!r}"


def _extract_png_capture_metadata(data: bytes) -> dict[str, Any] | None:
    """Extract and parse the talaria-evidence JSON chunk from PNG data if present."""
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        return None
    offset = 8
    while offset + 12 <= len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        chunk_type = data[offset + 4 : offset + 8]
        chunk_end = offset + 12 + length
        if chunk_end > len(data):
            break
        chunk_data = data[offset + 8 : offset + 8 + length]
        offset = chunk_end
        if chunk_type in _TEXT_CHUNK_TYPES:
            keyword, text_bytes, parse_err = _parse_png_text_chunk(chunk_type, chunk_data)
            if parse_err or keyword != "talaria-evidence":
                continue
            try:
                doc = json.loads(text_bytes.decode("utf-8"))
                if isinstance(doc, dict):
                    return doc
            except Exception:
                continue
    return None


def _extract_twin_digest_from_doc(doc: dict[str, Any]) -> str | None:
    digest = (
        doc.get("twin_digest")
        or doc.get("twin_sha256")
        or (
            doc.get("text_twin", {}).get("sha256")
            if isinstance(doc.get("text_twin"), dict)
            else None
        )
        or (
            doc.get("text_twin", {}).get("twin_digest")
            if isinstance(doc.get("text_twin"), dict)
            else None
        )
    )
    if isinstance(digest, str) and _V061_DIGEST.fullmatch(digest):
        return digest
    return None


def _find_capture_time_twin_digest(
    png_path: Path,
    *,
    receipt_dir: Path,
    listed: dict[Path, str],
) -> str | None:
    """Find capture-time twin_digest for a screenshot PNG.

    Checks:
    1. Screenshot PNG's talaria-evidence chunk.
    2. Per-screenshot capture metadata sidecars (<stem>.json, <stem>.metadata.json).
    Returns the 64-character hex digest if found, or None.
    """
    stem = png_path.stem
    # 1. Check PNG talaria-evidence chunk
    png_file = receipt_dir / png_path
    if png_file.is_file():
        try:
            doc = _extract_png_capture_metadata(png_file.read_bytes())
            if doc:
                digest = _extract_twin_digest_from_doc(doc)
                if digest:
                    return digest
        except Exception:
            pass

    # 2. Check per-screenshot capture metadata sidecars
    per_screenshot_sidecars = [
        png_path.with_suffix(".json"),
        png_path.parent / f"{stem}.metadata.json",
    ]
    for cand in per_screenshot_sidecars:
        if cand in listed:
            cand_file = receipt_dir / cand
            if cand_file.is_file():
                try:
                    doc = json.loads(cand_file.read_text(encoding="utf-8"))
                    if isinstance(doc, dict):
                        digest = _extract_twin_digest_from_doc(doc)
                        if digest:
                            return digest
                except Exception:
                    pass

    return None


def _find_redactions_for_twin(twin_path: Path) -> list[dict[str, Any]]:
    """Find capture-metadata redactions associated with a text twin file."""
    stem = twin_path.stem
    clean_stem = stem.replace(".screen", "")
    sidecars = [
        twin_path.with_suffix(".json"),
        twin_path.parent / f"{stem}.metadata.json",
        twin_path.parent / f"{clean_stem}.json",
    ]
    for cand in sidecars:
        if cand.is_file():
            try:
                doc = json.loads(cand.read_text(encoding="utf-8"))
                if isinstance(doc, dict) and isinstance(doc.get("redactions"), list):
                    return [r for r in doc["redactions"] if isinstance(r, dict)]
            except Exception:
                pass

    png_sidecars = [
        twin_path.with_suffix(".png"),
        twin_path.parent / f"{clean_stem}.png",
    ]
    for png_cand in png_sidecars:
        if png_cand.is_file():
            try:
                doc = _extract_png_capture_metadata(png_cand.read_bytes())
                if isinstance(doc, dict) and isinstance(doc.get("redactions"), list):
                    return [r for r in doc["redactions"] if isinstance(r, dict)]
            except Exception:
                pass

    return []


def _find_redactions_for_image(
    png_path: Path,
    *,
    receipt_dir: Path,
    listed: dict[Path, str],
) -> list[dict[str, Any]]:
    """Find capture-time redactions list for a screenshot PNG."""
    stem = png_path.stem
    clean_stem = stem.replace(".screen", "")
    # 1. Check PNG talaria-evidence chunk
    png_file = receipt_dir / png_path
    if png_file.is_file():
        try:
            doc = _extract_png_capture_metadata(png_file.read_bytes())
            if doc and isinstance(doc.get("redactions"), list):
                return [r for r in doc["redactions"] if isinstance(r, dict)]
        except Exception:
            pass

    # 2. Check per-screenshot capture metadata sidecars
    per_screenshot_sidecars = [
        png_path.with_suffix(".json"),
        png_path.parent / f"{stem}.metadata.json",
        png_path.parent / f"{clean_stem}.json",
    ]
    for cand in per_screenshot_sidecars:
        if cand in listed:
            cand_file = receipt_dir / cand
            if cand_file.is_file():
                try:
                    doc = json.loads(cand_file.read_text(encoding="utf-8"))
                    if isinstance(doc, dict) and isinstance(doc.get("redactions"), list):
                        return [r for r in doc["redactions"] if isinstance(r, dict)]
                except Exception:
                    pass

    return []


def _png_chunk_errors(path: Path, data: bytes) -> list[str]:
    """Parse PNG chunks, validate chunk types and text payloads against capture-metadata schema."""
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        return [f"{path}: screenshot is not a valid PNG file (invalid signature)"]
    errors: list[str] = []
    offset = 8
    has_ihdr = False
    has_iend = False
    while offset + 12 <= len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        chunk_type = data[offset + 4 : offset + 8]
        chunk_end = offset + 12 + length
        if chunk_end > len(data):
            errors.append(
                f"{path}: truncated PNG chunk {chunk_type.decode('latin-1', errors='replace')}"
            )
            break
        chunk_data = data[offset + 8 : offset + 8 + length]
        offset = chunk_end

        if chunk_type == b"IHDR":
            has_ihdr = True
        elif chunk_type == b"IEND":
            has_iend = True

        if (
            chunk_type in _ALLOWED_CRITICAL_CHUNKS
            or chunk_type in _ALLOWED_NON_TEXT_ANCILLARY_CHUNKS
        ):
            continue

        if chunk_type in _TEXT_CHUNK_TYPES:
            keyword, text_bytes, parse_err = _parse_png_text_chunk(chunk_type, chunk_data)
            if parse_err:
                errors.append(f"{path}: {parse_err}")
                continue
            if keyword != "talaria-evidence":
                errors.append(
                    f"{path}: PNG contains unauthorized text chunk "
                    f"{chunk_type.decode('latin-1', errors='replace')} with keyword {keyword!r} "
                    "(only talaria-evidence is permitted)"
                )
                continue
            try:
                doc = json.loads(text_bytes.decode("utf-8"))
            except Exception as exc:
                errors.append(f"{path}: PNG talaria-evidence chunk is not valid JSON: {exc}")
                continue
            if not isinstance(doc, dict):
                errors.append(f"{path}: PNG talaria-evidence chunk must be a JSON object")
                continue
            errors.extend(_validate_capture_metadata(doc, path=path))
            errors.extend(_walk_json_privacy_errors(doc, path=path, prefix="talaria-evidence"))
            continue

        errors.append(
            f"{path}: PNG contains disallowed chunk type "
            f"{chunk_type.decode('latin-1', errors='replace')!r}"
        )

    if not has_ihdr:
        errors.append(f"{path}: PNG missing IHDR chunk")
    if not has_iend:
        errors.append(f"{path}: PNG missing IEND chunk")
    return errors


def _png_ancillary_payloads(data: bytes) -> list[bytes]:
    """Return Portable Network Graphics ancillary chunks without image pixels."""
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        return [data]
    payloads: list[bytes] = []
    offset = 8
    while offset + 12 <= len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        chunk_type = data[offset + 4 : offset + 8]
        end = offset + 12 + length
        if end > len(data):
            return [data]
        if chunk_type[:1].islower():
            payloads.append(data[offset + 8 : offset + 8 + length])
        offset = end
    return payloads


def classify_evidence_file(path: Path) -> str:
    """Classify an evidence file by content rather than file name or manifest."""
    if path.suffix.lower() in {".md", ".markdown"}:
        parts = path.parts
        for idx, part in enumerate(parts):
            if part == "evidence" or (
                part == "acceptance" and idx + 2 < len(parts) and parts[idx + 2] == "evidence"
            ):
                return "markdown"
    try:
        data = path.read_bytes()
    except OSError:
        return "unreadable"
    if b'"kind": "frame"' in data or b'"kind":"frame"' in data:
        return "wire-capture"
    if path.name == "receipt.json":
        return "receipt"
    if path.suffix.lower() == ".png":
        return "screenshot"
    if path.suffix.lower() in {".ansi", ".txt"}:
        return "terminal-text"
    try:
        doc = json.loads(data.decode("utf-8"))
        if isinstance(doc, dict):
            schema = SchemaRegistry.lookup(path, doc)
            if schema is not None:
                return schema.name
        return "harness-record"
    except (UnicodeDecodeError, json.JSONDecodeError):
        pass
    try:
        text = data.decode("utf-8")
        lines = [line.strip() for line in text.splitlines() if line.strip()]
        if lines and all(_is_valid_json_line(line) for line in lines):
            return "harness-record"
    except UnicodeDecodeError:
        pass
    return "other"


def _is_valid_json_line(line: str) -> bool:
    try:
        json.loads(line)
        return True
    except (ValueError, TypeError):
        return False


def _count_redacted_literals(value: Any) -> int:
    """Count how many string values in the payload equal or contain [redacted]."""
    count = 0
    if isinstance(value, str):
        if value == "[redacted]":
            count += 1
        elif "[redacted]" in value:
            count += value.count("[redacted]")
    elif isinstance(value, dict):
        for v in value.values():
            count += _count_redacted_literals(v)
    elif isinstance(value, list):
        for item in value:
            count += _count_redacted_literals(item)
    return count


def _wire_capture_errors(path: Path, data: bytes) -> list[str]:
    """Validate that a wire capture is a valid derived frame log."""
    errors: list[str] = []
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as exc:
        return [f"{path}: wire capture is not valid UTF-8: {exc}"]
    lines = [line for line in text.splitlines() if line.strip()]
    if not lines:
        return [f"{path}: wire capture is empty"]

    # 1. Header validation (refuses headerless slices)
    try:
        header = json.loads(lines[0])
    except json.JSONDecodeError:
        return [f"{path}: wire capture is a headerless slice: header line missing or invalid JSON"]
    if not isinstance(header, dict) or header.get("kind") != "header":
        return [f"{path}: wire capture is a headerless slice: first line is not kind header"]

    # 2. Derivation metadata check (refuses raw recordings)
    derivation = header.get("derivation")
    if not isinstance(derivation, dict):
        return [
            f"{path}: wire capture is an undeclared raw recording: "
            "derivation block missing in header"
        ]
    required_derivation_keys = (
        "tool",
        "source_bytes",
        "source_frames",
        "selection",
        "rules",
        "derived_at",
    )
    if not ("source_sha256" in derivation or "source_digest" in derivation) or any(
        k not in derivation for k in required_derivation_keys
    ):
        return [f"{path}: wire capture derivation block missing required metadata"]

    # 3. Frames validation
    expected_seq = 1
    for idx, line in enumerate(lines[1:], start=1):
        try:
            record = json.loads(line)
        except json.JSONDecodeError as exc:
            errors.append(f"{path}: frame line {idx} is invalid JSON: {exc}")
            continue
        if not isinstance(record, dict) or record.get("kind") != "frame":
            errors.append(f"{path}: line {idx + 1} is not a valid frame record")
            continue
        source_seq = record.get("sourceSeq")
        if not isinstance(source_seq, int) or source_seq < 1:
            errors.append(f"{path}: frame seq {expected_seq} is missing sourceSeq")
        if record.get("seq") != expected_seq:
            errors.append(
                f"{path}: frame sequence is not gapless "
                f"(expected {expected_seq}, got {record.get('seq')})"
            )

        frame_payload = record.get("frame")
        literal_count = _count_redacted_literals(frame_payload)
        redactions = record.get("redactions", [])
        if not isinstance(redactions, list):
            errors.append(f"{path}: frame seq {expected_seq} redactions must be a list")
            redactions = []
        if literal_count != len(redactions):
            errors.append(
                f"{path}: frame seq {expected_seq} [redacted] literals ({literal_count}) "
                f"and redactions entries ({len(redactions)}) do not match one to one"
            )
        # Scan payload for remaining private patterns
        if isinstance(frame_payload, (dict, list)):
            payload_bytes = json.dumps(frame_payload).encode("utf-8")
            for pattern, label in _PRIVATE_PATTERNS:
                if pattern.search(payload_bytes):
                    errors.append(f"{path}: frame line {idx} contains a private {label}")
        expected_seq += 1

    return errors


def _walk_json_privacy_errors(value: Any, *, path: Path, prefix: str = "") -> list[str]:
    """Recursive walk on parsed JSON to refuse forbidden keys, absolute paths,
    and private string values.
    """
    errors: list[str] = []
    if isinstance(value, dict):
        for k, v in value.items():
            loc = f"{prefix}.{k}" if prefix else str(k)
            if is_forbidden_key(k):
                errors.append(f"{path}: contains forbidden key {k!r} at {loc}")
            errors.extend(_walk_json_privacy_errors(v, path=path, prefix=loc))
    elif isinstance(value, list):
        for idx, item in enumerate(value):
            loc = f"{prefix}[{idx}]"
            errors.extend(_walk_json_privacy_errors(item, path=path, prefix=loc))
    elif isinstance(value, str):
        for p in find_absolute_paths_in_text(value):
            errors.append(f"{path}: contains absolute filesystem path {p!r} at {prefix}")
        val_bytes = value.encode("utf-8")
        for pattern, label in _PRIVATE_PATTERNS:
            if pattern.search(val_bytes):
                errors.append(f"{path}: contains a private {label} at {prefix}")
    return errors


def evidence_file_privacy_errors(path: Path, repo_root: Path = _REPO_ROOT) -> list[str]:
    """Scan a single evidence file for privacy defects using content-based classification."""
    try:
        data = path.read_bytes()
    except OSError as exc:
        return [f"cannot privacy-scan {path}: {exc}"]

    if (
        is_within(path, repo_root / "docs" / "acceptance" / "v0.5.0")
        or is_within(path, repo_root / "docs" / "acceptance" / "v0.6.0")
        or is_within(path, repo_root / "docs" / "evidence")
    ):
        v050_errors: list[str] = []
        payloads = _png_ancillary_payloads(data) if path.suffix.lower() == ".png" else [data]
        for pattern, label in _PRIVATE_PATTERNS:
            if any(pattern.search(payload) for payload in payloads):
                v050_errors.append(f"{path}: contains a private {label}")
        return v050_errors

    file_class = classify_evidence_file(path)
    if file_class == "markdown":
        return [
            f"{path}: markdown files are forbidden under evidence/ "
            "(prose belongs in receipt narrative fields)"
        ]
    if file_class == "wire-capture":
        return _wire_capture_errors(path, data)

    errors: list[str] = []
    if file_class == "screenshot":
        errors.extend(_png_chunk_errors(path, data))
        for pattern, label in _PRIVATE_PATTERNS:
            if pattern.search(data):
                errors.append(f"{path}: contains a private {label}")
        return errors

    text: str | None = None
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        pass

    if text is not None:
        try:
            doc = json.loads(text)
            is_json = True
        except json.JSONDecodeError:
            is_json = False
            doc = None

        if is_json and isinstance(doc, dict):
            schema = SchemaRegistry.lookup(path, doc)
            if schema is not None:
                if isinstance(schema, DirectoryEqualityDerivationSchema):
                    errors.extend(schema.validate(doc, path=path, repo_root=repo_root))
                else:
                    errors.extend(schema.validate(doc, path=path))
            elif "evidence" in path.parts:
                errors.append(
                    f"{path}: unregistered record type or undeclared key in {path.name}"
                )
            errors.extend(_walk_json_privacy_errors(doc, path=path))
            return errors

        if is_json and isinstance(doc, list):
            for idx, item in enumerate(doc):
                loc = f"[{idx}]"
                if isinstance(item, dict):
                    schema = SchemaRegistry.lookup(path, item)
                    if schema is not None:
                        errors.extend(schema.validate(item, path=path, prefix=loc))
                    elif "evidence" in path.parts:
                        errors.append(
                            f"{path}: unregistered record type or undeclared key at {loc}"
                        )
                errors.extend(_walk_json_privacy_errors(item, path=path, prefix=loc))
            return errors

        lines = [line.strip() for line in text.splitlines() if line.strip()]
        if lines and all(_is_valid_json_line(line_str) for line_str in lines):
            for idx, line in enumerate(lines, start=1):
                loc = f"line_{idx}"
                line_obj = json.loads(line)
                if isinstance(line_obj, dict):
                    schema = SchemaRegistry.lookup(path, line_obj)
                    if schema is not None:
                        errors.extend(schema.validate(line_obj, path=path, prefix=loc))
                    elif "evidence" in path.parts:
                        errors.append(
                            f"{path}: unregistered record type or undeclared key at {loc}"
                        )
                errors.extend(_walk_json_privacy_errors(line_obj, path=path, prefix=loc))
            return errors

        # Free text (e.g. terminal-text, notes outside evidence, or text probes)
        redactions = _find_redactions_for_twin(path)
        if redactions or _SENTINEL_CANDIDATE_PATTERN.search(text):
            errors.extend(validate_twin_redactions(path, text, redactions))
            text = mask_matched_sentinels(text, redactions)
            data = text.encode("utf-8")

        for p in find_absolute_paths_in_text(text):
            errors.append(f"{path}: contains absolute filesystem path {p!r}")
        for pattern, label in _PRIVATE_PATTERNS:
            if pattern.search(data):
                errors.append(f"{path}: contains a private {label}")
        return errors

    for pattern, label in _PRIVATE_PATTERNS:
        if pattern.search(data):
            errors.append(f"{path}: contains a private {label}")
    return errors


def _privacy_errors(path: Path, repo_root: Path = _REPO_ROOT) -> list[str]:
    return evidence_file_privacy_errors(path, repo_root=repo_root)


def _public_evidence_files(repo_root: Path) -> tuple[Path, ...]:
    """Return files below every release publication root that currently exists."""
    files: set[Path] = set()
    for relative_root in _public_evidence_roots(repo_root):
        root = repo_root / relative_root
        if root.is_file():
            files.add(root)
        elif root.is_dir():
            files.update(path for path in root.rglob("*") if path.is_file())
    return tuple(sorted(files))


def public_evidence_privacy_errors(repo_root: Path = _REPO_ROOT) -> list[str]:
    """Return portable privacy defects from every public release-evidence root."""
    repo_root = repo_root.expanduser().resolve()
    errors: list[str] = []
    for path in _public_evidence_files(repo_root):
        for error in evidence_file_privacy_errors(path, repo_root=repo_root):
            errors.append(
                error.replace(str(path), _repo_relative(path, repo_root=repo_root), 1)
            )
    return errors


def _release_candidate_matches(
    evidence_commit: str, expected_commit: str, *, repo_root: Path
) -> tuple[bool, str]:
    if evidence_commit == expected_commit:
        return True, "exact"
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", evidence_commit, expected_commit],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    if ancestor.returncode != 0:
        return False, "evidence candidate is not an ancestor of the released commit"
    diff = subprocess.run(
        [
            "git",
            "diff",
            "--quiet",
            evidence_commit,
            expected_commit,
            "--",
            *_RELEASE_RELEVANT_PATHS,
        ],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    if diff.returncode == 0:
        return True, "release-relevant files are identical"
    if diff.returncode == 1:
        return False, "release-relevant files differ from the evidence candidate"
    detail = diff.stderr.strip() or diff.stdout.strip() or "unknown Git error"
    return False, f"cannot compare release-relevant files: {detail}"


def _harness_commit_matches(
    evidence_commit: str, current_commit: str, *, repo_root: Path
) -> tuple[bool, str]:
    """Accept the current harness or an ancestor with identical harness bytes."""
    if evidence_commit == current_commit:
        return True, "exact"
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", evidence_commit, current_commit],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    if ancestor.returncode != 0:
        return False, "receipt harness is not an ancestor of the current harness"
    diff = subprocess.run(
        [
            "git",
            "diff",
            "--quiet",
            evidence_commit,
            current_commit,
            "--",
            "scripts/acceptance",
        ],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    if diff.returncode == 0:
        return True, "acceptance harness files are identical"
    if diff.returncode == 1:
        return False, "acceptance harness files differ from the receipt harness"
    detail = diff.stderr.strip() or diff.stdout.strip() or "unknown Git error"
    return False, f"cannot compare acceptance harness files: {detail}"


def _write_portable_json(
    source: Path,
    destination: Path,
    *,
    repo_root: Path,
    scratch_root: Path | None = None,
) -> None:
    if destination.exists():
        raise HarnessError(f"refusing to replace published evidence: {destination}")
    document = read_json_object(source)
    destination.parent.mkdir(parents=True, exist_ok=True)
    write_json_object(
        destination,
        _portable_json(document, repo_root=repo_root, scratch_root=scratch_root),
    )


def _bind_pty_result_to_capture(pty_result: Path, capture: Path) -> None:
    """Update copied pseudo-terminal metadata for a sanitized public capture."""
    document = read_json_object(pty_result)
    capture_metadata = document.get("capture")
    if not isinstance(capture_metadata, dict):
        return
    capture_metadata["bytes"] = capture.stat().st_size
    capture_metadata["sha256"] = sha256_file(capture)
    write_json_object(pty_result, document, replace=True)


def publish_receipt(
    source_path: Path,
    *,
    public_install_receipt: Path,
    evidence_root: Path = _EVIDENCE_ROOT,
    repo_root: Path = _REPO_ROOT,
) -> Path:
    """Publish one reviewed scratch receipt with checkout-relative evidence paths."""
    source_path = source_path.expanduser().resolve()
    public_install_receipt = public_install_receipt.expanduser().resolve()
    evidence_root = evidence_root.expanduser().resolve()
    repo_root = repo_root.expanduser().resolve()
    if not is_within(evidence_root, repo_root):
        raise HarnessError("published evidence root must stay inside the repository")
    if not is_within(public_install_receipt, evidence_root):
        raise HarnessError("public install receipt must stay inside the evidence root")

    receipt = read_json_object(source_path)
    source_errors = validate_receipt(receipt, verify_files=True, repo_root=repo_root)
    if source_errors:
        raise HarnessError("scratch receipt is invalid:\n- " + "\n- ".join(source_errors))
    tester = validate_tester(_string(receipt.get("tester"), field="tester"))
    number = receipt.get("checklist_item")
    if isinstance(number, bool) or not isinstance(number, int) or number not in range(1, 37):
        raise HarnessError("checklist_item must be from 1 through 36")

    public_install = _install_receipt(public_install_receipt, tester)
    candidate = _object(public_install.get("candidate"), field="install.candidate")
    expected_commit = _string(candidate.get("commit"), field="install.candidate.commit")
    artifact = _object(receipt.get("artifact"), field="artifact")
    if artifact.get("commit") != expected_commit:
        raise HarnessError("scratch receipt does not match the public install receipt candidate")

    tester_root = evidence_root / tester.removeprefix("talaria-")
    evidence = _object(receipt.get("evidence"), field="evidence")
    capture_source = _evidence_path(
        evidence.get("capture_path"), field="evidence.capture_path", repo_root=repo_root
    )
    scratch_root = capture_source.parents[1]
    screenshot_source = _evidence_path(
        evidence.get("screenshot_path"), field="evidence.screenshot_path", repo_root=repo_root
    )
    pty_source = _evidence_path(
        evidence.get("pty_result_path"), field="evidence.pty_result_path", repo_root=repo_root
    )
    capture_destination = tester_root / "raw" / capture_source.name
    screenshot_destination = tester_root / "screenshots" / screenshot_source.name
    pty_destination = tester_root / "pty-results" / pty_source.name
    output = tester_root / "receipts" / f"item-{number:02d}-{tester}.json"

    _copy_public_capture(
        capture_source,
        capture_destination,
        scratch_root=scratch_root,
    )
    _copy_new(screenshot_source, screenshot_destination)
    _write_portable_json(
        pty_source,
        pty_destination,
        repo_root=repo_root,
        scratch_root=scratch_root,
    )
    _bind_pty_result_to_capture(pty_destination, capture_destination)
    evidence["capture_path"] = _repo_relative(capture_destination, repo_root=repo_root)
    evidence["capture_sha256"] = sha256_file(capture_destination)
    evidence["screenshot_path"] = _repo_relative(screenshot_destination, repo_root=repo_root)
    evidence["pty_result_path"] = _repo_relative(pty_destination, repo_root=repo_root)
    evidence["pty_result_sha256"] = sha256_file(pty_destination)
    artifact["install_receipt_path"] = _repo_relative(
        public_install_receipt, repo_root=repo_root
    )
    artifact["install_receipt_sha256"] = sha256_file(public_install_receipt)
    portable_receipt = _portable_json(
        receipt,
        repo_root=repo_root,
        scratch_root=scratch_root,
    )
    if not isinstance(portable_receipt, dict):
        raise HarnessError("published receipt must remain a JSON object")
    receipt = portable_receipt

    errors = validate_receipt(
        receipt,
        verify_files=True,
        expected_commit=expected_commit,
        repo_root=repo_root,
    )
    if errors:
        raise HarnessError("published receipt is invalid:\n- " + "\n- ".join(errors))
    write_json_object(output, receipt)
    return output


V060_ITEM_SCHEMA = "talaria-v0.6.0-receipt-v1"
V060_INSTALL_SCHEMA = "talaria-v0.6.0-install-v1"
V060_RELEASE = "0.6.0"

V050_ITEM_SCHEMA = "talaria-v0.5.0-receipt-v1"

#: A herdr workspace coordinate (``wFB:pT`` or ``w1:t1``) — an operational
#: handle, not a product fact, and kept out of the public tree.
_V061_PANE_ID = re.compile(r"\bw[A-Za-z0-9]+:[pt][A-Za-z0-9]+\b")
#: A session name from this run's vocabulary: a role word plus a numbered
#: suffix (``worker-2``, ``controller-3``, ``worker-3-3``). The closed role
#: labels (``worker-lane-a``) carry letters, not digits, so they survive.
_V061_SESSION_NAME = re.compile(
    r"\b(?:worker|controller|reviewer|architect|investigator|tester|operator)"
    r"-\d+(?:-\d+)*\b"
)
#: The literal a conversion may write only where the ruling allows it; the
#: never-fields reject it by their own type rules, and the error below names
#: the clause rather than leaving it to look like a typo.
_V061_NOT_RECORDED = "not recorded"
#: The live-case inventory the v0.6.1 run owes. A pattern, never a literal in
#: code: the manifest declares ``counts.expected_receipts`` and the verifier
#: reads it from there (the architect ruling on infiquetra/talaria#150).
_V061_ITEM = re.compile(r"^live-(0[1-9]|1[0-9]|2[0-3])$")
#: Kept for the not-recorded and identifier rules above; the tester field
#: itself is judged by membership in :data:`V061_ROLE_LABELS`.
_V061_TESTER = re.compile(r"^[A-Za-z][A-Za-z-]*$")
_V061_ISSUE = re.compile(
    r"^(?:(?P<number>\d+)|https://github\.com/infiquetra/talaria/issues/(?P<url_number>\d+))$"
)
_V061_DIGEST = re.compile(r"^[0-9a-f]{64}$")


def _validate_v060_receipt(
    receipt: dict[str, Any],
    *,
    expected_commit: str | None = None,
    expected_wheel: str | None = None,
) -> list[str]:
    """Return every defect in a version-0.6.0 matrix item receipt.

    Additive alongside :func:`validate_receipt`, which stays the sole reader
    of the v0.5.0 shape: v0.6.0 rows transcribe controller-observed live
    results (provenance in ``evidence``) rather than PTY captures, so they
    carry no terminal/session fields to check. What is checked — schema and
    release consts, checklist range, tester, verdict, and the candidate bind —
    mirrors the v0.5.0 vocabulary so the shared manifest loop below applies
    unchanged.
    """
    errors: list[str] = []
    if _contains_home_path(receipt):
        errors.append("receipt contains the current user's home path")
    if receipt.get("schema_version") != V060_ITEM_SCHEMA:
        errors.append(f"schema_version is not {V060_ITEM_SCHEMA}")
    if receipt.get("release") != V060_RELEASE:
        errors.append(f"release is not {V060_RELEASE}")
    harness_commit = receipt.get("harness_commit")
    if not isinstance(harness_commit, str) or not _COMMIT.fullmatch(harness_commit):
        errors.append("harness_commit must be a full lowercase 40-character Git commit")
    number = receipt.get("checklist_item")
    if not isinstance(number, int) or isinstance(number, bool) or not 1 <= number <= 12:
        errors.append("checklist_item is not an integer from 1 through 12")
    if receipt.get("tester") != "controller":
        errors.append("tester is not controller")
    if receipt.get("verdict") not in VERDICTS:
        errors.append("verdict must be pass, fail, blocked, or reserved")
    try:
        artifact = _object(receipt.get("artifact"), field="artifact")
        if expected_commit is not None and artifact.get("commit") != expected_commit:
            errors.append("artifact.commit does not match the release candidate")
        if expected_wheel is not None and artifact.get("wheel_sha256") != expected_wheel:
            errors.append("artifact.wheel_sha256 does not match the release candidate")
        if artifact.get("version") != V060_RELEASE:
            errors.append(f"artifact.version is not {V060_RELEASE}")
    except HarnessError as exc:
        errors.append(str(exc))
    try:
        evidence = _object(receipt.get("evidence"), field="evidence")
        if not isinstance(evidence.get("source"), str) or not evidence["source"].strip():
            errors.append("evidence.source must be a non-empty provenance pointer")
        if not isinstance(evidence.get("observation"), str) or not evidence["observation"].strip():
            errors.append("evidence.observation must be a non-empty observation")
    except HarnessError as exc:
        errors.append(str(exc))
    return errors


def _validate_v060_install(
    install: dict[str, Any],
    *,
    expected_commit: str | None = None,
    expected_wheel: str | None = None,
) -> list[str]:
    """Return every defect in a version-0.6.0 install-probe receipt."""
    errors: list[str] = []
    if install.get("schema_version") != V060_INSTALL_SCHEMA:
        errors.append(f"schema_version is not {V060_INSTALL_SCHEMA}")
    if install.get("tester") != "operator":
        errors.append("tester is not operator")
    try:
        candidate = _object(install.get("candidate"), field="candidate")
        if expected_commit is not None and candidate.get("commit") != expected_commit:
            errors.append("candidate.commit does not match the release candidate")
        if expected_wheel is not None and candidate.get("wheel_sha256") != expected_wheel:
            errors.append("candidate.wheel_sha256 does not match the release candidate")
        if candidate.get("version") != V060_RELEASE:
            errors.append(f"candidate.version is not {V060_RELEASE}")
    except HarnessError as exc:
        errors.append(str(exc))
    try:
        result = _object(install.get("install"), field="install")
        if result.get("version_reported") != V060_RELEASE:
            errors.append(f"install.version_reported is not {V060_RELEASE}")
        if result.get("help_ok") is not True:
            errors.append("install.help_ok is not true")
    except HarnessError as exc:
        errors.append(str(exc))
    return errors


def _v061_not_recorded_allowlist_errors(value: Any, *, path: str) -> list[str]:
    """Refuse the `not recorded` literal everywhere the ruling does not permit it.

    Written as an allowlist, not a denylist, because a denylist cannot cover a
    field that does not exist yet: every string is walked, the literal is
    permitted only under gateway, session, terminal, or at harness.identity,
    and it is refused at every other path — today's prose fields and every
    future one included (F-3: the never-list was an approximation of the
    rule, and a pass receipt with an unrecorded observation slipped through).
    """
    if isinstance(value, str):
        if value != _V061_NOT_RECORDED:
            return []
        head = path.partition(".")[0]
        if head in ("gateway", "session", "terminal") or path == "harness.identity":
            return []
        where = path or "the receipt root"
        return [
            f"{where}: the literal 'not recorded' is permitted only on gateway, "
            "session, terminal, or harness.identity"
        ]
    if isinstance(value, dict):
        errors: list[str] = []
        for key, item in value.items():
            child = f"{path}.{key}" if path else str(key)
            errors.extend(_v061_not_recorded_allowlist_errors(item, path=child))
        return errors
    if isinstance(value, list):
        errors = []
        for index, item in enumerate(value):
            errors.extend(
                _v061_not_recorded_allowlist_errors(item, path=f"{path}[{index}]")
            )
        return errors
    return []


def _v061_private_identifier_errors(value: Any, *, field: str) -> list[str]:
    """Every known-shape private identifier nested inside a receipt.

    A high-value FILTER for the shapes this run mints, not a boundary that
    makes reading receipts unnecessary: two patterns — herdr pane
    coordinates and role-word-plus-digit session names — scanned wherever a
    string can hide, because a pane coordinate in a ``method`` sentence is as
    private as one in a dedicated field (one filed receipt proved the
    sentence is where it landed). ``_contains_home_path`` covers other
    common leaks separately, and "private" is not a regex-expressible
    property, so a receipt still gets read; this catches the known shapes so
    the reading starts from a cleaner page (#150's F-5 framing).
    """
    if isinstance(value, str):
        found: list[str] = []
        if _V061_PANE_ID.search(value):
            found.append("a terminal pane identifier")
        if _V061_SESSION_NAME.search(value):
            found.append("a session name")
        return [f"{field} contains {label}" for label in found]
    if isinstance(value, dict):
        errors: list[str] = []
        for key, item in value.items():
            label = f"{field}.{key}" if field else str(key)
            if is_forbidden_key(key):
                errors.append(f"{label}: contains forbidden key {key!r}")
            errors.extend(_v061_private_identifier_errors(item, field=label))
        return errors
    if isinstance(value, list):
        errors = []
        for index, item in enumerate(value):
            errors.extend(
                _v061_private_identifier_errors(item, field=f"{field}[{index}]")
            )
        return errors
    return []


def _validate_v061_receipt(
    receipt: dict[str, Any],
    *,
    receipt_path: Path,
    verify_files: bool = True,
) -> list[str]:
    """Return every defect in a v0.6.1 live-case receipt (#150's rulings).

    Identity is split by key, not by convention: ``candidate_commit_sha``
    names the Talaria commit whose code ran, ``install`` says how the product
    was installed (a source checkout at that commit, or a wheel with its
    filename and digest), and ``harness`` names what wrote the receipt — its
    commit required only when the harness is this repository's own tooling,
    because a scratch harness under a temporary directory has no commit and
    says so with a null and an identity string. The retired ``harness_commit``
    key is rejected outright, so no reader ever has to guess which artefact a
    commit refers to.

    The literal ``not recorded`` is permitted only where the ruling allows
    it — gateway, session, terminal, harness.identity — and the fields the
    ruling forbids it on carry a named error alongside their type rules.
    Private identifiers (pane coordinates, session names) are refused the way
    home paths are. Unknown extra keys are allowed, so the rich receipts keep
    their detail; the two forbidden things are the retired key and private
    identifiers.
    """
    errors: list[str] = []
    if _contains_home_path(receipt):
        errors.append("receipt contains the current user's home path")
    errors.extend(_v061_private_identifier_errors(receipt, field="receipt"))
    if receipt.get("schema_version") != V061_ITEM_SCHEMA:
        errors.append(f"schema_version is not {V061_ITEM_SCHEMA}")
    if receipt.get("release") != V061_RELEASE:
        errors.append(f"release is not {V061_RELEASE}")
    if "harness_commit" in receipt:
        errors.append(
            "harness_commit is retired: candidate_commit_sha, install, and harness "
            "split the identities by key so no reader has to guess which "
            "artefact a commit refers to"
        )
    item = receipt.get("checklist_item")
    if not isinstance(item, str) or not _V061_ITEM.fullmatch(item):
        errors.append("checklist_item must be a live-NN string with NN from 01 through 23")
    title = receipt.get("title")
    if not isinstance(title, str) or not title.strip():
        errors.append("title must be a non-empty string")
    issue = receipt.get("issue")
    if not isinstance(issue, str) or not _V061_ISSUE.fullmatch(issue):
        errors.append("issue must be the owning child's number or its talaria issue URL")
    tester = receipt.get("tester")
    if tester not in V061_ROLE_LABELS:
        labels = ", ".join(V061_ROLE_LABELS[:-1]) + f", or {V061_ROLE_LABELS[-1]}"
        errors.append(f"tester must be a closed-set role label ({labels})")
    if receipt.get("verdict") not in VERDICTS:
        errors.append("verdict must be pass, fail, blocked, or reserved")
    candidate_commit = receipt.get("candidate_commit_sha")
    if not isinstance(candidate_commit, str) or not _COMMIT.fullmatch(candidate_commit):
        errors.append(
            "candidate_commit_sha must be a full lowercase 40-character Git commit"
        )
    recorded_at = receipt.get("recorded_at")
    if not isinstance(recorded_at, str):
        errors.append("recorded_at must be an ISO-8601 timestamp")
    else:
        try:
            dt.datetime.fromisoformat(recorded_at)
        except ValueError:
            errors.append("recorded_at must be an ISO-8601 timestamp")
    # The allowlist walk subsumes the old never-list: the mandatory fields
    # refuse the literal through their own type rules and through this walk,
    # and no future prose field can inherit acceptance silently.
    errors.extend(_v061_not_recorded_allowlist_errors(receipt, path=""))

    try:
        install = _object(receipt.get("install"), field="install")
        install_kind = install.get("kind")
        if install_kind not in ("source-checkout", "wheel"):
            errors.append(
                "install.kind must be source-checkout or wheel; the literal "
                "'not recorded' is not one of them"
            )
        elif install_kind == "source-checkout":
            install_commit = install.get("commit")
            if install_commit != candidate_commit:
                errors.append(
                    "install.commit must be the receipt's candidate_commit_sha for a "
                    "source-checkout install"
                )
        else:
            filename = install.get("filename")
            if not isinstance(filename, str) or not filename.strip():
                errors.append("install.filename must be a non-empty string for a wheel")
            install_sha = install.get("sha256")
            if not isinstance(install_sha, str) or not _V061_DIGEST.fullmatch(install_sha):
                errors.append("install.sha256 must be the wheel's SHA-256 digest")
    except HarnessError as exc:
        errors.append(str(exc))

    try:
        harness = _object(receipt.get("harness"), field="harness")
        harness_kind = harness.get("kind")
        if harness_kind not in ("repository-tooling", "scratch-capture", "manual"):
            errors.append("harness.kind must be repository-tooling, scratch-capture, or manual")
        harness_commit = harness.get("commit")
        if harness_commit is None:
            if harness_kind == "repository-tooling":
                errors.append(
                    "a repository-tooling harness must name its own commit in harness.commit"
                )
        elif not isinstance(harness_commit, str) or not _COMMIT.fullmatch(harness_commit):
            errors.append("harness.commit must be a full 40-character commit or null")
        harness_identity = harness.get("identity")
        if harness_identity is not None:
            if not isinstance(harness_identity, str) or not harness_identity.strip():
                errors.append("harness.identity must be a non-empty string or null")
            elif find_absolute_paths_in_text(harness_identity):
                errors.append(
                    "harness.identity must not contain an absolute filesystem path "
                    f"({harness_identity!r})"
                )
    except HarnessError as exc:
        errors.append(str(exc))

    try:
        evidence = _object(receipt.get("evidence"), field="evidence")
        narrative = evidence.get("narrative")
        if narrative is not None:
            if not isinstance(narrative, dict):
                errors.append("evidence.narrative must be an object when present")
            else:
                for prose_field in ("method", "observation"):
                    value = narrative.get(prose_field)
                    if not isinstance(value, str) or not value.strip():
                        errors.append(
                            f"evidence.narrative.{prose_field} must be a non-empty string"
                        )
        files = evidence.get("files")
        if not isinstance(files, dict) or not files:
            errors.append("evidence.files must name every evidence file with its digest")
            files = {}
        receipt_dir = receipt_path.parent
        listed: dict[Path, str] = {}
        for raw_name, raw_digest in files.items():
            if not isinstance(raw_name, str) or not raw_name.strip():
                errors.append("evidence.files keys must be non-empty relative paths")
                continue
            if not isinstance(raw_digest, str) or not _V061_DIGEST.fullmatch(raw_digest):
                errors.append(f"evidence.files['{raw_name}'] must carry a SHA-256 digest")
                continue
            name = Path(raw_name)
            if name.is_absolute() or raw_name.startswith("~") or ".." in name.parts:
                errors.append(
                    f"evidence.files['{raw_name}'] must stay inside the receipt's directory"
                )
                continue
            listed[name] = raw_digest
        if verify_files:
            for name, digest in sorted(listed.items()):
                target = (receipt_dir / name).resolve()
                if not is_within(target, receipt_dir):
                    errors.append(
                        f"evidence.files['{name}'] resolves outside the receipt's directory"
                    )
                    continue
                if not target.is_file():
                    errors.append(f"evidence file is missing: {receipt_dir / name}")
                    continue
                if sha256_file(target) != digest:
                    errors.append(f"evidence file does not match its digest: {receipt_dir / name}")
            on_disk = {
                path.relative_to(receipt_dir)
                for path in receipt_dir.rglob("*")
                if path.is_file() and path.name != "receipt.json"
            }
            for missing in sorted(on_disk - set(listed)):
                errors.append(
                    f"evidence file beside the receipt is not listed in evidence.files: {missing}"
                )
            png_names = [name for name in listed if name.suffix.lower() == ".png"]
            any_image_redacted = False
            has_redaction_defects = False
            for png_path in png_names:
                stem = png_path.stem
                twin_candidates = [
                    candidate
                    for candidate in (
                        png_path.with_suffix(".txt"),
                        png_path.with_suffix(".ansi"),
                        png_path.parent / f"{stem}.screen.txt",
                    )
                    if candidate in listed
                ]
                if not twin_candidates and "twin_path" in evidence:
                    explicit_twin = Path(evidence["twin_path"])
                    if explicit_twin in listed:
                        twin_candidates.append(explicit_twin)

                twin_file: Path | None = None
                twin_text: str | None = None
                if twin_candidates:
                    twin_file = twin_candidates[0]
                    twin_target = receipt_dir / twin_file
                    if twin_target.is_file():
                        twin_text = twin_target.read_text(encoding="utf-8", errors="replace")
                    twin_digest = listed[twin_file]

                    declared_twin = evidence.get("twin_digest") or receipt.get("twin_digest")
                    if declared_twin and declared_twin != twin_digest:
                        errors.append(
                            f"screenshot '{png_path}' text twin '{twin_file}' digest in "
                            f"evidence.files ({twin_digest}) does not match declared "
                            f"twin_digest ({declared_twin})"
                        )
                    declared_sha = evidence.get("twin_sha256") or receipt.get("twin_sha256")
                    if declared_sha and declared_sha != twin_digest:
                        errors.append(
                            f"screenshot '{png_path}' text twin '{twin_file}' digest in "
                            f"evidence.files ({twin_digest}) does not match declared "
                            f"twin_sha256 ({declared_sha})"
                        )

                    capture_twin_digest = _find_capture_time_twin_digest(
                        png_path, receipt_dir=receipt_dir, listed=listed
                    )
                    if capture_twin_digest is not None:
                        if capture_twin_digest != twin_digest:
                            errors.append(
                                f"screenshot '{png_path}' text twin '{twin_file}' digest "
                                f"({twin_digest}) does not match capture-time twin_digest "
                                f"({capture_twin_digest})"
                            )
                    else:
                        errors.append(
                            f"screenshot '{png_path}' text twin '{twin_file}' is not bound by "
                            "capture-time twin_digest (twin was not produced at capture time)"
                        )
                else:
                    read_by = receipt.get("screenshots_read_by") or evidence.get(
                        "screenshots_read_by"
                    )
                    read_at = receipt.get("screenshots_read_at") or evidence.get(
                        "screenshots_read_at"
                    )
                    if (
                        not isinstance(read_by, str)
                        or not read_by.strip()
                        or not isinstance(read_at, str)
                        or not read_at.strip()
                    ):
                        errors.append(
                            "screenshots have neither a text twin nor a recorded human read "
                            "(screenshots_read_by and screenshots_read_at in receipt)"
                        )
                        break
                    if read_by not in V061_ROLE_LABELS:
                        errors.append(
                            f"screenshots_read_by must be a closed-set role label "
                            f"({', '.join(V061_ROLE_LABELS[:-1])}, or {V061_ROLE_LABELS[-1]})"
                        )
                        break

                redactions = _find_redactions_for_image(
                    png_path, receipt_dir=receipt_dir, listed=listed
                )
                if redactions:
                    any_image_redacted = True
                    r_errs = validate_redactions_list(redactions, path=receipt_dir / png_path)
                    if r_errs:
                        has_redaction_defects = True
                        errors.extend(r_errs)
                    if twin_file is not None and twin_text is not None:
                        tw_errs = validate_twin_redactions(
                            receipt_dir / twin_file, twin_text, redactions
                        )
                        if tw_errs:
                            has_redaction_defects = True
                            errors.extend(tw_errs)

                confirmations = (
                    evidence.get("read_confirmations")
                    or receipt.get("read_confirmations")
                    or []
                )
                if not isinstance(confirmations, list):
                    errors.append(f"{receipt_path}: read_confirmations must be a list")
                    has_redaction_defects = True
                else:
                    c_errs = validate_image_read_confirmations(
                        str(png_path),
                        twin_file=str(twin_file) if twin_file is not None else None,
                        twin_text=twin_text,
                        redactions=redactions,
                        confirmations=confirmations,
                        receipt_or_path=receipt_path,
                    )
                    if c_errs:
                        has_redaction_defects = True
                        errors.extend(c_errs)

            if any_image_redacted:
                redaction_review = evidence.get("redaction_review") or receipt.get(
                    "redaction_review"
                )
                if not redaction_review:
                    errors.append(
                        f"{receipt_path}: evidence has redacted images "
                        "but no redaction_review field"
                    )
                elif redaction_review not in ("passed", "withheld", "pending"):
                    errors.append(
                        f"{receipt_path}: evidence.redaction_review must be passed, "
                        f"withheld, or pending ({redaction_review!r})"
                    )
                elif receipt.get("verdict") == "pass" and redaction_review != "passed":
                    errors.append(
                        f"{receipt_path}: verdict is pass but evidence.redaction_review "
                        f"is {redaction_review!r} (must be passed)"
                    )
                elif redaction_review == "passed" and has_redaction_defects:
                    errors.append(
                        f"{receipt_path}: evidence.redaction_review cannot be 'passed' "
                        "while redaction defects exist"
                    )

            derivation_name = Path("directory-equality-derivation.json")
            if derivation_name in listed:
                target = receipt_dir / derivation_name
                if target.is_file():
                    try:
                        derivation_doc = json.loads(target.read_text(encoding="utf-8"))
                        if isinstance(derivation_doc, dict):
                            d_commit = derivation_doc.get("candidate_commit")
                            if candidate_commit is not None and d_commit != candidate_commit:
                                errors.append(
                                    f"directory-equality derivation candidate_commit ({d_commit}) "
                                    f"does not match receipt candidate_commit_sha "
                                    f"({candidate_commit})"
                                )
                    except Exception:
                        pass
    except HarnessError as exc:
        errors.append(str(exc))
    if "supersedes" in receipt and receipt["supersedes"] is not None:
        try:
            supersedes = _object(receipt.get("supersedes"), field="supersedes")
            sup_receipt_sha = supersedes.get("receipt_sha256")
            if not isinstance(sup_receipt_sha, str) or not _V061_DIGEST.fullmatch(sup_receipt_sha):
                errors.append("supersedes.receipt_sha256 must be a 64-character SHA-256 digest")
            sup_commit = supersedes.get("candidate_commit_sha")
            if not isinstance(sup_commit, str) or not _COMMIT.fullmatch(sup_commit):
                errors.append("supersedes.candidate_commit_sha must be a full 40-character commit")
        except HarnessError as exc:
            errors.append(str(exc))
    return errors


def _validate_v061_install(
    install: dict[str, Any],
    *,
    expected_commit: str | None = None,
    expected_wheel: str | None = None,
) -> list[str]:
    """Return every defect in a v0.6.1 install-probe receipt."""
    errors: list[str] = []
    if install.get("schema_version") != V061_INSTALL_SCHEMA:
        errors.append(f"schema_version is not {V061_INSTALL_SCHEMA}")
    if install.get("tester") != "operator":
        errors.append("tester is not operator")
    try:
        candidate = _object(install.get("candidate"), field="candidate")
        if expected_commit is not None and candidate.get("commit") != expected_commit:
            errors.append("candidate.commit does not match the release candidate")
        if expected_wheel is not None and candidate.get("wheel_sha256") != expected_wheel:
            errors.append("candidate.wheel_sha256 does not match the release candidate")
        if candidate.get("version") != V061_RELEASE:
            errors.append(f"candidate.version is not {V061_RELEASE}")
    except HarnessError as exc:
        errors.append(str(exc))
    try:
        result = _object(install.get("install"), field="install")
        if result.get("version_reported") != V061_RELEASE:
            errors.append(f"install.version_reported is not {V061_RELEASE}")
        if result.get("help_ok") is not True:
            errors.append("install.help_ok is not true")
    except HarnessError as exc:
        errors.append(str(exc))
    return errors


def _is_ancestor(commit: str, descendant: str, *, repo_root: Path) -> bool:
    """Whether ``commit`` is an ancestor of ``descendant`` in this repository."""
    result = subprocess.run(
        ["git", "merge-base", "--is-ancestor", commit, descendant],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode == 0


def _manifest_candidate(manifest: dict[str, Any]) -> dict[str, Any] | None:
    candidate = manifest.get("current_candidate", manifest.get("candidate"))
    if candidate is None:
        return None
    return _object(candidate, field="manifest.current_candidate")


def verify_run(
    manifest_path: Path = _MANIFEST_PATH,
    *,
    evidence_root: Path = _EVIDENCE_ROOT,
    repo_root: Path = _REPO_ROOT,
    expected_candidate_commit: str | None = None,
) -> list[str]:
    """Bind every active receipt and its files to the manifest's current candidate."""
    manifest_path = manifest_path.expanduser().resolve()
    evidence_root = evidence_root.expanduser().resolve()
    repo_root = repo_root.expanduser().resolve()
    manifest = read_json_object(manifest_path)
    all_receipt_paths = receipt_paths(evidence_root)
    current_receipt_paths = active_receipt_paths(evidence_root)
    quarantined_receipt_paths = tuple(
        path
        for path in all_receipt_paths
        if is_quarantined_receipt(path, evidence_root=evidence_root)
    )
    install_paths = sorted(evidence_root.glob("*/install-receipt.json"))
    errors: list[str] = []

    try:
        candidate = _manifest_candidate(manifest)
    except HarnessError as exc:
        errors.append(str(exc))
        candidate = None
    if candidate is None:
        if current_receipt_paths or install_paths:
            errors.append("manifest candidate is null while receipts exist")
        return errors

    try:
        expected_commit = _string(candidate.get("commit"), field="manifest.candidate.commit")
        expected_wheel = _string(
            candidate.get("wheel_sha256"), field="manifest.candidate.wheel_sha256"
        )
    except HarnessError as exc:
        errors.append(str(exc))
        return errors
    if expected_candidate_commit is not None:
        matches, reason = _release_candidate_matches(
            expected_commit, expected_candidate_commit, repo_root=repo_root
        )
        if not matches:
            errors.append(
                "manifest candidate does not describe the released commit "
                f"{expected_candidate_commit}: {reason}"
            )
    try:
        current_harness_commit = repository_head(repo_root)
    except HarnessError as exc:
        errors.append(str(exc))
        current_harness_commit = None

    quarantined_origins: dict[str, list[Path]] = {}
    for path in quarantined_receipt_paths:
        quarantined_origins.setdefault(sha256_file(path), []).append(path)

    manifest_receipts = manifest.get("receipts")
    if not isinstance(manifest_receipts, list):
        errors.append("manifest.receipts must be an array")
        manifest_receipts = []
    manifest_installs = manifest.get("install_receipts", [])
    if not isinstance(manifest_installs, list):
        errors.append("manifest.install_receipts must be an array")
        manifest_installs = []
    receipt_entries: dict[str, dict[str, Any]] = {}
    for raw_entry in manifest_receipts:
        if isinstance(raw_entry, dict) and isinstance(raw_entry.get("receipt_path"), str):
            receipt_entries[raw_entry["receipt_path"]] = raw_entry
    install_entries: dict[str, dict[str, Any]] = {}
    for raw_entry in manifest_installs:
        if isinstance(raw_entry, dict) and isinstance(raw_entry.get("receipt_path"), str):
            install_entries[raw_entry["receipt_path"]] = raw_entry

    active_receipt_names: set[str] = set()
    v061_items: list[tuple[str, str]] = []
    v061_expected = None
    counts_block = manifest.get("counts")
    if isinstance(counts_block, dict):
        raw_expected = counts_block.get("expected_receipts")
        if (
            not isinstance(raw_expected, bool)
            and isinstance(raw_expected, int)
            and raw_expected >= 1
        ):
            v061_expected = raw_expected
    for path in current_receipt_paths:
        relative = _repo_relative(path, repo_root=repo_root)
        active_receipt_names.add(relative)
        receipt = read_json_object(path)
        matching_origins = quarantined_origins.get(sha256_file(path), [])
        for origin in matching_origins:
            errors.append(
                f"{relative}: active receipt has superseded evidence origin "
                f"{_repo_relative(origin, repo_root=repo_root)}"
            )
        schema_version = receipt.get("schema_version")
        if schema_version == V060_ITEM_SCHEMA:
            validator = _validate_v060_receipt(
                receipt,
                expected_commit=expected_commit,
                expected_wheel=expected_wheel,
            )
        elif schema_version == V061_ITEM_SCHEMA:
            validator = _validate_v061_receipt(
                receipt, receipt_path=path, verify_files=True
            )
        elif schema_version == V050_ITEM_SCHEMA:
            validator = validate_receipt(
                receipt,
                verify_files=True,
                expected_commit=expected_commit,
                repo_root=repo_root,
            )
        else:
            # The routing this branch replaces: any unrecognized schema used
            # to fall through to the v0.5.0 rules, so a receipt declaring a
            # newer shape failed on every v0.5.0 field instead of being named
            # unknown. Loud and specific is the fix the reviewer proved
            # needed — with the v0.5.0 shape named above so the oldest
            # receipts keep their own validator rather than reading unknown.
            errors.append(
                f"{relative}: unknown receipt schema_version {schema_version!r}; expected one of "
                f"{V050_ITEM_SCHEMA}, {V060_ITEM_SCHEMA}, or {V061_ITEM_SCHEMA}"
            )
            continue
        for error in validator:
            errors.append(f"{relative}: {error}")
        harness_commit = receipt.get("harness_commit")
        if schema_version == V061_ITEM_SCHEMA:
            # The v0.6.1 lineage replaces harness-identity with the manifest's
            # per-receipt attestation: live receipts ride frozen wave heads,
            # and `applies_to_candidate` says why each still applies. The
            # v0.5.0 harness-bytes check below does not run for them — which
            # is exactly why the candidate floor here is mechanical and
            # fail-closed: the receipt's named commit must resolve in this
            # repository and be an ancestor of what the manifest binds, or
            # the old skip would leave nothing under the candidate at all
            # (F-1: it failed toward accepting, and now it cannot).
            item = receipt.get("checklist_item")
            verdict = receipt.get("verdict")
            receipt_commit = receipt.get("candidate_commit_sha")
            if isinstance(receipt_commit, str) and _COMMIT.fullmatch(receipt_commit):
                if not _commit_resolves(receipt_commit, repo_root=repo_root):
                    errors.append(
                        f"{relative}: candidate_commit_sha {receipt_commit[:12]} does not "
                        f"resolve in this repository — a receipt may not name a commit "
                        f"that exists nowhere"
                    )
                elif not _is_ancestor(
                    receipt_commit, expected_commit, repo_root=repo_root
                ):
                    errors.append(
                        f"{relative}: candidate_commit_sha {receipt_commit[:12]} is not an "
                        f"ancestor of the manifest's candidate {expected_commit[:12]} — "
                        f"the attestation sentence must explain a real lineage, not an "
                        f"invented one"
                    )
            if isinstance(item, str):
                if any(item == seen for seen, _ in v061_items):
                    errors.append(f"checklist_item {item} is declared by more than one receipt")
                v061_items.append((item, verdict if isinstance(verdict, str) else ""))
        elif (
            current_harness_commit is not None
            and isinstance(harness_commit, str)
            and _COMMIT.fullmatch(harness_commit)
        ):
            matches, reason = _harness_commit_matches(
                harness_commit, current_harness_commit, repo_root=repo_root
            )
            if not matches:
                errors.append(f"{relative}: incompatible harness_commit: {reason}")
        artifact = receipt.get("artifact")
        if isinstance(artifact, dict) and artifact.get("wheel_sha256") != expected_wheel:
            errors.append(f"{relative}: artifact.wheel_sha256 does not match the release candidate")
        entry = receipt_entries.get(relative)
        if entry is None:
            errors.append(f"receipt is absent from manifest: {relative}")
        else:
            if entry.get("receipt_sha256") != sha256_file(path):
                errors.append(f"manifest receipt digest does not match: {relative}")
            for receipt_field, manifest_field in (
                ("checklist_item", "checklist_item"),
                ("tester", "tester"),
                ("verdict", "verdict"),
            ):
                if entry.get(manifest_field) != receipt.get(receipt_field):
                    errors.append(f"manifest {manifest_field} does not match: {relative}")
            if schema_version == V061_ITEM_SCHEMA:
                receipt_commit = receipt.get("candidate_commit_sha")
                if entry.get("candidate_commit_sha") != receipt_commit:
                    errors.append(f"manifest candidate_commit_sha does not match: {relative}")
                applies = entry.get("applies_to_candidate")
                if receipt_commit == expected_commit:
                    if applies != "same":
                        errors.append(
                            "applies_to_candidate must be 'same' when the receipt's "
                            f"candidate_commit_sha equals the candidate's: {relative}"
                        )
                elif not isinstance(applies, str) or not applies.strip() or applies == "same":
                    errors.append(
                        f"applies_to_candidate must be a non-empty sentence naming the "
                        f"unchanged surfaces since the receipt's commit: {relative}"
                    )
    if v061_expected is not None and v061_items:
        # The no-waiver READY rule, machine-enforced: the manifest declares how
        # many live receipts the run owes (a parameter read from the manifest,
        # never a literal here), and every one of them must read pass.
        if len(v061_items) != v061_expected:
            errors.append(
                f"{len(v061_items)} live receipts on disk, but counts.expected_receipts "
                f"declares {v061_expected}"
            )
        non_pass = sorted(item for item, verdict in v061_items if verdict != "pass")
        if non_pass:
            errors.append(
                "the READY rule has no waiver path: every live receipt must read pass, "
                "got a non-pass verdict for " + ", ".join(non_pass)
            )

    for relative in sorted(set(receipt_entries) - active_receipt_names):
        errors.append(f"manifest names an absent receipt: {relative}")

    active_install_names: set[str] = set()
    for path in install_paths:
        relative = _repo_relative(path, repo_root=repo_root)
        active_install_names.add(relative)
        install = read_json_object(path)
        if _contains_home_path(install):
            errors.append(f"{relative}: install receipt contains the current user's home path")
        install_schema = install.get("schema_version")
        if install_schema == V060_INSTALL_SCHEMA:
            for error in _validate_v060_install(
                install,
                expected_commit=expected_commit,
                expected_wheel=expected_wheel,
            ):
                errors.append(f"{relative}: {error}")
        elif install_schema == V061_INSTALL_SCHEMA:
            for error in _validate_v061_install(
                install,
                expected_commit=expected_commit,
                expected_wheel=expected_wheel,
            ):
                errors.append(f"{relative}: {error}")
        elif install_schema != V050_INSTALL_SCHEMA:
            errors.append(
                f"{relative}: unknown install receipt schema_version {install_schema!r}"
            )
        try:
            install_candidate = _object(
                install.get("candidate"), field=f"{relative}: candidate"
            )
            if install_candidate.get("commit") != expected_commit:
                errors.append(f"{relative}: candidate commit does not match the manifest")
            if install_candidate.get("wheel_sha256") != expected_wheel:
                errors.append(f"{relative}: wheel digest does not match the manifest")
        except HarnessError as exc:
            errors.append(str(exc))
        entry = install_entries.get(relative)
        if entry is None:
            errors.append(f"install receipt is absent from manifest: {relative}")
        elif entry.get("receipt_sha256") != sha256_file(path):
            errors.append(f"manifest install receipt digest does not match: {relative}")

    for relative in sorted(set(install_entries) - active_install_names):
        errors.append(f"manifest names an absent install receipt: {relative}")
    if manifest.get("status") == "not-run" and (current_receipt_paths or install_paths):
        errors.append("manifest status is not-run while receipts exist")
    errors.extend(public_evidence_privacy_errors(repo_root))
    return errors


def validate_matrix(directory: Path) -> list[str]:
    items = _checklist()
    expected = {
        (number, tester)
        for number, item in items.items()
        for tester in TESTERS
        if _tester_owns(str(item["owner"]), tester)
    }
    found: dict[tuple[int, str], Path] = {}
    errors: list[str] = []
    for path in sorted(directory.glob("*.json")):
        receipt = read_json_object(path)
        number = receipt.get("checklist_item")
        tester = receipt.get("tester")
        if not isinstance(number, int) or not isinstance(tester, str):
            errors.append(f"{path}: no checklist_item/tester identity")
            continue
        key = (number, tester)
        if key in found:
            errors.append(f"duplicate receipt {key}: {found[key]} and {path}")
            continue
        found[key] = path
        for error in validate_receipt(receipt):
            errors.append(f"{path}: {error}")
        if receipt.get("verdict") not in TERMINAL_VERDICTS:
            errors.append(f"{path}: terminal verdict is {receipt.get('verdict')!r}")
    for key in sorted(expected - set(found)):
        errors.append(f"missing receipt for item {key[0]} and tester {key[1]}")
    for key in sorted(set(found) - expected):
        errors.append(f"unexpected receipt for item {key[0]} and tester {key[1]}")
    return errors


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    record = subparsers.add_parser("record", help="create one immutable item receipt")
    record.add_argument("--install-receipt", type=Path, required=True)
    record.add_argument("--pty-result", type=Path, required=True)
    record.add_argument("--screenshot", type=Path, required=True)
    record.add_argument("--output", type=Path, required=True)
    record.add_argument("--tester", required=True)
    record.add_argument("--item", type=int, required=True)
    record.add_argument("--verdict", choices=ORDERED_VERDICTS, required=True)
    record.add_argument(
        "--session-mode",
        choices=("live", "replay", "install-probe", "failure"),
        required=True,
    )
    record.add_argument("--session-profile", required=True)
    record.add_argument("--route-requested", choices=tuple(_ROUTE_ALIASES), required=True)
    record.add_argument("--route-observed", choices=tuple(_ROUTE_ALIASES), required=True)
    record.add_argument(
        "--route-status", choices=("used", "not-reached", "not-applicable"), required=True
    )
    record.add_argument(
        "--fallback-availability",
        choices=("available", "unavailable", "not-checked", "not-applicable"),
        required=True,
    )
    record.add_argument("--fallback-reason-code", choices=tuple(sorted(FALLBACK_REASON_CODES)))
    record.add_argument("--fallback-reason-detail")
    record.add_argument(
        "--redaction-review", choices=("passed", "withheld", "pending"), required=True
    )
    record.add_argument("--observation", action="append", default=[])

    publish = subparsers.add_parser(
        "publish", help="copy one reviewed scratch receipt into repository evidence"
    )
    publish.add_argument("receipt", type=Path)
    publish.add_argument("--public-install-receipt", type=Path, required=True)
    publish.add_argument("--evidence-root", type=Path, default=_EVIDENCE_ROOT)

    validate = subparsers.add_parser("validate", help="validate one existing receipt")
    validate.add_argument("receipt", type=Path)
    validate.add_argument("--skip-file-checks", action="store_true")
    validate.add_argument("--manifest", type=Path)

    matrix = subparsers.add_parser("validate-matrix", help="require every owned receipt")
    matrix.add_argument("directory", type=Path)
    verify = subparsers.add_parser(
        "verify-run", help="verify active receipts against the generated manifest"
    )
    verify.add_argument("--manifest", type=Path, default=_MANIFEST_PATH)
    verify.add_argument("--evidence-root", type=Path, default=_EVIDENCE_ROOT)
    verify.add_argument("--expect-candidate")
    return parser


def _main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.command == "record":
        path = record_receipt(args)
        print(path)
        return 0 if args.verdict in TERMINAL_VERDICTS else 1
    if args.command == "publish":
        path = publish_receipt(
            args.receipt,
            public_install_receipt=args.public_install_receipt,
            evidence_root=args.evidence_root,
        )
        print(path)
        return 0
    if args.command == "validate":
        expected_commit = None
        if args.manifest is not None:
            manifest_candidate = _manifest_candidate(read_json_object(args.manifest))
            if manifest_candidate is None:
                raise HarnessError("manifest candidate is null")
            expected_commit = _string(
                manifest_candidate.get("commit"), field="manifest.candidate.commit"
            )
        receipt = read_json_object(args.receipt)
        verify_files = not args.skip_file_checks
        schema_version = receipt.get("schema_version")
        if schema_version == V060_ITEM_SCHEMA:
            errors = _validate_v060_receipt(receipt, expected_commit=expected_commit)
        elif schema_version == V061_ITEM_SCHEMA:
            # Machine-check a receipt as it is filed, before any manifest
            # exists: the evidence inventory is verifiable from the receipt's
            # own directory alone.
            errors = _validate_v061_receipt(
                receipt, receipt_path=args.receipt, verify_files=verify_files
            )
        else:
            errors = validate_receipt(
                receipt,
                verify_files=verify_files,
                expected_commit=expected_commit,
                repo_root=_REPO_ROOT,
            )
    elif args.command == "validate-matrix":
        errors = validate_matrix(args.directory)
    else:
        errors = verify_run(
            args.manifest,
            evidence_root=args.evidence_root,
            expected_candidate_commit=args.expect_candidate,
        )
    if errors:
        for error in errors:
            print(f"v050_receipt: {error}", file=sys.stderr)
        return 1
    print("valid")
    return 0


def main(argv: list[str] | None = None) -> int:
    try:
        return _main(argv)
    except HarnessError as exc:
        print(f"v050_receipt: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
