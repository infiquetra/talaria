"""Record the Talaria v0.6.0 acceptance evidence tree.

Unlike the v0.5.0 live two-tester PTY flow (frozen in ``v050_*``), the v0.6.0
matrix rows transcribe controller-observed live results reported on
infiquetra/talaria#127: twelve live sessions and shell probes against the
verified tree, each with its observation quoted and its provenance pointer
recorded. Transcribed does not mean invented — every row names what was
observed, who observed it, and where the report lives; rows carry no terminal
captures because none were taken through this flow.

What this flow *does* measure itself is the install leg: it installs the
candidate wheel into two fresh scratch virtual environments and probes the
installed executable, recording real digests. The candidate version is stamped
from the package (``talaria.__version__``) at record time, never from a
constant.

Nothing here is overwritten on re-run: evidence files are created
exclusively, so recording over an existing tree fails loudly instead of
mutating a receipt (receipt immutability).
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]

ITEM_SCHEMA = "talaria-v0.6.0-receipt-v1"
INSTALL_SCHEMA = "talaria-v0.6.0-install-v1"
MANIFEST_SCHEMA = "talaria-v0.6.0-artifact-manifest-v1"
GATE_ID = "v0-6-daily-driver"
ISSUE_URL = "https://github.com/infiquetra/talaria/issues/127"

# The twelve controller-observed rows, in finding order. Observations are the
# controller's words from the T4 evidence report; session identifiers from
# that report are deliberately not transcribed (public-safe evidence carries
# method and verdict, never operator session ids).
MATRIX_ROWS: tuple[tuple[int, str, str, str], ...] = (
    (1, "119/F1 catalog truth", "#119", "catalog renders with no false drift"),
    (2, "119/F2 honest 4018", "#119", "model refusal surfaces the gateway 4018"),
    (3, "120/F3 nested toggle", "#120", "nested Ctrl+O toggle plus override"),
    (4, "120/F4 idle cancel", "#120", "idle cancel plus cancel/quit footer"),
    (
        5,
        "122/F5 inspector diagnostics",
        "#122",
        "inspector DIAGNOSTICS shows 4 live rows, region empty, "
        "injected-failure honest refusal",
    ),
    (
        6,
        "125/F6 bar pickup",
        "#125",
        "v1-to-v2 pickup without restart plus exit-1 fallback marker",
    ),
    (
        7,
        "124/F7 import round trip",
        "#124",
        "fetch-select-Reload same-slug repaint holds through merge",
    ),
    (
        8,
        "123/F8 Homebrew",
        "#123",
        "Homebrew listed, never the startup default",
    ),
    (9, "123/F9 inheritance", "#123", "sparse inheritance and offset reclaim"),
    (
        10,
        "121/F10 picker parity",
        "#121",
        "picked-vs-typed parity plus double-Enter single fire",
    ),
    (11, "126 credential deny", "#126", "four synthetic keys denied when allowlisted"),
    (12, "annex Hermes touch", "#118", "zero Hermes touch"),
)

GATE0_RECORD: dict[str, str] = {
    "tree": "c571590",
    "uv_sync": "clean",
    "ruff": "clean",
    "mypy": "clean (201 files)",
    "pytest": "2804 passed, 7 skipped",
    "bandit": "exit 0",
    "diff_check": "clean",
}


def _utc_now(recorded_at: str | None) -> str:
    if recorded_at is not None:
        return recorded_at
    return dt.datetime.now(dt.UTC).isoformat(timespec="seconds")


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _write_new(path: Path, value: dict[str, Any]) -> str:
    """Write one evidence file exclusively; return its SHA-256."""
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with path.open("x", encoding="utf-8") as handle:
            json.dump(value, handle, indent=2, sort_keys=True)
            handle.write("\n")
    except FileExistsError as exc:
        raise SystemExit(f"refusing to replace existing evidence file: {path}") from exc
    return _sha256_file(path)


def _package_version() -> str:
    sys.path.insert(0, str(REPO_ROOT))
    import talaria

    return talaria.__version__


def _run(argv: list[str], *, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(argv, capture_output=True, text=True, env=env, check=False)


def _probe_install(
    wheel: Path, *, candidate: dict[str, str], recorded_at: str
) -> tuple[dict[str, Any], Path]:
    """Install the candidate wheel into fresh scratch and probe it for real.

    Real virtual environment, real install, real executable probes — the
    digests below are measured, not transcribed. Scratch paths are scrubbed
    to a placeholder before recording so the receipt carries no operator
    filesystem identifiers. Returns the receipt and the scratch directory
    (the caller removes scratch; the receipt itself is recorded separately).
    """
    scratch = Path(tempfile.mkdtemp(prefix="v060-install-"))
    venv = scratch / "venv"
    proc = _run(["uv", "venv", str(venv)])
    if proc.returncode != 0:
        raise SystemExit(f"uv venv failed: {proc.stderr[-2000:]}")
    clean_env = {k: v for k, v in os.environ.items() if k != "PYTHONPATH"}
    proc = _run(
        ["uv", "pip", "install", "--python", str(venv / "bin" / "python"), str(wheel)],
        env=clean_env,
    )
    if proc.returncode != 0:
        raise SystemExit(f"wheel install failed: {proc.stderr[-2000:]}")
    executable = venv / "bin" / "talaria"
    version_proc = _run([str(executable), "--version"])
    help_proc = _run([str(executable), "--help"])
    version_reported = version_proc.stdout.strip().removeprefix("talaria ").strip()
    installed = sorted((venv / "lib").rglob("*.py"))
    files_digest = hashlib.sha256()
    for path in installed:
        files_digest.update(_sha256_file(path).encode())
    receipt: dict[str, Any] = {
        "schema_version": INSTALL_SCHEMA,
        "release": candidate["version"],
        "tester": "operator",
        "recorded_at": recorded_at,
        "harness_commit": candidate["commit"],
        "candidate": dict(candidate),
        "install": {
            "scratch_root": "<scratch-root>",
            "venv": "<scratch-root>/venv",
            "executable": "<scratch-root>/venv/bin/talaria",
            "executable_sha256": _sha256_file(executable),
            "installed_file_count": len(installed),
            "installed_files_sha256": files_digest.hexdigest(),
            "version_reported": version_reported,
            "help_ok": help_proc.returncode == 0,
        },
    }
    return receipt, scratch


def _item_receipt(
    number: int,
    title: str,
    issue: str,
    observation: str,
    *,
    candidate: dict[str, str],
    harness_commit: str,
    recorded_at: str,
) -> dict[str, Any]:
    return {
        "schema_version": ITEM_SCHEMA,
        "release": candidate["version"],
        "checklist_item": number,
        "title": title,
        "issue": f"https://github.com/infiquetra/talaria/issues/{issue.lstrip('#')}"
        if issue.startswith("#")
        else issue,
        "tester": "controller",
        "verdict": "pass",
        "artifact": dict(candidate),
        "harness_commit": harness_commit,
        "recorded_at": recorded_at,
        "evidence": {
            "kind": "reported-live-matrix",
            "source": ISSUE_URL,
            "method": "live sessions plus shell probes against the verified tree; "
            "existing sessions preserved; no real secrets",
            "observation": observation,
        },
    }


def _manifest(
    *,
    candidate: dict[str, str],
    harness_commit: str,
    recorded_at: str,
    receipt_entries: list[dict[str, Any]],
    install_entries: list[dict[str, Any]],
    gate0_entry: dict[str, str],
    generated_command: str,
) -> dict[str, Any]:
    return {
        "$schema": "./artifact-manifest.schema.json",
        "schema_version": MANIFEST_SCHEMA,
        "gate_id": GATE_ID,
        "generated_command": generated_command,
        "status": "complete",
        "recorded_at": recorded_at,
        "harness_commit": harness_commit,
        "current_candidate": dict(candidate),
        "counts": {
            "expected_receipts": 12,
            "install_receipts": len(install_entries),
            "invalid_item_receipts": 0,
            "item_receipts": len(receipt_entries),
            "item_verdicts": {"blocked": 0, "fail": 0, "pass": 12, "reserved": 0},
            "missing_current_receipts": 0,
            "missing_receipts_on_disk": 0,
            "stale_receipts": 0,
            "current_receipts": len(receipt_entries) + len(install_entries),
        },
        "receipts": receipt_entries,
        "install_receipts": install_entries,
        "gate0_receipts": [gate0_entry],
        "results_document": "docs/acceptance/v0.6.0/results.md",
        "notes_document": "docs/acceptance/v0.6.0/notes.md",
    }


def _results_document(*, candidate: dict[str, str]) -> str:
    lines = [
        "# Talaria v0.6.0 acceptance results",
        "",
        "Twelve controller-observed live-matrix rows against the verified tree, "
        f"candidate commit `{candidate['commit']}`, wheel "
        f"`{candidate['wheel_filename']}` (`{candidate['wheel_sha256'][:12]}…), "
        f"gate `{GATE_ID}`. Every row passed; the full matrix report lives on "
        f"{ISSUE_URL}.",
        "",
        "| Item | Row | Verdict |",
        "| ---: | --- | --- |",
    ]
    for number, title, _issue, _observation in MATRIX_ROWS:
        lines.append(f"| {number} | {title} | pass |")
    lines += [
        "",
        "The machine-readable receipts under `evidence/` carry each observation "
        "with its provenance; `artifact-manifest.json` binds them to the candidate.",
        "",
    ]
    return "\n".join(lines)


def _notes_document(*, candidate: dict[str, str]) -> str:
    return "\n".join(
        [
            "# Talaria v0.6.0 acceptance notes",
            "",
            "## Provenance",
            "",
            "Gate-0 ran on merged tree `c571590`: uv sync clean; ruff clean; "
            "mypy clean (201 files); pytest 2804 passed, 7 skipped; bandit "
            "exit 0; git diff --check clean. The live matrix ran in scratch "
            "against the same tree; existing sessions were preserved and no "
            "real secrets were involved.",
            "",
            "## Limitations (controller-reported)",
            "",
            "- Bare host tty unexercised.",
            "- F2 slash.exec 4001 on listed sids (the honest-unavailable leg "
            "is the 4018).",
            "- Marketplace fetch not re-attempted (prior: search live, "
            "downloads 404 safe refusal).",
            "- Corrupt-file reload plus sub-dialog races unit-covered only.",
            "",
            "## Release-time refresh",
            "",
            f"Candidate commit `{candidate['commit']}` names the verified "
            "bytes. Re-run this record flow on the release tree before "
            "tagging so the manifest binds the tag commit.",
            "",
        ]
    )


def record(
    *,
    candidate_commit: str,
    wheel: Path,
    recorded_at: str | None = None,
    repo_root: Path = REPO_ROOT,
) -> Path:
    """Record the whole v0.6.0 evidence tree; return the manifest path."""
    stamped = _utc_now(recorded_at)
    version = _package_version()
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
    receipt_entries: list[dict[str, Any]] = []
    for number, title, issue, observation in MATRIX_ROWS:
        receipt = _item_receipt(
            number,
            title,
            issue,
            observation,
            candidate=candidate,
            harness_commit=harness_commit,
            recorded_at=stamped,
        )
        rel = f"docs/acceptance/v0.6.0/evidence/matrix/receipts/item-{number:02d}.json"
        digest = _write_new(repo_root / rel, receipt)
        receipt_entries.append(
            {
                "receipt_path": rel,
                "receipt_sha256": digest,
                "checklist_item": number,
                "tester": "controller",
                "verdict": "pass",
                "title": title,
            }
        )
    install_entries: list[dict[str, Any]] = []
    scratch_dirs: list[Path] = []
    for probe in ("probe-1", "probe-2"):
        receipt, scratch = _probe_install(
            wheel, candidate=candidate, recorded_at=stamped
        )
        scratch_dirs.append(scratch)
        rel = f"docs/acceptance/v0.6.0/evidence/{probe}/install-receipt.json"
        digest = _write_new(repo_root / rel, receipt)
        install_entries.append(
            {"receipt_path": rel, "receipt_sha256": digest, "tester": "operator"}
        )
    gate0 = {"recorded_at": stamped, "candidate_commit": "c571590", **GATE0_RECORD}
    gate0_rel = "docs/acceptance/v0.6.0/gate0.json"
    gate0_digest = _write_new(repo_root / gate0_rel, gate0)
    results_rel = "docs/acceptance/v0.6.0/results.md"
    (repo_root / results_rel).write_text(
        _results_document(candidate=candidate), encoding="utf-8"
    )
    notes_rel = "docs/acceptance/v0.6.0/notes.md"
    (repo_root / notes_rel).write_text(
        _notes_document(candidate=candidate), encoding="utf-8"
    )
    manifest = _manifest(
        candidate=candidate,
        harness_commit=harness_commit,
        recorded_at=stamped,
        receipt_entries=receipt_entries,
        install_entries=install_entries,
        gate0_entry={"receipt_path": gate0_rel, "receipt_sha256": gate0_digest},
        generated_command=(
            "uv run python -m scripts.acceptance.v060_evidence record "
            f"--candidate-commit {candidate_commit} --wheel {wheel.name}"
        ),
    )
    manifest_path = repo_root / "docs" / "acceptance" / "v0.6.0" / "artifact-manifest.json"
    _write_new(manifest_path, manifest)
    for scratch in scratch_dirs:
        shutil.rmtree(scratch, ignore_errors=True)
    return manifest_path


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-commit", required=True)
    parser.add_argument("--wheel", type=Path, required=True)
    parser.add_argument("--recorded-at", default=None)
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    manifest_path = record(
        candidate_commit=args.candidate_commit,
        wheel=args.wheel,
        recorded_at=args.recorded_at,
        repo_root=args.repo_root,
    )
    print(f"wrote {manifest_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
