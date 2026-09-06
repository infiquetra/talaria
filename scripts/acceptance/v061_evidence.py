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
    V061_RELEASE,
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
        harness = receipt.get("harness_commit")
        if harness == candidate_commit:
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
                f"{rel}: harness_commit {harness} is not the candidate commit, and the "
                f"applies map carries no sentence naming the unchanged surfaces"
            )
        elif sentence.strip() == "same" or not sentence.strip():
            errors.append(
                f"{rel}: applies_to_candidate must be a non-empty sentence naming the "
                f"unchanged surfaces since commit {harness}, not `same`"
            )
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

    verdicts = Counter(
        str(_read_json(path).get("verdict")) for path, _rel in receipts.values()
    )
    receipt_entries: list[dict[str, Any]] = []
    for _item, (path, rel) in sorted(receipts.items()):
        receipt = _read_json(path)
        harness = receipt["harness_commit"]
        receipt_entries.append(
            {
                "receipt_path": rel,
                "receipt_sha256": _sha256_file(path),
                "checklist_item": receipt["checklist_item"],
                "tester": receipt["tester"],
                "verdict": receipt["verdict"],
                "harness_commit": harness,
                "applies_to_candidate": (
                    "same" if harness == candidate_commit else applies_map[rel]
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
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
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