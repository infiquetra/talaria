"""talaria/cli.py — the ``talaria`` console-script entry point.

Implements KTD7's startup precedence: an explicit ``--session <id>`` beats
``--resume`` beats default-new, and the conflicting pair is a usage error
raised before any connection is dialed. Later units attach subcommands
(U2's recorder, U5's replay shell) to the parser built here rather than
building their own.
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from typing import Literal

StartupMode = Literal["session", "resume", "new"]


@dataclass(frozen=True)
class StartupSelection:
    """Exactly one startup path selects; no switcher exists afterward (KTD7)."""

    mode: StartupMode
    session_id: str | None = None


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
    return parser


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.session and args.resume:
        parser.error("--session and --resume are mutually exclusive")
    return args


def resolve_startup(args: argparse.Namespace) -> StartupSelection:
    """KTD7: explicit --session beats --resume beats default-new."""
    if args.session:
        return StartupSelection(mode="session", session_id=args.session)
    if args.resume:
        return StartupSelection(mode="resume")
    return StartupSelection(mode="new")


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    resolve_startup(args)
    # The replay shell (U5) and live transport (U7) are not built yet; U1
    # ships argument parsing, startup-path resolution, and the console
    # script only, so it has something for CI and `uv tool install` to run.
    return 0


if __name__ == "__main__":
    sys.exit(main())
