"""In-memory composer history — what was sent, not what was typed (C1, KTD1-KTD5).

The composer holds what the operator is still editing. History holds what they
pressed ``enter`` on and Talaria actually dispatched — plain messages and slash
commands alike, trimmed the way the gateway saw them, bounded at one hundred
entries and never written to disk. An abandoned draft, a notice-bar message and
the placeholder never enter. A message whose reply was ``unknown`` does, because
it was still sent and may be exactly the thing the operator wants to retry.

The shape is one frozen value with a cursor:

* ``entries`` — the bounded list, oldest first, newest last.
* ``draft_stash`` — the half-written line the operator was composing when they
  left the sentinel to browse.
* ``index`` — ``None`` means the sentinel (the live draft), otherwise the
  index into ``entries`` currently shown.

Only pure functions mutate it. The terminal framework never owns it — the
boundary check in :mod:`talaria.domain.state` proves the domain imports no
third-party package, so this module imports only the standard library and its
own package. The widget computes the caret boundary and passes the boolean in;
the domain never reads ``cursor_location`` itself.

Stash discipline (KTD3): the stash is taken once, on the first ``up`` from the
sentinel, and thereafter is read-only until the sentinel is re-reached or a
submission clears it. Moves inside history do not overwrite it, and editing a
recalled entry does not snapshot that edit on the next move. Pressing ``down``
past the newest entry restores the stash and returns to the sentinel. Abandoning
the walk without submitting (escape, focus loss) keeps the stash for the next
walk.
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from typing import Final

MAX_HISTORY: Final[int] = 100


@dataclass(frozen=True)
class ComposerHistory:
    """Bounded, in-memory history plus a draft stash and a cursor.

    ``index`` is ``None`` on the sentinel (the live draft). ``draft_stash`` is
    ``None`` until the first excursion from the sentinel, and is cleared only by
    :func:`push` or by an explicit restore via :func:`move_down`.
    """

    entries: tuple[str, ...] = ()
    draft_stash: str | None = None
    index: int | None = None


def push(state: ComposerHistory, text: str) -> ComposerHistory:
    """Append a submitted line and reset navigation.

    ``text`` is trimmed before it is considered. An empty string after
    stripping never enters history and leaves the state unchanged. Consecutive
    duplicates are kept — one history entry per submit, matching the transcript
    which always appends.
    """
    stripped = text.strip()
    if not stripped:
        return state
    # Append and bound.
    new_entries = (*state.entries, stripped)
    if len(new_entries) > MAX_HISTORY:
        new_entries = new_entries[-MAX_HISTORY:]
    return ComposerHistory(entries=tuple(new_entries), draft_stash=None, index=None)


def abandon(state: ComposerHistory) -> ComposerHistory:
    """Leave history navigation but keep the stash.

    ``index`` returns to the sentinel; the stash, if any, is preserved so a
    tab-away-and-back does not destroy a half-written message.
    """
    if state.index is None:
        return state
    return replace(state, index=None)


def move_up(
    state: ComposerHistory, current_text: str, caret_at_top: bool
) -> tuple[ComposerHistory, str | None]:
    """Step one entry back in history, if the caret and the list allow it.

    Returns the new state and the text to show, or ``None`` when the key
    belongs to caret movement or there is nothing to recall. The caller owns
    caret geometry — this function only consumes the precomputed boolean.
    """
    if not caret_at_top:
        return state, None
    if not state.entries:
        return state, None
    if state.index is None:
        # Leaving the sentinel: stash the live draft once.
        new_index = len(state.entries) - 1
        new_state = replace(state, draft_stash=current_text, index=new_index)
        return new_state, state.entries[new_index]
    if state.index == 0:
        # Already at the oldest entry — stay there. Returning ``None`` would
        # delegate to the widget's caret movement, which for a single-line
        # composer does nothing and for a multi-line one at the top row also
        # does nothing, so the observable effect is the same as staying.
        # Keeping the existing state avoids rewriting the same text; the caller
        # will delegate to ``super()._on_key`` which is a no-op at the top.
        return state, None
    new_index = state.index - 1
    new_state = replace(state, index=new_index)
    return new_state, state.entries[new_index]


def move_down(
    state: ComposerHistory, current_text: str, caret_at_bottom: bool
) -> tuple[ComposerHistory, str | None]:
    """Step one entry forward, or restore the stashed draft past the newest.

    The ``current_text`` argument is accepted for symmetry with :func:`move_up`
    and for the case where the caller wants to pass the live text through;
    it is not written back while navigating — only the sentinel restore uses the
    stash.
    """
    _ = current_text  # Not used while stepping inside history; kept for symmetry.
    if not caret_at_bottom:
        return state, None
    if state.index is None:
        return state, None
    if state.index == len(state.entries) - 1:
        # Past the newest — restore the stash and return to the sentinel.
        restored = state.draft_stash if state.draft_stash is not None else ""
        # Keep the stash after restore so a subsequent ``up`` from the sentinel
        # sees a consistent value; ``push`` clears it. If the caller prefers to
        # clear after restore they may do so via ``replace`` — but keeping it
        # is what makes a bare ``up``/``down`` round-trip idempotent.
        new_state = replace(state, index=None)
        return new_state, restored
    new_index = state.index + 1
    new_state = replace(state, index=new_index)
    return new_state, state.entries[new_index]
