"""``talaria record``'s credential handling, at the module that dials (U2, R4-R6).

This module exists because ``tests/recorder/test_recorder.py`` covers
:class:`~talaria.recorder.framelog.FrameRecorder` and never imports
:mod:`talaria.recorder.command` at all, so the one place a live credential is
materialized into a URL had no test of its own.

**Every credential value here is a canary.** The strings below have never
authenticated anything; they are written as literals at the point of use, rather
than hidden behind a shared constant, so that a reader checking "no credential
value appears in any file produced by this work" can see that for themselves
without following a reference.

No socket is opened. The connector seam
(:data:`~talaria.recorder.command.Connector`) exists precisely so the dial can be
observed — the URL handed to it is the single credentialed string in the whole
system, and asserting on it is the only direct evidence that the credential goes
exactly one place.
"""

from __future__ import annotations

import asyncio
import json
from collections.abc import AsyncIterator, Iterable
from pathlib import Path
from urllib.parse import parse_qsl, urlsplit

import pytest

from talaria.recorder.command import RecordTarget, resolve_record_target, run_record
from talaria.transport.credentials import (
    Credential,
    CredentialError,
    LoopbackTokenProvider,
    PrimingProvider,
)

GATEWAY_URL_ENV_VAR = "TALARIA_GATEWAY_URL"
TOKEN_ENV_VAR = "HERMES_DASHBOARD_SESSION_TOKEN"  # nosec B105 - a variable name, not a value


class _StubSocket:
    """The connector's yielded message source: an async iterator over fixed frames."""

    def __init__(self, messages: Iterable[str]) -> None:
        self._messages = list(messages)

    async def __aenter__(self) -> _StubSocket:
        return self

    async def __aexit__(self, *exc_info: object) -> bool | None:
        return None

    def __aiter__(self) -> AsyncIterator[str | bytes]:
        async def _frames() -> AsyncIterator[str | bytes]:
            for message in self._messages:
                yield message

        return _frames()


class _RecordingConnector:
    """A connector that remembers every URL it was asked to dial and never dials."""

    def __init__(self, *, messages: Iterable[str] = (), fail: BaseException | None = None) -> None:
        self.dialled: list[str] = []
        self._messages = list(messages)
        self._fail = fail

    def __call__(self, url: str) -> _StubSocket:
        self.dialled.append(url)
        if self._fail is not None:
            raise self._fail
        return _StubSocket(self._messages)


def _resolve(
    monkeypatch: pytest.MonkeyPatch,
    *,
    endpoint: str,
    credential: str,
) -> RecordTarget:
    """Resolve a target the way ``talaria record`` resolves one: from the environment."""
    monkeypatch.setenv(GATEWAY_URL_ENV_VAR, endpoint)
    monkeypatch.setenv(TOKEN_ENV_VAR, credential)
    return asyncio.run(resolve_record_target())


def _run(target: RecordTarget, out: Path, connector: _RecordingConnector) -> tuple[int, list[str]]:
    printed: list[str] = []
    code = asyncio.run(run_record(target, out=out, connector=connector, log=printed.append))
    return code, printed


# ── the one credentialed string in the system ────────────────────────────


def test_the_dialled_url_carries_the_credential_exactly_once(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """R4: the credential reaches the dial, and reaches it once.

    "Exactly once" is asserted rather than "is present" because the failure this
    guards against is an endpoint that already carried a ``token`` picking up a
    second one — two values on one URL, with the gateway free to read either.
    ``AttachTarget.dial_url`` drops the existing credential keys before appending,
    and this is what pins that.
    """
    target = _resolve(
        monkeypatch,
        endpoint="ws://127.0.0.1:9911/api/ws?token=NOT-A-REAL-STALE-0000",
        credential="NOT-A-REAL-CANARY-8ae13c",
    )
    connector = _RecordingConnector(messages=[json.dumps({"method": "ping"})])

    code, _printed = _run(target, tmp_path / "log.jsonl", connector)

    assert code == 0
    assert len(connector.dialled) == 1
    query = parse_qsl(urlsplit(connector.dialled[0]).query, keep_blank_values=True)
    assert [value for name, value in query if name == "token"] == ["NOT-A-REAL-CANARY-8ae13c"]


def test_the_printed_endpoint_and_the_frame_log_header_withhold_the_credential(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """R6 at the two surfaces a recording leaves behind: the terminal and the file.

    Both are taken from the credential-free half of the target, so this holds by
    construction rather than by a redaction pass catching it — which is the point
    of the two halves being separate objects.
    """
    target = _resolve(
        monkeypatch,
        endpoint="ws://127.0.0.1:9911/api/ws",
        credential="NOT-A-REAL-CANARY-8ae13c",
    )
    out = tmp_path / "log.jsonl"
    connector = _RecordingConnector(messages=[json.dumps({"method": "ping"})])

    _code, printed = _run(target, out, connector)

    transcript = "\n".join(printed)
    assert "NOT-A-REAL-CANARY-8ae13c" not in transcript
    assert "ws://127.0.0.1:9911/api/ws" in transcript, "the operator was not told the endpoint"

    written = out.read_text(encoding="utf-8")
    assert "NOT-A-REAL-CANARY-8ae13c" not in written
    header = json.loads(written.splitlines()[0])
    assert header["endpoint"] == "ws://127.0.0.1:9911/api/ws"


def test_userinfo_on_a_configured_endpoint_is_withheld_from_both_surfaces(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """An endpoint may carry a second kind of secret that is not the query token.

    ``ws://user:password@host/`` survives query-stripping — ``urlsplit`` keeps
    userinfo inside ``netloc`` — so the endpoint object is not credential-free in
    that shape. Both surfaces route through ``redact_url``, which withholds it.
    (The command line is a different matter: there this shape is refused outright,
    which ``tests/test_cli.py`` covers.)
    """
    target = _resolve(
        monkeypatch,
        endpoint="ws://operator:NOT-A-REAL-CANARY-5b71@127.0.0.1:9911/api/ws",
        credential="NOT-A-REAL-CANARY-8ae13c",
    )
    out = tmp_path / "log.jsonl"

    _code, printed = _run(target, out, _RecordingConnector())

    assert "NOT-A-REAL-CANARY-5b71" not in "\n".join(printed)
    assert "NOT-A-REAL-CANARY-5b71" not in out.read_text(encoding="utf-8")


def test_a_failed_attach_reports_a_cause_that_withholds_the_credential(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The dialler is handed the credentialed URL, so its exceptions are hostile.

    ``websockets`` writes the whole dialled URI into ``InvalidURI``'s message, so
    an operator-facing line that interpolated the exception raw would print a live
    credential to the terminal — and, under a supervisor, into a log file. This
    stubs the same shape: an exception whose text is the URL it was given.
    """
    target = _resolve(
        monkeypatch,
        endpoint="ws://127.0.0.1:9911/api/ws",
        credential="NOT-A-REAL-CANARY-8ae13c",
    )
    connector = _RecordingConnector(fail=OSError("cannot dial ws://127.0.0.1:9911/api/ws?token=NOT-A-REAL-CANARY-8ae13c"))

    code, printed = _run(target, tmp_path / "log.jsonl", connector)

    transcript = "\n".join(printed)
    assert code == 1, "a dial that never attached must still be exit 1"
    assert "NOT-A-REAL-CANARY-8ae13c" not in transcript
    assert "could not attach" in transcript


def test_the_failure_hint_no_longer_teaches_the_command_line_form(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The hint printed on a failed attach is documentation, and it instructed the leak.

    It used to end with ``talaria record 'ws://…?token=<token>'``, which is now a
    command that exits 2. A remedy that does not work is worse than no remedy.
    """
    target = _resolve(
        monkeypatch,
        endpoint="ws://127.0.0.1:9911/api/ws",
        credential="NOT-A-REAL-CANARY-8ae13c",
    )
    connector = _RecordingConnector(fail=ConnectionRefusedError("refused"))

    _code, printed = _run(target, tmp_path / "log.jsonl", connector)

    transcript = "\n".join(printed)
    assert "token=" not in transcript
    assert "talaria refresh-credential" in transcript


# ── resolution, which is now a step of its own ───────────────────────────


def test_resolution_walks_the_same_chain_the_launcher_walks(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """R4: environment first, then the credential file — KTD11's order, unchanged.

    ``record`` gets the chain rather than a copy of it, so the open question about
    file-versus-environment precedence stays one question with one answer.
    """
    credentials = tmp_path / "credentials"
    credentials.write_text('token = "NOT-A-REAL-FILE-CANARY-33"\n', encoding="utf-8")
    credentials.chmod(0o600)
    monkeypatch.setenv(GATEWAY_URL_ENV_VAR, "ws://127.0.0.1:9911/api/ws")

    from_file = asyncio.run(resolve_record_target(credentials_path=credentials))
    assert from_file.credential.source == "file"
    assert from_file.credential.value == "NOT-A-REAL-FILE-CANARY-33"

    monkeypatch.setenv(TOKEN_ENV_VAR, "NOT-A-REAL-ENV-CANARY-77")
    from_env = asyncio.run(resolve_record_target(credentials_path=credentials))
    assert from_env.credential.source == "environment"
    assert from_env.credential.value == "NOT-A-REAL-ENV-CANARY-77"


def test_an_endpoint_override_is_an_endpoint_and_not_a_credential(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """KTD2: the override goes where the launcher's ``override=`` goes.

    Any credential query key on it is stripped by ``AttachTarget`` rather than
    honoured. That stripping is why the refusal in ``talaria.cli`` has to inspect
    the operator's raw argument first (KTD3) — by the time a target exists there
    is nothing left to detect.
    """
    monkeypatch.setenv(GATEWAY_URL_ENV_VAR, "ws://127.0.0.1:9911/api/ws")
    monkeypatch.setenv(TOKEN_ENV_VAR, "NOT-A-REAL-CANARY-8ae13c")

    target = asyncio.run(resolve_record_target(override="ws://127.0.0.1:9222/api/ws"))

    assert target.endpoint == "ws://127.0.0.1:9222/api/ws"


def test_resolution_refuses_an_endpoint_that_will_not_parse(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A malformed endpoint is a named error, not a traceback — and echoes nothing.

    The string that fails to parse is also the string an operator is most likely
    to have pasted a credential into, so the message names the source and not the
    value.
    """
    monkeypatch.setenv(TOKEN_ENV_VAR, "NOT-A-REAL-CANARY-8ae13c")
    monkeypatch.setenv(GATEWAY_URL_ENV_VAR, "ws://[bad::/api/ws")

    with pytest.raises(CredentialError) as excinfo:
        asyncio.run(resolve_record_target())

    assert "[bad::" not in str(excinfo.value)


def test_resolution_primes_the_provider_and_seals_the_prompt(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The interactive level runs before the recording starts, then is sealed.

    Same ordering, and the same reason, as ``cli._prime_credential``: a hidden
    password prompt raised underneath a running recording is indistinguishable
    from a hung connection, and nothing on screen names the cause.
    """
    monkeypatch.setenv(GATEWAY_URL_ENV_VAR, "ws://127.0.0.1:9911/api/ws")
    provider = LoopbackTokenProvider(prompt=lambda _label: "NOT-A-REAL-TYPED-CANARY-2")
    assert isinstance(provider, PrimingProvider)

    target = asyncio.run(resolve_record_target(provider=provider))

    assert target.credential.source == "prompt"
    # Sealed: a later acquisition falls back to the remembered value rather than
    # asking again, and asking again is the failure mode being prevented.
    again = asyncio.run(provider.acquire())
    assert again.source == "prompt-cached"


def test_a_provider_that_is_not_a_priming_provider_is_merely_acquired(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The seam stays open to a test double that implements only ``acquire``.

    ``CredentialProvider`` is satisfied by any object with an ``acquire``, and
    requiring a second method would break every such double for the benefit of
    one caller. So priming is asked for structurally and skipped when absent.
    """

    class _Double:
        async def acquire(self) -> Credential:
            return Credential(parameter="token", value="NOT-A-REAL-DOUBLE-1", source="environment")

    monkeypatch.setenv(GATEWAY_URL_ENV_VAR, "ws://127.0.0.1:9911/api/ws")

    target = asyncio.run(resolve_record_target(provider=_Double()))

    assert target.credential.value == "NOT-A-REAL-DOUBLE-1"
