use super::*;
use super::{
    diff::format_value,
    secrets::has_env_reference,
    text,
    values::{lookup, option_text, put},
};
use crate::commands::local::LocalAction;
use serde_json::Value;
use std::{collections::BTreeMap, fmt, time::Duration};

#[cfg(test)]
mod privacy_tests {
    use super::*;

    #[test]
    fn refusal_does_not_echo_a_submitted_secret() {
        let mut state = SettingsState::new(&[]);
        state.screen = Screen::Applying;
        state.sent.insert(
            "voice.api_key".into(),
            Value::String("example-sensitive-value".into()),
        );
        state.update(SettingsInput::Written(WriteResult::Refused {
            status: 400,
            detail: Some("invalid credential example-sensitive-value".into()),
        }));
        let banner = state.view().banner.join(" ");
        assert!(banner.contains("(secret)"));
        assert!(!banner.contains("example-sensitive-value"));
    }

    #[test]
    fn an_unbound_physical_key_hides_a_reveal() {
        let mut state = SettingsState::new(&[]);
        state.reveal = Some((7, SecretField::new("example-sensitive-value".into())));
        assert!(state.update(SettingsInput::HideReveal).is_empty());
        assert!(state.reveal.is_none());
    }
}

#[derive(Clone)]
pub struct SettingsSnapshot {
    pub profile: String,
    pub schema: ConfigSchemaView,
    pub effective: Value,
    pub raw: Value,
    pub defaults: Value,
}
impl fmt::Debug for SettingsSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsSnapshot")
            .field("fields", &self.schema.fields.len())
            .finish_non_exhaustive()
    }
}
pub enum SettingsInput {
    Open {
        profile: String,
        filter: Option<String>,
    },
    Command {
        profile: String,
        args: String,
    },
    Loaded(SettingsSnapshot),
    LoadFailed(FailReason),
    Key(SettingsKey),
    Fresh(Value),
    FreshFailed(FailReason),
    Written(WriteResult),
    ReadBack {
        effective: Value,
        raw: Value,
    },
    ReadBackFailed(FailReason),
    RevealTimeout(u64),
    /// A physical key also hides a reveal when it has no settings binding.
    HideReveal,
}
#[derive(PartialEq)]
pub enum SettingsEffect {
    Load { profile: String },
    FetchFresh,
    Write { patch: Value },
    ReadBack,
    Notice(String),
    ScheduleReveal { id: u64, after: Duration },
    ForgetHistoryLine,
    OpenLocal(LocalAction),
    CloseOverlay,
}
#[derive(Clone, PartialEq)]
pub enum WriteResult {
    Ok,
    Refused { status: u16, detail: Option<String> },
    Unauthorized,
    Forbidden,
    NotFound { route: String },
    Server { status: u16 },
    Unknown,
    NotSent(FailReason),
}
#[derive(Clone, PartialEq)]
pub enum FailReason {
    NotConnected,
    ProfileNotServed,
    Unauthorized,
    Timeout { secs: u64 },
    Dropped,
    Http { status: u16 },
    NotFound { route: String },
    BadData,
    Ticket(String),
    NotLoopback,
}
#[derive(Clone, PartialEq)]
pub enum SettingsKey {
    Action(SettingsAction),
    Char(char),
    Edit(EditKey),
    Paste(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsAction {
    Up,
    Down,
    PageUp,
    PageDown,
    PrevCategory,
    NextCategory,
    Edit,
    Toggle,
    Filter,
    Default,
    UndoField,
    Reveal,
    Review,
    Reload,
    Help,
    Close,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKey {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Backspace,
    Delete,
    ClearLine,
    Enter,
    Esc,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Closed,
    Loading,
    LoadError,
    Browsing,
    Editing,
    Choosing,
    Help,
    Reviewing,
    Checking,
    Conflict,
    Applying,
    Verifying,
    ConfirmDiscard,
}
#[derive(Clone, Default)]
pub struct SettingsView {
    pub screen: Screen,
    pub profile: String,
    pub text_focus: bool,
    pub categories: Vec<String>,
    pub category: String,
    pub rows: Vec<SettingsRow>,
    pub cursor: usize,
    pub banner: Vec<String>,
    pub detail: String,
    pub footer: String,
    pub review_rows: Vec<ReviewRow>,
    pub conflict_rows: Vec<Conflict>,
    pub filter: String,
    pub editor: Option<SettingsEditorView>,
    pub choices: Vec<String>,
    pub choice_cursor: usize,
    pub links: Vec<String>,
    pub help: Vec<(String, String)>,
}
#[derive(Clone, Default)]
pub struct SettingsRow {
    pub key: String,
    pub value: String,
    pub origin: String,
    pub marker: String,
    pub effect_label: String,
}
#[derive(Clone)]
pub struct SettingsEditorView {
    pub text: String,
    pub cursor: usize,
    pub secret: bool,
}
struct Editor {
    key: String,
    kind: ValueKind,
    text: SecretField,
    caret: usize,
    secret: bool,
}
pub struct SettingsState {
    links: &'static [SettingsLink],
    screen: Screen,
    profile: String,
    snapshot: Option<SettingsSnapshot>,
    pending: BTreeMap<String, Value>,
    markers: BTreeMap<String, String>,
    saved: BTreeMap<String, Value>,
    sent: BTreeMap<String, Value>,
    before: Value,
    write_ok: bool,
    filter: String,
    filter_focus: bool,
    filter_caret: usize,
    cursor: usize,
    category: usize,
    banner: Vec<String>,
    editor: Option<Editor>,
    choice_filter: String,
    choice_cursor: usize,
    conflicts: Vec<Conflict>,
    fresh: Option<Value>,
    command: Option<Vec<(String, String)>>,
    command_close: bool,
    reloading: bool,
    unverified: bool,
    reveal: Option<(u64, SecretField)>,
    next_reveal: u64,
    close_action: Option<LocalAction>,
}
impl fmt::Debug for SettingsState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsState")
            .field("screen", &self.screen)
            .field("pending", &self.pending.len())
            .field("saved", &self.saved.len())
            .finish_non_exhaustive()
    }
}
macro_rules! private_debug {($name:ty,$self:ident,$body:expr)=>{impl fmt::Debug for $name{fn fmt(&$self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str($body)}}};}
impl fmt::Debug for SettingsInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loaded(snapshot) => f.debug_tuple("Loaded").field(snapshot).finish(),
            Self::Fresh(value) => f
                .debug_struct("Fresh")
                .field("keys", &key_count(value))
                .finish(),
            Self::ReadBack { effective, raw } => f
                .debug_struct("ReadBack")
                .field("effective_keys", &key_count(effective))
                .field("raw_keys", &key_count(raw))
                .finish(),
            other => f.write_str(match other {
                Self::Open { .. } => "Open",
                Self::Command { .. } => "Command",
                Self::LoadFailed(_) => "LoadFailed",
                Self::Key(_) => "Key",
                Self::FreshFailed(_) => "FreshFailed",
                Self::Written(_) => "Written",
                Self::ReadBackFailed(_) => "ReadBackFailed",
                Self::RevealTimeout(_) => "RevealTimeout",
                _ => "SettingsInput",
            }),
        }
    }
}
impl fmt::Debug for SettingsEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Write { patch } => f
                .debug_struct("Write")
                .field("keys", &key_count(patch))
                .finish(),
            other => f.write_str(match other {
                Self::Load { .. } => "Load",
                Self::FetchFresh => "FetchFresh",
                Self::ReadBack => "ReadBack",
                Self::Notice(_) => "Notice",
                Self::ScheduleReveal { .. } => "ScheduleReveal",
                Self::ForgetHistoryLine => "ForgetHistoryLine",
                Self::OpenLocal(_) => "OpenLocal",
                Self::CloseOverlay => "CloseOverlay",
                _ => "SettingsEffect",
            }),
        }
    }
}
fn key_count(value: &Value) -> usize {
    match value {
        Value::Object(map) => map.values().map(key_count).sum(),
        _ => 1,
    }
}
private_debug!(
    SettingsKey,
    self,
    match self {
        Self::Action(_) => "Action",
        Self::Char(_) => "Char",
        Self::Edit(_) => "Edit",
        Self::Paste(_) => "Paste",
    }
);
private_debug!(
    WriteResult,
    self,
    match self {
        Self::Ok => "Ok",
        Self::Refused { .. } => "Refused",
        Self::Unauthorized => "Unauthorized",
        Self::Forbidden => "Forbidden",
        Self::NotFound { .. } => "NotFound",
        Self::Server { .. } => "Server",
        Self::Unknown => "Unknown",
        Self::NotSent(_) => "NotSent",
    }
);
private_debug!(
    FailReason,
    self,
    match self {
        Self::NotConnected => "NotConnected",
        Self::ProfileNotServed => "ProfileNotServed",
        Self::Unauthorized => "Unauthorized",
        Self::Timeout { .. } => "Timeout",
        Self::Dropped => "Dropped",
        Self::Http { .. } => "Http",
        Self::NotFound { .. } => "NotFound",
        Self::BadData => "BadData",
        Self::Ticket(_) => "Ticket",
        Self::NotLoopback => "NotLoopback",
    }
);
private_debug!(SettingsView, self, "SettingsView");
private_debug!(SettingsRow, self, "SettingsRow");
private_debug!(SettingsEditorView, self, "SettingsEditorView");

impl SettingsState {
    pub fn is_open(&self) -> bool {
        self.screen != Screen::Closed
    }

    pub fn new(links: &'static [SettingsLink]) -> Self {
        Self {
            links,
            screen: Screen::Closed,
            profile: String::new(),
            snapshot: None,
            pending: BTreeMap::new(),
            markers: BTreeMap::new(),
            saved: BTreeMap::new(),
            sent: BTreeMap::new(),
            before: Value::Null,
            write_ok: false,
            filter: String::new(),
            filter_focus: false,
            filter_caret: 0,
            cursor: 0,
            category: 0,
            banner: Vec::new(),
            editor: None,
            choice_filter: String::new(),
            choice_cursor: 0,
            conflicts: Vec::new(),
            fresh: None,
            command: None,
            command_close: false,
            reloading: false,
            unverified: false,
            reveal: None,
            next_reveal: 0,
            close_action: None,
        }
    }
    pub fn update(&mut self, input: SettingsInput) -> Vec<SettingsEffect> {
        match input {
            SettingsInput::HideReveal => {
                self.reveal = None;
                Vec::new()
            }
            SettingsInput::Open { profile, filter } => self.open(profile, filter),
            SettingsInput::Command { profile, args } => match parse_config_command(&args) {
                Err(CommandError::Secret { key }) => vec![
                    SettingsEffect::Notice(format!(
                        "{key} is a secret; set it in /config, not on the command line. Nothing was sent."
                    )),
                    SettingsEffect::ForgetHistoryLine,
                ],
                Err(CommandError::Syntax { detail }) => vec![SettingsEffect::Notice(format!(
                    "Use /config key=value [key=value …]: {detail}."
                ))],
                Ok(ConfigCommand::Open { filter }) => self.open(profile, filter),
                Ok(ConfigCommand::Set(pairs)) => {
                    if self.busy() {
                        return Vec::new();
                    }
                    if self.screen != Screen::Closed
                        && self.snapshot.is_some()
                        && profile == self.profile
                    {
                        self.command_close = false;
                        self.apply_command(pairs);
                        Vec::new()
                    } else {
                        let effects = self.open(profile, None);
                        self.command = Some(pairs);
                        self.command_close = true;
                        effects
                    }
                }
            },
            SettingsInput::Loaded(mut snapshot) => {
                if snapshot.profile != self.profile
                    || (self.screen != Screen::Loading && !self.reloading)
                {
                    return Vec::new();
                }
                if self.reloading {
                    if let Some(old) = &self.snapshot {
                        let (_, outside) =
                            find_conflicts(&BTreeMap::new(), &old.effective, &snapshot.effective);
                        if !outside.is_empty() {
                            self.banner = vec![text::outside(outside.len())]
                        }
                        for key in self.pending.keys() {
                            put(
                                &mut snapshot.effective,
                                key,
                                lookup(&old.effective, key).cloned().unwrap_or(Value::Null),
                            );
                        }
                    }
                    self.reloading = false;
                } else {
                    self.screen = Screen::Browsing;
                    self.banner.clear();
                }
                self.snapshot = Some(snapshot);
                self.unverified = false;
                let categories = self
                    .snapshot
                    .as_ref()
                    .map_or(0, |snapshot| snapshot.schema.categories().len())
                    + usize::from(!self.links.is_empty());
                self.category = self.category.min(categories.saturating_sub(1));
                self.clamp_cursor();
                if let Some(pairs) = self.command.take() {
                    self.apply_command(pairs)
                }
                Vec::new()
            }
            SettingsInput::LoadFailed(why) => {
                if self.screen != Screen::Loading && !self.reloading {
                    return Vec::new();
                }
                let message=match why{FailReason::NotConnected=>"Settings need the Hermes gateway, and Talaria is not connected. r retry · Esc close".into(),FailReason::ProfileNotServed=>format!("Profile {} is not served by this gateway, so its settings cannot be opened here.",self.profile),_=>format!("Could not read settings from Hermes: {}. r retry · Esc close",text::reason(&why))};
                self.banner = vec![message];
                if !self.reloading {
                    self.screen = Screen::LoadError
                }
                self.reloading = false;
                Vec::new()
            }
            SettingsInput::Key(key) => {
                self.reveal = None;
                self.key(key)
            }
            SettingsInput::Fresh(value) if self.screen == Screen::Checking => {
                let Some(snapshot) = self.snapshot.as_mut() else {
                    return Vec::new();
                };
                let (conflicts, outside) =
                    find_conflicts(&self.pending, &snapshot.effective, &value);
                let mut baseline = value.clone();
                for key in self.pending.keys() {
                    put(
                        &mut baseline,
                        key,
                        lookup(&snapshot.effective, key)
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                }
                snapshot.effective = baseline;
                if !outside.is_empty() {
                    self.banner = vec![text::outside(outside.len())]
                }
                self.fresh = Some(value);
                self.conflicts = conflicts;
                if self.conflicts.is_empty() {
                    self.write()
                } else {
                    self.screen = Screen::Conflict;
                    Vec::new()
                }
            }
            SettingsInput::FreshFailed(why) if self.screen == Screen::Checking => {
                self.screen = Screen::Reviewing;
                self.banner = vec![text::save_failed(&why)];
                Vec::new()
            }
            SettingsInput::Written(result) if self.screen == Screen::Applying => {
                self.written(result)
            }
            SettingsInput::ReadBack { effective, raw } if self.screen == Screen::Verifying => {
                self.readback(effective, raw)
            }
            SettingsInput::ReadBackFailed(why) if self.screen == Screen::Verifying => {
                self.screen = Screen::Browsing;
                self.unverified = true;
                for key in self.sent.keys() {
                    self.markers.insert(key.clone(), "?".into());
                }
                self.banner = vec![format!(
                    "Could not check what was saved: {}. Press Ctrl+R before changing anything else.",
                    text::reason(&why)
                )];
                Vec::new()
            }
            SettingsInput::RevealTimeout(id) => {
                if self
                    .reveal
                    .as_ref()
                    .is_some_and(|(current, _)| *current == id)
                {
                    self.reveal = None;
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
    fn busy(&self) -> bool {
        matches!(
            self.screen,
            Screen::Checking | Screen::Applying | Screen::Verifying
        )
    }
    fn open(&mut self, profile: String, filter: Option<String>) -> Vec<SettingsEffect> {
        if self.busy() {
            return Vec::new();
        }
        if self.screen != Screen::Closed && profile == self.profile {
            self.command_close = false;
            self.filter = filter.unwrap_or_default();
            self.filter_focus = false;
            self.cursor = 0;
            self.editor = None;
            self.reveal = None;
            if self.snapshot.is_some() {
                self.screen = Screen::Browsing;
            }
            return Vec::new();
        }
        let links = self.links;
        *self = Self::new(links);
        self.profile = profile.clone();
        self.filter = filter.unwrap_or_default();
        self.screen = Screen::Loading;
        vec![SettingsEffect::Load { profile }]
    }
    fn fields(&self) -> Vec<&FieldView> {
        let Some(snapshot) = &self.snapshot else {
            return Vec::new();
        };
        let categories = snapshot.schema.categories();
        let selected = categories.get(self.category);
        let terms: Vec<_> = self
            .filter
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        snapshot
            .schema
            .ordered_fields()
            .into_iter()
            .filter(|field| {
                if terms.is_empty() {
                    selected == Some(&field.category)
                } else {
                    let haystack =
                        format!("{} {} {}", field.key, field.description, field.category)
                            .to_lowercase();
                    terms.iter().all(|term| haystack.contains(term))
                }
            })
            .collect()
    }
    fn selected(&self) -> Option<FieldView> {
        self.fields().get(self.cursor).map(|f| (*f).clone())
    }
    fn more(&self) -> bool {
        self.filter.is_empty()
            && self
                .snapshot
                .as_ref()
                .is_some_and(|s| self.category == s.schema.categories().len())
            && !self.links.is_empty()
    }
    fn clamp_cursor(&mut self) {
        let len = if self.more() {
            self.links.len()
        } else {
            self.fields().len()
        };
        self.cursor = self.cursor.min(len.saturating_sub(1));
    }
    fn kind(&self, field: &FieldView) -> ValueKind {
        let Some(snapshot) = &self.snapshot else {
            return ValueKind::ReadOnly(ReadOnlyReason::Unsupported(field.kind.name().into()));
        };
        value_kind(
            field,
            self.pending
                .get(&field.key)
                .or_else(|| lookup(&snapshot.effective, &field.key)),
            lookup(&snapshot.defaults, &field.key),
            lookup(&snapshot.raw, &field.key),
        )
    }
    fn pend(&mut self, key: String, value: Value) {
        let baseline = self
            .snapshot
            .as_ref()
            .and_then(|s| lookup(&s.effective, &key))
            .unwrap_or(&Value::Null);
        if &value == baseline {
            self.pending.remove(&key);
            self.markers.remove(&key);
        } else {
            self.markers.remove(&key);
            self.pending.insert(key, value);
        }
    }
    fn apply_command(&mut self, pairs: Vec<(String, String)>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let mut changes = Vec::new();
        let mut errors = Vec::new();
        let mut first = None;
        for (key, text) in pairs {
            let result = if let Some(field) = snapshot.schema.fields.iter().find(|f| f.key == key) {
                let kind = self.kind(field);
                if let ValueKind::ReadOnly(reason) = &kind {
                    Err(text::readonly(&key, reason))
                } else {
                    parse_input(
                        &kind,
                        field,
                        &text,
                        lookup(&snapshot.effective, &key)
                            .filter(|v| !v.is_null())
                            .or_else(|| lookup(&snapshot.defaults, &key)),
                    )
                    .map_err(|e| format!("{key}: {e}"))
                }
            } else {
                Err(format!("Unknown setting {key}."))
            };
            match result {
                Ok(value) => changes.push((key, value)),
                Err(error) => {
                    if first.is_none() {
                        first = Some(key)
                    }
                    errors.push(error)
                }
            }
        }
        if errors.is_empty() {
            for (key, value) in changes {
                self.pend(key, value);
            }
            self.screen = Screen::Reviewing;
        } else {
            self.screen = Screen::Browsing;
            self.banner = errors;
            self.filter = first.unwrap_or_default();
            self.cursor = 0;
        }
    }
    fn write(&mut self) -> Vec<SettingsEffect> {
        self.sent = self.pending.clone();
        self.before = self
            .snapshot
            .as_ref()
            .map(|s| s.effective.clone())
            .unwrap_or(Value::Null);
        self.screen = Screen::Applying;
        vec![SettingsEffect::Write {
            patch: build_patch(&self.sent),
        }]
    }
    fn written(&mut self, result: WriteResult) -> Vec<SettingsEffect> {
        self.write_ok = matches!(result, WriteResult::Ok);
        let message = match result {
            WriteResult::Ok => None,
            WriteResult::Server { status } => Some(format!(
                "Hermes could not save (HTTP {status}). Checking what was saved…"
            )),
            WriteResult::Unknown => Some(
                "The reply to the save was lost or unreadable. Checking what was saved…".into(),
            ),
            other => {
                self.screen = Screen::Reviewing;
                let message = match other {
                    WriteResult::Refused { status: 400, detail } => {
                        let mut detail = detail.unwrap_or_else(|| "the value was refused".into());
                        for (key, value) in &self.sent {
                            if is_secret_key(key)
                                && let Some(secret) = value.as_str().filter(|value| !value.is_empty())
                            {
                                detail = detail.replace(secret, "(secret)");
                            }
                        }
                        format!("Hermes refused the change: {detail}.")
                    }
                    WriteResult::Refused { status, .. } => format!(
                        "Talaria sent a request Hermes could not accept (HTTP {status}). Please report this."
                    ),
                    WriteResult::Forbidden => "Talaria sent a request Hermes could not accept (HTTP 403). Please report this.".into(),
                    WriteResult::Unauthorized => text::save_failed(&FailReason::Unauthorized),
                    WriteResult::NotSent(why) => text::save_failed(&why),
                    WriteResult::NotFound { route } => format!("This Hermes does not offer {route} (HTTP 404)."),
                    _ => String::new(),
                };
                self.banner = vec![message];
                return Vec::new();
            }
        };
        if let Some(message) = message {
            self.banner = vec![message]
        }
        self.screen = Screen::Verifying;
        vec![SettingsEffect::ReadBack]
    }
    fn readback(&mut self, effective: Value, raw: Value) -> Vec<SettingsEffect> {
        let mut saved = Vec::new();
        let mut lines = Vec::new();
        for (key, sent) in &self.sent {
            let after = lookup(&effective, key);
            let outcome = classify_readback(sent, lookup(&self.before, key), after);
            let shown = if is_secret_key(key) || after.is_some_and(contains_secret_key) {
                "(secret)".into()
            } else {
                format_value(key, after)
            };
            match outcome {
                KeyOutcome::Applied | KeyOutcome::Adjusted(_) => {
                    self.pending.remove(key);
                    self.markers.remove(key);
                    self.saved
                        .insert(key.clone(), after.cloned().unwrap_or(Value::Null));
                    saved.push(key.clone());
                    if matches!(outcome, KeyOutcome::Adjusted(_)) {
                        lines.push(format!("{key}: saved as {shown}, which differs from what was sent (Hermes adjusted it, or another client changed it)."));
                    }
                }
                KeyOutcome::NotApplied => {
                    self.markers.insert(key.clone(), "!".into());
                    lines.push(if self.write_ok{format!("{key}: not saved. Hermes kept the previous value; an administrator or a package-managed install may control this setting.")}else{format!("{key}: not saved.")});
                }
                KeyOutcome::ChangedElsewhere(_) => {
                    self.markers.insert(key.clone(), "!".into());
                    lines.push(format!(
                        "{key}: now {shown}; something else changed it while saving."
                    ));
                }
            }
        }
        self.banner = vec![if saved.is_empty() {
            format!("Nothing was saved for profile {}.", self.profile)
        } else {
            format!(
                "Saved {} change(s) for profile {}.",
                saved.len(),
                self.profile
            )
        }];
        for (effect, prefix) in [
            (
                ApplyEffect::NeedsRestart,
                "Needs a gateway restart to take effect",
            ),
            (
                ApplyEffect::MayNeedRestart,
                "May need a gateway restart to take effect",
            ),
            (ApplyEffect::NewSessions, "Applies to new sessions"),
        ] {
            let keys: Vec<_> = saved
                .iter()
                .filter(|key| apply_effect(key).0 == effect)
                .cloned()
                .collect();
            if !keys.is_empty() {
                self.banner
                    .push(format!("{prefix}: {}.", text::limited(&keys)));
            }
        }
        self.banner.extend(lines);
        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.effective = effective;
            snapshot.raw = raw;
        }
        self.screen = Screen::Browsing;
        self.unverified = false;
        self.sent.clear();
        if self.command_close && !saved.is_empty() {
            self.close()
        } else {
            Vec::new()
        }
    }
    fn close(&mut self) -> Vec<SettingsEffect> {
        self.reveal = None;
        self.editor = None;
        self.screen = Screen::Closed;
        let mut effects = vec![SettingsEffect::CloseOverlay];
        if !self.saved.is_empty() {
            let mut values = Vec::new();
            if let Some(snapshot) = &self.snapshot {
                for field in snapshot.schema.ordered_fields() {
                    if let Some(value) = self.saved.get(&field.key) {
                        let shown = if is_secret_key(&field.key) || contains_secret_key(value) {
                            "(secret)".into()
                        } else {
                            format_value(&field.key, Some(value))
                        };
                        values.push(format!("{} = {shown}", field.key));
                    }
                }
            }
            effects.push(SettingsEffect::Notice(format!(
                "Settings saved for profile {}: {}.",
                self.profile,
                text::limited(&values)
            )));
            self.saved.clear();
        }
        if let Some(action) = self.close_action.take() {
            effects.push(SettingsEffect::OpenLocal(action));
        }
        self.pending.clear();
        effects
    }
    fn key(&mut self, key: SettingsKey) -> Vec<SettingsEffect> {
        if self.busy() {
            return Vec::new();
        }
        if matches!(key, SettingsKey::Action(SettingsAction::Reload)) && self.snapshot.is_some() {
            self.reloading = true;
            return vec![SettingsEffect::Load {
                profile: self.profile.clone(),
            }];
        }
        if self.screen == Screen::Help {
            self.screen = Screen::Browsing;
            return Vec::new();
        }
        if self.screen == Screen::LoadError {
            return match key {
                SettingsKey::Char('r') => {
                    self.screen = Screen::Loading;
                    vec![SettingsEffect::Load {
                        profile: self.profile.clone(),
                    }]
                }
                SettingsKey::Edit(EditKey::Esc) | SettingsKey::Action(SettingsAction::Close) => {
                    self.close()
                }
                _ => Vec::new(),
            };
        }
        if self.screen == Screen::Closed || self.screen == Screen::Loading {
            return Vec::new();
        }
        if self.screen == Screen::ConfirmDiscard {
            return match key {
                SettingsKey::Char('y') => self.close(),
                SettingsKey::Char('n') | SettingsKey::Edit(EditKey::Esc) => {
                    self.screen = Screen::Browsing;
                    self.close_action = None;
                    Vec::new()
                }
                _ => Vec::new(),
            };
        }
        if self.screen == Screen::Reviewing {
            return match key {
                SettingsKey::Char('y') | SettingsKey::Edit(EditKey::Enter) => {
                    if self.pending.is_empty() {
                        self.screen = Screen::Browsing;
                        Vec::new()
                    } else {
                        self.screen = Screen::Checking;
                        vec![SettingsEffect::FetchFresh]
                    }
                }
                SettingsKey::Char('n') | SettingsKey::Edit(EditKey::Esc) => {
                    if self.command_close {
                        self.close()
                    } else {
                        self.screen = Screen::Browsing;
                        Vec::new()
                    }
                }
                _ => Vec::new(),
            };
        }
        if self.screen == Screen::Conflict {
            return match key {
                SettingsKey::Edit(EditKey::Esc) => {
                    self.screen = Screen::Reviewing;
                    Vec::new()
                }
                SettingsKey::Char('o' | 't') => {
                    let take = matches!(key, SettingsKey::Char('t'));
                    if let Some(snapshot) = self.snapshot.as_mut() {
                        for conflict in &self.conflicts {
                            put(
                                &mut snapshot.effective,
                                &conflict.key,
                                conflict.now.clone().unwrap_or(Value::Null),
                            );
                            if take {
                                self.pending.remove(&conflict.key);
                                self.markers.remove(&conflict.key);
                            }
                        }
                    }
                    let count = self.conflicts.len();
                    self.conflicts.clear();
                    if take {
                        self.screen = if self.pending.is_empty() {
                            Screen::Browsing
                        } else {
                            Screen::Reviewing
                        };
                        self.banner = vec![text::outside(count)];
                        Vec::new()
                    } else {
                        self.write()
                    }
                }
                _ => Vec::new(),
            };
        }
        if self.screen == Screen::Editing {
            return self.editor_key(key);
        }
        if self.screen == Screen::Choosing {
            return self.choice_key(key);
        }
        if self.filter_focus {
            match key {
                SettingsKey::Edit(EditKey::Enter | EditKey::Down) => {
                    self.filter_focus = false;
                    self.cursor = 0;
                }
                SettingsKey::Edit(EditKey::Esc) => {
                    self.filter.clear();
                    self.filter_focus = false;
                    self.cursor = 0;
                }
                key => {
                    edit_text(&mut self.filter, &mut self.filter_caret, &key);
                    self.cursor = 0;
                }
            }
            return Vec::new();
        }
        let action = match key {
            SettingsKey::Action(action) => action,
            SettingsKey::Edit(EditKey::Enter) => SettingsAction::Edit,
            SettingsKey::Edit(EditKey::Esc) => SettingsAction::Close,
            _ => return Vec::new(),
        };
        match action {
            SettingsAction::Up | SettingsAction::PageUp => {
                self.cursor =
                    self.cursor
                        .saturating_sub(if action == SettingsAction::Up { 1 } else { 10 })
            }
            SettingsAction::Down | SettingsAction::PageDown => {
                self.cursor = self
                    .cursor
                    .saturating_add(if action == SettingsAction::Down {
                        1
                    } else {
                        10
                    });
                self.clamp_cursor();
            }
            SettingsAction::PrevCategory | SettingsAction::NextCategory => {
                if self.filter.is_empty() {
                    let len = self
                        .snapshot
                        .as_ref()
                        .map_or(0, |s| s.schema.categories().len())
                        + usize::from(!self.links.is_empty());
                    if len > 0 {
                        self.category = if action == SettingsAction::PrevCategory {
                            (self.category + len - 1) % len
                        } else {
                            (self.category + 1) % len
                        };
                        self.cursor = 0;
                    }
                }
            }
            SettingsAction::Filter => {
                self.filter_focus = true;
                self.filter_caret = self.filter.chars().count();
            }
            SettingsAction::Edit | SettingsAction::Toggle => {
                if self.more() {
                    if let Some(link) = self.links.get(self.cursor) {
                        self.close_action = Some(link.action);
                        return self.request_close();
                    }
                } else if !self.unverified {
                    self.start_edit(action == SettingsAction::Toggle)
                }
            }
            SettingsAction::Default => {
                if !self.unverified
                    && let Some(field) = self.selected()
                    && !matches!(self.kind(&field), ValueKind::ReadOnly(_))
                    && let Some(default) = self
                        .snapshot
                        .as_ref()
                        .and_then(|s| lookup(&s.defaults, &field.key))
                        .cloned()
                {
                    self.pend(field.key, default);
                }
            }
            SettingsAction::UndoField => {
                if let Some(field) = self.selected() {
                    self.pending.remove(&field.key);
                    self.markers.remove(&field.key);
                }
            }
            SettingsAction::Review => {
                if self.pending.is_empty() {
                    self.banner = vec!["No changes to review.".into()]
                } else if !self.unverified {
                    self.screen = Screen::Reviewing;
                }
            }
            SettingsAction::Reveal => return self.reveal_selected(),
            SettingsAction::Help => self.screen = Screen::Help,
            SettingsAction::Close => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.cursor = 0;
                } else {
                    return self.request_close();
                }
            }
            SettingsAction::Reload => {}
        }
        Vec::new()
    }
    fn request_close(&mut self) -> Vec<SettingsEffect> {
        if self.pending.is_empty() {
            self.close()
        } else {
            self.screen = Screen::ConfirmDiscard;
            Vec::new()
        }
    }
    fn start_edit(&mut self, toggle_only: bool) {
        let Some(field) = self.selected() else { return };
        let kind = self.kind(&field);
        if let ValueKind::ReadOnly(reason) = &kind {
            self.banner = vec![text::readonly(&field.key, reason)];
            return;
        }
        let current = self
            .pending
            .get(&field.key)
            .or_else(|| {
                self.snapshot
                    .as_ref()
                    .and_then(|s| lookup(&s.effective, &field.key))
            })
            .cloned()
            .unwrap_or(Value::Null);
        if kind == ValueKind::Bool {
            self.pend(field.key, Value::Bool(!current.as_bool().unwrap_or(false)));
            return;
        }
        if toggle_only {
            return;
        }
        if kind == ValueKind::Choice {
            self.choice_filter.clear();
            self.choice_cursor = self
                .choices()
                .iter()
                .position(|value| value == &current)
                .unwrap_or(0);
            self.screen = Screen::Choosing;
            return;
        }
        let secret = is_secret_key(&field.key);
        let text = if secret {
            String::new()
        } else {
            match &kind {
                ValueKind::StringList => current
                    .as_array()
                    .map(|items| items.iter().map(option_text).collect::<Vec<_>>().join(", "))
                    .unwrap_or_default(),
                _ => {
                    if current.is_null() {
                        String::new()
                    } else {
                        option_text(&current)
                    }
                }
            }
        };
        self.editor = Some(Editor {
            key: field.key,
            kind,
            text: SecretField::new(text.clone()),
            caret: text.chars().count(),
            secret,
        });
        self.screen = Screen::Editing;
    }
    fn editor_key(&mut self, key: SettingsKey) -> Vec<SettingsEffect> {
        if matches!(key, SettingsKey::Edit(EditKey::Esc)) {
            self.editor = None;
            self.screen = Screen::Browsing;
            return Vec::new();
        }
        if matches!(key, SettingsKey::Action(SettingsAction::Reveal)) {
            return self.reveal_selected();
        }
        if matches!(key, SettingsKey::Edit(EditKey::Enter)) {
            let Some(editor) = &self.editor else {
                return Vec::new();
            };
            if editor.secret && editor.text.expose().is_empty() {
                self.editor = None;
                self.screen = Screen::Browsing;
                return Vec::new();
            }
            let Some(snapshot) = &self.snapshot else {
                return Vec::new();
            };
            let Some(field) = snapshot.schema.fields.iter().find(|f| f.key == editor.key) else {
                return Vec::new();
            };
            let current = lookup(&snapshot.effective, &editor.key)
                .filter(|v| !v.is_null())
                .or_else(|| lookup(&snapshot.defaults, &editor.key));
            match parse_input(&editor.kind, field, editor.text.expose(), current) {
                Ok(value) => {
                    let key = editor.key.clone();
                    self.pend(key, value);
                    self.editor = None;
                    self.screen = Screen::Browsing;
                    self.banner.clear();
                }
                Err(error) => self.banner = vec![error.message],
            };
        } else if let Some(editor) = self.editor.as_mut() {
            if matches!(&key,SettingsKey::Paste(text)if text.chars().any(char::is_control)) {
                self.banner = vec!["Use one line, without control characters.".into()]
            } else {
                edit_text(editor.text.text_mut(), &mut editor.caret, &key);
            }
        }
        Vec::new()
    }
    fn choices(&self) -> Vec<Value> {
        let Some(field) = self.selected() else {
            return Vec::new();
        };
        let mut choices = field.options.unwrap_or_default();
        if field.clearable {
            choices.insert(0, Value::String(String::new()));
        }
        let filter = self.choice_filter.to_lowercase();
        choices
            .into_iter()
            .filter(|value| option_text(value).to_lowercase().contains(&filter))
            .collect()
    }
    fn choice_key(&mut self, key: SettingsKey) -> Vec<SettingsEffect> {
        match key {
            SettingsKey::Edit(EditKey::Esc) => self.screen = Screen::Browsing,
            SettingsKey::Edit(EditKey::Up) => {
                self.choice_cursor = self.choice_cursor.saturating_sub(1)
            }
            SettingsKey::Edit(EditKey::Down) => {
                self.choice_cursor =
                    (self.choice_cursor + 1).min(self.choices().len().saturating_sub(1))
            }
            SettingsKey::Edit(EditKey::Enter) => {
                if let (Some(field), Some(value)) = (
                    self.selected(),
                    self.choices().get(self.choice_cursor).cloned(),
                ) {
                    self.pend(field.key, value);
                    self.screen = Screen::Browsing;
                }
            }
            key => {
                let mut caret = self.choice_filter.chars().count();
                edit_text(&mut self.choice_filter, &mut caret, &key);
                self.choice_cursor = 0;
            }
        }
        Vec::new()
    }
    fn reveal_selected(&mut self) -> Vec<SettingsEffect> {
        let Some(field) = self.selected() else {
            return Vec::new();
        };
        let Some(snapshot) = &self.snapshot else {
            return Vec::new();
        };
        if !is_secret_key(&field.key)
            || lookup(&snapshot.raw, &field.key).is_some_and(has_env_reference)
        {
            return Vec::new();
        }
        let value = if let Some(editor) = &self.editor {
            editor.text.expose().to_owned()
        } else {
            self.pending
                .get(&field.key)
                .or_else(|| lookup(&snapshot.effective, &field.key))
                .map(option_text)
                .unwrap_or_default()
        };
        self.next_reveal = self.next_reveal.wrapping_add(1);
        self.reveal = Some((self.next_reveal, SecretField::new(value)));
        vec![SettingsEffect::ScheduleReveal {
            id: self.next_reveal,
            after: Duration::from_secs(10),
        }]
    }
    pub fn view(&self) -> SettingsView {
        let mut categories = self
            .snapshot
            .as_ref()
            .map(|s| s.schema.categories())
            .unwrap_or_default();
        if !self.links.is_empty() {
            categories.push("More".into())
        }
        let category = categories.get(self.category).cloned().unwrap_or_default();
        let mut view = SettingsView {
            screen: self.screen,
            profile: self.profile.clone(),
            text_focus: self.filter_focus
                || matches!(self.screen, Screen::Editing | Screen::Choosing),
            categories,
            category,
            cursor: self.cursor,
            banner: self.banner.clone(),
            filter: self.filter.clone(),
            choice_cursor: self.choice_cursor,
            ..SettingsView::default()
        };
        if let Some(snapshot) = &self.snapshot {
            for field in self.fields() {
                let current = self
                    .pending
                    .get(&field.key)
                    .or_else(|| lookup(&snapshot.effective, &field.key));
                let raw = lookup(&snapshot.raw, &field.key);
                let mut value = if raw.is_some_and(contains_secret_key) {
                    "•••••••• (r reveals once)".into()
                } else if raw.is_some_and(has_env_reference) {
                    raw.map(Value::to_string).unwrap_or_default()
                } else {
                    format_value(&field.key, current)
                };
                if field.kind == FieldKind::Select
                    && !is_secret_key(&field.key)
                    && !raw.is_some_and(has_env_reference)
                    && current.is_some_and(|current| {
                        !field
                            .options
                            .as_ref()
                            .is_some_and(|options| options.contains(current))
                            && !(field.clearable && current.as_str() == Some(""))
                    })
                {
                    value.push_str(" (not in the list)");
                }
                view.rows.push(SettingsRow {
                    key: field.key.clone(),
                    value,
                    origin: if raw.is_some() { "set" } else { "default" }.into(),
                    marker: self.markers.get(&field.key).cloned().unwrap_or_else(|| {
                        if self.pending.contains_key(&field.key) {
                            "*".into()
                        } else {
                            String::new()
                        }
                    }),
                    effect_label: apply_effect(&field.key).0.label().into(),
                });
            }
            view.review_rows = review_rows(snapshot, &self.pending);
            if let Some(field) = self.selected() {
                let reason = match self.kind(&field) {
                    ValueKind::ReadOnly(reason) => text::readonly(&field.key, &reason),
                    _ => String::new(),
                };
                let default = format_value(&field.key, lookup(&snapshot.defaults, &field.key));
                view.detail = format!(
                    "{} · {} · {} · default {} · {} · {}",
                    field.key,
                    field.kind.name(),
                    field.description,
                    default,
                    apply_effect(&field.key).0.label(),
                    reason
                );
                if let Some(options) = &field.options
                    && !is_secret_key(&field.key)
                {
                    view.detail.push_str(&format!(
                        " · options {}",
                        options
                            .iter()
                            .map(|value| if contains_secret_key(value) {
                                "••••••••".into()
                            } else {
                                option_text(value)
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
        }
        if let Some((_, value)) = &self.reveal {
            view.detail = format!("{} (hidden on the next key)", value.expose());
        }
        view.editor = self.editor.as_ref().map(|editor| SettingsEditorView {
            text: if editor.secret {
                "•".repeat(editor.text.expose().chars().count())
            } else {
                editor.text.expose().into()
            },
            cursor: editor.caret,
            secret: editor.secret,
        });
        if let Some(editor) = &self.editor
            && editor.kind == ValueKind::Nullable
            && let Some(snapshot) = &self.snapshot
            && let Some(field) = snapshot
                .schema
                .fields
                .iter()
                .find(|field| field.key == editor.key)
            && let Ok(value) = parse_input(&editor.kind, field, editor.text.expose(), None)
        {
            let kind = match value {
                Value::Null => "null",
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "list",
                Value::Object(_) => "object",
            };
            view.detail.push_str(&format!(
                " · saved as {kind} {}",
                format_value(&editor.key, Some(&value))
            ));
        }
        if self.screen == Screen::Choosing {
            let secret = self
                .selected()
                .is_some_and(|field| is_secret_key(&field.key));
            view.choices = self
                .choices()
                .iter()
                .map(|v| {
                    if secret || contains_secret_key(v) {
                        "••••••••".into()
                    } else if v.as_str() == Some("") {
                        "(clear)".into()
                    } else {
                        option_text(v)
                    }
                })
                .collect();
        }
        view.conflict_rows = self
            .conflicts
            .iter()
            .cloned()
            .map(|mut conflict| {
                if is_secret_key(&conflict.key)
                    || contains_secret_key(&conflict.yours)
                    || conflict.now.as_ref().is_some_and(contains_secret_key)
                {
                    conflict.yours = Value::String("••••".into());
                    conflict.baseline = Some(Value::String("••••".into()));
                    conflict.now = Some(Value::String("••••".into()));
                }
                conflict
            })
            .collect();
        if self.more() {
            view.links = self.links.iter().map(|link| link.label.into()).collect();
        }
        view.help = [
            ("↑ ↓ / PgUp PgDn", "move"),
            ("← →", "category"),
            ("Enter / Space", "edit / toggle"),
            ("/", "filter"),
            ("d / u", "default / undo"),
            ("r", "reveal once"),
            ("a", "review"),
            ("Ctrl+R", "reload"),
            ("Esc", "clear filter / close"),
            ("* pending · ! not saved · ? not verified", ""),
        ]
        .into_iter()
        .map(|(key, meaning)| (key.into(), meaning.into()))
        .collect();
        view.footer = if self.busy() {
            text::APPLYING.into()
        } else {
            match self.screen {
                Screen::Reviewing => "Enter apply · Esc back".into(),
                Screen::Conflict => "o keep yours · t take theirs · Esc back".into(),
                Screen::ConfirmDiscard => format!(
                    "Discard {} unapplied change(s)? y discard · n keep editing",
                    self.pending.len()
                ),
                Screen::Editing | Screen::Choosing => "Enter save edit · Esc cancel".into(),
                _ => "Enter edit · / filter · a review · ? help · Esc close".into(),
            }
        };
        if self.screen == Screen::Loading {
            view.banner = vec![format!("Loading settings for profile {}…", self.profile)]
        }
        if self.screen == Screen::Reviewing {
            view.detail = format!(
                "Review {} change(s) · profile {}",
                self.pending.len(),
                self.profile
            )
        }
        if self.screen == Screen::Conflict {
            view.detail = format!(
                "{} setting(s) changed outside Talaria since you opened settings.",
                self.conflicts.len()
            )
        }
        if self.screen == Screen::Browsing && !self.filter.is_empty() && view.rows.is_empty() {
            view.banner
                .push(format!("No setting matches \"{}\".", self.filter));
        }
        view
    }
}

fn edit_text(text: &mut String, caret: &mut usize, key: &SettingsKey) {
    let mut chars: Vec<char> = text.chars().collect();
    *caret = (*caret).min(chars.len());
    match key {
        SettingsKey::Char(ch) if !ch.is_control() => {
            chars.insert(*caret, *ch);
            *caret += 1;
        }
        SettingsKey::Paste(value) if !value.chars().any(char::is_control) => {
            let insert: Vec<_> = value.chars().collect();
            let len = insert.len();
            chars.splice(*caret..*caret, insert);
            *caret += len;
        }
        SettingsKey::Edit(EditKey::Left) => *caret = caret.saturating_sub(1),
        SettingsKey::Edit(EditKey::Right) => *caret = (*caret + 1).min(chars.len()),
        SettingsKey::Edit(EditKey::Home) => *caret = 0,
        SettingsKey::Edit(EditKey::End) => *caret = chars.len(),
        SettingsKey::Edit(EditKey::ClearLine) => {
            chars.clear();
            *caret = 0;
        }
        SettingsKey::Edit(EditKey::Backspace) if *caret > 0 => {
            *caret -= 1;
            chars.remove(*caret);
        }
        SettingsKey::Edit(EditKey::Delete) if *caret < chars.len() => {
            chars.remove(*caret);
        }
        _ => {}
    }
    *text = chars.into_iter().collect();
}
