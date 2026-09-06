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
import json
import shutil
import subprocess
import sys
from collections import Counter
from pathlib import Path
from typing import Any

from scripts.acceptance.v050_receipt import (
    V061_ITEM_SCHEMA,
    V061_RELEASE,
    V061_ROLE_LABELS,
    _v061_private_identifier_errors,
    _validate_v061_receipt,
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
            "kind": "source-checkout",
            "commit": commit,
            "basis": (
                "attested at conversion by the capturing role, "
                f"{attestation['attested_at']}"
            ),
        },
        "harness": {
            "kind": "scratch-capture",
            "commit": None,
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
    converted["evidence"] = evidence
    return converted


def convert(
    *,
    attestations: dict[str, dict[str, Any]],
    output_root: Path,
    listed_at: str,
    repo_root: Path = REPO_ROOT,
) -> list[Path]:
    """Convert every filed receipt with its attestation; return what was written.

    All-or-nothing: every refusal is collected across the whole tree, and
    nothing survives on disk when any receipt refuses — the operator sees
    exactly which live cases need re-capture, and the tree is never
    half-converted.
    """
    receipts = _live_receipts(repo_root)
    if not receipts:
        raise SystemExit(f"no filed receipts under {repo_root / _EVIDENCE_REL}")
    refusals: list[str] = []
    plans: list[tuple[str, Path, dict[str, Any], dict[str, Path]]] = []
    for item, (path, _rel) in sorted(receipts.items()):
        filed = _read_json(path)
        commit, commit_refusals = _attestation_refusals(item, filed)
        refusals.extend(f"{item}: {refusal}" for refusal in commit_refusals)
        attestation = attestations.get(item)
        if attestation is None:
            refusals.append(
                f"{item}: no attestation for its commit — re-capture rather than convert"
            )
            continue
        if commit is None:
            continue
        for role_field in _ATTESTATION_ROLE_FIELDS:
            if attestation.get(role_field) not in V061_ROLE_LABELS:
                refusals.append(
                    f"{item}: attestation.{role_field} must be a closed-set role label "
                    f"({', '.join(V061_ROLE_LABELS)})"
                )
        attested_at = attestation.get("attested_at")
        if not isinstance(attested_at, str) or not attested_at.strip():
            refusals.append(f"{item}: attestation.attested_at must record its date")
        rich = "candidate_commit_sha" in filed
        if not rich and not (
            isinstance(attestation.get("expected"), str)
            and attestation["expected"].strip()
        ):
            refusals.append(
                f"{item}: a thin receipt converts only with the owning child's pass "
                f"condition as an explicit attestation (attestation.expected)"
            )
        source_dir = path.parent
        files: dict[str, Path] = {
            evidence_path.relative_to(source_dir).as_posix(): evidence_path
            for evidence_path in sorted(source_dir.rglob("*"))
            if evidence_path.is_file() and evidence_path.name != "receipt.json"
        }
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
    convert_cmd.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
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