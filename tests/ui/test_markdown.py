"""Inline markdown on agent prose, and the four things it must not break.

The feature is small; the constraints around it are not, and they are what most
of this file is about.

*Content survives.* R6's real obligation is that no transcript content is ever
dropped, and this renderer deletes characters. So one test walks a corpus and
proves that the only characters it can ever delete are the delimiters of a
construct it styled — asserted as a property of the transform, over adversarial
input, rather than case by case.

*Identifiers survive.* ``__init__.py`` and ``**kwargs`` are valid markdown
emphasis. Rendering them as *init*.py and **kwargs** would silently change an
identifier the operator may be about to act on, which on a client whose whole
posture is "the rendered path is the executed path" is a worse defect than
asterisks on screen.

*Untrusted text stays untrusted.* Every renderer in this package builds a
:class:`~rich.text.Text` so that Rich's console markup is never parsed, and
defangs so that escape sequences and invisible characters cannot act. Adding a
parser is exactly the kind of change that quietly reintroduces both, so both are
asserted here again rather than assumed from ``literal.py``'s own suite.

*The pane still knows what it holds.* The drawn line and the projected line are
no longer the same string, and several existing checks compare the pane against
the projection. The last section covers that split.
"""

from __future__ import annotations

from typing import Any

import pytest

from talaria.domain.projection import transcript_view
from talaria.replay.gate import interface_shows_everything
from talaria.ui.literal import INVISIBLE_MARK, defang
from talaria.ui.markdown import CODE_STYLE, EMPHASIS_STYLE, STRONG_STYLE, inline_markdown
from tests.ui.conftest import event, paused_app

# ── helpers ──────────────────────────────────────────────────────────────


def styled(source: str) -> list[tuple[str, str]]:
    """Every styled run as ``(drawn text, style)``, in document order."""
    text = inline_markdown(source)
    return [(text.plain[span.start : span.end], str(span.style)) for span in text.spans]


def drawn(source: str) -> str:
    """The characters the terminal would paint."""
    return inline_markdown(source).plain


def deleted(source: str) -> str:
    """Which characters of the defanged source did not survive rendering.

    Walks the drawn text against the defanged source as a subsequence, which
    also proves the ordering claim: if any character were reordered, inserted,
    or substituted, the walk would not consume the whole drawn string and the
    assertion below fires before the caller ever sees a result.
    """
    defanged = defang(source)
    painted = drawn(source)
    cursor = 0
    missing: list[str] = []
    for char in defanged:
        if cursor < len(painted) and painted[cursor] == char:
            cursor += 1
        else:
            missing.append(char)
    assert cursor == len(painted), (
        f"the drawn text is not the source with characters deleted: "
        f"{source!r} -> {painted!r}"
    )
    return "".join(missing)


# ── the visible complaint ────────────────────────────────────────────────


def test_strong_emphasis_and_code_are_styled_and_their_delimiters_go_away() -> None:
    """The whole point, taken from a real reply read off a live gateway."""
    source = "**Judgment** before action — the code is `concise` by default."
    assert drawn(source) == "Judgment before action — the code is concise by default."
    assert styled(source) == [("Judgment", STRONG_STYLE), ("concise", CODE_STYLE)]


def test_single_asterisks_are_emphasis_and_double_are_strong() -> None:
    assert styled("Use *this* and **that**.") == [
        ("this", EMPHASIS_STYLE),
        ("that", STRONG_STYLE),
    ]


def test_a_code_span_inside_emphasis_keeps_both_styles() -> None:
    """Code spans are extracted before emphasis is matched, so the enclosing
    construct has to survive the extraction rather than be broken by it."""
    runs = styled("**run `talaria --record` first**")
    assert drawn("**run `talaria --record` first**") == "run talaria --record first"
    assert ("run ", STRONG_STYLE) in runs
    assert ("talaria --record", CODE_STYLE) in runs
    assert ("talaria --record", STRONG_STYLE) in runs, "the strong run stopped at the code span"


def test_a_triple_run_is_both_bold_and_italic() -> None:
    assert drawn("***emphatic***") == "emphatic"
    assert sorted(style for _, style in styled("***emphatic***")) == sorted(
        [EMPHASIS_STYLE, STRONG_STYLE]
    )


# ── the identifiers that must not be mangled ─────────────────────────────

#: Strings a model writes routinely, every one of which a naive emphasis pass
#: would rewrite. The signature line and the dunder are the two that matter
#: most: both are text an operator may be about to copy, type, or approve.
UNTOUCHED = [
    "def handler(*args, **kwargs) -> None:",
    "the entry point lives in __init__.py",
    "__all__ and __main__ are left alone",
    "the area is 2 * 3 * 4 units",
    "match *.py and *.txt but not *.pyc",
    "**kwargs and **args in one sentence",
    "a lone * asterisk survives",
    "trailing emphasis marker at the end *",
    "snake_case_names_stay_snake_case",
]


@pytest.mark.parametrize("source", UNTOUCHED)
def test_text_that_only_looks_like_markdown_is_left_exactly_alone(source: str) -> None:
    assert drawn(source) == source
    assert styled(source) == []


def test_asterisks_inside_a_code_span_are_shown_not_obeyed() -> None:
    """The reason code spans are matched first.

    A code span is the one construct here whose characters are meant to be read
    literally. Matching emphasis first would style the inside of it, so the
    screen would disagree with the code the operator is reading.
    """
    source = "call it as `f(**opts)` please"
    assert drawn(source) == "call it as f(**opts) please"
    assert styled(source) == [("f(**opts)", CODE_STYLE)]


# ── content survives ─────────────────────────────────────────────────────

#: Deliberately hostile: unbalanced delimiters, nesting, adjacency, delimiters
#: at both ends of the line, and the constructs this renderer does not
#: implement (links, headings, list bullets) which must therefore pass through.
CORPUS = [
    *UNTOUCHED,
    "",
    "*",
    "**",
    "***",
    "****",
    "`",
    "``",
    "`unclosed code span",
    "**unclosed strong",
    "*a**b*c**d*",
    "**a**b**c**",
    "`a``b`",
    "``a`b``",
    "- a bullet with **strong** in it",
    "# a heading with `code` in it",
    "[a link](https://example.com/**not-bold**)",
    "**edge**",
    "*x*",
    "a**b**c",
    "**bold with `code` inside** and after",
    "`**not bold**` stays literal",
    "***both at once***",
    "**Judgment** before action — the code is `concise` by default.",
    "| col | **col** | `col` |",
    "emoji ✅ and a tab\tbetween",
]


@pytest.mark.parametrize("source", CORPUS)
def test_nothing_but_a_matched_delimiter_is_ever_removed(source: str) -> None:
    """R6's surviving obligation, asserted as a property rather than by example.

    The rendered line may be shorter than the projected one — that is the
    feature — but the only characters it may be shorter by are the asterisks and
    backticks of a construct that was styled. Any other deletion is content
    loss, and content loss is the thing R6 forbids outright.
    """
    assert set(deleted(source)) <= {"*", "`"}


@pytest.mark.parametrize("source", CORPUS)
def test_every_style_span_covers_real_text(source: str) -> None:
    """A span with reversed or out-of-range offsets renders as nothing, silently.

    Worth its own check because the offsets are into the *output*, which is
    shorter than the input by an amount that depends on what matched — the exact
    shape of arithmetic that is wrong quietly.
    """
    text = inline_markdown(source)
    for span in text.spans:
        assert 0 <= span.start < span.end <= len(text.plain)


# ── untrusted text stays untrusted ───────────────────────────────────────


def test_rich_console_markup_is_still_four_visible_characters() -> None:
    """The reason this package builds ``Text`` rather than passing strings."""
    source = "[red]danger[/] and **real emphasis**"
    assert drawn(source) == "[red]danger[/] and real emphasis"
    assert styled(source) == [("real emphasis", STRONG_STYLE)]


def test_control_characters_and_invisibles_are_still_defanged() -> None:
    """Defanging happens first, so the string being indexed into is the string
    being drawn — and a hidden character cannot ride in through the new path."""
    source = "**bo​ld**\x1b[2J and ‮reversed"
    painted = drawn(source)
    assert "\x1b" not in painted and "​" not in painted and "‮" not in painted
    assert painted.count(INVISIBLE_MARK) == 2
    assert "␛" in painted


def test_a_null_byte_cannot_forge_a_code_span() -> None:
    """The code-span placeholder is NUL, which is safe only because defanging
    removes NUL first. If that ever stopped being true, gateway text could aim a
    style span at a run of characters it does not cover."""
    source = "\x00\x00 `real` \x00"
    painted = drawn(source)
    assert painted == "␀␀ real ␀"
    assert styled(source) == [("real", CODE_STYLE)]


def test_the_defanged_text_can_never_contain_the_placeholder() -> None:
    """Stated directly, because the test above only samples one input."""
    for codepoint in (0x00, 0x01, 0x1B, 0x7F):
        assert "\x00" not in defang(chr(codepoint))


# ── the pane, where projected text and drawn text part ways ──────────────


def _markdown_session() -> list[dict[str, Any]]:
    reply = "Lead with **the recommendation** and cite `path:line`.\nThen stop."
    return [
        event("gateway.ready", {}),
        event("message.start", {}),
        event("message.delta", {"text": reply}),
        event("message.complete", {"text": reply}),
        event("tool.complete", {"name": "grep", "summary": "matched **kw** in 2 files"}),
    ]


async def _drain(app: Any, pilot: Any, controls: Any) -> None:
    controls.resume()
    await app.drain(timeout=60.0)
    await pilot.pause()
    await app.render_snapshot()
    await pilot.pause()


@pytest.mark.asyncio
async def test_the_pane_draws_agent_prose_styled_and_tool_output_verbatim() -> None:
    """Both halves of the same claim, because either alone is satisfiable by a
    renderer that is simply broken in a convenient direction."""
    app, controls = paused_app(_markdown_session())
    async with app.run_test(size=(100, 24)) as pilot:
        await _drain(app, pilot, controls)
        pane = app.transcript

        painted = "\n".join(pane.drawn_lines)
        assert "Lead with the recommendation and cite path:line." in painted
        assert "**the recommendation**" not in painted

        # The tool line is program output, so its asterisks are the program's.
        tool_line = next(line for line in pane.drawn_lines if "grep" in line)
        assert "matched **kw** in 2 files" in tool_line
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_pane_still_reports_the_projection_lines_it_holds() -> None:
    """``rendered_lines`` is what the replay gate and the bounds suite compare
    against ``view.lines``, so it has to stay the projection's text even though
    the screen now shows something shorter."""
    app, controls = paused_app(_markdown_session())
    async with app.run_test(size=(100, 24)) as pilot:
        await _drain(app, pilot, controls)
        pane = app.transcript

        view = transcript_view(app.state)
        assert pane.rendered_lines == view.lines[pane.condensed_count :]
        assert pane.drawn_lines != pane.rendered_lines, "nothing was styled; this proves nothing"
        assert len(pane.drawn_lines) == len(pane.rendered_lines)

        # Through the gate's own function, not just the comparison it makes.
        # AE5's completeness verdict is a position-by-position match of the
        # pane against the projection, so a renderer that reported drawn text
        # here would report content loss on every styled reply.
        assert interface_shows_everything(app, settled=True)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_streaming_prose_is_styled_before_the_turn_commits() -> None:
    """The provisional tail carries no entry, so its kind is asserted by the
    projection rather than read off one. If that were wrong, markdown would snap
    into place only when the turn ended — visible as a flicker on every reply."""
    app, controls = paused_app(
        [
            event("gateway.ready", {}),
            event("message.start", {}),
            event("message.delta", {"text": "still **arriving**"}),
        ]
    )
    async with app.run_test(size=(100, 24)) as pilot:
        await _drain(app, pilot, controls)
        pane = app.transcript

        view = transcript_view(app.state)
        assert view.kinds[-1] == "assistant"
        assert view.committed_lines < view.total_lines, "nothing was provisional"
        assert "still arriving" in "\n".join(pane.drawn_lines)
        await app.shutdown_sources()
