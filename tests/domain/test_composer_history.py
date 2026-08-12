"""C1 domain history: bounded list, draft stash, index, and pure transitions.

Every assertion is load-bearing: it names the entry, the index, and the stash,
so a wrong recall cannot hide behind a text-changed signal that would pass
whether the right entry or the wrong one was shown.
"""

from __future__ import annotations

from talaria.domain.composer_history import (
    MAX_HISTORY,
    ComposerHistory,
    abandon,
    move_down,
    move_up,
    push,
)


def test_push_stores_trimmed_and_ignores_empty_and_whitespace_only() -> None:
    s = ComposerHistory()
    s = push(s, "  hello  ")
    assert s.entries == ("hello",)
    assert s.draft_stash is None
    assert s.index is None
    # Empty after strip does not enter.
    before = s
    s = push(s, "   ")
    assert s is before
    s = push(s, "")
    assert s is before
    # Consecutive duplicates are kept.
    s = push(s, "hello")
    assert s.entries == ("hello", "hello")


def test_push_is_bounded_and_evicts_oldest() -> None:
    s = ComposerHistory()
    for i in range(MAX_HISTORY + 5):
        s = push(s, f"msg {i}")
    assert len(s.entries) == MAX_HISTORY
    assert s.entries[0] == "msg 5"
    assert s.entries[-1] == f"msg {MAX_HISTORY + 4}"
    # Push clears navigation.
    s = ComposerHistory(entries=("a", "b"), draft_stash="draft", index=1)
    s = push(s, "c")
    assert s.entries[-1] == "c"
    assert s.draft_stash is None
    assert s.index is None


def test_push_clears_stash_and_index() -> None:
    s = ComposerHistory(entries=("a",), draft_stash="half", index=0)
    s = push(s, "b")
    assert s.draft_stash is None
    assert s.index is None
    assert s.entries == ("a", "b")


def test_move_up_and_down_basic_ordering() -> None:
    s = ComposerHistory(entries=("a", "b", "c"))
    # Sentinel -> c
    s1, t = move_up(s, "", True)
    assert t == "c"
    assert s1.index == 2
    assert s1.draft_stash == ""
    assert s1.entries == ("a", "b", "c")
    # c -> b
    s2, t = move_up(s1, t, True)
    assert t == "b"
    assert s2.index == 1
    assert s2.draft_stash == ""
    # b -> a
    s3, t = move_up(s2, t, True)
    assert t == "a"
    assert s3.index == 0
    # at oldest, further up does nothing (caret movement instead)
    s4, t = move_up(s3, t, True)
    assert t is None
    assert s4 is s3
    # down a -> b
    s5, t = move_down(s3, "a", True)
    assert t == "b"
    assert s5.index == 1
    # b -> c
    s6, t = move_down(s5, t, True)
    assert t == "c"
    assert s6.index == 2
    # past newest restores stash
    s7, t = move_down(s6, t, True)
    assert t == ""
    assert s7.index is None
    # stash kept after restore (so next up sees same sentinel value)
    assert s7.draft_stash == ""
    # sentinel down does nothing
    s8, t = move_down(s7, "", True)
    assert t is None
    assert s8 is s7


def test_move_up_stashes_only_once_and_down_restores_verbatim() -> None:
    s = ComposerHistory(entries=("a", "b"))
    s1, _ = move_up(s, "draft half", True)
    assert s1.draft_stash == "draft half"
    assert s1.index == 1
    # Move inside history does not overwrite stash.
    s2, _ = move_up(s1, "b edited without stash", True)
    assert s2.draft_stash == "draft half"
    # Editing recalled text and then moving does not snapshot.
    s3, _ = move_down(s2, "a edited", True)
    assert s3.draft_stash == "draft half"
    # Down past newest restores exactly and puts caret at sentinel.
    s4, t = move_down(  # noqa: E501
        s1, "b", True
    )  # s1 at "b" (index 1)
    # Simpler: from b (index 1) which is newest, down restores.
    s_at_b, _ = move_up(ComposerHistory(entries=("a", "b")), "draft half", True)
    # s_at_b is at "b" (newest)
    assert s_at_b.index == 1
    restored_state, restored = move_down(s_at_b, "b", True)
    assert restored == "draft half"
    assert restored_state.index is None
    assert restored_state.draft_stash == "draft half"


def test_caret_boundary_blocks_recall() -> None:
    s = ComposerHistory(entries=("a",))
    # Not at top -> no recall, state unchanged, text None signals caret move.
    s1, t = move_up(s, "text", False)
    assert t is None
    assert s1 is s
    s_at = ComposerHistory(entries=("a", "b"), draft_stash="draft", index=1)
    s2, t = move_down(s_at, "b", False)
    assert t is None
    assert s2 is s_at


def test_abandon_keeps_stash_and_clears_index() -> None:
    s = ComposerHistory(entries=("a", "b"), draft_stash="half", index=0)
    s2 = abandon(s)
    assert s2.index is None
    assert s2.draft_stash == "half"
    assert s2.entries == ("a", "b")
    # abandon at sentinel is no-op
    s3 = abandon(s2)
    assert s3 is s2
    # A tab-away-and-back scenario: up, then abandon via focus, then down should still restore?
    # Abandon keeps stash, but down from sentinel does nothing — the walk is over.
    # The AE3 tab variant asserts that stash survives focus moves *without* abandoning index.
    # So abandon is explicit, but focus moves in the UI preserve index (no abandon call).
    s4, _ = move_up(ComposerHistory(entries=("old",)), "draft", True)
    assert s4.index == 0
    assert s4.draft_stash == "draft"
    s5 = abandon(s4)
    assert s5.index is None
    assert s5.draft_stash == "draft"
    # After abandon, up again stashes current text anew.
    s6, t = move_up(s5, "now showing old", True)
    assert t == "old"
    assert s6.draft_stash == "now showing old"


def test_move_with_empty_history_does_nothing() -> None:
    s = ComposerHistory()
    s1, t = move_up(s, "draft", True)
    assert t is None
    assert s1 is s
    s2, t = move_down(s, "draft", True)
    assert t is None
    assert s2 is s


def test_history_is_in_memory_only_no_file_import() -> None:
    # ADR-0002: domain must not import textual, and this module must not  # noqa: E501
    # touch pathlib/open/json file paths.
    import pathlib

    import talaria.domain.composer_history as mod

    source = pathlib.Path(mod.__file__).read_text()
    assert "textual" not in source
    # No file persistence: no open, pathlib write, json dump to file.  # noqa: E501
    # (Standard library json is allowed elsewhere, but this module must not  # noqa: E501
    #  write operator text to disk.)
    # We assert no obvious file write pattern.
    assert "open(" not in source
    # allow pathlib import for test only, not in module  # noqa: E501
    assert "Path(" not in source or "pathlib" not in source
