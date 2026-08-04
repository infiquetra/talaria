"""Render arbitrary text as *literal* text, with nothing interpreted.

Four separate interpreters sit between a string and a terminal cell, and each
one is a way for content Talaria did not author to change the screen:

1. **Rich console markup.** ``Static("[red]x[/]")`` would colour the text.
   Passing a :class:`rich.text.Text` instead of a ``str`` bypasses the markup
   parser entirely, which is why every renderer in this package builds one.
2. **ANSI escape sequences.** A status command that prints ``\\x1b[2J`` would
   clear the screen; one that prints ``\\x1b]0;…\\x07`` would retitle the
   terminal window. R22 requires these be *shown*, not obeyed, so the escape
   character is replaced with its visible symbol.
3. **Other C0 controls.** A stray carriage return re-homes the cursor mid-line
   and a bell audibly fires. Same treatment: shown, not obeyed.
4. **The terminal's own Unicode bidirectional algorithm, and the characters
   that occupy no cells at all.** This is the interpreter that has no escape
   character to look for, and it is why the list above used to be three items
   long. A terminal reorders glyphs on its own whenever the text contains a
   bidi override or isolate (U+202A–U+202E, U+2066–U+2069), so a string whose
   bytes read ``rm -rf /`` can be drawn as something harmless — the Trojan
   Source mechanism. A zero-width character (U+200B, U+200D, U+FEFF and
   friends) is the same attack with the opposite move: it occupies no cells, so
   ``rm -rf /home/build`` with one hidden inside is byte-for-byte a different
   command from the one the operator reads and is pixel-for-pixel the same
   picture. *Pixel-for-pixel the same picture* is the operative half, and it is
   what the emoji presentation selectors do not do — see
   :data:`PRESENTATION_SELECTORS`. **The bidirectional set is the smaller half
   of this.** Unicode's
   whole ``Cf`` category draws nothing, and the part of it that matters here is
   the Tag block (U+E0020–U+E007F): a full invisible copy of ASCII, and the
   current standard carrier for text hidden inside a string meant for a
   language model — which on this surface means a command whose visible half
   and whose executed half differ.

   **This became reachable when the approval card started rendering the
   command.** While the command was never on screen there was nothing to
   deceive; now :func:`~talaria.ui.prompts.wrap_command` and
   :func:`~talaria.domain.state.prompt_registration_line` both put a
   gateway-supplied command in front of a human who is about to grant it, and
   the rendered path has to be the executed path.

Tab is kept as a real tab (a status command aligning columns with tabs is doing
something legitimate) and newline is kept because the caller has already split
on it.
"""

from __future__ import annotations

from typing import Final

from rich.text import Text

#: Visible stand-ins for the control characters that would otherwise be obeyed.
#: Unicode's "control pictures" block exists for exactly this, and using it
#: keeps the substitution one cell wide so column alignment survives.
_CONTROL_PICTURES: dict[int, str] = {
    0x1B: "␛",
    0x07: "␇",
    0x08: "␈",
    0x0B: "␋",
    0x0C: "␌",
    0x0D: "␍",
    0x00: "␀",
}

#: What a bidi or zero-width character is replaced with.
#:
#: U+FFFD rather than another control picture, and one cell rather than a
#: spelled-out ``<U+202E>``. A distinct glyph because these are a different
#: failure from a C0 control — a C0 control would have been *obeyed*, while
#: these would have been *rendered*, correctly, into a different picture than
#: the bytes describe. One cell because the substitution has to keep the
#: column arithmetic in :func:`~talaria.ui.prompts.wrap_command` agreeing with
#: what the terminal draws, and because a marker that reflows the line is a
#: second way to make the rendered command differ from the real one.
INVISIBLE_MARK: Final[str] = "�"

#: Every character that changes what a terminal draws without drawing anything
#: itself, written as closed codepoint ranges.
#:
#: **The set is exactly two things, and the docstring above says which.** The
#: first is the whole of Unicode general category ``Cf`` (format characters) —
#: not the bidirectional subset, because the invisible carriers that matter
#: most today are outside it: the Tag block ``U+E0020``–``U+E007F`` is the
#: current standard way to hide text inside a string aimed at a language model,
#: and it is category ``Cf`` while carrying no bidi meaning at all. The second
#: is the small set of characters that are *not* ``Cf`` and share the effect
#: anyway: the variation selectors, which render as nothing and change the
#: glyph beside them, and the Hangul fillers, which are ordinary letters
#: (``Lo``) that occupy no ink. Two variation selectors are held out of that
#: half; :data:`PRESENTATION_SELECTORS` below names them and says why.
#:
#: Ranges rather than a derivation from ``unicodedata`` at import, for round
#: 4's reason, which still holds: deriving means either a per-character Python
#: loop on the hot path (every transcript line goes through :func:`defang`) or
#: a scan of the whole 1.1M-codepoint space at import. The expansion below is
#: ~430 entries built once.
#:
#: **This list is pinned against ``unicodedata`` by a test, not by care.** The
#: suite walks the code space, collects every ``Cf`` character, and asserts the
#: table covers all of them — so a Unicode version that adds a format character
#: fails the suite rather than silently letting one through. That failure is
#: the intended behaviour on a Python upgrade: it says the table is stale.
_INVISIBLE_RANGES: Final[tuple[tuple[int, int], ...]] = (
    # ── Unicode general category Cf, in codepoint order ──────────────────
    (0x00AD, 0x00AD),  # SOFT HYPHEN — drawn only at a line break
    (0x0600, 0x0605),  # ARABIC NUMBER SIGN … ARABIC NUMBER MARK ABOVE
    (0x061C, 0x061C),  # ARABIC LETTER MARK
    (0x06DD, 0x06DD),  # ARABIC END OF AYAH
    (0x070F, 0x070F),  # SYRIAC ABBREVIATION MARK
    (0x0890, 0x0891),  # ARABIC POUND/PIASTRE MARK ABOVE
    (0x08E2, 0x08E2),  # ARABIC DISPUTED END OF AYAH
    (0x180E, 0x180E),  # MONGOLIAN VOWEL SEPARATOR
    (0x200B, 0x200F),  # zero widths, and the implicit LTR/RTL marks
    (0x202A, 0x202E),  # the bidi embeddings and overrides (Trojan Source)
    (0x2060, 0x2064),  # WORD JOINER and the invisible math operators
    (0x2066, 0x206F),  # the bidi isolates, and the deprecated shaping set
    (0xFEFF, 0xFEFF),  # ZERO WIDTH NO-BREAK SPACE / BOM
    (0xFFF9, 0xFFFB),  # the interlinear annotation anchors
    (0x110BD, 0x110BD),  # KAITHI NUMBER SIGN
    (0x110CD, 0x110CD),  # KAITHI NUMBER SIGN ABOVE
    (0x13430, 0x1343F),  # Egyptian hieroglyph format controls
    (0x1BCA0, 0x1BCA3),  # Duployan shorthand format controls
    (0x1D173, 0x1D17A),  # musical notation beams, ties, phrases
    (0xE0001, 0xE0001),  # LANGUAGE TAG
    (0xE0020, 0xE007F),  # the Tag block — invisible text aimed at a model
    # ── not Cf, same effect ──────────────────────────────────────────────
    (0x115F, 0x1160),  # HANGUL CHOSEONG/JUNGSEONG FILLER — Lo, no ink
    (0x3164, 0x3164),  # HANGUL FILLER
    (0xFE00, 0xFE0D),  # VARIATION SELECTOR-1 … -14 — glyph variants, no ink
    (0xFFA0, 0xFFA0),  # HALFWIDTH HANGUL FILLER
    (0xE0100, 0xE01EF),  # VARIATION SELECTOR-17 … -256
)

#: The two variation selectors that are deliberately **not** in the table.
#:
#: U+FE0E and U+FE0F are the presentation selectors: they ask for the
#: monochrome text glyph or the coloured emoji one. They are the reason ``⚠️``,
#: ``ℹ️`` and ``❤️`` used to arrive on screen as a bare symbol with a marker
#: stuck to it, which is the defect this exemption fixes.
#:
#: **The line is drawn at whether the picture changes, and it is measurable.**
#: Everything else in this table draws nothing *and changes nothing visible* —
#: which is precisely what makes it dangerous, because two different byte
#: strings then produce one picture and approving a picture no longer says
#: which bytes run. A presentation selector fails that test: ``⚠`` is one cell
#: and ``⚠️`` is two, so the byte difference is on screen. The contrast with
#: U+200D ZERO WIDTH JOINER, which stays in the table, is the whole argument —
#: ``rm`` and ``r<ZWJ>m`` are the same picture and different commands.
#:
#: Two consequences are accepted rather than overlooked. Emoji built from ZWJ
#: sequences — a multi-person family — are still marked, because the joiner is
#: the hazard and its emoji use does not make it less of one. And a presentation
#: selector still changes a character's *width*, which is why
#: :func:`~talaria.ui.prompts.wrap_command` measures cells rather than
#: characters; ``tests/ui/test_prompts.py`` asserts the wrap agrees with the
#: rendered width on an emoji command, because a column count that disagrees
#: with the terminal is its own way of showing the wrong command.
PRESENTATION_SELECTORS: Final[tuple[int, ...]] = (0xFE0E, 0xFE0F)

_TRANSLATION = {
    codepoint: replacement for codepoint, replacement in _CONTROL_PICTURES.items()
}
# Every remaining C0 control except tab (0x09) and newline (0x0A).
for _code in range(0x00, 0x20):
    if _code in (0x09, 0x0A):
        continue
    _TRANSLATION.setdefault(_code, "␦")
_TRANSLATION.setdefault(0x7F, "␡")
for _first, _last in _INVISIBLE_RANGES:
    for _code in range(_first, _last + 1):
        _TRANSLATION.setdefault(_code, INVISIBLE_MARK)


def defang(value: str) -> str:
    """Replace obeyable and invisible characters with visible stand-ins.

    **The test is whether the character changes the picture, not whether it
    draws one.** A character that draws nothing *and leaves the picture the
    same* is the hazard: two different byte strings then render identically, and
    an approval that was given to a picture no longer says which bytes run. That
    is why U+200D ZERO WIDTH JOINER is marked — ``rm`` and ``r<ZWJ>m`` are one
    picture and two commands — and why the presentation selectors U+FE0E and
    U+FE0F are not, since ``⚠`` is one cell and ``⚠️`` is two.

    **The cost is taken deliberately, and emoji still pay part of it.** The
    joiner is how a sequence such as a multi-person family is built, so those
    arrive as their component parts with a marker between them. A single emoji
    asking for its coloured form does not, which is the common case in agent
    prose and the one the exemption above is for.

    **Emoji are not the only payers, and an earlier version of this docstring
    said they were.** The table covers every general-category ``Cf`` codepoint,
    so ordinary prose loses too: U+00AD SOFT HYPHEN turns ``co<AD>operate`` into
    ``co?operate``, a U+FEFF byte-order mark at the head of a pasted file
    becomes a visible cell, and U+0600 ARABIC NUMBER SIGN and the musical
    notation format characters are marked wherever they appear. Each of those
    is a character a reader cannot see, which is the whole reason it is in the
    table; the point of naming them here is that the next person to read this
    comment should not be surprised by a marker in a paragraph of English.

    That is a cosmetic loss in prose against a correctness gain on the one
    surface where the bytes and the picture must agree: a shell command
    somebody is about to approve.

    :func:`defang` is deliberately one function rather than a strict version
    for commands and a lenient one for prose, because two rules means one of
    them is eventually applied to the wrong string — and the string it would be
    applied to wrongly is the command.
    """
    return value.translate(_TRANSLATION)


def literal_text(value: str) -> Text:
    """The one way this package turns an untrusted string into a renderable.

    Returns a :class:`rich.text.Text` with no style and no markup parsing, over
    a string whose control characters have been defanged.
    """
    return Text(defang(value), no_wrap=False, end="")
