"""Render arbitrary text as literal Rich text, with no markup interpretation.

Passing :class:`rich.text.Text` rather than a string bypasses Rich's markup
parser. :func:`talaria.text.defang` separately makes terminal controls and
invisible Unicode visible; it lives outside the user-interface package so
framework-free callers use the same rule.
"""

from __future__ import annotations

from rich.text import Text

from talaria.text import INVISIBLE_MARK, PRESENTATION_SELECTORS, defang

__all__ = ("INVISIBLE_MARK", "PRESENTATION_SELECTORS", "defang", "literal_text")


def literal_text(value: str) -> Text:
    """Turn an untrusted string into markup-free, terminal-safe Rich text."""
    return Text(defang(value), no_wrap=False, end="")
