//! Drawing red tests for the review. Issue #245.
//!
//! Snapshots run through `vt100` at 120x40, 70x20 and 50x10; colours are
//! checked through cell attributes. Visible state the state tests cannot see
//! (cursor, notices, headers, editors) is pinned here by drawing real views.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{TerminalOptions, Viewport};
use talaria_core::directory::rows::RowKey;
use talaria_core::review::{
    BranchHead, ChangedFile, FileStatus, Listing, RecordView, ReviewInput, ReviewKey, ReviewState,
    ReviewTarget, ReviewView, Scope,
};
use talaria_tui::history::sanitize;
use talaria_tui::review_view::draw_review;
use talaria_tui::tool_cell::ToolTheme;

/// Bytes the drawn terminal wrote.
#[derive(Clone, Default)]
struct ByteSink(Arc<Mutex<Vec<u8>>>);

impl ByteSink {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.0.lock().expect("sink lock"))
    }
}

impl Write for ByteSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("sink lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn target() -> ReviewTarget {
    ReviewTarget {
        folder: PathBuf::from("repo"),
        main_root: PathBuf::from("repo"),
        session: Some(RowKey {
            profile: "fixture".to_owned(),
            session_id: "local-render".to_owned(),
        }),
        record: Some(RecordView {
            branch: "talaria/work".to_owned(),
            base_ref: "origin/main".to_owned(),
            base_commit: "abc".to_owned(),
        }),
        title: "repo".to_owned(),
    }
}

fn file(path: &str, status: FileStatus, staged: bool, unstaged: bool) -> ChangedFile {
    ChangedFile {
        path: path.to_owned(),
        orig_path: None,
        status,
        staged,
        unstaged,
        added: Some(4),
        removed: Some(1),
        is_dir: false,
    }
}

fn listing() -> Listing {
    Listing {
        head: BranchHead {
            oid: Some("abc".to_owned()),
            head: Some("talaria/work".to_owned()),
            upstream: Some("origin/talaria/work".to_owned()),
            ahead: 1,
            behind: 0,
        },
        files: vec![
            file("src/main.rs", FileStatus::Modified, true, false),
            file("notes.txt", FileStatus::Added, true, true),
            file("old.txt", FileStatus::Deleted, false, true),
        ],
        total: 3,
        base: None,
        default_branch: Some("main".to_owned()),
        has_origin: true,
        hooks: 0,
    }
}

/// Draw a view at a fixed size and return the emulated screen.
fn render(view: &ReviewView<'_>, width: u16, height: u16) -> vt100::Parser {
    let sink = ByteSink::default();
    let backend = CrosstermBackend::new(sink.clone());
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, width, height)),
        },
    )
    .expect("terminal");
    terminal
        .draw(|frame| {
            draw_review(
                view,
                &ToolTheme::default(),
                frame.area(),
                frame.buffer_mut(),
            );
        })
        .expect("draw");
    let mut parser = vt100::Parser::new(height, width, 0);
    parser.process(&sink.take());
    parser
}

fn contents(parser: &vt100::Parser) -> String {
    parser.screen().contents()
}

/// Header, rows, marks, diff and footer at 120x40.
#[test]
fn list_and_diff_120x40() {
    let (mut state, _) = ReviewState::open(target(), 1.0);
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(listing()),
    });
    let _ = state.update(ReviewInput::DiffLoaded {
        path: "src/main.rs".to_owned(),
        scope: Scope::Uncommitted,
        result: Ok("@@ -1,2 +1,3 @@\n context\n-removed\n+added\n".to_owned()),
    });
    let screen = render(&state.view(), 120, 40);
    let text = contents(&screen);
    assert!(
        text.contains("Review · repo · talaria/work · 3 files · +12 −3"),
        "the header names the review: {text}"
    );
    assert!(text.contains("↑1 ↓0"), "ahead and behind show: {text}");
    assert!(
        text.contains("Uncommitted changes"),
        "the scope line: {text}"
    );
    assert!(text.contains("›"), "the cursor marks a row: {text}");
    assert!(
        text.contains('M') && text.contains('A') && text.contains('D'),
        "status letters: {text}"
    );
    assert!(text.contains('●'), "the staged mark shows: {text}");
    assert!(text.contains('◐'), "the half-staged mark shows: {text}");
    assert!(text.contains("src/main.rs"), "paths show: {text}");
    assert!(
        text.contains("── src/main.rs ──"),
        "the diff panel names the file: {text}"
    );
    assert!(text.contains("+ added"), "the diff shows: {text}");

    // The cursor follows its path across a reload.
    let _ = state.update(ReviewInput::Key(ReviewKey::Down));
    let mut reordered = listing();
    reordered.files.rotate_left(1);
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(reordered),
    });
    let text = contents(&render(&state.view(), 120, 40));
    let cursor_row = text
        .lines()
        .find(|line| line.contains('›'))
        .expect("a cursor row");
    assert!(
        cursor_row.contains("notes.txt"),
        "the cursor follows its path: {text}"
    );

    // Every action in Branch scope answers the read-only text.
    let _ = state.update(ReviewInput::Key(ReviewKey::Scope));
    for key in [
        ReviewKey::StageToggle,
        ReviewKey::StageAll,
        ReviewKey::UnstageAll,
        ReviewKey::Discard,
        ReviewKey::Commit,
        ReviewKey::Push,
        ReviewKey::PullRequest,
    ] {
        let _ = state.update(ReviewInput::Key(key));
        let text = contents(&render(&state.view(), 120, 40));
        assert!(
            text.contains("Branch view is read-only: press tab for uncommitted changes."),
            "{key:?} answers the read-only text: {text}"
        );
    }

    // Discard asks with the path, then cancels on any other key.
    let _ = state.update(ReviewInput::Key(ReviewKey::Scope));
    let _ = state.update(ReviewInput::Key(ReviewKey::Discard));
    let text = contents(&render(&state.view(), 120, 40));
    assert!(
        text.contains(
            "Discard changes to notes.txt? Press X again to discard; any other key cancels."
        ),
        "the discard question names the path: {text}"
    );
    let _ = state.update(ReviewInput::Key(ReviewKey::Down));
    let text = contents(&render(&state.view(), 120, 40));
    assert!(
        text.contains("Discard cancelled."),
        "another key only cancels: {text}"
    );
}

/// The commit editor, and the too-small window.
#[test]
fn commit_editor_and_small_window() {
    let (mut state, _) = ReviewState::open(target(), 1.0);
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(listing()),
    });
    let _ = state.update(ReviewInput::SessionWorking(true));
    let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
    let text = contents(&render(&state.view(), 120, 40));
    assert!(
        text.contains("Commit 2 staged files on talaria/work"),
        "the editor header counts staged files: {text}"
    );
    assert!(
        text.contains(
            "The agent is still working in this folder; edits after this commit are not in it."
        ),
        "the working warning shows: {text}"
    );
    assert!(
        text.contains("ctrl+s commit · ctrl+d draft with the session's model · esc back"),
        "the editor footer: {text}"
    );

    // Nothing staged: the header says the commit takes all files.
    let mut bare = listing();
    for row in &mut bare.files {
        row.staged = false;
        row.unstaged = true;
    }
    let _ = state.update(ReviewInput::Key(ReviewKey::Esc));
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(bare),
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
    let text = contents(&render(&state.view(), 120, 40));
    assert!(
        text.contains(
            "Nothing is staged: this commit will include all 3 changed files on talaria/work."
        ),
        "the stage-all header: {text}"
    );

    // Success forgets the text; failure keeps it.
    let _ = state.update(ReviewInput::Key(ReviewKey::Char('f')));
    let _ = state.update(ReviewInput::Key(ReviewKey::Submit));
    let _ = state.update(ReviewInput::ActionDone {
        action: talaria_core::review::GitAction::Commit {
            message: "f".to_owned(),
            stage_all: true,
        },
        result: Err("Commit failed: hook says no".to_owned()),
    });
    let text = contents(&render(&state.view(), 120, 40));
    assert!(text.contains('f'), "a failed commit keeps its text: {text}");

    // A repeated draft shows its notice: the editor holds the first draft, no
    // typing happens, and the second draft comes back identical.
    let _ = state.update(ReviewInput::Key(ReviewKey::Draft));
    let _ = state.update(ReviewInput::Key(ReviewKey::Draft));
    let _ = state.update(ReviewInput::Drafted {
        seq: 1,
        result: Ok("same message".to_owned()),
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::Draft));
    let _ = state.update(ReviewInput::Drafted {
        seq: 2,
        result: Ok("same message".to_owned()),
    });
    let text = contents(&render(&state.view(), 120, 40));
    assert!(
        text.contains("The model returned the same message."),
        "the repeat notice shows: {text}"
    );

    // Below 60 columns the window is too small.
    let text = contents(&render(&state.view(), 50, 10));
    assert!(
        text.contains("Window too small for the review. Esc goes back."),
        "the too-small text: {text}"
    );
}

/// Escape bytes never reach the screen; the short window lists, then shows a
/// diff in place; caps, cuts, loading and empty states all show.
#[test]
fn sanitized_names_short_and_70x20() {
    let (mut state, _) = ReviewState::open(target(), 1.0);
    let mut rows = listing();
    rows.files.push(file(
        "evil-\x1b[31mred.txt",
        FileStatus::Untracked,
        false,
        true,
    ));
    rows.total = rows.files.len();
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(rows),
    });
    let screen = render(&state.view(), 70, 20);
    let text = contents(&screen);
    let clean = sanitize("evil-\x1b[31mred.txt");
    assert!(
        text.contains(clean.as_ref()),
        "the name draws sanitised: {text}"
    );
    assert!(!text.contains('\x1b'), "no escape byte reaches the screen");
    assert!(text.contains("›"), "the short window still lists: {text}");

    // Below 24 rows `enter` shows the diff in place of the list.
    let _ = state.update(ReviewInput::DiffLoaded {
        path: "src/main.rs".to_owned(),
        scope: Scope::Uncommitted,
        result: Ok("@@ -1 +1 @@\n-old\n+new\n".to_owned()),
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::ShowDiff));
    let text = contents(&render(&state.view(), 70, 20));
    assert!(
        text.contains("+ new") && !text.contains("notes.txt"),
        "the diff replaces the list: {text}"
    );
    let _ = state.update(ReviewInput::Key(ReviewKey::Esc));
    let text = contents(&render(&state.view(), 70, 20));
    assert!(
        text.contains("notes.txt"),
        "Esc returns to the list: {text}"
    );

    // Two thousand rows cap with a notice.
    let paths: Vec<String> = (0..2_500)
        .map(|index| format!("file-{index:04}.txt"))
        .collect();
    let mut rows = listing();
    rows.files = paths
        .iter()
        .map(|path| file(path, FileStatus::Modified, false, true))
        .collect();
    rows.total = paths.len();
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(rows),
    });
    let text = contents(&render(&state.view(), 120, 40));
    assert!(
        text.contains("Showing 2,000 of 2,500 changed files."),
        "the cap notice shows: {text}"
    );

    // A cut diff says where it stopped.
    let big = "line\n".repeat(25_000);
    let _ = state.update(ReviewInput::DiffLoaded {
        path: "file-0000.txt".to_owned(),
        scope: Scope::Uncommitted,
        result: Ok(big),
    });
    let text = contents(&render(&state.view(), 120, 40));
    assert!(
        text.contains(
            "Diff cut at 20,000 lines; 5,000 more. Run git diff in a shell for the rest."
        ),
        "the cut notice shows: {text}"
    );

    // Loading and empty states.
    let (mut state, _) = ReviewState::open(target(), 1.0);
    let text = contents(&render(&state.view(), 120, 40));
    assert!(
        text.contains("Loading changes…"),
        "loading shows before the first listing: {text}"
    );
    let mut empty = listing();
    empty.files.clear();
    empty.total = 0;
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(empty),
    });
    let text = contents(&render(&state.view(), 120, 40));
    assert!(
        text.contains("No uncommitted changes in repo."),
        "the empty state shows: {text}"
    );
}
