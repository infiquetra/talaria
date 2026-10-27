use super::text::{clean, long_text};
use super::*;

fn index(tab: CapTab) -> usize {
    match tab {
        CapTab::Skills => 0,
        CapTab::Hub => 1,
        CapTab::Toolsets => 2,
        CapTab::Plugins => 3,
    }
}
fn tab_at(i: usize) -> CapTab {
    [
        CapTab::Skills,
        CapTab::Hub,
        CapTab::Toolsets,
        CapTab::Plugins,
    ][i % 4]
}

#[derive(Debug, Default)]
struct Slot {
    generation: u64,
    detail_generation: u64,
    data: Option<CapData>,
    loading: bool,
    failure: Option<String>,
    selected: usize,
    filter: String,
}
#[derive(Clone, Debug)]
enum Dialog {
    Skill(HubItem),
    Plugin { target: InstallTarget, name: String },
    Remove(String),
}
#[derive(Clone, Debug)]
enum WriteKind {
    Skill {
        name: String,
        enabled: bool,
    },
    Toolset {
        name: String,
        label: String,
        enabled: bool,
    },
    Plugin {
        name: String,
        key: String,
        enabled: bool,
    },
    InstallSkill(HubItem),
    InstallPlugin(String),
    Remove(String),
}
impl WriteKind {
    fn tab(&self) -> CapTab {
        match self {
            Self::Skill { .. } | Self::InstallSkill(_) => CapTab::Skills,
            Self::Toolset { .. } => CapTab::Toolsets,
            _ => CapTab::Plugins,
        }
    }
    fn name(&self) -> &str {
        match self {
            Self::Skill { name, .. }
            | Self::Toolset { name, .. }
            | Self::Plugin { name, .. }
            | Self::InstallPlugin(name)
            | Self::Remove(name) => name,
            Self::InstallSkill(item) => &item.name,
        }
    }
    fn install(&self) -> bool {
        matches!(self, Self::InstallSkill(_) | Self::InstallPlugin(_))
    }
}
#[derive(Debug)]
struct Write {
    id: u64,
    kind: WriteKind,
    reading: bool,
    read_generation: Option<u64>,
    announced: bool,
    receipt: WriteReceipt,
}
#[derive(Clone, Copy, Debug)]
enum InputKind {
    Filter,
    Search,
    Install,
}
#[derive(Debug)]
struct TextInput {
    kind: InputKind,
    text: String,
    previous: String,
}

pub struct CapabilitiesState {
    profile: String,
    hermes_home: String,
    user_home: String,
    foreground: Option<SessionRef>,
    tab: CapTab,
    open: bool,
    slots: [Slot; 4],
    message: Vec<String>,
    message_persistent: bool,
    detail: Vec<String>,
    detail_focus: bool,
    detail_scroll: usize,
    input: Option<TextInput>,
    dialog: Option<Dialog>,
    scan: Option<HubItem>,
    lookup: Option<String>,
    write: Option<Write>,
    next_op: u64,
    reload_name: Option<String>,
    show_all: bool,
    page: u64,
    query: Option<String>,
    catalog: bool,
    catalog_generation: u64,
    catalog_entries: Vec<CatalogEntry>,
    due: Option<u64>,
    changed_during_write: bool,
}

impl std::fmt::Debug for CapabilitiesState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilitiesState")
            .field("tab", &self.tab)
            .field("open", &self.open)
            .field("busy", &self.is_busy())
            .finish_non_exhaustive()
    }
}

impl CapabilitiesState {
    pub fn new(profile: String, foreground: Option<SessionRef>, open_at: CapTab) -> Self {
        Self {
            profile,
            hermes_home: String::new(),
            user_home: String::new(),
            foreground,
            tab: open_at,
            open: false,
            slots: std::array::from_fn(|_| Slot::default()),
            message: Vec::new(),
            message_persistent: false,
            detail: Vec::new(),
            detail_focus: false,
            detail_scroll: 0,
            input: None,
            dialog: None,
            scan: None,
            lookup: None,
            write: None,
            next_op: 0,
            reload_name: None,
            show_all: false,
            page: 1,
            query: None,
            catalog: false,
            catalog_generation: 0,
            catalog_entries: Vec::new(),
            due: None,
            changed_during_write: false,
        }
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn is_busy(&self) -> bool {
        self.write.is_some() || self.reload_name.is_some()
    }
    pub fn set_display_paths(&mut self, hermes_home: String, user_home: String) {
        self.hermes_home = hermes_home;
        self.user_home = user_home;
    }
    fn display(&self, text: &str) -> String {
        clean(&redact_paths(text, &self.hermes_home, &self.user_home))
    }
    fn document(&self, text: &str) -> Vec<String> {
        long_text(&redact_paths(text, &self.hermes_home, &self.user_home))
    }
    fn formatted(&self, kind: CapText, args: &TextArgs) -> String {
        super::text::with_paths(kind, args, &self.hermes_home, &self.user_home)
    }
    fn args(&self) -> TextArgs {
        TextArgs {
            profile: self.profile.clone(),
            tab: self.tab,
            ..TextArgs::default()
        }
    }
    fn text(&self, kind: CapText) -> String {
        self.formatted(kind, &self.args())
    }
    fn notice(&mut self, text: String, effects: &mut Vec<CapEffect>) {
        let text = redact_paths(&text, &self.hermes_home, &self.user_home);
        self.message = text.lines().map(str::to_owned).collect();
        self.message_persistent = false;
        effects.push(CapEffect::Notice(text));
    }
    fn busy(&mut self, effects: &mut Vec<CapEffect>) -> bool {
        if self.write.is_some() {
            self.notice(self.text(CapText::Busy), effects);
            true
        } else {
            false
        }
    }
    fn load(&mut self, tab: CapTab, effects: &mut Vec<CapEffect>) {
        let slot = &mut self.slots[index(tab)];
        slot.generation += 1;
        slot.loading = true;
        slot.failure = None;
        let generation = slot.generation;
        let profile = self.profile.clone();
        effects.push(if tab == CapTab::Hub {
            if let Some(q) = &self.query {
                CapEffect::HubSearch {
                    profile,
                    q: q.clone(),
                    generation,
                }
            } else {
                CapEffect::HubBrowse {
                    profile,
                    page: self.page,
                    generation,
                }
            }
        } else {
            CapEffect::Fetch {
                profile,
                tab,
                generation,
            }
        });
    }
    fn visit(&mut self, tab: CapTab, effects: &mut Vec<CapEffect>) {
        self.tab = tab;
        self.detail_focus = false;
        self.detail.clear();
        self.detail_scroll = 0;
        self.input = None;
        self.catalog = false;
        if self.slots[index(tab)].generation == 0 {
            self.load(tab, effects);
        }
        if tab == CapTab::Hub && self.slots[0].generation == 0 {
            self.load(CapTab::Skills, effects);
        }
    }
    fn begin(
        &mut self,
        kind: WriteKind,
        effect: impl FnOnce(u64, String) -> CapEffect,
        effects: &mut Vec<CapEffect>,
    ) {
        self.next_op += 1;
        let id = self.next_op;
        self.write = Some(Write {
            id,
            kind,
            reading: false,
            read_generation: None,
            announced: false,
            receipt: WriteReceipt::default(),
        });
        effects.push(effect(id, self.profile.clone()));
    }
    fn finish(&mut self, effects: &mut Vec<CapEffect>) {
        self.write = None;
        if self.changed_during_write {
            self.changed_during_write = false;
            if self.slots[0].generation > 0 {
                self.load(CapTab::Skills, effects);
            }
        }
        if !self.open && self.reload_name.is_none() {
            effects.push(CapEffect::Dispose);
        }
    }
    pub fn update(&mut self, input: CapInput, now_ms: u64) -> Vec<CapEffect> {
        let mut effects = Vec::new();
        match input {
            CapInput::DismissMessage => {
                if self.dialog.is_none() && !self.message_persistent {
                    self.message.clear();
                }
            }
            CapInput::Open(tab) => {
                self.open = true;
                self.visit(tab, &mut effects);
            }
            CapInput::Close => {
                self.open = false;
                if self.write.is_none() && self.reload_name.is_none() {
                    effects.push(CapEffect::Dispose);
                }
            }
            CapInput::SetForeground(session) => self.foreground = session,
            CapInput::SkillsChanged => {
                if self.write.is_some() {
                    self.changed_during_write = true;
                } else if self.due.is_none() {
                    self.due = Some(now_ms.saturating_add(500));
                    effects.push(CapEffect::ScheduleTick { after_ms: 500 });
                }
            }
            CapInput::Tick => {
                if let Some(due) = self.due {
                    if self.write.is_some() {
                        self.due = None;
                        self.changed_during_write = true;
                    } else if now_ms < due {
                        effects.push(CapEffect::ScheduleTick {
                            after_ms: due - now_ms,
                        });
                    } else {
                        self.due = None;
                        if self.slots[0].generation > 0 {
                            self.load(CapTab::Skills, &mut effects);
                        }
                    }
                }
            }
            CapInput::Loaded {
                tab,
                generation,
                result,
            } => {
                if let Some(write) = &self.write
                    && write.reading
                    && write.kind.tab() == tab
                    && write.read_generation == Some(generation)
                {
                    let id = write.id;
                    self.read_back(id, result, &mut effects);
                    return effects;
                }
                if self.slots[index(tab)].generation == generation {
                    let name = self.selected_name();
                    let slot = &mut self.slots[index(tab)];
                    slot.loading = false;
                    match result {
                        Ok(mut data) => {
                            if let CapData::Hub(page) = &mut data {
                                page.query = self.query.clone();
                            }
                            slot.data = Some(data);
                            slot.failure = None;
                        }
                        Err(error) => {
                            slot.failure = Some(error_text(&error, tab, &self.profile, "", false))
                        }
                    }
                    self.restore_selection(name);
                }
            }
            CapInput::ReadBack { op_id, result } => self.read_back(op_id, result, &mut effects),
            CapInput::WriteDone { op_id, result } => self.write_done(op_id, result, &mut effects),
            CapInput::DetailLoaded {
                tab,
                generation,
                result,
            } => {
                if tab == self.tab
                    && generation == self.slots[index(tab)].detail_generation
                    && self.detail_focus
                {
                    self.detail = match result {
                        Ok(text) => self.document(&text),
                        Err(e) => vec![error_text(
                            &e,
                            tab,
                            &self.profile,
                            &self.selected_name().unwrap_or_default(),
                            true,
                        )],
                    };
                }
            }
            CapInput::PreviewDone(result) => {
                if self.tab == CapTab::Hub && self.detail_focus {
                    self.detail = match result {
                        Ok(t) => self.document(&t),
                        Err(e) => vec![error_text(
                            &e,
                            CapTab::Hub,
                            &self.profile,
                            &self
                                .hub_item()
                                .map(|item| item.install_key)
                                .unwrap_or_default(),
                            true,
                        )],
                    };
                }
            }
            CapInput::ScanDone(result) => self.scan_done(result, &mut effects),
            CapInput::LookupDone(result) => self.lookup_done(result, &mut effects),
            CapInput::CatalogLoaded { generation, result } => {
                if self.catalog && generation == self.catalog_generation {
                    match result {
                        Ok(entries) => {
                            self.catalog_entries = entries;
                            self.slots[3].selected = 0;
                        }
                        Err(e) => {
                            self.catalog = false;
                            let text = if matches!(
                                e,
                                CapError::Unauthorized | CapError::NotFound { .. }
                            ) {
                                self.text(CapText::CatalogListUnavailable)
                            } else {
                                error_text(&e, CapTab::Plugins, &self.profile, "", false)
                            };
                            self.notice(text, &mut effects);
                        }
                    }
                }
            }
            CapInput::ReloadDone(result) => {
                if let Some(name) = self.reload_name.take() {
                    let mut args = self.args();
                    args.name = name;
                    args.session_line = Some(match result {
                        Ok(t) => Ok(t
                            .lines()
                            .rev()
                            .find(|line| !line.trim().is_empty())
                            .unwrap_or("")
                            .to_owned()),
                        Err(e) => Err(error_detail(&e)),
                    });
                    self.notice(self.formatted(CapText::InstalledSkill, &args), &mut effects);
                    if !self.open && self.write.is_none() {
                        effects.push(CapEffect::Dispose);
                    }
                }
            }
            CapInput::Char(c) => {
                if !c.is_control() {
                    let selected = self.selected_name();
                    if let Some(input) = &mut self.input {
                        input.text.push(c);
                        self.restore_selection(selected);
                    } else if self.dialog.is_some() {
                        self.key(
                            if c == 'y' {
                                KeyAction::Confirm
                            } else {
                                KeyAction::Cancel
                            },
                            &mut effects,
                        );
                    }
                }
            }
            CapInput::Backspace => {
                let selected = self.selected_name();
                if let Some(input) = &mut self.input {
                    input.text.pop();
                    self.restore_selection(selected);
                } else if self.dialog.is_some() {
                    self.dialog = None;
                    self.message.clear();
                }
            }
            CapInput::Key(key) => self.key(key, &mut effects),
        }
        effects
    }

    fn write_done(
        &mut self,
        id: u64,
        result: Result<WriteReceipt, CapError>,
        effects: &mut Vec<CapEffect>,
    ) {
        let Some(write) = &self.write else { return };
        if write.id != id || write.reading {
            return;
        }
        let kind = write.kind.clone();
        match result {
            Ok(receipt) => {
                if matches!(kind, WriteKind::Plugin { .. }) && receipt.unchanged {
                    let mut a = self.args();
                    a.name = kind.name().into();
                    if let WriteKind::Plugin { enabled, .. } = kind {
                        a.enabled = !enabled;
                    }
                    self.notice(self.formatted(CapText::UnchangedPlugin, &a), effects);
                    self.finish(effects);
                    return;
                }
                if let Some(write) = &mut self.write {
                    write.reading = true;
                    write.receipt = receipt.clone();
                }
                self.slots[index(kind.tab())].generation += 1;
                if matches!(kind, WriteKind::Remove(_)) {
                    let mut args = self.args();
                    args.name = kind.name().into();
                    args.cleared_memory_provider = receipt.cleared_memory_provider;
                    self.notice(self.formatted(CapText::RemovedPlugin, &args), effects);
                    self.load(CapTab::Plugins, effects);
                    if let Some(write) = &mut self.write {
                        write.read_generation = Some(self.slots[3].generation);
                        write.announced = true;
                    }
                    return;
                }
                if matches!(kind, WriteKind::InstallPlugin(_)) {
                    let mut args = self.args();
                    args.name = receipt
                        .plugin_name
                        .clone()
                        .unwrap_or_else(|| kind.name().into());
                    args.enabled = receipt.enabled;
                    args.activation = receipt.activation.clone();
                    args.restart_required = receipt.restart_required;
                    args.warnings = receipt.warnings.clone();
                    args.missing_env = receipt.missing_env.clone();
                    args.known_issues = receipt.known_issues.clone();
                    self.notice(self.formatted(CapText::InstalledPlugin, &args), effects);
                    if let Some(write) = &mut self.write {
                        write.announced = true;
                    }
                }
                if let Some(plugin) = receipt.plugin
                    && matches!(kind, WriteKind::Plugin { .. })
                {
                    let mut rows = match &self.slots[3].data {
                        Some(CapData::Plugins(rows)) => rows.clone(),
                        _ => Vec::new(),
                    };
                    rows.retain(|p| p.key != plugin.key);
                    rows.push(plugin);
                    self.read_back(id, Ok(CapData::Plugins(rows)), effects);
                    return;
                }
                effects.push(CapEffect::ReadBack {
                    profile: self.profile.clone(),
                    tab: kind.tab(),
                    op_id: id,
                });
            }
            Err(error) if unknown(&error) => {
                if let Some(write) = &mut self.write {
                    write.reading = true;
                }
                self.slots[index(kind.tab())].generation += 1;
                self.notice(self.text(CapText::LostAnswer), effects);
                effects.push(CapEffect::ReadBack {
                    profile: self.profile.clone(),
                    tab: kind.tab(),
                    op_id: id,
                });
            }
            Err(error) => {
                let mut a = self.args();
                a.name = kind.name().into();
                a.message = error_detail(&error);
                let text = if kind.install()
                    && matches!(
                        error,
                        CapError::Rpc {
                            code: 5024 | 5026 | 5031,
                            ..
                        }
                    ) {
                    self.formatted(CapText::InstallFailed, &a)
                } else {
                    error_text(&error, kind.tab(), &self.profile, kind.name(), false)
                };
                if let CapError::Rpc { log, .. } = &error
                    && matches!(kind, WriteKind::InstallSkill(_))
                {
                    self.detail = log
                        .as_ref()
                        .map(|s| self.document(s))
                        .unwrap_or_else(|| vec![self.text(CapText::EmptyLog)]);
                    self.detail_focus = true;
                }
                self.notice(text, effects);
                self.finish(effects);
            }
        }
    }

    fn read_back(
        &mut self,
        id: u64,
        result: Result<CapData, CapError>,
        effects: &mut Vec<CapEffect>,
    ) {
        let Some(write) = &self.write else { return };
        if write.id != id || !write.reading {
            return;
        }
        let kind = write.kind.clone();
        let receipt = write.receipt.clone();
        let announced = write.announced;
        let mut a = self.args();
        a.name = kind.name().into();
        a.activation = receipt.activation.clone();
        a.restart_required = receipt.restart_required;
        let data = match result {
            Ok(data) => data,
            Err(_) => {
                self.notice(self.formatted(CapText::ReadBackFailed, &a), effects);
                self.finish(effects);
                return;
            }
        };
        let tab = kind.tab();
        let text = match (&kind, &data) {
            (WriteKind::Skill { name, enabled }, CapData::Skills(rows)) => toggle_text(
                self,
                rows.iter().find(|r| r.name == *name).map(|r| r.enabled),
                *enabled,
                CapText::SkillChanged,
                &mut a,
            ),
            (
                WriteKind::Toolset {
                    name,
                    label,
                    enabled,
                },
                CapData::Toolsets(rows),
            ) => {
                a.label = label.clone();
                let mut text = toggle_text(
                    self,
                    rows.iter().find(|r| r.name == *name).map(|r| r.enabled),
                    *enabled,
                    CapText::ToolsetChanged,
                    &mut a,
                );
                if let Some(key) = &receipt.post_setup_started {
                    a.key = key.clone();
                    text.push('\n');
                    text.push_str(&self.formatted(CapText::PostSetup, &a));
                }
                text
            }
            (WriteKind::Plugin { key, enabled, .. }, CapData::Plugins(rows)) => toggle_text(
                self,
                rows.iter()
                    .find(|r| r.key.as_deref() == Some(key))
                    .and_then(|r| r.status.enabled()),
                *enabled,
                CapText::PluginChanged,
                &mut a,
            ),
            (WriteKind::InstallSkill(item), CapData::Skills(rows)) => {
                let tail = item
                    .install_key
                    .rsplit('/')
                    .next()
                    .unwrap_or(&item.install_key);
                if let Some(row) = rows.iter().find(|r| r.name == item.name || r.name == tail) {
                    a.name = row.name.clone();
                    if let Some(session) = &self.foreground
                        && session.profile == self.profile
                    {
                        self.reload_name = Some(row.name.clone());
                        effects.push(CapEffect::ReloadSkills {
                            session_id: session.session_id.clone(),
                        });
                    }
                    self.formatted(CapText::InstalledSkill, &a)
                } else {
                    self.formatted(CapText::MissingInstall, &a)
                }
            }
            (WriteKind::InstallPlugin(name), CapData::Plugins(rows)) => {
                a.name = receipt.plugin_name.clone().unwrap_or_else(|| name.clone());
                if rows.iter().any(|r| r.name == a.name) {
                    a.enabled = receipt.enabled;
                    a.warnings = receipt.warnings.clone();
                    a.missing_env = receipt.missing_env.clone();
                    a.known_issues = receipt.known_issues.clone();
                    self.formatted(CapText::InstalledPlugin, &a)
                } else {
                    self.formatted(CapText::MissingInstall, &a)
                }
            }
            (WriteKind::Remove(name), CapData::Plugins(rows)) => {
                if rows.iter().all(|r| r.name != *name) {
                    a.cleared_memory_provider = receipt.cleared_memory_provider;
                    self.formatted(CapText::RemovedPlugin, &a)
                } else {
                    a.enabled = rows
                        .iter()
                        .find(|r| r.name == *name)
                        .and_then(|r| r.status.enabled())
                        .unwrap_or(false);
                    self.formatted(CapText::Mismatch, &a)
                }
            }
            _ => self.formatted(CapText::ReadBackFailed, &a),
        };
        let name = self.selected_name();
        self.slots[index(tab)].data = Some(data);
        self.slots[index(tab)].failure = None;
        self.slots[index(tab)].loading = false;
        self.restore_selection(name);
        if !announced || text.starts_with("Hermes ") {
            self.notice(text, effects);
        }
        self.finish(effects);
    }

    fn scan_done(&mut self, result: Result<ScanReport, CapError>, effects: &mut Vec<CapEffect>) {
        let Some(item) = self.scan.take() else { return };
        if !self.open || self.tab != CapTab::Hub {
            return;
        }
        match result {
            Ok(mut report) => {
                let mut a = self.args();
                a.name = item.name.clone();
                a.source = report.source;
                a.trust = report.trust;
                a.verdict = report.verdict;
                a.reason = report.policy_reason;
                a.counts = report.counts;
                let allowed = report.policy == ScanPolicy::Allow;
                self.message = vec![self.formatted(
                    if allowed {
                        CapText::ConfirmSkill
                    } else {
                        CapText::DeniedSkill
                    },
                    &a,
                )];
                report.findings.sort_by_key(|f| match f.severity.as_str() {
                    "critical" => 0,
                    "high" => 1,
                    "medium" => 2,
                    "low" => 3,
                    _ => 4,
                });
                let findings: Vec<_> = report
                    .findings
                    .iter()
                    .take(5)
                    .map(|f| {
                        format!(
                            "{} {} {}:{} {}",
                            self.display(&f.severity),
                            self.display(&f.category),
                            self.display(&f.file),
                            f.line,
                            self.display(&f.description)
                        )
                    })
                    .collect();
                self.message.extend(findings);
                if report.findings.len() > 5 {
                    a.number = report.findings.len() - 5;
                    self.message.push(self.formatted(CapText::FindingsTail, &a));
                }
                if allowed {
                    self.dialog = Some(Dialog::Skill(item));
                }
            }
            Err(e) => {
                let text = error_text(&e, CapTab::Hub, &self.profile, &item.install_key, true);
                self.notice(text, effects);
            }
        }
    }
    fn lookup_done(
        &mut self,
        result: Result<PluginLookup, CapError>,
        effects: &mut Vec<CapEffect>,
    ) {
        let Some(name) = self.lookup.take() else {
            return;
        };
        if !self.open || self.tab != CapTab::Plugins {
            return;
        }
        let mut a = self.args();
        a.name = name.clone();
        match result {
            Ok(PluginLookup::Found {
                name: found,
                repository,
                sha,
            }) => {
                a.name = found.clone();
                a.repository = repository;
                a.sha = sha;
                self.input = None;
                self.message = vec![self.formatted(CapText::ConfirmCatalog, &a)];
                self.dialog = Some(Dialog::Plugin {
                    target: InstallTarget::CatalogName(name),
                    name: found,
                });
            }
            Ok(other) => self.notice(
                self.formatted(
                    match other {
                        PluginLookup::Unknown => CapText::UnknownPlugin,
                        PluginLookup::InvalidName => CapText::InvalidInput,
                        _ => CapText::CatalogUnavailable,
                    },
                    &a,
                ),
                effects,
            ),
            Err(e) => self.notice(
                error_text(&e, CapTab::Plugins, &self.profile, &name, false),
                effects,
            ),
        }
    }

    fn key(&mut self, key: KeyAction, effects: &mut Vec<CapEffect>) {
        if let Some(dialog) = self.dialog.take() {
            self.message.clear();
            if key == KeyAction::Confirm {
                if self.busy(effects) {
                    return;
                }
                match dialog {
                    Dialog::Skill(item) => {
                        let mut a = self.args();
                        a.name = item.name.clone();
                        self.message = vec![self.formatted(CapText::InstallingSkill, &a)];
                        self.message_persistent = true;
                        let install_key = item.install_key.clone();
                        self.begin(
                            WriteKind::InstallSkill(item),
                            |op_id, profile| CapEffect::InstallSkill {
                                profile,
                                op_id,
                                install_key,
                            },
                            effects,
                        );
                    }
                    Dialog::Plugin { target, name } => self.begin(
                        WriteKind::InstallPlugin(name),
                        |op_id, profile| CapEffect::InstallPlugin {
                            profile,
                            op_id,
                            target,
                        },
                        effects,
                    ),
                    Dialog::Remove(name) => {
                        let send_name = name.clone();
                        self.begin(
                            WriteKind::Remove(name),
                            |op_id, profile| CapEffect::RemovePlugin {
                                profile,
                                op_id,
                                name: send_name,
                            },
                            effects,
                        );
                    }
                }
            }
            return;
        }
        if self.input.is_some() {
            match key {
                KeyAction::Enter => self.submit(effects),
                KeyAction::Cancel => {
                    if let Some(input) = self.input.take() {
                        if matches!(input.kind, InputKind::Filter) {
                            self.slots[index(self.tab)].filter = input.previous;
                        }
                        self.clamp_selection();
                    }
                }
                _ => {}
            }
            return;
        }
        if !self.message_persistent {
            self.message.clear();
        }
        let slot = &self.slots[index(self.tab)];
        if !self.catalog
            && (slot.loading || slot.failure.is_some())
            && matches!(
                key,
                KeyAction::Toggle | KeyAction::Install | KeyAction::Remove | KeyAction::Enter
            )
        {
            if key != KeyAction::Enter {
                self.busy(effects);
            }
            return;
        }
        match key {
            KeyAction::Cancel => {
                if self.detail_focus {
                    self.detail_focus = false;
                    self.detail.clear();
                    self.detail_scroll = 0;
                } else if self.catalog {
                    self.catalog = false;
                    self.clamp_selection();
                } else if !self.slots[index(self.tab)].filter.is_empty() {
                    self.slots[index(self.tab)].filter.clear();
                    self.clamp_selection();
                } else if self.tab == CapTab::Hub && self.query.is_some() {
                    self.query = None;
                    self.page = 1;
                    self.load(CapTab::Hub, effects);
                } else {
                    self.open = false;
                    if self.write.is_none() && self.reload_name.is_none() {
                        effects.push(CapEffect::Dispose);
                    }
                }
            }
            KeyAction::NextTab => self.visit(tab_at(index(self.tab) + 1), effects),
            KeyAction::PrevTab => self.visit(tab_at(index(self.tab) + 3), effects),
            KeyAction::SelectTab(tab) => self.visit(tab, effects),
            KeyAction::Refresh => {
                if self.catalog {
                    self.fetch_catalog(effects);
                } else {
                    self.load(self.tab, effects);
                }
            }
            KeyAction::Up
            | KeyAction::Down
            | KeyAction::PageUp
            | KeyAction::PageDown
            | KeyAction::Home
            | KeyAction::End => self.move_selection(key),
            KeyAction::Enter => self.enter(effects),
            KeyAction::Filter => {
                let kind = if self.tab == CapTab::Hub {
                    InputKind::Search
                } else {
                    InputKind::Filter
                };
                let previous = self.slots[index(self.tab)].filter.clone();
                let text = if matches!(kind, InputKind::Filter) {
                    previous.clone()
                } else {
                    String::new()
                };
                self.input = Some(TextInput {
                    kind,
                    text,
                    previous,
                });
            }
            KeyAction::ShowAll if self.tab == CapTab::Plugins => {
                let selected = self.selected_name();
                self.show_all = !self.show_all;
                self.restore_selection(selected);
            }
            KeyAction::Catalog if self.tab == CapTab::Plugins => {
                self.catalog = !self.catalog;
                if self.catalog {
                    self.fetch_catalog(effects);
                } else {
                    self.clamp_selection();
                }
            }
            KeyAction::NextPage | KeyAction::PrevPage
                if self.tab == CapTab::Hub && self.query.is_none() =>
            {
                let total = match &self.slots[1].data {
                    Some(CapData::Hub(p)) => p.total_pages,
                    _ => 1,
                };
                let next = if key == KeyAction::NextPage {
                    self.page.saturating_add(1).min(total.max(1))
                } else {
                    self.page.saturating_sub(1).max(1)
                };
                if next != self.page {
                    self.page = next;
                    self.load(CapTab::Hub, effects);
                }
            }
            KeyAction::Toggle => self.toggle(effects),
            KeyAction::Install => self.install(effects),
            KeyAction::Remove if self.tab == CapTab::Plugins => self.remove(effects),
            _ => {}
        }
    }

    fn submit(&mut self, effects: &mut Vec<CapEffect>) {
        let Some(input) = self.input.take() else {
            return;
        };
        match input.kind {
            InputKind::Filter => {
                self.slots[index(self.tab)].filter = input.text;
                self.clamp_selection();
            }
            InputKind::Search => {
                if input.text.is_empty() {
                    self.input = Some(input);
                } else {
                    self.query = Some(input.text);
                    self.load(CapTab::Hub, effects);
                }
            }
            InputKind::Install => match classify_install_input(&input.text) {
                Ok(InstallTarget::CatalogName(name)) => {
                    self.input = Some(input);
                    self.lookup = Some(name.clone());
                    effects.push(CapEffect::LookupPlugin {
                        profile: self.profile.clone(),
                        name,
                    });
                }
                Ok(target @ InstallTarget::Repository(_)) => {
                    let InstallTarget::Repository(address) = &target else {
                        return;
                    };
                    let mut a = self.args();
                    a.repository = address.clone();
                    let name = address.clone();
                    self.message = vec![self.formatted(CapText::ConfirmRepository, &a)];
                    self.dialog = Some(Dialog::Plugin { target, name });
                }
                Err(_) => {
                    self.input = Some(input);
                    self.notice(self.text(CapText::InvalidInput), effects);
                }
            },
        }
    }
    fn fetch_catalog(&mut self, effects: &mut Vec<CapEffect>) {
        self.catalog_generation += 1;
        effects.push(CapEffect::FetchCatalog {
            profile: self.profile.clone(),
            generation: self.catalog_generation,
        });
    }
    fn selected_name(&self) -> Option<String> {
        self.rows()
            .get(self.slots[index(self.tab)].selected)
            .filter(|r| !r.is_header)
            .map(|r| r.name.clone())
    }
    fn restore_selection(&mut self, name: Option<String>) {
        let rows = self.rows();
        let selected = &mut self.slots[index(self.tab)].selected;
        if let Some(i) = name.and_then(|n| rows.iter().position(|r| !r.is_header && r.name == n)) {
            *selected = i;
        } else {
            let max = rows.len().saturating_sub(1);
            *selected = (*selected).min(max);
            if rows.get(*selected).is_none_or(|r| r.is_header) {
                *selected = rows.iter().position(|r| !r.is_header).unwrap_or(0);
            }
        }
    }
    fn clamp_selection(&mut self) {
        let name = self.selected_name();
        self.restore_selection(name);
    }
    fn move_selection(&mut self, key: KeyAction) {
        if self.detail_focus {
            let last = self.detail.len().saturating_sub(1);
            self.detail_scroll = match key {
                KeyAction::Home => 0,
                KeyAction::End => last,
                KeyAction::Up => self.detail_scroll.saturating_sub(1),
                KeyAction::PageUp => self.detail_scroll.saturating_sub(10),
                KeyAction::Down => (self.detail_scroll + 1).min(last),
                _ => (self.detail_scroll + 10).min(last),
            };
            return;
        }
        let indexes: Vec<_> = self
            .rows()
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.is_header)
            .map(|(i, _)| i)
            .collect();
        if indexes.is_empty() {
            return;
        }
        let current = self.slots[index(self.tab)].selected;
        let position = indexes.iter().position(|i| *i == current).unwrap_or(0);
        let last = indexes.len() - 1;
        let next = match key {
            KeyAction::Home => 0,
            KeyAction::End => last,
            KeyAction::Up => position.saturating_sub(1),
            KeyAction::Down => (position + 1).min(last),
            KeyAction::PageUp => position.saturating_sub(10),
            _ => (position + 10).min(last),
        };
        self.slots[index(self.tab)].selected = indexes[next];
        self.detail.clear();
        self.detail_scroll = 0;
    }
    fn enter(&mut self, effects: &mut Vec<CapEffect>) {
        let Some(name) = self.selected_name() else {
            return;
        };
        if self.catalog {
            self.lookup = Some(name.clone());
            effects.push(CapEffect::LookupPlugin {
                profile: self.profile.clone(),
                name,
            });
            return;
        }
        self.detail_focus = true;
        self.detail_scroll = 0;
        if self.tab == CapTab::Plugins {
            self.detail = self.short_detail();
            return;
        }
        if self.tab == CapTab::Hub {
            if let Some(item) = self.hub_item() {
                effects.push(CapEffect::HubPreview {
                    profile: self.profile.clone(),
                    identifier: item.install_key,
                });
            }
            return;
        }
        let slot = &mut self.slots[index(self.tab)];
        slot.detail_generation += 1;
        effects.push(CapEffect::FetchDetail {
            profile: self.profile.clone(),
            tab: self.tab,
            key: name,
            generation: slot.detail_generation,
        });
    }
    fn hub_item(&self) -> Option<HubItem> {
        let name = self.selected_name()?;
        match &self.slots[1].data {
            Some(CapData::Hub(page)) => page.items.iter().find(|r| r.name == name).cloned(),
            _ => None,
        }
    }
    fn selected_plugin(&self) -> Option<PluginRow> {
        if self.catalog {
            return None;
        }
        let Some(CapData::Plugins(rows)) = &self.slots[3].data else {
            return None;
        };
        let filter = self
            .input
            .as_ref()
            .filter(|input| matches!(input.kind, InputKind::Filter))
            .map(|input| input.text.as_str())
            .unwrap_or(&self.slots[3].filter)
            .to_lowercase();
        let mut visible: Vec<_> = rows
            .iter()
            .filter(|row| {
                (self.show_all || row.source != PluginSource::Bundled)
                    && (filter.is_empty()
                        || row.name.to_lowercase().contains(&filter)
                        || row.description.to_lowercase().contains(&filter))
            })
            .collect();
        visible.sort_by(|a, b| a.name.cmp(&b.name));
        visible
            .get(self.slots[3].selected)
            .map(|row| (*row).clone())
    }
    fn install(&mut self, effects: &mut Vec<CapEffect>) {
        if self.busy(effects) {
            return;
        }
        match self.tab {
            CapTab::Hub => {
                if self.scan.is_none()
                    && let Some(item) = self.hub_item()
                {
                    effects.push(CapEffect::HubScan {
                        profile: self.profile.clone(),
                        identifier: item.install_key.clone(),
                    });
                    self.scan = Some(item);
                }
            }
            CapTab::Plugins => {
                self.input = Some(TextInput {
                    kind: InputKind::Install,
                    text: String::new(),
                    previous: String::new(),
                })
            }
            _ => {}
        }
    }
    fn toggle(&mut self, effects: &mut Vec<CapEffect>) {
        if self.busy(effects) {
            return;
        }
        let Some(name) = self.selected_name() else {
            return;
        };
        match &self.slots[index(self.tab)].data {
            Some(CapData::Skills(rows)) => {
                if let Some(row) = rows.iter().find(|r| r.name == name) {
                    let enabled = !row.enabled;
                    let send_name = name.clone();
                    self.begin(
                        WriteKind::Skill { name, enabled },
                        |op_id, profile| CapEffect::ToggleSkill {
                            profile,
                            op_id,
                            name: send_name,
                            enabled,
                        },
                        effects,
                    );
                }
            }
            Some(CapData::Toolsets(rows)) => {
                if let Some(row) = rows.iter().find(|r| r.name == name) {
                    let enabled = !row.enabled;
                    let label = row.label.clone();
                    let send_name = name.clone();
                    self.begin(
                        WriteKind::Toolset {
                            name,
                            label,
                            enabled,
                        },
                        |op_id, profile| CapEffect::ToggleToolset {
                            profile,
                            op_id,
                            name: send_name,
                            enabled,
                        },
                        effects,
                    );
                }
            }
            Some(CapData::Plugins(_)) => {
                if let Some(row) = self.selected_plugin() {
                    if let (Some(key), Some(enabled)) = (&row.key, row.status.enabled()) {
                        let key = key.clone();
                        let send_key = key.clone();
                        let enabled = !enabled;
                        self.begin(
                            WriteKind::Plugin { name, key, enabled },
                            |op_id, profile| CapEffect::TogglePlugin {
                                profile,
                                op_id,
                                key: send_key,
                                enable: enabled,
                            },
                            effects,
                        );
                    } else {
                        let mut a = self.args();
                        a.name = name;
                        self.notice(self.formatted(CapText::ReadOnly, &a), effects);
                    }
                }
            }
            _ => {}
        }
    }
    fn remove(&mut self, effects: &mut Vec<CapEffect>) {
        if self.busy(effects) {
            return;
        }
        let Some(name) = self.selected_name() else {
            return;
        };
        let Some(CapData::Plugins(rows)) = &self.slots[3].data else {
            return;
        };
        let Some(row) = self.selected_plugin() else {
            return;
        };
        let blocked = if row.source == PluginSource::Bundled {
            Some(CapText::BundledRemove)
        } else if row.source == PluginSource::Entrypoint {
            Some(CapText::PackageRemove)
        } else if rows
            .iter()
            .filter(|r| r.source != PluginSource::Bundled && r.name == name)
            .count()
            > 1
        {
            Some(CapText::AmbiguousRemove)
        } else {
            None
        };
        let mut a = self.args();
        a.name = name.clone();
        if let Some(kind) = blocked {
            self.notice(self.formatted(kind, &a), effects);
        } else {
            self.message = vec![self.formatted(CapText::ConfirmRemove, &a)];
            self.dialog = Some(Dialog::Remove(name));
        }
    }

    fn rows(&self) -> Vec<CapListRow> {
        if self.catalog {
            return self
                .catalog_entries
                .iter()
                .map(|r| CapListRow {
                    name: r.name.clone(),
                    label: r.title.clone(),
                    description: r.description.clone(),
                    trust: r.tier.clone(),
                    installed: r.installed,
                    ..CapListRow::default()
                })
                .collect();
        }
        let slot = &self.slots[index(self.tab)];
        let filter = match &self.input {
            Some(TextInput {
                kind: InputKind::Filter,
                text,
                ..
            }) => text.as_str(),
            _ => slot.filter.as_str(),
        }
        .to_lowercase();
        let matches = |name: &str, desc: &str, category: &str| {
            filter.is_empty()
                || name.to_lowercase().contains(&filter)
                || desc.to_lowercase().contains(&filter)
                || category.to_lowercase().contains(&filter)
        };
        let mut result = Vec::new();
        match &slot.data {
            Some(CapData::Skills(rows)) => {
                let mut sorted: Vec<_> = rows
                    .iter()
                    .filter(|r| {
                        matches(&r.name, &r.description, r.category.as_deref().unwrap_or(""))
                    })
                    .collect();
                sorted.sort_by(|a, b| {
                    a.category
                        .is_none()
                        .cmp(&b.category.is_none())
                        .then(a.category.cmp(&b.category))
                        .then(a.name.cmp(&b.name))
                });
                let mut previous = None;
                for r in sorted {
                    let category = r.category.clone().unwrap_or_else(|| "(no category)".into());
                    if previous.as_ref() != Some(&category) {
                        result.push(CapListRow {
                            name: category.clone(),
                            label: category.clone(),
                            is_header: true,
                            ..CapListRow::default()
                        });
                        previous = Some(category);
                    }
                    result.push(CapListRow {
                        name: r.name.clone(),
                        marker: box_mark(r.enabled).into(),
                        label: r.provenance.label().into(),
                        description: r.description.clone(),
                        category: r.category.clone(),
                        usage: r.usage,
                        ..CapListRow::default()
                    });
                }
            }
            Some(CapData::Hub(page)) => {
                for r in &page.items {
                    let installed = page.installed.contains(&r.install_key)
                        || match &self.slots[0].data {
                            Some(CapData::Skills(rows)) => rows
                                .iter()
                                .any(|s| s.provenance == Provenance::Hub && s.name == r.name),
                            _ => false,
                        };
                    result.push(CapListRow {
                        name: r.name.clone(),
                        description: r.description.clone(),
                        source: r.source.clone(),
                        trust: r.trust.clone().unwrap_or_else(|| "-".into()),
                        installed,
                        ..CapListRow::default()
                    });
                }
            }
            Some(CapData::Toolsets(rows)) => {
                for r in rows.iter().filter(|r| matches(&r.name, &r.description, "")) {
                    result.push(CapListRow {
                        name: r.name.clone(),
                        label: r.label.clone(),
                        marker: box_mark(r.enabled).into(),
                        description: r.description.clone(),
                        tool_count: r.tools.len(),
                        configured: r.configured,
                        platform: r.platform.clone(),
                        platform_label: r.platform_label.clone(),
                        ..CapListRow::default()
                    });
                }
            }
            Some(CapData::Plugins(rows)) => {
                for r in rows.iter().filter(|r| {
                    (self.show_all || r.source != PluginSource::Bundled)
                        && matches(&r.name, &r.description, "")
                }) {
                    result.push(CapListRow {
                        name: r.name.clone(),
                        marker: if self.write.as_ref().is_some_and(|write| matches!(&write.kind, WriteKind::Plugin {key,..} if r.key.as_deref() == Some(key.as_str()))) { "[…]" } else { match r.status {
                            PluginStatus::Enabled => "[x]",
                            PluginStatus::Disabled => "[-]",
                            PluginStatus::NotEnabled => "[ ]",
                            _ => "[?]",
                        } }
                        .into(),
                        version: r.version.clone(),
                        description: r.description.clone(),
                        source: r.source.label().into(),
                        label: r.source.label().into(),
                        server_count: r.servers.len(),
                        update_available: r.update_available,
                        ..CapListRow::default()
                    });
                }
            }
            None => {}
        }
        if let Some(write) = &self.write
            && write.kind.tab() == self.tab
            && !matches!(write.kind, WriteKind::Plugin { .. })
        {
            for r in &mut result {
                if !r.is_header && r.name == write.kind.name() {
                    r.marker = "[…]".into();
                }
            }
        }
        if !matches!(self.tab, CapTab::Skills | CapTab::Hub) {
            result.sort_by(|a, b| a.name.cmp(&b.name));
        }
        result
    }

    fn short_detail(&self) -> Vec<String> {
        let Some(name) = self.selected_name() else {
            return Vec::new();
        };
        match &self.slots[index(self.tab)].data {
            Some(CapData::Skills(rows)) => rows
                .iter()
                .find(|r| r.name == name)
                .map(|r| {
                    vec![
                        format!(
                            "{} · {} · {} · used {}",
                            self.display(&r.name),
                            self.display(r.category.as_deref().unwrap_or("(no category)")),
                            self.display(r.provenance.label()),
                            r.usage
                        ),
                        self.display(&r.description),
                    ]
                })
                .unwrap_or_default(),
            Some(CapData::Hub(page)) => page
                .items
                .iter()
                .find(|r| r.name == name)
                .map(|r| {
                    vec![
                        format!(
                            "{} · {} · {}",
                            self.display(&r.name),
                            self.display(&r.source),
                            self.display(r.trust.as_deref().unwrap_or("-"))
                        ),
                        self.display(&r.description),
                    ]
                })
                .unwrap_or_default(),
            Some(CapData::Toolsets(rows)) => rows
                .iter()
                .find(|r| r.name == name)
                .map(|r| {
                    vec![
                        self.display(&r.description),
                        r.tools
                            .iter()
                            .map(|s| self.display(s))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ]
                })
                .unwrap_or_default(),
            Some(CapData::Plugins(_)) => self
                .selected_plugin()
                .map(|r| {
                    let mut lines = vec![
                        self.display(&r.description),
                        format!(
                            "{} · {}",
                            self.display(&r.version),
                            self.display(r.source.label())
                        ),
                    ];
                    if let Some(tier) = &r.catalog_tier {
                        lines.push(self.display(tier));
                    }
                    if r.update_available {
                        lines.push("update available".into());
                        lines.push(format!(
                            "Update it with Hermes Desktop or `hermes plugins update {}`",
                            self.display(&r.name)
                        ));
                    }
                    for server in &r.servers {
                        lines.push(format!(
                            "{}: {}",
                            self.display(&server.name),
                            self.display(&server.sentence)
                        ));
                    }
                    for setting in &r.settings {
                        lines.push(format!(
                            "{}: {}",
                            self.display(&setting.label),
                            if setting.kind == "secret" {
                                if setting.has_value {
                                    "set".into()
                                } else {
                                    "not set".into()
                                }
                            } else {
                                self.display(&setting.display_value)
                            }
                        ));
                    }
                    lines
                })
                .unwrap_or_default(),
            None => Vec::new(),
        }
    }

    pub fn view(&self) -> CapView {
        let slot = &self.slots[index(self.tab)];
        let mut rows = self.rows();
        let mut args = self.args();
        if slot.loading && !self.catalog {
            rows = vec![CapListRow {
                label: self.text(CapText::Loading),
                is_header: true,
                ..CapListRow::default()
            }];
        } else if let Some(failure) = &slot.failure {
            rows = vec![CapListRow {
                label: failure.clone(),
                is_header: true,
                ..CapListRow::default()
            }];
        } else if rows.is_empty() && !self.catalog {
            let filter = self
                .input
                .as_ref()
                .filter(|i| matches!(i.kind, InputKind::Filter))
                .map(|i| i.text.as_str())
                .unwrap_or(&slot.filter);
            let kind = if !filter.is_empty() {
                args.query = filter.into();
                CapText::EmptyFilter
            } else {
                match self.tab {
                    CapTab::Skills => CapText::EmptySkills,
                    CapTab::Toolsets => CapText::EmptyToolsets,
                    CapTab::Plugins => {
                        args.number = match &slot.data {
                            Some(CapData::Plugins(p)) => p
                                .iter()
                                .filter(|r| r.source == PluginSource::Bundled)
                                .count(),
                            _ => 0,
                        };
                        CapText::EmptyPlugins
                    }
                    CapTab::Hub => {
                        if let Some(q) = &self.query {
                            args.query = q.clone();
                            CapText::NoSearchResults
                        } else {
                            CapText::EmptyHub
                        }
                    }
                }
            };
            rows.push(CapListRow {
                label: self.formatted(kind, &args),
                is_header: true,
                ..CapListRow::default()
            });
        }
        for row in &mut rows {
            row.name = self.display(&row.name);
            row.label = self.display(&row.label);
            row.description = self.display(&row.description);
            row.category = row.category.as_deref().map(|s| self.display(s));
            row.version = self.display(&row.version);
            row.platform = self.display(&row.platform);
            row.platform_label = self.display(&row.platform_label);
            row.trust = self.display(&row.trust);
            row.source = self.display(&row.source);
        }
        let tabs = (0..4)
            .map(|i| match &self.slots[i].data {
                Some(CapData::Skills(r)) => format!(
                    "Skills {}/{} on",
                    r.iter().filter(|r| r.enabled).count(),
                    r.len()
                ),
                Some(CapData::Toolsets(r)) => format!(
                    "Toolsets {}/{} on",
                    r.iter().filter(|r| r.enabled).count(),
                    r.len()
                ),
                Some(CapData::Plugins(r)) => format!(
                    "Plugins {} installed",
                    r.iter()
                        .filter(|r| r.source != PluginSource::Bundled)
                        .count()
                ),
                _ => match i {
                    0 => "Skills",
                    1 => "Skill hub",
                    2 => "Toolsets",
                    _ => "Plugins",
                }
                .into(),
            })
            .collect();
        let input = self
            .input
            .as_ref()
            .map(|i| {
                format!(
                    "{}{}",
                    match i.kind {
                        InputKind::Filter => "Filter: ",
                        InputKind::Search => "Search: ",
                        InputKind::Install => "Install: ",
                    },
                    self.display(&i.text)
                )
            })
            .or_else(|| {
                (!slot.filter.is_empty()).then(|| format!("Filter: {}", self.display(&slot.filter)))
            });
        let footer = if self.tab == CapTab::Hub {
            match &slot.data {
                Some(CapData::Hub(page)) => {
                    if let Some(q) = &self.query {
                        format!(
                            "Search: \"{}\" · {} results",
                            self.display(q),
                            page.items.len()
                        )
                    } else {
                        format!(
                            "Page {} of {} · {} skills",
                            page.page, page.total_pages, page.total
                        )
                    }
                }
                _ => String::new(),
            }
        } else {
            String::new()
        };
        let hints = if self.dialog.is_some() {
            "y confirm  any other key cancels"
        } else if self.input.is_some() {
            "Enter submit  Esc cancel"
        } else if self.detail_focus {
            match self.tab {
                CapTab::Skills | CapTab::Toolsets => "Up/Down scroll  Space on/off  Esc back",
                CapTab::Hub => "Up/Down scroll  i install  Esc back",
                CapTab::Plugins => "Up/Down scroll  Space on/off  x remove  Esc back",
            }
        } else {
            match self.tab {
                CapTab::Skills => {
                    "Space on/off  Enter read  / filter  Tab next tab  r refresh  Esc close"
                }
                CapTab::Hub if self.query.is_some() => {
                    "Enter preview  i install  / new search  Esc browse"
                }
                CapTab::Hub => "Enter preview  i install  / search  n/p page  r refresh  Esc close",
                CapTab::Toolsets => {
                    "Space on/off  Enter detail  / filter  Tab next tab  r refresh  Esc close"
                }
                CapTab::Plugins => {
                    "Space on/off  i install  c catalog  x remove  a all  r refresh  Esc close"
                }
            }
        }
        .into();
        let mut message = self.message.clone();
        if message.is_empty()
            && let Some(write) = &self.write
            && let WriteKind::InstallSkill(item) = &write.kind
            && !write.reading
        {
            args.name = item.name.clone();
            message.push(self.formatted(CapText::InstallingSkill, &args));
        }
        if message.is_empty() && rows.len() == 1 && rows[0].is_header {
            message = vec![rows[0].label.clone()];
        }
        if message.is_empty() && slot.loading && !self.catalog {
            message = vec![self.text(CapText::Loading)];
        } else if message.is_empty()
            && let Some(failure) = &slot.failure
        {
            message = vec![failure.clone()];
        }
        if message.is_empty()
            && let Some(CapData::Hub(page)) = &slot.data
            && !page.timed_out.is_empty()
        {
            args.sources = page.timed_out.clone();
            message.push(self.formatted(CapText::SlowSources, &args));
        }
        CapView {
            profile: self.display(&self.profile),
            tab: self.tab,
            title: format!("Capabilities · profile {}", self.display(&self.profile)),
            tabs,
            rows,
            selected: slot.selected,
            detail: if self.detail_focus {
                self.detail.clone()
            } else {
                self.short_detail()
            },
            detail_focus: self.detail_focus,
            detail_scroll: self.detail_scroll,
            message,
            input,
            text_focus: self.input.is_some(),
            filter: slot.filter.clone(),
            footer,
            hints,
            open: self.open,
            loading: slot.loading,
            catalog: self.catalog,
        }
    }
}

fn box_mark(enabled: bool) -> &'static str {
    if enabled { "[x]" } else { "[ ]" }
}
fn toggle_text(
    state: &CapabilitiesState,
    actual: Option<bool>,
    intended: bool,
    success: CapText,
    a: &mut TextArgs,
) -> String {
    match actual {
        Some(b) if b == intended => {
            a.enabled = b;
            state.formatted(success, a)
        }
        Some(b) => {
            a.enabled = b;
            state.formatted(CapText::Mismatch, a)
        }
        None => state.formatted(CapText::MissingChangedRow, a),
    }
}
fn unknown(error: &CapError) -> bool {
    matches!(
        error,
        CapError::Timeout { sent: true }
            | CapError::Dropped { sent: true }
            | CapError::TimedOut
            | CapError::Unknown
            | CapError::BadResult
            | CapError::BadBody
            | CapError::BodyTooLarge
    )
}
fn error_detail(error: &CapError) -> String {
    match error {
        CapError::Rpc {
            message, reason, ..
        } => {
            if let CapError::Rpc { code: 4001, .. } = error {
                reason.clone().unwrap_or_else(|| message.clone())
            } else {
                message.clone()
            }
        }
        CapError::Refused { status, detail } => {
            detail.clone().unwrap_or_else(|| format!("HTTP {status}"))
        }
        CapError::Server { status } => format!("HTTP {status}"),
        CapError::BadBody | CapError::BodyTooLarge | CapError::BadResult => {
            "unreadable reply".into()
        }
        _ => "request failed".into(),
    }
}
fn error_text(error: &CapError, tab: CapTab, profile: &str, name: &str, detail: bool) -> String {
    let mut a = TextArgs {
        profile: profile.into(),
        tab,
        name: name.into(),
        key: name.into(),
        message: error_detail(error),
        ..TextArgs::default()
    };
    let kind = match error {
        CapError::Connect
        | CapError::Ticket
        | CapError::NotLoopback
        | CapError::TlsNotSupported
        | CapError::Timeout { sent: false }
        | CapError::Dropped { sent: false }
        | CapError::NotConnected
        | CapError::Encode
        | CapError::InvalidTimeout => CapText::Unreachable,
        CapError::Timeout { sent: true }
        | CapError::Dropped { sent: true }
        | CapError::TimedOut
        | CapError::Unknown => {
            if tab == CapTab::Hub {
                CapText::HubUnavailable
            } else {
                CapText::ReadTimeout
            }
        }
        CapError::Unauthorized => CapText::Unauthorized,
        CapError::Forbidden { .. } | CapError::Rpc { code: 4064, .. } => {
            CapText::ProfileUnavailable
        }
        CapError::Rpc {
            code: 4001,
            reason: Some(reason),
            ..
        } if reason == "profile_mismatch" => CapText::ProfileUnavailable,
        CapError::NotFound { route } => {
            if tab == CapTab::Hub && detail {
                CapText::MissingHubItem
            } else if tab == CapTab::Skills && detail {
                CapText::MissingContent
            } else {
                a.key = route.clone();
                CapText::Unsupported
            }
        }
        CapError::Rpc { code: -32601, .. } => {
            a.key = if matches!(tab, CapTab::Hub | CapTab::Skills) {
                "skills.manage"
            } else {
                "plugins.manage"
            }
            .into();
            CapText::Unsupported
        }
        CapError::Server { .. } | CapError::Rpc { code: 5024, .. } if tab == CapTab::Hub => {
            CapText::HubUnavailable
        }
        _ => CapText::Refused,
    };
    format_text(kind, &a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_plugin_names_keep_the_selected_wire_key() {
        let mut state = CapabilitiesState::new("fixture".into(), None, CapTab::Plugins);
        state.update(CapInput::Open(CapTab::Plugins), 0);
        let data = CapData::from_json(
            CapTab::Plugins,
            &serde_json::json!({"plugins": [
                {"name":"same", "key":"first", "source":"user", "status":"enabled"},
                {"name":"same", "key":"second", "source":"git", "status":"disabled"}
            ]}),
        )
        .unwrap();
        state.update(
            CapInput::Loaded {
                tab: CapTab::Plugins,
                generation: 1,
                result: Ok(data),
            },
            1,
        );
        state.update(CapInput::Key(KeyAction::Down), 2);
        assert_eq!(
            state.update(CapInput::Key(KeyAction::Toggle), 3),
            vec![CapEffect::TogglePlugin {
                profile: "fixture".into(),
                op_id: 1,
                key: "second".into(),
                enable: true
            }]
        );
        let view = state.view();
        assert_eq!(view.rows[0].marker, "[x]");
        assert_eq!(view.rows[1].marker, "[…]");
    }

    #[test]
    fn display_context_keeps_wire_identity_and_hides_context_in_debug() {
        let mut state = CapabilitiesState::new("fixture".into(), None, CapTab::Skills);
        state.set_display_paths("/fixture/hermes".into(), "/fixture/user".into());
        state.update(CapInput::Open(CapTab::Skills), 0);
        assert!(state.update(CapInput::Key(KeyAction::Toggle), 0).is_empty());
        let name = "/fixture/user/guide\u{1b}";
        state.update(
            CapInput::Loaded {
                tab: CapTab::Skills,
                generation: 1,
                result: Ok(CapData::Skills(vec![SkillRow {
                    name: name.into(),
                    description: "/fixture/hermes/document".into(),
                    category: None,
                    enabled: true,
                    usage: 0,
                    provenance: Provenance::Local,
                }])),
            },
            1,
        );
        let view = state.view();
        let row = view.rows.iter().find(|row| !row.is_header).unwrap();
        assert_eq!(row.name, "~/guide␛");
        assert_eq!(row.description, "$HERMES_HOME/document");
        assert!(!format!("{state:?}").contains("/fixture/"));
        assert_eq!(
            state.update(CapInput::Key(KeyAction::Toggle), 2),
            vec![CapEffect::ToggleSkill {
                profile: "fixture".into(),
                op_id: 1,
                name: name.into(),
                enabled: false
            }]
        );
        let effects = state.update(
            CapInput::WriteDone {
                op_id: 1,
                result: Err(CapError::Rpc {
                    code: 5999,
                    message: "/fixture/hermes/rejected".into(),
                    reason: None,
                    log: None,
                }),
            },
            3,
        );
        assert_eq!(
            effects,
            vec![CapEffect::Notice(
                "Hermes refused this request: $HERMES_HOME/rejected".into()
            )]
        );
        state.update(CapInput::DismissMessage, 4);
        assert!(state.view().message.is_empty());
    }

    #[test]
    fn dismissal_preserves_confirmation_and_install_progress() {
        let mut state = CapabilitiesState::new("fixture".into(), None, CapTab::Hub);
        state.update(CapInput::Open(CapTab::Hub), 0);
        state.update(
            CapInput::Loaded {
                tab: CapTab::Hub,
                generation: 1,
                result: Ok(CapData::Hub(HubPage {
                    items: vec![HubItem {
                        name: "guide".into(),
                        description: String::new(),
                        source: "official".into(),
                        trust: None,
                        install_key: "official/guide".into(),
                    }],
                    page: 1,
                    total_pages: 1,
                    total: 1,
                    ..HubPage::default()
                })),
            },
            1,
        );
        state.update(CapInput::Key(KeyAction::Install), 2);
        state.update(
            CapInput::ScanDone(Ok(ScanReport {
                verdict: "clean".into(),
                policy: ScanPolicy::Allow,
                policy_reason: String::new(),
                source: "official".into(),
                trust: String::new(),
                counts: [0; 4],
                findings: Vec::new(),
            })),
            3,
        );
        let confirmation = state.view().message;
        state.update(CapInput::DismissMessage, 4);
        assert_eq!(state.view().message, confirmation);
        state.update(CapInput::Key(KeyAction::Confirm), 5);
        let progress = state.view().message;
        assert!(progress[0].starts_with("Installing guide..."));
        state.update(CapInput::DismissMessage, 6);
        state.update(CapInput::Key(KeyAction::Down), 7);
        assert_eq!(state.view().message, progress);
    }
}
