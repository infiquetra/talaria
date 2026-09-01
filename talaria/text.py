"""Make terminal-bound text visible rather than obeyable or invisible.

This module is framework-free so configuration notices and the user interface
share one control-character rule. A second, narrower rule would eventually be
used on a string that needs the complete guarantee.

The rule covers ANSI escape sequences and C0 controls, which a terminal would
obey, plus bidirectional and zero-width Unicode characters that can make
distinct byte strings look identical. Tab is kept because it can legitimately
align columns. Newline is kept because callers already own line splitting.
"""

from __future__ import annotations

from typing import Final

#: Visible stand-ins for controls that a terminal would otherwise obey.
_CONTROL_PICTURES: dict[int, str] = {
    0x1B: "␛",
    0x07: "␇",
    0x08: "␈",
    0x0B: "␋",
    0x0C: "␌",
    0x0D: "␍",
    0x00: "␀",
}

#: What a bidirectional or zero-width character is replaced with.
#:
#: One cell keeps wrapping and column arithmetic aligned with what the terminal
#: draws. It is distinct from a control picture because these characters would
#: be rendered into a misleading picture rather than obeyed as terminal input.
INVISIBLE_MARK: Final[str] = "�"

#: Characters that change the terminal picture without drawing anything,
#: written as closed codepoint ranges.
#:
#: The first group is Unicode's complete ``Cf`` category. The second is the
#: small set outside ``Cf`` with the same invisible effect. Static ranges avoid
#: a Unicode code-space scan at import and are pinned against ``unicodedata`` by
#: the test suite, so a Python Unicode update fails loudly if this table ages.
_INVISIBLE_RANGES: Final[tuple[tuple[int, int], ...]] = (
    # Unicode general category Cf, in codepoint order.
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
    # Not Cf, but with the same invisible effect.
    (0x115F, 0x1160),
    (0x3164, 0x3164),
    (0xFE00, 0xFE0D),
    (0xFFA0, 0xFFA0),
    (0xE0100, 0xE01EF),
)

#: The two variation selectors deliberately excluded from the table.
#:
#: U+FE0E and U+FE0F visibly select text or emoji presentation, so their byte
#: difference is represented on screen. Other variation selectors remain
#: marked because a reader cannot reliably distinguish the glyph variants.
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

    The test is whether a character can make distinct bytes appear identical,
    not merely whether it draws a cell. This marks U+200D ZERO WIDTH JOINER,
    bidirectional controls, Unicode Tag characters, and the complete ``Cf``
    category. The two presentation selectors stay intact because they visibly
    change the adjacent glyph.

    The cost is deliberate: emoji joiner sequences and invisible format
    characters in prose receive markers. That cosmetic loss preserves the
    stronger guarantee required for a command an operator may approve.

    One rule serves every caller. A strict command version and a lenient prose
    version would eventually be applied to the wrong string.
    """
    return value.translate(_TRANSLATION)
