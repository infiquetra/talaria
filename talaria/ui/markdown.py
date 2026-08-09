"""Inline markdown for transcript prose: emphasis and code spans, nothing else.

**Retired from the live rendering path by U4 (KTD8).** ``TranscriptPane``
(``talaria/ui/transcript.py``) no longer calls :func:`inline_markdown` for
assistant or reasoning entries — those are block-rendered through
``talaria/ui/blocks.py`` (U3) instead, and ``MARKDOWN_KINDS`` (the set of
kinds eligible for that) now lives on ``transcript.py``, which is where the
"line widget or block document" decision is made. This module is kept in
the tree, unmodified in its parsing behaviour, as the styling half of KTD8's
branch-held fallback: if the restated gate (U6) comes back red and the
operator invokes the fallback rather than iterating further, the fallback
reintroduces this module as a live caller — so it stays correct and tested
rather than being deleted and rewritten under pressure later. Its
pane-level assertions (what a mounted ``TranscriptLine`` shows) moved to
``tests/ui/test_transcript_blocks.py``'s reconstruction checks; this module
keeps only its own parser-level tests, unmodified.

R6 puts markdown presentation out of scope for v0.1, and this module is a
deliberate, narrow amendment to it rather than an oversight being corrected. The
part being amended is presentational only; the part R6 actually guarantees —
that no content is dropped — is untouched, because nothing here runs before the
projection. :class:`~talaria.domain.projection.TranscriptView` still publishes
the agent's bytes exactly as they arrived, the terminal-read buffer still serves
them (KTD10), and the recording still holds them. What changes is which cells
the terminal paints.

**Inline, and only inline, because a line is the transcript's unit.**
:class:`~talaria.ui.transcript.TranscriptPane` mounts one widget per line and
bounds itself by counting those widgets (KTD14), diffs by stable line prefix,
and holds a reader's scroll position by subtracting evicted widget *heights*. A
heading or a fenced block is one renderable spanning many lines, so it breaks
all four of those at once. Everything in this module is resolvable from a single
line in isolation, which is what makes it free. The rest is a real feature with
a real cost, and it is queued as one.

**Order of operations, and why it is not the other order.** Code spans are
extracted first and emphasis is matched afterwards, over a skeleton in which
each code span has been replaced by a single placeholder character. That
ordering is a correctness choice: asterisks *inside* a code span must stay on
screen, because a code span is the one construct here whose visible characters
are meant to be read literally, and emphasis *around* a code span is cosmetic.
Getting the priority backwards would trade a cosmetic gap for a wrong rendering
of code.

**What is deliberately not implemented**, so the next reader does not mistake
absence for oversight:

* ``_underscore_`` and ``__underscore__`` emphasis. CommonMark renders
  ``__init__.py`` as a bold ``init``, and mangling a Python identifier on a
  client whose stated principle is that the rendered path is the real path is a
  worse defect than the one this module fixes. Underscores are left alone.
* Links, images, strikethrough, autolinks, and HTML. Each either rewrites text
  the operator needs verbatim or has no meaning in a terminal transcript.
* CommonMark's full delimiter-run algorithm. The flanking rules here are the
  whitespace clauses only — an opener may not be followed by a space, a closer
  may not be preceded by one — which is what keeps ``*args, **kwargs`` and
  ``2 * 3 * 4`` unstyled. The punctuation clauses and the rule of three affect
  cases that do not appear in agent prose.
"""

from __future__ import annotations

import re
from typing import Final

from rich.text import Span, Text

from talaria.ui.literal import defang

#: Rich style for ``**strong**``.
STRONG_STYLE: Final[str] = "bold"

#: Rich style for ``*emphasis*``.
EMPHASIS_STYLE: Final[str] = "italic"

#: Rich style for a backtick code span.
#:
#: Rich's own ``markdown.code`` is ``bold cyan on black``; the background is
#: dropped on purpose. Talaria's panes have a themed background, and painting a
#: hard black box behind every code span would make the transcript look striped
#: on any theme that is not black — including the light ones.
CODE_STYLE: Final[str] = "bold cyan"

#: Stands in for a code span while emphasis is matched, so that a span can be
#: found *around* a code span without the code span's own characters being
#: eligible for matching.
#:
#: NUL is safe here precisely because :func:`~talaria.ui.literal.defang`
#: translates it to a control picture: after defanging, this character cannot
#: appear in the text, so nothing the gateway sends can forge a placeholder. A
#: test pins that property rather than trusting this comment.
_PLACEHOLDER: Final[str] = "\x00"

#: A run of backticks, the shortest body that reaches a matching run, and the
#: closing run. The backreference is what makes ``` ``a`b`` ``` one span rather
#: than two. An unpaired backtick matches nothing and stays on screen.
_CODE_SPAN: Final[re.Pattern[str]] = re.compile(r"(?P<fence>`+)(?P<body>.+?)(?P=fence)")

#: ``**strong**``. The closer may not be followed by a third asterisk, so
#: ``***both***`` yields a strong body of ``*both*`` that the emphasis pass then
#: italicises, rather than a bold ``*both`` with an orphaned asterisk trailing it.
_STRONG: Final[re.Pattern[str]] = re.compile(r"\*\*(?=\S)(?P<body>.+?)(?<=\S)\*\*(?!\*)")

#: ``*emphasis*``. The body excludes asterisks so this pass cannot reach across
#: a strong delimiter that the pass before it declined to match.
_EMPHASIS: Final[re.Pattern[str]] = re.compile(r"\*(?=\S)(?P<body>[^*]+?)(?<=\S)\*")

#: The characters without which no rule here can fire. Checked first so the
#: overwhelming majority of transcript lines skip every regex in this module.
_MARKERS: Final[str] = "*`"


class _Sink:
    """Accumulates the drawn text and the style spans over it, left to right.

    Built as an append-only buffer rather than by rewriting a string, because a
    span is a pair of offsets into the *output* and the output is shorter than
    the input by exactly the delimiters that were consumed. Tracking the length
    as text is appended is what keeps those offsets honest.
    """

    __slots__ = ("_bodies", "_length", "_parts", "spans")

    def __init__(self, bodies: list[str]) -> None:
        self._parts: list[str] = []
        self._length = 0
        self._bodies = iter(bodies)
        self.spans: list[Span] = []

    def emit(self, value: str, styles: tuple[str, ...]) -> None:
        """Append ``value``, expanding any code-span placeholder it contains.

        Placeholders are refilled in first-in order, which is sound because both
        the scan that created them and every walker below proceed strictly left
        to right.
        """
        if not value:
            return
        if _PLACEHOLDER not in value:
            self._append(value, styles)
            return
        for index, piece in enumerate(value.split(_PLACEHOLDER)):
            if index:
                self._append(next(self._bodies), (*styles, CODE_STYLE))
            self._append(piece, styles)

    def _append(self, value: str, styles: tuple[str, ...]) -> None:
        if not value:
            return
        start = self._length
        self._parts.append(value)
        self._length += len(value)
        self.spans.extend(Span(start, self._length, style) for style in styles)

    def text(self) -> Text:
        return Text("".join(self._parts), spans=self.spans, no_wrap=False, end="")


def _emphasis(segment: str, styles: tuple[str, ...], sink: _Sink) -> None:
    index = 0
    for match in _EMPHASIS.finditer(segment):
        sink.emit(segment[index : match.start()], styles)
        sink.emit(match["body"], (*styles, EMPHASIS_STYLE))
        index = match.end()
    sink.emit(segment[index:], styles)


def _strong(segment: str, styles: tuple[str, ...], sink: _Sink) -> None:
    index = 0
    for match in _STRONG.finditer(segment):
        _emphasis(segment[index : match.start()], styles, sink)
        _emphasis(match["body"], (*styles, STRONG_STYLE), sink)
        index = match.end()
    _emphasis(segment[index:], styles, sink)


def inline_markdown(value: str) -> Text:
    """Defang ``value``, then style its inline markdown and drop the delimiters.

    The defanging is not an extra step bolted on for safety; it is the same
    single call every other renderer in this package makes, and it happens
    *first* so that the string this function indexes into is exactly the string
    the terminal will draw. Nothing here parses Rich console markup — the result
    is a :class:`~rich.text.Text` with explicit spans, so a ``[red]`` in agent
    prose is still four visible characters.

    Only the delimiter characters of a matched construct are removed. Every
    other character survives, in order, which is the property that keeps this
    compatible with R6's no-content-dropped clause and is asserted directly by
    the suite.
    """
    text = defang(value)
    if not any(marker in text for marker in _MARKERS):
        # The common case, and identical to what ``literal_text`` would build.
        return Text(text, no_wrap=False, end="")

    bodies: list[str] = []

    def hide(match: re.Match[str]) -> str:
        bodies.append(match["body"])
        return _PLACEHOLDER

    skeleton = _CODE_SPAN.sub(hide, text)
    sink = _Sink(bodies)
    _strong(skeleton, (), sink)
    return sink.text()
