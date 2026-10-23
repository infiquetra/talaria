"""Generate the hermes-contract crate from the vendored Hermes OpenRPC document.

Red-run skeleton: every subcommand and model function raises NotImplementedError.
Python 3.9 or later, standard library only.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Iterable, Optional, Sequence

TYPIFY_VERSION = "0.8.0"

# The five events apps/shared/src/gateway-events.ts adds and the contract omits.
HAND_WRITTEN_EVENTS = (
    "approval.request",
    "approval.settled",
    "clarify.request",
    "clarify.settled",
    "session.replay_gap",
)

# Hard-coded from tui_gateway/ws_legacy_fallback.py at the pinned tag.
# drift does not re-read these sets from a Hermes checkout.
_PROCESS_GLOBAL = (
    "process.stop",
    "delegation.pause",
    "reload.mcp",
    "reload.env",
    "agents.list",
)
_SESSION_NAMESPACES = ("session.", "prompt.", "message.")
_SESSION_FALLBACK_ALLOWED = (
    "session.most_recent",
    "session.active_list",
    "session.events.stats",
    "session.foreign.list",
    "session.foreign.preview",
    "session.foreign.import",
)

# Kept as one object so the fallback sets stay in this module for the generator.
FALLBACK_RULE = {
    "process_global": _PROCESS_GLOBAL,
    "session_namespaces": _SESSION_NAMESPACES,
    "session_fallback_allowed": _SESSION_FALLBACK_ALLOWED,
}


class PinError(Exception):
    """PINNED is missing, malformed, or older than the minimum release."""


class ContractError(Exception):
    """A generator rule refused the contract. The message is one plain line."""


class CheckoutError(Exception):
    """A Hermes checkout HEAD is not the pinned commit."""


class Pin(object):
    """The two fields of crates/hermes-contract/PINNED."""

    def __init__(self, tag: str, commit: str) -> None:
        self.tag = tag
        self.commit = commit


def workspace_root() -> Path:
    """Repository root. This file lives at tools/contract-gen/contract_gen.py."""
    return Path(__file__).resolve().parents[2]


def parse_pinned(text: str) -> Pin:
    raise NotImplementedError


def validate_pinned(pin: Pin) -> None:
    raise NotImplementedError


def compare_versions(left: str, right: str) -> int:
    """Return negative, zero, or positive when left is older, equal, or newer."""
    raise NotImplementedError


def lift_schemas(contract: dict) -> dict:
    raise NotImplementedError


def preprocess_schemas(contract: dict) -> dict:
    raise NotImplementedError


def rejected_keyword_sites():
    """Return (schema, property, keyword) rows that typify 0.8.0 rejects."""
    raise NotImplementedError


def root_anyof_document(defs_document: dict) -> dict:
    raise NotImplementedError


def known_union_sites():
    """Return sorted Schema.property names for discriminator sites."""
    raise NotImplementedError


def known_received_constrained_types():
    """Return sorted schema names that constrain a string on received data."""
    raise NotImplementedError


def validate_contract(contract: dict) -> None:
    raise NotImplementedError


def rust_module_name(wire_name: str) -> str:
    raise NotImplementedError


def rust_variant_name(wire_name: str) -> str:
    raise NotImplementedError


def refuse_identifier_collisions(wire_names: Sequence[str]) -> None:
    """Raise ContractError naming every wire name that shares one Rust identifier."""
    raise NotImplementedError


def method_catalogue(contract: dict) -> dict:
    raise NotImplementedError


def notification_model(contract: dict) -> dict:
    raise NotImplementedError


def is_native(transport: str, name: str, owner_methods: Iterable[str]) -> bool:
    raise NotImplementedError


def is_native_legacy(name: str, owner_methods: Iterable[str]) -> bool:
    raise NotImplementedError


def non_native_legacy_names(contract: dict):
    """Legacy-only names the native socket does not serve. Reads the owner file."""
    raise NotImplementedError


def owner_methods_from_checkout(checkout: Path):
    raise NotImplementedError


def require_pinned_checkout(checkout: Path, commit: str) -> None:
    """No-op when checkout has no .git directory. Otherwise HEAD must equal commit."""
    raise NotImplementedError


def catalogue_diff(before: dict, after: dict, owners_before, owners_after) -> dict:
    raise NotImplementedError


def copy_workspace_for_drift(source: Path, dest: Path) -> None:
    raise NotImplementedError


def cmd_generate(args: argparse.Namespace) -> None:
    raise NotImplementedError


def cmd_check(args: argparse.Namespace) -> None:
    raise NotImplementedError


def cmd_verify_vendor(args: argparse.Namespace) -> None:
    raise NotImplementedError


def cmd_drift(args: argparse.Namespace) -> None:
    raise NotImplementedError


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="contract_gen.py")
    sub = parser.add_subparsers(dest="command")
    sub.required = True

    generate = sub.add_parser("generate")
    generate.add_argument("--contract")
    generate.add_argument("--out")
    generate.add_argument("--hermes-checkout")

    check = sub.add_parser("check")
    check.add_argument("--require-typify", action="store_true")

    verify = sub.add_parser("verify-vendor")
    verify.add_argument("--contract")

    drift = sub.add_parser("drift")
    drift.add_argument("--tag", required=True)
    drift.add_argument("--summary")
    drift.add_argument("--hermes-checkout")
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(list(argv) if argv is not None else None)
    if args.command == "generate":
        cmd_generate(args)
    elif args.command == "check":
        cmd_check(args)
    elif args.command == "verify-vendor":
        cmd_verify_vendor(args)
    elif args.command == "drift":
        cmd_drift(args)
    else:
        parser.print_usage(sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
