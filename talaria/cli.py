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
        help="gateway websocket URL, e.g. ws://127.0.0.1:9119/api/ws?token=<token>",
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
        # Imported here, not at module top level: `talaria.recorder.command`
        # pulls in `websockets`, and the default (no-subcommand) launch path
        # has no business paying for that import.
        from talaria.recorder.command import run_record

        cfg = config_module.load_config()
        out_path = Path(args.out) if args.out else None
        return asyncio.run(
            run_record(
                args.url,
                out=out_path,
                recordings_dir=None if out_path else config_module.recordings_dir(cfg.config_dir),
            )
        )

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
