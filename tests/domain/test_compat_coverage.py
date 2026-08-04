"""Every gateway method Talaria can call is pinned in the baseline (R34).

R34 says Talaria "verifies every gateway method required by R1 through R31
against a pinned compatibility baseline", and that a missing required method is
named and blocks the daily-driver verdict. The baseline in
``talaria/domain/compat.py`` is the list that promise is kept with — so a method
Talaria dispatches but never listed there is invisible to the startup check, and
the verdict it blocks is one it was never asked about.

That is not hypothetical. ``slash.exec`` was dispatched by
``talaria/domain/commands.py`` — the *primary* route for an ordinary catalogue
command, with ``command.dispatch`` only the fallback — while the baseline pinned
the fallback alone. Startup would have reported ready against a gateway that had
renamed or dropped ``slash.exec``, and the first slash command an operator typed
would have failed. Nothing caught it: every existing test reasons over the
declared set, so a method absent from that set was absent from the tests too.

The gap closes only from the other direction. This module reads what the source
actually declares and requires the baseline to account for it, so the next method
added without a baseline entry fails here rather than in front of an operator.

The check is one-directional on purpose: everything declared must be pinned, but
the baseline may hold methods no constant names — the read-only probes are issued
by iterating ``COMPAT_BASELINE`` itself and so need no constant.

Both scans parse rather than import. Parsing cannot run whatever a module does at
import time, it reaches modules nothing here would otherwise import, and it keeps
the terminal framework out of a domain test: the bridge table below lives in
``talaria/ui/prompts.py``, which imports Textual at module scope.
"""

from __future__ import annotations

import ast
from pathlib import Path

from talaria.domain.compat import REQUIRED_METHODS, SUBAGENT_AUTHORING_METHODS

PACKAGE = Path(__file__).resolve().parents[2] / "talaria"

#: A constant whose value is the JSON-RPC method a call goes out as is named
#: ``*_METHOD``. That convention is what makes the scan possible, and a method
#: name bound to some other identifier is out of its reach — a limitation worth
#: stating plainly rather than papering over, which is why the falsifiability
#: control below pins the names the scan is known to reach.
SUFFIX = "_METHOD"

#: The five blocking bridges are answered through one table rather than through
#: ``*_METHOD`` constants, deliberately, so a bridge cannot be added to one
#: dictionary and forgotten in another (``talaria/ui/prompts.py:96-114``). The
#: scan has to know that shape by name; nothing about it is inferable.
BRIDGE_TABLE = ("talaria/ui/prompts.py", "_BRIDGES")


def _module_body(relative: str) -> list[ast.stmt]:
    path = PACKAGE.parent / relative
    return ast.parse(path.read_text(encoding="utf-8"), filename=str(path)).body


def _assigned_names(node: ast.stmt) -> tuple[list[ast.expr], ast.expr | None]:
    if isinstance(node, ast.AnnAssign):
        return [node.target], node.value
    if isinstance(node, ast.Assign):
        return list(node.targets), node.value
    return [], None


def declared_methods() -> dict[str, str]:
    """Every ``*_METHOD = "…"`` bound at module level anywhere under ``talaria/``."""
    found: dict[str, str] = {}
    for path in sorted(PACKAGE.rglob("*.py")):
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in tree.body:
            targets, value = _assigned_names(node)
            if not isinstance(value, ast.Constant) or not isinstance(value.value, str):
                continue
            for target in targets:
                if isinstance(target, ast.Name) and target.id.endswith(SUFFIX):
                    found[f"{path.relative_to(PACKAGE.parent)}::{target.id}"] = value.value
    return found


def bridge_methods() -> list[str]:
    """The method each blocking bridge is answered with, read out of the table.

    Every row is ``(kind, method, field)``, so the method is the second element;
    a row shaped otherwise is skipped rather than guessed at, and the control
    below is what notices if that ever silently empties the result.
    """
    relative, name = BRIDGE_TABLE
    methods: list[str] = []
    for node in _module_body(relative):
        targets, value = _assigned_names(node)
        if not any(isinstance(t, ast.Name) and t.id == name for t in targets):
            continue
        if not isinstance(value, ast.Tuple):
            continue
        for row in value.elts:
            if not isinstance(row, ast.Tuple) or len(row.elts) < 2:
                continue
            method = row.elts[1]
            if isinstance(method, ast.Constant) and isinstance(method.value, str):
                methods.append(method.value)
    return methods


def test_every_declared_gateway_method_is_pinned_in_the_baseline() -> None:
    """The guard the ``slash.exec`` gap needed.

    A method reachable from Talaria that the baseline does not name is either an
    unpinned dependency — the defect — or a method R17 forbids calling at all, in
    which case declaring a constant for it is its own problem. Both deserve to
    fail here, so the message names the two readings instead of guessing.
    """
    unpinned = {
        where: method
        for where, method in declared_methods().items()
        if method not in REQUIRED_METHODS
    }
    assert not unpinned, (
        f"declared but not in the compatibility baseline: {unpinned}. "
        f"Either add a MethodBaseline entry for it in talaria/domain/compat.py, "
        f"or — if it is one of R17's authoring methods {sorted(SUBAGENT_AUTHORING_METHODS)} — "
        f"Talaria should not be naming it at all."
    )


def test_every_bridge_is_answered_with_a_pinned_method() -> None:
    unpinned = sorted(set(bridge_methods()) - REQUIRED_METHODS)
    assert not unpinned, f"bridge methods missing from the baseline: {unpinned}"


def test_the_scans_find_what_they_claim_to_find() -> None:
    """Falsifiability control for both scans.

    A scan that silently matched nothing would satisfy the two assertions above
    forever, and that failure would look exactly like success. Pin what each is
    known to reach, so a rename that breaks the ``*_METHOD`` convention or the
    bridge table's shape fails loudly here instead of quietly disarming the guard.
    """
    reached = set(declared_methods().values())
    for expected in (
        "slash.exec",
        "command.dispatch",
        "prompt.submit",
        "session.create",
        "session.resume",
        "session.interrupt",
        "session.most_recent",
        "commands.catalog",
        "paste.collapse",
        "subagent.interrupt",
    ):
        assert expected in reached, f"the constant scan no longer reaches {expected}"

    assert sorted(bridge_methods()) == [
        "approval.respond",
        "clarify.respond",
        "secret.respond",
        "sudo.respond",
        "terminal.read.respond",
    ], "the bridge table's shape changed and the scan stopped reading it"
