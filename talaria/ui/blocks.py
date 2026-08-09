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

**RA2 — table columns get an explicit bounded-fractional width instead of
Textual's stock ``grid-columns: auto``.** Content-driven auto layout starves
columns unpredictably once more than a couple of them compete for the same
row (a live five-column, 80-column-wide probe assigned data widths
``[0, 0, 58, 1, 1]``). :func:`_column_content_widths` and
:class:`_BoundedMarkdownTableContent` compute and set each column's width
explicitly, and drop the stock cell's tooltip-and-ellipsis truncation in
favor of wrapping, so every cell's content is legible at 80 columns without
a mouse — RA2's amendment to R3.
"""

from __future__ import annotations

import re
from collections.abc import Sequence
from fractions import Fraction
from typing import Final

from markdown_it import MarkdownIt
from markdown_it.token import Token
from rich.cells import cell_len
from textual.app import ComposeResult
from textual.await_complete import AwaitComplete
from textual.content import Content, Span
from textual.geometry import Size
from textual.layout import DockArrangeResult
from textual.style import Style
from textual.widgets import Markdown
from textual.widgets._markdown import (
    MarkdownTable,
    MarkdownTableCellContents,
    MarkdownTableContent,
)
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


def _flatten_alt_text(children: Sequence[Token] | None) -> str:
    """Flattens an image token's parsed alt-text children into plain text.

    An image token's ``content`` attribute is the raw, *unparsed* alt-text
    source — ``![*a* cat](url)`` carries ``"*a* cat"`` there, asterisks
    included — while ``children`` is the same alt text markdown-it already
    parsed into inline tokens (``text``, ``em_open``/``em_close``, and so
    on), the same shape a paragraph's own children have. This walks that
    parsed structure and keeps only the textual leaves, dropping every
    styling open/close marker, so bold or italic *inside* alt text renders
    as its plain characters rather than leaking literal markup punctuation
    (R10: alt text renders as text, not as re-parsed markdown).
    """
    if not children:
        return ""
    parts: list[str] = []
    for child in children:
        if child.type == "text":
            parts.append(_COLLAPSE_WHITESPACE.sub(" ", child.content))
        elif child.type == "code_inline":
            parts.append(child.content)
        elif child.type == "hardbreak":
            parts.append("\n")
        elif child.type == "softbreak":
            parts.append(" ")
        # em_open/strong_open/s_open/link_open and their *_close carry no
        # text of their own: alt text is plain text, not styled text, so
        # their markers contribute nothing and are silently skipped.
    return "".join(parts)


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
                # installed either. `attrs["alt"]` is always empty on an
                # `image` token (markdown-it leaves it that way); the alt
                # text markdown-it actually parsed lives in `child.children`
                # and is flattened by :func:`_flatten_alt_text` the same
                # way a paragraph's own children would be. `child.content`
                # is the raw, *unparsed* alt-text source — asterisks,
                # underscores, and all — and using it directly (the former
                # bug here) let inline formatting inside alt text leak as
                # literal markup punctuation instead of plain text.
                href = str(child.attrs.get("src", ""))
                alt = _flatten_alt_text(child.children)
                add_content(f"{alt} ({href})" if alt else f"({href})")
            elif child_type.endswith("_close"):
                close_tag()

        return Content("".join(tokens), spans=spans)


# ── RA2: bounded-fractional table column widths ────────────────────────────

#: RA2's amendment to R3: table content must be legible at 80 columns
#: without a mouse. The stock ``MarkdownTableContent`` trusts
#: ``grid-columns: auto``, which sizes each column from its own widest
#: rendered cell with no regard for how many columns are competing for the
#: same row — a live probe of a five-column, 80-column-wide table assigned
#: data widths ``[0, 0, 58, 1, 1]``, starving four columns down to nothing.
#: The functions and class below compute an explicit per-column width
#: instead: a per-column minimum capped so no single wide column can starve
#: its neighbors, with any leftover space handed out proportionally to each
#: column's own content length.


def _largest_remainder_shares(remainder: int, weights: Sequence[int]) -> list[int]:
    """Distributes ``remainder`` whole cells across ``len(weights)`` columns
    proportionally to ``weights`` by the largest-remainder method, ties
    broken to the leftmost column — deterministic integer arithmetic, no
    float residue (RA2). When every weight is zero (a table whose every
    cell is empty is valid and probed to mount), the split is equal instead,
    with the same leftmost-column tie rule.
    """
    n = len(weights)
    if n == 0 or remainder <= 0:
        return [0] * n
    total = sum(weights)
    if total == 0:
        base, extra = divmod(remainder, n)
        return [base + (1 if i < extra else 0) for i in range(n)]

    quotas = [Fraction(remainder * weight, total) for weight in weights]
    floors = [quota.numerator // quota.denominator for quota in quotas]
    leftover = remainder - sum(floors)
    fractional_parts = [
        quota - floor_ for quota, floor_ in zip(quotas, floors, strict=True)
    ]
    # Largest fractional remainder first; a tie keeps the lower index first,
    # which `sorted`'s stability alone would not guarantee against floating
    # point, but does here since the key is the exact `Fraction` itself.
    order = sorted(range(n), key=lambda i: (-fractional_parts[i], i))
    shares = floors[:]
    for i in order[:leftover]:
        shares[i] += 1
    return shares


def _column_content_widths(
    width: int, headers: Sequence[str], rows: Sequence[Sequence[str]]
) -> list[int]:
    """RA2's bounded-fractional column-width algorithm, exactly as specified.

    ``width`` is the table content box's own resolved width in cells (not
    the pane width — the enclosing ``Markdown`` document's own padding is
    already excluded by the time a ``MarkdownTableContent`` widget is
    arranged). For ``n`` columns, ``available = width - (3n + 1)`` cells
    remain for cell text once the per-cell padding (one cell each side, two
    per column) and one grid gutter per column boundary are subtracted.
    Each column's minimum is its longest single word, capped at
    ``max(8, available // n)``. If every minimum fits inside ``available``,
    the leftover space is handed out by the largest-remainder method,
    weighted by each column's total content length. Otherwise every column
    gets the equal split ``available // n`` with a hard floor of 3 cells,
    and a word wider than its column character-wraps at render time.

    Returns the *content* width per column — the width available to a
    cell's text, not the column's total width on screen (the caller adds
    the 2-cell padding back before handing this to ``grid-columns``).
    """
    n = len(headers)
    if n == 0:
        return []
    columns: list[list[str]] = [[headers[i]] for i in range(n)]
    for row in rows:
        for i, cell in enumerate(row):
            if i < n:
                columns[i].append(cell)

    longest_word = [
        max((cell_len(word) for text in texts for word in text.split()), default=0)
        for texts in columns
    ]
    total_content_len = [sum(cell_len(text) for text in texts) for texts in columns]

    available = width - (3 * n + 1)
    cap = max(8, available // n)
    minimums = [min(word, cap) for word in longest_word]

    if sum(minimums) <= available:
        remainder = available - sum(minimums)
        shares = _largest_remainder_shares(remainder, total_content_len)
        return [minimum + share for minimum, share in zip(minimums, shares, strict=True)]

    equal_split = available // n
    return [max(3, equal_split) for _ in range(n)]


class _BoundedMarkdownTableContent(MarkdownTableContent):
    """RA2's column widths, computed and set explicitly rather than trusted
    to the stock widget's ``grid-columns: auto``.

    Two changes from the stock ``MarkdownTableContent``: :meth:`compose`
    drops ``.with_tooltip(...)`` from every cell and sets each cell's
    ``text-overflow`` to ``"fold"`` (character-wrap, never truncate) instead
    of the stock ``"ellipsis"`` — RA2's amended R3 reads "no construct
    depends on hover or focus to show its content", so a cell's *only* path
    to full legibility is wrapping, never a tooltip a mouse would have to
    open. :meth:`arrange` is the actual RA2 arithmetic: it is called with
    this widget's own resolved content-box size before Textual's grid
    layout consumes ``styles.grid_columns``, which is exactly the point
    :func:`_column_content_widths` needs to measure ``width`` from — so the
    widths are computed and assigned there, immediately before delegating
    to the stock layout.
    """

    def compose(self) -> ComposeResult:
        for header in self.headers:
            header_cell = MarkdownTableCellContents(header, classes="header")
            header_cell.styles.text_overflow = "fold"
            yield header_cell
        for row_index, row in enumerate(self.rows, 1):
            for cell_index, cell in enumerate(row, 1):
                data_cell = MarkdownTableCellContents(
                    cell,
                    classes=f"row{row_index} cell",
                    name=f"cell{row_index}.{cell_index}",
                )
                data_cell.styles.text_overflow = "fold"
                yield data_cell
            self.last_row = row_index

    def arrange(self, size: Size, optimal: bool = False) -> DockArrangeResult:
        if self.headers:
            content_widths = _column_content_widths(
                size.width,
                [header.plain for header in self.headers],
                [[cell.plain for cell in row] for row in self.rows],
            )
            # +2: the padding each cell keeps (one cell each side) is part
            # of the grid column's own width, not part of the content width
            # the algorithm above solved for.
            self.styles.grid_columns = [width + 2 for width in content_widths]
        return super().arrange(size, optimal)


def _bounded_table_compose(self: MarkdownTable) -> ComposeResult:
    """Replaces the stock ``MarkdownTable.compose`` to mount
    :class:`_BoundedMarkdownTableContent` in place of the stock
    ``MarkdownTableContent`` (RA2). Otherwise identical to Textual's own
    ``MarkdownTable.compose`` (``_markdown.py:735``) — this function's only
    job is the substitution, not the header/row extraction it delegates to
    ``self._get_headers_and_rows()`` for unchanged.
    """
    headers, rows = self._get_headers_and_rows()
    self._headers = headers
    self._rows = rows
    yield _BoundedMarkdownTableContent(headers, rows)


def _safe_block_classes() -> dict[str, type[MarkdownBlock]]:
    """Every stock block class, single-inheriting from itself with the one
    method patched onto its own class dict (U4 fix).

    Built from ``Markdown.BLOCKS`` itself — the live mapping token names use
    to select a widget class — rather than importing every class by name
    from the private ``textual.widgets._markdown`` module, so this stays
    correct if Textual renames or reorganizes a block class; a class Textual
    adds or removes entirely is exactly what the Textual-version pin test in
    ``tests/ui/test_blocks.py`` exists to notice, not something this function
    should paper over. ``MarkdownTable`` is the one class this module does
    import by name — RA2's table override (:func:`_bounded_table_compose`)
    has to be wired onto that specific class, not onto whichever one
    ``Markdown.BLOCKS["table_open"]`` happens to be, so the ``block_cls is
    MarkdownTable`` check below needs the class as a value to compare
    against, not only as something discovered generically.

    **Not** ``type(name, (_InertContentMixin, block_cls), {})`` — that was
    U3's original shape, and it crashes on every table (found live while
    building U4). Textual's ``DOMNode._css_bases`` derives a class's CSS type
    chain by walking ``__bases__`` and, at each step, following the *first*
    base that is itself a ``DOMNode`` subclass
    (``textual/dom.py:647-667``). With the mixin listed first, that walk
    picks ``_InertContentMixin`` — whose own base is plain ``MarkdownBlock``
    — and never reaches ``block_cls`` at all, so a wrapped
    ``MarkdownTableContent`` loses every construct-specific CSS type name
    including ``"MarkdownTable"``. ``MarkdownTableContent.pre_layout`` calls
    ``self.query_ancestor(MarkdownTable)`` (``_markdown.py:664``), which
    resolves the type to that same string and raises ``NoMatches`` the
    instant a table is laid out — probed live against Textual 8.2.8,
    traceback confirmed. Subclassing ``block_cls`` directly and patching
    ``_token_to_content`` straight into the new class's own ``__dict__``
    keeps ``__bases__ == (block_cls,)``, so ``_css_bases`` walks the real
    chain (``MarkdownTable`` → ``MarkdownBlock`` → …) and every
    construct-specific selector Textual's own layout code depends on still
    matches. The class-dict assignment (not inheritance order) is what makes
    the override win regardless: ``type(widget)`` is checked before any base
    in an attribute lookup, so putting the method directly in the leaf
    class's ``__dict__`` doesn't depend on getting mixin order right a
    second time.
    """
    wrapped: dict[type[MarkdownBlock], type[MarkdownBlock]] = {}
    safe: dict[str, type[MarkdownBlock]] = {}
    for name, block_cls in Markdown.BLOCKS.items():
        if block_cls not in wrapped:
            overrides: dict[str, object] = {
                "_token_to_content": _InertContentMixin._token_to_content,
            }
            if block_cls is MarkdownTable:
                # RA2: mount `_BoundedMarkdownTableContent` (bounded-
                # fractional column widths) instead of the stock,
                # auto-layout table content.
                overrides["compose"] = _bounded_table_compose
            wrapped[block_cls] = type(block_cls.__name__, (block_cls,), overrides)
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
        """Replace the document's content, defanged before it is parsed.

        **Corrects Textual 8.2.8's ``_last_parsed_line`` seeding defect
        (KTD3), immediately after every ``update()`` completes.** Textual's
        own ``Markdown.update`` — the same code path ``_on_mount`` calls to
        seed a widget's *initial* content, so this covers construction too —
        sets the private append checkpoint ``_last_parsed_line`` from "total
        source lines minus one, if the last line is non-empty"
        (``_markdown.py:1435-1436``). That is correct only when the
        document's last construct happens to be exactly one line long at
        the moment ``update()`` runs. A live tail first mounted with a
        multi-line table or fence already in it — the *only* way a table
        can become block-eligible at all, since markdown-it needs a header,
        a separator, and at least one row before it recognizes one — is
        seeded through exactly this path, with the checkpoint left pointing
        partway *into* the construct rather than at its start. The next
        :meth:`append` then reparses a window with no header or
        fence-opening delimiter in view, which markdown-it parses as a bare
        paragraph, and Textual silently swaps the mounted table or fence for
        it (reproduced against a bare, unwrapped ``textual.widgets.Markdown``
        too — not something this module introduced).

        :meth:`_correct_last_parsed_line` fixes the checkpoint using the
        identical technique Textual's own :meth:`append` already uses to
        correct *itself* after every reparse (``_markdown.py:1472-1476``):
        parse the full source with the same parser this document renders
        with, then walk the tokens in reverse for the first one at
        ``level == 0`` carrying a source ``map`` — the last top-level
        block's own start line, whatever that block is, open or already
        closed. This is what keeps the incremental ``append`` path intact
        rather than forcing an ``update()`` on every write: the seed is
        fixed once, here, so every subsequent ``append`` reparses from the
        true block boundary the way it always should have.
        """
        clean = defang(markdown)
        parent_update = super().update(clean)

        async def _update_then_correct() -> None:
            await parent_update
            self._correct_last_parsed_line(clean)

        return AwaitComplete(_update_then_correct())

    def _correct_last_parsed_line(self, markdown: str) -> None:
        """Recomputes ``_last_parsed_line`` from the real last top-level
        block's own start line (see :meth:`update`'s docstring for why this
        is necessary and the mechanism this mirrors).

        A source with no top-level block carrying a ``map`` at all (empty,
        or whitespace-only) leaves Textual's own computed value alone —
        there is no better answer than what it already set, and
        :data:`~talaria.ui.transcript.is_zero_block` sources never take this
        path as a block document in the first place.

        This touches a private Textual attribute deliberately, per
        ADR-0006's KTD3 append/update contract — guarded by the Textual
        8.2.8 pin test in ``tests/ui/test_blocks.py``, which pins both this
        attribute's existence and Textual's own uncorrected seeding
        behavior, so an upgrade that changes either fails loudly rather than
        silently leaving this correction wired to a fact that stopped being
        true.
        """
        tokens = parser_factory().parse(markdown)
        for token in reversed(tokens):
            if token.level == 0 and token.map is not None:
                self._last_parsed_line = token.map[0]
                return

    def append(self, markdown: str) -> AwaitComplete:
        """Append a fragment, defanged before it is parsed.

        No correction needed here: Textual's own :meth:`Markdown.append`
        already recomputes ``_last_parsed_line`` correctly after every
        reparse (the same technique :meth:`_correct_last_parsed_line`
        mirrors) — the defect lives only in how the checkpoint gets seeded
        by :meth:`update`, never in how ``append`` maintains it afterward.
        """
        return super().append(defang(markdown))
