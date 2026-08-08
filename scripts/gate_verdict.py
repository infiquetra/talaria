#!/usr/bin/env python3
"""scripts/gate_verdict.py — read a gating document's declared verdict.

The release workflow refuses to publish a tag while the v0.1 daily-driver gate
reads anything other than ``READY``. That check needs to parse the same fenced
``gate`` block that ``tests/docs/test_gating_documents.py`` cross-examines
against its evidence table, and a second parser written inline in a workflow
step would drift from the first one silently. So the reader lives here, and the
test suite runs *this file* as a subprocess — the same interface continuous
integration uses — rather than a reimplementation of it.

Gates are found by ``id`` rather than by path. A document that moves or is
renamed then produces a loud "no gate with that id" failure instead of a check
that quietly stops checking anything.

Usage::

    scripts/gate_verdict.py --id v0-1-daily-driver
    scripts/gate_verdict.py --id v0-1-daily-driver --expect READY

Exit codes are distinct because the caller should treat them differently:
``0`` the verdict was read (and matched ``--expect`` if given), ``1`` it was read
and did not match, ``2`` it could not be determined at all. Collapsing the last
two would let a moved file read as a failed gate, which is the wrong diagnosis.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parents[1]
_DOCS = _REPO_ROOT / "docs"

# Mirrors tests/docs/test_gating_documents.py. Kept deliberately small: a check
# whose whole job is to be trusted should be obviously correct by inspection.
_GATE_BLOCK = re.compile(r"^```gate\n(.*?)^```", re.MULTILINE | re.DOTALL)


def _fields(body: str) -> dict[str, str]:
    """Flat ``key: value`` lines. ``blocks-on`` repeats, so it is not a field."""
    fields: dict[str, str] = {}
    for raw_line in body.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        key, separator, value = line.partition(":")
        if not separator or key.strip() == "blocks-on":
            continue
        fields[key.strip()] = value.strip()
    return fields


def find_verdict(identifier: str, docs: Path = _DOCS) -> str:
    """Return the verdict declared by the gate with ``identifier``.

    Raises :class:`LookupError` when no gate declares that id, when the gate
    declares no verdict, or when more than one block claims the id — ambiguity
    is an error rather than a first-match win, because "which one gates the
    release" would have no answer.
    """
    found: list[tuple[Path, str]] = []
    for path in sorted(docs.rglob("*.md")):
        for match in _GATE_BLOCK.finditer(path.read_text(encoding="utf-8")):
            fields = _fields(match.group(1))
            if fields.get("id") != identifier:
                continue
            verdict = fields.get("verdict")
            if verdict is None:
                raise LookupError(f"{path}: gate {identifier!r} declares no `verdict`")
            found.append((path, verdict))

    if not found:
        raise LookupError(
            f"no document under {docs} declares a gate with id {identifier!r}. "
            f"If the gating document moved, this check has stopped checking it."
        )
    if len(found) > 1:
        listed = ", ".join(f"{path}={verdict!r}" for path, verdict in found)
        raise LookupError(f"gate id {identifier!r} is declared more than once: {listed}")
    return found[0][1]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="gate_verdict.py",
        description="Print the verdict a gating document declares for a gate id.",
    )
    parser.add_argument(
        "--id",
        dest="identifier",
        required=True,
        metavar="GATE-ID",
        help="the `id` declared in the document's fenced gate block",
    )
    parser.add_argument(
        "--expect",
        metavar="VERDICT",
        help="exit 1 unless the declared verdict is exactly this",
    )
    args = parser.parse_args(argv)

    try:
        verdict = find_verdict(args.identifier)
    except LookupError as error:
        print(f"gate_verdict: {error}", file=sys.stderr)
        return 2

    print(verdict)
    if args.expect is not None and verdict != args.expect:
        print(
            f"gate_verdict: gate {args.identifier!r} reads {verdict!r}, "
            f"not {args.expect!r}",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
