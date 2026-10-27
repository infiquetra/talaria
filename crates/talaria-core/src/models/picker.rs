//! Picker keys, rows and the effects a change asks the app to run. Issue 230.

use hermes_contract::schemas::ModelOptionsResult;
use serde_json::Value;

use super::effort::Effort;
use super::saved::SavedModels;
use crate::mutation::{MutationOutcome, MutationRequest};

/// Whether the change applies only here, or also to later Talaria sessions on this profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    ThisSession,
    ThisSessionAndNew,
}

/// A row the picker can select.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowKey {
    Favourite { provider: String, model: String },
    Model { provider: String, model: String },
    NotSetUp { provider: String },
}

/// The effort control drawn in the footer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffortControl {
    Hidden,
    Shown { current: Effort, chosen: Effort },
}

/// How loud a notice is. The app maps this onto the transcript notice styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

/// A key the picker handles while it is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerKey {
    Char(char),
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Left,
    Right,
    Enter,
    Backspace,
    ClearFilter,
    Escape,
    Close,
    ToggleScope,
    ToggleFavourite,
    Refresh,
}

/// A key the confirmation card handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmKey {
    Accept,
    Decline,
}

/// One painted picker row. Badges and the price are separate so narrow widths can drop them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerRow {
    pub key: Option<RowKey>,
    pub text: String,
    pub badges: Vec<String>,
    pub price: Option<String>,
    pub dim: bool,
    pub header: bool,
}

/// What the overlay draws. The list height is the caller's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickerView<'a> {
    pub title: &'a str,
    pub filter: &'a str,
    pub effort: &'a str,
    pub hints: &'a str,
    pub refreshing: bool,
    pub rows: &'a [PickerRow],
    pub selected: Option<usize>,
    pub banner: Option<&'a str>,
}

/// A `/model` command after the verb.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelCommand {
    OpenPicker,
    Switch {
        model: String,
        provider: Option<String>,
        effort: Option<Effort>,
        save_default: bool,
    },
}

/// A `/effort` command after the verb.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffortCommand {
    Show,
    Set {
        level: Option<Effort>,
        save_default: bool,
    },
}

/// Either local command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelCommandOrEffort {
    Model(ModelCommand),
    Effort(EffortCommand),
}

/// What the app should do after one picker step.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelsEffect {
    OpenOverlay,
    CloseOverlay,
    LoadOptions {
        session_id: String,
        refresh: bool,
    },
    ReadEffort {
        session_id: String,
    },
    Mutate(MutationRequest),
    SetEffort {
        session_id: String,
        level: Option<Effort>,
    },
    SaveFile(SavedModels),
    Notice {
        session_id: String,
        level: NoticeLevel,
        text: String,
    },
    /// Enter on a not-set-up row. The app closes the picker and opens providers.
    OpenProviders {
        /// Provider slug.
        slug: String,
    },
}

/// One fact the app feeds the picker.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelsInput {
    OpenPicker {
        session_id: Option<String>,
        profile: String,
    },
    Picker(PickerKey),
    Confirm(ConfirmKey),
    Command {
        session_id: Option<String>,
        profile: String,
        command: Result<ModelCommandOrEffort, String>,
    },
    SessionOpened {
        session_id: String,
        revision: Option<i64>,
        execution_generation: Option<i64>,
    },
    SessionEvent {
        session_id: String,
        type_name: String,
        payload: Value,
    },
    ForegroundChanged {
        session_id: Option<String>,
    },
    CardOpened,
    OptionsLoaded {
        session_id: String,
        result: ModelOptionsResult,
    },
    OptionsFailed {
        session_id: String,
        reason: String,
        unknown_method: bool,
    },
    EffortRead {
        session_id: String,
        value: Option<String>,
    },
    EffortReadFailed {
        session_id: String,
        reason: String,
        unknown_method: bool,
    },
    MutationReturned {
        request: MutationRequest,
        outcome: MutationOutcome,
    },
    EffortSetReturned {
        session_id: String,
        level: Option<Effort>,
        outcome: Result<(), MutationOutcome>,
    },
    FileSaved,
    FileFailed {
        reason: String,
    },
}
