"""Session working-directory status (C13, issue #157, window one).

When Talaria creates a session it sends the launch directory as the requested
``cwd``; the gateway reports back the directory the session actually holds.
Those two values can disagree — a request naming a directory the gateway
cannot use is silently replaced by the gateway's own default — and the
complaint behind this work (feedback item 12) was Talaria displaying one
directory while the agent answered from another. This module is the vocabulary
that keeps that divergence from reappearing in a new place: one status type
that always carries both what was requested and what was reported, derived
from those values alone, never from the absence of an error.

Pure and filesystem-free (ADR-0002): normalisation is textual, comparison is
on normalised text, and nothing here touches the disk, the process, or the
gateway. There is deliberately no permission vocabulary in this module. The
gateway exposes no directory permission model on any route, so the only
truthful assertion is that no permission change was made or claimed, and no
helper here may imply otherwise.
"""

from __future__ import annotations

import posixpath
from typing import Literal

#: What the interface may honestly claim about a session's working directory.
#: ``requested`` is what Talaria sent at creation, or None on resume, where
#: nothing is sent. ``first`` and ``latest`` are the first and most recent
#: directories any reply or event named, each None where nothing has yet.
DirectoryStatus = Literal["unreported", "reported", "adopted", "not-adopted", "moved"]

__all__ = ["DirectoryStatus", "directory_status", "normalize_cwd"]


def normalize_cwd(value: str) -> str:
    """Reduce a directory to its comparable textual form.

    A trailing separator is dropped (root stays root), ``.`` and ``..``
    segments resolve lexically, and case is untouched, so ``/A`` never equals
    ``/a``. :mod:`posixpath` — never :mod:`os.path` — so the result is
    identical on every platform without touching the filesystem.
    """
    return posixpath.normpath(value)


def directory_status(
    requested: str | None,
    first_reported: str | None,
    latest_reported: str | None,
) -> DirectoryStatus:
    """Derive the directory status from the request and the reports so far.

    A caller holding only one report passes it as both ``first_reported`` and
    ``latest_reported``; a None ``first_reported`` beside a set ``latest`` is
    read the same way. Both sides compare after :func:`normalize_cwd`, so a
    trailing separator or a ``.`` segment can never manufacture a mismatch.

    The table in issue #157 is the authority, applied literally:

    * no report yet names a directory → ``unreported``;
    * reported with nothing requested (a resumed session) → ``reported``,
      whatever later reports arrive, since resume never adopts;
    * the first report differs from the request → ``not-adopted``, even if a
      later report lands on the requested directory: the gateway never adopted
      the request, and a later coincidence is not an adoption;
    * adopted first, differing later (the agent changed its own directory) →
      ``moved``;
    * otherwise the reported directory is the requested one → ``adopted``.

    A move away and back reads ``adopted``: only ``first`` and ``latest`` are
    visible here, both agree with the request, and the interface rows agree
    with each other. The move itself is history, and history is the
    transcript's job in window two, not the status's.
    """
    latest = latest_reported if latest_reported is not None else first_reported
    if latest is None:
        return "unreported"
    if requested is None:
        return "reported"
    first = first_reported if first_reported is not None else latest
    if normalize_cwd(first) != normalize_cwd(requested):
        return "not-adopted"
    if normalize_cwd(latest) != normalize_cwd(requested):
        return "moved"
    return "adopted"
