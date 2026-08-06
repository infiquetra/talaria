"""KTD11's credential chain — acquired on **every dial**, never once at startup.

The whole reason this module exists as an interface rather than as a string
read at launch is recorded in KTD11, and it is worth repeating where the code
lives. Loopback ``?token=`` is a fixed value that can be fetched once and reused
forever; a gated ``?ticket=`` is single-use with a 30-second lifetime and must be
minted fresh on every dial *including every reconnect*. If v0.1 caches a
credential at startup, the reconnect path silently acquires a dependency on the
fixed-value assumption, and adding remote support later means rewriting
reconnect — the subtlest concurrency work in the transport. So the seam is one
method, called per dial, returning a ``(query parameter name, value)`` pair.

**v0.1 ships :class:`LoopbackTokenProvider` only.** ``GatedTicketProvider`` is
specified in ``docs/engineering-journal/QUEUED.md`` and deliberately not built.

**The credential rides the URL query string, never argv (KTD13, R1, R9).** At
Hermes ``7f4d15515`` the WebSocket upgrade credential is read exclusively from
``ws.query_params`` (``hermes_cli/web_server.py:14443-14524``); no header is
inspected. That is a protocol fact, not a preference — which is why
``redact_url`` (``talaria/recorder/redact.py``) and KTD5's query-stripping of
``TALARIA_GATEWAY_URL`` are load-bearing rather than incidental.

**Acquisition precedence, highest first.**

1. A ``token`` query parameter already present on ``TALARIA_GATEWAY_URL``. An
   operator who has exported that variable with a credential on it has already
   supplied the credential, and Talaria reads the endpoint from it either way.
   This level is about the *environment*, and only the environment. The command
   line is not an equivalent place to put the same string, and ``talaria record``
   now refuses an endpoint argument carrying a credential rather than accepting
   one (``talaria/cli.py``): an exported variable is not in the process table of
   every process that reads it, and a command line is.
2. ``<config_dir>/credentials``, rejected unless its mode is no looser than
   ``0600``. ``talaria refresh-credential`` writes this file, so the operator
   never has to hold the value themselves.
3. An interactive hidden prompt, via :func:`getpass.getpass`.

**``HERMES_DASHBOARD_SESSION_TOKEN`` is not a level, and that is a decision.**
It used to be the highest one. It was removed on 2026-08-06 — KTD8 of
``docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md``, option (b) of
``QUEUED.md``'s entry *"R1's environment clause is unmet, and no change to
Talaria can meet it"*. Read the module's own claim narrowly, because the narrow
claim is the true one:

* **True.** Talaria no longer reads a dedicated credential variable, so an
  operator who unsets ``HERMES_DASHBOARD_SESSION_TOKEN`` loses no supported way
  of supplying a credential. Level 2 is a route with no environment footprint
  at all.
* **Not true, and not claimed anywhere.** That R1's environment clause is now
  met. An inherited variable stays visible in ``/proc/<pid>/environ`` for the
  life of the process — the kernel snapshots the block at ``exec`` and no code
  change touches that. ``tests/transport/test_process_surface.py`` asserts that
  *failure* and is meant to keep asserting it.
* **Also not true.** That no supported route puts a credential in the
  environment. Level 1 still does, by the operator's own choice, and the README
  says so plainly. What changed is that a credential-free environment is now
  reachable without giving anything up.

Levels 1–2 are re-read on **every** call, so a rotated token is picked up by the
next reconnect without restarting Talaria — the guarantee KTD11 exists for, now
carried by the credential file rather than by a variable. Level 3 is different
on purpose: a prompt-sourced credential is held in memory for the process
lifetime and never re-prompted, because re-prompting mid-stream would block
reconnection on operator presence.

**Level 3 is reachable only before the interface takes the terminal.** A dial
happens inside a running Textual application, which owns the screen and is
reading stdin; a :func:`getpass.getpass` issued from there writes its prompt
where nothing can display it and then blocks a worker thread on a read that
competes with the UI's own input driver. The observable result is a client that
appears hung at "connecting" while it is in fact waiting, invisibly, for typing
that cannot reach it. So the launcher calls :meth:`LoopbackTokenProvider.prime`
**before** the interface starts and that call seals the prompt for the rest of
the process: every later dial resolves from levels 1–2 or from the primed value,
and a dial that can find none of those fails with :class:`CredentialError`
rather than asking a question nobody can see.

**Nothing here logs, prints, or reprs a credential value.** :class:`Credential`
overrides ``__repr__`` for that reason — a dataclass's generated repr would put
the token into any traceback, any ``logging`` call with ``%r``, and any pytest
assertion failure that happens to hold one.
"""

from __future__ import annotations

import asyncio
import getpass
import os
import tomllib
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Literal, Protocol, runtime_checkable
from urllib.parse import parse_qsl, urlsplit

__all__ = [
    "CREDENTIAL_FILE_MODE",
    "DEFAULT_GATEWAY_URL",
    "GATEWAY_URL_ENV_VAR",
    "Credential",
    "CredentialError",
    "CredentialProvider",
    "CredentialSource",
    "LoopbackTokenProvider",
    "PrimingProvider",
    "resolve_endpoint",
]

#: The environment variable holding the gateway endpoint. Also forwarded to the
#: status child with its query string removed (KTD5), which is why this module
#: and ``talaria/status/contract.py`` both have an opinion about it.
GATEWAY_URL_ENV_VAR = "TALARIA_GATEWAY_URL"

#: Hermes's ``start_server`` default bind (``hermes_cli/web_server.py:17059-17061``
#: at ``7f4d15515``) plus the gateway mount point (``:15609``). A default Hermes
#: is a loopback bind and is therefore ungated, which is what makes the
#: ``?token=`` form v0.1 targets the correct one.
DEFAULT_GATEWAY_URL = "ws://127.0.0.1:9119/api/ws"

#: The strictest-but-usable mode for the credential file. Anything with a
#: group or other bit set is refused.
CREDENTIAL_FILE_MODE = 0o600

#: Bits whose presence means "looser than 0600". Owner-execute is deliberately
#: not in this mask: ``0700`` exposes the file to nobody the way ``0640`` does,
#: and refusing it would be tidiness dressed up as a security check.
_TOO_LOOSE_MASK = 0o077

#: Where a value came from. Carried on the credential so a test can assert the
#: precedence chain by observation rather than by reconstructing it.
#:
#: ``"environment"`` was this list's first member and named the
#: ``HERMES_DASHBOARD_SESSION_TOKEN`` level. It is gone with that level (KTD8):
#: a label nothing can produce is a precedence chain a test can still claim to
#: have observed, and it is the shape a reintroduction would slip back in
#: through. ``"endpoint-url"`` is the surviving environment-borne source and
#: says which variable it means.
CredentialSource = Literal["endpoint-url", "file", "prompt", "prompt-cached"]


class CredentialError(Exception):
    """A credential could not be acquired, or a source was refused.

    Carries the offending source in its message and **never the value**, so an
    unhandled error is safe to print.
    """


@dataclass(frozen=True)
class Credential:
    """One dial's credential: the query parameter name and its value.

    ``parameter`` is ``"token"`` for every credential v0.1 can produce. It is a
    field rather than a constant because the gated forms Hermes also accepts
    (``ticket``, ``internal``) differ only in this name, and a provider that
    returns the pair is the seam that keeps them from becoming a rewrite.
    """

    parameter: str
    value: str
    source: CredentialSource

    def __repr__(self) -> str:
        """Withhold the value.

        The generated repr would place the credential into every traceback and
        every failed assertion that happens to hold one. This is the same
        reasoning as the frame log's redaction boundary, applied to the one
        object that always holds a live secret.
        """
        return f"Credential(parameter={self.parameter!r}, source={self.source!r}, value=<withheld>)"


@runtime_checkable
class CredentialProvider(Protocol):
    """Acquires a credential for one dial.

    Declared as a ``Protocol`` so a test double is a provider without inheriting
    anything, matching :class:`~talaria.transport.source.FrameSource`.

    :meth:`acquire` is called on **every** dial, including every reconnect
    (KTD11). An implementation that caches must document why, as
    :class:`LoopbackTokenProvider` does for its prompt level.
    """

    async def acquire(self) -> Credential:
        """Return the credential for the dial about to be made."""
        ...


@runtime_checkable
class PrimingProvider(Protocol):
    """A provider that can resolve once *before* the interface owns the terminal.

    Deliberately **not** folded into :class:`CredentialProvider`. That protocol
    is satisfied by any object with an ``acquire``, which is what lets a test
    double be a provider without inheriting anything; requiring a second method
    would break every such double for the benefit of one caller. So the launcher
    asks structurally — ``isinstance(provider, PrimingProvider)`` — and a
    provider that has nothing to prime is simply not primed.
    """

    async def prime(self) -> Credential:
        """Resolve a credential now, and forbid interactive prompting after."""
        ...


def _has_controlling_terminal() -> bool:
    """Whether there is a terminal to ask on *and* to turn echo off on.

    Separate function so the refusal below is testable without a subprocess and
    without taking away the terminal the test suite is running on.
    """
    try:
        with open("/dev/tty"):
            return True
    except OSError:
        return False


async def _hidden_prompt(label: str) -> str:
    """Read a credential without echoing it, or refuse to read it at all.

    :func:`getpass.getpass` turns off terminal echo through ``termios`` for the
    duration of the read. When it cannot find a terminal it does **not** give
    up: it warns ``Password input may be echoed`` and falls back to a plain
    :func:`input`. That fallback is refused here rather than used. A credential
    read with echo on is a credential written to the scrollback of whatever
    launched Talaria, and this module's contract is that nothing in it prints,
    logs or reprs a credential value (R9) — a contract the standard library's
    fallback would quietly break on exactly the unattended launches least likely
    to be watched.

    Refusing raises :class:`EOFError`, which :meth:`LoopbackTokenProvider.acquire`
    already turns into a :class:`CredentialError` naming both non-interactive
    ways to supply the credential.

    The read runs in a worker thread because it blocks on the operator.
    """
    if not _has_controlling_terminal():
        raise EOFError("no controlling terminal to prompt on without echoing")
    return await asyncio.to_thread(getpass.getpass, label)


class LoopbackTokenProvider:
    """KTD11's v0.1 provider: the ungated loopback ``?token=`` credential.

    Every dependency is injected — the environment mapping, the credential file
    path, and the prompt function — so the precedence chain is testable without
    touching the operator's real environment or terminal.
    """

    #: What the operator sees at the interactive level. No default value is
    #: shown and nothing is echoed.
    PROMPT_LABEL = "Hermes gateway session token: "

    def __init__(
        self,
        *,
        credentials_path: Path | None = None,
        environ: Mapping[str, str] | None = None,
        prompt: Callable[[str], object] | None = None,
        allow_prompt: bool = True,
    ) -> None:
        self._credentials_path = credentials_path
        self._environ = environ
        self._prompt = prompt
        self._allow_prompt = allow_prompt
        self._remembered: str | None = None
        #: How many dials have asked for a credential. Public because KTD11's
        #: per-dial rule is otherwise invisible: a provider that were called
        #: once and cached would behave identically until the first reconnect.
        self.acquisitions = 0

    @property
    def environ(self) -> Mapping[str, str]:
        # Read late rather than captured at construction, so a rotated
        # environment variable is picked up by the next dial even when the
        # provider was built before the rotation.
        return self._environ if self._environ is not None else os.environ

    async def acquire(self) -> Credential:
        """Walk the precedence chain and return a credential. Never logs it."""
        self.acquisitions += 1
        return await self._resolve()

    async def prime(self) -> Credential:
        """Resolve once *before* the interface starts, then seal the prompt.

        Two things happen here, and the second is the one that matters. The
        first is ordinary: walk the same chain :meth:`acquire` walks, at a moment
        when the terminal is still an ordinary terminal, so level 4 can actually
        ask the operator a question and be answered.

        The second is that the prompt is sealed **whichever level answered**, and
        sealed even when this call raises. After this returns, no dial can reach
        :func:`getpass.getpass`, so the failure mode this method exists to
        prevent — a hidden prompt underneath a running full-screen interface,
        indistinguishable from a hung connection — is unreachable rather than
        merely unlikely. A credential that disappears mid-session (a file
        deleted, an endpoint variable rotated to one carrying no token)
        therefore surfaces on the next
        reconnect as :class:`CredentialError`, which
        :meth:`~talaria.transport.source.LiveSource._dial` already reports as
        ``credential_unavailable`` with the reason on screen.

        Not counted in :attr:`acquisitions`: that counter measures dials, and
        priming is not one. Counting it here would make the KTD11 per-dial
        evidence read one high forever, which is worse than not counting at all —
        a number that is always wrong by one is a number nobody can use.
        """
        try:
            return await self._resolve()
        finally:
            self._allow_prompt = False

    async def _resolve(self) -> Credential:
        env = self.environ

        value = _token_in_url(env.get(GATEWAY_URL_ENV_VAR))
        if value:
            return Credential(parameter="token", value=value, source="endpoint-url")

        value = self._from_file()
        if value:
            return Credential(parameter="token", value=value, source="file")

        if self._remembered is not None:
            # KTD11: an operator-typed credential is never re-prompted, because
            # a reconnect that waits for a human is a reconnect that does not
            # happen while the operator is away from the terminal.
            return Credential(parameter="token", value=self._remembered, source="prompt-cached")

        if not self._allow_prompt:
            raise CredentialError(
                "no gateway credential: run `talaria refresh-credential` to write "
                f"{self._describe_path()} at mode 0600, or put a token query "
                f"parameter on {GATEWAY_URL_ENV_VAR}, or allow the interactive prompt"
            )

        prompt = self._prompt if self._prompt is not None else _hidden_prompt
        try:
            typed = await _await_maybe(prompt(self.PROMPT_LABEL))
        except (EOFError, OSError) as exc:
            # There is no terminal to ask on. :func:`getpass.getpass` falls back
            # to reading stdin, and a closed or absent stdin makes that an
            # ``EOFError`` — which used to escape this whole module as an
            # unhandled exception, surfacing three layers up as "the frame
            # stream failed" and telling an operator running Talaria under a
            # supervisor nothing about the credential they need to supply.
            raise CredentialError(
                f"no terminal to prompt for a gateway credential ({exc}); run "
                f"`talaria refresh-credential` to write {self._describe_path()} at "
                f"mode 0600, or put a token query parameter on {GATEWAY_URL_ENV_VAR}"
            ) from exc
        value = _clean(typed)
        if not value:
            raise CredentialError("no gateway credential was entered at the prompt")
        self._remembered = value
        return Credential(parameter="token", value=value, source="prompt")

    # ── the file level ───────────────────────────────────────────────────

    def _describe_path(self) -> str:
        return str(self._credentials_path) if self._credentials_path is not None else "<unset>"

    def _from_file(self) -> str | None:
        """Read ``token`` out of the credential file, refusing loose permissions."""
        path = self._credentials_path
        if path is None or not path.exists():
            return None

        document = _read_credential_file(path)
        raw = document.get("token")
        if raw is None:
            return None
        if not isinstance(raw, str):
            raise CredentialError(f"{path}: token must be a string")
        return _clean(raw)


def _read_credential_file(path: Path) -> Mapping[str, object]:
    """Parse the credential file, refusing it unless its mode is 0600 or stricter.

    The mode check comes before the read, not after: a file the whole machine
    can read has already leaked, and opening it anyway to produce a nicer error
    would be the one action that makes the leak useful.

    This is one function rather than two because it was two, and the second one
    forgot. :func:`resolve_endpoint` opened the same file for its ``url`` key
    with no ``stat`` at all, so a world-writable credential file could not hand
    over a token — but could still redirect the dial to a host of the writer's
    choosing while the token came from the environment and was delivered there
    intact. The gate now belongs to the file, not to one of its keys.
    """
    mode = path.stat().st_mode & 0o777
    if mode & _TOO_LOOSE_MASK:
        raise CredentialError(
            f"{path} has mode {mode:04o}; the gateway credential file must be "
            f"{CREDENTIAL_FILE_MODE:04o} or stricter. Run: chmod 600 {path}"
        )

    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)
    except tomllib.TOMLDecodeError as exc:
        raise CredentialError(
            f'{path} is not valid TOML: {exc}. Expected a line like: token = "…"'
        ) from exc
    except OSError as exc:
        raise CredentialError(f"{path} could not be read: {exc}") from exc


def resolve_endpoint(
    *,
    environ: Mapping[str, str] | None = None,
    credentials_path: Path | None = None,
    override: str | None = None,
) -> str:
    """Resolve the gateway endpoint, **without** its credential query.

    Precedence: an explicit override, ``TALARIA_GATEWAY_URL``, a ``url`` key in
    the credential file, then :data:`DEFAULT_GATEWAY_URL`. The credential is
    stripped here rather than downstream, so no caller can accidentally hold a
    credential-bearing endpoint: the provider is the only thing that carries a
    value, and :class:`~talaria.transport.attach.AttachTarget` puts it back for
    exactly one dial.

    Raises :class:`CredentialError` for a credential file this process will not
    trust and for an endpoint that will not parse — never a bare ``ValueError``
    out of ``urlsplit``. :meth:`~talaria.transport.attach.AttachTarget.from_environment`
    turns both into a named failure state, which is the shape the rest of the
    transport is in.
    """
    from talaria.transport.attach import strip_credential_query

    env = environ if environ is not None else os.environ

    candidate = _clean(override) or _clean(env.get(GATEWAY_URL_ENV_VAR))
    source = f"{GATEWAY_URL_ENV_VAR}" if candidate and override is None else "the endpoint"
    if not candidate and credentials_path is not None and credentials_path.exists():
        url = _read_credential_file(credentials_path).get("url")
        candidate = _clean(url) if isinstance(url, str) else None
        source = f"the url key in {credentials_path}"

    try:
        return strip_credential_query(candidate or DEFAULT_GATEWAY_URL)
    except ValueError as exc:
        # The value itself is deliberately not echoed: a URL malformed enough to
        # defeat ``urlsplit`` cannot be redacted by parsing it, and it is exactly
        # the kind of string an operator pastes a token into.
        raise CredentialError(f"{source} is not a valid URL: {exc}") from exc


def _clean(value: object) -> str | None:
    """A non-empty stripped string, or ``None``.

    Whitespace is stripped because the most common way to write a credential
    file is ``echo "$TOKEN" > …``, which appends a newline that the gateway's
    constant-time comparison would then reject with no useful message.
    """
    if not isinstance(value, str):
        return None
    stripped = value.strip()
    return stripped or None


def _token_in_url(url: object) -> str | None:
    """The ``token`` query parameter of an endpoint URL, if it carries one."""
    if not isinstance(url, str) or "://" not in url:
        return None
    try:
        parts = urlsplit(url)
    except ValueError:
        return None
    for name, value in parse_qsl(parts.query, keep_blank_values=True):
        if name == "token":
            return _clean(value)
    return None


async def _await_maybe(value: object) -> object:
    """Await ``value`` when it is awaitable, else return it.

    The prompt hook is allowed to be either a coroutine function or a plain one:
    :func:`getpass.getpass` wrapped in a thread is async, and a test double that
    returns a string is not. Accepting both keeps the double from having to be
    async for reasons that have nothing to do with what it is proving.
    """
    if asyncio.iscoroutine(value):
        return await value
    return value
