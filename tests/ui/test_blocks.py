"""The block factory's forgery proof (U3): every channel Textual's stock
``Markdown`` widget would let gateway-authored text act through, closed and
proven shut, one channel at a time.

Textual's ``Markdown`` widget is not adversarial by design — it renders a
document a developer wrote. Every test here asks the same question of a
different construct: when the *text* comes from a channel talaria does not
control, can it still make the terminal do something other than draw
characters? R9 (as amended by RA1), R10, and R15 are one claim stated four
ways — the gateway's bytes may be drawn but never acted on — and KTD4 is
explicit that the parser configuration is not the whole of that claim, so each
channel gets its own proof rather than one test that trusts the others by
implication.

The allowlist test is the odd one out: it does not import
``talaria.ui.blocks.RULE_SET`` and compare it to itself. It hardcodes the
expected rule names directly, the way ``test_prompts.py`` walks
``unicodedata`` fresh rather than trusting ``literal.py``'s own table — a test
that re-reads the module's own constant as ground truth cannot catch that
constant drifting.
"""

from __future__ import annotations

import pytest
from textual import __version__ as TEXTUAL_VERSION
from textual.app import App, ComposeResult
from textual.widgets import Markdown
from textual.widgets.markdown import MarkdownBlock, MarkdownFence, MarkdownStream

from talaria.ui.blocks import LINK_STYLE, EntryMarkdown, enabled_rules, parser_factory
from talaria.ui.literal import defang

#: This module is written against one specific Textual release. KTD3 and
#: KTD4's evidence — the append/update/get_stream semantics probed below —
#: is evidence *about 8.2.8*; a different version is a different, unaudited
#: contract, so this fails loudly rather than silently re-running the same
#: assertions against behavior nobody has reviewed.
_PINNED_TEXTUAL_VERSION = "8.2.8"


class Host(App[None]):
    """A bare app that mounts ``count`` ``EntryMarkdown`` documents and
    records every ``Markdown.LinkClicked`` message any of them post."""

    def __init__(self, count: int = 1) -> None:
        super().__init__()
        self._count = count
        self.link_clicks: list[str] = []
        self.opened_urls: list[str] = []

    def compose(self) -> ComposeResult:
        for index in range(self._count):
            yield EntryMarkdown(id=f"doc{index}")

    def on_markdown_link_clicked(self, event: Markdown.LinkClicked) -> None:
        self.link_clicks.append(event.href)

    def open_url(self, url: str, *, new_tab: bool = True) -> None:
        # Overridden rather than the real browser-launching implementation,
        # so "nothing is opened" is a recorded fact, not an assumption about
        # what a real `open_url` would have done in a headless test run.
        self.opened_urls.append(url)

    def doc(self, index: int = 0) -> EntryMarkdown:
        return self.query_one(f"#doc{index}", EntryMarkdown)


def block_texts(document: EntryMarkdown) -> list[str]:
    """Every mounted block's drawn text, in document order."""
    return [block._content.plain for block in _blocks(document)]


def _blocks(document: EntryMarkdown) -> list[MarkdownBlock]:
    return list(document.query(MarkdownBlock))


# ── AE3 quartet ──────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_html_is_literalized_as_visible_text_never_dropped() -> None:
    app = Host()
    async with app.run_test() as pilot:
        await app.doc().update("<script>alert(1)</script>")
        await pilot.pause()
        assert block_texts(app.doc()) == ["<script>alert(1)</script>"]


@pytest.mark.asyncio
async def test_rich_console_markup_is_never_parsed() -> None:
    """The widget draws from parser tokens via `Content`, never by handing a
    string through Rich's console-markup parser — asserted here rather than
    only trusted from that architectural fact, the same way ``test_markdown.py``
    re-asserts it for the inline styler instead of only trusting ``literal.py``."""
    app = Host()
    async with app.run_test() as pilot:
        await app.doc().update("[bold red]x[/] and **real emphasis**")
        await pilot.pause()
        block = _blocks(app.doc())[0]
        assert "[bold red]x[/]" in block._content.plain
        # The literal brackets carry no style; only the real emphasis does.
        styled_runs = [
            block._content.plain[span.start : span.end] for span in block._content.spans
        ]
        assert "bold red" not in styled_runs
        assert "real emphasis" in styled_runs


@pytest.mark.asyncio
async def test_a_raw_ansi_escape_is_defanged_before_it_reaches_the_parser() -> None:
    app = Host()
    async with app.run_test() as pilot:
        source = "before\x1b[2Jafter"
        await app.doc().update(source)
        await pilot.pause()
        painted = block_texts(app.doc())[0]
        assert "\x1b" not in painted
        assert painted == defang(source)


@pytest.mark.asyncio
async def test_a_link_is_styled_but_clicking_it_posts_no_linkclicked_and_opens_nothing() -> None:
    app = Host()
    async with app.run_test() as pilot:
        await app.doc().update("[click here](https://example.com)")
        await pilot.pause()
        block = _blocks(app.doc())[0]
        assert block._content.plain == "click here"
        assert any(span.style == LINK_STYLE for span in block._content.spans), (
            "the link body carries no visible style at all"
        )

        await pilot.click("#doc0", offset=(2, 0))
        await pilot.pause()

        assert app.link_clicks == []
        assert app.opened_urls == []


@pytest.mark.asyncio
async def test_the_same_click_position_does_fire_linkclicked_on_the_stock_widget() -> None:
    """The positive control for the test above: proves ``offset=(2, 0)`` lands
    on the rendered link, so the inertness just asserted is a property of
    ``EntryMarkdown``, not a click that missed."""

    class StockHost(App[None]):
        def __init__(self) -> None:
            super().__init__()
            self.link_clicks: list[str] = []

        def compose(self) -> ComposeResult:
            yield Markdown("[click here](https://example.com)", open_links=False)

        def on_markdown_link_clicked(self, event: Markdown.LinkClicked) -> None:
            self.link_clicks.append(event.href)

    app = StockHost()
    async with app.run_test() as pilot:
        await pilot.pause()
        await pilot.click(Markdown, offset=(2, 0))
        await pilot.pause()
        assert app.link_clicks == ["https://example.com"]


# ── images: alt text and target both render as text (R10) ──────────────────


@pytest.mark.asyncio
async def test_an_image_renders_as_alt_and_target_text_nothing_fetched() -> None:
    app = Host()
    async with app.run_test() as pilot:
        await app.doc().update("![a cat](http://example.com/cat.png)")
        await pilot.pause()
        painted = block_texts(app.doc())[0]
        assert painted == "a cat (http://example.com/cat.png)"
        # No click action either: images share the inert rendering hook.
        await pilot.click("#doc0", offset=(2, 0))
        await pilot.pause()
        assert app.link_clicks == []


@pytest.mark.asyncio
async def test_an_image_with_no_alt_text_still_shows_its_target() -> None:
    app = Host()
    async with app.run_test() as pilot:
        await app.doc().update("![](http://example.com/cat.png)")
        await pilot.pause()
        assert block_texts(app.doc())[0] == "(http://example.com/cat.png)"


@pytest.mark.asyncio
async def test_an_image_with_inline_formatted_alt_text_renders_flattened_plain_text() -> None:
    """``child.content`` on an image token is the raw, unparsed alt-text
    *source* -- for ``![*a* cat](url)`` that is the literal string
    ``"*a* cat"``, asterisks included. Using it directly (the bug this
    pins) would render ``"*a* cat (url)"``: inline formatting inside alt
    text leaking as literal markup punctuation instead of the flattened
    plain text R10 and the module docstring both promise. The alt text must
    come out with the emphasis markers gone and only the letters left.
    """
    app = Host()
    async with app.run_test() as pilot:
        await app.doc().update("![*a* **bold** cat](http://example.com/cat.png)")
        await pilot.pause()
        painted = block_texts(app.doc())[0]
        assert painted == "a bold cat (http://example.com/cat.png)"
        assert "*" not in painted


# ── bare URLs stay text (linkify off) ───────────────────────────────────────


@pytest.mark.asyncio
async def test_a_bare_url_stays_plain_text() -> None:
    app = Host()
    async with app.run_test() as pilot:
        await app.doc().update("visit https://example.com now")
        await pilot.pause()
        block = _blocks(app.doc())[0]
        assert block._content.plain == "visit https://example.com now"
        assert block._content.spans == []


# ── the allowlist pin: R9 as amended by RA1 ─────────────────────────────────


def test_the_parsers_enabled_rule_set_matches_the_hardcoded_allowlist() -> None:
    """The allowlist upper bound. Every name below is a construct that can
    reach the screen; markdown-it adding, renaming, or the "gfm-like" preset
    picking up a new rule fails this test rather than silently rendering it.

    RA1's amendment is asserted directly: ``emphasis`` (which does not
    distinguish ``*x*`` from ``_x_`` — there is no separate "underscore
    emphasis" rule to check) and ``strikethrough`` are both present.
    """
    expected = {
        "core": ("block", "inline", "linkify", "normalize", "text_join"),
        "block": (
            "blockquote",
            "code",
            "fence",
            "heading",
            "hr",
            "html_block",
            "lheading",
            "list",
            "paragraph",
            "reference",
            "table",
        ),
        "inline": (
            "autolink",
            "backticks",
            "emphasis",
            "entity",
            "escape",
            "html_inline",
            "image",
            "link",
            "linkify",
            "newline",
            "strikethrough",
            "text",
        ),
        "inline2": ("balance_pairs", "emphasis", "fragments_join", "strikethrough"),
    }
    observed = enabled_rules(parser_factory())
    assert observed == expected
    assert "emphasis" in observed["inline"], "RA1: underscore emphasis comes from this rule"
    assert "strikethrough" in observed["inline"], "RA1: strikethrough joins the allowlist"


def test_html_and_linkify_options_are_off_even_though_their_rules_stay_listed() -> None:
    """``html_block``/``html_inline``/``linkify`` appear in the rule set above
    unconditionally — markdown-it has no separate rule to disable for either
    feature, only an *option* that makes the rule a no-op. So the allowlist
    test alone cannot see html/linkify drift; this asserts the options
    directly, which is the other half of what ``parser_factory`` checks
    before returning a parser at all."""
    parser = parser_factory()
    assert parser.options["html"] is False
    assert parser.options["linkify"] is False


# ── defang runs before parse (parity with literal.py) ───────────────────────


@pytest.mark.asyncio
async def test_a_bidi_and_nul_payload_arrives_as_control_pictures_in_block_content() -> None:
    app = Host()
    async with app.run_test() as pilot:
        source = "rm -rf ‮/emoh/\x00 safe"
        await app.doc().update(source)
        await pilot.pause()
        painted = block_texts(app.doc())[0]
        assert "‮" not in painted
        assert "\x00" not in painted
        assert painted == defang(source)


# ── entry isolation: an unclosed fence in one entry never absorbs another ───


@pytest.mark.asyncio
async def test_an_unclosed_fence_in_one_entry_never_absorbs_a_second_entrys_text() -> None:
    """R15: the entry *is* the parser-isolation boundary. Each ``EntryMarkdown``
    builds its own parser on every ``update``/``append`` (``parser_factory``
    has no shared state across calls or instances), so nothing here proves a
    clever containment trick — it proves the absence of one: there is no
    module-level buffer an open construct in one entry could leak into."""
    app = Host(count=2)
    async with app.run_test() as pilot:
        first, second = app.doc(0), app.doc(1)

        await first.append("```python\nprint('still open")
        await second.update("an unrelated paragraph")
        await pilot.pause()

        # The still-open fence in the first entry has not touched the second.
        assert block_texts(second) == ["an unrelated paragraph"]
        second_blocks = _blocks(second)
        assert len(second_blocks) == 1
        assert not isinstance(second_blocks[0], type(_blocks(first)[0]))

        await first.append("')\n```\n")
        await pilot.pause()

        # Closing the fence resolves inside the first entry alone.
        first_blocks = _blocks(first)
        assert len(first_blocks) == 1
        assert "print('still open')" in first_blocks[0]._content.plain
        assert block_texts(second) == ["an unrelated paragraph"], (
            "the second entry changed when the first entry's fence closed"
        )


# ── the Textual 8.2.8 pin: append reparses, update replaces, get_stream ────


def test_the_installed_textual_version_matches_the_audited_one() -> None:
    assert TEXTUAL_VERSION == _PINNED_TEXTUAL_VERSION, (
        "talaria/ui/blocks.py and this suite were audited against Textual "
        f"{_PINNED_TEXTUAL_VERSION}; {TEXTUAL_VERSION} is installed. Re-review "
        "KTD3's append/update/get_stream evidence before moving this pin."
    )


@pytest.mark.asyncio
async def test_append_reparses_from_the_last_unfinished_block_not_the_whole_document() -> None:
    """The probed fact KTD3 cites directly: an open fence grows across
    ``append`` calls as one widget, its content accumulating, rather than
    each boundary re-parsing the whole document from the top.

    Rendered content alone cannot tell a true incremental ``append`` apart
    from a disguised full-document ``update(self.source + fragment)`` --
    both end up painting the same fence text (confirmed live: replaying this
    test's assertions against that substitution still passed every one of
    them). What actually differs is *widget identity*. Textual's own
    ``Markdown.append`` mutates the trailing block in place through
    ``MarkdownBlock._update_from_block`` (``_markdown.py:1483``,
    ``MarkdownFence._update_from_block`` at ``:943`` copies the new code
    onto ``self`` rather than replacing ``self``), so the *same*
    ``MarkdownFence`` object survives every boundary. ``Markdown.update``
    takes the opposite path unconditionally: it removes every existing
    block and mounts brand-new ones from scratch
    (``_markdown.py:1403-1416``). So a disguised ``update()`` would still
    paint the right characters but would mount a *new* ``MarkdownFence``
    instance on every call -- which is exactly what the identity
    assertions below catch.
    """
    app = Host()
    async with app.run_test() as pilot:
        document = app.doc()

        def fence_code() -> str:
            blocks = _blocks(document)
            assert len(blocks) == 1
            fence = blocks[0]
            assert isinstance(fence, MarkdownFence)
            return fence.code

        await document.update("```text\n")
        await pilot.pause()
        assert fence_code() == ""
        fence_widget = _blocks(document)[0]

        await document.append("line one\n")
        await pilot.pause()
        assert fence_code() == "line one"
        assert _blocks(document)[0] is fence_widget, (
            "append must extend the same fence widget in place -- a "
            "disguised whole-document update() would mount a new one here"
        )

        await document.append("line two\n")
        await pilot.pause()
        assert fence_code() == "line one\nline two", (
            "the second append should extend the still-open fence, not replace it"
        )
        assert _blocks(document)[0] is fence_widget, (
            "the fence widget must still be the same object after a second append"
        )

        await document.append("```\n")
        await document.append("trailing paragraph\n")
        await pilot.pause()

        blocks = _blocks(document)
        assert isinstance(blocks[0], MarkdownFence)
        assert blocks[0] is fence_widget, "closing the fence still updates it in place"
        assert blocks[0].code == "line one\nline two"
        assert block_texts(document)[1:] == ["trailing paragraph"]


@pytest.mark.asyncio
async def test_update_replaces_the_whole_document_rather_than_extending_it() -> None:
    app = Host()
    async with app.run_test() as pilot:
        document = app.doc()
        await document.update("first version\n\nwith two blocks")
        await pilot.pause()
        assert len(_blocks(document)) == 2

        await document.update("second version, one block")
        await pilot.pause()

        assert block_texts(document) == ["second version, one block"]


@pytest.mark.asyncio
async def test_markdown_get_stream_is_public_and_returns_a_markdown_stream() -> None:
    """The corrected KTD3 evidence: Textual 8.2.8 does expose a public
    streaming surface. Talaria's own factory chooses direct ``append`` at the
    50ms coalescing boundary anyway (KTD3's rationale), but that choice is
    only meaningful if this alternative genuinely exists — so it is asserted
    here rather than only cited in a comment."""
    assert not Markdown.get_stream.__name__.startswith("_")
    app = Host()
    async with app.run_test() as pilot:
        await pilot.pause()
        document = app.doc()
        stream = Markdown.get_stream(document)
        try:
            assert isinstance(stream, MarkdownStream)
        finally:
            # `MarkdownStream.stop` re-raises the cancellation it triggers
            # (a Textual quirk unrelated to what this test is pinning), so
            # the background task is cancelled directly instead.
            if stream._task is not None:
                stream._task.cancel()
