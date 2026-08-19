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
from collections.abc import Mapping
from pathlib import Path
from typing import TYPE_CHECKING, Any

from talaria import __version__
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
    # Reports the literal that hatchling also builds the distribution's
    # metadata from, so the answer to "what am I running" is the same string
    # a bug report can be tied back to a build with.
    parser.add_argument(
        "--version",
        action="version",
        version=f"%(prog)s {__version__}",
        help="print the version and exit",
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
    refresh_parser.add_argument(
        "--profile",
        metavar="NAME",
        default="",
        help="pair a named profile: write [profiles.NAME] in the credential file "
        "instead of the top-level token, deriving the dashboard from that "
        "profile's [profiles.endpoints] entry in config.toml (v0.4 KTD5)",
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

    **The route list has shrunk twice, and must keep naming only what works.**
    It once advertised ``HERMES_DASHBOARD_SESSION_TOKEN`` as one of "two routes";
    KTD8 removed that variable from the precedence chain on 2026-08-06. It then
    advertised a ``token`` query parameter on ``TALARIA_GATEWAY_URL`` as one of
    "three routes"; that route was removed on 2026-08-07 and the same variable is
    now *refused* for carrying one. Naming a dead route here would send an
    operator who has just exposed a credential somewhere that silently does
    nothing — the worst possible moment to be wrong about what works.
    """
    from talaria.transport.credentials import GATEWAY_URL_ENV_VAR

    return "\n".join(
        (
            "talaria: refusing to record: that endpoint carries a credential.",
            "",
            "  A credential on a command line is readable by anyone who can run `ps` while the",
            "  command runs, and your shell has already written it to history. Treat the value",
            "  you just passed as exposed, and rotate it now.",
            "",
            "  Two routes supply a credential, and neither puts it on a command line or in an",
            "  environment variable:",
            "    talaria refresh-credential"
            "   — rewrites the credential file at mode 0600, printing nothing secret",
            "    the interactive prompt"
            "   — asked once before recording starts, with terminal echo off",
            "",
            "  Then record with no argument, or with an endpoint that carries no credential:",
            "    talaria record",
            "    talaria record ws://<host>:<port>/api/ws",
            "",
            f"  {GATEWAY_URL_ENV_VAR} names the endpoint only. An exported one carrying a",
            "  credential is refused for the same reason this argument is.",
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
    behaviour KTD1 rejects. By the time ``argparse`` has seen the argument the
    value is already in the process table and the shell history, so stripping it
    quietly would preserve the exact habit that leaked it.
    """
    # Imported here, not at module top level: `talaria.recorder.command` pulls in
    # `websockets`, and the default (no-subcommand) launch path has no business
    # paying for that import.
    from talaria.recorder.command import resolve_record_target, run_record
    from talaria.transport.attach import url_carries_credential
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
        target = asyncio.run(resolve_record_target(credentials_path=credentials, override=args.url))
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


class _HomeDispatcher:
    """Dispatch to whichever connection is currently home (U7, KTD1).

    ``TalariaApp`` takes one dispatcher at construction, while a fleet's home
    moves: ``/profiles`` ensures a connection and makes it the home for the next
    ``session.create``/``session.resume``. Holding a source captured at build
    time would send every later call to the connection that happened to be first
    in the inventory. Resolving on each call is what makes "the next session
    lands on the profile you just selected" true.

    Satisfies :class:`~talaria.ui.app.LiveDispatcher`, which is one method wide.
    """

    def __init__(self, connections: Any) -> None:
        self._connections = connections

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> Any:
        source = self._connections.source_for(self._connections.home)
        if source is None:
            # The home connection is not up. Answered with the transport's own
            # not-connected outcome rather than an exception, and deliberately the
            # SAME one ``LiveSource.call`` returns in that state: a caller must not
            # be able to tell "the home connection has not answered yet" from "this
            # connection has not answered yet", because the honest report is
            # identical and every screen that renders one already renders the other.
            from talaria.transport.rpc import NOT_CONNECTED, unknown_outcome

            return unknown_outcome(method, NOT_CONNECTED, epoch=0)
        return await source.call(method, params, timeout=timeout)


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
    from talaria.transport.admin import AdminClient, AdminError
    from talaria.transport.attach import AttachTarget
    from talaria.transport.connection_set import (
        ConnectionSet,
        build_source_factory,
        credential_provider_factory,
        plan_connections,
    recorded_connections,
        resolve_connections,
    )
    from talaria.transport.credentials import LoopbackTokenProvider
    from talaria.transport.source import LiveSource
    from talaria.ui.app import TalariaApp

    credentials = config_module.credentials_path(cfg.config_dir)
    target = AttachTarget.from_environment(credentials_path=credentials)
    credential_provider = LoopbackTokenProvider(credentials_path=credentials)

    # U2's admin HTTP surface (KTD1). Origin derivation can refuse an endpoint
    # ``LiveSource`` would happily dial over WebSocket (``refused_origin``,
    # e.g. a scheme neither surface actually reaches) — that failure belongs to
    # the picker alone and must not take the whole launch down with it, so it
    # is caught here and the picker instead renders "unavailable" (R7).
    def admin_for(endpoint: str) -> AdminClient | None:
        try:
            return AdminClient(endpoint, credential_for(endpoint))
        except AdminError:
            return None

    def credential_for(endpoint: str) -> LoopbackTokenProvider:
        """KTD6: a fresh provider bound to the endpoint about to be dialled.

        Fresh, and never the one already in hand. Each profile's dashboard
        mints its own token, and a provider that has cached a prompt-typed
        value would carry the previous gateway's credential to the next one —
        where the refusal would read as an authentication problem rather than
        as "that credential was never for this gateway".

        ``allow_prompt=False`` for the same reason
        :func:`_prime_credential` seals the prompt before the interface starts:
        a switch happens inside a running Textual application that owns the
        screen, so a hidden prompt issued from here would be invisible and
        would present as a hung switch. A profile whose credential is not in
        the file therefore surfaces as ``credential_unavailable`` with the
        reason on screen, which is the named state U4 requires.

        ``endpoint`` is unused today and named anyway: the moment the
        credential file grows a per-endpoint form (explicitly out of scope,
        Scope Boundaries), this is the one signature that has to change, and a
        parameter that is already there makes that a body edit instead of a
        call-site hunt.
        """
        del endpoint
        return LoopbackTokenProvider(credentials_path=credentials, allow_prompt=False)

    admin_client: AdminClient | None = admin_for(target.url)

    # **No recorder, and the removal is the point.** This source is never
    # dialled — it survives only so :func:`_prime_credential` can read its
    # provider (see the note where it is returned) — so the recorder it used to
    # be handed could never receive a frame from it. Passing one made the
    # recording look wired here while every recorded frame actually arrived
    # through the connection set's per-connection views, which is where U8 found
    # the untagged-log defect hiding.
    source = LiveSource(
        target,
        credential_provider,
        credential_factory=credential_for,
    )

    # ── U7's composition root ────────────────────────────────────────────
    #
    # U2 built the whole assembly kit — ``plan_connections``, ``resolve_connections``,
    # ``build_source_factory``, ``ConnectionSet`` — and nothing ever called it, so
    # ``TalariaApp.connections`` was permanently ``None``, every multi-connection
    # branch was dead, and U2's goal sentence ("Talaria dials every configured
    # profile endpoint concurrently") was not true of the running program. This is
    # the call. Pinned by
    # ``test_the_live_app_is_assembled_on_a_connection_set``, which fails if this
    # entry point ever reverts to handing the app a lone ``LiveSource``.
    provider_for = credential_provider_factory(credentials)
    members = plan_connections(
        default_endpoint=target.url,
        config_endpoints=config_module.profile_endpoints(cfg),
    )
    # Synchronous launcher, asynchronous resolution — the same shape
    # :func:`_prime_credential` already uses, and for the same reason: reading a
    # credential is async because providers are, while nothing has started a loop
    # yet. Resolution dials nothing; it only decides which configured profiles
    # share one gateway.
    #
    # THE CONSTRAINT THIS IMPOSES, stated because a caller only discovers it by
    # crashing: ``build_live_app`` must be called from a SYNCHRONOUS context.
    # ``asyncio.run`` refuses to nest, so calling this from inside a running loop
    # raises. That is true of :func:`_prime_credential` already and of the whole
    # launch path by design — ``run_live`` assembles, primes, and only then hands
    # control to Textual — but a test or a future async caller has to build the
    # app before entering its loop, not during.
    entries = asyncio.run(resolve_connections(members, provider_for))

    # **Built here rather than beside the admin client above, and the position is
    # the whole of U8's first finding.** ``recorded_connections`` needs the
    # RESOLVED inventory, which does not exist until the line above — so a
    # recorder constructed earlier could only ever be told about one connection.
    # It then set ``_multi`` False, which makes ``view()`` skip its profile
    # validation and ``_tag()`` drop the profile key outright, so a live
    # two-gateway ``talaria --record`` run wrote a version-1, untagged log: the
    # one recording U8 exists to replay per-connection, and the one shape it
    # could not produce.
    #
    # The frames reach it through ``build_source_factory(recorder=recorder)``
    # below, which hands each connection its own ``recorder.view(profile)``. The
    # ``LiveSource`` built further down is never dialled and records nothing —
    # see the note where it is kept.
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
            connections=recorded_connections(entries),
        )


    # The app does not exist yet and the set needs to reach it, which is the same
    # circularity ``LiveSource.bind`` solves one line further down. A holder keeps
    # the indirection visible rather than hiding it behind a mutable attribute.
    holder: list[TalariaApp] = []

    def on_fleet_connection(
        profile: str,
        state: Any,
        detail: str,
        cause: Any,
    ) -> None:
        if holder:
            holder[0].note_connection_state(state, detail, cause, profile=profile)

    connections = ConnectionSet(
        entries,
        build_source_factory(provider_for=provider_for, recorder=recorder),
        on_connection=on_fleet_connection,
        recorder=recorder,
    )

    app = TalariaApp(
        # The set IS the frame source: it yields ``TaggedFrame``, so every frame
        # reaches the registry carrying the connection it crossed and that
        # connection's own epoch, which is what registry-rooted routing means.
        # Pinned by ``test_a_tagged_frame_routes_to_its_own_connections_row``,
        # which feeds one session id on two connections — the case that is
        # genuinely two sessions and would be one if the tag were ignored.
        connections,
        mode="live",
        dispatcher=_HomeDispatcher(connections),
        admin_client=admin_client,
        admin_factory=admin_for,
        # ``None`` on purpose: ``switch_to_endpoint`` retargets one socket and
        # therefore drops whatever it was connected to, which a fleet client must
        # never do — the connection it dropped is the only feed for that gateway's
        # sessions. With a ``ConnectionEnsurer`` present the app takes the
        # ensure-beside path instead and never reads this.
        switcher=None,
        connections=connections,
        # CR7 finding 4. Without this the app does not know which connection it
        # is on, while the set beside it does — `app.current_profile` came up
        # empty in a live run against `connections.home == "default"`. Two shipped
        # behaviours were dead as a result, neither of them noisily: `/profiles`
        # marked no row and opened at row one, contradicting `Selection.opened`'s
        # own promise to open on the current row; and `/models <n> default` was
        # refused on EVERY fresh session, because `set_model_default` returns
        # early on `if not self.current_profile` and its notice blamed a session
        # "started without one named" — which was every session.
        #
        # `home` rather than a constant: it is the profile this app is focused on
        # and the one `_adopt_profile` will overwrite when `/profiles` moves it.
        current_profile=connections.home,
        profile_endpoints=config_module.profile_endpoints(cfg),
        status_runner=_build_status_runner(cfg),
        status_interval=float(cfg.get("status", "interval_seconds", default=5) or 5),
        paste_threshold=_build_paste_threshold(cfg),
        startup=selection_from_args(args),
    )
    holder.append(app)
    # ``source`` is no longer bound, and the removal is the point rather than an
    # oversight. It used to carry the app's connection callbacks, because it was
    # the app's stream; the set is now, and the set builds its own sources
    # through ``build_source_factory`` and observes each one itself, handing the
    # profile through to ``note_connection_state`` above. KTD7's typed terminal
    # cause still travels the same end-to-end path — every source sets a cause on
    # its four disconnect sites, and the set's observer forwards it unchanged —
    # it simply arrives with the name of the connection it belongs to.
    #
    # ``source`` survives for exactly one job: :func:`_prime_credential` reads its
    # provider so the one credential level that needs a human runs while a human
    # can still see the terminal. It is never dialled. The fleet's own providers
    # never prompt, by construction (``credential_provider_factory``), because a
    # fleet launch dials several gateways at once and an interactive prompt would
    # be issued once per unpaired profile underneath an interface that cannot
    # display any of them.
    return app, source


def run_live(args: argparse.Namespace) -> int:
    """Launch the live shell against a running Hermes gateway (R2, R31).

    **This path has been run against a real Hermes gateway**, first on
    2026-08-04. Live startup acceptance (R2) and a live turn compared against
    replay (R3) are both graded *measured* in
    ``docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`` (evidence-table rows
    17 and 18), cited by frame-log digest. Every automated *test* in this
    repository still dials a loopback stub, so the live evidence is a set of
    recordings rather than a suite, and that document's verdict is still *not
    ready* — on three other gaps now, not on this one.

    This docstring used to read: "**This path has never been run against a real
    Hermes gateway.** Every transport test in this repository dials a loopback
    stub. The unmet requirements that follow from that — live startup acceptance
    (R2) and a live turn (R3) — are recorded in [the verdict], and they are why
    that document's verdict is *not ready*." It was true when written and stopped
    being true on 2026-08-04.

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

    ``--profile <name>`` pairs a *named* profile (KTD5): it writes
    ``[profiles.<name>]`` rather than the top-level ``token``, and it derives
    the dashboard from that profile's ``[profiles.endpoints]`` entry in
    ``config.toml`` rather than from the credential file's own endpoint. Those
    two halves belong together — a profile's token is minted by that profile's
    own dashboard, so deriving the origin from the *default* endpoint would
    write one gateway's credential under another gateway's name, which is the
    exact confusion KTD5's single-endpoint-source rule exists to prevent.
    """
    from talaria.transport.refresh import (
        RefreshError,
        dashboard_origin_for,
        refresh_credential,
    )

    cfg = config_module.load_config()
    credentials = config_module.credentials_path(cfg.config_dir)
    profile = str(getattr(args, "profile", "") or "")

    try:
        origin = args.dashboard
        if not origin:
            origin = dashboard_origin_for(_endpoint_to_pair(cfg, credentials, profile))
        report = refresh_credential(
            origin, credentials, timeout=args.timeout, profile=profile
        )
    except RefreshError as exc:
        print(f"talaria: {exc}", file=sys.stderr)
        return 2

    action = "created" if report.created else "updated"
    entry = f"[profiles.{report.profile}]" if report.profile else "the default profile"
    print(f"{action} {entry} in {report.path} (mode 0600) from {report.origin}")
    if report.tightened:
        print("  permissions tightened to 0600; the previous file was readable by others")
    if report.preserved_keys:
        print(f"  kept: {', '.join(report.preserved_keys)}")
    return 0


def _endpoint_to_pair(
    cfg: config_module.Config, credentials: Path, profile: str
) -> str:
    """Which gateway's dashboard mints the credential being refreshed (KTD5).

    For the default profile that is the endpoint a bare ``talaria`` launch
    dials. For a named profile it is that profile's ``[profiles.endpoints]``
    entry in ``config.toml`` and nothing else — the credential file is not an
    endpoint source for a named profile, so a profile with no configured
    endpoint is refused by name rather than silently paired against whichever
    gateway happens to be the default.
    """
    from talaria.transport.attach import AttachTarget
    from talaria.transport.credentials import validate_profile_name
    from talaria.transport.refresh import RefreshError

    if not profile:
        target = AttachTarget.from_environment(credentials_path=credentials)
        if target.problem:
            raise RefreshError(target.problem)
        return target.url

    from talaria.transport.credentials import CredentialError

    try:
        validate_profile_name(profile)
    except CredentialError as exc:
        raise RefreshError(str(exc)) from exc

    endpoint = config_module.profile_endpoints(cfg).get(profile)
    if not endpoint:
        raise RefreshError(
            f"no endpoint is configured for profile {profile!r}: add it under "
            "[profiles.endpoints] in config.toml, or pass --from with that "
            "profile's dashboard http URL"
        )
    target = AttachTarget.from_url(endpoint)
    if target.problem:
        raise RefreshError(target.problem)
    return target.url


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
