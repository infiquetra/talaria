"""R1's process-surface sweep: what a running Talaria's argv and environ carry.

KTD13 says the gateway credential rides the WebSocket URL's ``?token=`` query
parameter and never argv, because at Hermes ``7f4d15515`` the upgrade credential
is read exclusively from ``ws.query_params``
(``hermes_cli/web_server.py:14443-14524``) and because argv is world-readable on
both platforms Talaria targets. R1 asks for that to be *measured* on a running
process rather than reasoned about.

**The finding is narrower than R1's wording, and it is stated narrowly on
purpose.** The operator supplies the credential through
``HERMES_DASHBOARD_SESSION_TOKEN``, and Talaria inherits it. A process cannot
remove what ``/proc/<pid>/environ`` captured at ``exec`` time — that snapshot is
taken by the kernel before any Python runs, and ``os.environ.pop`` does not
change it. So the two halves are measured separately:

* **Holds, and is checked here.** Talaria's own command line carries neither the
  credential nor any ``?token=``-bearing URL, and Talaria adds nothing
  credential-shaped to its own environment: what is there is exactly what it was
  launched with.
* **Cannot hold, and is recorded rather than redefined.** An inherited
  ``HERMES_DASHBOARD_SESSION_TOKEN`` is visible in the process environment for
  the process's whole life, on Linux by kernel snapshot and on macOS through
  ``ps -E`` to the owning user.

The one environment Talaria *does* control is its status child's, and that one
is checked in full: KTD5's ``build_child_env`` is what makes it so, and
``tests/status/test_env.py`` covers the boundary. What this file adds is the
same assertion against a **running** process's surface as another process can
read it.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

import pytest

from talaria.transport.credentials import GATEWAY_URL_ENV_VAR, TOKEN_ENV_VAR

REPO_ROOT = Path(__file__).resolve().parents[2]

#: A credential-shaped value distinctive enough that one substring search over a
#: whole process surface settles whether it is there. Not a real token, and not
#: accepted by anything.
CANARY_TOKEN = "R1-CANARY-TOKEN-8Kd3nQ7wPz"

#: An endpoint carrying the canary the way an operator's exported
#: ``TALARIA_GATEWAY_URL`` would (KTD11's second precedence level).
CANARY_URL = f"ws://127.0.0.1:19119/api/ws?token={CANARY_TOKEN}"

#: What the probe subprocess runs. It builds the live launcher exactly as
#: ``talaria`` does — same :class:`~talaria.transport.attach.AttachTarget`, same
#: credential provider — and then sits still without dialling, because the point
#: is what the *process surface* looks like while it holds those objects.
#:
#: The endpoint deliberately points at port 19119, which nothing serves, and the
#: probe never calls ``start()``. A test that dialled would attach to whatever
#: gateway happens to be running on the machine.
#: **Nothing in this string may name the endpoint, the port, or the token.** The
#: probe is launched as ``python -c <this source>``, so every literal here lands
#: in the probe's own argv — and the sweep below searches argv for exactly those
#: strings. A hard-coded ``19119`` in this text failed the argv assertion for a
#: leak the harness had written itself. The expected host is therefore read back
#: out of the environment at runtime.
PROBE = """
import asyncio, os, sys, time
sys.path.insert(0, {root!r})
from talaria import config as config_module
from talaria.cli import build_live_app, parse_args
from talaria.transport.credentials import LoopbackTokenProvider
from urllib.parse import urlsplit

cfg = config_module.load_config()
app, source = build_live_app(parse_args([]), cfg)
# Prove the object under test really holds the configured endpoint, so a probe
# that failed to build one cannot pass the sweep by being empty.
expected = urlsplit(os.environ["TALARIA_GATEWAY_URL"]).netloc
assert expected and expected in source.safe_url, source.safe_url

# Acquire a credential and keep it, without dialling anything. This is what
# makes the sweep meaningful: the process under inspection is *holding a live
# credential in memory* for the whole observation, which is the state R1 is
# actually about. A probe that had never resolved one would pass every
# "no credential on the surface" assertion by having no credential at all.
provider = LoopbackTokenProvider(
    credentials_path=config_module.credentials_path(cfg.config_dir), allow_prompt=False
)
held = asyncio.run(provider.acquire())
assert held.value, "the probe resolved no credential"
print("PROBE-READY", flush=True)
time.sleep(30)
"""


class Surface:
    """One running process's command line and environment, as another sees them.

    **Never put** :attr:`environ` **on either side of an assertion.** This is a
    public repository with public CI logs, and the block it holds is the whole
    inherited environment of whoever ran the suite. pytest prints the operands
    of a failing assertion — including a long string's leading characters, even
    when an explicit message is supplied — so ``assert CANARY in surface.environ``
    publishes an arbitrary slice of the developer's real environment the first
    time it goes red. Use :meth:`carries` and :meth:`names_carrying`, which
    answer the same questions with booleans and variable *names*.
    """

    def __init__(self, argv: str, environ: str) -> None:
        self.argv = argv
        self._environ = environ

    @property
    def environ_is_readable(self) -> bool:
        """Whether anything was read at all. The positive control for the rest."""
        return bool(self._environ.strip())

    def carries(self, needle: str) -> bool:
        return needle in self._environ

    def names_carrying(self, needle: str) -> set[str]:
        """The environment variable *names* whose entry contains ``needle``.

        Names only, never values: a leaked name is a fact about Talaria, and a
        leaked value is a fact about the machine that ran the test.
        """
        return {
            line.split("=", 1)[0]
            for line in self._environ.replace(" ", "\n").splitlines()
            if needle in line and "=" in line
        }


def read_surface(pid: int) -> Surface:
    """Read ``pid``'s argv and environ through the platform's own facility.

    Linux exposes both under ``/proc``; macOS exposes them through ``ps``, where
    ``-E`` appends the environment to the command column. Both are read rather
    than one being simulated, because R1 is a claim about what *another process*
    can see, and only the platform's own view answers that.
    """
    proc = Path(f"/proc/{pid}")
    if proc.exists():  # pragma: no cover - exercised on Linux only
        cmdline = (proc / "cmdline").read_bytes().replace(b"\0", b" ").decode("utf-8", "replace")
        environ = (proc / "environ").read_bytes().replace(b"\0", b"\n").decode("utf-8", "replace")
        return Surface(argv=cmdline, environ=environ)

    argv = subprocess.run(  # noqa: S603 - a fixed argv, no shell
        ["ps", "-ww", "-o", "command=", "-p", str(pid)],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    both = subprocess.run(  # noqa: S603 - a fixed argv, no shell
        ["ps", "-Eww", "-o", "command=", "-p", str(pid)],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    # ``ps -E`` prints the command followed by the environment in one column, so
    # the environment is what remains once the command has been taken off the
    # front. Sliced rather than split on whitespace: an argument may contain
    # spaces, and a split would move part of argv into the environment or the
    # other way around.
    environ = both[len(argv.rstrip("\n")) :] if both.startswith(argv.rstrip("\n")) else both
    return Surface(argv=argv, environ=environ)


def start_probe(env: dict[str, str]) -> subprocess.Popen[str]:
    process = subprocess.Popen(
        [sys.executable, "-c", PROBE.format(root=str(REPO_ROOT))],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
        cwd=str(REPO_ROOT),
    )
    assert process.stdout is not None
    deadline = time.monotonic() + 60.0
    while time.monotonic() < deadline:
        line = process.stdout.readline()
        if "PROBE-READY" in line:
            return process
        if process.poll() is not None:
            stderr = process.stderr.read() if process.stderr else ""
            raise AssertionError(f"the probe exited before it was ready:\n{stderr}")
    process.kill()  # pragma: no cover - a hung probe
    raise AssertionError("the probe never became ready")


@pytest.fixture
def probe_environment(tmp_path: Path) -> dict[str, str]:
    """The operator's environment, credential included, as KTD11 expects it."""
    env = dict(os.environ)
    env[TOKEN_ENV_VAR] = CANARY_TOKEN
    env[GATEWAY_URL_ENV_VAR] = CANARY_URL
    env["TALARIA_CONFIG_DIR"] = str(tmp_path / "config")
    (tmp_path / "config").mkdir(parents=True, exist_ok=True)
    return env


# ── the half that holds ──────────────────────────────────────────────────


def test_a_running_talarias_command_line_carries_no_credential(
    probe_environment: dict[str, str],
) -> None:
    """R1's argv clause, measured on a live process.

    The positive pair matters here more than usual: an empty command line would
    satisfy every negative below. So the sweep also asserts the command line is
    the one that launched *this* probe.
    """
    probe = start_probe(probe_environment)
    try:
        surface = read_surface(probe.pid)
        assert "python" in surface.argv.lower(), f"read no command line at all: {surface.argv!r}"
        assert CANARY_TOKEN not in surface.argv, "the credential is on the command line"
        assert "token=" not in surface.argv, "a ?token= bearing URL is on the command line"
        assert "19119" not in surface.argv, (
            "the gateway endpoint reached argv, which is where a token would ride with it"
        )
    finally:
        probe.kill()
        probe.wait()


def test_talaria_adds_no_credential_of_its_own_to_its_environment(
    probe_environment: dict[str, str],
) -> None:
    """The half of R1's environment clause that a process *can* control.

    Talaria holds a live credential — the provider mints one per dial — and the
    question this answers is whether any of that is ever written back out into
    the process environment, where a child, a crash reporter, or ``ps`` would
    pick it up.

    **Asserted as equality, not as a subset.** ``carrying <= launched_with`` is
    the natural way to write "added nothing", and an empty ``carrying`` satisfies
    it — so a platform whose environment read came back blind would pass this
    test while proving nothing. Equality carries the positive half in the same
    observation: the reader really did see both names the probe was launched
    with, *and* Talaria added no third one.
    """
    probe = start_probe(probe_environment)
    try:
        surface = read_surface(probe.pid)
        assert surface.environ_is_readable, "read no environment at all"

        carrying = surface.names_carrying(CANARY_TOKEN)
        launched_with = {
            name for name, value in probe_environment.items() if CANARY_TOKEN in value
        }
        assert launched_with == {TOKEN_ENV_VAR, GATEWAY_URL_ENV_VAR}
        assert carrying == launched_with, (
            "the set of environment names carrying the credential is not the set "
            f"the probe was launched with: extra={sorted(carrying - launched_with)} "
            f"unseen={sorted(launched_with - carrying)}"
        )
    finally:
        probe.kill()
        probe.wait()


def test_the_endpoint_talaria_holds_has_had_its_credential_stripped(
    probe_environment: dict[str, str],
) -> None:
    """The in-process half of the same claim, and the reason argv stays clean.

    :class:`~talaria.transport.attach.AttachTarget` strips every credential
    query parameter at construction, so the object every other module holds is
    credential-free by construction rather than by discipline. This is what the
    probe's ``safe_url`` assertion above depends on.
    """
    from talaria.transport.attach import AttachTarget

    target = AttachTarget.from_url(CANARY_URL)

    assert "19119" in target.url, "the endpoint was lost, not stripped"
    assert CANARY_TOKEN not in target.url
    assert CANARY_TOKEN not in target.safe_url
    assert CANARY_TOKEN not in repr(target)


# ── the half that cannot hold, measured rather than assumed ──────────────


def test_the_inherited_credential_is_visible_in_the_process_environment(
    probe_environment: dict[str, str],
) -> None:
    """R1 as written is **not** met, and this is the measurement that says so.

    This test asserts the *failure*. It is here so the limitation is a checked
    fact with a name rather than a sentence in a document that could drift away
    from the code — and so that if a future Talaria ever does manage to scrub
    its inherited environment, this test fails and someone has to come and
    delete it deliberately.

    Why it cannot be fixed by scrubbing: on Linux the kernel snapshots the
    environment block at ``exec`` and ``/proc/<pid>/environ`` serves that
    snapshot for the life of the process, so ``os.environ.pop`` changes nothing
    a reader can see. The mitigation is on the operator's side — supply the
    credential through the ``0600`` credential file
    (``<config_dir>/credentials``) instead of the environment, which is KTD11's
    third precedence level and is exactly why that level exists.
    """
    probe = start_probe(probe_environment)
    try:
        surface = read_surface(probe.pid)
        assert surface.carries(CANARY_TOKEN), (
            "the inherited credential is no longer visible in the process "
            "environment — if Talaria now scrubs it, R1's environment clause is "
            "met and this test should be deleted rather than relaxed"
        )
        assert TOKEN_ENV_VAR in surface.names_carrying(CANARY_TOKEN)
    finally:
        probe.kill()
        probe.wait()


def test_the_credential_file_route_keeps_the_environment_clean(tmp_path: Path) -> None:
    """The mitigation, measured: no credential in the environment at all.

    KTD11's file level is what an operator uses when the process surface matters,
    and this is the pair to the test above — the same launcher, the same running
    process, with the credential supplied the other way.
    """
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    credentials = config_dir / "credentials"
    credentials.write_text(f'token = "{CANARY_TOKEN}"\n', encoding="utf-8")
    credentials.chmod(0o600)

    env = dict(os.environ)
    env.pop(TOKEN_ENV_VAR, None)
    env[GATEWAY_URL_ENV_VAR] = "ws://127.0.0.1:19119/api/ws"
    env["TALARIA_CONFIG_DIR"] = str(config_dir)

    probe = start_probe(env)
    try:
        surface = read_surface(probe.pid)
        assert surface.environ_is_readable, "read no environment at all"
        assert not surface.carries(CANARY_TOKEN), (
            "a file-sourced credential reached the process environment"
        )
        assert CANARY_TOKEN not in surface.argv
    finally:
        probe.kill()
        probe.wait()
