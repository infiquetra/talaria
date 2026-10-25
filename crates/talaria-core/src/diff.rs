//! A diff as files, hunks and rows. Issue 221. Styling lives elsewhere.
//!
//! One parser reads Hermes's rendered inline diff (a `<from> → <to>` line per file), a raw
//! unified diff, and `git diff` output. Hunk-header counts decide which lines are rows, so a
//! removed line whose text starts `-- ` is not mistaken for a file header.

/// One parsed diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffModel {
    /// Files in the order they were opened.
    pub files: Vec<DiffFile>,
    /// Lines that are not rows of any file, such as a binary notice.
    pub notes: Vec<String>,
}

/// One file inside a diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffFile {
    /// Path before the change. Absent for a new file.
    pub old_path: Option<String>,
    /// Path after the change. Absent for a deletion.
    pub new_path: Option<String>,
    /// Hunks in order.
    pub hunks: Vec<DiffHunk>,
}

/// One hunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffHunk {
    /// The whole `@@` line.
    pub header: String,
    /// Old start line when the header parsed.
    pub old_start: Option<u32>,
    /// New start line when the header parsed.
    pub new_start: Option<u32>,
    /// Rows of this hunk.
    pub rows: Vec<DiffRow>,
}

/// One row. `text` does not include the leading sign.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffRow {
    /// Added, removed, context or a note inside the hunk.
    pub kind: RowKind,
    /// Old line number when this row has one.
    pub old_no: Option<u32>,
    /// New line number when this row has one.
    pub new_no: Option<u32>,
    /// Row text without the sign character.
    pub text: String,
}

/// What a diff row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A line that was added.
    Added,
    /// A line that was removed.
    Removed,
    /// A line that is unchanged.
    Context,
    /// A note inside a hunk, such as "no newline".
    Note,
}

/// Counts of files and changed rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffStats {
    /// How many files the model holds.
    pub files: usize,
    /// Added rows.
    pub added: usize,
    /// Removed rows.
    pub removed: usize,
}

impl DiffModel {
    /// Added and removed rows, and the file count.
    #[must_use]
    pub fn stats(&self) -> DiffStats {
        let mut stats = DiffStats {
            files: self.files.len(),
            added: 0,
            removed: 0,
        };
        for row in self.rows() {
            match row.kind {
                RowKind::Added => stats.added += 1,
                RowKind::Removed => stats.removed += 1,
                RowKind::Context | RowKind::Note => {}
            }
        }
        stats
    }

    /// True when there is no file and no note.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.notes.is_empty()
    }

    /// The single file's new path, or its old path, without a leading `a/` or `b/`.
    #[must_use]
    pub fn display_path(&self) -> Option<&str> {
        let [file] = self.files.as_slice() else {
            return None;
        };
        file.display_path()
    }

    /// Rows, plus a header after the first hunk, plus a file row when there are several files.
    #[must_use]
    pub fn row_count(&self) -> usize {
        let rows = self.rows().count();
        let hunks: usize = self.files.iter().map(|file| file.hunks.len()).sum();
        let file_rows = if self.files.len() > 1 {
            self.files.len()
        } else {
            0
        };
        rows + hunks.saturating_sub(1) + file_rows
    }

    fn rows(&self) -> impl Iterator<Item = &DiffRow> {
        self.files
            .iter()
            .flat_map(|file| file.hunks.iter().flat_map(|hunk| hunk.rows.iter()))
    }
}

impl DiffFile {
    /// The new path, or the old one, without a leading `a/` or `b/`.
    #[must_use]
    pub fn display_path(&self) -> Option<&str> {
        let path = self.new_path.as_deref().or(self.old_path.as_deref())?;
        Some(
            path.strip_prefix("a/")
                .or_else(|| path.strip_prefix("b/"))
                .unwrap_or(path),
        )
    }
}

/// File metadata lines of `git diff` that carry nothing to draw.
const METADATA: [&str; 9] = [
    "index ",
    "new file mode ",
    "deleted file mode ",
    "old mode ",
    "new mode ",
    "similarity index ",
    "dissimilarity index ",
    "copy from ",
    "copy to ",
];

/// The open hunk's remaining old and new lines, or `None` when its header did not parse.
type Remaining = Option<(u32, u32)>;

struct Parser {
    model: DiffModel,
    /// A file opened by `diff --git` that has not yet had its `---`/`+++` or a hunk.
    git_header: bool,
    /// The path of the last `--- ` line outside a hunk.
    pending_old: Option<String>,
    /// Inside a hunk: its remaining counts, and the next old and new numbers.
    hunk: Option<(Remaining, Option<u32>, Option<u32>)>,
}

/// `/dev/null` and Hermes's `a/?` and `b/?` mean the file does not exist on that side.
fn side(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() || matches!(path, "/dev/null" | "a/?" | "b/?") {
        None
    } else {
        Some(path.to_owned())
    }
}

/// The path of a `--- ` or `+++ ` line: cut at the first tab, trimmed.
fn header_path(rest: &str) -> Option<String> {
    side(rest.split('\t').next().unwrap_or(rest))
}

/// `@@ -a[,b] +c[,d] @@`: the starts and the counts, a missing count meaning 1.
fn hunk_numbers(line: &str) -> Option<(u32, u32, u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let end = rest.find(" @@")?;
    let new = &rest[..end];
    let range = |text: &str| -> Option<(u32, u32)> {
        let (start, count) = match text.split_once(',') {
            Some((start, count)) => (start, Some(count)),
            None => (text, None),
        };
        let digits = |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
        if !digits(start) || count.is_some_and(|count| !digits(count)) {
            return None;
        }
        let start = start.parse().ok()?;
        let count = match count {
            Some(count) => count.parse().ok()?,
            None => 1,
        };
        Some((start, count))
    };
    let (old_start, old_count) = range(old)?;
    let (new_start, new_count) = range(new)?;
    Some((old_start, old_count, new_start, new_count))
}

impl Parser {
    fn new() -> Self {
        Self {
            model: DiffModel {
                files: Vec::new(),
                notes: Vec::new(),
            },
            git_header: false,
            pending_old: None,
            hunk: None,
        }
    }

    fn open_file(&mut self, old_path: Option<String>, new_path: Option<String>) {
        self.model.files.push(DiffFile {
            old_path,
            new_path,
            hunks: Vec::new(),
        });
        self.git_header = false;
        self.hunk = None;
    }

    fn push_row(&mut self, kind: RowKind, text: &str) {
        let Some((remaining, old_next, new_next)) = self.hunk.as_mut() else {
            return;
        };
        let (old_no, new_no) = match kind {
            RowKind::Added => (None, *new_next),
            RowKind::Removed => (*old_next, None),
            RowKind::Context => (*old_next, *new_next),
            RowKind::Note => (None, None),
        };
        let advance = |next: &mut Option<u32>| {
            *next = next.and_then(|number| number.checked_add(1));
        };
        match kind {
            RowKind::Added => advance(new_next),
            RowKind::Removed => advance(old_next),
            RowKind::Context => {
                advance(old_next);
                advance(new_next);
            }
            RowKind::Note => {}
        }
        if let Some((old_left, new_left)) = remaining.as_mut() {
            match kind {
                RowKind::Added => *new_left -= 1,
                RowKind::Removed => *old_left -= 1,
                RowKind::Context => {
                    *old_left -= 1;
                    *new_left -= 1;
                }
                RowKind::Note => {}
            }
        }
        let row = DiffRow {
            kind,
            old_no,
            new_no,
            text: text.to_owned(),
        };
        if let Some(hunk) = self
            .model
            .files
            .last_mut()
            .and_then(|file| file.hunks.last_mut())
        {
            hunk.rows.push(row);
        }
    }

    /// Try `line` as a row of the open hunk. False closes the hunk.
    fn hunk_line(&mut self, line: &str) -> bool {
        let Some((remaining, _, _)) = self.hunk else {
            return false;
        };
        let first = line.chars().next();
        match remaining {
            Some((old_left, new_left)) if old_left > 0 || new_left > 0 => match first {
                Some('-') if old_left > 0 => self.push_row(RowKind::Removed, &line[1..]),
                Some('+') if new_left > 0 => self.push_row(RowKind::Added, &line[1..]),
                Some(' ') if old_left > 0 && new_left > 0 => {
                    self.push_row(RowKind::Context, &line[1..]);
                }
                None if old_left > 0 && new_left > 0 => self.push_row(RowKind::Context, ""),
                Some('\\') => self.push_row(RowKind::Note, &line[1..]),
                _ => return false,
            },
            Some(_) => match first {
                Some('\\') => self.push_row(RowKind::Note, &line[1..]),
                _ => return false,
            },
            None => {
                if line.starts_with("--- ") || line.starts_with("+++ ") {
                    return false;
                }
                match first {
                    Some('-') => self.push_row(RowKind::Removed, &line[1..]),
                    Some('+') => self.push_row(RowKind::Added, &line[1..]),
                    Some(' ') => self.push_row(RowKind::Context, &line[1..]),
                    Some('\\') => self.push_row(RowKind::Note, &line[1..]),
                    None => self.push_row(RowKind::Context, ""),
                    Some(_) => return false,
                }
            }
        }
        true
    }

    fn open_hunk(&mut self, line: &str) {
        if self.model.files.is_empty() {
            self.open_file(None, None);
        }
        self.git_header = false;
        let numbers = hunk_numbers(line);
        let (old_start, new_start) = match numbers {
            Some((old_start, _, new_start, _)) => (Some(old_start), Some(new_start)),
            None => (None, None),
        };
        if let Some(file) = self.model.files.last_mut() {
            file.hunks.push(DiffHunk {
                header: line.to_owned(),
                old_start,
                new_start,
                rows: Vec::new(),
            });
        }
        self.hunk = Some((
            numbers.map(|(_, old_count, _, new_count)| (old_count, new_count)),
            old_start,
            new_start,
        ));
    }

    fn outside_line(&mut self, line: &str) {
        // Hermes's localised header line: dropped by its marker, never by its words.
        if self.model.files.is_empty() && line.trim_start().starts_with('\u{250a}') {
            return;
        }
        if line.starts_with("@@") {
            self.open_hunk(line);
        } else if let Some(rest) = line.strip_prefix("diff --git ") {
            let (old_path, new_path) = match rest.rfind(" b/") {
                Some(split) => (side(&rest[..split]), side(&rest[split + 1..])),
                None => (None, None),
            };
            self.open_file(old_path, new_path);
            self.git_header = true;
        } else if METADATA.iter().any(|prefix| line.starts_with(prefix)) {
        } else if let Some(rest) = line.strip_prefix("rename from ") {
            if let Some(file) = self.model.files.last_mut() {
                file.old_path = side(rest);
            }
        } else if let Some(rest) = line.strip_prefix("rename to ") {
            if let Some(file) = self.model.files.last_mut() {
                file.new_path = side(rest);
            }
        } else if let Some(rest) = line.strip_prefix("--- ") {
            self.pending_old = header_path(rest);
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            let old_path = self.pending_old.take();
            let new_path = header_path(rest);
            if self.git_header {
                if let Some(file) = self.model.files.last_mut() {
                    file.old_path = old_path;
                    file.new_path = new_path;
                }
                self.git_header = false;
            } else {
                self.open_file(old_path, new_path);
            }
        } else if let Some((old, new)) = line.split_once(" \u{2192} ") {
            self.open_file(side(old), side(new));
        } else if !line.trim().is_empty() {
            self.model.notes.push(line.to_owned());
        }
    }

    fn line(&mut self, line: &str) {
        if self.hunk.is_some() {
            if self.hunk_line(line) {
                return;
            }
            self.hunk = None;
        }
        self.outside_line(line);
    }
}

/// Read a Hermes diff, a unified diff, or `git diff` text. Never panics.
///
/// The text has its escape sequences removed already. Issue 221.
#[must_use]
pub fn parse_diff(text: &str) -> DiffModel {
    let mut parser = Parser::new();
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    for line in lines {
        parser.line(line.strip_suffix('\r').unwrap_or(line));
    }
    parser.model
}
