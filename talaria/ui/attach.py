"""The two D6 input routes, reduced to one path (C9).

``/attach <path>`` and a dropped path both end here as a filesystem path;
from a path down the contract is identical, which is what makes this module
route-independent the way :mod:`talaria.transport.attachments` is. It holds
only the decisions that need no socket and no screen: is this paste a drop,
file route or image route, and how does the ``@file:`` chip enter and leave
the composer string. Staging itself stays in
:mod:`talaria.transport.attachments`, the ledger in
:mod:`talaria.domain.attachments`, and the dialog copy in
:mod:`talaria.ui.dialog` — this module never sends, stores, or renders.

**A paste that names an existing file diverts to attach.** Terminals deliver
a dropped path as pasted text, indistinguishably from typed text at the
framework level, so certainty is unavailable and the rule is conservative:
:func:`detect_dropped_path` answers only when the whole paste body is one
line naming a file that exists. A pasted sentence that happens to equal a
filename still diverts — the confirm dialog ahead of every stage is the
safety net, and cancelling it stages nothing. A body naming nothing stays
text for the composer to insert: a drop route cannot refuse what might be
prose. The ``/attach`` route is stricter — its argument explicitly names a
path, so :func:`stat_for_confirm` returning ``None`` is answered with a
missing/unreadable notice and no dialog opens.
"""

from __future__ import annotations

import uuid
from pathlib import Path
from typing import Final, Literal

from talaria.domain.attachments import AttachmentKind

__all__ = [
    "IMAGE_EXTENSIONS",
    "clean_path_text",
    "classify_route",
    "detect_dropped_path",
    "new_attachment_id",
    "place_ref_chip",
    "remove_ref_chip",
    "stat_for_confirm",
]

#: Image extensions the gateway accepts (I6 addendum on #147). Routing is by
#: extension alone: the gateway sniffs magic bytes and answers 4016 itself,
#: so a misnamed file still gets an honest verdict rather than a client
#: guess. Everything not listed here — including extensionless files —
#: travels ``file.attach``, where the turn inlines UTF-8 and stubs binary.
IMAGE_EXTENSIONS: Final[frozenset[str]] = frozenset(
    {
        ".png",
        ".jpg",
        ".jpeg",
        ".gif",
        ".webp",
        ".bmp",
        ".tiff",
        ".tif",
        ".svg",
        ".ico",
    }
)

#: What the confirm dialog may ask. ``confirm`` stages nothing yet — escape
#: cancels and no bytes move. ``remove`` unstages — escape keeps.
DialogMode = Literal["confirm", "remove"]


def classify_route(path: str | Path) -> AttachmentKind:
    """File route or image route, by extension alone (D6's accepted set).

    Portable-document format is deliberately *not* refused here: the refusal
    lives in :mod:`talaria.transport.attachments`, decided on extension
    *and* magic bytes after the read, so a renamed PDF cannot slip through
    as text. Routing a ``.pdf`` to the file path is what delivers it to
    that refusal.
    """
    if Path(path).suffix.lower() in IMAGE_EXTENSIONS:
        return "image"
    return "file"


def clean_path_text(text: str) -> str:
    """Strip one pasted or typed path to its bare form, without touching disk.

    Surrounding matched quotes (single, double, or backtick — what terminals
    wrap a dropped path in) are stripped, backslash escapes of spaces and
    quotes are undone (what terminals that do not quote do instead), and a
    leading ``~`` expands: every downstream reader (stat, transport, dialog)
    takes this output unsplit, so expanding here is what keeps ``~/notes``
    one path on every route instead of working on drops and failing on
    ``/attach``. Multi-line input cleans to itself: only single-line text
    can be a path, and the existence check belongs to the caller that knows
    which route it is answering.
    """
    candidate = text.strip()
    if not candidate or "\n" in candidate:
        return candidate
    if len(candidate) >= 2 and candidate[0] == candidate[-1] and candidate[0] in "'\"`":
        candidate = candidate[1:-1]
    else:
        candidate = (
            candidate.replace("\\ ", " ").replace("\\'", "'").replace('\\"', '"')
        )
    return str(Path(candidate.strip()).expanduser())


def detect_dropped_path(text: str) -> str | None:
    """Read a whole paste body as a dropped path, or ``None`` when it is not.

    Answers only when the body is exactly one non-empty line naming a file
    that exists — :func:`clean_path_text` plus the existence check. Anything
    else stays text for the composer to insert literally.
    """
    if not text:
        return None
    candidate = clean_path_text(text)
    if not candidate or "\n" in candidate:
        return None
    try:
        if not Path(candidate).expanduser().is_file():
            return None
    except (OSError, ValueError):
        return None
    return candidate


def stat_for_confirm(path: str | Path) -> tuple[AttachmentKind, int] | None:
    """Kind and byte size for the confirm dialog, or ``None`` unreadable.

    ``None`` means the route says so immediately with a missing/unreadable
    notice and no dialog opens: a confirm dialog for a file that cannot be
    staged would ask a question with no working answer.
    """
    local = Path(path)
    try:
        size = local.stat().st_size
        if not local.is_file():
            return None
    except (OSError, ValueError):
        return None
    return (classify_route(local), size)


def new_attachment_id() -> str:
    """Mint a ledger-unique attachment id. Randomness lives at the route
    layer, never in the domain: the ledger assigns no identities."""
    return uuid.uuid4().hex[:12]


def place_ref_chip(text: str, ref: str) -> str:
    """Append the staged ``@file:`` token to the composer string (C9).

    The client places the reference; the gateway does not inject it (I6
    addendum on #147). Space-joined, never newline-joined: the token must
    survive in running prose the operator keeps editing, and a newline
    would read as a paragraph break the operator did not write.
    """
    if not text:
        return ref
    if text[-1].isspace():
        return f"{text}{ref}"
    return f"{text} {ref}"


def remove_ref_chip(text: str, ref: str) -> str:
    """Take one staged ``@file:`` token back out of the composer string.

    Removes the first occurrence and tidies the joint it leaves: ``"a R b"``
    becomes ``"a b"``, edge tokens vanish without leaving a leading or
    trailing space, and text without the token is returned unchanged. The
    token match is exact — a prefix of a longer token is not a chip.
    """
    if ref not in text:
        return text
    start = text.find(ref)
    before = text[:start]
    after = text[start + len(ref):]
    # Glued to non-space on either side, the match is part of a longer
    # token rather than a chip, and removing it would corrupt that token.
    if before and not before[-1].isspace():
        return text
    if after and not after[0].isspace():
        return text
    if before.endswith(" ") and after.startswith(" "):
        return before + after[1:]
    return (before + after).strip()
