//! The background-launch dialog. It speaks in its own types.
//!
//! The dialog owns the keyboard while it is open. The Prompt field edits through the
//! composer; the other fields are small single-line editors. Drawing goes into a ratatui
//! buffer and never writes an escape sequence.

use std::cell::Cell;
use std::path::PathBuf;
use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::composer::wrap::{Row, wrap};
use crate::composer::{Composer, ComposerContext};
use crate::history::sanitize;
use crate::keys::{Action, Key, KeyChord, KeyCode, KeyContext, KeyKind, KeyMap};

/// The most Prompt lines the box shows; the field scrolls past them.
const MAX_PROMPT_LINES: usize = 6;
/// The widest the box grows.
const MAX_WIDTH: u16 = 120;
/// Columns taken before the value: border, space, label (9), marker (2).
const VALUE_OFFSET: u16 = 13;
/// Columns taken after the value: one of padding and the border.
const RIGHT_MARGIN: u16 = 2;
/// The label column, padded.
const LABEL_WIDTH: usize = 9;
/// The key hint line while editing.
const HINTS: &str = "enter launch · tab next · shift+tab back · esc close";
/// What sits at the right of the Profile row when there is more than one profile.
const PROFILE_HINT: &str = "← → to change";
/// One frame of the spinner every 100 ms.
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Which launch the dialog is collecting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogMode {
    /// A session in the current folder.
    Plain,
    /// A session in a new worktree.
    Worktree,
}

/// Values the dialog opens with.
pub struct DialogDefaults {
    /// Profiles the link can launch in, in display order.
    pub profiles: Vec<String>,
    /// The profile the cursor starts on.
    pub profile: String,
    /// Shown when the link can reach only one profile.
    pub profile_note: Option<String>,
    /// The folder field's starting text.
    pub folder: PathBuf,
    /// A prompt kept from the last time the dialog closed.
    pub draft: Option<String>,
    /// Shown in the model field while it is empty.
    pub model_placeholder: String,
    /// Turns a prompt into the live worktree-name preview.
    pub slug: fn(&str) -> String,
}

/// What the operator submitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogSubmit {
    /// The prompt, as typed.
    pub prompt: String,
    /// The profile the cursor was on.
    pub profile: String,
    /// The folder field, as typed. `~` is not expanded here.
    pub folder: String,
    /// The model field, or `None` while it was left empty.
    pub model: Option<String>,
    /// Whether this was the worktree dialog.
    pub worktree: bool,
    /// The name field once the operator has edited it.
    pub worktree_name: Option<String>,
}

/// What a key did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogAction {
    /// The key changed nothing the caller has to act on.
    None,
    /// The operator asked to launch.
    Launch(DialogSubmit),
    /// The operator closed the dialog. `draft` is the prompt to keep.
    Close { draft: String },
}

/// The fields, in Tab order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Prompt,
    Profile,
    Folder,
    Model,
    Worktree,
}

/// The red line above the hints.
#[derive(Clone, Debug)]
enum Status {
    /// A validation problem; the next key clears it.
    Invalid(String),
    /// A launch failure; it stays until the next launch attempt.
    Failed(String),
}

/// A launch in flight: the stage text and when the first stage began.
#[derive(Clone, Debug)]
struct Progress {
    stage: String,
    since: Instant,
}

/// A single-line text field. The caret is a byte offset on a grapheme boundary.
#[derive(Clone, Debug, Default)]
struct LineField {
    text: String,
    caret: usize,
}

impl LineField {
    fn new(text: String) -> Self {
        let caret = text.len();
        Self { text, caret }
    }

    /// Insert text, dropping line breaks and other controls.
    fn insert(&mut self, text: &str) {
        let kept: String = text.chars().filter(|c| !c.is_control()).collect();
        self.text.insert_str(self.caret, &kept);
        self.caret += kept.len();
    }

    fn prev_boundary(&self) -> usize {
        self.text[..self.caret]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(at, _)| at)
    }

    fn next_boundary(&self) -> usize {
        self.text[self.caret..]
            .graphemes(true)
            .next()
            .map_or(self.text.len(), |cluster| self.caret + cluster.len())
    }

    /// Apply an editing key. True when the text changed.
    fn handle_key(&mut self, key: &Key) -> bool {
        let mods = key.mods;
        if mods.ctrl || mods.alt || mods.super_key || mods.other {
            return false;
        }
        match key.code {
            KeyCode::Char(c) => {
                self.insert(&c.to_string());
                true
            }
            KeyCode::Backspace => {
                let start = self.prev_boundary();
                let changed = start != self.caret;
                self.text.replace_range(start..self.caret, "");
                self.caret = start;
                changed
            }
            KeyCode::Delete => {
                let end = self.next_boundary();
                let changed = end != self.caret;
                self.text.replace_range(self.caret..end, "");
                changed
            }
            KeyCode::Left => {
                self.caret = self.prev_boundary();
                false
            }
            KeyCode::Right => {
                self.caret = self.next_boundary();
                false
            }
            KeyCode::Home => {
                self.caret = 0;
                false
            }
            KeyCode::End => {
                self.caret = self.text.len();
                false
            }
            _ => false,
        }
    }
}

/// The dialog.
pub struct LaunchDialog {
    mode: DialogMode,
    defaults: DialogDefaults,
    keymap: KeyMap,
    focus: Field,
    prompt: Composer,
    profile: String,
    folder: LineField,
    model: LineField,
    /// The worktree name once the operator has edited it; `None` follows the prompt.
    name: Option<LineField>,
    status: Option<Status>,
    /// Shown on the hint line until the next key.
    next_step: Option<String>,
    progress: Option<Progress>,
    /// The Prompt text width of the last [`Self::area`] call, so the composer wraps as drawn.
    text_width: Cell<u16>,
    /// The first Prompt row drawn last time, kept so the window scrolls only when it must.
    scroll_top: Cell<usize>,
}

impl LaunchDialog {
    /// Open the dialog. Nothing is drawn until [`Self::draw`].
    #[must_use]
    pub fn open(mode: DialogMode, defaults: DialogDefaults) -> Self {
        let mut prompt = Composer::new(KeyMap::defaults());
        if let Some(draft) = &defaults.draft {
            // The composer drops control characters. A draft is untrusted text, so show them
            // as stand-ins instead of losing them.
            prompt.load(visible_controls(draft));
        }
        let folder = LineField::new(defaults.folder.to_string_lossy().into_owned());
        let profile = defaults.profile.clone();
        Self {
            mode,
            defaults,
            keymap: KeyMap::defaults(),
            focus: Field::Prompt,
            prompt,
            profile,
            folder,
            model: LineField::default(),
            name: None,
            status: None,
            next_step: None,
            progress: None,
            text_width: Cell::new(0),
            scroll_top: Cell::new(0),
        }
    }

    /// Where the box goes on a screen of this size.
    ///
    /// Width is `min(screen width - 4, 120)`, centred. The top edge is two rows down.
    /// Height is 4 plus the Prompt lines shown (1 to 6) plus the three fields below the
    /// prompt, plus one more row in worktree mode.
    #[must_use]
    pub fn area(&self, screen: Rect) -> Rect {
        let width = screen.width.saturating_sub(4).min(MAX_WIDTH);
        let x = screen
            .x
            .saturating_add(screen.width.saturating_sub(width) / 2);
        self.text_width.set(value_width(width));
        let extra = match self.mode {
            DialogMode::Worktree => 1,
            DialogMode::Plain => 0,
        };
        let lines = self.prompt_rows(width).len().min(MAX_PROMPT_LINES);
        let height = 4 + u16::try_from(lines).unwrap_or(1) + 3 + extra;
        Rect {
            x,
            y: screen.y.saturating_add(2),
            width,
            height,
        }
    }

    /// Apply one key chord.
    pub fn handle_key(&mut self, key: KeyChord) -> DialogAction {
        self.handle_pressed(key.to_key())
    }

    /// Apply one key as the terminal reported it. Esc closes. Enter launches and Tab moves.
    /// Everything else edits the focused field, or does nothing while a launch is starting.
    pub fn handle_pressed(&mut self, key: Key) -> DialogAction {
        if key.kind == KeyKind::Release {
            return DialogAction::None;
        }
        self.next_step = None;
        let ctrl_c = key.code == KeyCode::Char('c') && key.mods.ctrl;
        if key.code == KeyCode::Esc || ctrl_c {
            return DialogAction::Close {
                draft: self.prompt.text().to_owned(),
            };
        }
        if self.progress.is_some() {
            return DialogAction::None;
        }
        if matches!(self.status, Some(Status::Invalid(_))) {
            self.status = None;
        }
        if key.code == KeyCode::Tab {
            self.move_focus(!key.mods.shift);
            return DialogAction::None;
        }
        let plain = !(key.mods.ctrl || key.mods.alt || key.mods.super_key || key.mods.shift);
        let launches = match self.focus {
            Field::Prompt => {
                self.keymap.resolve(&key, KeyContext::Composer) == Some(Action::Submit)
                    && !self.prompt.is_collecting_burst()
            }
            _ => key.code == KeyCode::Enter && plain,
        };
        if launches {
            self.status = None;
            return DialogAction::Launch(self.submit());
        }
        match self.focus {
            Field::Prompt => {
                self.prime_composer();
                let ctx = ComposerContext {
                    turn_running: false,
                };
                self.prompt.handle_key(key, ctx, Instant::now());
            }
            Field::Profile => match key.code {
                KeyCode::Left => self.cycle_profile(false),
                KeyCode::Right => self.cycle_profile(true),
                _ => {}
            },
            Field::Folder => {
                self.folder.handle_key(&key);
            }
            Field::Model => {
                self.model.handle_key(&key);
            }
            Field::Worktree => {
                // The first edit freezes the preview into a name of the operator's own.
                let mut field = self
                    .name
                    .clone()
                    .unwrap_or_else(|| LineField::new(self.preview_name()));
                let changed = field.handle_key(&key);
                if changed || self.name.is_some() {
                    self.name = Some(field);
                }
            }
        }
        DialogAction::None
    }

    /// Insert pasted text into the focused field.
    pub fn handle_paste(&mut self, text: &str) {
        if self.progress.is_some() {
            return;
        }
        if matches!(self.status, Some(Status::Invalid(_))) {
            self.status = None;
        }
        match self.focus {
            Field::Prompt => {
                self.prime_composer();
                self.prompt.handle_paste(text);
            }
            Field::Profile => {}
            Field::Folder => self.folder.insert(text),
            Field::Model => self.model.insert(text),
            Field::Worktree => {
                let mut field = self
                    .name
                    .take()
                    .unwrap_or_else(|| LineField::new(self.preview_name()));
                field.insert(text);
                self.name = Some(field);
            }
        }
    }

    /// Flush a pasted run the composer is still collecting once `now` has reached its due time.
    pub fn tick(&mut self, now: Instant) {
        self.prompt.tick(now);
    }

    /// When [`Self::tick`] next has work to do.
    #[must_use]
    pub fn next_tick(&self) -> Option<Instant> {
        self.prompt.next_tick()
    }

    /// Show a stage line. The fields turn read-only and the hint line shows the stage with a
    /// spinner and the whole seconds since the launch began.
    pub fn show_progress(&mut self, text: &str) {
        let since = self
            .progress
            .take()
            .map_or_else(Instant::now, |progress| progress.since);
        self.progress = Some(Progress {
            stage: text.to_owned(),
            since,
        });
        self.status = None;
        self.next_step = None;
    }

    /// Show a launch failure. Every field is kept; the next step takes the hint line until the
    /// next key.
    pub fn show_error(&mut self, what: &str, next_step: &str) {
        self.progress = None;
        self.status = Some(Status::Failed(format!("Could not launch: {what}")));
        self.next_step = Some(next_step.to_owned());
    }

    /// Show a validation line and move focus to the field it is about.
    pub fn show_invalid(&mut self, text: &str) {
        self.progress = None;
        self.next_step = None;
        if let Some(field) = self.field_for(text) {
            self.focus = field;
        }
        self.status = Some(Status::Invalid(text.to_owned()));
    }

    /// Draw the box into `area`.
    pub fn draw(&self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                buf[(x, y)].reset();
            }
        }
        if area.width < 24 || area.height < 8 {
            return;
        }
        let dim = Style::default().add_modifier(Modifier::DIM);
        let marker = Style::default().add_modifier(Modifier::BOLD);
        let red = Style::default().fg(Color::Red);
        let right = area.right() - 1;
        let value_width = value_width(area.width);
        let interior = usize::from(area.width) - 2;

        // The top edge, with the title set into it.
        let title = match self.mode {
            DialogMode::Plain => "New background session",
            DialogMode::Worktree => "New background session in a worktree",
        };
        let title = cut(title, interior.saturating_sub(4));
        let head = format!(
            "╭─ {title} {}╮",
            "─".repeat(interior.saturating_sub(3 + title.width()))
        );
        buf.set_string(area.x, area.y, head, Style::default());

        let editable = self.progress.is_none();
        let mut y = area.y + 1;

        // Prompt rows.
        let rows = self.prompt_rows(area.width);
        let text = self.prompt.text();
        let caret_row = row_of(&rows, self.prompt.caret());
        let shown = rows.len().min(MAX_PROMPT_LINES);
        let mut first = self.scroll_top.get().min(rows.len() - shown);
        if caret_row < first {
            first = caret_row;
        }
        if caret_row >= first + shown {
            first = caret_row + 1 - shown;
        }
        self.scroll_top.set(first);
        let focused = self.focus == Field::Prompt && editable;
        for offset in 0..shown {
            let index = first + offset;
            let label = if offset == 0 { "Prompt" } else { "" };
            self.frame_row(buf, area, y, label, focused && offset == 0, marker);
            let row = &rows[index];
            let caret = (focused && caret_row == index).then(|| self.prompt.caret());
            put_text(
                buf,
                area.x + VALUE_OFFSET,
                y,
                area.x + VALUE_OFFSET + value_width,
                &text[row.start..row.end],
                row.start,
                caret,
                Style::default(),
            );
            y += 1;
        }

        // Profile.
        let focused = self.focus == Field::Profile && editable;
        self.frame_row(buf, area, y, "Profile", focused, marker);
        let value_end = area.x + VALUE_OFFSET + value_width;
        let hint = if self.defaults.profiles.len() > 1 {
            Some(PROFILE_HINT.to_owned())
        } else {
            self.defaults
                .profile_note
                .as_deref()
                .map(|note| cut(&sanitize(note), usize::from(value_width).saturating_sub(10)))
        };
        let hint_start = hint.as_ref().map_or(value_end, |hint| {
            value_end.saturating_sub(u16::try_from(hint.width()).unwrap_or(0))
        });
        put_text(
            buf,
            area.x + VALUE_OFFSET,
            y,
            hint_start.saturating_sub(1).max(area.x + VALUE_OFFSET),
            &self.profile,
            0,
            None,
            Style::default(),
        );
        if let Some(hint) = hint {
            buf.set_string(hint_start, y, hint, dim);
        }
        y += 1;

        // Folder.
        let focused = self.focus == Field::Folder && editable;
        self.frame_row(buf, area, y, "Folder", focused, marker);
        put_text(
            buf,
            area.x + VALUE_OFFSET,
            y,
            value_end,
            &self.folder.text,
            0,
            focused.then_some(self.folder.caret),
            Style::default(),
        );
        y += 1;

        // Model.
        let focused = self.focus == Field::Model && editable;
        self.frame_row(buf, area, y, "Model", focused, marker);
        if self.model.text.is_empty() {
            let caret = focused.then_some(0);
            put_text(
                buf,
                area.x + VALUE_OFFSET,
                y,
                value_end,
                &self.defaults.model_placeholder,
                0,
                caret,
                dim,
            );
        } else {
            put_text(
                buf,
                area.x + VALUE_OFFSET,
                y,
                value_end,
                &self.model.text,
                0,
                focused.then_some(self.model.caret),
                Style::default(),
            );
        }
        y += 1;

        // Worktree name.
        if self.mode == DialogMode::Worktree {
            let focused = self.focus == Field::Worktree && editable;
            self.frame_row(buf, area, y, "Worktree", focused, marker);
            let (name, caret) = match &self.name {
                Some(field) => (field.text.clone(), focused.then_some(field.caret)),
                None => {
                    let preview = self.preview_name();
                    let end = preview.len();
                    (preview, focused.then_some(end))
                }
            };
            put_text(
                buf,
                area.x + VALUE_OFFSET,
                y,
                value_end,
                &name,
                0,
                caret,
                Style::default(),
            );
            y += 1;
        }

        // The line above the hints.
        self.frame_row_blank(buf, area, y);
        let (line, style) = match &self.status {
            Some(Status::Invalid(text) | Status::Failed(text)) => (Some(text.as_str()), red),
            None => (None, Style::default()),
        };
        if let Some(line) = line {
            put_text(
                buf,
                area.x + 2,
                y,
                right - 1,
                &cut(&sanitize(line), usize::from(area.width).saturating_sub(4)),
                0,
                None,
                style,
            );
        }
        y += 1;

        // The hint line.
        self.frame_row_blank(buf, area, y);
        let (line, style) = if let Some(progress) = &self.progress {
            let elapsed = progress.since.elapsed();
            let frame = usize::try_from(elapsed.as_millis() / 100).unwrap_or(0) % SPINNER.len();
            let stage = format!(
                "{} {} {}s",
                SPINNER[frame],
                sanitize(&progress.stage),
                elapsed.as_secs()
            );
            (stage, Style::default())
        } else if let Some(next_step) = &self.next_step {
            (sanitize(next_step).into_owned(), Style::default())
        } else {
            (HINTS.to_owned(), dim)
        };
        put_text(
            buf,
            area.x + 2,
            y,
            right - 1,
            &cut(&line, usize::from(area.width).saturating_sub(4)),
            0,
            None,
            style,
        );
        y += 1;

        // The bottom edge.
        let foot = format!("╰{}╯", "─".repeat(interior));
        buf.set_string(area.x, y, foot, Style::default());
    }

    /// The border, the label and the marker of one field row. The value is drawn by the caller.
    fn frame_row(
        &self,
        buf: &mut Buffer,
        area: Rect,
        y: u16,
        label: &str,
        focused: bool,
        mark: Style,
    ) {
        self.frame_row_blank(buf, area, y);
        buf.set_string(
            area.x + 2,
            y,
            format!("{label:<LABEL_WIDTH$}"),
            Style::default(),
        );
        if focused {
            buf.set_string(area.x + 2 + LABEL_WIDTH as u16, y, "› ", mark);
        }
    }

    /// The two side borders of a row.
    fn frame_row_blank(&self, buf: &mut Buffer, area: Rect, y: u16) {
        buf.set_string(area.x, y, "│", Style::default());
        buf.set_string(area.right() - 1, y, "│", Style::default());
    }

    /// The Prompt text wrapped to a box of this width, with one cell held for the caret.
    fn prompt_rows(&self, box_width: u16) -> Vec<Row> {
        let cells = usize::from(value_width(box_width)).saturating_sub(1).max(1);
        let mut rows = wrap(self.prompt.text(), cells);
        if rows.is_empty() {
            rows.push(Row {
                start: 0,
                end: 0,
                hard_break_after: false,
            });
        }
        rows
    }

    /// Tell the composer how wide the Prompt text is, so Up and Down follow the drawn rows.
    fn prime_composer(&mut self) {
        let cells = self.text_width.get();
        if cells > 0 {
            // The composer's own box leaves seven columns beside its text.
            let _ = self.prompt.section(cells.saturating_add(7), 8);
        }
    }

    /// The worktree name the prompt would give.
    fn preview_name(&self) -> String {
        (self.defaults.slug)(self.prompt.text())
    }

    fn fields(&self) -> &'static [Field] {
        match self.mode {
            DialogMode::Plain => &[Field::Prompt, Field::Profile, Field::Folder, Field::Model],
            DialogMode::Worktree => &[
                Field::Prompt,
                Field::Profile,
                Field::Folder,
                Field::Model,
                Field::Worktree,
            ],
        }
    }

    fn move_focus(&mut self, forward: bool) {
        let fields = self.fields();
        let at = fields.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = if forward {
            (at + 1) % fields.len()
        } else {
            (at + fields.len() - 1) % fields.len()
        };
        self.focus = fields[next];
    }

    fn cycle_profile(&mut self, forward: bool) {
        let profiles = &self.defaults.profiles;
        if profiles.is_empty() {
            return;
        }
        let next = match profiles.iter().position(|p| *p == self.profile) {
            None => 0,
            Some(at) if forward => (at + 1) % profiles.len(),
            Some(at) => (at + profiles.len() - 1) % profiles.len(),
        };
        self.profile.clone_from(&profiles[next]);
    }

    /// The field a validation line is about, from its wording.
    fn field_for(&self, text: &str) -> Option<Field> {
        if text.starts_with("Write a prompt") {
            Some(Field::Prompt)
        } else if text.starts_with("The folder") {
            Some(Field::Folder)
        } else if text.starts_with("The worktree name") && self.mode == DialogMode::Worktree {
            Some(Field::Worktree)
        } else {
            None
        }
    }

    fn submit(&self) -> DialogSubmit {
        let model = self.model.text.trim();
        let worktree = self.mode == DialogMode::Worktree;
        DialogSubmit {
            prompt: self.prompt.text().to_owned(),
            profile: self.profile.clone(),
            folder: self.folder.text.clone(),
            model: (!model.is_empty()).then(|| model.to_owned()),
            worktree,
            worktree_name: self
                .name
                .as_ref()
                .filter(|_| worktree)
                .map(|field| field.text.clone()),
        }
    }
}

/// Columns the value of a field may use in a box this wide.
fn value_width(box_width: u16) -> u16 {
    box_width.saturating_sub(VALUE_OFFSET + RIGHT_MARGIN)
}

/// The row that holds the caret. A caret at the end of a soft-wrapped row belongs to the next.
fn row_of(rows: &[Row], caret: usize) -> usize {
    let last = rows.len().saturating_sub(1);
    for (index, row) in rows.iter().enumerate() {
        if caret < row.start {
            return index.saturating_sub(1);
        }
        if caret < row.end || (caret == row.end && (row.hard_break_after || index == last)) {
            return index;
        }
    }
    last
}

/// Draw `text` from column `x`, stopping before `limit`. Control and direction characters are
/// drawn as stand-ins. The cluster at byte `caret` (counted from `base`) is reversed, or a
/// reversed space follows the text when the caret sits at its end. Returns the next column.
#[allow(clippy::too_many_arguments)]
fn put_text(
    buf: &mut Buffer,
    mut x: u16,
    y: u16,
    limit: u16,
    text: &str,
    base: usize,
    caret: Option<usize>,
    style: Style,
) -> u16 {
    let reversed = style.add_modifier(Modifier::REVERSED);
    for (at, cluster) in text.grapheme_indices(true) {
        let drawn = sanitize(cluster);
        let width = u16::try_from(drawn.width()).unwrap_or(u16::MAX);
        if width == 0 {
            continue;
        }
        if x.saturating_add(width) > limit {
            return x;
        }
        let cell_style = if caret == Some(base + at) {
            reversed
        } else {
            style
        };
        buf.set_stringn(x, y, drawn.as_ref(), usize::from(width), cell_style);
        x += width;
    }
    if caret == Some(base + text.len()) && x < limit {
        buf.set_string(x, y, " ", reversed);
        x += 1;
    }
    x
}

/// `text` cut to `width` cells, with an ellipsis when it did not fit.
fn cut(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for cluster in text.graphemes(true) {
        let step = cluster.width();
        if used + step + 1 > width {
            break;
        }
        out.push_str(cluster);
        used += step;
    }
    out.push('…');
    out
}

/// `text` with every control character except line breaks and tabs written as its stand-in.
fn visible_controls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() && c != '\n' && c != '\t' {
            out.push_str(&sanitize(c.encode_utf8(&mut [0; 4])));
        } else {
            out.push(c);
        }
    }
    out
}

/// Values collected for a session owned by an external agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpLaunchSubmit {
    pub agent_id: String,
    pub cwd: PathBuf,
    pub prompt: String,
}

/// The fleet launch affordance for an external agent.
pub struct AcpLaunchDialog;

impl AcpLaunchDialog {
    pub fn from_fleet_action(
        _action: talaria_core::fleet_view::FleetAction,
        _agents: Vec<(String, String)>,
        _launch_directory: PathBuf,
    ) -> Option<Self> {
        todo!("add external agent launch")
    }
    pub fn choose_agent(&mut self, _agent_id: &str) {
        todo!("add external agent launch")
    }
    pub fn set_prompt(&mut self, _prompt: &str) {
        todo!("add external agent launch")
    }
    pub fn set_cwd(&mut self, _cwd: PathBuf) {
        todo!("add external agent launch")
    }
    pub fn submit(&self) -> Result<AcpLaunchSubmit, String> {
        todo!("add external agent launch")
    }
}
