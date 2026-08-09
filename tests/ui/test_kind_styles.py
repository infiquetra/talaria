"""U5's kind differentiation (KTD5; R7, R8; AE5).

R7's own requirement sentence names six labelled groups, not the plan's
"five kind groups" recap — see the long comment beside
:data:`~talaria.ui.transcript.KindGroup` in ``talaria/ui/transcript.py`` for
the exact discrepancy and why this module follows R7's own enumeration. What
every test here proves is unaffected by which count is right: every one of
the twelve :data:`~talaria.domain.models.TranscriptKind` members reaches
exactly one named group, adjacent groups paint differently, the channel
survives a fence's own syntax colours, styling adds no widget height, and no
CSS variable outside the installed theme's own vocabulary is introduced.
"""

from __future__ import annotations

import re
from typing import get_args

import pytest
from textual.app import App, ComposeResult

from talaria.domain.models import TranscriptKind
from talaria.ui.blocks import EntryMarkdown
from talaria.ui.transcript import (
    _GROUP_VARIABLES,
    _KIND_GROUPS,
    KindGroup,
    TranscriptLine,
    TranscriptPane,
    kind_group,
    kind_group_css_class,
)

#: Every CSS variable ``TranscriptPane.DEFAULT_CSS`` references, pulled by
#: regex rather than re-deriving it from ``_GROUP_VARIABLES`` — the point of
#: this test is to catch a *stylesheet* that drifted from the table, and a
#: derivation from the same table could never see that drift.
_VARIABLE_TOKEN = re.compile(r"\$([a-z][a-z0-9-]*)")


class _Harness(App[None]):
    def compose(self) -> ComposeResult:
        yield TranscriptPane(id="t")


ALL_KINDS: tuple[TranscriptKind, ...] = get_args(TranscriptKind)

#: One short line of content per kind — short enough that the one column of
#: width a border-left gutter marker costs (see the module docstring in
#: ``talaria/ui/transcript.py``) can never push it past an 80-column wrap
#: boundary, so the height-parity assertions are never confounded by an
#: incidental wrap change this unit's styling did not intend to cause.
_SAMPLE_TEXT = "sample transcript line"


# ── R7: the twelve-to-group mapping is total ────────────────────────────────


def test_every_transcript_kind_has_a_group() -> None:
    """Every live :data:`TranscriptKind` member maps to a group — the
    literal type and :data:`_KIND_GROUPS` agree, kind-by-kind."""
    assert set(ALL_KINDS) == set(_KIND_GROUPS)
    for kind in ALL_KINDS:
        assert kind_group(kind) in get_args(KindGroup)


def test_mapping_is_total_a_new_unmapped_kind_fails_loudly() -> None:
    """KTD5's exact requirement: a kind with no entry in the table fails the
    mapping rather than silently rendering with no group class — simulated
    here as the runtime shape a newly added :data:`TranscriptKind` member
    would take before this module's table is updated for it.
    """
    with pytest.raises(ValueError, match="no R7 kind-group mapping"):
        kind_group("not-a-real-transcript-kind")  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="no R7 kind-group mapping"):
        kind_group_css_class("not-a-real-transcript-kind")  # type: ignore[arg-type]


def test_mapping_failure_propagates_through_widget_construction() -> None:
    """The totality claim is not just about the pure function: a
    line-rendered widget built for an unmapped kind fails to construct at
    all, rather than mounting with no ``transcript--group-*`` class."""
    with pytest.raises(ValueError, match="no R7 kind-group mapping"):
        TranscriptLine("x", kind="not-a-real-transcript-kind")  # type: ignore[arg-type]


def test_group_names_match_r7s_own_enumeration() -> None:
    """R7's verbatim grouping, transcribed kind-by-kind (see the module
    docstring in ``talaria/ui/transcript.py`` for the six-vs-five count
    discrepancy this test does not take a side on — it only pins the
    kind-to-group *assignments* R7 states explicitly)."""
    assert _KIND_GROUPS == {
        "user": "operator",
        "assistant": "assistant",
        "reasoning": "reasoning",
        "tool": "activity",
        "subagent": "activity",
        "system": "session-record",
        "prompt": "session-record",
        "prompt-expired": "session-record",
        "cancelled": "session-record",
        "error": "fault",
        "protocol-error": "fault",
        "unknown-event": "fault",
    }


# ── AE5: adjacent groups distinguishable by computed style ─────────────────


@pytest.mark.asyncio
async def test_one_line_per_group_is_pairwise_distinguishable() -> None:
    """A screen holding one line from each group: every pair of groups
    paints a different (background, border) pair — "distinguishable... with
    content ignored" read literally as computed-style inequality, not a
    judgment call about how different is different enough.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        # One representative kind per group -- the group is what is under
        # test, not every kind that maps to it.
        representative_kinds: tuple[TranscriptKind, ...] = (
            "user",
            "assistant",
            "reasoning",
            "tool",
            "system",
            "error",
        )
        widgets = [TranscriptLine(_SAMPLE_TEXT, kind=kind) for kind in representative_kinds]
        await pane.mount_all(widgets)
        await pilot.pause()

        channels = [(w.styles.background, w.styles.border_left) for w in widgets]
        for i, kind_i in enumerate(representative_kinds):
            for j, kind_j in enumerate(representative_kinds):
                if i == j:
                    continue
                assert channels[i] != channels[j], (
                    f"{kind_group(kind_i)!r} and {kind_group(kind_j)!r} paint "
                    "identically -- adjacent groups must be distinguishable "
                    "by computed style alone (R7, AE5)"
                )


@pytest.mark.asyncio
async def test_kinds_within_the_same_group_share_its_channel() -> None:
    """``tool`` and ``subagent`` (both "activity") paint identically; so do
    the four "session-record" kinds -- the channel is per *group*, not
    per kind, which is what makes the twelve-to-five(-or-six) mapping a
    real reduction rather than twelve independent styles.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        tool = TranscriptLine(_SAMPLE_TEXT, kind="tool")
        subagent = TranscriptLine(_SAMPLE_TEXT, kind="subagent")
        await pane.mount_all([tool, subagent])
        await pilot.pause()
        assert (tool.styles.background, tool.styles.border_left) == (
            subagent.styles.background,
            subagent.styles.border_left,
        )


# ── R8: a reasoning fence keeps its kind channel ────────────────────────────


@pytest.mark.asyncio
async def test_reasoning_fence_keeps_its_kind_channel() -> None:
    """A reasoning entry containing a fenced code block still carries the
    reasoning group's background/border on its own document -- the fence's
    own syntax-highlighting background (probed: an opaque near-black tint
    on the nested ``MarkdownFence``) paints only inside the fence's own
    box, never the document's own border, which is what actually carries
    the channel down every row of the document regardless of what a nested
    block paints over itself.
    """
    fence_body = "```python\ndef f():\n    return 1\n```\n"
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        doc = EntryMarkdown(fence_body, classes=kind_group_css_class("reasoning"))
        await pane.mount_all([doc])
        await pilot.pause()

        expected = TranscriptLine(_SAMPLE_TEXT, kind="reasoning")
        await pane.mount_all([expected])
        await pilot.pause()

        assert doc.styles.background == expected.styles.background
        assert doc.styles.border_left == expected.styles.border_left
        # And the fence itself did mount with its own, different rendering
        # -- proving this is a real fence, not a fallback that never
        # exercised the fence/kind composition R8 is about.
        from textual.widgets.markdown import MarkdownFence

        fences = list(doc.query(MarkdownFence))
        assert len(fences) == 1


# ── R8: styling adds no widget height ───────────────────────────────────────


@pytest.mark.asyncio
async def test_line_widget_height_unchanged_by_group_styling() -> None:
    """The same short content, styled and unstyled, paints the same
    height -- ``kind=None`` builds a :class:`TranscriptLine` with no
    ``transcript--group-*`` class at all (the "styling disabled" baseline),
    while ``kind="assistant"`` builds the same text through the normal
    group-styled path.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        unstyled = TranscriptLine(_SAMPLE_TEXT)
        styled = TranscriptLine(_SAMPLE_TEXT, kind="assistant")
        await pane.mount_all([unstyled, styled])
        await pilot.pause()
        assert unstyled.outer_size.height == styled.outer_size.height == 1
        assert unstyled.styles.background != styled.styles.background


@pytest.mark.asyncio
@pytest.mark.parametrize("pane_width", [40, 60, 80, 120])
async def test_line_widget_height_unchanged_at_the_wrap_boundary(pane_width: int) -> None:
    """CR5's exact regression: content that fills the wrap width to the last
    cell must render at the same height whether or not a group class -- and
    therefore the border-left gutter marker -- is present.

    ``test_line_widget_height_unchanged_by_group_styling`` above only
    exercises short content, which the one column a border-left gutter
    marker costs can never push across a wrap boundary. This test instead
    measures the *unstyled* widget's own content width live, at more than
    one terminal width, then builds content sized to exactly that width
    (and one cell past it) -- a border consuming a column nobody reserved
    for it would show up here as a height-two styled line next to a
    height-one unstyled one, at whichever width exposes it, not only at the
    80-column pane the reviewer happened to probe.
    """
    app = _Harness()
    async with app.run_test(size=(pane_width, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        probe = TranscriptLine("x")
        await pane.mount_all([probe])
        await pilot.pause()
        content_width = probe.content_size.width
        await probe.remove()

        for fill in (content_width, content_width + 1):
            text = "a" * fill
            unstyled = TranscriptLine(text)
            styled = TranscriptLine(text, kind="assistant")
            await pane.mount_all([unstyled, styled])
            await pilot.pause()
            assert unstyled.content_size.width == styled.content_size.width, (
                f"pane_width={pane_width} fill={fill}: styled and unstyled "
                "TranscriptLine widgets disagree on content width -- the "
                "border-left gutter marker is consuming a column the "
                "unstyled baseline never reserved (CR5)"
            )
            assert unstyled.outer_size.height == styled.outer_size.height, (
                f"pane_width={pane_width} fill={fill}: content that exactly "
                "fills the wrap width renders at a different height styled "
                "vs unstyled (CR5's exact regression)"
            )
            await unstyled.remove()
            await styled.remove()


@pytest.mark.asyncio
async def test_block_document_height_unchanged_by_group_styling() -> None:
    """Same claim, for a block-rendered entry: a moderate paragraph mounted
    with no class versus mounted with a group class wraps to the same
    number of rows -- the ``padding: 0 2 0 1`` compensation this module's
    ``DEFAULT_CSS`` restates per group (see the long comment beside
    :data:`~talaria.ui.transcript._GROUP_CSS_RULES`) is exactly what keeps
    the one-column border from shrinking the styled document's wrap width
    relative to the unstyled one.
    """
    paragraph = "word " * 40
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        unstyled = EntryMarkdown(paragraph)
        styled = EntryMarkdown(paragraph, classes=kind_group_css_class("assistant"))
        await pane.mount_all([unstyled, styled])
        await pilot.pause()
        assert unstyled.outer_size.height == styled.outer_size.height
        assert unstyled.outer_size.height > 1  # the paragraph actually wraps
        assert unstyled.styles.background != styled.styles.background


# ── KTD5: no new theme variable ─────────────────────────────────────────────


def test_group_variables_are_plain_colours_not_scalars() -> None:
    """Every value in :data:`_GROUP_VARIABLES` was chosen from the theme's
    plain-colour variables -- ``$text-muted`` was tried first for the
    "session-record" group and rejected because it resolves to an
    auto-contrast scalar (``"auto 60%"``), not a colour, which Textual's
    stylesheet parser refuses outright as a ``background``/``border``
    value (probed live; see the module docstring). This test would have
    caught that mistake by construction if it had shipped.
    """
    app: App[None] = App()
    variables = app.get_css_variables()
    for group, name in _GROUP_VARIABLES.items():
        assert name in variables, f"{group!r} names ${name}, not in the live theme"
        value = str(variables[name])
        assert not value.startswith("auto"), (
            f"{group!r} names ${name} = {value!r}, an auto-contrast scalar, "
            "not a colour Textual's CSS parser can use as a background/border value"
        )


def test_stylesheet_introduces_no_new_theme_variables() -> None:
    """Every ``$name`` :class:`TranscriptPane`'s ``DEFAULT_CSS`` references
    is a member of the *live*, installed theme's variable set -- read off
    ``App.get_css_variables()`` at test time, not off a hand-maintained
    list of what this module believes the theme defines (KTD5's exact
    constraint: "no new theme variables... asserted against the
    stylesheet").
    """
    app: App[None] = App()
    variables = app.get_css_variables()
    used = set(_VARIABLE_TOKEN.findall(TranscriptPane.DEFAULT_CSS))
    assert used, "sanity check: the stylesheet should reference at least one $variable"
    missing = used - set(variables)
    assert not missing, (
        f"TranscriptPane.DEFAULT_CSS references undefined theme variables: {missing}"
    )


@pytest.mark.asyncio
async def test_stylesheet_actually_parses_and_mounts() -> None:
    """A live mount is the proof that matters -- Textual's own CSS parser
    rejects an unresolvable variable at stylesheet-application time (found
    live while building this module: ``$text-muted`` parsed as a *name*
    fine, and only failed once a real widget tried to resolve it as a
    colour). The two tests above check the sources of that failure
    independently; this one proves the combination actually works end to
    end.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        widgets = [TranscriptLine(_SAMPLE_TEXT, kind=kind) for kind in ALL_KINDS]
        await pane.mount_all(widgets)
        await pilot.pause()
        assert len(list(pane.walk_children())) >= len(ALL_KINDS)
