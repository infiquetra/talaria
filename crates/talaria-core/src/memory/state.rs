use super::*;
use std::time::Instant;

mod editing;
mod following;
mod rendering;
mod writes;

const WAITING: &str = "Waiting for Hermes.";
const UNKNOWN: &str = "The connection dropped after Talaria sent the request, so it may or may not have happened. Reading the current state…";
const DEFAULTS: &str = "Hermes also stores the default of every setting you left unset.";
const DISCONNECTED: &str = "Not connected to a Hermes gateway, so memory settings cannot be read.";

enum ConfirmAction {
    Switch(String),
    Save,
    Reset(ResetTarget),
    Run,
    Discard(Option<&'static str>),
}
struct Confirmation {
    view: ConfirmView,
    action: ConfirmAction,
}
enum Write {
    Switch(String),
    Save(Box<SaveCheck>),
    Reset(ResetTarget),
    Pause(bool),
    Run,
}
struct SaveCheck {
    before: ConfigView,
    values: SubmittedValues,
    defaults: bool,
    config_answered: bool,
    after: Option<Result<ConfigView, FailText>>,
    status_answered: bool,
    detail: Option<String>,
}
struct Card {
    name: String,
    config: Option<Result<ConfigView, FailText>>,
    pending: SubmittedValues,
    failed: Vec<String>,
    cursor: usize,
    results: Vec<String>,
}
enum EditorText {
    Plain(String),
    Secret(SecretField),
}
struct Editor {
    key: String,
    text: EditorText,
    cursor: usize,
}
enum RunCheck {
    BeforeStart(u64),
    UnknownStart(u64),
}
struct Follow {
    id: u64,
    pid: Option<i64>,
    started: Instant,
    polls: u16,
    failures: u8,
    lines: Vec<String>,
}

pub struct MemoryState {
    profile: String,
    open: bool,
    connected: bool,
    status: Option<Result<StatusView, FailText>>,
    curator: Option<Result<CuratorView, FailText>>,
    banner: Option<String>,
    cursor: usize,
    card: Option<Card>,
    editor: Option<Editor>,
    confirm: Option<Confirmation>,
    write: Option<Write>,
    verifying: bool,
    check: Option<RunCheck>,
    follow: Option<Follow>,
    next_follow: u64,
}

impl Default for MemoryState {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryState {
    /// A write or its readback still owns the current profile.
    pub fn is_busy(&self) -> bool {
        self.write.is_some() || matches!(self.check, Some(RunCheck::UnknownStart(_)))
    }

    pub fn new() -> Self {
        Self {
            profile: String::new(),
            open: false,
            connected: true,
            status: None,
            curator: None,
            banner: None,
            cursor: 0,
            card: None,
            editor: None,
            confirm: None,
            write: None,
            verifying: false,
            check: None,
            follow: None,
            next_follow: 0,
        }
    }

    pub fn update(&mut self, input: MemoryInput, now: Instant) -> Vec<MemoryEffect> {
        match input {
            MemoryInput::Open { profile } => {
                if self.is_busy() {
                    self.open = true;
                    self.banner = Some(WAITING.into());
                    return Vec::new();
                }
                self.profile = profile;
                self.open = true;
                self.connected = true;
                self.status = None;
                self.curator = None;
                self.cursor = 0;
                self.card = None;
                self.editor = None;
                self.confirm = None;
                self.check = None;
                self.follow = None;
                self.banner = Some(format!(
                    "Loading memory settings for profile {}…",
                    self.profile
                ));
                vec![MemoryEffect::LoadStatus, MemoryEffect::LoadCurator]
            }
            MemoryInput::NotConnected => {
                self.connected = false;
                self.banner = Some(DISCONNECTED.into());
                Vec::new()
            }
            MemoryInput::Key(key) => self.key(key, now),
            MemoryInput::StatusLoaded(result) => {
                self.status = Some(result);
                self.cursor = self.cursor.min(self.row_count().saturating_sub(1));
                self.clear_loading();
                self.status_readback()
            }
            MemoryInput::CuratorLoaded(result) => {
                self.curator = Some(result);
                self.clear_loading();
                self.curator_readback()
            }
            MemoryInput::ConfigLoaded { name, result } => {
                if let Some(card) = &mut self.card
                    && card.name == name
                {
                    card.config = Some(result.clone());
                }
                if self.verifying {
                    if let Some(Write::Save(save)) = &mut self.write
                        && save.before.name == name
                    {
                        save.config_answered = true;
                        save.after = Some(result);
                    }
                    self.finish_save()
                } else {
                    if self.card.as_ref().is_some_and(|card| card.name == name) {
                        self.banner = None;
                    }
                    Vec::new()
                }
            }
            MemoryInput::ProviderSet(outcome) => self.write_reply(outcome, "memory provider", now),
            MemoryInput::ConfigSaved(outcome) => {
                self.write_reply(outcome, "memory provider settings", now)
            }
            MemoryInput::ResetDone(outcome) => self.write_reply(outcome, "memory reset", now),
            MemoryInput::PausedSet(outcome) => self.write_reply(outcome, "curator pause", now),
            MemoryInput::RunStarted(outcome) => self.write_reply(outcome, "curator run", now),
            MemoryInput::ActionPolled { follow, result } => self.action_polled(follow, result, now),
        }
    }

    fn clear_loading(&mut self) {
        if self
            .banner
            .as_deref()
            .is_some_and(|text| text.starts_with("Loading memory settings"))
        {
            self.banner = None;
        }
    }
    fn status(&self) -> Option<&StatusView> {
        self.status.as_ref()?.as_ref().ok()
    }
    fn curator(&self) -> Option<&CuratorView> {
        self.curator.as_ref()?.as_ref().ok()
    }
    fn active(&self) -> &str {
        self.status().map_or("", |s| s.active.as_str())
    }
    fn row_count(&self) -> usize {
        self.provider_names().len() + 3
    }
    fn provider_names(&self) -> Vec<String> {
        let mut names = vec![String::new()];
        if let Some(status) = self.status() {
            names.extend(status.providers.iter().map(|p| p.name.clone()));
            if !status.active.is_empty() && !names.contains(&status.active) {
                names.push(status.active.clone());
            }
        }
        names
    }
    fn selected_provider(&self) -> Option<String> {
        if let Some(card) = &self.card {
            return Some(card.name.clone());
        }
        self.provider_names().get(self.cursor).cloned()
    }
    fn provider(&self, name: &str) -> Option<&ProviderRowView> {
        self.status()?
            .providers
            .iter()
            .find(|provider| provider.name == name)
    }
    fn setup_command(&self, name: &str) -> String {
        format!("hermes -p {} memory setup {name}", self.profile)
    }
    fn confirm(&mut self, lines: Vec<String>, button: &str, action: ConfirmAction) {
        self.confirm = Some(Confirmation {
            view: ConfirmView {
                lines,
                buttons: ["Cancel".into(), button.into()],
                selected: 0,
            },
            action,
        });
    }
    fn key(&mut self, key: MemoryKey, now: Instant) -> Vec<MemoryEffect> {
        if self.confirm.is_some() {
            return self.confirm_key(key);
        }
        if self.editor.is_some() {
            return self.editor_key(key);
        }
        if !self.open {
            return Vec::new();
        }
        if key == MemoryKey::Close {
            return self.close_or_discard(None);
        }
        if !self.connected {
            return Vec::new();
        }
        if self.write.is_some()
            && matches!(
                key,
                MemoryKey::Use
                    | MemoryKey::Delete
                    | MemoryKey::DeleteBoth
                    | MemoryKey::Pause
                    | MemoryKey::Run
                    | MemoryKey::Save
                    | MemoryKey::Reload
            )
        {
            self.banner = Some(WAITING.into());
            return Vec::new();
        }
        match key {
            MemoryKey::Settings => self.close_or_discard(Some("memory.")),
            MemoryKey::CuratorSettings => self.close_or_discard(Some("curator.")),
            MemoryKey::Reload => {
                let mut effects = vec![MemoryEffect::LoadStatus, MemoryEffect::LoadCurator];
                if let Some(card) = &self.card {
                    effects.push(MemoryEffect::LoadConfig {
                        name: card.name.clone(),
                    });
                }
                effects
            }
            MemoryKey::Up | MemoryKey::Down => {
                let down = key == MemoryKey::Down;
                if let Some(card) = &self.card {
                    let count = self.visible_fields().len();
                    let cursor = card.cursor;
                    if let Some(card) = &mut self.card {
                        card.cursor = step(cursor, down, count);
                    }
                } else {
                    self.cursor = step(self.cursor, down, self.row_count());
                }
                Vec::new()
            }
            MemoryKey::Open => {
                if self.card.is_some() {
                    return self.start_editor();
                }
                let Some(name) = self.selected_provider() else {
                    return Vec::new();
                };
                if name.is_empty() {
                    return Vec::new();
                }
                if !is_valid_provider_name(&name) {
                    self.banner = Some("Talaria will not send this provider name.".into());
                    return Vec::new();
                }
                self.card = Some(Card {
                    name: name.clone(),
                    config: None,
                    pending: SubmittedValues::default(),
                    failed: Vec::new(),
                    cursor: 0,
                    results: Vec::new(),
                });
                self.banner = Some(format!("Loading settings for {name}…"));
                vec![MemoryEffect::LoadConfig { name }]
            }
            MemoryKey::Space => {
                self.toggle_selected_bool();
                Vec::new()
            }
            MemoryKey::Use => {
                self.ask_switch();
                Vec::new()
            }
            MemoryKey::Delete | MemoryKey::DeleteBoth => {
                self.ask_reset(key == MemoryKey::DeleteBoth);
                Vec::new()
            }
            MemoryKey::Pause if self.card.is_none() && self.cursor + 1 == self.row_count() => {
                let Some(curator) = self.curator() else {
                    return Vec::new();
                };
                if !curator.enabled {
                    self.off();
                    return Vec::new();
                }
                let paused = !curator.paused;
                self.write = Some(Write::Pause(paused));
                self.verifying = false;
                vec![MemoryEffect::SetPaused(paused)]
            }
            MemoryKey::Run if self.card.is_none() && self.cursor + 1 == self.row_count() => {
                self.check_run(now)
            }
            MemoryKey::Save => self.ask_save(),
            MemoryKey::Copy => self
                .card
                .as_ref()
                .map(|card| vec![MemoryEffect::CopyText(self.setup_command(&card.name))])
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn confirm_key(&mut self, key: MemoryKey) -> Vec<MemoryEffect> {
        let Some(mut confirmation) = self.confirm.take() else {
            return Vec::new();
        };
        match key {
            MemoryKey::Left => confirmation.view.selected = 0,
            MemoryKey::Right => confirmation.view.selected = 1,
            MemoryKey::Tab => confirmation.view.selected = 1 - confirmation.view.selected,
            MemoryKey::Close => return Vec::new(),
            MemoryKey::Open => {
                if confirmation.view.selected == 0 {
                    return Vec::new();
                }
                self.verifying = false;
                return match confirmation.action {
                    ConfirmAction::Switch(name) => {
                        self.write = Some(Write::Switch(name.clone()));
                        vec![MemoryEffect::SetProvider { name }]
                    }
                    ConfirmAction::Reset(target) => {
                        self.write = Some(Write::Reset(target));
                        vec![MemoryEffect::Reset { target }]
                    }
                    ConfirmAction::Save => self.send_save(),
                    ConfirmAction::Run => {
                        self.write = Some(Write::Run);
                        vec![MemoryEffect::RunCurator]
                    }
                    ConfirmAction::Discard(filter) => {
                        self.card = None;
                        if filter.is_some() {
                            self.close_or_discard(filter)
                        } else {
                            Vec::new()
                        }
                    }
                };
            }
            _ => {}
        }
        self.confirm = Some(confirmation);
        Vec::new()
    }

    fn close_or_discard(&mut self, filter: Option<&'static str>) -> Vec<MemoryEffect> {
        if let Some(card) = &self.card {
            let changes = card.pending.plain.len() + card.pending.secrets.len();
            if changes > 0 {
                self.confirm(
                    vec![format!(
                        "Discard {changes} unsaved change(s) to {}?",
                        card.name
                    )],
                    "Discard",
                    ConfirmAction::Discard(filter),
                );
                return Vec::new();
            }
            self.card = None;
            if filter.is_none() {
                return Vec::new();
            }
        }
        self.open = false;
        if !matches!(self.check, Some(RunCheck::UnknownStart(_))) {
            self.check = None;
        }
        let mut effects = Vec::new();
        if self.follow.take().is_some() {
            effects.push(MemoryEffect::Notice(
                "Curator run continues in Hermes; Talaria stopped following it.".into(),
            ));
        }
        effects.push(MemoryEffect::CloseOverlay);
        if let Some(filter) = filter {
            effects.push(MemoryEffect::OpenSettings {
                profile: self.profile.clone(),
                filter,
            });
        }
        effects
    }

    fn ask_switch(&mut self) {
        if self.status().is_none() {
            return;
        }
        let Some(name) = self.selected_provider() else {
            return;
        };
        let label = provider_name(&name);
        if name == self.active() {
            self.banner = Some(format!("{label} is already in use."));
            return;
        }
        if !name.is_empty() && !is_valid_provider_name(&name) {
            self.banner = Some("Talaria will not send this provider name.".into());
            return;
        }
        if !name.is_empty() && self.provider(&name).is_none_or(|p| p.status != "ready") {
            let status = self
                .provider(&name)
                .map_or("missing", |p| p.status.as_str());
            self.banner = Some(format!(
                "{name} is not ready ({}). Press enter to see what it needs.",
                provider_label(status)
            ));
            return;
        }
        self.confirm(
            vec![format!(
                "Switch the memory provider for profile {} from {} to {label}?",
                self.profile,
                provider_name(self.active())
            )],
            "Switch",
            ConfirmAction::Switch(name),
        );
    }

    fn ask_reset(&mut self, both: bool) {
        if self.card.is_some() {
            return;
        }
        let provider_count = self.provider_names().len();
        let target = if both {
            ResetTarget::All
        } else if !both && self.cursor == provider_count {
            ResetTarget::Memory
        } else if !both && self.cursor == provider_count + 1 {
            ResetTarget::User
        } else {
            return;
        };
        let Some(status) = self.status() else {
            return;
        };
        let files = &status.builtin_files;
        let mut names = Vec::new();
        if target != ResetTarget::User && files.memory > 0 {
            names.push("MEMORY.md");
        }
        if target != ResetTarget::Memory && files.user > 0 {
            names.push("USER.md");
        }
        if names.is_empty() {
            self.banner = Some(
                match target {
                    ResetTarget::Memory => "MEMORY.md is already empty.",
                    ResetTarget::User => "USER.md is already empty.",
                    ResetTarget::All => "MEMORY.md and USER.md are already empty.",
                }
                .into(),
            );
            return;
        }
        let mut lines = vec![format!(
            "Delete {} for profile {}? This cannot be undone.",
            names.join(" and "),
            self.profile
        )];
        if !status.active.is_empty() {
            lines.push(format!(
                "Only Hermes's built-in memory is deleted; data held by {} is not touched.",
                status.active
            ));
        }
        self.confirm(lines, "Delete", ConfirmAction::Reset(target));
    }

    fn off(&mut self) {
        self.banner = Some(format!(
            "The curator is off for profile {}. Turn on curator.enabled with k (curator settings).",
            self.profile
        ));
    }
}

fn provider_name(name: &str) -> &str {
    if name.is_empty() {
        "built-in only"
    } else {
        name
    }
}
fn step(cursor: usize, down: bool, count: usize) -> usize {
    if down {
        cursor.saturating_add(1).min(count.saturating_sub(1))
    } else {
        cursor.saturating_sub(1)
    }
}
