"""Render arbitrary text as *literal* text, with nothing interpreted.

Three separate interpreters sit between a string and a terminal cell, and each
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

Tab is kept as a real tab (a status command aligning columns with tabs is doing
something legitimate) and newline is kept because the caller has already split
on it.
"""

from __future__ import annotations

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

_TRANSLATION = {
    codepoint: replacement for codepoint, replacement in _CONTROL_PICTURES.items()
}
# Every remaining C0 control except tab (0x09) and newline (0x0A).
for _code in range(0x00, 0x20):
    if _code in (0x09, 0x0A):
        continue
    _TRANSLATION.setdefault(_code, "␦")
_TRANSLATION.setdefault(0x7F, "␡")


def defang(value: str) -> str:
    """Replace obeyable control characters with visible stand-ins."""
    return value.translate(_TRANSLATION)


def literal_text(value: str) -> Text:
    """The one way this package turns an untrusted string into a renderable.

    Returns a :class:`rich.text.Text` with no style and no markup parsing, over
    a string whose control characters have been defanged.
    """
    return Text(defang(value), no_wrap=False, end="")
