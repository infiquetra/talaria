//! The model roles page. Inputs in, effects out.

use serde_json::Value;

use super::{
    Assignment, AuxView, FallbackEntry, Loaded, MoaConfigView, PickerStep, RawRoles, Read,
    RolesEffect, RolesError, RolesInfo, RolesInput, RolesView, RouteCatalog, Section, StaleSlot,
    SubagentRoles,
};
use crate::models::Effort;

/// Rows a list shows before it scrolls.
pub(super) const PAGE: usize = 12;

pub(super) const SECTIONS: [Section; 5] = [
    Section::Main,
    Section::Fallbacks,
    Section::Helpers,
    Section::Subagents,
    Section::Moa,
];

pub(super) fn index_of(section: Section) -> usize {
    SECTIONS
        .iter()
        .position(|candidate| *candidate == section)
        .unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Link {
    Connected,
    NotConnected,
    NotServed(String),
}

/// One read: not yet answered, answered, or failed (keeping the last answer for display).
#[derive(Clone, Debug)]
pub(super) enum Load<T> {
    Pending,
    Ready(T),
    Failed { last: Option<T>, text: Vec<String> },
    Unavailable { last: Option<T>, text: String },
}

impl<T> Load<T> {
    pub(super) fn value(&self) -> Option<&T> {
        match self {
            Load::Ready(value) => Some(value),
            Load::Failed { last, .. } | Load::Unavailable { last, .. } => last.as_ref(),
            Load::Pending => None,
        }
    }

    pub(super) fn is_bad(&self) -> bool {
        matches!(self, Load::Failed { .. } | Load::Unavailable { .. })
    }

    pub(super) fn fail(&mut self, text: Vec<String>) {
        let last = std::mem::replace(self, Load::Pending).into_last();
        *self = Load::Failed { last, text };
    }

    pub(super) fn unavailable(&mut self, text: String) {
        let last = std::mem::replace(self, Load::Pending).into_last();
        *self = Load::Unavailable { last, text };
    }

    fn into_last(self) -> Option<T> {
        match self {
            Load::Ready(value) => Some(value),
            Load::Failed { last, .. } | Load::Unavailable { last, .. } => last,
            Load::Pending => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum CatalogState {
    Pending,
    Ready(RouteCatalog),
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ChosenRoute {
    pub provider: String,
    pub model: String,
    pub base_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum HelperRoute {
    Inherit,
    Pin(ChosenRoute),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HelperEdit {
    pub task: String,
    pub route: Option<HelperRoute>,
    /// `None`: untouched. `Some(None)`: inherit. `Some(Some(level))`: that level.
    pub effort: Option<Option<Effort>>,
}

/// Everything the user changed and has not applied. Only one section holds changes at a time.
#[derive(Clone, Debug, Default)]
pub(super) struct Edits {
    pub main_route: Option<ChosenRoute>,
    pub main_context: Option<u64>,
    pub main_effort: Option<Option<Effort>>,
    pub helpers: Vec<HelperEdit>,
    pub fallbacks: Option<Vec<FallbackEntry>>,
    pub subagents: Option<SubagentRoles>,
    pub moa: Option<MoaConfigView>,
}

/// What to do once the section that blocked it has nothing pending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Then {
    Nothing,
    Section(Section),
    Reload,
    Close,
}

#[derive(Clone, Debug)]
pub(super) enum Prompt {
    Pending { section: Section, then: Then },
    Confirm { message: String },
    StaleOffer(Vec<StaleSlot>),
    DeletePreset(usize),
}

#[derive(Clone, Debug)]
pub(super) struct ReviewMode {
    pub section: Section,
    pub then: Then,
    pub fresh: bool,
    pub go: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PickTarget {
    Main,
    Fallback(Option<usize>),
    Helper(String),
    SubRoute,
    SubChain(Option<usize>),
    MoaReference(usize),
    MoaAggregator,
}

#[derive(Clone, Debug)]
pub(super) struct PickerMode {
    pub target: PickTarget,
    pub step: PickerStep,
    pub filter: String,
    pub cursor: usize,
    pub refreshing: bool,
    /// Leaving the picker sets the subagent chain to "no fallback".
    pub escape_disables_chain: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InputTarget {
    Context,
    PresetName,
    FanoutEvery,
    Timeout,
    ReferenceTemperature,
    AggregatorTemperature,
}

#[derive(Clone, Debug)]
pub(super) struct InputMode {
    pub target: InputTarget,
    pub value: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ChoiceTarget {
    MainEffort,
    HelperEffort(String),
    SubagentEffort,
    ReferenceEffort(usize),
    AggregatorEffort,
    Fanout,
}

#[derive(Clone, Debug)]
pub(super) struct ChoiceMode {
    pub target: ChoiceTarget,
    pub options: Vec<String>,
    pub selected: usize,
}

#[derive(Clone, Debug)]
pub(super) enum Mode {
    Browse,
    Prompt(Prompt),
    Review(ReviewMode),
    Picker(PickerMode),
    Input(InputMode),
    Choice(ChoiceMode),
    Applying,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum JobStatus {
    Queued,
    Held,
    Saved,
    Unknown,
    Failed,
    Confirm,
}

#[derive(Clone, Debug)]
pub(super) struct Job {
    pub task: String,
    pub assignment: Assignment,
    pub status: JobStatus,
}

/// The apply in flight for one section.
#[derive(Clone, Debug)]
pub(super) struct Run {
    pub section: Section,
    pub then: Then,
    pub main_assignment: Option<Assignment>,
    pub model_saved: bool,
    pub saved_route: Option<(String, String)>,
    pub config_patch: Option<Value>,
    pub jobs: Vec<Job>,
    pub stale: Vec<StaleSlot>,
    pub failed: bool,
}

impl Run {
    pub(super) fn new(section: Section, then: Then) -> Self {
        Self {
            section,
            then,
            main_assignment: None,
            model_saved: false,
            saved_route: None,
            config_patch: None,
            jobs: Vec::new(),
            stale: Vec::new(),
            failed: false,
        }
    }
}

pub struct ModelRolesState {
    pub(super) profile: String,
    pub(super) link: Link,
    pub(super) section: Section,
    pub(super) info: Load<RolesInfo>,
    pub(super) aux: Load<AuxView>,
    pub(super) moa: Load<MoaConfigView>,
    pub(super) raw: Load<RawRoles>,
    pub(super) catalog: CatalogState,
    pub(super) cursor: [usize; 5],
    pub(super) scroll: [usize; 5],
    pub(super) detail_cursor: usize,
    pub(super) detail_scroll: usize,
    pub(super) focus_detail: bool,
    pub(super) edits: Edits,
    pub(super) mode: Mode,
    pub(super) notice: Vec<String>,
    pub(super) help: bool,
    pub(super) run: Option<Run>,
    pub(super) saved_default: Option<(String, String)>,
}

impl ModelRolesState {
    pub fn new(profile: String) -> (Self, Vec<RolesEffect>) {
        let state = Self {
            profile,
            link: Link::Connected,
            section: Section::Main,
            info: Load::Pending,
            aux: Load::Pending,
            moa: Load::Pending,
            raw: Load::Pending,
            catalog: CatalogState::Pending,
            cursor: [0; 5],
            scroll: [0; 5],
            detail_cursor: 0,
            detail_scroll: 0,
            focus_detail: false,
            edits: Edits::default(),
            mode: Mode::Browse,
            notice: Vec::new(),
            help: false,
            run: None,
            saved_default: None,
        };
        (state, super::flow::load_effects())
    }

    pub fn update(&mut self, input: RolesInput) -> Vec<RolesEffect> {
        let effects = match input {
            RolesInput::NotConnected => {
                self.link = Link::NotConnected;
                self.mode = Mode::Browse;
                Vec::new()
            }
            RolesInput::NotServed { profile } => {
                self.link = Link::NotServed(profile);
                self.mode = Mode::Browse;
                Vec::new()
            }
            RolesInput::Loaded(loaded) => {
                self.on_loaded(loaded);
                Vec::new()
            }
            RolesInput::LoadFailed { read, error } => {
                self.on_load_failed(read, &error);
                Vec::new()
            }
            RolesInput::CatalogLoaded(catalog) => {
                self.catalog = CatalogState::Ready(catalog);
                if let Mode::Picker(picker) = &mut self.mode {
                    picker.refreshing = false;
                }
                Vec::new()
            }
            RolesInput::SavedDefault(saved) => {
                self.saved_default = saved;
                Vec::new()
            }
            RolesInput::CatalogFailed { .. } => {
                self.catalog = CatalogState::Failed;
                if let Mode::Picker(picker) = &mut self.mode {
                    picker.refreshing = false;
                }
                Vec::new()
            }
            RolesInput::Key(key) => self.on_key(key),
            RolesInput::Fresh { section, reads } => self.on_fresh(section, &reads),
            RolesInput::FreshFailed { error, .. } => self.on_fresh_failed(&error),
            RolesInput::Applied { step, reply } => self.on_applied(step, reply),
            RolesInput::ApplyFailed { step, error } => self.on_apply_failed(&step, &error),
            RolesInput::ReadBack { section, reads } => self.on_read_back(section, &reads),
            RolesInput::ReadBackFailed { section, error } => {
                self.on_read_back_failed(section, &error)
            }
        };
        self.settle();
        effects
    }

    pub fn view(&self) -> RolesView {
        self.build_view()
    }

    fn on_loaded(&mut self, loaded: Loaded) {
        self.link = Link::Connected;
        match loaded {
            Loaded::Info(info) => self.info = Load::Ready(info),
            Loaded::Auxiliary(aux) => self.aux = Load::Ready(aux),
            Loaded::Moa(moa) => self.moa = Load::Ready(moa),
            Loaded::Raw(raw) => {
                if matches!(self.raw, Load::Pending) {
                    // Lists edited before the file was read would replace what the file holds.
                    self.edits.fallbacks = None;
                    self.edits.subagents = None;
                }
                self.raw = Load::Ready(raw);
            }
        }
    }

    fn on_load_failed(&mut self, read: Read, error: &RolesError) {
        let route = match read {
            Read::Info => "model information",
            Read::Auxiliary => "helper models",
            Read::Moa => "Mixture of Agents settings",
            Read::Raw => "the profile configuration",
        };
        let missing = match error {
            RolesError::NotFound { route } => Some(super::text::unavailable(route)),
            _ => None,
        };
        let lines = super::errors::read_lines(error, route);
        match read {
            Read::Info => set_bad(&mut self.info, missing, lines),
            Read::Auxiliary => set_bad(&mut self.aux, missing, lines),
            Read::Moa => set_bad(&mut self.moa, missing, lines),
            Read::Raw => set_bad(&mut self.raw, missing, lines),
        }
    }

    /// Keeps every cursor on a row that exists and can be selected.
    fn settle(&mut self) {
        if !matches!(self.link, Link::Connected) {
            return;
        }
        for section in SECTIONS {
            let index = index_of(section);
            let items = self.items(section);
            self.cursor[index] = super::items::settle_cursor(&items, self.cursor[index]);
            let len = items.len();
            super::items::fit(&mut self.scroll[index], self.cursor[index], len);
        }
        let detail = self.detail_items();
        self.detail_cursor = super::items::settle_cursor(&detail, self.detail_cursor);
        super::items::fit(&mut self.detail_scroll, self.detail_cursor, detail.len());
    }
}

fn set_bad<T>(load: &mut Load<T>, missing: Option<String>, lines: Vec<String>) {
    match missing {
        Some(text) => load.unavailable(text),
        None => load.fail(lines),
    }
}
