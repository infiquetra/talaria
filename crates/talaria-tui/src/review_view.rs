//! Draw the review overlay: a header, the file list, the diff panel or the commit editor, the
//! notice line and the footer. Reads the view by reference and writes only into the buffer.
//! Issue #245.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Widget};
use talaria_core::review::{
    BranchHead, ChangedFile, DiffContent, EditorView, FileStatus, LabelTone, PrLabel,
    ReviewDecision, ReviewFocus, ReviewView, Scope,
};

use crate::diff_view::{DiffRenderOptions, diff_lines};
use crate::history::sanitize;
use crate::tool_cell::{ToolTheme, WrapRows, cut_to, text_width};

/// The narrowest window that draws the review.
const MIN_WIDTH: u16 = 60;
/// The shortest window that draws the review.
const MIN_HEIGHT: u16 = 12;
/// From this height the diff panel takes the lower half; below it the diff replaces the list.
const SPLIT_HEIGHT: u16 = 24;
/// The fewest rows the message area of the commit editor gets.
const MIN_EDITOR_ROWS: usize = 5;
/// Shown when the window is too small to draw the review.
const TOO_SMALL: &str = "Window too small for the review. Esc goes back.";
/// Shown before the first listing arrives.
const LOADING: &str = "Loading changes\u{2026}";

/// Draw the review into `buf`.
pub fn draw_review(view: &ReviewView<'_>, theme: &ToolTheme, area: Rect, buf: &mut Buffer) {
    Clear.render(area, buf);
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        put(buf, area.x, area.y, TOO_SMALL, area.width, theme.meta);
        return;
    }
    let width = area.width;
    put_line(buf, area.x, area.y, &header_line(view, theme, width), width);
    put(
        buf,
        area.x,
        area.y + 1,
        &scope_text(view),
        width,
        theme.subject,
    );
    let footer_y = area.y + area.height - 1;
    let notice_y = footer_y - 1;
    if let Some(notice) = &view.notice {
        put(
            buf,
            area.x,
            notice_y,
            &sanitize(notice),
            width,
            theme.output,
        );
    }
    put(buf, area.x, footer_y, &view.footer, width, theme.meta);

    let body = Rect::new(area.x, area.y + 2, width, notice_y - area.y - 2);
    if let Some(editor) = &view.editor {
        draw_editor(editor, theme, body, buf);
        return;
    }
    if !view.loaded {
        put(buf, body.x, body.y, LOADING, body.width, theme.meta);
        return;
    }
    if view.files.is_empty() {
        put(
            buf,
            body.x,
            body.y,
            &empty_text(view),
            body.width,
            theme.meta,
        );
        return;
    }
    let tall = area.height >= SPLIT_HEIGHT;
    if *view.focus == ReviewFocus::DiffOnly && !tall {
        draw_diff_panel(view, theme, body, buf);
    } else if tall {
        let list_height = body.height / 2;
        let list = Rect::new(body.x, body.y, body.width, list_height);
        let panel = Rect::new(
            body.x,
            body.y + list_height,
            body.width,
            body.height - list_height,
        );
        draw_list(view, theme, list, buf);
        draw_diff_panel(view, theme, panel, buf);
    } else {
        draw_list(view, theme, body, buf);
    }
}

/// Draw one pull-request label, cut to `width` columns.
pub fn draw_pr_label(label: &PrLabel, width: u16) -> Span<'static> {
    let style = match label.tone {
        LabelTone::Success => Style::default().fg(Color::Green),
        LabelTone::Error => Style::default().fg(Color::Red),
        LabelTone::Warning => Style::default().fg(Color::Yellow),
        LabelTone::Muted => Style::default().add_modifier(Modifier::DIM),
        LabelTone::Accent => Style::default().fg(Color::Magenta),
    };
    Span::styled(cut_to(&label.text, usize::from(width)), style)
}

/// Write `text` at the position, cut to `width` columns.
fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, width: u16, style: Style) {
    buf.set_stringn(
        x,
        y,
        cut_to(text, usize::from(width)),
        usize::from(width),
        style,
    );
}

/// Write a styled line at the position, cut to `width` columns.
fn put_line(buf: &mut Buffer, x: u16, y: u16, line: &Line<'_>, width: u16) {
    buf.set_line(x, y, line, width);
}

fn short_oid(oid: &str) -> &str {
    oid.get(..7).unwrap_or(oid)
}

fn branch_text(head: &BranchHead) -> String {
    match (&head.head, &head.oid) {
        (Some(name), _) => name.clone(),
        (None, Some(oid)) => format!("detached at {}", short_oid(oid)),
        (None, None) => "detached".to_owned(),
    }
}

fn files_text(count: usize) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

fn header_line(view: &ReviewView<'_>, theme: &ToolTheme, width: u16) -> Line<'static> {
    let title = sanitize(view.title);
    let left = match view.head {
        Some(head) => format!(
            "Review \u{b7} {title} \u{b7} {} \u{b7} {} \u{b7} +{} \u{2212}{}",
            sanitize(&branch_text(head)),
            files_text(view.total),
            view.added,
            view.removed
        ),
        None => format!("Review \u{b7} {title}"),
    };
    let right = view.head.map(|head| {
        if head.upstream.is_some() {
            format!("\u{2191}{} \u{2193}{}", head.ahead, head.behind)
        } else {
            "not on origin".to_owned()
        }
    });
    let label_width = |label: &PrLabel| text_width(&label.text) + 1;
    let right_width = right.as_ref().map_or(0, |text| text_width(text) + 1)
        + view.pr.as_ref().map_or(0, label_width);
    let room = usize::from(width).saturating_sub(right_width + 1);
    let left = cut_to(&left, room);
    let pad = usize::from(width).saturating_sub(text_width(&left) + right_width);
    let mut spans = vec![Span::styled(left, theme.title), Span::raw(" ".repeat(pad))];
    if let Some(right) = right {
        spans.push(Span::styled(right, theme.meta));
    }
    if let Some(label) = &view.pr {
        spans.push(Span::raw(" "));
        spans.push(draw_pr_label(label, width));
    }
    Line::from(spans)
}

fn decision_text(decision: ReviewDecision) -> &'static str {
    match decision {
        ReviewDecision::Approved => "approved",
        ReviewDecision::ChangesRequested => "changes requested",
        ReviewDecision::ReviewRequired => "review required",
    }
}

fn scope_text(view: &ReviewView<'_>) -> String {
    let mut text = match view.scope {
        Scope::Uncommitted => "Uncommitted changes".to_owned(),
        Scope::Branch => match (view.base, view.base_ref) {
            (Some(base), Some(reference)) => format!(
                "Branch changes since {} ({})",
                short_oid(base),
                sanitize(reference)
            ),
            (Some(base), None) => format!("Branch changes since {}", short_oid(base)),
            _ => "Branch changes".to_owned(),
        },
    };
    if view.scope == Scope::Branch {
        text.push_str(" \u{b7} read-only");
    }
    if let Some(decision) = view.review {
        text.push_str(" \u{b7} review: ");
        text.push_str(decision_text(decision));
    }
    text
}

fn empty_text(view: &ReviewView<'_>) -> String {
    match view.scope {
        Scope::Uncommitted => format!(
            "No uncommitted changes in {}.",
            sanitize(&view.folder.display().to_string())
        ),
        Scope::Branch => match view.base {
            None => "No base to compare with: origin has no default branch here.".to_owned(),
            Some(base) => {
                let branch = view
                    .head
                    .map_or_else(|| "this branch".to_owned(), branch_text);
                format!(
                    "No commits on {} since {}.",
                    sanitize(&branch),
                    short_oid(base)
                )
            }
        },
    }
}

fn status_letter(status: FileStatus) -> char {
    match status {
        FileStatus::Modified => 'M',
        FileStatus::Added => 'A',
        FileStatus::Deleted => 'D',
        FileStatus::Renamed => 'R',
        FileStatus::Copied => 'C',
        FileStatus::TypeChanged => 'T',
        FileStatus::Untracked => '?',
        FileStatus::Conflicted => 'U',
    }
}

fn letter_style(status: FileStatus, theme: &ToolTheme) -> Style {
    match status {
        FileStatus::Added | FileStatus::Untracked => theme.glyph_ok,
        FileStatus::Deleted | FileStatus::Conflicted => theme.glyph_failed,
        _ => theme.subject,
    }
}

fn mark(file: &ChangedFile) -> char {
    match (file.staged, file.unstaged) {
        (true, true) => '\u{25d0}',
        (true, false) => '\u{25cf}',
        _ => ' ',
    }
}

fn counts_text(file: &ChangedFile) -> String {
    if file.is_dir {
        return String::new();
    }
    match (file.added, file.removed) {
        (Some(added), Some(removed)) => format!("+{added} \u{2212}{removed}"),
        _ if file.status == FileStatus::Untracked => "+?".to_owned(),
        _ => "bin".to_owned(),
    }
}

fn path_text(file: &ChangedFile) -> String {
    let mut path = match (&file.orig_path, file.status) {
        (Some(old), FileStatus::Renamed | FileStatus::Copied) => {
            format!("{old} \u{2192} {}", file.path)
        }
        _ => file.path.clone(),
    };
    if file.is_dir && !path.ends_with('/') {
        path.push('/');
    }
    sanitize(&path).into_owned()
}

fn draw_list(view: &ReviewView<'_>, theme: &ToolTheme, area: Rect, buf: &mut Buffer) {
    let height = usize::from(area.height);
    if height == 0 {
        return;
    }
    let cursor = view.cursor.unwrap_or(0);
    let start = if cursor >= height {
        cursor + 1 - height
    } else {
        0
    };
    let width = usize::from(area.width);
    for (offset, file) in view.files.iter().skip(start).take(height).enumerate() {
        let selected = view.cursor == Some(start + offset);
        let y = area.y + u16::try_from(offset).unwrap_or(0);
        let counts = counts_text(file);
        let counts_width = text_width(&counts);
        let path_room = width.saturating_sub(5 + counts_width + usize::from(counts_width > 0));
        let path = cut_to(&path_text(file), path_room);
        let path_style = if selected {
            theme.subject.add_modifier(Modifier::BOLD)
        } else {
            theme.subject
        };
        let used = 5 + text_width(&path);
        let pad = width.saturating_sub(used + counts_width);
        let line = Line::from(vec![
            Span::styled(if selected { "\u{203a} " } else { "  " }, theme.title),
            Span::styled(
                status_letter(file.status).to_string(),
                letter_style(file.status, theme),
            ),
            Span::styled(mark(file).to_string(), theme.glyph_ok),
            Span::raw(" "),
            Span::styled(path, path_style),
            Span::raw(" ".repeat(pad)),
            Span::styled(counts, theme.meta),
        ]);
        put_line(buf, area.x, y, &line, area.width);
    }
}

fn draw_diff_panel(view: &ReviewView<'_>, theme: &ToolTheme, area: Rect, buf: &mut Buffer) {
    if area.height == 0 {
        return;
    }
    if let Some(file) = view.cursor.and_then(|index| view.files.get(index)) {
        let rule = format!("\u{2500}\u{2500} {} \u{2500}\u{2500}", sanitize(&file.path));
        put(buf, area.x, area.y, &rule, area.width, theme.meta);
    }
    let body = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    );
    if body.height == 0 {
        return;
    }
    match &view.diff {
        DiffContent::Empty => {}
        DiffContent::Message(text) => {
            put(buf, body.x, body.y, &sanitize(text), body.width, theme.meta)
        }
        DiffContent::Diff {
            model,
            scroll,
            cut_note,
            ..
        } => {
            let note_rows = u16::from(cut_note.is_some()).min(body.height);
            let rows = usize::from(body.height - note_rows);
            let options = DiffRenderOptions {
                width: body.width,
                indent: 0,
                max_rows: Some(scroll + rows),
                theme: theme.clone(),
                color: true,
            };
            let (mut lines, more) = diff_lines(model, &options);
            if more == 0 {
                let room = (scroll + rows).saturating_sub(lines.len());
                for note in model.notes.iter().take(room) {
                    lines.push(Line::styled(
                        cut_to(&sanitize(note), usize::from(body.width)),
                        theme.diff_note,
                    ));
                }
            }
            for (offset, line) in lines.iter().skip(*scroll).take(rows).enumerate() {
                let y = body.y + u16::try_from(offset).unwrap_or(0);
                put_line(buf, body.x, y, line, body.width);
            }
            if let Some(note) = cut_note {
                put(
                    buf,
                    body.x,
                    body.y + body.height - 1,
                    note,
                    body.width,
                    theme.diff_note,
                );
            }
        }
    }
}

fn draw_editor(editor: &EditorView<'_>, theme: &ToolTheme, area: Rect, buf: &mut Buffer) {
    let mut y = area.y;
    let bottom = area.y + area.height;
    let header_style = theme.title.add_modifier(Modifier::BOLD);
    put(
        buf,
        area.x,
        y,
        &sanitize(&editor.header),
        area.width,
        header_style,
    );
    y += 1;
    for warning in &editor.warnings {
        // Keep room for the message area; the header always shows.
        if usize::from(bottom.saturating_sub(y)) <= MIN_EDITOR_ROWS {
            break;
        }
        put(buf, area.x, y, &sanitize(warning), area.width, theme.error);
        y += 1;
    }
    let rows = usize::from(bottom.saturating_sub(y));
    if rows == 0 {
        return;
    }
    let avail = usize::from(area.width);
    let mut wrapped: Vec<String> = Vec::new();
    for line in editor.text.split('\n') {
        let line = sanitize(line);
        for piece in WrapRows::new(&line, avail.saturating_sub(1).max(1)) {
            wrapped.push(piece.into_owned());
        }
    }
    let first = wrapped.len().saturating_sub(rows);
    for (offset, text) in wrapped.iter().skip(first).enumerate() {
        let row = y + u16::try_from(offset).unwrap_or(0);
        put(buf, area.x, row, text, area.width, theme.output);
    }
    // A reversed blank marks where typing lands.
    let last = wrapped.len().saturating_sub(first + 1);
    let caret_x =
        area.x + u16::try_from(wrapped.last().map_or(0, |text| text_width(text))).unwrap_or(0);
    let caret_y = y + u16::try_from(last).unwrap_or(0);
    if !editor.drafting && caret_x < area.x + area.width && caret_y < bottom {
        buf[(caret_x, caret_y)].set_style(Style::default().add_modifier(Modifier::REVERSED));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use talaria_core::review::{
        BranchHead, GitAction, Listing, RecordView, ReviewInput, ReviewKey, ReviewState,
        ReviewTarget,
    };

    use super::*;

    fn target() -> ReviewTarget {
        ReviewTarget {
            folder: PathBuf::from("repo"),
            main_root: PathBuf::from("repo"),
            session: None,
            record: Some(RecordView {
                branch: "feature".to_owned(),
                base_ref: "origin/main".to_owned(),
                base_commit: "0123456789abcdef".to_owned(),
            }),
            title: "repo".to_owned(),
        }
    }

    fn row(path: &str, status: FileStatus) -> ChangedFile {
        ChangedFile {
            path: path.to_owned(),
            orig_path: None,
            status,
            staged: false,
            unstaged: true,
            added: Some(2),
            removed: Some(1),
            is_dir: false,
        }
    }

    fn listing(files: Vec<ChangedFile>, upstream: bool) -> Listing {
        Listing {
            head: BranchHead {
                oid: Some("fedcba9876".to_owned()),
                head: Some("feature".to_owned()),
                upstream: upstream.then(|| "origin/feature".to_owned()),
                ahead: 3,
                behind: 1,
            },
            total: files.len(),
            files,
            base: Some("0123456789abcdef".to_owned()),
            default_branch: Some("main".to_owned()),
            has_origin: true,
            hooks: 0,
        }
    }

    fn state_with(files: Vec<ChangedFile>) -> ReviewState {
        let (mut state, _) = ReviewState::open(target(), 1.0);
        let _ = state.update(ReviewInput::Listed {
            scope: Scope::Uncommitted,
            result: Ok(listing(files, false)),
        });
        state
    }

    fn screen(state: &ReviewState, width: u16, height: u16) -> Vec<String> {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        draw_review(&state.view(), &ToolTheme::default(), area, &mut buf);
        (0..height)
            .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect()
    }

    #[test]
    fn labels_cut_and_take_their_tone() {
        let label = PrLabel {
            text: "#12 draft \u{2713}".to_owned(),
            tone: LabelTone::Success,
        };
        let span = draw_pr_label(&label, 40);
        assert_eq!(span.content, "#12 draft \u{2713}");
        assert_eq!(span.style.fg, Some(Color::Green));
        assert_eq!(draw_pr_label(&label, 6).content.chars().count(), 6);
        assert_eq!(draw_pr_label(&label, 0).content, "");
        for (tone, color) in [
            (LabelTone::Error, Some(Color::Red)),
            (LabelTone::Warning, Some(Color::Yellow)),
            (LabelTone::Accent, Some(Color::Magenta)),
            (LabelTone::Muted, None),
        ] {
            let span = draw_pr_label(
                &PrLabel {
                    text: "x".to_owned(),
                    tone,
                },
                4,
            );
            assert_eq!(span.style.fg, color, "{tone:?}");
        }
    }

    #[test]
    fn a_small_window_says_so_even_at_zero_files() {
        let state = state_with(vec![]);
        let lines = screen(&state, 59, 20);
        assert!(lines[0].starts_with("Window too small for the review. Esc goes back."));
        let lines = screen(&state, 80, 11);
        assert!(lines[0].starts_with("Window too small"));
    }

    #[test]
    fn rows_show_letters_marks_counts_and_renames() {
        let mut renamed = row("new.rs", FileStatus::Renamed);
        renamed.orig_path = Some("old.rs".to_owned());
        renamed.staged = true;
        renamed.unstaged = false;
        let mut binary = row("logo.png", FileStatus::Modified);
        binary.added = None;
        binary.removed = None;
        let mut untracked = row("big.log", FileStatus::Untracked);
        untracked.added = None;
        untracked.removed = None;
        let mut folder = row("target/", FileStatus::Untracked);
        folder.is_dir = true;
        folder.added = None;
        folder.removed = None;
        let mut bare_folder = row("cache", FileStatus::Untracked);
        bare_folder.is_dir = true;
        let state = state_with(vec![
            renamed,
            binary,
            untracked,
            folder,
            bare_folder,
            row("clash.rs", FileStatus::Conflicted),
            row("t.rs", FileStatus::TypeChanged),
            row("c.rs", FileStatus::Copied),
        ]);
        let text = screen(&state, 100, 30).join("\n");
        assert!(text.contains("R\u{25cf} old.rs \u{2192} new.rs"), "{text}");
        assert!(text.contains("bin"), "{text}");
        assert!(text.contains("+?"), "{text}");
        assert!(text.contains("target/"), "{text}");
        assert!(text.contains("cache/"), "{text}");
        for letter in ["U ", "T ", "C "] {
            assert!(text.contains(letter), "{letter}: {text}");
        }
    }

    #[test]
    fn a_long_list_scrolls_to_keep_the_cursor_visible() {
        let files: Vec<ChangedFile> = (0..60)
            .map(|n| row(&format!("f{n:02}.rs"), FileStatus::Modified))
            .collect();
        let mut state = state_with(files);
        let _ = state.update(ReviewInput::Key(ReviewKey::Bottom));
        let text = screen(&state, 100, 40).join("\n");
        assert!(text.contains("\u{203a} M  f59.rs"), "{text}");
        assert!(!text.contains("f00.rs"), "{text}");
    }

    #[test]
    fn branch_scope_names_its_base_and_empty_states() {
        let mut state = state_with(vec![row("a", FileStatus::Modified)]);
        let _ = state.update(ReviewInput::Key(ReviewKey::Scope));
        let _ = state.update(ReviewInput::Listed {
            scope: Scope::Branch,
            result: Ok(listing(vec![], true)),
        });
        let lines = screen(&state, 100, 30);
        assert!(lines[0].contains("\u{2191}3 \u{2193}1"), "{}", lines[0]);
        assert_eq!(
            lines[1].trim_end(),
            "Branch changes since 0123456 (origin/main) \u{b7} read-only"
        );
        assert!(
            lines
                .join("\n")
                .contains("No commits on feature since 0123456."),
            "{lines:?}"
        );
        let mut baseless = listing(vec![], true);
        baseless.base = None;
        let _ = state.update(ReviewInput::Listed {
            scope: Scope::Branch,
            result: Ok(baseless),
        });
        let text = screen(&state, 100, 30).join("\n");
        assert!(text.contains("No base to compare with: origin has no default branch here."));
    }

    #[test]
    fn the_editor_shows_warnings_text_and_a_caret() {
        let mut files = vec![row("a", FileStatus::Modified)];
        files[0].staged = true;
        let mut rows = listing(files, false);
        rows.hooks = 1;
        let (mut state, _) = ReviewState::open(target(), 1.0);
        let _ = state.update(ReviewInput::Listed {
            scope: Scope::Uncommitted,
            result: Ok(rows),
        });
        let _ = state.update(ReviewInput::SessionWorking(true));
        let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
        for c in "feat: x".chars() {
            let _ = state.update(ReviewInput::Key(ReviewKey::Char(c)));
        }
        let _ = state.update(ReviewInput::Key(ReviewKey::Newline));
        let _ = state.update(ReviewInput::Paste("a very long line ".repeat(8)));
        let text = screen(&state, 80, 20).join("\n");
        assert!(text.contains("Commit 1 staged file on feature"), "{text}");
        assert!(
            text.contains("Hooks: 1 active (they run as in your shell)."),
            "{text}"
        );
        assert!(text.contains("feat: x"), "{text}");
        assert!(text.contains("a very long line"), "{text}");
        let _ = state.update(ReviewInput::Key(ReviewKey::Submit));
        let _ = state.update(ReviewInput::ActionDone {
            action: GitAction::Push,
            result: Err("Push failed: no".to_owned()),
        });
        assert!(
            screen(&state, 80, 20)
                .join("\n")
                .contains("Push failed: no")
        );
    }

    #[test]
    fn a_pull_request_label_and_decision_show_in_the_header() {
        use talaria_core::review::ReviewDecision;
        let state = state_with(vec![row("a", FileStatus::Modified)]);
        let mut view = state.view();
        view.pr = Some(PrLabel {
            text: "#7 draft \u{2713}".to_owned(),
            tone: LabelTone::Success,
        });
        view.review = Some(ReviewDecision::ChangesRequested);
        let area = Rect::new(0, 0, 120, 30);
        let mut buf = Buffer::empty(area);
        draw_review(&view, &ToolTheme::default(), area, &mut buf);
        let line = |y: u16| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>();
        assert!(
            line(0).trim_end().ends_with("#7 draft \u{2713}"),
            "{}",
            line(0)
        );
        assert!(line(1).contains("review: changes requested"), "{}", line(1));
    }

    #[test]
    fn a_diff_scrolls_and_notes_follow_the_rows() {
        let mut state = state_with(vec![row("a", FileStatus::Modified)]);
        let _ = state.update(ReviewInput::DiffLoaded {
            path: "a".to_owned(),
            scope: Scope::Uncommitted,
            result: Ok("Binary files a/a and b/a differ\n".to_owned()),
        });
        let text = screen(&state, 100, 30).join("\n");
        assert!(text.contains("Binary files a/a and b/a differ"), "{text}");
        let _ = state.update(ReviewInput::DiffLoaded {
            path: "a".to_owned(),
            scope: Scope::Uncommitted,
            result: Ok(format!("@@ -1,40 +1,40 @@\n{}", " row\n".repeat(40))),
        });
        let _ = state.update(ReviewInput::Key(ReviewKey::PageDown));
        let _ = state.update(ReviewInput::Key(ReviewKey::PageDown));
        let text = screen(&state, 100, 30).join("\n");
        assert!(text.contains("row"), "{text}");
    }
}
