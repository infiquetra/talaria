//! Segment text, priority and the one-row layout.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::review::{LabelTone, PrLabel};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{Connection, RunMode, StatusPalette, Tone, TurnState};
use crate::history::sanitize;
use crate::live::LiveSection;

/// The separator drawn between two segments: a space, a box drawing, a space.
const SEPARATOR: &str = " │ ";
/// The width of a folder path, in columns.
const FOLDER_WIDTH: usize = 32;
/// The width of a branch name, in columns.
const BRANCH_WIDTH: usize = 24;
/// The mark that ends a cut text.
const ELLIPSIS: char = '…';
/// The prefix of a path that was cut.
const CUT_PREFIX: &str = "…/";

/// The focused session: its identifier and its human-facing title.
#[derive(Clone, Debug)]
pub struct SessionRef {
    /// The session identifier.
    pub id: String,
    /// The title, when the session has one.
    pub title: Option<String>,
}

/// The token counts of the focused session.
#[derive(Clone, Copy, Debug)]
pub struct TokenUsage {
    /// Tokens read.
    pub input_tokens: u64,
    /// Tokens written.
    pub output_tokens: u64,
}

/// Everything the line shows, handed over by the caller.
#[derive(Clone, Debug, Default)]
pub struct StatusInputs {
    /// Workflow routing is enabled for the foreground session.
    pub ultracode: bool,
    /// The attach state of the link.
    pub connection: Connection,
    /// The model in use, with its provider path.
    pub model: Option<String>,
    /// The configured reasoning effort.
    pub reasoning_effort: Option<String>,
    /// The effort actually sent on the wire.
    pub reasoning_effort_wire: Option<String>,
    /// The working folder on the gateway's machine.
    pub working_dir: Option<String>,
    /// The client's own home folder, used to write `~`.
    pub home_dir: Option<String>,
    /// The branch of the working folder.
    pub branch: Option<String>,
    /// The context use, in percent.
    pub context_percent: Option<i64>,
    /// Whether the context figure is an estimate.
    pub context_estimated: bool,
    /// The session cost in dollars.
    pub cost_usd: Option<f64>,
    /// The cost status reported with the cost.
    pub cost_status: Option<String>,
    /// Prompts waiting for the operator.
    pub needs_you: u32,
    /// Background rows whose turn failed.
    pub failed: u32,
    /// Background rows whose turn is done.
    pub done: u32,
    /// Sessions working on a turn.
    pub working: u32,
    /// Whether the client is live or replaying.
    pub mode: RunMode,
    /// The focused session.
    pub session: Option<SessionRef>,
    /// The state of the focused session's turn.
    pub turn: TurnState,
    /// Outstanding prompts of the focused session.
    pub pending_prompts: u32,
    /// Sub-agents running.
    pub subagents_active: u32,
    /// Sub-agents in a terminal state.
    pub subagents_terminal: u32,
    /// The token counts, when they have been observed.
    pub usage_tokens: Option<TokenUsage>,
    /// The approvals segment, when the permissions state shows one (#237).
    pub approvals: Option<ApprovalsSegment>,
    /// The foreground folder's pull-request label, when its branch has one (#245).
    pub pull_request: Option<PrLabel>,
}

/// The approvals segment text and tone, handed over by the caller.
#[derive(Clone, Debug)]
pub struct ApprovalsSegment {
    /// The full segment text, for example `approvals: smart`.
    pub text: String,
    /// The tone the segment is drawn with.
    pub tone: Tone,
}

/// The segment the external command contributes.
#[derive(Clone, Debug)]
pub struct ExternalSegment {
    /// The text to show.
    pub text: String,
    /// The tone to draw it in.
    pub tone: Tone,
}

/// The segments of the status line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Segment {
    /// Workflow routing for this session.
    Ultracode,
    /// The attach state.
    Connection,
    /// Prompts waiting for the operator.
    NeedsYou,
    /// The model and its effort.
    Model,
    /// The context use.
    Context,
    /// The working folder and branch.
    Folder,
    /// Sessions working on a turn.
    Working,
    /// The session cost.
    Cost,
    /// The external command's text.
    External,
    /// The approvals state (#237).
    Approvals,
    /// Background rows whose turn failed.
    Failed,
    /// Background rows whose turn is done.
    Done,
    /// The foreground folder's pull-request label (#245).
    PullRequest,
}

impl Segment {
    /// Every segment, from the one dropped last to the one dropped first.
    pub const ALL: [Segment; 13] = [
        Segment::Connection,
        Segment::NeedsYou,
        Segment::Approvals,
        Segment::Failed,
        Segment::Model,
        Segment::Context,
        Segment::Folder,
        Segment::Working,
        Segment::Done,
        Segment::Cost,
        Segment::PullRequest,
        Segment::External,
        Segment::Ultracode,
    ];

    /// Smaller priorities are retained longer.
    pub fn priority(self) -> u8 {
        match self {
            Segment::Connection => 1,
            Segment::NeedsYou => 2,
            Segment::Approvals => 3,
            Segment::Failed => 4,
            Segment::Model => 5,
            Segment::Context => 6,
            Segment::Folder => 7,
            Segment::Working => 8,
            Segment::Done => 9,
            Segment::Cost => 10,
            Segment::PullRequest => 11,
            Segment::External => 12,
            Segment::Ultracode => 13,
        }
    }

    /// Left to right position, 0 first.
    pub fn position(self) -> u8 {
        match self {
            Segment::Connection => 0,
            Segment::Model => 1,
            Segment::Ultracode => 2,
            Segment::Approvals => 3,
            Segment::Folder => 4,
            Segment::Context => 5,
            Segment::Cost => 6,
            Segment::NeedsYou => 7,
            Segment::Failed => 8,
            Segment::Working => 9,
            Segment::Done => 10,
            Segment::PullRequest => 11,
            Segment::External => 12,
        }
    }
}

/// The pieces of one status line.
#[derive(Clone, Debug)]
pub struct LinePlan {
    /// The segments, left to right.
    pub pieces: Vec<(Segment, String, Tone)>,
}

impl LinePlan {
    /// The line as one string: a leading blank, then the pieces joined by the separator.
    pub fn text(&self) -> String {
        if self.pieces.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for (index, (_, text, _)) in self.pieces.iter().enumerate() {
            if index == 0 {
                out.push(' ');
            } else {
                out.push_str(SEPARATOR);
            }
            out.push_str(text);
        }
        out
    }

    /// The display width of the line.
    pub fn width(&self) -> usize {
        if self.pieces.is_empty() {
            return 0;
        }
        let text: usize = self
            .pieces
            .iter()
            .map(|(_, piece, _)| UnicodeWidthStr::width(piece.as_str()))
            .sum();
        1 + text + 3 * (self.pieces.len() - 1)
    }
}

/// The line for `inputs` at `cols` columns, with the external segment when there is one.
pub fn plan(inputs: &StatusInputs, external: Option<&ExternalSegment>, cols: u16) -> LinePlan {
    let mut pieces: Vec<(Segment, String, Tone)> = Vec::new();
    add(&mut pieces, Segment::Connection, connection_segment(inputs));
    add(&mut pieces, Segment::Model, model_segment(inputs));
    add(
        &mut pieces,
        Segment::Ultracode,
        inputs.ultracode.then(|| ("ultracode".into(), Tone::Accent)),
    );
    add(&mut pieces, Segment::Approvals, approvals_segment(inputs));
    add(&mut pieces, Segment::Folder, folder_segment(inputs));
    add(&mut pieces, Segment::Context, context_segment(inputs));
    add(&mut pieces, Segment::Cost, cost_segment(inputs));
    add(&mut pieces, Segment::NeedsYou, needs_you_segment(inputs));
    add(&mut pieces, Segment::Failed, failed_segment(inputs));
    add(&mut pieces, Segment::Working, working_segment(inputs));
    add(&mut pieces, Segment::Done, done_segment(inputs));
    add(
        &mut pieces,
        Segment::PullRequest,
        pull_request_segment(inputs),
    );
    add(&mut pieces, Segment::External, external_segment(external));

    // One blank column on the left is reserved, so the line never reaches the last column.
    let available = usize::from(cols.saturating_sub(1));
    loop {
        let need: usize = pieces
            .iter()
            .map(|(_, text, _)| UnicodeWidthStr::width(text.as_str()))
            .sum();
        let need = need + 3 * pieces.len().saturating_sub(1);
        if need <= available {
            break;
        }
        if pieces.len() == 1 {
            let (segment, text, tone) = pieces.remove(0);
            let cut = cut_to_width(&text, available);
            if !cut.is_empty() {
                pieces.push((segment, cut, tone));
            }
            break;
        }
        // Whole segments go, lowest priority first.
        let index = pieces
            .iter()
            .enumerate()
            .max_by_key(|(_, (segment, _, _))| segment.priority())
            .map(|(index, _)| index)
            .expect("at least two pieces");
        pieces.remove(index);
    }

    pieces.sort_by_key(|(segment, _, _)| segment.position());
    LinePlan { pieces }
}

fn add(pieces: &mut Vec<(Segment, String, Tone)>, segment: Segment, part: Option<(String, Tone)>) {
    if let Some((text, tone)) = part {
        pieces.push((segment, text, tone));
    }
}

/// The connection segment: hidden while the link is up.
fn connection_segment(inputs: &StatusInputs) -> Option<(String, Tone)> {
    let (text, tone) = match inputs.connection {
        Connection::Connected => return None,
        Connection::Disconnected => ("disconnected", Tone::Error),
        Connection::Connecting => ("connecting…", Tone::Warning),
        Connection::Reconnecting => ("reconnecting…", Tone::Warning),
        Connection::AuthFailed => ("auth failed", Tone::Error),
    };
    Some((text.to_string(), tone))
}

/// The model segment: the name after the last slash, then the effort.
fn model_segment(inputs: &StatusInputs) -> Option<(String, Tone)> {
    let model = inputs.model.as_deref()?;
    let name = model.rsplit('/').next().unwrap_or("").trim();
    if name.is_empty() {
        return None;
    }
    let effort = inputs
        .reasoning_effort
        .as_deref()
        .map(str::trim)
        .filter(|effort| !effort.is_empty())
        .map(str::to_lowercase);
    let wire = inputs
        .reasoning_effort_wire
        .as_deref()
        .map(str::trim)
        .filter(|wire| !wire.is_empty())
        .map(str::to_lowercase);

    let mut text = String::from(name);
    if let Some(effort) = effort {
        text.push(' ');
        text.push_str(&effort);
        // The arrow appears only when the wire level differs from the effort.
        if let Some(wire) = wire
            && wire != effort
        {
            text.push('→');
            text.push_str(&wire);
        }
    }
    Some((sanitize(&text).into_owned(), Tone::Muted))
}

/// The approvals segment: the text and tone the caller handed over, when there is one.
fn approvals_segment(inputs: &StatusInputs) -> Option<(String, Tone)> {
    let approvals = inputs.approvals.as_ref()?;
    let text = sanitize(approvals.text.trim()).into_owned();
    if text.is_empty() {
        return None;
    }
    Some((text, approvals.tone))
}

/// The folder segment: the working folder and the branch.
fn folder_segment(inputs: &StatusInputs) -> Option<(String, Tone)> {
    let folder = inputs
        .working_dir
        .as_deref()
        .map(str::trim)
        .filter(|folder| !folder.is_empty());
    let branch = inputs
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|branch| !branch.is_empty());
    if folder.is_none() && branch.is_none() {
        return None;
    }
    let mut text = String::new();
    if let Some(folder) = folder {
        text.push_str(&shorten_path(folder, inputs.home_dir.as_deref()));
    }
    if let Some(branch) = branch {
        let branch = sanitize(branch).into_owned();
        let branch = if UnicodeWidthStr::width(branch.as_str()) > BRANCH_WIDTH {
            cut_to_width(&branch, BRANCH_WIDTH)
        } else {
            branch
        };
        if text.is_empty() {
            text.push('(');
        } else {
            text.push_str(" (");
        }
        text.push_str(&branch);
        text.push(')');
    }
    Some((text, Tone::Muted))
}

/// The home-relative path, cut to [`FOLDER_WIDTH`] columns.
fn shorten_path(folder: &str, home: Option<&str>) -> String {
    let home = home
        .map(str::trim)
        .map(|home| home.trim_end_matches('/'))
        .filter(|home| !home.is_empty());
    let relative = match home {
        Some(home) if folder == home => "~".to_string(),
        Some(home) => match folder.strip_prefix(home) {
            Some(rest) if rest.starts_with('/') => format!("~{rest}"),
            _ => folder.to_string(),
        },
        None => folder.to_string(),
    };
    let text = sanitize(&relative).into_owned();
    if UnicodeWidthStr::width(text.as_str()) <= FOLDER_WIDTH {
        return text;
    }
    // The most trailing components that fit behind the cut prefix.
    let components: Vec<&str> = text.split('/').filter(|part| !part.is_empty()).collect();
    let Some((last, rest)) = components.split_last() else {
        return text;
    };
    let budget = FOLDER_WIDTH - UnicodeWidthStr::width(CUT_PREFIX);
    if UnicodeWidthStr::width(*last) > budget {
        return format!("{CUT_PREFIX}{}", cut_to_width(last, budget));
    }
    let mut kept: Vec<&str> = vec![last];
    let mut used = UnicodeWidthStr::width(CUT_PREFIX) + UnicodeWidthStr::width(*last);
    for component in rest.iter().rev() {
        let next = used + 1 + UnicodeWidthStr::width(*component);
        if next > FOLDER_WIDTH {
            break;
        }
        used = next;
        kept.push(component);
    }
    kept.reverse();
    format!("{CUT_PREFIX}{}", kept.join("/"))
}

/// The context segment: hidden until a figure is reported.
fn context_segment(inputs: &StatusInputs) -> Option<(String, Tone)> {
    let percent = inputs.context_percent?;
    let mark = if inputs.context_estimated { "~" } else { "" };
    Some((format!("ctx {mark}{}%", percent.clamp(0, 100)), Tone::Muted))
}

/// The cost segment: hidden without a figure, and never a zero that was not reported.
fn cost_segment(inputs: &StatusInputs) -> Option<(String, Tone)> {
    let usd = inputs.cost_usd?;
    if !usd.is_finite() || usd < 0.0 {
        return None;
    }
    let status = inputs
        .cost_status
        .as_deref()
        .map(|status| status.trim().to_lowercase());
    if matches!(status.as_deref(), Some("unknown") | Some("included")) {
        return None;
    }
    let mark = if status.as_deref() == Some("estimated") {
        "~"
    } else {
        ""
    };
    if usd > 0.0 && usd < 0.01 {
        return Some((format!("{mark}<$0.01"), Tone::Muted));
    }
    Some((format!("{mark}${usd:.2}"), Tone::Muted))
}

/// The needs-you segment: the operator is blocked.
fn needs_you_segment(inputs: &StatusInputs) -> Option<(String, Tone)> {
    if inputs.needs_you == 0 {
        return None;
    }
    let word = if inputs.needs_you == 1 {
        "needs you"
    } else {
        "need you"
    };
    Some((format!("⚑ {} {word}", inputs.needs_you), Tone::Attention))
}

/// The working segment: sessions running a turn.
fn working_segment(inputs: &StatusInputs) -> Option<(String, Tone)> {
    if inputs.working == 0 {
        return None;
    }
    Some((format!("{} working", inputs.working), Tone::Muted))
}

/// The failed segment: a background turn failed.
fn failed_segment(inputs: &StatusInputs) -> Option<(String, Tone)> {
    if inputs.failed == 0 {
        return None;
    }
    Some((format!("✗ {} failed", inputs.failed), Tone::Error))
}

/// The done segment: a background turn is done.
fn done_segment(inputs: &StatusInputs) -> Option<(String, Tone)> {
    if inputs.done == 0 {
        return None;
    }
    Some((format!("✓ {} done", inputs.done), Tone::Success))
}

/// The pull-request segment: the foreground folder's label, or nothing (#245).
fn pull_request_segment(inputs: &StatusInputs) -> Option<(String, Tone)> {
    let label = inputs.pull_request.as_ref()?;
    let text = sanitize(label.text.trim()).into_owned();
    if text.is_empty() {
        return None;
    }
    Some((text, label_tone(label.tone)))
}

/// The status-line tone for a pull-request label. The line has no accent tone, so a merged
/// pull request (`Accent`) draws in the terminal's own foreground.
fn label_tone(tone: LabelTone) -> Tone {
    match tone {
        LabelTone::Success => Tone::Success,
        LabelTone::Error => Tone::Error,
        LabelTone::Warning => Tone::Warning,
        LabelTone::Muted => Tone::Muted,
        LabelTone::Accent => Tone::Text,
    }
}

/// The external segment: the command's own text.
fn external_segment(external: Option<&ExternalSegment>) -> Option<(String, Tone)> {
    let external = external?;
    let text = external.text.trim();
    if text.is_empty() {
        return None;
    }
    Some((sanitize(text).into_owned(), external.tone))
}

/// Cut `text` to `available` columns, ending with an ellipsis.
fn cut_to_width(text: &str, available: usize) -> String {
    if available == 0 {
        return String::new();
    }
    let budget = available - 1;
    let mut out = String::new();
    let mut used = 0usize;
    for character in text.chars() {
        let width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + width > budget {
            break;
        }
        out.push(character);
        used += width;
    }
    out.push(ELLIPSIS);
    out
}

/// The one-row live section.
pub struct StatusSection<'a> {
    inputs: &'a StatusInputs,
    external: Option<&'a ExternalSegment>,
    palette: &'a StatusPalette,
}

impl<'a> StatusSection<'a> {
    /// The section for `inputs`, with the external segment when there is one.
    pub fn new(
        inputs: &'a StatusInputs,
        external: Option<&'a ExternalSegment>,
        palette: &'a StatusPalette,
    ) -> Self {
        Self {
            inputs,
            external,
            palette,
        }
    }

    /// Draw the line into `buf` over `area`.
    ///
    /// The row is owned by the section: it is cleared first, so a line with nothing to show
    /// leaves blanks rather than what was there before, and the rest of the row is padded with
    /// blanks.
    pub fn render_into(&self, area: Rect, buf: &mut Buffer) {
        let region = buf.area.intersection(area);
        if region.width == 0 || region.height == 0 {
            return;
        }
        let row = region.y;
        // The section owns the whole row: what was there before is cleared first.
        for x in region.x..region.right() {
            buf[(x, row)].reset();
        }
        let pieces = plan(self.inputs, self.external, region.width).pieces;
        if pieces.is_empty() {
            return;
        }
        let mut x = region.x;
        buf[(x, row)]
            .set_symbol(" ")
            .set_style(self.palette.style(Tone::Text));
        x += 1;
        for (index, (_, text, tone)) in pieces.iter().enumerate() {
            if index > 0 {
                let (next, _) = buf.set_stringn(
                    x,
                    row,
                    SEPARATOR,
                    usize::MAX,
                    self.palette.style(Tone::Separator),
                );
                x = next;
            }
            let (next, _) =
                buf.set_stringn(x, row, text.as_str(), usize::MAX, self.palette.style(*tone));
            x = next;
        }
    }
}

impl LiveSection for StatusSection<'_> {
    fn height(&self, _width: u16) -> u16 {
        1
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_into(area, buf);
    }
}
