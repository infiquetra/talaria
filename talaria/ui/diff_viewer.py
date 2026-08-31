"""A bounded, strictly read-only projection of session-reported unified diffs.

The viewer accepts immutable presentation input.  The application adapter added
when the shared-surface lease is granted will translate the domain's
``DiffDocument`` into these values; this module never opens a repository file,
runs Git, or reaches a dispatcher.  Parsing and row indexing happen once in the
constructor.  Painting formats only the viewport and ten rows of overscan on
either side.

Pygments is deliberately a presentation dependency.  Lexer selection is a
small extension map rather than filename guessing, so an unknown suffix has an
honest plain-text fallback and cannot trigger plugin discovery.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from difflib import SequenceMatcher
from typing import ClassVar, Final, Literal

from pygments import lex  # type: ignore[import-untyped]
from pygments.lexer import Lexer  # type: ignore[import-untyped]
from pygments.lexers import get_lexer_by_name  # type: ignore[import-untyped]
from pygments.token import (  # type: ignore[import-untyped]
    Comment,
    Keyword,
    Name,
    Operator,
    Punctuation,
    String,
)
from pygments.token import Literal as PygmentsLiteral
from rich.cells import cell_len
from rich.style import Style
from rich.text import Text
from textual import events
from textual.app import ComposeResult
from textual.binding import Binding, BindingType
from textual.containers import Vertical
from textual.geometry import Size
from textual.screen import ModalScreen
from textual.scroll_view import ScrollView
from textual.strip import Strip
from textual.widgets import Static

from talaria.domain.changes import DiffDocument
from talaria.domain.selection import Choice, PickerSource, Selection, Stage
from talaria.ui.dialog import PickerDialog
from talaria.ui.literal import defang, literal_text

DiffMode = Literal["side-by-side", "unified"]
LineKind = Literal["context", "added", "removed", "hunk", "metadata"]

SIDE_BY_SIDE_MIN_WIDTH: Final = 112
OVERSCAN_ROWS: Final = 10
INTRALINE_CELL_CAP: Final = 2_000
MINIMUM_PANE_WIDTH: Final = 54

SIDE_BY_SIDE_REFUSAL: Final = (
    "side-by-side needs 112 columns; unified active"
)
NO_DIFFS: Final = "[none available from this session]"

# Deliberately bounded and documented here rather than delegated to Pygments'
# filename guessing.  The latter considers every installed lexer plugin.
LEXER_BY_EXTENSION: Final[dict[str, str]] = {
    ".bash": "bash",
    ".cjs": "javascript",
    ".js": "javascript",
    ".json": "json",
    ".md": "markdown",
    ".mjs": "javascript",
    ".py": "python",
    ".sh": "bash",
    ".toml": "toml",
    ".ts": "typescript",
    ".tsx": "tsx",
    ".yaml": "yaml",
    ".yml": "yaml",
}

_HUNK_HEADER = re.compile(
    r"^@@ -(?P<old>\d+)(?:,(?P<old_count>\d+))? "
    r"\+(?P<new>\d+)(?:,(?P<new_count>\d+))? @@"
)


@dataclass(frozen=True)
class DiffViewerFile:
    """One held file handed to the presentation adapter."""

    key: str
    path: str
    unified_diff: str


@dataclass(frozen=True)
class DiffViewerDocument:
    """The session-reported file set; an empty tuple is an honest empty state."""

    files: tuple[DiffViewerFile, ...] = ()


def adapt_diff_document(document: DiffDocument) -> DiffViewerDocument:
    """Copy the domain's held diff set into the read-only presentation shape."""
    return DiffViewerDocument(
        tuple(
            DiffViewerFile(
                key=changed_file.key,
                path=changed_file.path,
                unified_diff=changed_file.unified_diff,
            )
            for changed_file in document.files
        )
    )


@dataclass(frozen=True)
class _DiffLine:
    kind: LineKind
    text: str
    old_number: int | None = None
    new_number: int | None = None
    hunk_index: int | None = None
    pair_id: int | None = None
    row_id: int = 0

    @property
    def content(self) -> str:
        if self.kind in {"added", "removed", "context"}:
            return self.text[1:]
        return self.text


@dataclass(frozen=True)
class _SideRow:
    left: _DiffLine | None
    right: _DiffLine | None
    hunk_index: int | None


@dataclass(frozen=True)
class _IndexedFile:
    source: DiffViewerFile
    unified_rows: tuple[_DiffLine, ...]
    side_rows: tuple[_SideRow, ...]
    hunk_count: int
    lexer_alias: str | None
    lexer: Lexer | None
    pair_lines: dict[int, tuple[_DiffLine, _DiffLine]]


@dataclass(frozen=True)
class _IntralineSpans:
    removed: tuple[tuple[int, int], ...]
    added: tuple[tuple[int, int], ...]


def _extension(path: str) -> str:
    leaf = path.rsplit("/", 1)[-1]
    dot = leaf.rfind(".")
    return "" if dot < 0 else leaf[dot:].casefold()


def _lexer(path: str) -> tuple[str | None, Lexer | None]:
    alias = LEXER_BY_EXTENSION.get(_extension(path))
    if alias is None:
        return None, None
    return alias, get_lexer_by_name(alias, stripnl=False, ensurenl=False)


def _pair_change_runs(
    lines: list[_DiffLine],
) -> tuple[list[_DiffLine], dict[int, tuple[_DiffLine, _DiffLine]]]:
    """Assign pair ids to adjacent deletion/addition runs within one hunk."""
    paired = list(lines)
    pairs: dict[int, tuple[_DiffLine, _DiffLine]] = {}
    pair_id = 0
    index = 0
    while index < len(paired):
        line = paired[index]
        if line.kind != "removed":
            index += 1
            continue
        removed_start = index
        while index < len(paired) and paired[index].kind == "removed":
            index += 1
        added_start = index
        while index < len(paired) and paired[index].kind == "added":
            index += 1
        pair_count = min(added_start - removed_start, index - added_start)
        for offset in range(pair_count):
            removed_at = removed_start + offset
            added_at = added_start + offset
            removed = paired[removed_at]
            added = paired[added_at]
            removed = _DiffLine(
                kind=removed.kind,
                text=removed.text,
                old_number=removed.old_number,
                new_number=removed.new_number,
                hunk_index=removed.hunk_index,
                pair_id=pair_id,
                row_id=removed.row_id,
            )
            added = _DiffLine(
                kind=added.kind,
                text=added.text,
                old_number=added.old_number,
                new_number=added.new_number,
                hunk_index=added.hunk_index,
                pair_id=pair_id,
                row_id=added.row_id,
            )
            paired[removed_at] = removed
            paired[added_at] = added
            pairs[pair_id] = (removed, added)
            pair_id += 1
    return paired, pairs


def _parse_unified(source: DiffViewerFile) -> _IndexedFile:
    rows: list[_DiffLine] = []
    old_number = 0
    new_number = 0
    hunk_index: int | None = None

    for raw in source.unified_diff.splitlines():
        match = _HUNK_HEADER.match(raw)
        if match is not None:
            hunk_index = 0 if hunk_index is None else hunk_index + 1
            old_number = int(match.group("old"))
            new_number = int(match.group("new"))
            rows.append(
                _DiffLine(
                    "hunk",
                    raw,
                    hunk_index=hunk_index,
                    row_id=len(rows),
                )
            )
            continue

        # File headers and producer metadata are indexed but not rendered as
        # source rows.  The selected filename is already present in the modal
        # header, and showing ---/+++ as deletions/additions would be false.
        if hunk_index is None:
            continue

        if raw.startswith("+") and not raw.startswith("+++"):
            rows.append(
                _DiffLine(
                    "added",
                    raw,
                    new_number=new_number,
                    hunk_index=hunk_index,
                    row_id=len(rows),
                )
            )
            new_number += 1
        elif raw.startswith("-") and not raw.startswith("---"):
            rows.append(
                _DiffLine(
                    "removed",
                    raw,
                    old_number=old_number,
                    hunk_index=hunk_index,
                    row_id=len(rows),
                )
            )
            old_number += 1
        elif raw.startswith(" "):
            rows.append(
                _DiffLine(
                    "context",
                    raw,
                    old_number=old_number,
                    new_number=new_number,
                    hunk_index=hunk_index,
                    row_id=len(rows),
                )
            )
            old_number += 1
            new_number += 1
        else:
            rows.append(
                _DiffLine(
                    "metadata",
                    raw,
                    hunk_index=hunk_index,
                    row_id=len(rows),
                )
            )

    paired, pairs = _pair_change_runs(rows)
    side_rows = _to_side_rows(paired)
    alias, lexer = _lexer(source.path)
    return _IndexedFile(
        source=source,
        unified_rows=tuple(paired),
        side_rows=side_rows,
        hunk_count=0 if hunk_index is None else hunk_index + 1,
        lexer_alias=alias,
        lexer=lexer,
        pair_lines=pairs,
    )


def _to_side_rows(lines: list[_DiffLine]) -> tuple[_SideRow, ...]:
    rows: list[_SideRow] = []
    index = 0
    while index < len(lines):
        line = lines[index]
        if line.kind == "removed":
            removed: list[_DiffLine] = []
            while index < len(lines) and lines[index].kind == "removed":
                removed.append(lines[index])
                index += 1
            added: list[_DiffLine] = []
            while index < len(lines) and lines[index].kind == "added":
                added.append(lines[index])
                index += 1
            for offset in range(max(len(removed), len(added))):
                left = removed[offset] if offset < len(removed) else None
                right = added[offset] if offset < len(added) else None
                held = left if left is not None else right
                rows.append(
                    _SideRow(left, right, None if held is None else held.hunk_index)
                )
            continue
        if line.kind == "added":
            rows.append(_SideRow(None, line, line.hunk_index))
        else:
            rows.append(_SideRow(line, line, line.hunk_index))
        index += 1
    return tuple(rows)


def _intraline_spans(removed: str, added: str) -> _IntralineSpans:
    """Return changed character spans, with no comparison above the cell cap."""
    if cell_len(removed) > INTRALINE_CELL_CAP or cell_len(added) > INTRALINE_CELL_CAP:
        return _IntralineSpans(((0, len(removed)),), ((0, len(added)),))

    removed_spans: list[tuple[int, int]] = []
    added_spans: list[tuple[int, int]] = []
    matcher = SequenceMatcher(None, removed, added, autojunk=False)
    for tag, old_start, old_end, new_start, new_end in matcher.get_opcodes():
        if tag == "equal":
            continue
        if old_start != old_end:
            removed_spans.append((old_start, old_end))
        if new_start != new_end:
            added_spans.append((new_start, new_end))
    return _IntralineSpans(tuple(removed_spans), tuple(added_spans))


def _syntax_token(token_type: object) -> str | None:
    if token_type in Comment:
        return "talaria-syntax-comment"
    if token_type in Keyword:
        return "talaria-syntax-keyword"
    if token_type in String:
        return "talaria-syntax-string"
    if token_type in PygmentsLiteral.Number:
        return "talaria-syntax-number"
    if token_type in Name.Function:
        return "talaria-syntax-function"
    if token_type in (Name.Class, Name.Namespace, Name.Builtin, Keyword.Type):
        return "talaria-syntax-type"
    if token_type in (Name.Variable, Name.Attribute, Name.Parameter):
        return "talaria-syntax-variable"
    if token_type in Operator or token_type in Punctuation:
        return "talaria-syntax-operator"
    if token_type in (Name.Constant, PygmentsLiteral):
        return "talaria-syntax-constant"
    return None


class _FilePickerSource(PickerSource):
    def __init__(self, files: tuple[_IndexedFile, ...], selected: int) -> None:
        self._choices = tuple(
            Choice(
                key=file.source.key,
                label=file.source.path,
                payload=file.source.key,
                marked=index == selected,
            )
            for index, file in enumerate(files)
        )

    def root(self) -> Stage:
        return Stage(
            title="changed files",
            selection=Selection.opened(self._choices),
        )

    def descend(self, depth: int, choice: Choice) -> Stage | str:
        return choice.payload


class DiffCanvas(ScrollView, can_focus=True):
    """Line-API canvas whose cache is one viewport plus bounded overscan."""

    DEFAULT_CSS = """
    DiffCanvas {
        width: 100%;
        height: 1fr;
        scrollbar-size-vertical: 1;
        scrollbar-size-horizontal: 1;
        background: $background;
    }
    """

    def __init__(self, files: tuple[_IndexedFile, ...]) -> None:
        super().__init__(id="diff-canvas")
        self._files = files
        self.file_index = 0
        self.hunk_index = 0
        self.mode: DiffMode = "unified"
        self._pane_width = MINIMUM_PANE_WIDTH
        self._cache_key: tuple[object, ...] | None = None
        self._row_cache: dict[int, Text] = {}
        self.formatted_row_count = 0
        self.intraline_compare_count = 0
        self.intraline_pair_ids: frozenset[int] = frozenset()
        self._update_virtual_size()

    @property
    def active_file(self) -> _IndexedFile | None:
        if not self._files:
            return None
        return self._files[self.file_index]

    @property
    def row_count(self) -> int:
        file = self.active_file
        if file is None:
            return 1
        rows = file.side_rows if self.mode == "side-by-side" else file.unified_rows
        return max(1, len(rows))

    @property
    def anchor_row_id(self) -> int | None:
        """Identity of the top source row, shared by both render modes."""
        file = self.active_file
        if file is None or not file.unified_rows:
            return None
        row = min(self.scroll_offset.y, self.row_count - 1)
        if self.mode == "unified":
            return file.unified_rows[row].row_id
        side = file.side_rows[row]
        held = side.left if side.left is not None else side.right
        return None if held is None else held.row_id

    @property
    def active_lexer(self) -> str:
        file = self.active_file
        if file is None or file.lexer_alias is None:
            return "plain text"
        return file.lexer_alias

    def set_view(
        self,
        *,
        file_index: int,
        hunk_index: int,
        mode: DiffMode,
        preserve_anchor: bool,
    ) -> None:
        anchor = self.anchor_row_id
        if self._files:
            self.file_index = file_index % len(self._files)
            count = self._files[self.file_index].hunk_count
            self.hunk_index = 0 if count == 0 else hunk_index % count
        else:
            self.file_index = 0
            self.hunk_index = 0
        self.mode = mode
        self._invalidate()
        self._update_virtual_size()
        if preserve_anchor:
            self.scroll_to(
                y=self._row_for_anchor(anchor),
                animate=False,
                immediate=True,
                force=True,
            )
        else:
            self.scroll_to(
                x=0,
                y=self._hunk_row(),
                animate=False,
                immediate=True,
                force=True,
            )

    def _row_for_anchor(self, row_id: int | None) -> int:
        file = self.active_file
        if file is None or row_id is None:
            return 0
        if self.mode == "unified":
            for index, line in enumerate(file.unified_rows):
                if line.row_id == row_id:
                    return index
        else:
            for index, row in enumerate(file.side_rows):
                if any(
                    line is not None and line.row_id == row_id
                    for line in (row.left, row.right)
                ):
                    return index
        return 0

    def _hunk_row(self) -> int:
        file = self.active_file
        if file is None:
            return 0
        if self.mode == "unified":
            for index, line in enumerate(file.unified_rows):
                if line.kind == "hunk" and line.hunk_index == self.hunk_index:
                    return index
        else:
            for index, row in enumerate(file.side_rows):
                if (
                    row.left is not None
                    and row.left.kind == "hunk"
                    and row.hunk_index == self.hunk_index
                ):
                    return index
        return 0

    def _update_virtual_size(self) -> None:
        file = self.active_file
        viewport = max(1, self.size.width - 1)
        if file is None:
            self.virtual_size = Size(viewport, 1)
            return
        number_width = self._number_width(file)
        source_width = max(
            (cell_len(defang(line.content)) for line in file.unified_rows),
            default=0,
        )
        if self.mode == "side-by-side":
            fitted = max(MINIMUM_PANE_WIDTH, (viewport - 1) // 2)
            self._pane_width = max(fitted, number_width + 2 + source_width)
            width = self._pane_width * 2 + 1
        else:
            width = max(viewport, number_width * 2 + 5 + source_width)
        self.virtual_size = Size(width, self.row_count)

    @staticmethod
    def _number_width(file: _IndexedFile) -> int:
        maximum = max(
            (
                number
                for line in file.unified_rows
                for number in (line.old_number, line.new_number)
                if number is not None
            ),
            default=0,
        )
        return max(2, len(str(maximum)))

    def on_resize(self, event: events.Resize) -> None:
        self._invalidate()
        self._update_virtual_size()

    def _invalidate(self) -> None:
        self._cache_key = None
        self._row_cache.clear()
        self.refresh()

    def render_line(self, y: int) -> Strip:
        scroll_x, scroll_y = self.scroll_offset
        absolute_y = scroll_y + y
        self._prepare_window()
        text = self._row_cache.get(absolute_y)
        if text is None:
            return Strip.blank(self.size.width, self.rich_style)
        full = Strip(text.render(self.app.console), cell_len(text.plain))
        width = self.scrollable_content_region.width
        hidden_right = full.cell_length > scroll_x + width
        if hidden_right and width > 0:
            visible = full.crop_extend(scroll_x, scroll_x + width - 1, self.rich_style)
            ellipsis = Strip(
                Text("…", style=self._style("talaria-diff-line-number")).render(
                    self.app.console
                ),
                1,
            )
            return Strip.join((visible, ellipsis)).apply_offsets(scroll_x, absolute_y)
        return full.crop_extend(
            scroll_x, scroll_x + width, self.rich_style
        ).apply_offsets(scroll_x, absolute_y)

    def _prepare_window(self) -> None:
        theme_values = tuple(sorted(self.app.theme_variables.items()))
        start = max(0, self.scroll_offset.y - OVERSCAN_ROWS)
        end = min(
            self.row_count,
            self.scroll_offset.y + max(1, self.size.height) + OVERSCAN_ROWS,
        )
        key: tuple[object, ...] = (
            self.file_index,
            self.hunk_index,
            self.mode,
            self._pane_width,
            start,
            end,
            self.app.theme,
            theme_values,
        )
        if key == self._cache_key:
            return
        self._cache_key = key
        self._row_cache.clear()
        spans = self._visible_intraline_spans(start, end)
        for row in range(start, end):
            self._row_cache[row] = self._format_row(row, spans)
        self.formatted_row_count = len(self._row_cache)

    def _visible_intraline_spans(
        self, start: int, end: int
    ) -> dict[int, _IntralineSpans]:
        file = self.active_file
        if file is None:
            self.intraline_compare_count = 0
            self.intraline_pair_ids = frozenset()
            return {}
        pair_ids: set[int] = set()
        if self.mode == "unified":
            for line in file.unified_rows[start:end]:
                if line.pair_id is not None:
                    pair_ids.add(line.pair_id)
        else:
            for row in file.side_rows[start:end]:
                for side_line in (row.left, row.right):
                    if side_line is not None and side_line.pair_id is not None:
                        pair_ids.add(side_line.pair_id)
        spans: dict[int, _IntralineSpans] = {}
        compared = 0
        for pair_id in pair_ids:
            removed, added = file.pair_lines[pair_id]
            if (
                cell_len(removed.content) <= INTRALINE_CELL_CAP
                and cell_len(added.content) <= INTRALINE_CELL_CAP
            ):
                compared += 1
            spans[pair_id] = _intraline_spans(removed.content, added.content)
        self.intraline_compare_count = compared
        self.intraline_pair_ids = frozenset(pair_ids)
        return spans

    def _format_row(
        self, row_index: int, spans: dict[int, _IntralineSpans]
    ) -> Text:
        file = self.active_file
        if file is None or not file.unified_rows:
            return literal_text(NO_DIFFS)
        lexer = self._active_lexer()
        if self.mode == "unified":
            line = file.unified_rows[row_index]
            return self._format_unified(line, file, lexer, spans)
        return self._format_side(file.side_rows[row_index], file, lexer, spans)

    def _active_lexer(self) -> Lexer | None:
        file = self.active_file
        if file is None:
            return None
        return file.lexer

    def _format_unified(
        self,
        line: _DiffLine,
        file: _IndexedFile,
        lexer: Lexer | None,
        spans: dict[int, _IntralineSpans],
    ) -> Text:
        if line.kind == "hunk":
            return Text(
                defang(line.text),
                style=self._line_style(line, selected=True),
                no_wrap=True,
                end="",
            )
        if line.kind == "metadata":
            return Text(
                defang(line.text),
                style=self._line_style(line),
                no_wrap=True,
                end="",
            )
        width = self._number_width(file)
        old = "" if line.old_number is None else str(line.old_number)
        new = "" if line.new_number is None else str(line.new_number)
        marker = {"added": "+", "removed": "-", "context": " "}[line.kind]
        result = Text(
            f"{old:>{width}} {new:>{width}} {marker}",
            style=self._line_style(line),
            no_wrap=True,
            end="",
        )
        content_start = len(result.plain)
        result.append_text(self._syntax_text(line.content, lexer))
        self._apply_intraline(result, line, content_start, spans)
        return result

    def _format_side(
        self,
        row: _SideRow,
        file: _IndexedFile,
        lexer: Lexer | None,
        spans: dict[int, _IntralineSpans],
    ) -> Text:
        left = self._format_pane(row.left, file, lexer, spans)
        right = self._format_pane(row.right, file, lexer, spans)
        self._pad(left, self._pane_width)
        self._pad(right, self._pane_width)
        left.append("│", style=self._style("talaria-diff-line-number"))
        left.append_text(right)
        return left

    def _format_pane(
        self,
        line: _DiffLine | None,
        file: _IndexedFile,
        lexer: Lexer | None,
        spans: dict[int, _IntralineSpans],
    ) -> Text:
        if line is None:
            return Text("", no_wrap=True, end="")
        if line.kind in {"hunk", "metadata"}:
            return Text(
                defang(line.text),
                style=self._line_style(
                    line,
                    selected=line.kind == "hunk" and line.hunk_index == self.hunk_index,
                ),
                no_wrap=True,
                end="",
            )
        width = self._number_width(file)
        number = line.old_number if line.kind == "removed" else line.new_number
        marker = {"added": "+", "removed": "-", "context": " "}[line.kind]
        result = Text(
            f"{'' if number is None else number:>{width}}{marker}│",
            style=self._line_style(line),
            no_wrap=True,
            end="",
        )
        content_start = len(result.plain)
        result.append_text(self._syntax_text(line.content, lexer))
        self._apply_intraline(result, line, content_start, spans)
        return result

    def _syntax_text(self, content: str, lexer: Lexer | None) -> Text:
        safe = literal_text(content).plain
        if lexer is None:
            return Text(safe, no_wrap=True, end="")
        result = Text("", no_wrap=True, end="")
        for token_type, value in lex(safe, lexer):
            token = _syntax_token(token_type)
            style = None if token is None else self._style(token)
            result.append(value, style=style)
        return result

    def _apply_intraline(
        self,
        result: Text,
        line: _DiffLine,
        content_start: int,
        spans: dict[int, _IntralineSpans],
    ) -> None:
        if line.pair_id is None:
            return
        pair_spans = spans.get(line.pair_id)
        if pair_spans is None:
            return
        if line.kind == "removed":
            ranges = pair_spans.removed
            background_name = "talaria-diff-intraline-removed-background"
        else:
            ranges = pair_spans.added
            background_name = "talaria-diff-intraline-added-background"
        style = Style(bgcolor=self._color(background_name))
        for start, end in ranges:
            result.stylize(style, content_start + start, content_start + end)

    def _line_style(self, line: _DiffLine, *, selected: bool = False) -> Style:
        if line.kind == "added":
            return self._style(
                "talaria-diff-added",
                background="talaria-diff-added-background",
            )
        if line.kind == "removed":
            return self._style(
                "talaria-diff-removed",
                background="talaria-diff-removed-background",
            )
        if line.kind == "hunk":
            return self._style(
                "talaria-diff-hunk",
                background="talaria-diff-hunk-background",
                bold=selected and line.hunk_index == self.hunk_index,
            )
        if line.kind == "metadata":
            return self._style("talaria-diff-line-number")
        return self._style("talaria-diff-context")

    def _style(
        self,
        foreground: str,
        *,
        background: str | None = None,
        bold: bool = False,
    ) -> Style:
        return Style(
            color=self._color(foreground),
            bgcolor=None if background is None else self._color(background),
            bold=bold,
        )

    def _color(self, token: str) -> str:
        value = self.app.theme_variables.get(token)
        if value is not None:
            return value
        theme = self.app.current_theme
        fallback_attribute = {
            "talaria-diff-added": "success",
            "talaria-diff-removed": "error",
            "talaria-diff-hunk": "primary",
            "talaria-syntax-comment": "secondary",
            "talaria-syntax-keyword": "primary",
            "talaria-syntax-string": "success",
            "talaria-syntax-number": "warning",
            "talaria-syntax-function": "primary",
            "talaria-syntax-type": "secondary",
            "talaria-syntax-variable": "foreground",
            "talaria-syntax-operator": "accent",
            "talaria-syntax-constant": "warning",
        }.get(token, "foreground")
        fallback = getattr(theme, fallback_attribute, None)
        return fallback or theme.foreground or theme.primary

    def _pad(self, text: Text, width: int) -> None:
        missing = width - cell_len(text.plain)
        if missing > 0:
            text.append(" " * missing)


class DiffViewer(ModalScreen[None]):
    """Full-terminal read-only diff modal."""

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("escape", "close", "close", show=False),
        Binding("n", "next_hunk", "next hunk", show=False),
        Binding("p", "previous_hunk", "previous hunk", show=False),
        Binding("N", "next_file", "next file", show=False),
        Binding("P", "previous_file", "previous file", show=False),
        Binding("f", "file_list", "files", show=False),
        Binding("u", "unified", "unified", show=False),
        Binding("s", "side_by_side", "side-by-side", show=False),
    ]

    DEFAULT_CSS = """
    DiffViewer {
        align: center middle;
        background: $background;
    }
    DiffViewer > #diff-shell {
        width: 100%;
        height: 100%;
        border: solid $accent;
        background: $background;
    }
    DiffViewer .diff--header {
        width: 100%;
        height: 1;
        color: $text;
        text-style: bold;
    }
    DiffViewer .diff--columns {
        width: 100%;
        height: 1;
        color: $text-muted;
    }
    DiffViewer .diff--hint {
        width: 100%;
        height: 1;
        color: $text-muted;
    }
    """

    def __init__(
        self,
        document: DiffViewerDocument,
        *,
        file_key: str | None = None,
        hunk_index: int = 0,
    ) -> None:
        super().__init__()
        self._files = tuple(_parse_unified(file) for file in document.files)
        self.index_passes = len(self._files)
        self.file_index = self._index_for_key(file_key)
        self.hunk_index = max(0, hunk_index)
        self.preferred_mode: DiffMode = "side-by-side"
        self.effective_mode: DiffMode = "unified"
        self._refusal = ""
        self._header: Static | None = None
        self._columns: Static | None = None
        self._hint: Static | None = None
        self.canvas = DiffCanvas(self._files)

    def _index_for_key(self, key: str | None) -> int:
        if key is not None:
            for index, file in enumerate(self._files):
                if file.source.key == key:
                    return index
        return 0

    @property
    def header_text(self) -> str:
        return "" if self._header is None else str(self._header.content)

    @property
    def refusal_text(self) -> str:
        return self._refusal

    @property
    def active_file_key(self) -> str | None:
        if not self._files:
            return None
        return self._files[self.file_index].source.key

    @property
    def active_lexer(self) -> str:
        return self.canvas.active_lexer

    def compose(self) -> ComposeResult:
        with Vertical(id="diff-shell"):
            self._header = Static(
                literal_text(""), markup=False, classes="diff--header"
            )
            yield self._header
            self._columns = Static(
                literal_text(""), markup=False, classes="diff--columns"
            )
            yield self._columns
            yield self.canvas
            self._hint = Static(literal_text(""), markup=False, classes="diff--hint")
            yield self._hint

    def on_mount(self) -> None:
        self._settle_mode(self.size.width, preserve_anchor=False)
        self.canvas.focus()

    def on_resize(self, event: events.Resize) -> None:
        self._settle_mode(event.size.width, preserve_anchor=True)

    def _settle_mode(self, width: int, *, preserve_anchor: bool) -> None:
        effective: DiffMode = (
            self.preferred_mode
            if width >= SIDE_BY_SIDE_MIN_WIDTH
            else "unified"
        )
        if width >= SIDE_BY_SIDE_MIN_WIDTH:
            self._refusal = ""
        self.effective_mode = effective
        self.canvas.set_view(
            file_index=self.file_index,
            hunk_index=self.hunk_index,
            mode=effective,
            preserve_anchor=preserve_anchor,
        )
        self._repaint_chrome()

    def _repaint_chrome(self) -> None:
        if self._header is None or self._columns is None or self._hint is None:
            return
        if not self._files:
            header = "diff · no session-reported changes · read only"
            columns = ""
        else:
            file = self._files[self.file_index].source
            mode = self._refusal or self.effective_mode
            header = (
                f"diff · {self.file_index + 1}/{len(self._files)} "
                f"{file.path} · {mode} · read only"
            )
            columns = (
                "base · old │ working tree · new"
                if self.effective_mode == "side-by-side"
                else "old new"
            )
        hint_mode = (
            "u unified" if self.effective_mode == "side-by-side" else "s side-by-side"
        )
        hint = (
            f"[read only] n/p hunk · N/P file · {hint_mode} · f files · esc close"
        )
        self._header.update(literal_text(header))
        self._columns.update(literal_text(columns))
        self._hint.update(literal_text(hint))

    def _select(self, file_index: int, hunk_index: int, *, preserve_anchor: bool) -> None:
        if self._files:
            self.file_index = file_index % len(self._files)
            count = self._files[self.file_index].hunk_count
            self.hunk_index = 0 if count == 0 else hunk_index % count
        self._refusal = ""
        self.canvas.set_view(
            file_index=self.file_index,
            hunk_index=self.hunk_index,
            mode=self.effective_mode,
            preserve_anchor=preserve_anchor,
        )
        self._repaint_chrome()

    def action_close(self) -> None:
        self.dismiss(None)

    def action_next_hunk(self) -> None:
        self._select(self.file_index, self.hunk_index + 1, preserve_anchor=False)

    def action_previous_hunk(self) -> None:
        self._select(self.file_index, self.hunk_index - 1, preserve_anchor=False)

    def action_next_file(self) -> None:
        self._select(self.file_index + 1, 0, preserve_anchor=False)

    def action_previous_file(self) -> None:
        self._select(self.file_index - 1, 0, preserve_anchor=False)

    def action_file_list(self) -> None:
        if not self._files:
            return
        source = _FilePickerSource(self._files, self.file_index)
        self.app.push_screen(PickerDialog(source), self._file_picked)

    def _file_picked(self, key: str | None) -> None:
        if key is None:
            self.canvas.focus()
            return
        for index, file in enumerate(self._files):
            if file.source.key == key:
                self._select(index, 0, preserve_anchor=False)
                break
        self.canvas.focus()

    def action_unified(self) -> None:
        self.preferred_mode = "unified"
        self._refusal = ""
        self._settle_mode(self.size.width, preserve_anchor=True)

    def action_side_by_side(self) -> None:
        self.preferred_mode = "side-by-side"
        if self.size.width < SIDE_BY_SIDE_MIN_WIDTH:
            self._refusal = SIDE_BY_SIDE_REFUSAL
            self.effective_mode = "unified"
            self._repaint_chrome()
            return
        self._refusal = ""
        self._settle_mode(self.size.width, preserve_anchor=True)
