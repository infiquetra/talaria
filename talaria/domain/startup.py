"""KTD7's startup precedence as a pure function (R2, PC5, AE12).

The precedence is one line of policy — explicit ``--session`` beats ``--resume``
beats default-new — and it lives here rather than in the argument parser for two
reasons. It has to be decidable before any connection is dialed, so it cannot
depend on a transport; and U7 resolves the same precedence from a stored config
value rather than from ``argv``, so a function that only understands
``argparse.Namespace`` would have to be re-derived there.

"Exactly one path selects; no switcher exists afterward" (R2) is a property of
the *return type*: :class:`StartupSelection` names one mode, and nothing in the
domain offers a way to change it later.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

StartupMode = Literal["session", "resume", "new"]


class StartupConflictError(ValueError):
    """Both an explicit session and a resume request were given.

    A usage error rather than a silent precedence win: the two flags express
    different intents, and picking one for the operator would hide the mistake
    until they noticed they were in the wrong conversation.
    """


@dataclass(frozen=True)
class StartupSelection:
    """The one selected startup path."""

    mode: StartupMode
    session_id: str | None = None


def resolve_startup(*, session: str | None = None, resume: bool = False) -> StartupSelection:
    """Resolve the startup path, or raise on the conflicting pair.

    ``--resume`` resolves through ``session.most_recent``, which is registered at
    the pin (``tui_gateway/methods_session.py:214``) and already drops sub-agent
    and dispatcher rows, so "most recent" means the most recent conversation a
    human had rather than the most recent row written.
    """
    target = session.strip() if isinstance(session, str) else None
    if target and resume:
        raise StartupConflictError("--session and --resume are mutually exclusive")
    if target:
        return StartupSelection(mode="session", session_id=target)
    if resume:
        return StartupSelection(mode="resume")
    return StartupSelection(mode="new")
