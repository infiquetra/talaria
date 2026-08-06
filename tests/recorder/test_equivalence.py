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
import os
import subprocess
import tempfile
from pathlib import Path
from typing import Any
from urllib.parse import parse_qsl, unquote, urlsplit

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

#: Set on the CI leg that installs the bridge. Skipping is the right behaviour
#: on a developer machine without Node; it is the wrong behaviour on the job
#: that exists to prove parity.
REQUIRE_TS_BRIDGE = os.environ.get("TALARIA_REQUIRE_TS_BRIDGE") == "1"


def test_the_equivalence_bridge_is_present_where_ci_requires_it() -> None:
    """A skip is invisible inside a green run, which is how this went unnoticed.

    Every ``@requires_ts_bridge`` test below skipped in CI because the Python
    jobs installed ``uv`` and never Node, so the harness that is cited as the
    standing evidence for the KTD6/R28 parity relation had never executed there.
    The tests pass when actually run -- the port does not diverge -- so the
    defect was never a wrong result, only an unrun proof. This check fails on
    the leg that is supposed to run it, rather than letting it lapse again.
    """
    if not REQUIRE_TS_BRIDGE:
        pytest.skip("TALARIA_REQUIRE_TS_BRIDGE is unset -- enforced only on the CI leg")
    assert _tsx_present, (
        f"TALARIA_REQUIRE_TS_BRIDGE=1 but the bridge is missing: "
        f"tsx={TSX_BIN} exists={TSX_BIN.exists()}, "
        f"bridge={TS_BRIDGE} exists={TS_BRIDGE.exists()}. "
        "Run `npm ci` before pytest on this job."
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


#: Redaction reasons the Python recorder may emit inside a *frame body* where
#: the TypeScript reference emits nothing (KTD6 divergences 2 and 4 in
#: ``talaria/recorder/redact.py``). The reference has no in-body URL redaction
#: at all, so any credential-bearing URL in a frame is a Python-only withholding.
PYTHON_ONLY_FRAME_REDACTION_REASONS = frozenset({"url-credential"})


def _is_authorized_url_divergence(ts_value: object, py_value: object) -> bool:
    """True when ``py_value`` is ``ts_value`` with URL credentials withheld.

    Encodes the expectation independently rather than calling ``redact_url``:
    asking the implementation under test whether its own output is correct is
    the self-comparison this harness exists to avoid. Scheme, host, port and
    path must survive untouched; every query key must survive with its value
    either unchanged or replaced by the redaction marker; and userinfo and the
    fragment may disappear but must never appear where they were previously
    absent.

    The fragment moved out of the "must survive untouched" group on 2026-08-05,
    when divergence 5 gave the Python redactor a fragment rule the TypeScript
    reference has no equivalent of. Until then this predicate pinned it equal,
    which is why widening the redactor is a two-file change: a redactor that
    withholds more than the comparator authorizes reads here as a port bug.
    """
    if not isinstance(ts_value, str) or not isinstance(py_value, str):
        return False
    if "://" not in ts_value:
        return False
    ts, py = urlsplit(ts_value), urlsplit(py_value)
    if not ts.scheme or not ts.netloc:
        return False
    if (ts.scheme, ts.path) != (py.scheme, py.path):
        return False
    if (ts.hostname, ts.port) != (py.hostname, py.port):
        return False
    # Userinfo and the fragment may only be removed or masked, never introduced.
    if (
        (ts.username, ts.password) == (py.username, py.password)
        and ts.query == py.query
        and ts.fragment == py.fragment
    ):
        return False  # nothing diverged; the caller should have matched on equality
    # `urlsplit` hands back the raw component, so the marker arrives
    # percent-encoded (`%5Bredacted%5D`) exactly as it sits on disk.
    if py.username is not None and unquote(py.username) not in ("[redacted]", ts.username):
        return False
    if py.fragment and not ts.fragment:
        return False
    if py.fragment and unquote(py.fragment) not in ("[redacted]", ts.fragment):
        return False

    ts_query = dict(parse_qsl(ts.query, keep_blank_values=True))
    py_query = dict(parse_qsl(py.query, keep_blank_values=True))
    if set(ts_query) != set(py_query):
        return False
    return all(
        py_query[key] == ts_query[key] or py_query[key] == "[redacted]" for key in ts_query
    )


def _frames_equivalent(ts_value: object, py_value: object) -> bool:
    """KTD6 frame equality, allowing only the enumerated Python-only URL superset.

    Previously a flat ``!=``. That was correct until the recorder gained in-body
    URL redaction: from then on a frame carrying a credential URL diverged
    legitimately, and the harness had no way to say so — it would have reported
    "parsed frame value differs" and read as a port bug. Nothing caught the gap
    because no fixture frame contained a URL at all.
    """
    if isinstance(ts_value, dict) and isinstance(py_value, dict):
        if set(ts_value) != set(py_value):
            return False
        return all(_frames_equivalent(ts_value[k], py_value[k]) for k in ts_value)
    if isinstance(ts_value, list) and isinstance(py_value, list):
        if len(ts_value) != len(py_value):
            return False
        return all(_frames_equivalent(a, b) for a, b in zip(ts_value, py_value, strict=True))
    if ts_value == py_value:
        return True
    return _is_authorized_url_divergence(ts_value, py_value)


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

        if not _frames_equivalent(ts_entry.get("frame"), py_entry.get("frame")):
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
        # Every extra Python redaction must be drawn from the enumerated
        # divergence (KTD6). `ticket`/`internal` only ever appear inside an
        # `endpoint` URL, compared separately above. A frame-body redaction may
        # land here for exactly one reason -- an in-body credential URL, which
        # the TypeScript reference does not redact at all. This used to assert
        # that *no* frame-body redaction could ever be authorized, which stopped
        # being true when the recorder gained in-body URL redaction.
        unexplained = {
            (path, reason)
            for path, reason in extra_in_python
            if reason not in PYTHON_ONLY_FRAME_REDACTION_REASONS
        }
        if unexplained:
            raise EquivalenceError(
                f"seq {ts_entry['seq']}: unexplained extra Python redactions "
                f"not in the enumerated superset: {unexplained}"
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


@pytest.mark.parametrize(
    ("label", "ts_frame", "py_frame"),
    [
        (
            "an ordinary value changed",
            {"result": {"count": 1}},
            {"result": {"count": 2}},
        ),
        (
            "a key appeared on one side only",
            {"result": {"a": 1}},
            {"result": {"a": 1, "b": 2}},
        ),
        (
            "the URL host was rewritten, not just its credentials",
            {"url": "wss://gateway.local/attach?token=s"},
            {"url": "wss://evil.example/attach?token=%5Bredacted%5D"},
        ),
        (
            "a non-credential query value changed under cover of a redaction",
            {"url": "wss://h/attach?token=s&session=s-1"},
            {"url": "wss://h/attach?token=%5Bredacted%5D&session=s-2"},
        ),
        (
            "a query key vanished under cover of a redaction",
            {"url": "wss://h/attach?token=s&session=s-1"},
            {"url": "wss://h/attach?token=%5Bredacted%5D"},
        ),
        (
            "the path changed",
            {"url": "wss://h/attach?token=s"},
            {"url": "wss://h/elsewhere?token=%5Bredacted%5D"},
        ),
        (
            "userinfo was introduced where there was none",
            {"url": "wss://h/attach?token=s"},
            {"url": "wss://someone@h/attach?token=%5Bredacted%5D"},
        ),
        (
            "a list changed length",
            {"items": [1, 2]},
            {"items": [1, 2, 3]},
        ),
    ],
)
def test_the_frame_comparator_still_rejects_unauthorized_divergence(
    label: str, ts_frame: dict[str, Any], py_frame: dict[str, Any]
) -> None:
    """The URL allowance must not become a hole the whole frame fits through.

    ``_frames_equivalent`` relaxed an exact ``!=`` into "equal, or a Python-only
    URL redaction". A relaxation nobody has seen reject anything is
    indistinguishable from deleting the check.
    """
    assert _frames_equivalent(ts_frame, py_frame) is False, label


def test_the_frame_comparator_accepts_the_authorized_url_divergence() -> None:
    """Both shapes the recorder actually produces, and nothing else."""
    assert _frames_equivalent(
        {"result": {"url": "ws://h:9222/api/ws?token=secret&session=s-1"}},
        {"result": {"url": "ws://h:9222/api/ws?token=%5Bredacted%5D&session=s-1"}},
    )
    assert _frames_equivalent(
        {"result": {"url": "http://operator:pw@cdp.example/"}},
        {"result": {"url": "http://%5Bredacted%5D@cdp.example/"}},
    )


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
