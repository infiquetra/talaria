"""TS/Python recorder contract equivalence (AE6, R28, KTD6, PC4).

Feeds one captured receive-only input stream to *both* recorders -- the real
TypeScript reference (``src/record/recorder.ts``, run via a Node/``tsx``
subprocess bridge under ``tests/recorder/ts_bridge/``) and the Python port
under test (``talaria.recorder.framelog.FrameRecorder``) -- and asserts
KTD6's equivalence relation between their output frame logs.

**Two corpora, two purposes.**

1. The synthetic fixture (``tests/recorder/fixtures/equivalence_corpus.json``,
   committed, R29) exercises every credential shape the redaction boundary
   must catch: the four blocking bridges, ``model.save_key``, an
   unknown-method credential, nested/array credentials, camelCase keys,
   non-object frames, and an unparseable payload. This is the test that
   always runs, in CI and locally, and needs no live gateway.

2. A real receive-only corpus, captured live per the plan's Unattended
   execution contract (self-capture authorized 2026-08-02): a scratch
   ``deepseek-v4-flash`` Hermes session, dialled through a harness-launched
   loopback dashboard instance and torn down afterward. It proves the two
   recorders agree on genuine, unscripted gateway traffic shapes, not only on
   hand-written fixtures. It stays out of version control (R29,
   ``.gitignore``'s ``corpora/``) and this test skips cleanly when it is not
   present on the machine running the suite -- e.g. a fresh CI checkout that
   has not run the capture step.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
from pathlib import Path
from typing import Any
from urllib.parse import parse_qsl, urlsplit

import pytest

from talaria.recorder.framelog import FrameRecorder

REPO_ROOT = Path(__file__).resolve().parents[2]
TS_BRIDGE = REPO_ROOT / "tests" / "recorder" / "ts_bridge" / "run_ts_recorder.mjs"
TSX_BIN = REPO_ROOT / "node_modules" / ".bin" / "tsx"
FIXTURE_PATH = Path(__file__).parent / "fixtures" / "equivalence_corpus.json"
LIVE_CORPUS_PATH = REPO_ROOT / "corpora" / "2026-08-02-u2-live-capture.jsonl"

#: Query keys the Python redactor additionally denies on a URL, beyond what
#: the TypeScript reference denies (KTD6, PC10, R28). Pinned independently
#: here so the harness encodes its own expectation of the divergence rather
#: than trusting the implementation under test to describe itself.
PYTHON_ONLY_URL_DENIALS = frozenset({"ticket", "internal"})

_tsx_present = TSX_BIN.exists() and TS_BRIDGE.exists()
requires_ts_bridge = pytest.mark.skipif(
    not _tsx_present,
    reason=f"tsx toolchain not found at {TSX_BIN} -- run `npm install` first",
)


class EquivalenceError(AssertionError):
    """A KTD6 equivalence-relation violation, naming the offending record."""


def run_ts_recorder(endpoint: str, frames: list[str]) -> list[dict[str, Any]]:
    """Run the real TypeScript reference recorder over ``frames`` via a Node
    subprocess and return its parsed frame-log records (header, then
    frames) -- the actual reference implementation, not a port of its
    behavior.
    """
    proc = subprocess.run(
        [str(TSX_BIN), str(TS_BRIDGE)],
        input=json.dumps({"endpoint": endpoint, "frames": frames}),
        capture_output=True,
        text=True,
        timeout=120,
        cwd=REPO_ROOT,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"TS bridge failed (exit {proc.returncode}): {proc.stderr}")
    result: list[dict[str, Any]] = json.loads(proc.stdout)
    return result


def run_python_recorder(endpoint: str, frames: list[str]) -> list[dict[str, Any]]:
    """Run the Python ``FrameRecorder`` over ``frames``, returning records in
    the same shape :func:`run_ts_recorder` returns, for a structural diff.
    """
    with tempfile.TemporaryDirectory() as tmp:
        out_path = Path(tmp) / "out.jsonl"
        recorder = FrameRecorder(out_path, endpoint)
        for raw in frames:
            recorder.record("in", raw)
        recorder.close()
        lines = [
            json.loads(line)
            for line in out_path.read_text(encoding="utf-8").splitlines()
            if line
        ]
    return lines


def _compare_endpoint(ts_endpoint: str, py_endpoint: str) -> None:
    """KTD6: compare ``scheme``/``path`` exactly, compare every query
    parameter's presence and redaction state exactly (allowing the
    enumerated Python-only superset), and normalize only ``host``/``port``.
    """
    if ts_endpoint == "[redacted]" or py_endpoint == "[redacted]":
        if ts_endpoint != py_endpoint:
            raise EquivalenceError(
                f"endpoint redaction disagreement: ts={ts_endpoint!r} py={py_endpoint!r}"
            )
        return

    ts_parts = urlsplit(ts_endpoint)
    py_parts = urlsplit(py_endpoint)

    if ts_parts.scheme != py_parts.scheme:
        raise EquivalenceError(
            f"endpoint scheme differs: ts={ts_parts.scheme!r} py={py_parts.scheme!r}"
        )
    if ts_parts.path != py_parts.path:
        raise EquivalenceError(f"endpoint path differs: ts={ts_parts.path!r} py={py_parts.path!r}")

    ts_query = dict(parse_qsl(ts_parts.query, keep_blank_values=True))
    py_query = dict(parse_qsl(py_parts.query, keep_blank_values=True))

    if set(ts_query) != set(py_query):
        raise EquivalenceError(
            "endpoint query-parameter presence differs: "
            f"ts={sorted(ts_query)} py={sorted(py_query)}"
        )

    for key in ts_query:
        ts_value = ts_query[key]
        py_value = py_query[key]
        ts_redacted = ts_value == "[redacted]"
        py_redacted = py_value == "[redacted]"
        if ts_redacted != py_redacted:
            if py_redacted and not ts_redacted and key in PYTHON_ONLY_URL_DENIALS:
                continue  # authorized divergence (KTD6/PC10): Python denies more
            raise EquivalenceError(
                f"endpoint query key {key!r} redaction state differs: "
                f"ts_redacted={ts_redacted} py_redacted={py_redacted}"
            )
        if not ts_redacted and ts_value != py_value:
            raise EquivalenceError(
                f"endpoint query key {key!r} value differs: ts={ts_value!r} py={py_value!r}"
            )


def _redaction_key(entry: dict[str, Any]) -> tuple[str, str]:
    return entry["path"], entry["reason"]


def compare_records(ts_records: list[dict[str, Any]], py_records: list[dict[str, Any]]) -> None:
    """Assert KTD6's equivalence relation between two frame-log record lists."""
    if not ts_records or not py_records:
        raise EquivalenceError("one side produced no records at all")

    ts_header, *ts_frames = ts_records
    py_header, *py_frames = py_records

    if ts_header.get("kind") != "header" or py_header.get("kind") != "header":
        raise EquivalenceError("first record is not a header on one side")
    if ts_header["version"] != py_header["version"]:
        raise EquivalenceError(
            f"header version differs: ts={ts_header['version']} py={py_header['version']}"
        )
    # startedAt is normalized away (KTD6): observation timestamps never compared.
    _compare_endpoint(ts_header["endpoint"], py_header["endpoint"])

    if len(ts_frames) != len(py_frames):
        raise EquivalenceError(f"frame count differs: ts={len(ts_frames)} py={len(py_frames)}")

    for ts_entry, py_entry in zip(ts_frames, py_frames, strict=True):
        if ts_entry["seq"] != py_entry["seq"]:
            raise EquivalenceError(f"seq differs: ts={ts_entry['seq']} py={py_entry['seq']}")
        if ts_entry["dir"] != py_entry["dir"]:
            raise EquivalenceError(f"seq {ts_entry['seq']}: dir differs")
        # `at` is normalized away (KTD6): observation timestamps never compared.

        ts_has_parse_error = "parseError" in ts_entry
        py_has_parse_error = "parseError" in py_entry
        if ts_has_parse_error != py_has_parse_error:
            raise EquivalenceError(
                f"seq {ts_entry['seq']}: parseError presence differs "
                f"(ts={ts_has_parse_error} py={py_has_parse_error})"
            )
        # Message text is normalized away (KTD6): engine-specific wording.

        if ts_entry.get("frame") != py_entry.get("frame"):
            raise EquivalenceError(
                f"seq {ts_entry['seq']}: parsed frame value differs: "
                f"ts={ts_entry.get('frame')!r} py={py_entry.get('frame')!r}"
            )

        ts_redactions = {_redaction_key(r) for r in ts_entry.get("redactions", [])}
        py_redactions = {_redaction_key(r) for r in py_entry.get("redactions", [])}

        missing_from_python = ts_redactions - py_redactions
        if missing_from_python:
            raise EquivalenceError(
                f"seq {ts_entry['seq']}: TS redactions missing from Python output: "
                f"{missing_from_python}"
            )

        extra_in_python = py_redactions - ts_redactions
        if extra_in_python:
            # Every extra Python redaction must be drawn from the enumerated
            # divergence (KTD6). `ticket`/`internal` only ever appear inside
            # an `endpoint` URL, compared separately above, so no frame-body
            # redaction should ever land here -- if one does, it is
            # unexplained drift, not the authorized divergence.
            raise EquivalenceError(
                f"seq {ts_entry['seq']}: unexplained extra Python redactions "
                f"not in the enumerated superset: {extra_in_python}"
            )


def run_equivalence(endpoint: str, frames: list[str]) -> dict[str, Any]:
    """Run both recorders over ``frames``, assert KTD6 equivalence, and
    return a small summary dict for test-output/evidence purposes.
    """
    ts_records = run_ts_recorder(endpoint, frames)
    py_records = run_python_recorder(endpoint, frames)
    compare_records(ts_records, py_records)
    redaction_count = sum(
        len(r.get("redactions", [])) for r in ts_records if r.get("kind") == "frame"
    )
    return {
        "frame_count": len(frames),
        "record_count": len(ts_records),
        "redaction_count": redaction_count,
    }


# ---------------------------------------------------------------------------
# Synthetic-corpus equivalence: always runs (committed fixture, R29).
# ---------------------------------------------------------------------------


@requires_ts_bridge
def test_equivalence_over_the_synthetic_credential_corpus() -> None:
    corpus = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
    frames = corpus["frames"]
    assert len(frames) >= 15  # a real superset-of-shapes corpus, not a token gesture

    summary = run_equivalence("ws://127.0.0.1:8765/api/ws", frames)

    assert summary["frame_count"] == len(frames)
    # The four blocking bridges + model.save_key + unknown-method +
    # nested + array + camelCase credential each redact at least once.
    assert summary["redaction_count"] >= 8


@requires_ts_bridge
@pytest.mark.parametrize("credential_key", ["token", "ticket", "internal"])
def test_equivalence_endpoint_divergence_is_exactly_the_enumerated_superset(
    credential_key: str,
) -> None:
    """PC10/AE3: a synthetic endpoint URL bearing each of ``?token=``,
    ``?ticket=``, and ``?internal=`` is withheld by the Python side in all
    three cases, while the TypeScript reference is asserted to withhold only
    ``token`` -- so the divergence is pinned by this test, not left to drift.
    """
    endpoint = f"ws://127.0.0.1:8765/api/ws?{credential_key}=super-secret-credential&session=s-1"
    ts_records = run_ts_recorder(endpoint, [])
    py_records = run_python_recorder(endpoint, [])

    ts_endpoint = ts_records[0]["endpoint"]
    py_endpoint = py_records[0]["endpoint"]

    assert "super-secret-credential" not in py_endpoint

    if credential_key == "token":
        # Both sides withhold `token`: no divergence, no exception raised.
        compare_records(ts_records, py_records)
        assert "super-secret-credential" not in ts_endpoint
    else:
        # TS does *not* withhold ticket/internal (the refuted-then-fixed gap
        # KTD6 documents) -- proving the divergence is real, then proving the
        # comparator correctly classifies it as the authorized superset.
        assert "super-secret-credential" in ts_endpoint
        compare_records(ts_records, py_records)  # must not raise: superset is allowed


@requires_ts_bridge
def test_equivalence_over_a_clean_endpoint_with_no_query_string() -> None:
    summary = run_equivalence("ws://127.0.0.1:8765/api/ws", [])
    assert summary["record_count"] == 1  # header only


#: R29's committed-fixture size guarantee, mechanically enforced: committed
#: fixtures under `tests/**/fixtures/` are small synthetic frames only, never
#: a real recorded corpus accidentally checked in. 16 KiB is generous
#: headroom over this fixture's actual size (~3 KiB) while still catching a
#: multi-megabyte corpus pasted in by mistake.
_MAX_COMMITTED_FIXTURE_BYTES = 16 * 1024


def test_committed_fixtures_stay_small_and_synthetic() -> None:
    """R29: a committed-fixture size check keeps fixtures synthetic and small."""
    fixtures_dir = Path(__file__).parent / "fixtures"
    fixture_files = list(fixtures_dir.rglob("*"))
    assert any(f.is_file() for f in fixture_files), "expected at least one committed fixture"

    for path in fixture_files:
        if not path.is_file():
            continue
        size = path.stat().st_size
        assert size <= _MAX_COMMITTED_FIXTURE_BYTES, (
            f"{path} is {size} bytes, over the {_MAX_COMMITTED_FIXTURE_BYTES}-byte "
            "committed-fixture ceiling (R29) -- a real recorded corpus does not belong "
            "in version control; keep fixtures small and synthetic."
        )


# ---------------------------------------------------------------------------
# Live-corpus equivalence: proves the harness against genuine, unscripted
# gateway traffic. Skips cleanly when the corpus is absent (the normal case
# for a checkout that has not run the self-capture step -- R29 keeps it out
# of version control).
# ---------------------------------------------------------------------------


def _load_live_corpus_frames() -> list[str]:
    frames = []
    with LIVE_CORPUS_PATH.open("r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                frames.append(line)
    return frames


@requires_ts_bridge
@pytest.mark.skipif(
    not LIVE_CORPUS_PATH.exists(),
    reason=(
        f"no live corpus at {LIVE_CORPUS_PATH} -- self-capture was not run on this "
        "checkout (R29: corpora stay out of version control). The synthetic-corpus "
        "equivalence test above is the CI-standing evidence; this test additionally "
        "proves the harness against genuine gateway traffic when the corpus exists."
    ),
)
def test_equivalence_over_the_live_captured_corpus() -> None:
    frames = _load_live_corpus_frames()
    assert len(frames) > 0

    digest = hashlib.sha256(LIVE_CORPUS_PATH.read_bytes()).hexdigest()
    summary = run_equivalence("ws://127.0.0.1:18799/api/ws?token=redacted-for-test", frames)

    print(
        f"live corpus: {LIVE_CORPUS_PATH.name} "
        f"sha256={digest} frames={summary['frame_count']} "
        f"records={summary['record_count']} redactions={summary['redaction_count']}"
    )
    assert summary["frame_count"] == len(frames)
