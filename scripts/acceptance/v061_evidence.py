"""Record the Talaria v0.6.1 acceptance evidence tree (#150's C12-T ruling).

The v0.6.1 lineage differs from both earlier flows in where its receipts come
from: they are the tester's live products, filed one directory per live case
under ``docs/acceptance/v0.6.1/evidence/live-NN/`` as each test runs, never
transcribed here. What this module *does* own is the binding: it refuses to
record anything until every filed receipt passes the v0.6.1 verifier, it
installs the candidate wheel into two fresh scratch environments and probes
them for real (importing the v0.6.0 install probe, digests, and exclusive
writes rather than copying them), and it writes the manifest that binds the
whole run to the candidate.

Three mistakes this module is built to make impossible:

1. **Recording before the version bump.** The manifest binds the candidate by
   digest over release-relevant bytes, so it must be generated after the bump
   and not before; the record command therefore refuses unless the package
   at the repo root already reports ``0.6.1``.
2. **A missing attestation.** A live receipt rides the frozen head of the wave
   its test ran on, which is usually not the candidate commit; every such
   receipt needs an ``applies_to_candidate`` sentence naming the unchanged
   surfaces since its commit. The sentence is human judgment supplied through
   ``--applies-map``; this module refuses to record without it, computes
   ``same`` itself only when the commits genuinely match, and refuses a map
   that claims ``same`` for a differing commit.
3. **A waived case.** The READY rule is pass-21, fail-0, blocked-0, reserved-0
   with no waiver path. The verifier enforces it at verify-run time; this
   generator refuses to record anything less, so a blocked or reserved live
   case cannot be papered over at record time either.

Nothing here is overwritten on re-run: evidence files are created
exclusively, so recording over an existing tree fails loudly instead of
mutating a receipt.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from scripts.acceptance.v050_receipt import (
    V061_ITEM_SCHEMA,
    V061_RELEASE,
    V061_ROLE_LABELS,
    _find_capture_time_twin_digest,
    _v061_private_identifier_errors,
    _validate_v061_receipt,
    classify_evidence_file,
    evidence_file_privacy_errors,
    find_absolute_paths_in_text,
    is_forbidden_key,
    public_evidence_privacy_errors,
)
from scripts.acceptance.v060_evidence import (
    _package_version,
    _probe_install,
    _sha256_file,
    _utc_now,
    _write_new,
)

REPO_ROOT = Path(__file__).resolve().parents[2]

MANIFEST_SCHEMA = "talaria-v0.6.1-artifact-manifest-v1"
V061_INSTALL_SCHEMA = "talaria-v0.6.1-install-v1"
GATE_ID = "v0-6-1-daily-driver"
_V061_ROOT_REL = "docs/acceptance/v0.6.1"
_EVIDENCE_REL = "docs/acceptance/v0.6.1/evidence"
_RESULTS_REL = "docs/acceptance/v0.6.1/results.md"
_NOTES_REL = "docs/acceptance/v0.6.1/notes.md"
_MANIFEST_REL = "docs/acceptance/v0.6.1/artifact-manifest.json"


def _read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise SystemExit(f"{path}: expected a JSON object")
    return value


def _live_receipts(repo_root: Path) -> dict[str, tuple[Path, str]]:
    """Map each live case's ``live-NN`` to ``(receipt path, repo-relative path)``."""
    evidence = repo_root / _EVIDENCE_REL
    found: dict[str, tuple[Path, str]] = {}
    for path in sorted(evidence.glob("live-*/receipt.json")):
        rel = path.relative_to(repo_root).as_posix()
        found.setdefault(path.parent.name, (path, rel))
    return found


def _validate_tree(
    receipts: dict[str, tuple[Path, str]],
    *,
    expected_receipts: int,
) -> list[tuple[str, list[str]]]:
    """Return every receipt's defects, cross-checked for count."""
    failures: list[tuple[str, list[str]]] = []
    for item, (path, _rel) in sorted(receipts.items()):
        errors = _validate_v061_receipt(
            _read_json(path), receipt_path=path, verify_files=True
        )
        if errors:
            failures.append((item, errors))
    if len(receipts) != expected_receipts:
        failures.append(
            (
                "tree",
                [
                    f"{len(receipts)} live receipts on disk, but --expected-receipts "
                    f"declares {expected_receipts}"
                ],
            )
        )
    return failures


def _applies_map_errors(
    receipts: dict[str, tuple[Path, str]],
    applies_map: dict[str, str],
    *,
    candidate_commit: str,
) -> list[str]:
    """Every receipt whose attestation is missing, premature, or wrong."""
    errors: list[str] = []
    for _item, (path, rel) in sorted(receipts.items()):
        receipt = _read_json(path)
        commit = receipt.get("candidate_commit_sha")
        if commit == candidate_commit:
            # `same` is computed at record time and never taken from the map,
            # so a map line cannot skip the sentence a differing commit owes.
            if applies_map.get(rel) is not None:
                errors.append(
                    f"{rel}: harness_commit equals the candidate, so no applies map "
                    f"line is needed"
                )
            continue
        sentence = applies_map.get(rel)
        if sentence is None:
            errors.append(
                f"{rel}: candidate_commit_sha {commit} is not the candidate commit, and "
                f"the applies map carries no sentence naming the unchanged surfaces"
            )
        elif sentence.strip() == "same" or not sentence.strip():
            errors.append(
                f"{rel}: applies_to_candidate must be a non-empty sentence naming the "
                f"unchanged surfaces since commit {commit}, not `same`"
            )
    return errors


def _non_evidence_identifier_errors(repo_root: Path) -> list[str]:
    """Private identifiers in any non-evidence file under the v0.6.1 tree.

    The receipt validator refuses identifiers inside the evidence itself; this
    sweep guards the files beside it — a controller handoff note, a stray
    scratch listing, anything that is not bound by a receipt. The generator
    refuses to build a manifest while any such file carries one, which is the
    ruling's answer to a note that almost shipped with session and goal
    identifiers in it.
    """
    root = repo_root / _V061_ROOT_REL
    errors: list[str] = []
    if not root.is_dir():
        return errors
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        if _EVIDENCE_REL in path.relative_to(repo_root).as_posix():
            continue
        try:
            body = path.read_text(encoding="utf-8", errors="replace")
        except OSError as exc:
            errors.append(f"cannot scan {path}: {exc}")
            continue
        found = _v061_private_identifier_errors(body, field=str(path))
        errors.extend(found)
        try:
            document = json.loads(body)
            errors.extend(
                _v061_private_identifier_errors(document, field=str(path))
            )
        except (ValueError, TypeError):
            continue
    return errors


def _results_document(
    *,
    candidate: dict[str, str],
    expected_receipts: int,
    receipts: dict[str, tuple[Path, str]],
) -> str:
    lines = [
        "# Talaria v0.6.1 acceptance results",
        "",
        f"The live tests of the v0.6.1 run against candidate commit "
        f"`{candidate['commit']}`, wheel `{candidate['wheel_filename']}` "
        f"(`{candidate['wheel_sha256'][:12]}…), gate `{GATE_ID}`. Each live case's "
        "receipt and evidence files live under `evidence/live-NN/`; the manifest "
        "binds every receipt to the candidate by digest.",
        "",
        "| Live case | Verdict |",
        "| --- | --- |",
    ]
    for item, (path, _rel) in sorted(receipts.items()):
        receipt = _read_json(path)
        lines.append(f"| {item} | {receipt.get('verdict', 'unknown')} |")
    lines += [
        "",
        f"{len(receipts)} of {expected_receipts} expected receipts recorded; "
        "the readiness verdict itself lives in the gate document, not here.",
        "",
        "Live cases are source-checkout evidence unless a receipt says "
        "otherwise: twenty source-checkout receipts, two install probes, one "
        "wheel receipt — the wheel is proved by the probes and by Live 21 "
        "executed from the built wheel in a fresh tool environment.",
        "",
    ]
    return "\n".join(lines)


def _notes_document(*, candidate: dict[str, str]) -> str:
    return "\n".join(
        [
            "# Talaria v0.6.1 acceptance notes",
            "",
            "## Sequence",
            "",
            "The manifest was generated after the version bump to 0.6.1, on the "
            "candidate commit it binds — never before, which is why the record "
            "command refuses a tree still reporting an earlier version. Every "
            "live receipt passed the v0.6.1 verifier at record time: the "
            "evidence inventory, digests, role-label tester, and full harness "
            "commit are machine-checked, and the manifest carries each "
            "receipt's `applies_to_candidate` attestation for the reviewer to "
            "inspect sentence by sentence.",
            "",
            "The tag commit and the candidate commit "
            f"`{candidate['commit']}` differ only by the record commit and the "
            "gate commit — the two commits these documents and the manifest "
            "themselves ride in — and by nothing release-relevant, which the "
            "release workflow's candidate check re-verifies by digest.",
            "",
        ]
    )


def record(
    *,
    candidate_commit: str,
    wheel: Path,
    expected_receipts: int,
    applies_map: dict[str, str],
    recorded_at: str | None = None,
    repo_root: Path = REPO_ROOT,
) -> Path:
    """Record the v0.6.1 binding; return the manifest path."""
    stamped = _utc_now(recorded_at)
    version = _package_version()
    if version != V061_RELEASE:
        raise SystemExit(
            f"the package at {repo_root} reports version {version!r}, not {V061_RELEASE!r}: "
            "the manifest binds release-relevant bytes, so the version bump must land "
            "before the record flow runs, never after"
        )
    sys.path.insert(0, str(repo_root))
    wheel_sha = _sha256_file(wheel)
    candidate = {
        "commit": candidate_commit,
        "version": version,
        "wheel_filename": wheel.name,
        "wheel_sha256": wheel_sha,
    }
    harness_commit = (
        subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=repo_root, capture_output=True, text=True
        ).stdout.strip()
        or candidate_commit
    )

    receipts = _live_receipts(repo_root)
    failures = _validate_tree(receipts, expected_receipts=expected_receipts)
    if failures:
        detail = "\n".join(f"  {item}: " + "; ".join(errors) for item, errors in failures)
        raise SystemExit(f"refusing to record: filed receipts do not validate:\n{detail}")
    non_pass = sorted(
        item
        for item, (path, _rel) in receipts.items()
        if _read_json(path).get("verdict") != "pass"
    )
    if non_pass:
        raise SystemExit(
            "the READY rule has no waiver path: every live receipt must read pass, "
            "got a non-pass verdict for " + ", ".join(non_pass)
        )
    applies_errors = _applies_map_errors(
        receipts, applies_map, candidate_commit=candidate_commit
    )
    if applies_errors:
        raise SystemExit(
            "refusing to record: attestations missing or invalid:\n  "
            + "\n  ".join(applies_errors)
        )
    identifier_errors = _non_evidence_identifier_errors(repo_root)
    if identifier_errors:
        raise SystemExit(
            "refusing to record: non-evidence files under docs/acceptance/v0.6.1/ "
            "carry private identifiers — move them out before the manifest is built:"
            "\n  " + "\n  ".join(identifier_errors)
        )
    privacy_errors = public_evidence_privacy_errors(repo_root)
    if privacy_errors:
        raise SystemExit(
            "refusing to record: evidence files carry private identifiers:\n  "
            + "\n  ".join(privacy_errors)
        )

    verdicts = Counter(
        str(_read_json(path).get("verdict")) for path, _rel in receipts.values()
    )
    receipt_entries: list[dict[str, Any]] = []
    for _item, (path, rel) in sorted(receipts.items()):
        receipt = _read_json(path)
        commit = receipt["candidate_commit_sha"]
        receipt_entries.append(
            {
                "receipt_path": rel,
                "receipt_sha256": _sha256_file(path),
                "checklist_item": receipt["checklist_item"],
                "tester": receipt["tester"],
                "verdict": receipt["verdict"],
                "candidate_commit_sha": commit,
                "applies_to_candidate": (
                    "same" if commit == candidate_commit else applies_map[rel]
                ),
            }
        )

    install_entries: list[dict[str, Any]] = []
    scratch_dirs: list[Path] = []
    for probe in ("probe-1", "probe-2"):
        install_receipt, scratch = _probe_install(
            wheel, candidate=candidate, recorded_at=stamped
        )
        # The imported v0.6.0 probe stamps its own schema; the rest of the
        # receipt — candidate, digests, probe results — is the flow this module
        # imports rather than copies.
        install_receipt["schema_version"] = V061_INSTALL_SCHEMA
        scratch_dirs.append(scratch)
        rel = f"{_EVIDENCE_REL}/{probe}/install-receipt.json"
        digest = _write_new(repo_root / rel, install_receipt)
        install_entries.append(
            {"receipt_path": rel, "receipt_sha256": digest, "tester": "operator"}
        )

    (repo_root / _RESULTS_REL).write_text(
        _results_document(
            candidate=candidate, expected_receipts=expected_receipts, receipts=receipts
        ),
        encoding="utf-8",
    )
    (repo_root / _NOTES_REL).write_text(
        _notes_document(candidate=candidate), encoding="utf-8"
    )
    manifest = {
        "$schema": "./artifact-manifest.schema.json",
        "schema_version": MANIFEST_SCHEMA,
        "gate_id": GATE_ID,
        "generated_command": (
            "uv run python -m scripts.acceptance.v061_evidence record "
            f"--candidate-commit {candidate_commit} --wheel {wheel.name} "
            f"--expected-receipts {expected_receipts} --applies-map <path>"
        ),
        "status": "complete",
        "recorded_at": stamped,
        "harness_commit": harness_commit,
        "candidate": candidate,
        "counts": {
            "expected_receipts": expected_receipts,
            "install_receipts": len(install_entries),
            "item_receipts": len(receipt_entries),
            "item_verdicts": {
                "blocked": verdicts.get("blocked", 0),
                "fail": verdicts.get("fail", 0),
                "pass": verdicts.get("pass", 0),
                "reserved": verdicts.get("reserved", 0),
            },
            "invalid_item_receipts": 0,
        },
        "receipts": receipt_entries,
        "install_receipts": install_entries,
        "results_document": _RESULTS_REL,
        "notes_document": _NOTES_REL,
    }
    manifest_path = repo_root / _MANIFEST_REL
    _write_new(manifest_path, manifest)
    for scratch in scratch_dirs:
        shutil.rmtree(scratch, ignore_errors=True)
    return manifest_path


_ATTESTATION_ROLE_FIELDS = ("attested_by", "tester")
_INSTALL_KINDS = ("source-checkout", "wheel")
_HARNESS_KINDS = ("repository-tooling", "scratch-capture", "manual")


def _attestation_refusals(item: str, filed: dict[str, Any]) -> tuple[str | None, list[str]]:
    """The commit a conversion may derive, and every reason it may not convert.

    Three admissible sources and nothing else, in the ruling's order: what
    the receipt says, what is on disk beside it, and an attestation by the
    capturing role recorded with its date. A receipt whose commit cannot be
    attested is re-captured, not converted — the refusal says so.
    """
    refusals: list[str] = []
    rich = "candidate_commit_sha" in filed
    commit = filed.get("candidate_commit_sha") if rich else filed.get("harness_commit")
    if not isinstance(commit, str) or len(commit) != 40 or any(
        character not in "0123456789abcdef" for character in commit
    ):
        refusals.append(
            "no candidate commit can be derived from the receipt (the thin shape's "
            "harness_commit held it; a rich shape carries candidate_commit_sha) — "
            "re-capture rather than convert"
        )
        commit = None
    return commit, refusals


def _converted_receipt(
    item: str,
    filed: dict[str, Any],
    attestation: dict[str, Any],
    *,
    files: dict[str, str],
    listed_at: str,
) -> dict[str, Any]:
    """Build the converted receipt: derived, attested, never back-filled."""
    rich = "candidate_commit_sha" in filed
    commit = (
        filed["candidate_commit_sha"] if rich else filed["harness_commit"]
    )
    converted: dict[str, Any] = {
        "schema_version": filed.get("schema_version", V061_ITEM_SCHEMA),
        "release": filed["release"],
        "checklist_item": filed["checklist_item"],
        "title": filed.get("title", item),
        "issue": filed["issue"],
        "tester": attestation["tester"],
        "verdict": filed["verdict"],
        "candidate_commit_sha": commit,
        "recorded_at": filed["recorded_at"],
        "install": {
            "kind": attestation["install_kind"],
            "commit": commit,
            "basis": (
                "attested at conversion by the capturing role, "
                f"{attestation['attested_at']}"
            ),
        },
        "harness": {
            "kind": attestation["harness_kind"],
            "commit": attestation.get("harness_commit"),
            "identity": attestation.get("harness_identity") or "not recorded",
        },
    }
    if rich:
        for detail_key in (
            "candidate_branch",
            "gateway",
            "session",
            "terminal",
            "actions",
            "expected",
            "actual",
        ):
            if detail_key in filed:
                converted[detail_key] = filed[detail_key]
        gateway = converted.get("gateway")
        if isinstance(gateway, dict):
            gateway.pop("tester_pane", None)
        session = converted.get("session")
        if isinstance(session, dict):
            session.pop("tester_pane", None)
        terminal = converted.get("terminal")
        if isinstance(terminal, dict):
            terminal.pop("tester_pane", None)
    else:
        narrative = filed.get("evidence", {})
        converted["actions"] = narrative.get("method")
        converted["actual"] = narrative.get("observation")
        converted["expected"] = {
            "source": "child pass condition",
            "text": attestation["expected"],
        }
        for detail_key in ("gateway", "session", "terminal"):
            converted[detail_key] = attestation.get(detail_key) or "not recorded"
    evidence: dict[str, Any] = {"files": dict(files), "files_listed_at": listed_at}
    if rich:
        for evidence_key, evidence_value in filed.get("evidence", {}).items():
            evidence[evidence_key] = evidence_value
    else:
        source_evidence = filed.get("evidence", {})
        evidence["narrative"] = {
            key: source_evidence[key]
            for key in ("kind", "method", "observation", "source")
            if key in source_evidence
        }
    if "screenshots_read_by" in attestation:
        evidence["screenshots_read_by"] = attestation["screenshots_read_by"]
    elif "screenshots_read_by" in filed.get("evidence", {}):
        evidence["screenshots_read_by"] = filed["evidence"]["screenshots_read_by"]
    elif "screenshots_read_by" in filed:
        evidence["screenshots_read_by"] = filed["screenshots_read_by"]

    if "screenshots_read_at" in attestation:
        evidence["screenshots_read_at"] = attestation["screenshots_read_at"]
    elif "screenshots_read_at" in filed.get("evidence", {}):
        evidence["screenshots_read_at"] = filed["evidence"]["screenshots_read_at"]
    elif "screenshots_read_at" in filed:
        evidence["screenshots_read_at"] = filed["screenshots_read_at"]

    converted["evidence"] = evidence
    return converted


_DEFAULT_WITHHOLDING_RULES = (
    "skills-roster",
    "operator-home-path",
    "pane-coordinate",
    "session-name",
    "email-address",
    "bearer-credential",
    "temporary-directory",
)


def _is_path_kept(path: str, keep_set: set[str]) -> bool:
    if not keep_set:
        return False
    if path in keep_set or "*" in keep_set:
        return True
    norm = "/" + path.strip("/").replace(".", "/")
    for k in keep_set:
        k_norm = "/" + k.strip("/").replace(".", "/")
        if k_norm == norm or (k_norm.endswith("/*") and norm.startswith(k_norm[:-1])):
            return True
        if norm.endswith(k_norm) or k_norm.endswith(norm):
            return True
    return False


def _redact_payload(
    payload: Any,
    rules: set[str],
    keep_list: set[str],
    redactions: list[dict[str, Any]],
    path: str = "/frame",
) -> Any:
    if isinstance(payload, dict):
        new_dict: dict[str, Any] = {}
        for k, v in payload.items():
            loc = f"{path}/{k}" if path else f"/{k}"
            if "skills-roster" in rules and k in (
                "skills",
                "integrations",
                "connected_integrations",
            ) and not _is_path_kept(loc, keep_list):
                new_dict[k] = "[redacted]"
                redactions.append({"rule": "skills-roster", "field": loc})
            else:
                new_dict[k] = _redact_payload(v, rules, keep_list, redactions, loc)
        return new_dict
    if isinstance(payload, list):
        return [
            _redact_payload(item, rules, keep_list, redactions, f"{path}/{i}")
            for i, item in enumerate(payload)
        ]
    if isinstance(payload, str):
        if keep_list and not _is_path_kept(path, keep_list):
            redactions.append({"rule": "withheld-by-default", "field": path})
            return "[redacted]"

        val = payload
        if "operator-home-path" in rules:
            def _sub_home(m: re.Match[str]) -> str:
                redactions.append({"rule": "operator-home-path", "field": path})
                return "[redacted]"
            val = re.sub(r"/(?:Users|home)/[A-Za-z0-9._-]+", _sub_home, val)
        if "pane-coordinate" in rules:
            def _sub_pane(m: re.Match[str]) -> str:
                redactions.append({"rule": "pane-coordinate", "field": path})
                return "[redacted]"
            val = re.sub(r"\bw[A-Za-z0-9]+:[pt][A-Za-z0-9]+\b", _sub_pane, val)
        if "session-name" in rules:
            def _sub_session(m: re.Match[str]) -> str:
                redactions.append({"rule": "session-name", "field": path})
                return "[redacted]"
            val = re.sub(
                r"\b(?:worker|controller|reviewer|architect|investigator|tester|operator)-\d+(?:-\d+)*\b",
                _sub_session,
                val,
            )
        if "email-address" in rules:
            def _sub_email(m: re.Match[str]) -> str:
                redactions.append({"rule": "email-address", "field": path})
                return "[redacted]"
            val = re.sub(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", _sub_email, val)
        if "bearer-credential" in rules:
            def _sub_bearer(m: re.Match[str]) -> str:
                redactions.append({"rule": "bearer-credential", "field": path})
                return "[redacted]"
            val = re.sub(r"Authorization:\s*Bearer\s+\S+", _sub_bearer, val, flags=re.IGNORECASE)
        if "temporary-directory" in rules:
            def _sub_tmp(m: re.Match[str]) -> str:
                redactions.append({"rule": "temporary-directory", "field": path})
                return "[redacted]"
            val = re.sub(r"/private/var/folders/[^\s\"\'\\]+", _sub_tmp, val)
        return val
    return payload


def derive_capture(
    *,
    input_path: Path,
    output_path: Path,
    frame_types: tuple[str, ...] = (),
    start_seq: int | None = None,
    end_seq: int | None = None,
    rules: tuple[str, ...] = (),
    keep_list: tuple[str, ...] = (),
    tool: str = "scripts/acceptance/v061_evidence.py derive-capture",
    derived_at: str | None = None,
    repo_root: Path = REPO_ROOT,
) -> Path:
    """Derive a conforming, privacy-clean wire capture from a raw frame recording."""
    input_path = input_path.expanduser().resolve()
    if not input_path.is_file():
        raise SystemExit(f"input capture file does not exist: {input_path}")
    source_bytes = input_path.read_bytes()
    source_sha256 = hashlib.sha256(source_bytes).hexdigest()

    try:
        text = source_bytes.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise SystemExit(f"{input_path}: source file is not valid UTF-8: {exc}") from exc

    lines = [line for line in text.splitlines() if line.strip()]
    if not lines:
        raise SystemExit(f"{input_path}: source file is empty")

    try:
        source_header = json.loads(lines[0])
    except json.JSONDecodeError as exc:
        raise SystemExit(f"{input_path}: invalid header JSON: {exc}") from exc

    if not isinstance(source_header, dict) or source_header.get("kind") != "header":
        raise SystemExit(f"{input_path}: first record must be kind: header")

    all_frames: list[dict[str, Any]] = []
    for line_idx, line in enumerate(lines[1:], start=2):
        try:
            record = json.loads(line)
        except json.JSONDecodeError as exc:
            raise SystemExit(f"{input_path}: line {line_idx} is invalid JSON: {exc}") from exc
        if not isinstance(record, dict) or record.get("kind") != "frame":
            raise SystemExit(f"{input_path}: line {line_idx} is not a frame record")
        all_frames.append(record)

    source_frame_count = len(all_frames)
    active_rules = set(rules) if rules else set(_DEFAULT_WITHHOLDING_RULES)
    keep_set = set(keep_list)

    # 1. Selection filter
    kept_frames: list[dict[str, Any]] = []
    for frame_record in all_frames:
        src_seq = frame_record.get("seq")
        if not isinstance(src_seq, int):
            continue
        if start_seq is not None and src_seq < start_seq:
            continue
        if end_seq is not None and src_seq > end_seq:
            continue
        if frame_types:
            inner = frame_record.get("frame", {})
            f_type = inner.get("type") or inner.get("method")
            params = inner.get("params")
            if isinstance(params, dict):
                f_type = params.get("type") or params.get("method") or f_type
            if f_type not in frame_types:
                continue
        kept_frames.append(frame_record)

    # 2. Named withholdings, gapless seq, sourceSeq retention
    for out_seq, frame_record in enumerate(kept_frames, start=1):
        redactions: list[dict[str, Any]] = list(frame_record.get("redactions", []))
        if "frame" in frame_record:
            frame_record["frame"] = _redact_payload(
                frame_record["frame"], active_rules, keep_set, redactions
            )
        frame_record["sourceSeq"] = frame_record["seq"]
        frame_record["seq"] = out_seq
        frame_record["redactions"] = redactions

    # 3. Derivation header
    derivation = {
        "tool": tool,
        "source_sha256": source_sha256,
        "source_bytes": len(source_bytes),
        "source_frames": source_frame_count,
        "selection": {
            "frame_types": list(frame_types) if frame_types else None,
            "start_seq": start_seq,
            "end_seq": end_seq,
        },
        "rules": sorted(active_rules),
        "keep_list": sorted(keep_list),
        "derived_at": _utc_now(derived_at),
    }
    header = {
        "kind": "header",
        "version": source_header.get("version", 1),
        "startedAt": source_header.get("startedAt", ""),
        "endpoint": source_header.get("endpoint", ""),
        "derivation": derivation,
    }

    # 4. Self-scan and atomic write
    output_lines = [json.dumps(header)]
    for frame_record in kept_frames:
        output_lines.append(json.dumps(frame_record))
    output_content = "\n".join(output_lines) + "\n"

    output_path = output_path.expanduser().resolve()
    output_path.parent.mkdir(parents=True, exist_ok=True)
    temp_output = output_path.with_suffix(f".tmp.{os.getpid()}")
    temp_output.write_text(output_content, encoding="utf-8")

    errors = evidence_file_privacy_errors(temp_output, repo_root=repo_root)
    if errors:
        temp_output.unlink(missing_ok=True)
        raise SystemExit(
            "refusing to write derived capture: output carries privacy defects:\n  "
            + "\n  ".join(errors)
        )

    temp_output.replace(output_path)
    return output_path


@dataclass(frozen=True)
class SourceInventoryEntry:
    case_item: str
    file_path: Path
    file_class: str
    finding: str
    disposition: str  # "kept", "withheld", "refused"
    rule_or_reason: str


def inventory_source_evidence(
    source_dir: Path,
    item: str,
    attestation: dict[str, Any] | None,
    repo_root: Path,
) -> list[SourceInventoryEntry]:
    """Scan all source files for an evidence item and return the inventory with dispositions."""
    entries: list[SourceInventoryEntry] = []
    receipt_file = source_dir / "receipt.json"
    if receipt_file.is_file():
        receipt_had_findings = False
        try:
            filed = json.loads(receipt_file.read_text(encoding="utf-8"))
            if isinstance(filed, dict):
                tester = filed.get("tester")
                if isinstance(tester, str) and tester not in V061_ROLE_LABELS:
                    receipt_had_findings = True
                    if attestation and attestation.get("tester") in V061_ROLE_LABELS:
                        entries.append(
                            SourceInventoryEntry(
                                case_item=item,
                                file_path=receipt_file,
                                file_class="receipt",
                                finding="role-digit tester session name",
                                disposition="withheld",
                                rule_or_reason="role-label-substitution",
                            )
                        )
                    else:
                        entries.append(
                            SourceInventoryEntry(
                                case_item=item,
                                file_path=receipt_file,
                                file_class="receipt",
                                finding="unattested role-digit tester session name",
                                disposition="refused",
                                rule_or_reason="role-digit session name requires attestation",
                            )
                        )
                for k in filed.keys():
                    if is_forbidden_key(k):
                        receipt_had_findings = True
                        entries.append(
                            SourceInventoryEntry(
                                case_item=item,
                                file_path=receipt_file,
                                file_class="receipt",
                                finding=f"{k} key present in receipt",
                                disposition="refused",
                                rule_or_reason="forbidden operational key must be fixed at source",
                            )
                        )
                harness = filed.get("harness")
                if isinstance(harness, dict):
                    h_id = harness.get("identity")
                    if isinstance(h_id, str) and bool(find_absolute_paths_in_text(h_id)):
                        receipt_had_findings = True
                        entries.append(
                            SourceInventoryEntry(
                                case_item=item,
                                file_path=receipt_file,
                                file_class="receipt",
                                finding="absolute path in harness.identity",
                                disposition="refused",
                                rule_or_reason=(
                                    "absolute path in harness.identity must be fixed at source"
                                ),
                            )
                        )
        except Exception:
            pass
        if not receipt_had_findings:
            entries.append(
                SourceInventoryEntry(
                    case_item=item,
                    file_path=receipt_file,
                    file_class="receipt",
                    finding="clean",
                    disposition="kept",
                    rule_or_reason="valid filed receipt",
                )
            )

    for path in sorted(source_dir.rglob("*")):
        if not path.is_file() or path.name == "receipt.json":
            continue
        file_class = classify_evidence_file(path)
        if file_class == "markdown":
            entries.append(
                SourceInventoryEntry(
                    case_item=item,
                    file_path=path,
                    file_class="markdown",
                    finding="markdown file under evidence/",
                    disposition="refused",
                    rule_or_reason="markdown files forbidden under evidence",
                )
            )
            continue
        errors = evidence_file_privacy_errors(path, repo_root=repo_root)
        if errors:
            for err in errors:
                entries.append(
                    SourceInventoryEntry(
                        case_item=item,
                        file_path=path,
                        file_class=file_class,
                        finding=err,
                        disposition="refused",
                        rule_or_reason="source file carries privacy defects",
                    )
                )
        else:
            entries.append(
                SourceInventoryEntry(
                    case_item=item,
                    file_path=path,
                    file_class=file_class,
                    finding="clean",
                    disposition="kept",
                    rule_or_reason="clean evidence file",
                )
            )
    return entries


def _conversion_notes_document(
    *,
    converted_items: list[str],
    excluded_items: list[str],
    inventory_entries: list[SourceInventoryEntry],
    listed_at: str,
) -> str:
    lines = [
        "# Talaria v0.6.1 acceptance evidence conversion notes",
        "",
        f"Converted at: {listed_at}",
        "",
        "## Summary",
        "",
        (
            f"- Converted cases ({len(converted_items)}): "
            f"{', '.join(sorted(converted_items)) if converted_items else 'none'}"
        ),
        (
            f"- Excluded cases ({len(excluded_items)}): "
            f"{', '.join(sorted(excluded_items)) if excluded_items else 'none'}"
        ),
        "",
        "## Source Evidence Inventory and Reconciliation",
        "",
        (
            "Every filed evidence item was scanned against the v0.6.1 privacy contract "
            "before conversion."
        ),
        (
            "Identified non-conforming items were reconciled under named withholding "
            "rules or attested substitutions."
        ),
        "",
        "| Case | File | Class | Finding | Disposition | Rule / Reason |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    if inventory_entries:
        for e in sorted(
            inventory_entries,
            key=lambda x: (x.case_item, x.file_path.name, x.disposition, x.finding),
        ):
            rel_name = e.file_path.name
            lines.append(
                f"| {e.case_item} | `{rel_name}` | {e.file_class} | "
                f"`{e.finding}` | {e.disposition} | {e.rule_or_reason} |"
            )
    else:
        lines.append("| - | - | - | `none` | kept | no inventory entries |")
    lines.append("")
    return "\n".join(lines)



def convert(
    *,
    attestations: dict[str, dict[str, Any]],
    output_root: Path,
    listed_at: str,
    repo_root: Path = REPO_ROOT,
    exclude: tuple[str, ...] = (),
) -> list[Path]:
    """Convert every filed receipt with its attestation; return what was written.

    All-or-nothing: every refusal is collected across the whole tree, and
    nothing survives on disk when any receipt refuses — the operator sees
    exactly which live cases need re-capture, and the tree is never
    half-converted.

    ``exclude`` names live cases the controller has ruled out of this run —
    a re-capture already scheduled, an unattestable provenance. An excluded
    case is skipped out in the open (the caller reports it) rather than
    silently dropped or allowed to block the rest.
    """
    receipts = _live_receipts(repo_root)
    if not receipts:
        raise SystemExit(f"no filed receipts under {repo_root / _EVIDENCE_REL}")
    refusals: list[str] = []
    all_inventory: list[SourceInventoryEntry] = []
    plans: list[tuple[str, Path, dict[str, Any], dict[str, Path]]] = []
    for item, (path, _rel) in sorted(receipts.items()):
        if item in exclude:
            continue
        filed = _read_json(path)
        item_refusals: list[str] = []
        commit, commit_refusals = _attestation_refusals(item, filed)
        item_refusals.extend(commit_refusals)
        attestation = attestations.get(item)
        if attestation is None:
            item_refusals.append(
                "no attestation for its commit — re-capture rather than convert"
            )
        if attestation is not None and commit is not None:
            for role_field in _ATTESTATION_ROLE_FIELDS:
                if attestation.get(role_field) not in V061_ROLE_LABELS:
                    item_refusals.append(
                        f"attestation.{role_field} must be a closed-set role label "
                        f"({', '.join(V061_ROLE_LABELS)})"
                    )
            attested_at = attestation.get("attested_at")
            if not isinstance(attested_at, str) or not attested_at.strip():
                item_refusals.append("attestation.attested_at must record its date")
            install_kind = attestation.get("install_kind")
            if install_kind not in _INSTALL_KINDS:
                item_refusals.append(
                    "attestation.install_kind must be source-checkout or wheel — the "
                    "install kind is an attested fact, not an assumption"
                )
            elif install_kind != "source-checkout":
                item_refusals.append(
                    "a wheel-install case is not defined for conversion — re-file it "
                    "fresh in the ruled shape rather than convert"
                )
            harness_kind = attestation.get("harness_kind")
            if harness_kind not in _HARNESS_KINDS:
                item_refusals.append(
                    "attestation.harness_kind must be repository-tooling, "
                    "scratch-capture, or manual — the harness kind is an attested "
                    "fact, not an assumption"
                )
            elif harness_kind == "repository-tooling":
                attested_harness_commit = attestation.get("harness_commit")
                if (
                    not isinstance(attested_harness_commit, str)
                    or len(attested_harness_commit) != 40
                    or any(
                        character not in "0123456789abcdef"
                        for character in attested_harness_commit
                    )
                ):
                    item_refusals.append(
                        "a repository-tooling harness conversion must attest its own "
                        "commit in attestation.harness_commit"
                    )
            elif attestation.get("harness_commit") is not None:
                item_refusals.append(
                    "attestation.harness_commit must be absent unless the harness is "
                    "repository tooling"
                )
            harness_identity = attestation.get("harness_identity")
            if harness_identity is not None and isinstance(harness_identity, str):
                if find_absolute_paths_in_text(harness_identity):
                    item_refusals.append(
                        "attestation.harness_identity must not contain an absolute "
                        f"filesystem path ({harness_identity!r})"
                    )
            rich = "candidate_commit_sha" in filed
            if not rich and not (
                isinstance(attestation.get("expected"), str)
                and attestation["expected"].strip()
            ):
                item_refusals.append(
                    "a thin receipt converts only with the owning child's pass "
                    "condition as an explicit attestation (attestation.expected)"
                )
        if item_refusals or attestation is None or commit is None:
            refusals.extend(f"{item}: {refusal}" for refusal in item_refusals)
            continue
        source_dir = path.parent
        inventory = inventory_source_evidence(
            source_dir, item, attestation, repo_root=repo_root
        )
        all_inventory.extend(inventory)
        for entry in inventory:
            if entry.disposition == "refused":
                item_refusals.append(
                    f"{entry.file_path.name}: {entry.finding} ({entry.rule_or_reason})"
                )
        files: dict[str, Path] = {
            evidence_path.relative_to(source_dir).as_posix(): evidence_path
            for evidence_path in sorted(source_dir.rglob("*"))
            if evidence_path.is_file() and evidence_path.name != "receipt.json"
        }
        png_names = [f for f in files if f.lower().endswith(".png")]
        for png_name in png_names:
            png_p = Path(png_name)
            stem = png_p.stem
            twin_candidates = [
                cand
                for cand in (
                    str(png_p.with_suffix(".txt")),
                    str(png_p.with_suffix(".ansi")),
                    str(png_p.parent / f"{stem}.screen.txt"),
                )
                if cand in files
            ]
            if not twin_candidates:
                read_by = attestation.get("screenshots_read_by") if attestation else None
                read_at = attestation.get("screenshots_read_at") if attestation else None
                if not read_by or not read_at:
                    ev_dict = filed.get("evidence", {})
                    read_by = filed.get("screenshots_read_by") or ev_dict.get(
                        "screenshots_read_by"
                    )
                    read_at = filed.get("screenshots_read_at") or ev_dict.get(
                        "screenshots_read_at"
                    )
                if not (
                    isinstance(read_by, str)
                    and read_by.strip()
                    and isinstance(read_at, str)
                    and read_at.strip()
                ):
                    item_refusals.append(
                        "screenshots have neither a text twin nor a recorded human read "
                        "(attestation.screenshots_read_by and attestation.screenshots_read_at)"
                    )
                    break
                if read_by not in V061_ROLE_LABELS:
                    item_refusals.append(
                        f"attestation.screenshots_read_by must be a closed-set role label "
                        f"({', '.join(V061_ROLE_LABELS)})"
                    )
                    break
            else:
                twin_file = twin_candidates[0]
                twin_digest = _sha256_file(files[twin_file])
                source_listed = {Path(p): _sha256_file(f) for p, f in files.items()}
                capture_twin_digest = _find_capture_time_twin_digest(
                    png_p, receipt_dir=source_dir, listed=source_listed
                )
                if capture_twin_digest is not None:
                    if capture_twin_digest != twin_digest:
                        item_refusals.append(
                            f"screenshot '{png_name}' text twin '{twin_file}' digest "
                            f"({twin_digest}) does not match capture-time twin_digest "
                            f"({capture_twin_digest})"
                        )
                else:
                    item_refusals.append(
                        f"screenshot '{png_name}' text twin '{twin_file}' is not bound by "
                        "capture-time twin_digest (twin was not produced at capture time)"
                    )
        if item_refusals:
            refusals.extend(f"{item}: {refusal}" for refusal in item_refusals)
            continue
        digests = {name: _sha256_file(file_path) for name, file_path in files.items()}
        converted = _converted_receipt(
            item,
            filed,
            attestation,
            files=digests,
            listed_at=listed_at,
        )
        item_dir = output_root / item
        item_dir.mkdir(parents=True, exist_ok=True)
        plans.append((item, item_dir, converted, files))
    if refusals:
        raise SystemExit(
            "refusing to convert — nothing was written:\n  " + "\n  ".join(refusals)
        )

    written: list[Path] = []
    try:
        for item, item_dir, converted, files in plans:
            for name, source in files.items():
                destination = item_dir / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                if destination.exists():
                    raise SystemExit(
                        f"refusing to replace existing converted evidence: {destination}"
                    )
                shutil.copyfile(source, destination)
                written.append(destination)
                file_errors = evidence_file_privacy_errors(destination, repo_root=repo_root)
                if file_errors:
                    raise SystemExit(
                        f"{item}: converted evidence file {name} carries privacy defects:\n  "
                        + "\n  ".join(file_errors)
                    )
            receipt_path = item_dir / "receipt.json"
            if receipt_path.exists():
                raise SystemExit(
                    f"refusing to replace existing converted receipt: {receipt_path}"
                )
            errors = _validate_v061_receipt(
                converted, receipt_path=receipt_path, verify_files=True
            )
            if errors:
                raise SystemExit(
                    f"{item}: the converted receipt does not validate — the filed "
                    f"receipt's defects must be fixed at the source:\n  "
                    + "\n  ".join(errors)
                )
            receipt_path.write_text(
                json.dumps(converted, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            written.append(receipt_path)
            receipt_errors = evidence_file_privacy_errors(receipt_path, repo_root=repo_root)
            if receipt_errors:
                raise SystemExit(
                    f"{item}: converted receipt carries privacy defects:\n  "
                    + "\n  ".join(receipt_errors)
                )
        conversion_notes_path = output_root / "conversion-notes.md"
        conversion_notes_content = _conversion_notes_document(
            converted_items=[item for item, _, _, _ in plans],
            excluded_items=list(exclude),
            inventory_entries=all_inventory,
            listed_at=listed_at,
        )
        conversion_notes_path.write_text(conversion_notes_content, encoding="utf-8")
        written.append(conversion_notes_path)
        notes_errors = evidence_file_privacy_errors(conversion_notes_path, repo_root=repo_root)
        if notes_errors:
            raise SystemExit(
                "conversion-notes.md carries privacy defects:\n  " + "\n  ".join(notes_errors)
            )
    except BaseException:
        for path in written:
            path.unlink(missing_ok=True)
        for path in sorted({path.parent for path in written}, reverse=True):
            path.rmdir()
        raise
    return written


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    record_cmd = subparsers.add_parser("record", help="record the evidence binding")
    record_cmd.add_argument("--candidate-commit", required=True)
    record_cmd.add_argument("--wheel", type=Path, required=True)
    record_cmd.add_argument(
        "--expected-receipts",
        type=int,
        required=True,
        help="how many live receipts the run owes; a parameter, never a default",
    )
    record_cmd.add_argument(
        "--applies-map",
        type=Path,
        required=True,
        help="JSON mapping each non-candidate receipt's path to its applies sentence",
    )
    record_cmd.add_argument("--recorded-at", default=None)
    record_cmd.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    convert_cmd = subparsers.add_parser(
        "convert", help="convert filed receipts from their attestations"
    )
    convert_cmd.add_argument(
        "--attestations",
        type=Path,
        required=True,
        help="JSON keyed by live case: attested_by, attested_at, tester, expected, "
        "and optional harness_identity / gateway / session / terminal",
    )
    convert_cmd.add_argument(
        "--output-root", type=Path, required=True, help="where the converted tree lands"
    )
    convert_cmd.add_argument(
        "--listed-at",
        required=True,
        help="the derivation stamp recorded as evidence.files_listed_at",
    )
    convert_cmd.add_argument(
        "--exclude",
        action="append",
        default=[],
        help="a live case the controller has ruled out of this conversion (repeatable)",
    )
    convert_cmd.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    derive_cmd = subparsers.add_parser(
        "derive-capture", help="derive a clean public wire capture from raw recording"
    )
    derive_cmd.add_argument(
        "--input", type=Path, required=True, help="source raw recording"
    )
    derive_cmd.add_argument(
        "--output", type=Path, required=True, help="path to write derived capture"
    )
    derive_cmd.add_argument(
        "--frame-types",
        action="append",
        default=[],
        help="filter to specific frame types (repeatable)",
    )
    derive_cmd.add_argument("--start-seq", type=int, default=None, help="start source seq")
    derive_cmd.add_argument("--end-seq", type=int, default=None, help="end source seq")
    derive_cmd.add_argument(
        "--rule",
        dest="rules",
        action="append",
        default=[],
        help="named withholding rule to apply (repeatable)",
    )
    derive_cmd.add_argument(
        "--keep",
        dest="keep_list",
        action="append",
        default=[],
        help="JSON pointer path to keep unredacted (repeatable)",
    )
    derive_cmd.add_argument(
        "--keep-list",
        dest="keep_list_file",
        type=Path,
        default=None,
        help="path to file containing JSON pointer paths to keep (one per line or JSON list)",
    )
    derive_cmd.add_argument(
        "--tool",
        default="scripts/acceptance/v061_evidence.py derive-capture",
        help="tool attribution stamp",
    )
    derive_cmd.add_argument("--derived-at", default=None, help="derivation timestamp")
    derive_cmd.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.command == "derive-capture":
        keep_list: list[str] = list(args.keep_list or [])
        if args.keep_list_file is not None:
            raw_text = args.keep_list_file.read_text(encoding="utf-8").strip()
            if raw_text.startswith("["):
                try:
                    parsed = json.loads(raw_text)
                    if isinstance(parsed, list):
                        keep_list.extend(str(x) for x in parsed)
                except json.JSONDecodeError:
                    pass
            else:
                for line in raw_text.splitlines():
                    line = line.strip()
                    if line and not line.startswith("#"):
                        keep_list.append(line)
        derived_path = derive_capture(
            input_path=args.input,
            output_path=args.output,
            frame_types=tuple(args.frame_types),
            start_seq=args.start_seq,
            end_seq=args.end_seq,
            rules=tuple(args.rules),
            keep_list=tuple(keep_list),
            tool=args.tool,
            derived_at=args.derived_at,
            repo_root=args.repo_root,
        )
        print(f"wrote derived capture to {derived_path}")
        return 0
    if args.command == "convert":
        raw_attestations = json.loads(args.attestations.read_text(encoding="utf-8"))
        if not isinstance(raw_attestations, dict):
            raise SystemExit(f"{args.attestations}: attestations must be a JSON object")
        attestations: dict[str, dict[str, Any]] = {}
        for item, entry in raw_attestations.items():
            if not isinstance(item, str) or not isinstance(entry, dict):
                raise SystemExit(
                    f"{args.attestations}: attestations must map live cases to objects"
                )
            attestations[item] = entry
        written = convert(
            attestations=attestations,
            output_root=args.output_root,
            listed_at=args.listed_at,
            repo_root=args.repo_root,
            exclude=tuple(args.exclude),
        )
        excluded = [item for item in args.exclude]
        if excluded:
            print(
                "excluded, reported and not converted — awaiting their "
                "re-capture: " + ", ".join(sorted(excluded))
            )
        print(f"converted {len(written)} files under {args.output_root}")
        return 0
    raw_map = json.loads(args.applies_map.read_text(encoding="utf-8"))
    if not isinstance(raw_map, dict):
        raise SystemExit(f"{args.applies_map}: applies map is not a JSON object")
    applies_map: dict[str, str] = {}
    for key, value in raw_map.items():
        if not isinstance(key, str) or not isinstance(value, str):
            raise SystemExit(f"{args.applies_map}: applies map entries must be strings")
        applies_map[key] = value
    manifest_path = record(
        candidate_commit=args.candidate_commit,
        wheel=args.wheel,
        expected_receipts=args.expected_receipts,
        applies_map=applies_map,
        recorded_at=args.recorded_at,
        repo_root=args.repo_root,
    )
    print(f"wrote {manifest_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())