//! Vim-style commands over the composer's existing draft.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::buffer::{next_boundary, prev_boundary};
use super::{Composer, ComposerContext, ComposerOutcome};
use crate::keys::{Action, Key, KeyCode, KeyKind, Mods};

/// One vim mode. Off is composer state, not a mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMode {
    /// Motions and operators.
    Normal,
    /// The ordinary composer, with Escape returning to normal mode.
    Insert,
    /// A characterwise selection.
    Visual,
    /// A linewise selection.
    VisualLine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    text: String,
    caret: usize,
}

impl Snapshot {
    pub(crate) fn of(composer: &Composer) -> Self {
        Self {
            text: composer.draft.text().to_string(),
            caret: composer.draft.caret(),
        }
    }

    fn restore(&self, composer: &mut Composer) {
        composer.draft.replace(self.text.clone());
        composer.draft.set_caret(self.caret);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Repeat {
    keys: String,
    inserted: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InsertSession {
    before: Snapshot,
    base: String,
    command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Waiting {
    Find(char),
    Replace,
    G,
}

/// The mode, pending command, selection and bounded editing history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VimState {
    mode: VimMode,
    anchor: Option<usize>,
    count: Option<usize>,
    operator: Option<(char, usize)>,
    waiting: Option<Waiting>,
    label: String,
    last_find: Option<(char, char)>,
    last_change: Option<Repeat>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    insert: Option<InsertSession>,
    replaying: bool,
}

impl VimState {
    pub(crate) fn new() -> Self {
        Self {
            mode: VimMode::Normal,
            anchor: None,
            count: None,
            operator: None,
            waiting: None,
            label: String::new(),
            last_find: None,
            last_change: None,
            undo: Vec::new(),
            redo: Vec::new(),
            insert: None,
            replaying: false,
        }
    }

    /// The mode now showing.
    #[must_use]
    pub fn mode(&self) -> VimMode {
        self.mode
    }

    /// The command text waiting for another key.
    #[must_use]
    pub fn pending_label(&self) -> Option<String> {
        (!self.label.is_empty()).then(|| self.label.clone())
    }

    fn clear_pending(&mut self) -> bool {
        let had_label = !self.label.is_empty();
        self.count = None;
        self.operator = None;
        self.waiting = None;
        self.label.clear();
        had_label
    }

    fn push_undo(&mut self, before: Snapshot) {
        if self.replaying {
            return;
        }
        if self.undo.len() == 100 {
            self.undo.remove(0);
        }
        self.undo.push(before);
        self.redo.clear();
    }

    fn finish_change(&mut self, before: Snapshot, composer: &Composer, command: Option<String>) {
        if before.text != composer.draft.text() {
            self.push_undo(before);
            if !self.replaying
                && let Some(keys) = command
            {
                self.last_change = Some(Repeat {
                    keys,
                    inserted: String::new(),
                });
            }
        }
    }

    pub(crate) fn loaded(&mut self, before: Snapshot, after: Snapshot) {
        self.undo.clear();
        self.redo.clear();
        self.undo.push(before);
        self.adjust_anchor(&after.text);
        self.insert = (self.mode == VimMode::Insert).then(|| InsertSession {
            base: after.text.clone(),
            before: after,
            command: None,
        });
        self.clear_pending();
    }

    pub(crate) fn record_clear(&mut self, before: Snapshot) {
        if self.anchor.is_some() {
            self.anchor = Some(0);
        }
        if self.mode != VimMode::Insert || self.insert.is_none() {
            self.push_undo(before);
        }
    }

    pub(crate) fn adjust_anchor(&mut self, text: &str) {
        let Some(anchor) = self.anchor else {
            return;
        };
        let bounded = anchor.min(text.len());
        let at = if bounded == text.len() {
            bounded
        } else {
            text.grapheme_indices(true)
                .map(|(index, _)| index)
                .take_while(|index| *index <= bounded)
                .last()
                .unwrap_or(0)
        };
        self.anchor = Some(displayed_caret(text, at));
    }

    fn begin_insert(&mut self, composer: &mut Composer, before: Snapshot, command: Option<String>) {
        self.mode = VimMode::Insert;
        self.anchor = None;
        self.clear_pending();
        composer.disarm();
        self.insert = Some(InsertSession {
            before,
            base: composer.draft.text().to_string(),
            command,
        });
    }

    pub(crate) fn leave_insert(&mut self, composer: &mut Composer) {
        if let Some(session) = self.insert.take()
            && session.before.text != composer.draft.text()
        {
            let inserted = inserted_text(&session.base, composer.draft.text());
            self.push_undo(session.before);
            if !self.replaying
                && let Some(keys) = session.command
            {
                self.last_change = Some(Repeat { keys, inserted });
            }
        }
        self.mode = VimMode::Normal;
        self.anchor = None;
        self.clear_pending();
        composer.disarm();
        clamp_caret(composer, false);
    }

    pub(crate) fn submitted(&mut self, composer: &mut Composer) {
        self.undo.clear();
        self.redo.clear();
        self.clear_pending();
        self.anchor = None;
        self.mode = VimMode::Insert;
        self.insert = Some(InsertSession {
            before: Snapshot::of(composer),
            base: composer.draft.text().to_string(),
            command: None,
        });
    }

    fn undo(&mut self, composer: &mut Composer) -> bool {
        let Some(before) = self.undo.pop() else {
            return false;
        };
        self.redo.push(Snapshot::of(composer));
        before.restore(composer);
        true
    }

    fn redo(&mut self, composer: &mut Composer) -> bool {
        let Some(after) = self.redo.pop() else {
            return false;
        };
        if self.undo.len() == 100 {
            self.undo.remove(0);
        }
        self.undo.push(Snapshot::of(composer));
        after.restore(composer);
        self.mode = VimMode::Normal;
        self.anchor = None;
        composer.disarm();
        true
    }

    /// The selected byte range, including all selected clusters.
    pub(crate) fn selection(&self, text: &str, caret: usize) -> Option<Range<usize>> {
        let anchor = self.anchor?;
        if self.mode == VimMode::VisualLine {
            let start = line_start(text, anchor.min(caret));
            let end = line_end(text, anchor.max(caret));
            Some(start..if end < text.len() { end + 1 } else { end })
        } else if self.mode == VimMode::Visual {
            let start = anchor.min(caret);
            let last = anchor.max(caret);
            Some(start..next_boundary(text, last))
        } else {
            None
        }
    }

    pub(crate) fn paste(&mut self, composer: &mut Composer, text: &str) -> ComposerOutcome {
        composer.flush_burst();
        composer.burst.reset();
        composer.run.clear();
        let before = Snapshot::of(composer);
        let clean = super::clean_text(text);
        let selection = self.selection(composer.draft.text(), composer.draft.caret());
        let range = selection.unwrap_or(composer.draft.caret()..composer.draft.caret());
        if range.start != range.end || !clean.is_empty() {
            let _ = composer.draft.replace_range(range, &clean);
            if !clean.is_empty() {
                let at = prev_boundary(composer.draft.text(), composer.draft.caret());
                composer.draft.set_caret(at);
            }
        }
        let changed = before.text != composer.draft.text();
        let was_visual = self.anchor.take().is_some();
        self.mode = VimMode::Normal;
        self.clear_pending();
        clamp_caret(composer, false);
        composer.disarm();
        if changed {
            self.push_undo(before);
        }
        if changed || was_visual {
            ComposerOutcome::Changed
        } else {
            ComposerOutcome::Unchanged
        }
    }

    pub(crate) fn handle_key(
        &mut self,
        composer: &mut Composer,
        key: Key,
        action: Option<Action>,
        ctx: ComposerContext,
    ) -> ComposerOutcome {
        let visual = matches!(self.mode, VimMode::Visual | VimMode::VisualLine);
        let control_c = key.mods.ctrl
            && !key.mods.alt
            && !key.mods.super_key
            && !key.mods.other
            && matches!(key.code, KeyCode::Char('c' | 'C'));
        let escape = key.code == KeyCode::Esc && key.mods == Mods::default();
        if escape || control_c {
            let pending = self.clear_pending();
            if visual {
                self.mode = VimMode::Normal;
                self.anchor = None;
                composer.disarm();
                clamp_caret(composer, false);
                return ComposerOutcome::Changed;
            }
            if pending {
                return ComposerOutcome::Changed;
            }
            if ctx.turn_running && (escape || composer.is_empty()) {
                return ComposerOutcome::Interrupt;
            }
            return ComposerOutcome::Unchanged;
        }
        if self.waiting.is_some() {
            return self.finish_waiting(composer, key);
        }
        if action == Some(Action::HistorySearch) {
            if self.operator.is_some() {
                self.clear_pending();
                return ComposerOutcome::Changed;
            }
            let cleared = self.clear_pending();
            return if self.redo(composer) || cleared {
                ComposerOutcome::Changed
            } else {
                ComposerOutcome::Unchanged
            };
        }
        if key.code == KeyCode::Enter && key.mods == Mods::default() {
            if self.clear_pending() {
                return ComposerOutcome::Changed;
            }
            if visual {
                self.mode = VimMode::Normal;
                self.anchor = None;
                composer.disarm();
                clamp_caret(composer, false);
                return ComposerOutcome::Changed;
            }
            return composer.submit();
        }
        if key.mods.ctrl
            || key.mods.alt
            || key.mods.super_key
            || key.mods.other
            || (key.mods.shift && !matches!(key.code, KeyCode::Char(_)))
        {
            return if self.clear_pending() {
                ComposerOutcome::Changed
            } else {
                ComposerOutcome::Unchanged
            };
        }
        let ch = match key.code {
            KeyCode::Char(ch) => ch,
            KeyCode::Left => 'h',
            KeyCode::Right => 'l',
            KeyCode::Up => 'k',
            KeyCode::Down => 'j',
            _ => {
                return if self.clear_pending() {
                    ComposerOutcome::Changed
                } else {
                    ComposerOutcome::Unchanged
                };
            }
        };
        if ch.is_ascii_digit() && (ch != '0' || self.count.is_some()) {
            let digit = usize::from(ch as u8 - b'0');
            self.count = Some(
                self.count
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(digit)
                    .min(9999),
            );
            self.label.push(ch);
            return ComposerOutcome::Changed;
        }
        if matches!(ch, 'f' | 'F' | 't' | 'T') {
            self.waiting = Some(Waiting::Find(ch));
            self.label.push(ch);
            return ComposerOutcome::Changed;
        }
        if ch == 'g' {
            self.waiting = Some(Waiting::G);
            self.label.push(ch);
            return ComposerOutcome::Changed;
        }
        if self.operator.is_some() && !matches!(ch, 'd' | 'c' | 'y') && !is_motion(ch) {
            self.clear_pending();
            return ComposerOutcome::Changed;
        }
        if is_motion(ch) || matches!(ch, ';' | ',') {
            return self.apply_motion(composer, ch, None);
        }
        if visual {
            return self.visual_command(composer, ch);
        }
        if matches!(ch, 'd' | 'c' | 'y') {
            if let Some((operator, first)) = self.operator {
                if operator == ch {
                    let count = multiply(first, self.count.unwrap_or(1));
                    let command = format!("{}{ch}", self.label);
                    self.clear_pending();
                    return self.line_operator(composer, operator, count, command);
                }
                self.clear_pending();
                return ComposerOutcome::Changed;
            }
            self.operator = Some((ch, self.count.take().unwrap_or(1)));
            self.label.push(ch);
            return ComposerOutcome::Changed;
        }
        let count = self.count.unwrap_or(1);
        let command = format!("{}{ch}", self.label);
        let had_pending = self.clear_pending();
        match ch {
            'i' | 'a' | 'I' | 'A' | 'o' | 'O' => self.entry(composer, ch, command),
            'v' | 'V' => {
                self.mode = if ch == 'v' {
                    VimMode::Visual
                } else {
                    VimMode::VisualLine
                };
                clamp_caret(composer, false);
                self.anchor = Some(composer.draft.caret());
                composer.disarm();
                ComposerOutcome::Changed
            }
            'x' | 'X' | 's' | 'S' => self.single_edit(composer, ch, count, command),
            'r' => {
                self.count = Some(count);
                self.label = command;
                self.waiting = Some(Waiting::Replace);
                ComposerOutcome::Changed
            }
            'D' | 'C' => {
                self.operator = Some((if ch == 'D' { 'd' } else { 'c' }, 1));
                self.count = Some(count);
                self.apply_operator_motion(composer, '$', None, command)
            }
            'p' | 'P' => self.put(composer, ch == 'p', count, Some(command)),
            'u' => {
                if self.undo(composer) || had_pending {
                    ComposerOutcome::Changed
                } else {
                    ComposerOutcome::Unchanged
                }
            }
            '.' => self.repeat(composer, ctx),
            _ => {
                if had_pending {
                    ComposerOutcome::Changed
                } else {
                    ComposerOutcome::Unchanged
                }
            }
        }
    }

    fn finish_waiting(&mut self, composer: &mut Composer, key: Key) -> ComposerOutcome {
        let waiting = self.waiting.take();
        let plain = !(key.mods.ctrl || key.mods.alt || key.mods.super_key || key.mods.other);
        let KeyCode::Char(ch) = key.code else {
            self.clear_pending();
            return ComposerOutcome::Changed;
        };
        if !plain {
            self.clear_pending();
            return ComposerOutcome::Changed;
        }
        match waiting {
            Some(Waiting::G) if ch == 'g' => self.apply_motion(composer, 'g', None),
            Some(Waiting::Find(kind)) => {
                self.last_find = Some((kind, ch));
                self.apply_motion(composer, kind, Some(ch))
            }
            Some(Waiting::Replace) => {
                let count = self.count.unwrap_or(1);
                let command = format!("{}{ch}", self.label);
                self.clear_pending();
                let replacement = super::clean_text(&ch.to_string());
                if replacement.is_empty() || replacement.contains('\n') {
                    return ComposerOutcome::Unchanged;
                }
                let start = composer.draft.caret();
                let end_line = line_end(composer.draft.text(), start);
                let mut end = start;
                let mut changed = 0;
                while end < end_line && changed < count {
                    end = next_boundary(composer.draft.text(), end);
                    changed += 1;
                }
                if changed == 0 {
                    return ComposerOutcome::Unchanged;
                }
                let before = Snapshot::of(composer);
                let _ = composer
                    .draft
                    .replace_range(start..end, &replacement.repeat(changed));
                composer.draft.set_caret(start);
                self.finish_change(before, composer, Some(command));
                ComposerOutcome::Changed
            }
            _ => {
                self.clear_pending();
                ComposerOutcome::Changed
            }
        }
    }

    fn apply_motion(
        &mut self,
        composer: &mut Composer,
        kind: char,
        target: Option<char>,
    ) -> ComposerOutcome {
        let suffix = match target {
            Some(ch) => format!("{ch}"),
            None if kind == 'g' => "g".to_string(),
            None => kind.to_string(),
        };
        let command = if target.is_some() || kind == 'g' {
            format!("{}{suffix}", self.label)
        } else {
            format!("{}{kind}", self.label)
        };
        if self.operator.is_some() {
            return self.apply_operator_motion(composer, kind, target, command);
        }
        let count = self.count.unwrap_or(1);
        let explicit = self.count.is_some();
        let start = composer.draft.caret();
        let motion = self.motion(composer, kind, target, count, explicit, false);
        self.clear_pending();
        if motion.is_some_and(|at| at != start) {
            ComposerOutcome::Changed
        } else {
            ComposerOutcome::Unchanged
        }
    }

    fn apply_operator_motion(
        &mut self,
        composer: &mut Composer,
        kind: char,
        target: Option<char>,
        command: String,
    ) -> ComposerOutcome {
        let Some((operator, first)) = self.operator else {
            return ComposerOutcome::Unchanged;
        };
        let count = multiply(first, self.count.unwrap_or(1));
        let explicit = first != 1 || self.count.is_some();
        let start = composer.draft.caret();
        let actual_kind = match (operator, kind) {
            ('c', 'w') if !is_space_at(composer.draft.text(), start) => 'e',
            ('c', 'W') if !is_space_at(composer.draft.text(), start) => 'E',
            _ => kind,
        };
        let target = self.motion(composer, actual_kind, target, count, explicit, true);
        composer.draft.set_caret(start);
        self.clear_pending();
        let Some(target) = target else {
            return ComposerOutcome::Unchanged;
        };
        if matches!(actual_kind, 'j' | 'k' | 'g' | 'G') {
            return self.apply_line_span(composer, operator, start, target, command);
        }
        let inclusive = matches!(
            actual_kind,
            'e' | 'E' | '$' | 'f' | 'F' | 't' | 'T' | ';' | ','
        );
        let text = composer.draft.text();
        let (from, to) = if target < start {
            (
                target,
                if inclusive {
                    next_boundary(text, start)
                } else {
                    start
                },
            )
        } else {
            (
                start,
                if inclusive {
                    next_boundary(text, target)
                } else {
                    target
                },
            )
        };
        if from == to {
            composer.draft.set_caret(start);
            return ComposerOutcome::Unchanged;
        }
        let killed = text[from..to].to_string();
        self.apply_span(composer, operator, from..to, killed, false, command)
    }

    fn motion(
        &mut self,
        composer: &mut Composer,
        kind: char,
        target: Option<char>,
        count: usize,
        explicit: bool,
        operator: bool,
    ) -> Option<usize> {
        let start = composer.draft.caret();
        let text = composer.draft.text();
        let result = match kind {
            'h' => {
                let mut at = start;
                let line = line_start(text, start);
                for _ in 0..count {
                    if at <= line {
                        break;
                    }
                    at = prev_boundary(text, at);
                }
                Some(at)
            }
            'l' => {
                let mut at = start;
                let end = line_end(text, start);
                let last = if end > line_start(text, start) {
                    prev_boundary(text, end)
                } else {
                    end
                };
                for _ in 0..count {
                    if at >= last {
                        break;
                    }
                    at = next_boundary(text, at).min(last);
                }
                Some(at)
            }
            '0' => Some(line_start(text, start)),
            '^' => Some(first_nonblank(text, start)),
            '$' => {
                let mut line = line_start(text, start);
                for _ in 1..count {
                    let end = line_end(text, line);
                    if end == text.len() {
                        break;
                    }
                    line = end + 1;
                }
                let end = line_end(text, line);
                Some(if end > line {
                    prev_boundary(text, end)
                } else {
                    line
                })
            }
            'w' | 'W' | 'b' | 'B' | 'e' | 'E' => {
                let clusters = positions(text);
                let mut at = start;
                for index in 0..count {
                    let next = match kind {
                        'w' | 'W' => word_forward(text, &clusters, at, kind == 'W'),
                        'b' | 'B' => word_backward(&clusters, at, kind == 'B'),
                        _ => word_end(text, &clusters, at, kind == 'E', operator && index == 0),
                    };
                    if next == at {
                        break;
                    }
                    at = next;
                }
                Some(at)
            }
            'j' | 'k' => {
                composer.draft.move_logical_by(kind == 'j', count);
                clamp_caret(composer, true);
                return Some(composer.draft.caret());
            }
            'g' | 'G' => {
                let wanted = if explicit {
                    count
                } else if kind == 'g' {
                    1
                } else {
                    usize::MAX
                };
                let mut line = 0;
                let mut number = 1;
                while number < wanted {
                    let end = line_end(text, line);
                    if end == text.len() {
                        break;
                    }
                    line = end + 1;
                    number += 1;
                }
                Some(line)
            }
            'f' | 'F' | 't' | 'T' => {
                target.and_then(|needle| find_cluster(text, start, kind, needle, count))
            }
            ';' | ',' => self.last_find.and_then(|(previous, needle)| {
                let direction = if kind == ',' {
                    reverse_find(previous)
                } else {
                    previous
                };
                find_cluster(text, start, direction, needle, count)
            }),
            _ => None,
        };
        if let Some(at) = result {
            composer.draft.set_caret(at);
        }
        result
    }

    fn line_operator(
        &mut self,
        composer: &mut Composer,
        operator: char,
        count: usize,
        command: String,
    ) -> ComposerOutcome {
        let start = composer.draft.caret();
        let mut end = line_start(composer.draft.text(), start);
        for _ in 1..count {
            let line_end = line_end(composer.draft.text(), end);
            if line_end == composer.draft.text().len() {
                break;
            }
            end = line_end + 1;
        }
        self.apply_line_span(composer, operator, start, end, command)
    }

    fn apply_line_span(
        &mut self,
        composer: &mut Composer,
        operator: char,
        a: usize,
        b: usize,
        command: String,
    ) -> ComposerOutcome {
        let text = composer.draft.text();
        let start = line_start(text, a.min(b));
        let last_end = line_end(text, a.max(b));
        let copy_end = if last_end < text.len() {
            last_end + 1
        } else {
            last_end
        };
        let mut killed = text[start..copy_end].to_string();
        if !killed.ends_with('\n') {
            killed.push('\n');
        }
        if killed == "\n" && text.is_empty() && operator != 'c' {
            composer.draft.set_caret(start);
            return ComposerOutcome::Unchanged;
        }
        if operator == 'c' {
            let before = Snapshot::of(composer);
            composer.draft.set_kill(killed, true);
            if start < last_end {
                let _ = composer.draft.replace_range(start..last_end, "");
            }
            composer.draft.set_caret(start);
            self.begin_insert(composer, before, Some(command));
            return ComposerOutcome::Changed;
        }
        let deletion_start = if operator != 'y' && copy_end == text.len() && start > 0 {
            start - 1
        } else {
            start
        };
        let deletion = deletion_start..copy_end;
        self.apply_span(composer, operator, deletion, killed, true, command)
    }

    fn apply_span(
        &mut self,
        composer: &mut Composer,
        operator: char,
        span: Range<usize>,
        killed: String,
        linewise: bool,
        command: String,
    ) -> ComposerOutcome {
        let before = Snapshot::of(composer);
        let start = span.start;
        composer.draft.set_kill(killed, linewise);
        if operator != 'y' {
            let _ = composer.draft.replace_range(span, "");
        } else {
            composer.draft.set_caret(start);
        }
        if operator == 'c' {
            self.begin_insert(composer, before, Some(command));
        } else {
            clamp_caret(composer, false);
            if operator == 'd' {
                self.finish_change(before, composer, Some(command));
            }
        }
        ComposerOutcome::Changed
    }

    fn entry(&mut self, composer: &mut Composer, kind: char, command: String) -> ComposerOutcome {
        let before = Snapshot::of(composer);
        let caret = composer.draft.caret();
        let text = composer.draft.text();
        let start = line_start(text, caret);
        let end = line_end(text, caret);
        match kind {
            'a' => composer
                .draft
                .set_caret(next_boundary(text, caret).min(end)),
            'I' => composer.draft.set_caret(first_nonblank(text, caret)),
            'A' => composer.draft.set_caret(end),
            'o' => {
                composer.draft.set_caret(end);
                composer.draft.insert_clean("\n");
            }
            'O' => {
                composer.draft.set_caret(start);
                composer.draft.insert_clean("\n");
                composer.draft.set_caret(start);
            }
            _ => {}
        }
        let repeat = matches!(kind, 'o' | 'O').then_some(command);
        self.begin_insert(composer, before, repeat);
        ComposerOutcome::Changed
    }

    fn single_edit(
        &mut self,
        composer: &mut Composer,
        kind: char,
        count: usize,
        command: String,
    ) -> ComposerOutcome {
        if kind == 'S' {
            return self.line_operator(composer, 'c', count, command);
        }
        let caret = composer.draft.caret();
        let text = composer.draft.text();
        let start_line = line_start(text, caret);
        let end_line = line_end(text, caret);
        let (mut from, mut to) = (caret, caret);
        for _ in 0..count {
            if kind == 'X' {
                if from <= start_line {
                    break;
                }
                from = prev_boundary(text, from);
            } else {
                if to >= end_line {
                    break;
                }
                to = next_boundary(text, to);
            }
        }
        if from == to && kind != 's' {
            return ComposerOutcome::Unchanged;
        }
        let before = Snapshot::of(composer);
        if from < to {
            let killed = composer.draft.text()[from..to].to_string();
            composer.draft.set_kill(killed, false);
            let _ = composer.draft.replace_range(from..to, "");
        }
        if kind == 's' {
            self.begin_insert(composer, before, Some(command));
        } else {
            clamp_caret(composer, false);
            self.finish_change(before, composer, Some(command));
        }
        ComposerOutcome::Changed
    }

    fn visual_command(&mut self, composer: &mut Composer, kind: char) -> ComposerOutcome {
        if matches!(kind, 'v' | 'V') {
            let target = if kind == 'v' {
                VimMode::Visual
            } else {
                VimMode::VisualLine
            };
            self.clear_pending();
            if self.mode == target {
                self.mode = VimMode::Normal;
                self.anchor = None;
                clamp_caret(composer, false);
            } else {
                self.mode = target;
            }
            composer.disarm();
            return ComposerOutcome::Changed;
        }
        if !matches!(kind, 'd' | 'x' | 'y' | 'c' | 'p') {
            return if self.clear_pending() {
                ComposerOutcome::Changed
            } else {
                ComposerOutcome::Unchanged
            };
        }
        self.clear_pending();
        let Some(selection) = self.selection(composer.draft.text(), composer.draft.caret()) else {
            return ComposerOutcome::Unchanged;
        };
        let linewise = self.mode == VimMode::VisualLine;
        let before = Snapshot::of(composer);
        let mut killed = composer.draft.text()[selection.clone()].to_string();
        if linewise && !killed.ends_with('\n') {
            killed.push('\n');
        }
        let original_kill = composer.draft.kill_text().to_string();
        let original_linewise = composer.draft.kill_linewise();
        let mut delete = selection.clone();
        if kind == 'p' && original_linewise {
            let text = composer.draft.text();
            let first = line_start(text, selection.start);
            let last = if selection.end > selection.start {
                prev_boundary(text, selection.end)
            } else {
                selection.start
            };
            delete = first..line_end(text, last);
            killed = text[delete.clone()].to_string();
            if linewise {
                killed.push('\n');
            }
        } else if linewise
            && delete.end == composer.draft.text().len()
            && delete.start > 0
            && matches!(kind, 'd' | 'x')
        {
            delete.start -= 1;
        }
        if kind == 'c'
            && linewise
            && delete.end > delete.start
            && composer.draft.text()[delete.clone()].ends_with('\n')
        {
            delete.end -= 1;
        }
        if kind != 'y' {
            let replacement = if kind == 'p' && !original_kill.is_empty() {
                if original_linewise {
                    original_kill.strip_suffix('\n').unwrap_or(&original_kill)
                } else {
                    &original_kill
                }
            } else {
                ""
            };
            if kind == 'p' && original_kill.is_empty() {
                return ComposerOutcome::Unchanged;
            }
            let insertion = delete.start;
            let _ = composer.draft.replace_range(delete, replacement);
            if kind == 'p' && !replacement.is_empty() {
                let caret = if original_linewise {
                    insertion
                } else {
                    prev_boundary(composer.draft.text(), composer.draft.caret())
                };
                composer.draft.set_caret(caret);
            }
        } else {
            composer.draft.set_caret(selection.start);
        }
        composer.draft.set_kill(killed, linewise);
        self.anchor = None;
        if kind == 'c' {
            self.begin_insert(composer, before, None);
        } else {
            self.mode = VimMode::Normal;
            composer.disarm();
            clamp_caret(composer, false);
            if kind != 'y' && before.text != composer.draft.text() {
                self.push_undo(before);
            }
        }
        ComposerOutcome::Changed
    }

    fn put(
        &mut self,
        composer: &mut Composer,
        after: bool,
        count: usize,
        command: Option<String>,
    ) -> ComposerOutcome {
        let kill = composer.draft.kill_text().to_string();
        if kill.is_empty() {
            return ComposerOutcome::Unchanged;
        }
        let before = Snapshot::of(composer);
        let linewise = composer.draft.kill_linewise();
        let caret = composer.draft.caret();
        if linewise {
            let content = kill.strip_suffix('\n').unwrap_or(&kill);
            let copies = std::iter::repeat_n(content, count)
                .collect::<Vec<_>>()
                .join("\n");
            let text = composer.draft.text();
            let start = line_start(text, caret);
            let end = line_end(text, caret);
            let (at, inserted, result_caret) = if text.is_empty() {
                (
                    0,
                    copies.clone(),
                    if after {
                        copies.rfind('\n').map_or(0, |i| i + 1)
                    } else {
                        0
                    },
                )
            } else if after && end == text.len() {
                (
                    end,
                    format!("\n{copies}"),
                    end + 1 + copies.rfind('\n').map_or(0, |i| i + 1),
                )
            } else if after {
                (
                    end + 1,
                    format!("{copies}\n"),
                    end + 1 + copies.rfind('\n').map_or(0, |i| i + 1),
                )
            } else {
                (start, format!("{copies}\n"), start)
            };
            let _ = composer.draft.replace_range(at..at, &inserted);
            composer.draft.set_caret(result_caret);
        } else {
            let text = composer.draft.text();
            let end = line_end(text, caret);
            let at = if after && caret < end {
                next_boundary(text, caret)
            } else {
                caret
            };
            let inserted = kill.repeat(count);
            let _ = composer.draft.replace_range(at..at, &inserted);
            let last = prev_boundary(composer.draft.text(), composer.draft.caret());
            composer.draft.set_caret(last);
        }
        clamp_caret(composer, false);
        self.finish_change(before, composer, command);
        ComposerOutcome::Changed
    }

    fn repeat(&mut self, composer: &mut Composer, ctx: ComposerContext) -> ComposerOutcome {
        let Some(repeat) = self.last_change.clone() else {
            return ComposerOutcome::Unchanged;
        };
        let before = Snapshot::of(composer);
        self.replaying = true;
        for ch in repeat.keys.chars() {
            let key = Key {
                code: KeyCode::Char(ch),
                mods: Mods::default(),
                kind: KeyKind::Press,
            };
            let _ = self.handle_key(composer, key, None, ctx);
        }
        if self.mode == VimMode::Insert {
            composer.draft.insert_clean(&repeat.inserted);
            self.leave_insert(composer);
        }
        self.replaying = false;
        self.last_change = Some(repeat);
        if before.text != composer.draft.text() {
            self.push_undo(before);
            ComposerOutcome::Changed
        } else {
            ComposerOutcome::Unchanged
        }
    }
}

/// Byte offset where cluster `n` starts. Past the end, the text length.
#[must_use]
pub fn byte_of_cluster(text: &str, n: usize) -> usize {
    text.grapheme_indices(true)
        .nth(n)
        .map_or(text.len(), |(index, _)| index)
}

fn multiply(a: usize, b: usize) -> usize {
    a.saturating_mul(b).min(9999)
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at.min(text.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    text[at..].find('\n').map_or(text.len(), |index| at + index)
}

fn first_nonblank(text: &str, at: usize) -> usize {
    let start = line_start(text, at);
    let end = line_end(text, at);
    text[start..end]
        .grapheme_indices(true)
        .find(|(_, cluster)| *cluster != " " && *cluster != "\t")
        .map_or(start, |(offset, _)| start + offset)
}

pub(crate) fn clamp_caret(composer: &mut Composer, preserve_column: bool) {
    let text = composer.draft.text();
    let caret = composer.draft.caret();
    let start = line_start(text, caret);
    let end = line_end(text, caret);
    if end > start && caret >= end {
        let at = prev_boundary(text, end);
        if preserve_column {
            composer.draft.set_caret_preserving_column(at);
        } else {
            composer.draft.set_caret(at);
        }
    }
}

pub(crate) fn displayed_caret(text: &str, caret: usize) -> usize {
    let start = line_start(text, caret);
    let end = line_end(text, caret);
    if end > start && caret == end {
        prev_boundary(text, end)
    } else {
        caret
    }
}

fn is_motion(ch: char) -> bool {
    matches!(
        ch,
        'h' | 'l' | 'j' | 'k' | '0' | '^' | '$' | 'w' | 'W' | 'b' | 'B' | 'e' | 'E' | 'G'
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    Word,
    Punctuation,
}

fn class(cluster: &str, big: bool) -> Class {
    if cluster.chars().all(char::is_whitespace) {
        Class::Space
    } else if big
        || cluster
            .chars()
            .next()
            .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
    {
        Class::Word
    } else {
        Class::Punctuation
    }
}

fn is_space_at(text: &str, at: usize) -> bool {
    text.get(at..)
        .and_then(|tail| tail.graphemes(true).next())
        .is_none_or(|cluster| class(cluster, false) == Class::Space)
}

fn positions(text: &str) -> Vec<(usize, &str)> {
    text.grapheme_indices(true).collect()
}

fn final_caret(text: &str) -> usize {
    if text.is_empty() || text.ends_with('\n') {
        text.len()
    } else {
        prev_boundary(text, text.len())
    }
}

fn word_forward(text: &str, clusters: &[(usize, &str)], at: usize, big: bool) -> usize {
    let mut i = clusters.partition_point(|(index, _)| *index < at);
    if i == clusters.len() {
        return at;
    }
    let current = class(clusters[i].1, big);
    if current != Class::Space {
        while i < clusters.len() && class(clusters[i].1, big) == current {
            i += 1;
        }
    }
    while i < clusters.len() && class(clusters[i].1, big) == Class::Space {
        i += 1;
    }
    clusters
        .get(i)
        .map_or_else(|| final_caret(text), |(index, _)| *index)
}

fn word_backward(clusters: &[(usize, &str)], at: usize, big: bool) -> usize {
    let mut i = clusters.partition_point(|(index, _)| *index < at);
    if i == 0 {
        return at;
    }
    i -= 1;
    while i > 0 && class(clusters[i].1, big) == Class::Space {
        i -= 1;
    }
    if class(clusters[i].1, big) == Class::Space {
        return clusters[i].0;
    }
    let current = class(clusters[i].1, big);
    while i > 0 && class(clusters[i - 1].1, big) == current {
        i -= 1;
    }
    clusters[i].0
}

fn word_end(
    text: &str,
    clusters: &[(usize, &str)],
    at: usize,
    big: bool,
    include_current: bool,
) -> usize {
    let mut i = clusters.partition_point(|(index, _)| *index < at);
    if i == clusters.len() {
        return at;
    }
    let current = class(clusters[i].1, big);
    if current != Class::Space
        && (include_current || (i + 1 < clusters.len() && class(clusters[i + 1].1, big) == current))
    {
        while i + 1 < clusters.len() && class(clusters[i + 1].1, big) == current {
            i += 1;
        }
        return clusters[i].0;
    }
    if current != Class::Space {
        i += 1;
    }
    while i < clusters.len() && class(clusters[i].1, big) == Class::Space {
        i += 1;
    }
    if i >= clusters.len() {
        return final_caret(text);
    }
    let current = class(clusters[i].1, big);
    while i + 1 < clusters.len() && class(clusters[i + 1].1, big) == current {
        i += 1;
    }
    clusters[i].0
}

fn reverse_find(kind: char) -> char {
    match kind {
        'f' => 'F',
        'F' => 'f',
        't' => 'T',
        'T' => 't',
        _ => kind,
    }
}

fn find_cluster(text: &str, at: usize, kind: char, needle: char, count: usize) -> Option<usize> {
    let start = line_start(text, at);
    let end = line_end(text, at);
    let clusters: Vec<(usize, &str)> = text[start..end]
        .grapheme_indices(true)
        .map(|(i, s)| (start + i, s))
        .collect();
    let needle = needle.to_string();
    let forward = matches!(kind, 'f' | 't');
    let till = matches!(kind, 't' | 'T');
    let mut cursor = at;
    for _ in 0..count {
        let candidates: Box<dyn Iterator<Item = &(usize, &str)>> = if forward {
            Box::new(clusters.iter())
        } else {
            Box::new(clusters.iter().rev())
        };
        let mut found = None;
        for (index, cluster) in candidates {
            if (forward && *index <= cursor) || (!forward && *index >= cursor) || *cluster != needle
            {
                continue;
            }
            let landing = if till {
                if forward {
                    prev_boundary(text, *index)
                } else {
                    next_boundary(text, *index)
                }
            } else {
                *index
            };
            if till && landing == cursor {
                continue;
            }
            found = Some(landing);
            break;
        }
        cursor = found?;
    }
    Some(cursor)
}

fn inserted_text(before: &str, after: &str) -> String {
    let before_clusters: Vec<&str> = before.graphemes(true).collect();
    let after_clusters: Vec<&str> = after.graphemes(true).collect();
    let mut prefix = 0;
    while prefix < before_clusters.len()
        && prefix < after_clusters.len()
        && before_clusters[prefix] == after_clusters[prefix]
    {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < before_clusters.len().saturating_sub(prefix)
        && suffix < after_clusters.len().saturating_sub(prefix)
        && before_clusters[before_clusters.len() - suffix - 1]
            == after_clusters[after_clusters.len() - suffix - 1]
    {
        suffix += 1;
    }
    after_clusters[prefix..after_clusters.len() - suffix].concat()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Composer, ComposerContext, ComposerOutcome, Key, KeyCode, KeyKind, Mods, VimMode};
    use crate::keys::KeyMap;

    fn send(composer: &mut Composer, script: &str) {
        let mut now = Instant::now();
        for ch in script.chars() {
            now += Duration::from_millis(100);
            let key = Key {
                code: KeyCode::Char(ch),
                mods: Mods::default(),
                kind: KeyKind::Press,
            };
            let _ = composer.handle_key(
                key,
                ComposerContext {
                    turn_running: false,
                },
                now,
            );
        }
    }

    #[test]
    fn visual_linewise_put_replaces_the_selected_lines() {
        for (script, linewise, killed) in [("yyjvlp", false, "two"), ("yyjVp", true, "two\n")] {
            let mut composer = Composer::new(KeyMap::defaults());
            composer.set_vim_enabled(true);
            composer.load("one\ntwo\nthree".to_string());
            composer.set_caret(0);
            send(&mut composer, script);
            assert_eq!(composer.text(), "one\none\nthree");
            assert_eq!(composer.caret(), 4);
            assert_eq!(composer.kill_buffer(), killed);
            assert_eq!(composer.kill_linewise(), linewise);
            send(&mut composer, "u");
            assert_eq!(composer.text(), "one\ntwo\nthree");
        }
    }

    #[test]
    fn visual_characterwise_put_keeps_the_previous_line() {
        let mut composer = Composer::new(KeyMap::defaults());
        composer.set_vim_enabled(true);
        composer.load("ab\nlast".to_string());
        composer.set_caret(0);
        send(&mut composer, "yejVp");
        assert_eq!(composer.text(), "ab\nab");
        assert_eq!(composer.caret(), 4);
        assert_eq!(composer.kill_buffer(), "last\n");
        assert!(composer.kill_linewise());
    }

    #[test]
    fn visual_selection_survives_shorter_load_and_clear() {
        let mut composer = Composer::new(KeyMap::defaults());
        composer.set_vim_enabled(true);
        composer.load("hello".to_string());
        composer.set_caret(4);
        send(&mut composer, "v");
        composer.load("🙂".to_string());
        assert_eq!(composer.vim_mode(), Some(VimMode::Visual));
        assert_eq!(
            composer
                .vim_state()
                .and_then(|state| state.selection(composer.text(), composer.caret())),
            Some(0.."🙂".len())
        );
        send(&mut composer, "d");
        assert_eq!(composer.text(), "");

        composer.load("hello".to_string());
        composer.set_caret(4);
        send(&mut composer, "v");
        composer.clear();
        assert_eq!(composer.vim_mode(), Some(VimMode::Visual));
        assert_eq!(
            composer
                .vim_state()
                .and_then(|state| state.selection(composer.text(), composer.caret())),
            Some(0..0)
        );
    }

    #[test]
    fn modified_escape_does_not_change_mode_or_interrupt() {
        let mut composer = Composer::new(KeyMap::defaults());
        composer.set_vim_enabled(true);
        let key = Key {
            code: KeyCode::Esc,
            mods: Mods {
                alt: true,
                ..Mods::default()
            },
            kind: KeyKind::Press,
        };
        let context = ComposerContext { turn_running: true };
        assert_eq!(
            composer.handle_key(key, context, Instant::now()),
            ComposerOutcome::Unchanged
        );
        assert_eq!(composer.vim_mode(), Some(VimMode::Normal));
        send(&mut composer, "i");
        let _ = composer.handle_key(key, context, Instant::now());
        assert_eq!(composer.vim_mode(), Some(VimMode::Insert));
    }
}
