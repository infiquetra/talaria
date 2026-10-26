//! Multi-line draft editor. It writes no escape sequence and calls no gateway.

mod buffer;
pub mod paste_burst;
pub mod recall;
mod render;
pub mod wrap;

pub use buffer::clean_text;
pub use render::ComposerSection;

use std::time::{Duration, Instant};

use crate::keys::{Action, Key, KeyCode, KeyContext, KeyKind, KeyMap};
use buffer::{Draft, logical_rows, prev_boundary, row_of};
use paste_burst::{CharStep, EnterStep, PasteBurst};
use recall::Recall;
use wrap::{Row, wrap};

/// How long a first Ctrl+C waits for the second.
const EXIT_WINDOW: Duration = Duration::from_secs(2);

/// What the composer should do with one key or one paste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposerOutcome {
    /// The key did nothing visible.
    Unchanged,
    /// The draft or the hint changed and should be drawn again.
    Changed,
    /// Send this text. Trailing whitespace has already been removed.
    Submit(String),
    /// Stop the running turn. The draft is kept.
    Interrupt,
    /// The person confirmed they want to leave.
    ExitRequested,
}

/// Facts from outside the draft that change what a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComposerContext {
    /// A turn is in progress.
    pub turn_running: bool,
}

/// The wrapped rows of one revision of the draft at one width.
#[derive(Debug)]
struct RowCache {
    revision: u64,
    width: usize,
    rows: Vec<Row>,
}

/// The layout the last `section` call prepared.
#[derive(Debug, Clone, Copy)]
struct Prepared {
    width: u16,
    boxed: bool,
    /// Cells of the text area.
    text_cells: usize,
    /// First wrapped row shown.
    first: usize,
    /// Wrapped rows shown.
    shown: usize,
    /// Rows hidden above and below the window.
    above: usize,
    below: usize,
    /// The borderless layout ends with a hint row.
    hint_row: bool,
    height: u16,
}

/// The draft, its history, and the paste and exit timers.
#[derive(Debug)]
pub struct Composer {
    keymap: KeyMap,
    draft: Draft,
    recall: Recall,
    burst: PasteBurst,
    /// Bytes each character of the current fast run put into the draft, so a burst takes
    /// exactly those back out.
    run: Vec<usize>,
    exit_armed: Option<Instant>,
    title: Option<String>,
    /// Border and prompt-marker styles; the default is the phase 1 look.
    style: crate::theme::ComposerStyle,
    keyboard_enhancement: bool,
    /// The wrap width of the last draw; `None` before the first, when each logical line is a row.
    wrap_width: Option<usize>,
    cache: Option<RowCache>,
    scroll_top: usize,
    prepared: Option<Prepared>,
    highlights: Vec<(std::ops::Range<usize>, ratatui::style::Style)>,
    highlight_revision: u64,
    last_history_recall: bool,
}

impl Composer {
    /// An empty draft bound to `keymap`.
    #[must_use]
    pub fn new(keymap: KeyMap) -> Self {
        Self::with_history(keymap, Vec::new())
    }

    /// An empty draft whose history is seeded, oldest first.
    #[must_use]
    pub fn with_history(keymap: KeyMap, entries: Vec<String>) -> Self {
        Self {
            keymap,
            draft: Draft::new(),
            recall: Recall::with_entries(entries),
            burst: PasteBurst::new(),
            run: Vec::new(),
            exit_armed: None,
            title: None,
            style: crate::theme::ComposerStyle::default(),
            keyboard_enhancement: true,
            wrap_width: None,
            cache: None,
            scroll_top: 0,
            prepared: None,
            highlights: Vec::new(),
            highlight_revision: 0,
            last_history_recall: false,
        }
    }

    /// Apply one key.
    pub fn handle_key(&mut self, key: Key, ctx: ComposerContext, now: Instant) -> ComposerOutcome {
        self.last_history_recall = false;
        if key.kind == KeyKind::Release {
            return ComposerOutcome::Unchanged;
        }
        let action = self.keymap.resolve(&key, KeyContext::Composer);
        let mods = key.mods;
        let plain = match key.code {
            KeyCode::Char(c) if !(mods.ctrl || mods.alt || mods.super_key || mods.other) => Some(c),
            _ => None,
        };
        match (action, plain) {
            (None, Some(c)) => self.type_char(c, now),
            (Some(action @ (Action::Submit | Action::Newline)), _) => self.enter(action, now),
            (Some(action), _) => {
                let flushed = self.flush_burst();
                let outcome = self.run_action(action, ctx, now);
                merge(flushed, outcome)
            }
            (None, None) => {
                if self.flush_burst() {
                    ComposerOutcome::Changed
                } else {
                    ComposerOutcome::Unchanged
                }
            }
        }
    }

    /// Insert a bracketed paste. Never submits.
    pub fn handle_paste(&mut self, text: &str) -> ComposerOutcome {
        let mut inserted = self.flush_burst();
        self.burst.reset();
        self.run.clear();
        let clean = clean_text(text);
        if !clean.is_empty() {
            self.draft.insert_clean(&clean);
            inserted = true;
        }
        if inserted {
            self.disarm();
            ComposerOutcome::Changed
        } else {
            ComposerOutcome::Unchanged
        }
    }

    /// Flush a paste burst or retire an exit hint when `now` has reached it.
    pub fn tick(&mut self, now: Instant) -> ComposerOutcome {
        let mut changed = false;
        if let Some(text) = self.burst.tick(now) {
            self.run.clear();
            changed |= self.insert_paste_text(&text);
        }
        if self
            .exit_armed
            .is_some_and(|armed| now >= armed + EXIT_WINDOW)
        {
            self.exit_armed = None;
            changed = true;
        }
        if changed {
            ComposerOutcome::Changed
        } else {
            ComposerOutcome::Unchanged
        }
    }

    /// Put any buffered raw-key burst into the draft before taking an insertion snapshot.
    pub fn flush_pending(&mut self) {
        self.flush_burst();
    }

    /// Raw keys are temporarily held outside the visible draft until the burst is flushed.
    #[must_use]
    pub fn is_collecting_burst(&self) -> bool {
        self.burst.is_active()
    }

    /// When the caller should wake with no key, if something is pending.
    #[must_use]
    pub fn next_tick(&self) -> Option<Instant> {
        let hint = self.exit_armed.map(|armed| armed + EXIT_WINDOW);
        match (self.burst.next_due(), hint) {
            (Some(burst), Some(hint)) => Some(burst.min(hint)),
            (burst, hint) => burst.or(hint),
        }
    }

    /// The draft text.
    #[must_use]
    pub fn text(&self) -> &str {
        self.draft.text()
    }

    /// Replace a UTF-8 byte range with safe text in one edit. Invalid ranges do nothing.
    /// The caret ends after the insertion, advanced to a grapheme boundary when necessary.
    pub fn replace_range(&mut self, range: std::ops::Range<usize>, text: &str) {
        self.flush_burst();
        self.burst.reset();
        self.run.clear();
        if self.draft.replace_range(range, &clean_text(text)) {
            self.recall.end();
            self.disarm();
        }
    }

    /// Mark live labels. The screen maps accent and dimmed onto these styles.
    pub fn set_highlights(&mut self, spans: Vec<(std::ops::Range<usize>, ratatui::style::Style)>) {
        let text = self.draft.text();
        self.highlights = spans
            .into_iter()
            .filter(|(range, _)| range.start < range.end && text.get(range.clone()).is_some())
            .collect();
        self.highlight_revision = self.draft.revision();
    }

    /// Byte offset of the caret, on a grapheme boundary.
    #[must_use]
    pub fn caret(&self) -> usize {
        self.draft.caret()
    }

    /// Whether the last key actually recalled a history entry or restored its saved draft.
    #[must_use]
    pub fn last_history_recall(&self) -> bool {
        self.last_history_recall
    }

    /// The draft has no characters. Spaces are not empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.draft.text().is_empty() && !self.burst.is_active()
    }

    /// The caret is on the first visual row.
    #[must_use]
    pub fn cursor_on_first_row(&self) -> bool {
        let rows = self.current_rows();
        row_of(&rows, self.draft.caret()) == 0
    }

    /// Submitted prompts, oldest first.
    #[must_use]
    pub fn history(&self) -> &[String] {
        self.recall.entries()
    }

    /// Take back the history entry that submitting `text` just made, when it made one. For a
    /// submit that turned out to be a hint, not a prompt. Earlier entries are never touched.
    pub fn forget_submitted(&mut self, text: &str) {
        self.recall.forget_last(text);
    }

    /// Replace the draft. The caret lands at the end. History is unchanged.
    pub fn load(&mut self, text: String) {
        self.clear();
        let clean = clean_text(&text);
        self.draft.replace(clean);
    }

    /// Remove the draft, end a history browse, and disarm exit.
    pub fn clear(&mut self) {
        self.draft.replace(String::new());
        self.recall.end();
        self.burst.reset();
        self.run.clear();
        self.exit_armed = None;
    }

    /// Title drawn on the top border. `None` clears it.
    pub fn set_title(&mut self, title: Option<String>) {
        self.title = title;
    }

    /// Border and prompt-marker styles from the current theme.
    pub fn set_style(&mut self, style: crate::theme::ComposerStyle) {
        self.style = style;
    }

    /// Whether Shift+Enter can be sent. The default is true.
    pub fn set_keyboard_enhancement(&mut self, on: bool) {
        self.keyboard_enhancement = on;
    }

    /// Prepare wrapping for this width and return the section to draw.
    pub fn section(&mut self, width: u16, max_rows: u16) -> ComposerSection<'_> {
        let boxed = width >= 20 && max_rows >= 3;
        let text_cells = usize::from(if boxed {
            width - 7
        } else {
            width.saturating_sub(3)
        });
        let wrap_width = text_cells.saturating_sub(1).max(1);
        self.wrap_width = Some(wrap_width);
        self.refresh_cache(wrap_width);
        let total = if self.draft.text().is_empty() {
            1
        } else {
            self.cache.as_ref().map_or(1, |cache| cache.rows.len())
        };
        let caret_row = self
            .cache
            .as_ref()
            .map_or(0, |cache| row_of(&cache.rows, self.draft.caret()));

        let limit =
            usize::from(if boxed { max_rows - 2 } else { max_rows }).clamp(1, MAX_COMPOSER_ROWS);
        let exit_hint = self.exit_armed.is_some();
        let (shown, hint_row) = if boxed {
            (total.min(limit), false)
        } else if max_rows >= 2 && (total > limit || exit_hint) {
            (total.min(limit - 1).max(1), true)
        } else {
            (total.min(limit), false)
        };

        let mut first = self.scroll_top;
        if caret_row < first {
            first = caret_row;
        }
        if caret_row >= first + shown {
            first = caret_row + 1 - shown;
        }
        first = first.min(total - shown);
        self.scroll_top = first;

        let rows = u16::try_from(shown).unwrap_or(u16::MAX);
        let height = if boxed {
            rows + 2
        } else {
            rows + u16::from(hint_row)
        };
        self.prepared = Some(Prepared {
            width,
            boxed,
            text_cells,
            first,
            shown,
            above: first,
            below: total - first - shown,
            hint_row,
            height,
        });
        ComposerSection::new(self)
    }

    /// Type one plain character, through the burst detector.
    fn type_char(&mut self, c: char, now: Instant) -> ComposerOutcome {
        let disarmed = self.disarm();
        match self.burst.on_char(c, now) {
            CharStep::Insert => {
                if self.burst_run_restarted() {
                    self.run.clear();
                }
                let before = self.draft.text().len();
                self.draft.insert(&c.to_string());
                self.run.push(self.draft.text().len() - before);
                if self.draft.text().len() != before || disarmed {
                    ComposerOutcome::Changed
                } else {
                    ComposerOutcome::Unchanged
                }
            }
            CharStep::TakeBack(count) => {
                let taken: usize = self.run.iter().rev().take(count).sum();
                self.draft.remove_before(taken);
                self.run.clear();
                ComposerOutcome::Changed
            }
            CharStep::Append => {
                if disarmed {
                    ComposerOutcome::Changed
                } else {
                    ComposerOutcome::Unchanged
                }
            }
        }
    }

    /// The detector started a new run with this character.
    fn burst_run_restarted(&self) -> bool {
        self.burst.run_len() == 1
    }

    /// Enter on `submit` or `newline`, through the burst detector.
    fn enter(&mut self, action: Action, now: Instant) -> ComposerOutcome {
        self.run.clear();
        match self.burst.on_enter(now) {
            EnterStep::Append => {
                self.disarm();
                ComposerOutcome::Unchanged
            }
            EnterStep::Newline => self.insert_newline(),
            EnterStep::Submit if action == Action::Newline => self.insert_newline(),
            EnterStep::Submit => self.submit(),
        }
    }

    fn insert_newline(&mut self) -> ComposerOutcome {
        self.disarm();
        self.draft.insert("\n");
        ComposerOutcome::Changed
    }

    fn submit(&mut self) -> ComposerOutcome {
        let text = self.draft.text().trim_end().to_string();
        if text.trim().is_empty() {
            return ComposerOutcome::Unchanged;
        }
        self.recall.add(text.clone());
        self.recall.end();
        self.draft.replace(String::new());
        self.disarm();
        ComposerOutcome::Submit(text)
    }

    /// Insert the burst buffer, if one is collecting. True when text went in.
    fn flush_burst(&mut self) -> bool {
        self.run.clear();
        match self.burst.on_other_key() {
            Some(text) => self.insert_paste_text(&text),
            None => false,
        }
    }

    fn insert_paste_text(&mut self, text: &str) -> bool {
        let clean = clean_text(text);
        if clean.is_empty() {
            return false;
        }
        self.draft.insert_clean(&clean);
        self.disarm();
        true
    }

    /// Forget an armed exit. True when one was armed.
    fn disarm(&mut self) -> bool {
        self.exit_armed.take().is_some()
    }

    fn run_action(
        &mut self,
        action: Action,
        ctx: ComposerContext,
        now: Instant,
    ) -> ComposerOutcome {
        match action {
            Action::Interrupt => {
                let disarmed = self.disarm();
                if ctx.turn_running {
                    ComposerOutcome::Interrupt
                } else if disarmed {
                    ComposerOutcome::Changed
                } else {
                    ComposerOutcome::Unchanged
                }
            }
            Action::Cancel => self.cancel(ctx, now),
            Action::Submit | Action::Newline => ComposerOutcome::Unchanged,
            _ => {
                let before = (self.draft.revision(), self.draft.caret());
                let disarmed = self.disarm();
                let moved = self.edit(action);
                if moved || disarmed || before != (self.draft.revision(), self.draft.caret()) {
                    ComposerOutcome::Changed
                } else {
                    ComposerOutcome::Unchanged
                }
            }
        }
    }

    fn cancel(&mut self, ctx: ComposerContext, now: Instant) -> ComposerOutcome {
        if !self.is_empty() {
            let text = self.draft.text().to_string();
            self.recall.add(text);
            self.recall.end();
            self.draft.replace(String::new());
            self.burst.reset();
            self.disarm();
            return ComposerOutcome::Changed;
        }
        if ctx.turn_running {
            self.disarm();
            return ComposerOutcome::Interrupt;
        }
        match self.exit_armed {
            Some(armed) if now < armed + EXIT_WINDOW => {
                self.exit_armed = None;
                ComposerOutcome::ExitRequested
            }
            _ => {
                self.exit_armed = Some(now);
                ComposerOutcome::Changed
            }
        }
    }

    /// An editing or caret action. True when history changed the draft.
    fn edit(&mut self, action: Action) -> bool {
        match action {
            Action::MoveLeft => self.draft.move_left(),
            Action::MoveRight => self.draft.move_right(),
            Action::MoveUp => return self.move_up(),
            Action::MoveDown => return self.move_down(),
            Action::LineStart => self.draft.line_start(),
            Action::LineEnd => self.draft.line_end(),
            Action::BufferStart => self.draft.set_caret(0),
            Action::BufferEnd => self.draft.set_caret(self.draft.text().len()),
            Action::WordLeft => self.draft.word_left(),
            Action::WordRight => self.draft.word_right(),
            Action::DeleteBackward => self.draft.backspace(),
            Action::DeleteForward => self.draft.delete_forward(),
            Action::DeleteWordBackward => self.draft.delete_word_backward(),
            Action::KillToLineEnd => self.draft.kill_to_line_end(),
            Action::KillToLineStart => self.draft.kill_to_line_start(),
            Action::Yank => self.draft.yank(),
            Action::Submit
            | Action::Newline
            | Action::Interrupt
            | Action::Cancel
            | Action::CompletionNext
            | Action::CompletionPrev
            | Action::CompletionAccept
            | Action::CompletionClose
            | Action::OpenEditor
            | Action::HistorySearch
            | Action::Suspend
            | Action::AttachClipboardImage
            | Action::TranscriptOpen
            | Action::PagerLineUp
            | Action::PagerLineDown
            | Action::PagerPageUp
            | Action::PagerPageDown
            | Action::PagerHalfUp
            | Action::PagerHalfDown
            | Action::PagerTop
            | Action::PagerBottom
            | Action::PagerPrevPrompt
            | Action::PagerNextPrompt
            | Action::PagerSearch
            | Action::PagerNextMatch
            | Action::PagerPrevMatch
            | Action::PagerDetails
            | Action::PagerClose
            | Action::ToggleTasks
            | Action::ApprovalsToggle
            | Action::QueueEdit
            | Action::QueueRemove
            | Action::QueueSteer
            | Action::QueueSendNow
            | Action::QueueMoveUp
            | Action::QueueMoveDown
            | Action::QueueLeave
            | Action::ComposerSendNow
            | Action::ComposerSteer
            // The jump key never edits the draft; it opens the fleet view.
            | Action::AttentionNext => {}
            // Panel actions never edit the draft; the panel owns them. Issue #242.
            Action::AgentsPanel
            | Action::AgentsUp
            | Action::AgentsDown
            | Action::AgentsTail
            | Action::AgentsDetail
            | Action::AgentsBack
            | Action::AgentsClose
            | Action::AgentsSteer
            | Action::AgentsStop
            | Action::AgentsStopAll
            | Action::AgentsPause
            | Action::AgentsNextSession
            | Action::AgentsPageUp
            | Action::AgentsPageDown
            | Action::AgentsTop
            | Action::AgentsBottom
            | Action::AgentsOpenInPane => {}
            // Picker actions never edit the draft; the picker owns them.
            Action::SessionPickerOpen
            | Action::PickerUp
            | Action::PickerDown
            | Action::PickerPageUp
            | Action::PickerPageDown
            | Action::PickerTop
            | Action::PickerBottom
            | Action::PickerOpen
            | Action::PickerRename
            | Action::PickerBranch
            | Action::PickerExport
            | Action::PickerSearch
            | Action::PickerProfileNext
            | Action::PickerProfilePrev
            | Action::PickerRefresh
            | Action::PickerClose
            | Action::PickerFormatMarkdown
            | Action::PickerFormatJson
            | Action::PickerFormatHtml
            | Action::PickerToggleRedact
            | Action::FleetViewOpen
            | Action::FleetUp
            | Action::FleetDown
            | Action::FleetPageUp
            | Action::FleetPageDown
            | Action::FleetTop
            | Action::FleetBottom
            | Action::FleetOpen
            | Action::FleetOpenInPane
            | Action::FleetPeek
            | Action::FleetReply
            | Action::FleetStop
            | Action::FleetAnswer
            | Action::FleetFilter
            | Action::FleetProfileNext
            | Action::FleetProfilePrev
            | Action::FleetNew
            | Action::FleetNewWorktree
            | Action::FleetReview => {}
            // Review actions never edit the draft; the review owns them. Issue #245.
            Action::ReviewUp
            | Action::ReviewDown
            | Action::ReviewTop
            | Action::ReviewBottom
            | Action::ReviewShowDiff
            | Action::ReviewDiffPageUp
            | Action::ReviewDiffPageDown
            | Action::ReviewScope
            | Action::ReviewStage
            | Action::ReviewStageAll
            | Action::ReviewUnstageAll
            | Action::ReviewDiscard
            | Action::ReviewCommit
            | Action::ReviewPush
            | Action::ReviewPullRequest
            | Action::ReviewOpenPr
            | Action::ReviewRefresh
            | Action::CommitSubmit
            | Action::CommitDraft => {}
        }
        false
    }

    fn move_up(&mut self) -> bool {
        let rows = self.current_rows();
        if self.draft.move_up_in(&rows) {
            return false;
        }
        if self.recall.entries().is_empty() {
            self.draft.set_caret(0);
            return false;
        }
        let Some(shown) = self.recall.older(self.draft.text()) else {
            return false;
        };
        self.draft.replace(shown);
        self.last_history_recall = true;
        let rows = self.current_rows();
        let first = &rows[0];
        let end = if first.hard_break_after || rows.len() == 1 {
            first.end
        } else {
            prev_boundary(self.draft.text(), first.end)
        };
        self.draft.set_caret(end);
        true
    }

    fn move_down(&mut self) -> bool {
        let rows = self.current_rows();
        if self.draft.move_down_in(&rows) {
            return false;
        }
        match self.recall.newer(self.draft.text()) {
            Some(shown) => {
                self.draft.replace(shown);
                self.last_history_recall = true;
                true
            }
            None => {
                self.draft.set_caret(self.draft.text().len());
                false
            }
        }
    }

    /// The visual rows at the last drawn width, or the logical lines before the first draw.
    fn current_rows(&self) -> Vec<Row> {
        match (self.wrap_width, &self.cache) {
            (Some(width), Some(cache))
                if cache.width == width && cache.revision == self.draft.revision() =>
            {
                cache.rows.clone()
            }
            (Some(width), _) => wrap(self.draft.text(), width),
            (None, _) => logical_rows(self.draft.text()),
        }
    }

    fn refresh_cache(&mut self, width: usize) {
        let revision = self.draft.revision();
        let fresh = self
            .cache
            .as_ref()
            .is_some_and(|cache| cache.width == width && cache.revision == revision);
        if !fresh {
            self.cache = Some(RowCache {
                revision,
                width,
                rows: wrap(self.draft.text(), width),
            });
        }
    }

    /// The placeholder for an empty draft, with the first chord of each action.
    fn placeholder(&self) -> String {
        let submit = self.keymap.label(Action::Submit);
        let newline = self
            .keymap
            .chords(Action::Newline)
            .iter()
            .find(|chord| self.keyboard_enhancement || !chord.needs_enhancement())
            .map(|chord| chord.label());
        match (submit, newline) {
            (Some(submit), Some(newline)) => {
                format!("Type a message. {submit} sends, {newline} adds a line.")
            }
            (Some(submit), None) => format!("Type a message. {submit} sends."),
            (None, Some(newline)) => format!("Type a message. {newline} adds a line."),
            (None, None) => "Type a message.".to_string(),
        }
    }
}

/// Most text rows the composer shows at once.
const MAX_COMPOSER_ROWS: usize = 8;

/// A flush that changed the draft makes an unchanged outcome a change.
fn merge(flushed: bool, outcome: ComposerOutcome) -> ComposerOutcome {
    if flushed && outcome == ComposerOutcome::Unchanged {
        ComposerOutcome::Changed
    } else {
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::Composer;
    use crate::keys::KeyMap;
    use ratatui::style::Style;

    #[test]
    fn replacement_cleans_text_and_keeps_a_valid_caret() {
        let mut composer = Composer::new(KeyMap::defaults());
        composer.load("see @cal tail".into());
        composer.replace_range(4..8, &format!("@file:calc.py{}\r\n", char::from(27)));
        assert_eq!(composer.text(), "see @file:calc.py\n tail");
        assert_eq!(composer.caret(), "see @file:calc.py\n".len());

        composer.load("éx".into());
        composer.replace_range(1..2, "wrong");
        assert_eq!(composer.text(), "éx");
        composer.replace_range(0..2, "a");
        assert_eq!(composer.text(), "ax");
        assert_eq!(composer.caret(), 1);

        composer.load("a\u{301}z".into());
        composer.replace_range(0..1, "e");
        assert_eq!(composer.text(), "e\u{301}z");
        assert_eq!(composer.caret(), "e\u{301}".len());
    }

    #[test]
    fn highlights_are_valid_byte_ranges_for_one_revision() {
        let mut composer = Composer::new(KeyMap::defaults());
        composer.load("é label".into());
        composer.set_highlights(vec![
            (0..2, Style::default()),
            (1..2, Style::default()),
            (3..99, Style::default()),
        ]);
        assert_eq!(composer.highlights.len(), 1);
        assert_eq!(composer.highlight_revision, composer.draft.revision());
        composer.replace_range(0..2, "x");
        assert_ne!(composer.highlight_revision, composer.draft.revision());
    }

    #[test]
    fn recall_flag_tracks_history_instead_of_caret_motion() {
        use super::{ComposerContext, ComposerOutcome};
        use crate::keys::Key;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut composer = Composer::with_history(KeyMap::defaults(), vec!["old".into()]);
        composer.load("new".into());
        let now = std::time::Instant::now();
        let ctx = ComposerContext {
            turn_running: false,
        };
        let key = |code| Key::from(KeyEvent::new(code, KeyModifiers::NONE));
        assert_eq!(
            composer.handle_key(key(KeyCode::Up), ctx, now),
            ComposerOutcome::Changed
        );
        assert!(composer.last_history_recall());
        composer.handle_key(key(KeyCode::Left), ctx, now);
        assert!(!composer.last_history_recall());
        composer.handle_key(key(KeyCode::Down), ctx, now);
        assert!(composer.last_history_recall());
        assert_eq!(composer.text(), "new");
    }

    #[test]
    fn flushing_before_an_insertion_snapshot_preserves_typing_order() {
        use super::ComposerContext;
        use crate::keys::Key;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut composer = Composer::new(KeyMap::defaults());
        let now = std::time::Instant::now();
        let ctx = ComposerContext {
            turn_running: false,
        };
        for character in ['A', 'B', 'C'] {
            composer.handle_key(
                Key::from(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE)),
                ctx,
                now,
            );
        }
        assert_eq!(composer.text(), "");
        composer.flush_pending();
        assert_eq!(composer.text(), "ABC");
        let caret = composer.caret();
        composer.replace_range(caret..caret, "[Image #1]");
        assert_eq!(composer.text(), "ABC[Image #1]");
        assert_eq!(composer.caret(), composer.text().len());
    }
}
