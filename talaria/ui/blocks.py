"""The one constructor for a safely configured Markdown entry document.

ADR-0006 restates ADR-0005's decision 7 — the rule that untrusted text may
only reach the screen through :func:`~talaria.ui.literal.literal_text` — to
say it may *additionally* reach the screen through the parser configuration
and the rendering hooks this module builds, and nothing else (KTD4). R4
confines this module to configuration and subclassed rendering hooks over
Textual's built-in ``Markdown`` widget family: nothing here parses markdown or
assembles blocks from tokens. That work is Textual's; this module's job is to
prove the four ways Textual's own widget would otherwise let gateway-authored
text act rather than merely display.

**Parser configuration is not the whole boundary.** Textual's stock
``Markdown`` widget installs ``@click`` action metadata for every link
*unconditionally* and posts ``Markdown.LinkClicked`` regardless of
``open_links`` (``_markdown.py:342``, ``:1194`` in Textual 8.2.8) — passing
``open_links=False`` stops the widget from acting on its own message, but the
message still fires and a handler mounted anywhere else in the app tree could
still act on it. And it renders an image as its alt text with the target
silently dropped (``:346``) rather than showing where the image pointed. Both
are rendering-hook defects, invisible from the parser configuration alone, so
:class:`EntryMarkdown` overrides the one method that installs them
(``MarkdownBlock._token_to_content``) rather than trusting
``parser_factory=…, open_links=False`` to be sufficient by themselves.

**The four forbidden channels, and what closes each one:**

1. **HTML.** ``html=False`` on the parser (not a disabled rule — markdown-it
   has no separate "html" rule to disable; the ``html_block``/``html_inline``
   rules stay active and the option makes them no-ops, so ``<script>…`` is
   tokenized as literal ``text`` and rendered as visible characters, never
   dropped — R10).
2. **Links.** Rendered styled (:data:`LINK_STYLE`, an explicit underline) but
   with no ``@click`` metadata at all, so clicking posts no
   ``Markdown.LinkClicked`` message and nothing is opened.
3. **Images.** Rendered as literal ``"alt (target)"`` text; nothing is
   fetched and no click action is installed either (R10).
4. **Bare URLs.** ``linkify=False`` keeps a bare URL as the plain ``text``
   token it already was — no autolink token is created for the rendering hook
   to have had to defuse.

**Defanging runs before parsing, at parity with today's boundary
(``talaria/ui/literal.py``).** :class:`EntryMarkdown` calls
:func:`~talaria.ui.literal.defang` on every string handed to it — at
construction, on :meth:`~EntryMarkdown.update`, and on
:meth:`~EntryMarkdown.append` — so a C0 control, a bidi override, or a
zero-width character never reaches the parser at all; it arrives as the same
visible control picture every other renderer in this package would show.
Rich console markup is never parsed over gateway bytes either, for a reason
that needs no code here to enforce: Textual's ``Markdown`` widget builds its
output from parser tokens through :class:`textual.content.Content`, never by
feeding a string through Rich's console-markup parser.

**Isolation is structural, not a property this module computes (R15).** Each
entry gets its own :class:`EntryMarkdown` instance, and
:meth:`~EntryMarkdown.update`/:meth:`~EntryMarkdown.append` each build a fresh
parser from :func:`parser_factory` — nothing in this module keeps parser state
across calls or across instances, so an unclosed fence in one entry has no
shared buffer through which it could absorb the next.

**RA1 — underscore emphasis and strikethrough are allowed, on purpose.**
markdown-it's ``emphasis`` rule does not distinguish ``*word*`` from
``_word_`` — disabling one disables both — and disabling ``strikethrough``
outright would mean GitHub-Flavoured Markdown's ``~~text~~`` staple never
renders. Both stay in the allowlist :func:`parser_factory` pins.
"""

from __future__ import annotations

import re
from typing import Final

from markdown_it import MarkdownIt
from markdown_it.token import Token
from textual.await_complete import AwaitComplete
from textual.content import Content, Span
from textual.style import Style
from textual.widgets import Markdown
from textual.widgets.markdown import MarkdownBlock

from talaria.ui.literal import defang

# ── the parser: gfm-like, minus HTML and linkify (KTD4, RA1) ───────────────

#: The exact rule set :func:`parser_factory` pins its parser to, one tuple per
#: markdown-it ruler. This *is* the R9 allowlist as amended by RA1 — every
#: name here is a construct that can reach the screen, and the only path by
#: which anything else could is a change to markdown-it's "gfm-like" preset
#: that :func:`parser_factory` failed to notice.
#:
#: ``html``/``linkify`` are **options**, not rule names — markdown-it has no
#: rule called "html" to disable, so turning HTML off cannot be read off this
#: table; it is why :func:`parser_factory` also asserts the options directly
#: rather than trusting an unchanged rule set to imply them.
RULE_SET: Final[dict[str, tuple[str, ...]]] = {
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


def enabled_rules(parser: MarkdownIt) -> dict[str, tuple[str, ...]]:
    """The exact rule set active in ``parser``, one sorted tuple per ruler.

    Reads the parser's own rulers (``core``, ``block``, ``inline``, and
    inline's second pass ``inline2``) rather than trusting any cached
    description of them, so a preset change upstream is observed directly.
    """
    return {
        "core": tuple(sorted(parser.core.ruler.get_active_rules())),
        "block": tuple(sorted(parser.block.ruler.get_active_rules())),
        "inline": tuple(sorted(parser.inline.ruler.get_active_rules())),
        "inline2": tuple(sorted(parser.inline.ruler2.get_active_rules())),
    }


def parser_factory() -> MarkdownIt:
    """Build the one parser configuration every entry document parses with.

    Returns ``MarkdownIt("gfm-like")`` reconfigured with ``html=False`` and
    ``linkify=False``. Raises loudly rather than returning a parser that
    silently allows more than :data:`RULE_SET` names, or one whose ``html``/
    ``linkify`` options came back on — either is a markdown-it upgrade this
    module has not been reviewed against, and ADR-0006 says a parser upgrade
    is the kind of change that updates the pin deliberately, not one that
    passes by accident.
    """
    parser = MarkdownIt("gfm-like", {"html": False, "linkify": False})
    observed = enabled_rules(parser)
    if observed != RULE_SET:
        raise RuntimeError(  # nosec B608 - no SQL here; bandit's f-string
            # heuristic misfires on the word "Update" a few lines down.
            "markdown-it's 'gfm-like' preset no longer matches the pinned R9/RA1 "
            f"allowlist: expected {RULE_SET!r}, got {observed!r}. Revise ADR-0006 "
            "and this module's RULE_SET deliberately if the new rule should be "
            "allowed; do not silently pass a parser with an unreviewed rule active."
        )
    if parser.options["html"] or parser.options["linkify"]:
        raise RuntimeError(
            "parser_factory built a parser with 'html' or 'linkify' still enabled"
        )
    return parser


# ── rendering hooks: links styled but inert, images as text (KTD4, R10) ────

#: Applied to a rendered link's body in place of the stock widget's
#: ``@click`` action metadata. Worth stating why a style is needed at all:
#: Textual only applies its own automatic ``link_style`` underline to spans
#: whose meta carries an ``"@click"`` key (``textual/widget.py``, the
#: ``_RenderStyled.__rich_console__`` check), so removing that metadata to
#: make the link inert also silently removes its only visual distinction.
#: This restores one without going through the click machinery that would
#: also restore the action.
LINK_STYLE: Final[Style] = Style(underline=True)

#: Collapses runs of whitespace the same way the stock widget's
#: ``_token_to_content`` does, kept identical so plain prose renders
#: byte-for-byte the same as it would through the un-subclassed widget.
_COLLAPSE_WHITESPACE: Final[re.Pattern[str]] = re.compile(r"\s+")


class _InertContentMixin(MarkdownBlock):
    """Overrides the one method that installs link/image action metadata.

    Textual's ``MarkdownBlock._token_to_content`` (``_markdown.py:281``) is
    the single place every block type — paragraph, heading, list item, table
    cell — turns an ``inline`` token's children into drawable
    :class:`~textual.content.Content`. This mixin re-walks the same children
    and changes exactly two of the existing branches (``link_open`` and
    ``image``); every other branch is unchanged from the stock method. That
    is the whole of R4's "subclassed rendering hooks, no parsing or block
    assembly": this class does not touch ``MarkdownIt``, tokens, or how
    blocks are built from them — only how one already-built token's children
    become styled text.
    """

    def _token_to_content(self, token: Token) -> Content:
        if token.children is None:
            return Content("")

        tokens: list[str] = []
        spans: list[Span] = []
        style_stack: list[tuple[Style | str, int]] = []
        position = 0

        def add_content(text: str) -> None:
            nonlocal position
            tokens.append(text)
            position += len(text)

        def add_style(style: Style | str) -> None:
            style_stack.append((style, position))

        def close_tag() -> None:
            style, start = style_stack.pop()
            spans.append(Span(start, position, style))

        for child in token.children:
            child_type = child.type
            if child_type == "text":
                add_content(_COLLAPSE_WHITESPACE.sub(" ", child.content))
            elif child_type == "hardbreak":
                add_content("\n")
            elif child_type == "softbreak":
                add_content(" ")
            elif child_type == "code_inline":
                add_style(".code_inline")
                add_content(child.content)
                close_tag()
            elif child_type == "em_open":
                add_style(".em")
            elif child_type == "strong_open":
                add_style(".strong")
            elif child_type == "s_open":
                add_style(".s")
            elif child_type == "link_open":
                # R10/KTD4: styled, never actionable. No "@click" meta means
                # no `Markdown.LinkClicked` is ever posted for this span, and
                # nothing this app or Textual itself does on click.
                add_style(LINK_STYLE)
            elif child_type == "image":
                # R10: alt text *and* target render as text; nothing is
                # fetched, and — unlike the stock method, which installs a
                # click action and then drops the target — no action is
                # installed either. `child.content` (not `attrs["alt"]`,
                # which markdown-it leaves empty on an `image` token) is
                # where the flattened alt text actually lives.
                href = str(child.attrs.get("src", ""))
                alt = child.content or str(child.attrs.get("alt", ""))
                add_content(f"{alt} ({href})" if alt else f"({href})")
            elif child_type.endswith("_close"):
                close_tag()

        return Content("".join(tokens), spans=spans)


def _safe_block_classes() -> dict[str, type[MarkdownBlock]]:
    """Every stock block class, with :class:`_InertContentMixin` ahead of it.

    Built from ``Markdown.BLOCKS`` itself — the live mapping token names use
    to select a widget class — rather than importing each class by name from
    the private ``textual.widgets._markdown`` module, so this stays correct
    if Textual renames or reorganizes a block class; a class Textual adds or
    removes entirely is exactly what the Textual-version pin test in
    ``tests/ui/test_blocks.py`` exists to notice, not something this function
    should paper over.
    """
    wrapped: dict[type[MarkdownBlock], type[MarkdownBlock]] = {}
    safe: dict[str, type[MarkdownBlock]] = {}
    for name, block_cls in Markdown.BLOCKS.items():
        if block_cls not in wrapped:
            wrapped[block_cls] = type(
                f"Inert{block_cls.__name__}", (_InertContentMixin, block_cls), {}
            )
        safe[name] = wrapped[block_cls]
    return safe


class EntryMarkdown(Markdown):
    """The single constructor for a safely configured entry document (U3).

    Every caller that wants to render agent-authored markdown as a block
    document builds one of these instead of a plain
    ``textual.widgets.Markdown`` — the difference is entirely the four
    channels this module's docstring names; streaming (``append``),
    replacement (``update``), and layout are exactly the stock widget's.
    """

    #: Every entry document parses through the inert rendering hooks, keyed
    #: exactly as the stock widget keys its own ``BLOCKS``.
    BLOCKS = _safe_block_classes()

    def __init__(
        self,
        markdown: str = "",
        *,
        name: str | None = None,
        id: str | None = None,
        classes: str | None = None,
    ) -> None:
        super().__init__(
            defang(markdown),
            name=name,
            id=id,
            classes=classes,
            parser_factory=parser_factory,
            open_links=False,
        )

    def update(self, markdown: str) -> AwaitComplete:
        """Replace the document's content, defanged before it is parsed."""
        return super().update(defang(markdown))

    def append(self, markdown: str) -> AwaitComplete:
        """Append a fragment, defanged before it is parsed."""
        return super().append(defang(markdown))
