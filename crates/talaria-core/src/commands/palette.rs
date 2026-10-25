//! Palette rows and the pure keystroke transition. Issue #223.
//!
//! The palette reacts to every draft change from local data at once, then refines the rows
//! with a `complete.slash` reply. It never fetches: the application runs the effects.

use std::collections::BTreeSet;
use std::time::Duration;

use super::catalog::{CatalogEntry, CommandCatalog, CommandSource, SkillOrigin};
use super::local::{LocalCommand, LocalCommandTable};
use super::parse::parse_slash_line;
use super::state::CatalogStatus;

/// A `complete.slash` result.
pub type CompleteResult = hermes_contract::methods::legacy::complete_slash::Result;

/// What a row is. Only [`RowKind::Command`] can be selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A selectable command.
    Command,
    /// A group title.
    Header,
    /// Usage hint while an argument is being typed.
    Hint,
    /// Loading, failure, or "no match".
    Status,
}

/// One line of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteRow {
    /// Row class.
    pub kind: RowKind,
    /// Command name, or the header text.
    pub name: String,
    /// Description. Empty for a header.
    pub description: String,
    /// Provenance. `None` for a header, hint, or status row.
    pub source: Option<CommandSource>,
    /// The bare form was refused on this connection.
    pub refused: bool,
    /// The command takes an argument.
    pub takes_argument: bool,
    /// Group title, when the row is a command.
    pub group: Option<String>,
}

/// Command name, or the argument after a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteStage {
    /// Choosing a command.
    Command,
    /// Choosing or typing an argument. `replace_from_bytes` is a byte offset.
    Argument { replace_from_bytes: Option<usize> },
}

/// The palette the screen draws. Issue #223.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteState {
    /// The palette is on screen.
    pub open: bool,
    /// Command name or argument.
    pub stage: PaletteStage,
    /// Rows in display order, including headers.
    pub rows: Vec<PaletteRow>,
    /// Index of the selected row in `rows`.
    pub selected: usize,
    /// The person moved the selection.
    pub explicit_selection: bool,
    /// Id of the completion request in flight. Zero when none is.
    pub request_id: u64,
    /// Footer text, when it replaces the default hint.
    pub footer: Option<String>,
    /// Draft for which Esc closed the palette. `None` when it was not closed that way.
    pub closed_for: Option<String>,
    /// Selectable rows in browse mode.
    pub total: usize,
    /// The draft the rows were derived from.
    pub draft: String,
    /// Completion ids handed out so far. The next request gets the one after.
    pub issued: u64,
    /// The text each argument row puts after `replace_from`, in row order.
    pub item_texts: Vec<String>,
    /// The completion reply merged into the rows, kept for a catalogue change.
    pub reply: Option<Vec<ReplyItem>>,
    /// Why the last completion failed. It wins over the catalogue warning in the footer.
    pub completion_failure: Option<String>,
}

/// One `complete.slash` item, as the palette keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyItem {
    /// Text that replaces the draft from `replace_from`.
    pub text: String,
    /// Text shown for the item.
    pub display: String,
    /// Description.
    pub meta: String,
    /// `skill` or `command`, when Hermes says.
    pub kind: Option<String>,
}

impl Default for PaletteState {
    fn default() -> Self {
        Self::closed()
    }
}

impl PaletteState {
    /// A closed palette.
    #[must_use]
    pub fn closed() -> Self {
        Self {
            open: false,
            stage: PaletteStage::Command,
            rows: Vec::new(),
            selected: 0,
            explicit_selection: false,
            request_id: 0,
            footer: None,
            closed_for: None,
            total: 0,
            draft: String::new(),
            issued: 0,
            item_texts: Vec::new(),
            reply: None,
            completion_failure: None,
        }
    }
}

/// One thing that happened to the draft or a reply.
#[derive(Clone, Debug, PartialEq)]
pub enum PaletteInput {
    /// The draft or the caret changed. `caret` is a byte offset.
    DraftChanged { text: String, caret: usize },
    /// The catalogue store changed. Rows are derived again.
    CatalogChanged,
    /// A `complete.slash` reply for `request_id`.
    CompletionArrived {
        request_id: u64,
        result: CompleteResult,
    },
    /// That completion failed.
    CompletionFailed { request_id: u64, reason: String },
    /// Select the next row.
    Next,
    /// Select the previous row.
    Prev,
    /// Tab.
    Accept,
    /// Enter.
    Enter,
    /// Esc.
    Close,
}

/// What the application should do after one input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteEffect {
    /// Ask Hermes to complete `text` after `after` (60 ms).
    RequestCompletion {
        request_id: u64,
        text: String,
        after: Duration,
    },
    /// Replace the composer text.
    ReplaceDraft { text: String },
    /// Run this slash line.
    Run { line: String },
    /// Draw again.
    Redraw,
}

/// What the transition may read. It does not fetch.
pub struct PaletteContext<'a> {
    /// The latest catalogue, when one has arrived.
    pub catalog: Option<&'a CommandCatalog>,
    /// Whether that catalogue is loading, ready, or failed.
    pub catalog_status: &'a CatalogStatus,
    /// Local commands.
    pub local: &'a LocalCommandTable,
    /// Bare names refused on this connection.
    pub refused: &'a BTreeSet<String>,
}

/// How long after the last draft change a completion request leaves.
pub const COMPLETION_DELAY: Duration = Duration::from_millis(60);
/// Header shown while no catalogue is held and one is on its way.
pub const LOADING_HEADER: &str = "Loading Hermes commands\u{2026}";
/// Failure reasons in the palette are cut to this many characters.
pub const REASON_CHARS: usize = 60;

const TALARIA_GROUP: &str = "Talaria";
const SKILLS_GROUP: &str = "Skills";
const OTHER_GROUP: &str = "Other commands";
const META_GLYPHS: [&str; 3] = ["\u{26a1} ", "\u{25a3} ", "\u{1f50c} "];

fn cut(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

/// The run of non-whitespace characters touching the caret (a byte offset).
fn caret_word(text: &str, caret: usize) -> &str {
    let mut caret = caret.min(text.len());
    while !text.is_char_boundary(caret) {
        caret -= 1;
    }
    let start = text[..caret]
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map_or(0, |(index, ch)| index + ch.len_utf8());
    let end = text[caret..]
        .find(char::is_whitespace)
        .map_or(text.len(), |offset| caret + offset);
    &text[start..end]
}

/// The palette opens for a one-line draft that is `/` or a slash command with a name, unless
/// the word at the caret starts with `@` (that word belongs to the mention list).
fn should_open(text: &str, caret: usize) -> bool {
    if text.contains('\n') {
        return false;
    }
    let shaped = text == "/" || parse_slash_line(text).is_some_and(|line| !line.name.is_empty());
    shaped && !caret_word(text, caret).starts_with('@')
}

fn stage_of(text: &str) -> PaletteStage {
    let rest = text.strip_prefix('/').unwrap_or(text);
    if rest.contains(char::is_whitespace) {
        PaletteStage::Argument {
            replace_from_bytes: None,
        }
    } else {
        PaletteStage::Command
    }
}

/// The typed name: the draft after `/` up to the first whitespace, as typed.
fn typed_name(text: &str) -> &str {
    let rest = text.strip_prefix('/').unwrap_or(text);
    rest.split(char::is_whitespace).next().unwrap_or_default()
}

fn bare(name: &str) -> &str {
    name.strip_prefix('/').unwrap_or(name)
}

fn local_named<'a>(local: &'a LocalCommandTable, name: &str) -> Option<&'a LocalCommand> {
    let name = bare(name);
    local.iter().find(|command| {
        command.name.eq_ignore_ascii_case(name)
            || command
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(name))
    })
}

fn hidden_when_browsing(entry: &CatalogEntry) -> bool {
    matches!(
        entry.source,
        CommandSource::Skill {
            origin: SkillOrigin::Bundled
        }
    ) && entry.usage == 0
}

fn group_of(entry: &CatalogEntry) -> &str {
    match &entry.source {
        CommandSource::Hermes { category } if !category.is_empty() => category,
        CommandSource::Hermes { .. } => OTHER_GROUP,
        CommandSource::Talaria => TALARIA_GROUP,
        CommandSource::UserCommand => "User commands",
        CommandSource::Plugin => "Plugin commands",
        CommandSource::Skill { .. } => SKILLS_GROUP,
    }
}

fn header(name: &str) -> PaletteRow {
    PaletteRow {
        kind: RowKind::Header,
        name: name.to_owned(),
        description: String::new(),
        source: None,
        refused: false,
        takes_argument: false,
        group: None,
    }
}

fn plain(kind: RowKind, text: String) -> PaletteRow {
    PaletteRow {
        kind,
        name: text,
        description: String::new(),
        source: None,
        refused: false,
        takes_argument: false,
        group: None,
    }
}

fn local_row(command: &LocalCommand) -> PaletteRow {
    PaletteRow {
        kind: RowKind::Command,
        name: format!("/{}", command.name),
        description: command.summary.to_owned(),
        source: Some(CommandSource::Talaria),
        refused: false,
        takes_argument: command.takes_argument,
        group: Some(TALARIA_GROUP.to_owned()),
    }
}

fn entry_row(entry: &CatalogEntry, ctx: &PaletteContext<'_>) -> PaletteRow {
    PaletteRow {
        kind: RowKind::Command,
        name: entry.name.clone(),
        description: entry.description.clone(),
        source: Some(entry.source.clone()),
        refused: ctx.refused.contains(&bare(&entry.name).to_lowercase()),
        takes_argument: entry.takes_argument(),
        group: Some(group_of(entry).to_owned()),
    }
}

/// Selectable rows the bare `/` list shows.
fn browse_total(ctx: &PaletteContext<'_>) -> usize {
    let listed = ctx.catalog.map_or(0, |catalog| {
        catalog
            .entries()
            .iter()
            .filter(|entry| local_named(ctx.local, &entry.name).is_none())
            .filter(|entry| !hidden_when_browsing(entry))
            .count()
    });
    ctx.local.iter().count() + listed
}

/// The bare `/` list: the Talaria group, each category in Hermes's order, then skills.
fn browse_rows(ctx: &PaletteContext<'_>) -> Vec<PaletteRow> {
    let mut rows = vec![header(TALARIA_GROUP)];
    rows.extend(ctx.local.iter().map(local_row));
    let Some(catalog) = ctx.catalog else {
        match ctx.catalog_status {
            CatalogStatus::Failed(reason) => rows.push(plain(
                RowKind::Status,
                format!("Hermes commands unavailable: {}", cut(reason, REASON_CHARS)),
            )),
            _ => rows.push(header(LOADING_HEADER)),
        }
        return rows;
    };
    let mut group: Option<&str> = None;
    for entry in catalog.entries() {
        if local_named(ctx.local, &entry.name).is_some() || hidden_when_browsing(entry) {
            continue;
        }
        let name = group_of(entry);
        if group != Some(name) {
            rows.push(header(name));
            group = Some(name);
        }
        rows.push(entry_row(entry, ctx));
    }
    if let Some(error) = catalog.plugin_discovery_error() {
        rows.push(plain(
            RowKind::Status,
            format!("Plugin commands unknown: {error}"),
        ));
    }
    rows
}

/// 0 for the same name, 1 for a prefix, 2 for a substring; `None` when it does not match.
fn tier(name: &str, query: &str) -> Option<u8> {
    let name = bare(name).to_lowercase();
    if name == query {
        Some(0)
    } else if name.starts_with(query) {
        Some(1)
    } else if name.contains(query) {
        Some(2)
    } else {
        None
    }
}

fn local_tier(command: &LocalCommand, query: &str) -> Option<u8> {
    std::iter::once(command.name)
        .chain(command.aliases.iter().copied())
        .filter_map(|name| tier(name, query))
        .min()
}

fn local_matches(query: &str, ctx: &PaletteContext<'_>) -> Vec<(u8, PaletteRow)> {
    ctx.local
        .iter()
        .filter_map(|command| local_tier(command, query).map(|tier| (tier, local_row(command))))
        .collect()
}

fn no_match_row(typed: &str) -> PaletteRow {
    plain(
        RowKind::Status,
        format!("No matching commands. Enter sends /{typed} to Hermes as typed."),
    )
}

/// The local filter for `/query`: exact names first, then prefixes, then substrings;
/// local commands before catalogue ones within a tier, then by name.
fn search_rows(typed: &str, ctx: &PaletteContext<'_>) -> Vec<PaletteRow> {
    let query = typed.to_lowercase();
    let mut ranked: Vec<(u8, bool, PaletteRow)> = local_matches(&query, ctx)
        .into_iter()
        .map(|(tier, row)| (tier, false, row))
        .collect();
    if let Some(catalog) = ctx.catalog {
        let alias = catalog
            .is_alias(&query)
            .then(|| catalog.canonical(&query))
            .flatten();
        for entry in catalog.entries() {
            if local_named(ctx.local, &entry.name).is_some() {
                continue;
            }
            let found = if alias == Some(entry.name.as_str()) {
                Some(0)
            } else {
                tier(&entry.name, &query)
            };
            if let Some(found) = found {
                ranked.push((found, true, entry_row(entry, ctx)));
            }
        }
    }
    ranked.sort_by(|left, right| {
        (left.0, left.1)
            .cmp(&(right.0, right.1))
            .then_with(|| left.2.name.cmp(&right.2.name))
    });
    let mut rows: Vec<PaletteRow> = ranked.into_iter().map(|(_, _, row)| row).collect();
    if rows.is_empty() {
        rows.push(no_match_row(typed));
    }
    rows
}

fn strip_glyph(meta: &str) -> &str {
    META_GLYPHS
        .iter()
        .find_map(|glyph| meta.strip_prefix(glyph))
        .unwrap_or(meta)
}

/// `/name` for an item's text, with or without its slash.
fn item_name(item: &ReplyItem) -> String {
    format!("/{}", bare(item.text.trim()))
}

/// Local matches first, then the reply's rows in its order, with an exact name first.
fn merged_rows(typed: &str, items: &[ReplyItem], ctx: &PaletteContext<'_>) -> Vec<PaletteRow> {
    let query = typed.to_lowercase();
    let mut local: Vec<(u8, PaletteRow)> = local_matches(&query, ctx);
    local.sort_by_key(|(tier, _)| *tier);
    let mut rows: Vec<PaletteRow> = local.into_iter().map(|(_, row)| row).collect();
    let canonical = ctx.catalog.and_then(|catalog| catalog.canonical(&query));
    let mut reply: Vec<PaletteRow> = Vec::new();
    for item in items {
        let name = item_name(item);
        if local_named(ctx.local, &name).is_some()
            || rows.iter().chain(&reply).any(|row| row.name == name)
        {
            continue;
        }
        let entry = ctx.catalog.and_then(|catalog| catalog.lookup(&name));
        let row = match entry {
            Some(entry) => entry_row(entry, ctx),
            None if item.kind.as_deref() == Some("skill") => PaletteRow {
                kind: RowKind::Command,
                name,
                description: strip_glyph(&item.meta).to_owned(),
                source: Some(CommandSource::Skill {
                    origin: SkillOrigin::Other("bundle".to_owned()),
                }),
                refused: false,
                takes_argument: true,
                group: Some(SKILLS_GROUP.to_owned()),
            },
            None => continue,
        };
        reply.push(row);
    }
    if let Some(exact) = reply.iter().position(|row| {
        bare(&row.name).eq_ignore_ascii_case(&query) || canonical == Some(row.name.as_str())
    }) {
        let row = reply.remove(exact);
        let local_exact = rows.first().is_some_and(|row| {
            local_named(ctx.local, &row.name)
                .is_some_and(|command| local_tier(command, &query) == Some(0))
        });
        let at = usize::from(local_exact);
        rows.insert(at.min(rows.len()), row);
    }
    rows.extend(reply);
    if rows.is_empty() {
        rows.push(no_match_row(typed));
    }
    rows
}

/// The usage hint for `/name` in the argument stage, when there is one.
fn hint_row(name: &str, ctx: &PaletteContext<'_>) -> Option<PaletteRow> {
    let text = match local_named(ctx.local, name) {
        Some(command) if !command.args_hint.is_empty() => {
            format!("usage: /{} {}", command.name, command.args_hint)
        }
        Some(_) => return None,
        None => format!("usage: {}", ctx.catalog?.usage_hint(name)?),
    };
    Some(plain(RowKind::Hint, text))
}

fn item_row(item: &ReplyItem) -> PaletteRow {
    let name = if item.display.is_empty() {
        item.text.clone()
    } else {
        item.display.clone()
    };
    PaletteRow {
        kind: RowKind::Command,
        name,
        description: strip_glyph(&item.meta).to_owned(),
        source: None,
        refused: false,
        takes_argument: false,
        group: None,
    }
}

/// Derive the rows, the counts and the footer from the draft, the stage and the reply.
fn rebuild(state: &mut PaletteState, ctx: &PaletteContext<'_>) {
    state.item_texts.clear();
    match state.stage {
        PaletteStage::Command => {
            let typed = typed_name(&state.draft).to_owned();
            state.rows = if typed.is_empty() {
                browse_rows(ctx)
            } else {
                match &state.reply {
                    Some(items) => merged_rows(&typed, items, ctx),
                    None => search_rows(&typed, ctx),
                }
            };
            state.total = browse_total(ctx);
        }
        PaletteStage::Argument { replace_from_bytes } => {
            let name = typed_name(&state.draft).to_lowercase();
            state.rows = hint_row(&name, ctx).into_iter().collect();
            if let (Some(items), Some(_)) = (&state.reply, replace_from_bytes) {
                for item in items {
                    state.rows.push(item_row(item));
                    state.item_texts.push(item.text.clone());
                }
            }
            state.total = state.item_texts.len();
        }
    }
    state.footer = state.completion_failure.clone().or_else(|| {
        ctx.catalog
            .and_then(CommandCatalog::warning)
            .map(|warning| format!("Hermes: {warning}"))
    });
}

fn selected_name(state: &PaletteState) -> Option<String> {
    state
        .rows
        .get(state.selected)
        .filter(|row| row.kind == RowKind::Command)
        .map(|row| row.name.clone())
}

fn first_selectable(rows: &[PaletteRow]) -> usize {
    rows.iter()
        .position(|row| row.kind == RowKind::Command)
        .unwrap_or(0)
}

/// Keep the selection on `keep` when it is still listed; else select the first row.
fn reselect(state: &mut PaletteState, keep: Option<&str>) {
    let kept = keep.and_then(|name| {
        state
            .rows
            .iter()
            .position(|row| row.kind == RowKind::Command && row.name == name)
    });
    match kept {
        Some(index) => state.selected = index,
        None => {
            state.selected = first_selectable(&state.rows);
            state.explicit_selection = false;
        }
    }
}

fn close_quietly(state: &mut PaletteState) {
    state.open = false;
    state.stage = PaletteStage::Command;
    state.rows.clear();
    state.selected = 0;
    state.explicit_selection = false;
    state.request_id = 0;
    state.item_texts.clear();
    state.reply = None;
    state.completion_failure = None;
    state.footer = None;
    state.total = 0;
}

/// True when the draft should be completed by Hermes: a Hermes command name or argument.
/// A local command is never sent, so it is not completed either.
fn wants_completion(state: &PaletteState, ctx: &PaletteContext<'_>) -> bool {
    let name = typed_name(&state.draft);
    !name.is_empty() && local_named(ctx.local, name).is_none()
}

fn request(state: &mut PaletteState) -> PaletteEffect {
    state.issued += 1;
    state.request_id = state.issued;
    PaletteEffect::RequestCompletion {
        request_id: state.request_id,
        text: state.draft.clone(),
        after: COMPLETION_DELAY,
    }
}

fn draft_changed(
    state: &mut PaletteState,
    text: String,
    caret: usize,
    ctx: &PaletteContext<'_>,
) -> Vec<PaletteEffect> {
    if let Some(closed) = &state.closed_for {
        if *closed == text {
            state.draft = text;
            return Vec::new();
        }
        state.closed_for = None;
    }
    let changed = text != state.draft;
    let was_open = state.open;
    state.draft = text;
    if !should_open(&state.draft, caret) {
        if was_open {
            close_quietly(state);
            return vec![PaletteEffect::Redraw];
        }
        return Vec::new();
    }
    let keep = if changed || !was_open {
        None
    } else {
        selected_name(state)
    };
    if changed || !was_open {
        state.explicit_selection = false;
        state.reply = None;
        state.request_id = 0;
        state.stage = stage_of(&state.draft);
    }
    state.open = true;
    rebuild(state, ctx);
    reselect(state, keep.as_deref());
    let mut effects = Vec::new();
    if (changed || !was_open) && wants_completion(state, ctx) {
        effects.push(request(state));
    }
    effects.push(PaletteEffect::Redraw);
    effects
}

/// Code points to a byte offset of `text`. `None` past the end.
fn byte_offset(text: &str, code_points: i64) -> Option<usize> {
    let wanted = usize::try_from(code_points).ok()?;
    if wanted == text.chars().count() {
        return Some(text.len());
    }
    text.char_indices().nth(wanted).map(|(index, _)| index)
}

fn arrived(
    state: &mut PaletteState,
    request_id: u64,
    result: &CompleteResult,
    ctx: &PaletteContext<'_>,
) -> Vec<PaletteEffect> {
    if !state.open || request_id == 0 || request_id != state.request_id {
        return Vec::new();
    }
    state.request_id = 0;
    state.completion_failure = None;
    let items: Vec<ReplyItem> = result
        .items
        .iter()
        .map(|item| ReplyItem {
            text: item.text.clone(),
            display: item.display.clone(),
            meta: item.meta.clone(),
            kind: item.kind.clone(),
        })
        .collect();
    let keep = selected_name(state);
    match state.stage {
        PaletteStage::Command => {
            state.reply = Some(items);
            rebuild(state, ctx);
            reselect(state, keep.as_deref());
        }
        PaletteStage::Argument { .. } => {
            let offset = result
                .replace_from
                .and_then(|from| byte_offset(&state.draft, from));
            match offset {
                Some(offset) if !items.is_empty() => {
                    state.stage = PaletteStage::Argument {
                        replace_from_bytes: Some(offset),
                    };
                    state.reply = Some(items);
                }
                // An empty reply leaves the hint alone; a reply that points past the draft
                // is dropped whole.
                _ => {
                    state.stage = PaletteStage::Argument {
                        replace_from_bytes: None,
                    };
                    state.reply = None;
                }
            }
            rebuild(state, ctx);
            reselect(state, None);
        }
    }
    vec![PaletteEffect::Redraw]
}

fn failed(
    state: &mut PaletteState,
    request_id: u64,
    reason: &str,
    ctx: &PaletteContext<'_>,
) -> Vec<PaletteEffect> {
    if !state.open || request_id == 0 || request_id != state.request_id {
        return Vec::new();
    }
    state.request_id = 0;
    state.completion_failure = Some(format!(
        "completion unavailable: {}",
        cut(reason, REASON_CHARS)
    ));
    let keep = selected_name(state);
    rebuild(state, ctx);
    reselect(state, keep.as_deref());
    vec![PaletteEffect::Redraw]
}

fn step(state: &mut PaletteState, forward: bool) -> Vec<PaletteEffect> {
    if !state.open {
        return Vec::new();
    }
    let selectable: Vec<usize> = state
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.kind == RowKind::Command)
        .map(|(index, _)| index)
        .collect();
    if selectable.is_empty() {
        return Vec::new();
    }
    let count = selectable.len();
    let next = match selectable.iter().position(|index| *index == state.selected) {
        Some(at) if forward => (at + 1) % count,
        Some(at) => (at + count - 1) % count,
        None if forward => 0,
        None => count - 1,
    };
    state.selected = selectable[next];
    state.explicit_selection = true;
    vec![PaletteEffect::Redraw]
}

fn selected_row(state: &PaletteState) -> Option<&PaletteRow> {
    state
        .rows
        .get(state.selected)
        .filter(|row| row.kind == RowKind::Command)
}

/// The draft with the selected argument row put in from `replace_from`.
fn accepted_item(state: &PaletteState) -> Option<String> {
    let PaletteStage::Argument {
        replace_from_bytes: Some(offset),
    } = state.stage
    else {
        return None;
    };
    selected_row(state)?;
    let position = state.rows[..state.selected]
        .iter()
        .filter(|row| row.kind == RowKind::Command)
        .count();
    let text = state.item_texts.get(position)?;
    let head = state.draft.get(..offset)?;
    Some(format!("{head}{text}"))
}

fn completed_name(row: &PaletteRow) -> String {
    if row.takes_argument {
        format!("{} ", row.name)
    } else {
        row.name.clone()
    }
}

fn accept(state: &mut PaletteState) -> Vec<PaletteEffect> {
    if !state.open {
        return Vec::new();
    }
    let text = match state.stage {
        PaletteStage::Command => selected_row(state).map(completed_name),
        PaletteStage::Argument { .. } => accepted_item(state),
    };
    let Some(text) = text else {
        return Vec::new();
    };
    state.request_id = 0;
    state.explicit_selection = false;
    vec![PaletteEffect::ReplaceDraft { text }]
}

fn run(line: String) -> Vec<PaletteEffect> {
    vec![
        PaletteEffect::ReplaceDraft { text: line.clone() },
        PaletteEffect::Run { line },
    ]
}

/// Choose a row by Enter: a command that takes no argument is completed and run; one that
/// takes an argument is completed to `/name ` and waits for it.
fn choose(row: &PaletteRow) -> Vec<PaletteEffect> {
    if row.takes_argument {
        vec![PaletteEffect::ReplaceDraft {
            text: completed_name(row),
        }]
    } else {
        run(row.name.clone())
    }
}

/// True when `typed` (lower-case, no slash) is this row's name, a local alias of it, or a
/// catalogue alias of it.
fn names_row(row: &PaletteRow, typed: &str, ctx: &PaletteContext<'_>) -> bool {
    if row.kind != RowKind::Command {
        return false;
    }
    if bare(&row.name).eq_ignore_ascii_case(typed) {
        return true;
    }
    if row.source == Some(CommandSource::Talaria) {
        return local_named(ctx.local, typed)
            .is_some_and(|command| command.name.eq_ignore_ascii_case(bare(&row.name)));
    }
    ctx.catalog
        .and_then(|catalog| catalog.canonical(typed))
        .is_some_and(|canonical| canonical == row.name)
}

fn enter_command(state: &PaletteState, ctx: &PaletteContext<'_>) -> Vec<PaletteEffect> {
    let typed = typed_name(&state.draft).to_lowercase();
    if typed.is_empty() {
        return match selected_row(state) {
            Some(row) if state.explicit_selection => choose(row),
            _ => Vec::new(),
        };
    }
    if !state.rows.iter().any(|row| row.kind == RowKind::Command) {
        return run(state.draft.clone());
    }
    if let Some(equal) = state
        .rows
        .iter()
        .position(|row| names_row(row, &typed, ctx))
        && (!state.explicit_selection || state.selected == equal)
    {
        return run(state.draft.clone());
    }
    match selected_row(state) {
        Some(row)
            if state.explicit_selection || bare(&row.name).to_lowercase().starts_with(&typed) =>
        {
            choose(row)
        }
        _ => run(state.draft.clone()),
    }
}

fn enter(state: &mut PaletteState, ctx: &PaletteContext<'_>) -> Vec<PaletteEffect> {
    if !state.open {
        return Vec::new();
    }
    state.request_id = 0;
    match state.stage {
        PaletteStage::Command => enter_command(state, ctx),
        PaletteStage::Argument { .. } => {
            let line = state
                .explicit_selection
                .then(|| accepted_item(state))
                .flatten()
                .unwrap_or_else(|| state.draft.clone());
            run(line)
        }
    }
}

fn close(state: &mut PaletteState) -> Vec<PaletteEffect> {
    let draft = state.draft.clone();
    close_quietly(state);
    state.closed_for = Some(draft);
    vec![PaletteEffect::Redraw]
}

/// Apply one input. Issue #223.
///
/// The transition is pure: it reads the catalogue, the local table and the refused names
/// from `ctx` and says what the application should do next.
#[must_use]
pub fn update(
    state: &mut PaletteState,
    input: PaletteInput,
    ctx: &PaletteContext<'_>,
) -> Vec<PaletteEffect> {
    match input {
        PaletteInput::DraftChanged { text, caret } => draft_changed(state, text, caret, ctx),
        PaletteInput::CatalogChanged => {
            if !state.open {
                return Vec::new();
            }
            let keep = selected_name(state);
            let explicit = state.explicit_selection;
            rebuild(state, ctx);
            reselect(state, keep.as_deref());
            if selected_name(state) == keep {
                state.explicit_selection = explicit;
            }
            vec![PaletteEffect::Redraw]
        }
        PaletteInput::CompletionArrived { request_id, result } => {
            arrived(state, request_id, &result, ctx)
        }
        PaletteInput::CompletionFailed { request_id, reason } => {
            failed(state, request_id, &reason, ctx)
        }
        PaletteInput::Next => step(state, true),
        PaletteInput::Prev => step(state, false),
        PaletteInput::Accept => accept(state),
        PaletteInput::Enter => enter(state, ctx),
        PaletteInput::Close => close(state),
    }
}
