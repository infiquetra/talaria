//! Pure per-session model choices and picker transitions.

use super::picker::*;
use super::saved::SavedModels;
use super::text::*;
use super::{Catalog, CatalogKind, DisplayZone, Effort};
use crate::mutation::{Confirmation, Fences, MutationLedger, MutationOutcome, MutationRequest};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

#[derive(Clone)]
struct Pick {
    operation: String,
    payload: Map<String, Value>,
    profile: String,
    save: bool,
    model: Option<(String, Option<String>)>,
    effort: Option<Option<Effort>>,
    confirmed: bool,
}

#[derive(Clone)]
struct Active {
    request: MutationRequest,
    pick: Pick,
    setting_effort: bool,
}

struct PendingConfirmation {
    confirmation: Confirmation,
    pick: Pick,
    from_event: bool,
    keep: String,
}

#[derive(Default)]
struct Session {
    profile: String,
    catalog: Option<Catalog>,
    model: Option<String>,
    effort: Option<Effort>,
    show_effort: bool,
    active: Option<Active>,
    unknown: Option<Active>,
    queued: Option<Pick>,
    confirmation: Option<PendingConfirmation>,
    deferred: Option<Option<String>>,
    pending_save: Option<Pick>,
}

struct Picker {
    session_id: String,
    profile: String,
    filter: String,
    scope: Scope,
    refreshing: bool,
    loading: bool,
    failed: Option<String>,
    current_effort: Option<Effort>,
    chosen_effort: Option<Effort>,
    rows: Vec<PickerRow>,
    selected: Option<usize>,
    title: String,
    effort_text: String,
    banner: Option<String>,
    feedback: Option<String>,
}

/// The picker, per-session requests, and saved choices for this launch.
pub struct ModelsState {
    ledger: MutationLedger,
    saved: Result<SavedModels, String>,
    saved_warned: bool,
    sessions: BTreeMap<String, Session>,
    foreground: Option<String>,
    picker: Option<Picker>,
    list_height: usize,
    display_zone: DisplayZone,
}

fn notice(session_id: &str, level: NoticeLevel, text: impl Into<String>) -> ModelsEffect {
    ModelsEffect::Notice {
        session_id: session_id.into(),
        level,
        text: text.into(),
    }
}
fn unknown_method(action: &str) -> String {
    format!(
        "This Hermes gateway cannot {action} (unknown method). Update Hermes to the pinned release."
    )
}
fn effort_value(level: Option<Effort>) -> Value {
    level.map_or(Value::Null, |e| json!(e.as_wire()))
}

impl ModelsState {
    pub fn new(launch_id: &str, saved: Result<SavedModels, String>) -> Self {
        Self {
            ledger: MutationLedger::new(launch_id),
            saved,
            saved_warned: false,
            sessions: BTreeMap::new(),
            foreground: None,
            picker: None,
            list_height: 10,
            display_zone: DisplayZone { minutes_east: 0 },
        }
    }
    pub fn confirm_keep(&self) -> Option<&str> {
        self.visible_confirmation()
            .map(|pending| pending.keep.as_str())
    }
    pub fn launch_id(&self) -> &str {
        self.ledger.launch_id()
    }
    pub fn saved_error(&self) -> Option<&str> {
        self.saved.as_ref().err().map(String::as_str)
    }
    /// Consume the startup warning once, including when no picker is opened.
    pub fn take_saved_warning(&mut self) -> Option<String> {
        if self.saved_warned {
            return None;
        }
        let reason = self.saved.as_ref().err()?;
        self.saved_warned = true;
        Some(
            if reason.starts_with("~/.talaria/models.json could not be read (") {
                reason.clone()
            } else {
                notice_t21(reason)
            },
        )
    }
    pub fn set_list_height(&mut self, rows: usize) {
        self.list_height = rows.max(3);
    }
    pub fn list_height(&self) -> usize {
        self.list_height
    }

    /// The application supplies its local offset; the reducer never reads a clock.
    pub fn set_display_zone(&mut self, zone: DisplayZone) {
        self.display_zone = zone;
        self.rebuild();
    }

    pub fn update(&mut self, input: ModelsInput) -> Vec<ModelsEffect> {
        let mut effects = Vec::new();
        match input {
            ModelsInput::SessionOpened {
                session_id,
                revision,
                execution_generation,
            } => {
                self.ledger
                    .fences_mut(&session_id)
                    .adopt(revision, execution_generation);
                self.sessions.entry(session_id).or_default();
            }
            ModelsInput::ForegroundChanged { session_id } => {
                if self.foreground != session_id {
                    self.close(&mut effects);
                }
                self.foreground = session_id;
            }
            ModelsInput::CardOpened => self.close(&mut effects),
            ModelsInput::OpenPicker {
                session_id,
                profile,
            } => self.open(session_id, profile, &mut effects),
            ModelsInput::Picker(key) => self.key(key, &mut effects),
            ModelsInput::Confirm(key) => self.confirm(key, &mut effects),
            ModelsInput::Command {
                session_id,
                profile,
                command,
            } => {
                let Some(sid) = session_id else {
                    return vec![notice("", NoticeLevel::Info, "No session is open yet.")];
                };
                self.warn_saved(&sid, &mut effects);
                self.sessions.entry(sid.clone()).or_default().profile = profile.clone();
                match command {
                    Err(text) => effects.push(notice(&sid, NoticeLevel::Info, text)),
                    Ok(ModelCommandOrEffort::Model(ModelCommand::OpenPicker)) => {
                        self.open(Some(sid), profile, &mut effects)
                    }
                    Ok(ModelCommandOrEffort::Model(ModelCommand::Switch {
                        model,
                        provider,
                        effort,
                        save_default,
                    })) => {
                        let mut payload = Map::new();
                        payload.insert("model".into(), json!(model));
                        if let Some(p) = &provider {
                            payload.insert("provider".into(), json!(p));
                        }
                        if let Some(e) = effort {
                            payload.insert("reasoning".into(), json!(e.as_wire()));
                        }
                        self.submit(
                            &sid,
                            Pick {
                                operation: "model".into(),
                                payload,
                                profile,
                                save: save_default,
                                model: Some((model, provider)),
                                effort: effort.map(Some),
                                confirmed: false,
                            },
                            &mut effects,
                        );
                    }
                    Ok(ModelCommandOrEffort::Effort(EffortCommand::Show)) => {
                        self.sessions.entry(sid.clone()).or_default().show_effort = true;
                        effects.push(ModelsEffect::ReadEffort { session_id: sid });
                    }
                    Ok(ModelCommandOrEffort::Effort(EffortCommand::Set {
                        level,
                        save_default,
                    })) => {
                        let mut payload = Map::new();
                        payload.insert("effort".into(), effort_value(level));
                        self.submit(
                            &sid,
                            Pick {
                                operation: "reasoning".into(),
                                payload,
                                profile,
                                save: save_default,
                                model: None,
                                effort: Some(level),
                                confirmed: false,
                            },
                            &mut effects,
                        );
                    }
                }
            }
            ModelsInput::OptionsLoaded { session_id, result } => {
                let session = self.sessions.entry(session_id.clone()).or_default();
                session.model = Some(result.model.clone());
                session.catalog = Some(Catalog::from_result(&result));
                if let Some(picker) = &mut self.picker
                    && picker.session_id == session_id
                {
                    picker.loading = false;
                    picker.refreshing = false;
                    picker.failed = None;
                }
                self.rebuild();
            }
            ModelsInput::OptionsFailed {
                session_id,
                reason,
                unknown_method: unknown,
            } => {
                if let Some(picker) = &mut self.picker
                    && picker.session_id == session_id
                {
                    picker.loading = false;
                    picker.refreshing = false;
                    picker.failed = Some(if unknown {
                        unknown_method("list models")
                    } else {
                        notice_t16(&reason)
                    });
                    if unknown {
                        effects.push(notice(
                            &session_id,
                            NoticeLevel::Error,
                            unknown_method("list models"),
                        ));
                    }
                }
                self.rebuild();
            }
            ModelsInput::EffortRead { session_id, value } => {
                let effort = value
                    .as_deref()
                    .and_then(Effort::parse)
                    .unwrap_or(Effort::Medium);
                let session = self.sessions.entry(session_id.clone()).or_default();
                session.effort = Some(effort);
                if std::mem::take(&mut session.show_effort) {
                    effects.push(notice(
                        &session_id,
                        NoticeLevel::Info,
                        format!("Effort for this session: {}.", effort.as_wire()),
                    ));
                }
                if let Some(picker) = &mut self.picker
                    && picker.session_id == session_id
                {
                    picker.current_effort = Some(effort);
                    picker.chosen_effort = Some(effort);
                }
                self.rebuild();
            }
            ModelsInput::EffortReadFailed {
                session_id,
                reason,
                unknown_method: unknown,
            } => {
                let session = self.sessions.entry(session_id.clone()).or_default();
                session.effort = None;
                if std::mem::take(&mut session.show_effort) {
                    effects.push(notice(
                        &session_id,
                        NoticeLevel::Error,
                        if unknown {
                            unknown_method("read the effort")
                        } else {
                            format!("Could not read the effort: {reason}.")
                        },
                    ));
                } else if unknown {
                    effects.push(notice(
                        &session_id,
                        NoticeLevel::Error,
                        unknown_method("read the effort"),
                    ));
                }
                if let Some(picker) = &mut self.picker
                    && picker.session_id == session_id
                {
                    picker.current_effort = None;
                    picker.chosen_effort = None;
                }
                self.rebuild();
            }
            ModelsInput::MutationReturned { request, outcome } => {
                self.returned(request, outcome, &mut effects)
            }
            ModelsInput::EffortSetReturned {
                session_id,
                level,
                outcome,
            } => self.effort_returned(&session_id, level, outcome, &mut effects),
            ModelsInput::SessionEvent {
                session_id,
                type_name,
                payload,
            } => self.event(&session_id, &type_name, &payload, &mut effects),
            ModelsInput::FileSaved => {}
            ModelsInput::FileFailed { reason } => effects.push(notice(
                self.foreground.as_deref().unwrap_or(""),
                NoticeLevel::Error,
                format!("Could not save model choices: {reason}."),
            )),
        }
        effects
    }

    fn warn_saved(&mut self, sid: &str, effects: &mut Vec<ModelsEffect>) {
        if let Some(text) = self.take_saved_warning() {
            effects.push(notice(sid, NoticeLevel::Warning, text));
        }
    }
    fn close(&mut self, effects: &mut Vec<ModelsEffect>) {
        if self.picker.take().is_some() {
            effects.push(ModelsEffect::CloseOverlay);
        }
    }
    fn open(
        &mut self,
        session_id: Option<String>,
        profile: String,
        effects: &mut Vec<ModelsEffect>,
    ) {
        let Some(sid) = session_id else {
            effects.push(notice("", NoticeLevel::Info, "No session is open yet."));
            return;
        };
        if self.picker.as_ref().is_some_and(|p| p.session_id == sid) {
            return;
        }
        self.warn_saved(&sid, effects);
        let session = self.sessions.entry(sid.clone()).or_default();
        session.profile = profile.clone();
        let cached = session.catalog.is_some();
        self.picker = Some(Picker {
            session_id: sid.clone(),
            profile,
            filter: String::new(),
            scope: Scope::ThisSession,
            refreshing: cached,
            loading: !cached,
            failed: None,
            current_effort: session.effort,
            chosen_effort: session.effort,
            rows: Vec::new(),
            selected: None,
            title: String::new(),
            effort_text: String::new(),
            banner: None,
            feedback: None,
        });
        self.rebuild();
        effects.extend([
            ModelsEffect::OpenOverlay,
            ModelsEffect::LoadOptions {
                session_id: sid.clone(),
                refresh: false,
            },
            ModelsEffect::ReadEffort { session_id: sid },
        ]);
    }

    fn rebuild(&mut self) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        let previous = picker
            .selected
            .and_then(|i| picker.rows.get(i))
            .and_then(|r| r.key.clone());
        let catalog = self
            .sessions
            .get(&picker.session_id)
            .and_then(|s| s.catalog.as_ref());
        let favourites = self
            .saved
            .as_ref()
            .map_or(&[][..], |s| s.favourites.as_slice());
        picker.title = match picker.scope {
            Scope::ThisSession => "Switch model · this session".into(),
            Scope::ThisSessionAndNew => format!(
                "Switch model · this session and new sessions on {}",
                picker.profile
            ),
        };
        picker.banner = picker.feedback.clone().or_else(|| picker.failed.clone());
        picker.rows.clear();
        let mut current_index = None;
        if let Some(catalog) = catalog {
            let terms: Vec<_> = picker
                .filter
                .split_whitespace()
                .map(str::to_lowercase)
                .collect();
            let mut header = None;
            for row in catalog.rows(favourites, self.display_zone) {
                if row.kind == CatalogKind::Header {
                    header = Some(PickerRow {
                        key: None,
                        text: std::iter::once(row.text)
                            .chain(row.segments)
                            .collect::<Vec<_>>()
                            .join(" · "),
                        badges: Vec::new(),
                        price: None,
                        dim: false,
                        header: true,
                    });
                    continue;
                }
                let text = format!("{} {} {}", row.provider_name, row.provider_slug, row.model)
                    .to_lowercase();
                if !terms.iter().all(|term| text.contains(term)) {
                    continue;
                }
                if let Some(header) = header.take() {
                    picker.rows.push(header);
                }
                let key = match row.kind {
                    CatalogKind::Favourite => RowKey::Favourite {
                        provider: row.provider_slug,
                        model: row.model,
                    },
                    CatalogKind::Model => RowKey::Model {
                        provider: row.provider_slug,
                        model: row.model,
                    },
                    CatalogKind::NotSetUp => RowKey::NotSetUp {
                        provider: row.provider_slug,
                    },
                    CatalogKind::Header => continue,
                };
                if row.current && row.kind == CatalogKind::Model {
                    current_index = Some(picker.rows.len());
                }
                picker.rows.push(PickerRow {
                    key: Some(key),
                    text: row.text,
                    badges: row.badges,
                    price: row.price,
                    dim: row.dim,
                    header: false,
                });
            }
            if picker.rows.is_empty() && picker.failed.is_none() {
                picker.banner = Some(if picker.filter.is_empty() {
                    "No models found. Set up a provider with hermes model, then press ctrl+r."
                        .into()
                } else {
                    notice_t18(&picker.filter)
                });
            }
        } else if picker.loading {
            picker.rows.push(PickerRow {
                key: None,
                text: "Loading models…".into(),
                badges: Vec::new(),
                price: None,
                dim: true,
                header: false,
            });
        }
        picker.selected = previous
            .as_ref()
            .and_then(|key| {
                picker
                    .rows
                    .iter()
                    .position(|row| row.key.as_ref() == Some(key))
            })
            .or(current_index.filter(|_| previous.is_none()))
            .or_else(|| picker.rows.iter().position(|r| r.key.is_some()));
        Self::footer(picker);
    }
    fn footer(picker: &mut Picker) {
        let offers = picker
            .selected
            .and_then(|i| picker.rows.get(i))
            .is_none_or(|r| !r.badges.iter().any(|b| b == "no effort"));
        picker.effort_text = if picker.current_effort.is_none() {
            "effort: could not be read".into()
        } else if !offers {
            "effort: not offered by this model".into()
        } else {
            let current = picker.current_effort.unwrap_or(Effort::Medium);
            let chosen = picker.chosen_effort.unwrap_or(current);
            if current == chosen {
                format!("effort: {}", current.as_wire())
            } else {
                format!("effort: {} → {}", current.as_wire(), chosen.as_wire())
            }
        };
    }
    fn key(&mut self, key: PickerKey, effects: &mut Vec<ModelsEffect>) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        picker.feedback = None;
        picker.banner = picker.failed.clone();
        match key {
            PickerKey::Char(ch) if !ch.is_control() => picker.filter.push(ch),
            PickerKey::Backspace => {
                picker.filter.pop();
            }
            PickerKey::ClearFilter => picker.filter.clear(),
            PickerKey::Escape if !picker.filter.is_empty() => picker.filter.clear(),
            PickerKey::Escape | PickerKey::Close => {
                self.close(effects);
                return;
            }
            PickerKey::ToggleScope => {
                if self.saved.is_err() {
                    return;
                }
                picker.scope = if picker.scope == Scope::ThisSession {
                    Scope::ThisSessionAndNew
                } else {
                    Scope::ThisSession
                }
            }
            PickerKey::Left | PickerKey::Right => {
                if picker
                    .selected
                    .and_then(|i| picker.rows.get(i))
                    .is_none_or(|r| !r.badges.iter().any(|b| b == "no effort"))
                    && let Some(chosen) = picker.chosen_effort
                {
                    picker.chosen_effort = Some(if key == PickerKey::Left {
                        chosen.down()
                    } else {
                        chosen.up()
                    });
                }
            }
            PickerKey::Up
            | PickerKey::Down
            | PickerKey::PageUp
            | PickerKey::PageDown
            | PickerKey::Home
            | PickerKey::End => {
                let indexes: Vec<_> = picker
                    .rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| r.key.is_some().then_some(i))
                    .collect();
                if !indexes.is_empty() {
                    let pos = indexes
                        .iter()
                        .position(|i| Some(*i) == picker.selected)
                        .unwrap_or(0);
                    let next = match key {
                        PickerKey::Home => 0,
                        PickerKey::End => indexes.len() - 1,
                        PickerKey::Up => pos.saturating_sub(1),
                        PickerKey::PageUp => pos.saturating_sub(self.list_height),
                        PickerKey::PageDown => (pos + self.list_height).min(indexes.len() - 1),
                        _ => (pos + 1).min(indexes.len() - 1),
                    };
                    picker.selected = Some(indexes[next]);
                }
                Self::footer(picker);
                return;
            }
            PickerKey::Refresh => {
                picker.refreshing = true;
                effects.push(ModelsEffect::LoadOptions {
                    session_id: picker.session_id.clone(),
                    refresh: true,
                });
            }
            PickerKey::ToggleFavourite => {
                let selected = picker
                    .selected
                    .and_then(|i| picker.rows.get(i))
                    .and_then(|r| r.key.clone());
                if let Some(
                    RowKey::Model { provider, model } | RowKey::Favourite { provider, model },
                ) = selected
                    && let Ok(saved) = &mut self.saved
                {
                    let key = format!("{provider}::{model}");
                    if let Some(index) = saved.favourites.iter().position(|f| *f == key) {
                        saved.favourites.remove(index);
                    } else {
                        saved.favourites.push(key);
                    }
                    effects.push(ModelsEffect::SaveFile(saved.clone()));
                }
            }
            PickerKey::Enter => {
                self.enter(effects);
                return;
            }
            _ => {}
        }
        self.rebuild();
    }
    fn enter(&mut self, effects: &mut Vec<ModelsEffect>) {
        let Some(picker) = &self.picker else { return };
        let Some(row) = picker.selected.and_then(|i| picker.rows.get(i)) else {
            return;
        };
        let Some(key) = row.key.clone() else { return };
        let sid = picker.session_id.clone();
        let session = self.sessions.get(&sid);
        let catalog = session.and_then(|s| s.catalog.as_ref());
        let name = |slug: &str| {
            catalog
                .and_then(|c| c.display_name(slug))
                .unwrap_or(slug)
                .to_string()
        };
        let (provider, model) = match key {
            RowKey::NotSetUp { provider } => {
                let text = notice_t14(&name(&provider));
                self.picker_notice(&sid, text, effects);
                return;
            }
            RowKey::Model { provider, model } | RowKey::Favourite { provider, model } => {
                (provider, model)
            }
        };
        if row.badges.iter().any(|b| b == "not listed") {
            let text = notice_t37(&model, &name(&provider));
            self.picker_notice(&sid, text, effects);
            return;
        }
        if row.badges.iter().any(|b| b == "unavailable") {
            let text = notice_t15(&model, &name(&provider));
            self.picker_notice(&sid, text, effects);
            return;
        }
        let offers = !row.badges.iter().any(|b| b == "no effort");
        let changed = offers
            && picker.current_effort.is_some()
            && picker.current_effort != picker.chosen_effort;
        let same = catalog.is_some_and(|c| {
            c.result.model == model
                && (c.result.provider == provider
                    || (c.result.provider.is_empty()
                        && c.result
                            .providers
                            .iter()
                            .any(|p| p.slug == provider && p.is_current == Some(true))))
        });
        let mut pick = Pick {
            operation: "model".into(),
            payload: Map::new(),
            profile: picker.profile.clone(),
            save: picker.scope == Scope::ThisSessionAndNew,
            model: Some((model.clone(), Some(provider.clone()))),
            effort: if changed {
                Some(picker.chosen_effort)
            } else {
                None
            },
            confirmed: false,
        };
        let already = same && !session.is_some_and(|s| s.deferred.is_some());
        if already && !changed {
            let text = notice_already(
                &model,
                if offers {
                    picker.current_effort.map(Effort::as_wire)
                } else {
                    None
                },
            );
            self.close(effects);
            if let Some(unknown) = self.sessions.get_mut(&sid).and_then(|s| s.unknown.take()) {
                self.ledger.settle(
                    &unknown.request,
                    &MutationOutcome::NotSent {
                        reason: String::new(),
                    },
                );
            }
            if pick.save {
                self.save_pick(&sid, &pick, None, effects);
            } else {
                effects.push(notice(&sid, NoticeLevel::Info, text));
            }
            return;
        }
        if already && changed {
            pick.operation = "reasoning".into();
            pick.payload
                .insert("effort".into(), effort_value(picker.chosen_effort));
        } else {
            pick.payload.insert("model".into(), json!(model));
            pick.payload.insert("provider".into(), json!(provider));
            if changed {
                pick.payload
                    .insert("reasoning".into(), effort_value(picker.chosen_effort));
            }
        }
        self.submit(&sid, pick, effects);
        if effects.iter().any(|e| matches!(e, ModelsEffect::Mutate(_)))
            || self.sessions.get(&sid).is_some_and(|s| s.queued.is_some())
        {
            self.close(effects);
        }
    }
    fn submit(&mut self, sid: &str, mut pick: Pick, effects: &mut Vec<ModelsEffect>) {
        let session = self.sessions.entry(sid.into()).or_default();
        if session.active.is_some() {
            session.queued = Some(pick);
            return;
        }
        if let Some(unknown) = session.unknown.take() {
            let mut sent = unknown.pick.payload.clone();
            sent.remove("confirm");
            let mut requested = pick.payload.clone();
            requested.remove("confirm");
            if unknown.pick.operation == pick.operation && sent == requested {
                pick.payload = unknown.pick.payload;
                pick.confirmed = unknown.pick.confirmed;
            }
        }
        session.confirmation = None;
        session.deferred = None;
        session.pending_save = None;
        match self
            .ledger
            .build(sid, &pick.operation, pick.payload.clone(), true)
        {
            Ok(request) => {
                session.active = Some(Active {
                    request: request.clone(),
                    pick,
                    setting_effort: false,
                });
                effects.push(ModelsEffect::Mutate(request));
            }
            Err(outcome) => self.failure(sid, &pick, &outcome, effects),
        }
    }
    fn drain(&mut self, sid: &str, effects: &mut Vec<ModelsEffect>) {
        if let Some(pick) = self.sessions.get_mut(sid).and_then(|s| s.queued.take()) {
            self.submit(sid, pick, effects);
        }
    }
    fn returned(
        &mut self,
        request: MutationRequest,
        outcome: MutationOutcome,
        effects: &mut Vec<ModelsEffect>,
    ) {
        let sid = request.session_id.clone();
        let matches = self
            .sessions
            .get(&sid)
            .and_then(|s| s.active.as_ref())
            .is_some_and(|active| !active.setting_effort && active.request == request);
        if !matches {
            return;
        }
        let Some(mut active) = self.sessions.get_mut(&sid).and_then(|s| s.active.take()) else {
            return;
        };
        if matches!(outcome, MutationOutcome::FenceRefused { .. })
            && let Some(retry) = self.ledger.retry_after_fence(&request, &outcome)
        {
            active.request = retry.clone();
            self.sessions.entry(sid).or_default().active = Some(active);
            effects.push(ModelsEffect::Mutate(retry));
            return;
        }
        self.ledger.settle(&request, &outcome);
        match &outcome {
            MutationOutcome::Committed(receipt) => {
                if active.pick.operation == "model" {
                    let model = receipt
                        .get("model")
                        .and_then(Value::as_str)
                        .or_else(|| active.pick.payload.get("model").and_then(Value::as_str))
                        .unwrap_or("");
                    let provider = receipt
                        .get("provider")
                        .and_then(Value::as_str)
                        .or_else(|| active.pick.payload.get("provider").and_then(Value::as_str))
                        .unwrap_or("the current provider");
                    let mut text =
                        format!("Model: {model} on {}.", self.provider_name(&sid, provider));
                    if let Some(effort) = receipt.get("effort").and_then(Value::as_str) {
                        text.push_str(&format!(" Effort: {effort}."));
                    }
                    effects.push(notice(&sid, NoticeLevel::Info, text));
                    self.adopt_model(&sid, receipt);
                } else {
                    let level = receipt
                        .get("effort")
                        .map_or(active.pick.effort.flatten(), |value| {
                            value.as_str().and_then(Effort::parse)
                        });
                    effects.push(notice(
                        &sid,
                        NoticeLevel::Info,
                        level.map_or_else(
                            || notice_effort_back().into(),
                            |e| notice_effort(e.as_wire()),
                        ),
                    ));
                    self.sessions.entry(sid.clone()).or_default().effort = level;
                }
                self.save_pick(&sid, &active.pick, Some(receipt), effects);
            }
            MutationOutcome::Deferred(receipt) => {
                let session = self.sessions.entry(sid.clone()).or_default();
                session.deferred = Some(
                    session
                        .catalog
                        .as_ref()
                        .map(|c| c.result.model.clone())
                        .or_else(|| session.model.clone()),
                );
                let model = receipt
                    .get("model")
                    .and_then(Value::as_str)
                    .or_else(|| active.pick.payload.get("model").and_then(Value::as_str))
                    .unwrap_or("");
                effects.push(notice(&sid, NoticeLevel::Info, notice_deferred(model)));
                if active.pick.save
                    && active
                        .pick
                        .model
                        .as_ref()
                        .is_some_and(|(_, provider)| provider.is_none())
                    && receipt.get("provider").and_then(Value::as_str).is_none()
                {
                    session.pending_save = Some(active.pick.clone());
                } else {
                    self.save_pick(&sid, &active.pick, Some(receipt), effects);
                }
            }
            MutationOutcome::NeedsConfirmation(confirmation) => {
                if active.pick.confirmed {
                    effects.push(notice(
                        &sid,
                        NoticeLevel::Warning,
                        format!(
                            "Hermes still asks for confirmation, so the model was not changed: {}",
                            confirmation.message
                        ),
                    ));
                } else {
                    let session = self.sessions.entry(sid.clone()).or_default();
                    let keep = session
                        .model
                        .clone()
                        .unwrap_or_else(|| "the current model".into());
                    session.confirmation = Some(PendingConfirmation {
                        confirmation: confirmation.clone(),
                        pick: active.pick,
                        from_event: false,
                        keep,
                    });
                }
            }
            MutationOutcome::Refused { reason, .. }
                if active.pick.operation == "reasoning"
                    && matches!(reason.as_str(), "session_busy" | "unknown_execution") =>
            {
                let level = active.pick.effort.flatten();
                active.setting_effort = true;
                self.sessions.entry(sid.clone()).or_default().active = Some(active);
                effects.push(ModelsEffect::SetEffort {
                    session_id: sid,
                    level,
                });
                return;
            }
            MutationOutcome::Unknown => {
                self.failure(&sid, &active.pick, &outcome, effects);
                self.sessions.entry(sid.clone()).or_default().unknown = Some(active);
            }
            _ => self.failure(&sid, &active.pick, &outcome, effects),
        }
        self.drain(&sid, effects);
    }
    fn effort_returned(
        &mut self,
        sid: &str,
        level: Option<Effort>,
        outcome: Result<(), MutationOutcome>,
        effects: &mut Vec<ModelsEffect>,
    ) {
        if !self
            .sessions
            .get(sid)
            .and_then(|s| s.active.as_ref())
            .is_some_and(|a| a.setting_effort && a.pick.effort == Some(level))
        {
            return;
        }
        let Some(active) = self.sessions.get_mut(sid).and_then(|s| s.active.take()) else {
            return;
        };
        match outcome {
            Ok(()) => {
                effects.push(notice(sid, NoticeLevel::Info, level.map_or_else(|| notice_effort_held_profile().into(), |e| notice_t31_level(e.as_wire())))); self.save_pick(sid, &active.pick, None, effects);
            }
            Err(MutationOutcome::Unknown | MutationOutcome::Unreadable) => effects.push(notice(sid, NoticeLevel::Warning, "The connection dropped before Hermes answered, so the effort may or may not change when the current turn ends. The status line shows what Hermes reports; setting the same effort again is safe.")),
            Err(outcome) => self.failure(sid, &active.pick, &outcome, effects),
        }
        self.drain(sid, effects);
    }
    fn failure(
        &self,
        sid: &str,
        pick: &Pick,
        outcome: &MutationOutcome,
        effects: &mut Vec<ModelsEffect>,
    ) {
        let (level, text) = match outcome {
            MutationOutcome::FenceRefused { .. } => (NoticeLevel::Warning, "Another window changed this session at the same moment, so nothing was changed. Try again.".into()),
            MutationOutcome::Unknown => (NoticeLevel::Warning, "The connection dropped before Hermes answered, so the change may or may not have happened. The status line shows what Hermes reports; making the same change again is safe.".into()),
            MutationOutcome::Unreadable => (NoticeLevel::Warning, "Hermes's answer could not be read, so the change may or may not have happened. The status line shows what Hermes reports.".into()),
            MutationOutcome::NotSent { reason } => {
                if self.fences(sid).ready().is_none() { (NoticeLevel::Warning, "Talaria has not learned this session's revision yet; try again in a moment.".into()) }
                else { (NoticeLevel::Error, format!("Not sent: {reason}.")) }
            }
            MutationOutcome::Refused { reason, detail, rpc_code, .. } => {
                let text = if *rpc_code == -32601 { unknown_method(if pick.operation == "model" { "change the model" } else { "change the effort" }) } else { match reason.as_str() {
                    "model_resolution_failed" => {
                        let model = pick.payload.get("model").and_then(Value::as_str).unwrap_or("");
                        let provider = pick.payload.get("provider").and_then(Value::as_str).map(|p| self.provider_name(sid, p)).unwrap_or_else(|| "the current provider".into()); notice_t6(model, &provider)
                    }
                    "permission_denied" => "Hermes did not allow this window to change the session (permission_denied).".into(),
                    "not_found" => "Hermes no longer has this session (not_found).".into(),
                    _ => {
                        let reason = if !reason.is_empty() && reason.len() <= 80 && reason.bytes().all(|c| c.is_ascii_lowercase() || c == b'_') { reason.as_str() } else { "an unnamed reason" };
                        let detail = detail.as_ref().map(|text| text.chars().take(120).collect::<String>()); notice_t24(reason, detail.as_deref())
                    }
                } };
                (NoticeLevel::Error, text)
            }
            MutationOutcome::OtherStatus { status, .. } => (NoticeLevel::Error, notice_t24(status, None)),
            _ => return,
        };
        effects.push(notice(sid, level, text));
    }
    fn save_pick(
        &mut self,
        sid: &str,
        pick: &Pick,
        receipt: Option<&Value>,
        effects: &mut Vec<ModelsEffect>,
    ) {
        if !pick.save {
            return;
        }
        let model_pair = pick.model.as_ref().and_then(|(model, provider)| {
            let model = receipt
                .and_then(|r| r.get("model"))
                .and_then(Value::as_str)
                .unwrap_or(model)
                .to_string();
            let provider = receipt
                .and_then(|r| r.get("provider"))
                .and_then(Value::as_str)
                .map(String::from)
                .or_else(|| provider.clone())?;
            (!model.trim().is_empty() && !provider.trim().is_empty()).then_some((model, provider))
        });
        let effort = pick.effort.map(|sent| {
            receipt
                .and_then(|r| r.get("effort"))
                .map_or(sent, |value| value.as_str().and_then(Effort::parse))
        });
        let mut what = Vec::new();
        if let Some((model, provider)) = &model_pair {
            what.push(format!("{model} on {}", self.provider_name(sid, provider)));
        }
        if let Some(effort) = effort {
            what.push(effort.map_or_else(
                || "no saved effort".into(),
                |e| format!("effort {}", e.as_wire()),
            ));
        }
        if what.is_empty() {
            return;
        }
        if let Ok(saved) = &mut self.saved {
            let default = saved.defaults.entry(pick.profile.clone()).or_default();
            if let Some((model, provider)) = model_pair {
                default.model = Some(model);
                default.provider = Some(provider);
            }
            if let Some(effort) = effort {
                default.effort = effort;
            }
            effects.push(ModelsEffect::SaveFile(saved.clone()));
            effects.push(notice(
                sid,
                NoticeLevel::Info,
                notice_saved(&pick.profile, &what.join(", ")),
            ));
        }
    }
    fn provider_name(&self, sid: &str, provider: &str) -> String {
        self.sessions
            .get(sid)
            .and_then(|s| s.catalog.as_ref())
            .and_then(|c| c.display_name(provider))
            .unwrap_or(provider)
            .into()
    }
    fn adopt_model(&mut self, sid: &str, receipt: &Value) {
        let session = self.sessions.entry(sid.into()).or_default();
        if let Some(effort) = receipt
            .get("effort")
            .or_else(|| receipt.get("reasoning_effort"))
        {
            session.effort = effort.as_str().and_then(Effort::parse);
        }
        if let Some(model) = receipt.get("model").and_then(Value::as_str) {
            session.model = Some(model.into());
            if let Some(catalog) = &mut session.catalog {
                catalog.result.model = model.into();
            }
        }
        if let Some(provider) = receipt.get("provider").and_then(Value::as_str)
            && let Some(catalog) = &mut session.catalog
        {
            catalog.result.provider = provider.into();
        }
    }
    fn event(
        &mut self,
        sid: &str,
        type_name: &str,
        value: &Value,
        effects: &mut Vec<ModelsEffect>,
    ) {
        if !matches!(type_name, "session.info" | "session.updated") {
            return;
        }
        if value
            .get("session_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id != sid)
        {
            return;
        }
        self.ledger.fences_mut(sid).observe(sid, type_name, value);
        let nested = value.get("payload");
        let body = if value.get("operation").is_none()
            && value.get("status").is_none()
            && (value.get("type").is_some()
                || value.get("session_id").is_some()
                || nested
                    .is_some_and(|p| p.get("operation").is_some() || p.get("status").is_some()))
        {
            nested.unwrap_or(value)
        } else {
            value
        };
        self.ledger.fences_mut(sid).adopt(
            body.get("revision").and_then(Value::as_i64),
            body.get("execution_generation").and_then(Value::as_i64),
        );
        if type_name == "session.info" {
            let session = self.sessions.entry(sid.into()).or_default();
            if let Some(Some(from)) = &session.deferred
                && body
                    .get("model")
                    .and_then(Value::as_str)
                    .is_some_and(|model| model != from)
            {
                session.deferred = None;
            }
            self.adopt_model(sid, body);
            return;
        }
        if type_name != "session.updated" {
            return;
        }
        match body.get("status").and_then(Value::as_str) {
            None if body.get("operation").and_then(Value::as_str) == Some("model") => {
                let session = self.sessions.entry(sid.into()).or_default();
                session.deferred = None;
                let pending = session.pending_save.take();
                if let Some(pick) = pending {
                    self.save_pick(sid, &pick, Some(body), effects);
                }
                self.adopt_model(sid, body);
            }
            Some("refused") => {
                let session = self.sessions.entry(sid.into()).or_default();
                session.deferred = None;
                session.confirmation = None;
                session.pending_save = None;
                effects.push(notice(
                    sid,
                    NoticeLevel::Error,
                    notice_t13(
                        body.get("target_model")
                            .and_then(Value::as_str)
                            .unwrap_or("the requested model"),
                        body.get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("an unnamed reason"),
                        body.get("model")
                            .and_then(Value::as_str)
                            .or(session.model.as_deref())
                            .unwrap_or("the current model"),
                    ),
                ));
            }
            Some("confirmation_required") => {
                let session = self.sessions.entry(sid.into()).or_default();
                session.deferred = None;
                session.pending_save = None;
                if session.active.is_some() {
                    return;
                }
                let Some(token) = body
                    .get("confirm")
                    .and_then(Value::as_str)
                    .filter(|t| !t.is_empty())
                else {
                    return;
                };
                let Some(target) = body.get("target_model").and_then(Value::as_str) else {
                    return;
                };
                let provider = body
                    .get("target_provider")
                    .and_then(Value::as_str)
                    .map(String::from);
                let mut payload = body
                    .get("payload")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                payload.insert("model".into(), json!(target));
                if let Some(provider) = &provider {
                    payload.insert("provider".into(), json!(provider));
                }
                let keep = body
                    .get("committed_model")
                    .and_then(Value::as_str)
                    .or(session.model.as_deref())
                    .unwrap_or("the current model")
                    .to_string();
                let confirmation = Confirmation {
                    token: token.into(),
                    message: body
                        .get("confirm_message")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    target_model: target.into(),
                    target_provider: provider.clone(),
                    pick: payload.clone(),
                };
                let effort = payload
                    .get("reasoning")
                    .map(|v| v.as_str().and_then(Effort::parse));
                let pick = Pick {
                    operation: "model".into(),
                    payload,
                    profile: session.profile.clone(),
                    save: false,
                    model: Some((target.into(), provider)),
                    effort,
                    confirmed: false,
                };
                session.confirmation = Some(PendingConfirmation {
                    confirmation,
                    pick,
                    from_event: true,
                    keep,
                });
            }
            _ => {}
        }
    }
    fn confirm(&mut self, key: ConfirmKey, effects: &mut Vec<ModelsEffect>) {
        if self.visible_confirmation().is_none() {
            return;
        }
        let Some(sid) = self.foreground.clone() else {
            return;
        };
        let Some(mut pending) = self
            .sessions
            .get_mut(&sid)
            .and_then(|s| s.confirmation.take())
        else {
            return;
        };
        match key {
            ConfirmKey::Decline => effects.push(notice(
                &sid,
                NoticeLevel::Info,
                if pending.from_event {
                    notice_kept_pending(&pending.keep)
                } else {
                    notice_kept(&pending.keep)
                },
            )),
            ConfirmKey::Accept => {
                pending
                    .pick
                    .payload
                    .insert("confirm".into(), json!(pending.confirmation.token));
                pending.pick.confirmed = true;
                self.submit(&sid, pending.pick, effects);
            }
        }
    }
    fn picker_notice(&mut self, session_id: &str, text: String, effects: &mut Vec<ModelsEffect>) {
        if let Some(picker) = &mut self.picker {
            picker.banner = Some(text.clone());
            picker.feedback = Some(text.clone());
        }
        effects.push(notice(session_id, NoticeLevel::Info, text));
    }

    pub fn view(&self) -> Option<PickerView<'_>> {
        self.picker.as_ref().map(|p| PickerView { title: &p.title, filter: &p.filter, effort: &p.effort_text, hints: if p.refreshing { "refreshing all providers can take a few seconds; other actions wait until it finishes" } else { "enter switch · ctrl+g scope · ctrl+f ★ · ctrl+r refresh · esc" }, refreshing: p.refreshing, rows: &p.rows, selected: p.selected, banner: p.banner.as_deref() })
    }
    fn visible_confirmation(&self) -> Option<&PendingConfirmation> {
        if self.picker.is_some() {
            return None;
        }
        self.foreground
            .as_ref()
            .and_then(|sid| self.sessions.get(sid))
            .and_then(|s| s.confirmation.as_ref())
    }
    pub fn confirmation(&self) -> Option<&Confirmation> {
        self.visible_confirmation()
            .map(|pending| &pending.confirmation)
    }
    pub fn is_deferred(&self, session_id: &str) -> bool {
        self.sessions
            .get(session_id)
            .is_some_and(|s| s.deferred.is_some())
    }
    pub fn fences(&self, session_id: &str) -> Fences {
        self.ledger.fences(session_id)
    }
    /// Adopt fences another ledger learned for the session; the larger numbers win.
    pub fn adopt_fences(&mut self, session_id: &str, fences: Fences) {
        self.ledger
            .fences_mut(session_id)
            .adopt(fences.revision, fences.execution_generation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> ModelsState {
        let mut state = ModelsState::new("L", Ok(SavedModels::empty()));
        state.update(ModelsInput::SessionOpened {
            session_id: "s".into(),
            revision: Some(1),
            execution_generation: Some(1),
        });
        state.update(ModelsInput::ForegroundChanged {
            session_id: Some("s".into()),
        });
        state
    }

    fn switch(state: &mut ModelsState) -> Vec<ModelsEffect> {
        state.update(ModelsInput::Command {
            session_id: Some("s".into()),
            profile: "fixture".into(),
            command: Ok(ModelCommandOrEffort::Model(ModelCommand::Switch {
                model: "target".into(),
                provider: Some("p".into()),
                effort: Some(Effort::High),
                save_default: false,
            })),
        })
    }

    fn request(effects: Vec<ModelsEffect>) -> MutationRequest {
        effects
            .into_iter()
            .find_map(|effect| match effect {
                ModelsEffect::Mutate(request) => Some(request),
                _ => None,
            })
            .expect("a request")
    }

    #[test]
    fn a_lost_confirmed_reply_replays_the_exact_confirmed_request() {
        let mut state = seeded();
        let first = request(switch(&mut state));
        state.update(ModelsInput::MutationReturned {
            request: first,
            outcome: MutationOutcome::NeedsConfirmation(Confirmation {
                token: "token".into(),
                message: "Confirm".into(),
                target_model: "target".into(),
                target_provider: Some("p".into()),
                pick: Map::new(),
            }),
        });
        let confirmed = request(state.update(ModelsInput::Confirm(ConfirmKey::Accept)));
        state.update(ModelsInput::MutationReturned {
            request: confirmed.clone(),
            outcome: MutationOutcome::Unknown,
        });
        state.update(ModelsInput::SessionEvent {
            session_id: "s".into(),
            type_name: "session.info".into(),
            payload: json!({"revision":9,"execution_generation":4}),
        });
        assert_eq!(request(switch(&mut state)), confirmed);
    }

    #[test]
    fn an_event_for_another_session_cannot_raise_a_card_or_advance_fences() {
        let mut state = seeded();
        state.update(ModelsInput::SessionEvent { session_id: "s".into(), type_name: "session.updated".into(), payload: json!({"session_id":"other","payload":{"status":"confirmation_required","confirm":"token","target_model":"target","revision":99,"execution_generation":99}}) });
        assert!(state.confirmation().is_none());
        assert_eq!(state.fences("s").ready(), Some((1, 1)));
    }

    #[test]
    fn the_saved_file_warning_is_shown_once_and_never_written() {
        let mut state = ModelsState::new("L", Err("bad version".into()));
        let first = state.update(ModelsInput::OpenPicker {
            session_id: Some("s".into()),
            profile: "fixture".into(),
        });
        assert_eq!(
            first
                .iter()
                .filter(|effect| matches!(effect, ModelsEffect::Notice { .. }))
                .count(),
            1
        );
        state.update(ModelsInput::Picker(PickerKey::Close));
        let second = state.update(ModelsInput::OpenPicker {
            session_id: Some("s".into()),
            profile: "fixture".into(),
        });
        assert!(second.iter().all(|effect| !matches!(
            effect,
            ModelsEffect::Notice { .. } | ModelsEffect::SaveFile(_)
        )));
    }

    #[test]
    fn a_refused_row_stays_visible_with_its_notice_until_the_next_key() {
        let mut state = seeded();
        state.update(ModelsInput::OpenPicker {
            session_id: Some("s".into()),
            profile: "fixture".into(),
        });
        state.update(ModelsInput::OptionsLoaded {
            session_id: "s".into(),
            result: serde_json::from_value(json!({
                "model":"current", "provider":"other", "providers":[{
                    "name":"Other", "slug":"other", "models":[], "authenticated":false
                }]
            }))
            .unwrap(),
        });
        let effects = state.update(ModelsInput::Picker(PickerKey::Enter));
        let text = notice_t14("Other");
        assert_eq!(state.view().unwrap().banner, Some(text.as_str()));
        assert!(effects.iter().any(|effect| matches!(effect,
            ModelsEffect::Notice { text: shown, .. } if shown == &text)));
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, ModelsEffect::Mutate(_)))
        );
        state.update(ModelsInput::EffortRead {
            session_id: "s".into(),
            value: Some("medium".into()),
        });
        assert_eq!(state.view().unwrap().banner, Some(text.as_str()));
        state.update(ModelsInput::Picker(PickerKey::Down));
        assert_eq!(state.view().unwrap().banner, None);
    }
}
