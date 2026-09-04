"""Issue #123 U4/R5: host terminal palette inheritance (decision C).

:func:`apply_host_palette` is pure data — no terminal framework — so this
module never imports ``talaria.ui``. The registry wiring (layer order,
readability reverts) is covered in ``tests/ui/test_theme.py``; what is
pinned here is the inheritance unit itself: what applies, what is
preserved, and that unresolvable input degrades with a notice.
"""

from __future__ import annotations

from talaria.themes import THEME_TOKENS
from talaria.themes.builtins import REFINED_DEFAULT
from talaria.themes.host_palette import HOST_INHERITED_TOKENS, apply_host_palette

BASE = dict(REFINED_DEFAULT.tokens)


def test_unresolvable_hosts_degrade_to_the_base_with_a_notice() -> None:
    """None, a string, and a list all degrade — never a crash, never blank."""
    for host in (None, "#000000", ["talaria.canvas", "#000000"], 7):
        result = apply_host_palette(BASE, host)

        assert dict(result.tokens) == BASE
        assert result.used_host is False
        assert result.adopted_tokens == ()
        assert len(result.notices) == 1
        assert "unavailable" in result.notices[0]


def test_empty_and_useless_hosts_degrade_without_adopting() -> None:
    """A mapping with nothing usable is unresolvable in effect as in name."""
    result = apply_host_palette(
        BASE,
        {"bogus": "#000000", "talaria.canvas": "not-a-color", "talaria.text": None},
    )

    assert dict(result.tokens) == BASE
    assert result.used_host is False
    assert "no usable host entries" in result.notices[0]


def test_only_flat_chrome_tokens_are_inherited() -> None:
    """Semantic channels stay Talaria-owned no matter what the host claims."""
    result = apply_host_palette(
        BASE,
        {
            "talaria.canvas": "#0A0A0A",
            "talaria.text": "#F0F0F0",
            "talaria.transcript.operator": "#123456",
            "talaria.transcript.operator.background": "#234567",
            "talaria.diff.added": "#345678",
            "talaria.syntax.keyword": "#456789",
            "talaria.status.success": "#56789A",
        },
    )

    assert result.tokens["talaria.canvas"] == "#0A0A0A"
    assert result.tokens["talaria.text"] == "#F0F0F0"
    for token in (
        "talaria.transcript.operator",
        "talaria.transcript.operator.background",
        "talaria.diff.added",
        "talaria.syntax.keyword",
        "talaria.status.success",
    ):
        assert result.tokens[token] == BASE[token]
    assert result.adopted_tokens == ("talaria.canvas", "talaria.text")
    assert result.used_host is True


def test_explicit_talaria_overrides_always_win() -> None:
    """Overrides beat inherited values on chrome and semantic tokens alike."""
    result = apply_host_palette(
        BASE,
        {"talaria.canvas": "#0A0A0A", "talaria.text": "#F0F0F0"},
        overrides={
            "talaria.canvas": "#111111",
            "talaria.transcript.operator": "#222222",
            "talaria.text": "bogus",
        },
    )

    assert result.tokens["talaria.canvas"] == "#111111"
    assert result.tokens["talaria.transcript.operator"] == "#222222"
    # A malformed override is not an override — the host value stands.
    assert result.tokens["talaria.text"] == "#F0F0F0"
    assert result.adopted_tokens == ("talaria.text",)


def test_sparse_hosts_leave_everything_else_on_the_base() -> None:
    """One inherited token never drags its neighbors along."""
    result = apply_host_palette(BASE, {"talaria.panel": "#1A1A1A"})

    assert result.tokens["talaria.panel"] == "#1A1A1A"
    assert result.tokens["talaria.canvas"] == BASE["talaria.canvas"]
    assert result.tokens["talaria.surface"] == BASE["talaria.surface"]
    assert result.adopted_tokens == ("talaria.panel",)


def test_one_bad_entry_never_breaks_the_rest() -> None:
    """Malformed and unknown entries are ignored with a visible notice."""
    result = apply_host_palette(
        BASE,
        {
            "talaria.canvas": "#0A0A0A",
            "talaria.surface": "transparent",
            "talaria.panel": None,
            "nope": "#1B1B1B",
        },
    )

    assert result.tokens["talaria.canvas"] == "#0A0A0A"
    assert result.tokens["talaria.surface"] == BASE["talaria.surface"]
    assert result.tokens["talaria.panel"] == BASE["talaria.panel"]
    assert any("ignored" in notice for notice in result.notices)


def test_the_inherited_set_covers_chrome_and_only_chrome() -> None:
    """The decision-C boundary, pinned: flat surfaces in, semantics out."""
    assert "talaria.canvas" in HOST_INHERITED_TOKENS
    assert "talaria.selection.text" in HOST_INHERITED_TOKENS
    assert "talaria.status.background" in HOST_INHERITED_TOKENS
    assert "talaria.inspector.background" in HOST_INHERITED_TOKENS
    assert not any(
        token.startswith(("talaria.transcript.", "talaria.diff.", "talaria.syntax."))
        for token in HOST_INHERITED_TOKENS
    )
    assert all(token in THEME_TOKENS for token in HOST_INHERITED_TOKENS)
