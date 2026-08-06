"""talaria/cli.py — the ``talaria`` console-script entry point.

Implements KTD7's startup precedence: an explicit ``--session <id>`` beats
``--resume`` beats default-new, and the conflicting pair is a usage error
raised before any connection is dialed. Later units attach subcommands
(U2's recorder, U5's replay shell) to the parser built here rather than
building their own.
"""

from __future__ import annotations

import argparse
import asyncio
import sys
from pathlib import Path
from typing import TYPE_CHECKING
from urllib.parse import parse_qsl, urlsplit

from talaria import config as config_module
from talaria.domain.commands import PasteThreshold
from talaria.domain.startup import (
    StartupConflictError,
    StartupMode,
    StartupSelection,
    resolve_startup,
)

if TYPE_CHECKING:
    from talaria.status.runner import StatusRunner
    from talaria.transport.source import LiveSource
    from talaria.ui.app import TalariaApp

__all__ = [
    "StartupConflictError",
    "StartupMode",
    "StartupSelection",
    "build_parser",
    "main",
    "parse_args",
    "resolve_startup",
]


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="talaria",
        description="Hermes-native terminal UI client",
    )
    parser.add_argument(
        "--session",
        metavar="ID",
        help="attach to an explicit session id",
    )
    parser.add_argument(
        "--resume",
        action="store_true",
        help="resume the most recently used session (session.most_recent)",
    )
    parser.add_argument(
        "--record",
        nargs="?",
        const="",
        metavar="PATH",
        help="record every frame of this live session to a frame log "
        "(default: a timestamped file under <config_dir>/recordings/, KTD15)",
    )

    subparsers = parser.add_subparsers(dest="command")
    record_parser = subparsers.add_parser(
        "record",
        help="attach to a Hermes gateway and record every frame (R25-R29)",
    )
    record_parser.add_argument(
        "url",
        nargs="?",
        metavar="ENDPOINT",
        help="gateway websocket endpoint, overriding the configured one, e.g. "
        "ws://127.0.0.1:9119/api/ws. Optional: with no argument the endpoint is "
        "resolved exactly as a bare `talaria` launch resolves it. An endpoint "
        "carrying a credential is refused (R9)",
    )
    record_parser.add_argument(
        "--out",
        metavar="PATH",
        help="frame-log output path (default: a timestamped file under "
        "<config_dir>/recordings/, KTD15)",
    )

    replay_parser = subparsers.add_parser(
        "replay",
        help="drive the full interface from a recorded frame log, with no gateway (R30)",
    )
    replay_parser.add_argument("corpus", help="path to a frame-log v1 recording")
    replay_parser.add_argument(
        "--speed",
        type=float,
        default=1.0,
        help="replay rate multiplier; 0 or inf means as fast as possible (R40)",
    )
    replay_parser.add_argument(
        "--paused",
        action="store_true",
        help="start paused",
    )

    refresh_parser = subparsers.add_parser(
        "refresh-credential",
        help="rewrite the credential file from a running Hermes dashboard's session token",
    )
    refresh_parser.add_argument(
        "--from",
        dest="dashboard",
        metavar="URL",
        help="dashboard http URL (default: derived from the configured gateway endpoint)",
    )
    refresh_parser.add_argument(
        "--timeout",
        type=float,
        default=10.0,
        metavar="SECONDS",
        help="how long to wait for the dashboard to answer (default: 10)",
    )

    gate_parser = subparsers.add_parser(
        "gate",
        help="run the framework validation gate and print its measurements as JSON",
    )
    gate_parser.add_argument(
        "--corpus",
        metavar="PATH",
        help="recorded frame log to replay alongside the synthetic stress corpus",
    )
    gate_parser.add_argument(
        "--deltas",
        type=int,
        default=50_000,
        help="size of the generated stress corpus, in message deltas (KTD14: 50000)",
    )
    gate_parser.add_argument(
        "--seed", type=int, default=20260802, help="stress-corpus generation seed"
    )
    gate_parser.add_argument(
        "--json", metavar="PATH", help="write the full measurement record here"
    )
    return parser


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.session and args.resume:
        parser.error("--session and --resume are mutually exclusive")
    return args


def selection_from_args(args: argparse.Namespace) -> StartupSelection:
    """Adapt parsed arguments to the domain's precedence function (KTD7).

    The policy itself lives in :mod:`talaria.domain.startup`. This wrapper only
    unpacks the namespace, so the precedence is tested once — framework-free,
    against every AE12 combination — rather than once per caller.
    """
    return resolve_startup(session=args.session, resume=bool(args.resume))


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)

    if getattr(args, "command", None) == "record":
        return run_record_command(args)

    if getattr(args, "command", None) == "replay":
        return run_replay(args)

    if getattr(args, "command", None) == "refresh-credential":
        return run_refresh_credential(args)

    if getattr(args, "command", None) == "gate":
        return run_gate_command(args)

    return run_live(args)


def _build_status_runner(cfg: config_module.Config) -> StatusRunner | None:
    """Construct U6's runner from configuration, or ``None`` when unconfigured.

    Imported lazily for the same reason ``record`` is: the default launch path
    has no business paying for a module it will not use.
    """
    from talaria.status.contract import parse_command
    from talaria.status.runner import StatusRunner

    argv = parse_command(cfg.get("status", "command"))
    if argv is None:
        return None
    allowlist = cfg.get("environment", "allowlist", default=[]) or []
    return StatusRunner(
        argv=argv,
        launch_cwd=Path.cwd(),
        allowlist=tuple(str(name) for name in allowlist),
    )


def url_carries_credential(url: str) -> bool:
    """Whether the operator's raw ``record`` argument carries a credential (R5).

    Inspects the **raw** argument, before anything strips it (KTD3).
    :meth:`~talaria.transport.attach.AttachTarget.from_url` removes credential
    query parameters as an invariant, so a check placed downstream of it would
    be examining a string that can no longer hold the thing being looked for and
    would report "clean" every time.

    Three shapes count, and they count for different reasons.

    A credential *query parameter* — the names in
    :data:`~talaria.transport.attach.CREDENTIAL_QUERY_KEYS` — is the form the
    Hermes upgrade path actually reads, so this is the URL that used to work and
    that leaked while it worked. The constant is imported rather than restated:
    the refusal and the stripping must not disagree the first time one of them
    gains a key.

    *Userinfo* — ``ws://user:pass@host/api/ws`` — would never have authenticated,
    because Hermes reads the upgrade credential only from query parameters. What
    it did do is put a secret into the process table and the shell history, which
    is the entire thing KTD1 exists to tell the operator about. This repository
    already treats userinfo as a credential everywhere else
    (:func:`~talaria.recorder.redact.redact_url` withholds it deliberately), so
    keying the refusal on query parameters alone would leave a hole with exactly
    the shape of the one this closes.

    A *fragment* — ``ws://host/api/ws#token=…`` — is refused whatever it holds,
    and of the three it is the shape with the worst consequences. Like userinfo
    it can never authenticate: a fragment is by definition not sent to a server,
    and ``websockets`` rejects such a URI outright ("fragment identifier is
    meaningless"). Unlike userinfo it is withheld by *nothing* downstream —
    :func:`~talaria.transport.attach.strip_credential_query` drops credential
    query keys and :func:`~talaria.recorder.redact.redact_url` withholds
    userinfo, but neither touches a fragment, so a credential in one reached the
    printed endpoint and the frame-log header verbatim. Because a WebSocket
    endpoint has no legitimate use for a fragment at all, any fragment is
    refused rather than sniffed for credential-shaped keys: guessing at the
    contents of a component that should not be there is a worse boundary than
    rejecting the component.

    A URL that :func:`~urllib.parse.urlsplit` cannot read is treated as carrying
    one. It cannot be shown to be credential-free, and a string malformed enough
    to defeat ``urlsplit`` is exactly what an operator produces by pasting a
    credential into a URL by hand.
    """
    from talaria.transport.attach import CREDENTIAL_QUERY_KEYS

    try:
        parts = urlsplit(url)
    except ValueError:
        return True
    if "@" in parts.netloc:
        return True
    if parts.fragment:
        return True
    return any(
        name.lower() in CREDENTIAL_QUERY_KEYS
        for name, _value in parse_qsl(parts.query, keep_blank_values=True)
    )


def _record_credential_refusal() -> str:
    """The refusal ``talaria record`` prints, which reproduces nothing (R6).

    Not one character of the operator's argument appears here — not the value,
    not the URL that carried it, not a fragment of either. By the time this runs
    the string is already in two places it should not be, the process table and
    the shell history; writing it to stderr would add a third, and stderr is a
    log file in continuous integration. The existing
    :func:`~talaria.transport.credentials.resolve_endpoint` makes the same choice
    for the same reason.

    Built by a function rather than held as a module constant so the environment
    variable's name comes from the one place that defines it.
    """
    from talaria.transport.credentials import TOKEN_ENV_VAR

    return "\n".join(
        (
            "talaria: refusing to record: that endpoint carries a credential.",
            "",
            "  A credential on a command line is readable by anyone who can run `ps`"
            " while the",
            "  command runs, and your shell has already written it to history."
            " Treat the value",
            "  you just passed as exposed, and rotate it now.",
            "",
            "  Two routes supply a credential without putting it on a command line:",
            "    talaria refresh-credential"
            "   — rewrites the credential file at mode 0600, printing nothing secret",
            f"    {TOKEN_ENV_VAR}"
            "   — read from the environment, ahead of the file",
            "",
            "  Then record with no argument, or with an endpoint that carries no"
            " credential:",
            "    talaria record",
            "    talaria record ws://<host>:<port>/api/ws",
        )
    )


def run_record_command(args: argparse.Namespace) -> int:
    """Dispatch ``talaria record`` (KTD7).

    Two operator errors exit 2 here rather than inside
    :func:`~talaria.recorder.command.run_record`: a credential on the command
    line, and a credential the chain could not supply. ``run_record``'s
    documented contract is 0 for a normal close and 1 for never-attached or a
    write failure, mirroring the TypeScript reference; a refusal is neither, and
    2 is what the sibling operator-error path (:func:`run_refresh_credential`)
    already returns.

    The refusal is checked before anything else is constructed, because
    construction is where the stripping lives and stripping is the silent
    behaviour KTD1 rejects.
    """
    # Imported here, not at module top level: `talaria.recorder.command` pulls in
    # `websockets`, and the default (no-subcommand) launch path has no business
    # paying for that import.
    from talaria.recorder.command import resolve_record_target, run_record
    from talaria.transport.credentials import CredentialError

    if args.url is not None and url_carries_credential(args.url):
        print(_record_credential_refusal(), file=sys.stderr)
        return 2

    cfg = config_module.load_config()
    credentials = config_module.credentials_path(cfg.config_dir)

    try:
        # Resolved before the recorder exists and before a socket is opened. The
        # interactive level of the chain can therefore reach a terminal that is
        # still an ordinary terminal — the same ordering, and the same reason, as
        # :func:`_prime_credential` on the live path.
        target = asyncio.run(
            resolve_record_target(credentials_path=credentials, override=args.url)
        )
    except CredentialError as exc:
        print(f"talaria: {exc}", file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print("talaria: cancelled at the credential prompt", file=sys.stderr)
        return 130

    out_path = Path(args.out) if args.out else None
    return asyncio.run(
        run_record(
            target,
            out=out_path,
            recordings_dir=None if out_path else config_module.recordings_dir(cfg.config_dir),
        )
    )


def run_replay(args: argparse.Namespace) -> int:
    """Launch the Textual shell over a recorded corpus (U5)."""
    from talaria.replay.controls import ReplayControls
    from talaria.replay.source import ReplaySource
    from talaria.ui.app import TalariaApp

    cfg = config_module.load_config()
    speed = args.speed if args.speed and args.speed > 0 else float("inf")
    controls = ReplayControls(speed=speed, paused=bool(args.paused))
    source = ReplaySource.from_path(args.corpus, controls=controls)
    app = TalariaApp(
        source,
        mode="replay",
        controls=controls,
        status_runner=_build_status_runner(cfg),
        status_interval=float(cfg.get("status", "interval_seconds", default=5) or 5),
        paste_threshold=_build_paste_threshold(cfg),
    )
    app.run()
    return 0


def build_live_app(
    args: argparse.Namespace, cfg: config_module.Config
) -> tuple[TalariaApp, LiveSource]:
    """Assemble the live shell: transport, credential provider, app, callbacks.

    Split out from :func:`run_live` so the assembly is testable without opening
    a socket or a screen. Everything that decides behaviour is decided here —
    which endpoint, which credential chain, which startup path, which paste
    thresholds — and :func:`run_live` only starts it.

    The credential provider is *constructed* here and acquires nothing: KTD11
    calls it once per dial, inside :class:`~talaria.transport.source.LiveSource`,
    so building the app never touches the operator's terminal or their
    credential file.

    ``--record`` is wired here for a reason worth stating. R3 — one live turn
    streamed to completion and its transcript compared against a replay of the
    same frames — is the requirement that would move this build's verdict, and
    it cannot be attempted without a recording of a live turn.
    :class:`~talaria.transport.source.LiveSource` has always accepted a recorder
    and the launcher never passed one, so the only way to record a session was
    ``talaria record``, which has no interface: an operator could have a usable
    client or a recording, never both. That is the same defect shape as the
    paste threshold that was configurable in the mode that ignored it, one level
    up.
    """
    from talaria.recorder.framelog import FrameRecorder, default_log_path
    from talaria.transport.attach import AttachTarget
    from talaria.transport.credentials import LoopbackTokenProvider
    from talaria.transport.source import LiveSource
    from talaria.ui.app import TalariaApp

    credentials = config_module.credentials_path(cfg.config_dir)
    target = AttachTarget.from_environment(credentials_path=credentials)

    recorder: FrameRecorder | None = None
    requested = getattr(args, "record", None)
    if requested is not None:
        recordings = config_module.recordings_dir(cfg.config_dir)
        if not requested:
            recordings.mkdir(parents=True, exist_ok=True)
        # ``target.url`` and not the dialled URL: the endpoint written into the
        # frame log's header has already had every credential query parameter
        # stripped by ``AttachTarget`` (R1, KTD13). The recorder redacts as well,
        # but handing it a credential and trusting the redaction would make the
        # log's safety depend on one boundary instead of two.
        recorder = FrameRecorder(
            Path(requested) if requested else default_log_path(recordings),
            target.url,
        )

    source = LiveSource(
        target,
        LoopbackTokenProvider(credentials_path=credentials),
        recorder=recorder,
    )
    app = TalariaApp(
        source,
        mode="live",
        dispatcher=source,
        status_runner=_build_status_runner(cfg),
        status_interval=float(cfg.get("status", "interval_seconds", default=5) or 5),
        paste_threshold=_build_paste_threshold(cfg),
        startup=selection_from_args(args),
    )
    # Bound after construction rather than passed in: the app is built *from*
    # the source, so wiring the source's callbacks to the app's methods at
    # construction time would be circular (see ``LiveSource.bind``).
    source.bind(on_connection=app.note_connection_state, on_reconnect=app.note_reconnect)
    return app, source


def run_live(args: argparse.Namespace) -> int:
    """Launch the live shell against a running Hermes gateway (R2, R31).

    **This path has never been run against a real Hermes gateway.** Every
    transport test in this repository dials a loopback stub. The unmet
    requirements that follow from that — live startup acceptance (R2) and a live
    turn (R3) — are recorded in
    ``docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md``, and they are why
    that document's verdict is *not ready*.

    ``--session`` and ``--resume`` are resolved before anything is dialled
    (KTD7): the conflicting pair is already a usage error out of
    :func:`parse_args`, so by the time a socket exists exactly one path has been
    selected and nothing can switch it afterwards.
    """
    cfg = config_module.load_config()
    app, source = build_live_app(args, cfg)

    # Before ``app.run()``, and that ordering is the whole point: see
    # :func:`_prime_credential`.
    failure = _prime_credential(source)
    if failure:
        return failure

    app.run()
    return app.return_code or 0


def _prime_credential(source: LiveSource) -> int:
    """Resolve the gateway credential while the terminal is still an ordinary one.

    Returns ``0`` when the launch may proceed, or the exit code to stop with.

    KTD11 puts credential acquisition inside the dial, which is right, and the
    dial happens in ``on_mount`` — inside a running Textual application that owns
    the screen and is reading stdin. When the chain fell through to its
    interactive level there, :func:`getpass.getpass` wrote its prompt where
    nothing could show it and blocked a worker thread on a read racing the UI's
    own input driver. What the operator saw was a client stuck on "connecting to
    gateway" and half-deaf to typing, with no socket ever opened and nothing on
    screen naming the cause. Diagnosing it took a process sample; it should have
    taken a sentence on screen.

    Priming here fixes the ordering without weakening KTD11. The provider is
    still called on every dial, and levels 1–3 are still re-read each time, so a
    rotated token still lands on the next reconnect. What changes is that the one
    level requiring a human runs while a human can still see the terminal, and is
    then sealed for the life of the process.
    """
    from talaria.transport.credentials import CredentialError, PrimingProvider

    provider = source.provider
    if not isinstance(provider, PrimingProvider):
        return 0

    try:
        asyncio.run(provider.prime())
    except CredentialError as exc:
        # Printed, not raised: the operator needs the remedy, and a traceback
        # for a missing credential file buries it under frames from three
        # modules that are working correctly.
        print(f"talaria: {exc}", file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print("talaria: cancelled at the credential prompt", file=sys.stderr)
        return 130
    return 0


def _build_paste_threshold(cfg: config_module.Config) -> PasteThreshold:
    """KTD16's two bounds, from configuration (KTD15).

    Non-integer values fall back to the documented defaults rather than
    raising. A malformed threshold should not stop the client from starting,
    and :class:`~talaria.domain.commands.PasteThreshold` already treats a
    non-positive bound as "this half is off".
    """
    from talaria.domain.commands import DEFAULT_COLLAPSE_BYTES, DEFAULT_COLLAPSE_LINES

    def _bound(key: str, fallback: int) -> int:
        value = cfg.get("composer", key, default=fallback)
        return value if isinstance(value, int) and not isinstance(value, bool) else fallback

    return PasteThreshold(
        lines=_bound("paste_collapse_lines", DEFAULT_COLLAPSE_LINES),
        byte_limit=_bound("paste_collapse_bytes", DEFAULT_COLLAPSE_BYTES),
    )


def run_refresh_credential(args: argparse.Namespace) -> int:
    """Rewrite the credential file from a running dashboard's session token.

    The dashboard mints its session token at server start and keeps it in
    memory, so every dashboard restart leaves ``<config_dir>/credentials``
    holding a value that will be refused. Before this command the remedy was to
    read the token out of the served page by hand, which is a thing an operator
    should not be doing with a credential at a shell prompt where it lands in
    shell history.

    The token is never printed here. What is printed is which file was written,
    which dashboard it came from, and which of the file's other keys survived —
    enough to confirm the command did what was intended without putting the
    value on a screen or into a scrollback.
    """
    from talaria.transport.attach import AttachTarget
    from talaria.transport.refresh import (
        RefreshError,
        dashboard_origin_for,
        refresh_credential,
    )

    cfg = config_module.load_config()
    credentials = config_module.credentials_path(cfg.config_dir)

    try:
        origin = args.dashboard
        if not origin:
            target = AttachTarget.from_environment(credentials_path=credentials)
            if target.problem:
                raise RefreshError(target.problem)
            origin = dashboard_origin_for(target.url)
        report = refresh_credential(origin, credentials, timeout=args.timeout)
    except RefreshError as exc:
        print(f"talaria: {exc}", file=sys.stderr)
        return 2

    action = "created" if report.created else "updated"
    print(f"{action} {report.path} (mode 0600) from {report.origin}")
    if report.tightened:
        print("  permissions tightened to 0600; the previous file was readable by others")
    if report.preserved_keys:
        print(f"  kept: {', '.join(report.preserved_keys)}")
    return 0


def run_gate_command(args: argparse.Namespace) -> int:
    """Run the framework validation gate; exit non-zero on a fail verdict."""
    import json

    from talaria.replay.gate import run_gate

    result = asyncio.run(
        run_gate(live_corpus=args.corpus, deltas=args.deltas, seed=args.seed)
    )
    document = json.dumps(result.to_dict(), indent=2, sort_keys=True)
    if args.json:
        Path(args.json).write_text(f"{document}\n", encoding="utf-8")
    print(document)
    return 0 if result.passed else 1


if __name__ == "__main__":
    sys.exit(main())
