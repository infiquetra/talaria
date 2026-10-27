//! Review, conflict check, write and read-back for one section at a time.

use super::baseline::same_roles;
use super::errors::{is_unknown, needs_read_back, write_lines};
use super::fallbacks::{fallback_patch, is_complete, same_chain};
use super::helpers::follows_main;
use super::main_role::{config_patch, effort_from_raw};
use super::state::{
    HelperEdit, HelperRoute, Job, JobStatus, Mode, ModelRolesState, Prompt, ReviewMode, Run, Then,
};
use super::subagents::{chain_value, subagent_patch};
use super::{
    ApplyReply, ApplyStep, AssignOutcome, AssignScopeView, Assignment, AuxSlot, ChainMode,
    EffortChange, Loaded, RolesEffect, RolesError, Section, text,
};

pub(super) fn load_effects() -> Vec<RolesEffect> {
    vec![
        RolesEffect::LoadAll,
        RolesEffect::LoadCatalog { refresh: false },
    ]
}

impl ModelRolesState {
    pub(super) fn reload(&mut self) -> Vec<RolesEffect> {
        self.link = super::state::Link::Connected;
        load_effects()
    }

    pub(super) fn run_then(&mut self, then: Then) -> Vec<RolesEffect> {
        match then {
            Then::Nothing => Vec::new(),
            Then::Section(section) => {
                self.section = section;
                self.focus_detail = false;
                Vec::new()
            }
            Then::Reload => self.reload(),
            Then::Close => vec![RolesEffect::Close],
        }
    }

    /// Problems that stop a review from opening, in the words Hermes would use.
    fn validation(&self, section: Section) -> Vec<String> {
        let numbered = |entries: &[super::FallbackEntry]| {
            entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| !is_complete(entry))
                .map(|(index, _)| text::entry_needs_model(index + 1))
                .collect::<Vec<_>>()
        };
        match section {
            Section::Fallbacks => numbered(&self.fallback_work()),
            Section::Subagents => match self.sub_work().chain {
                ChainMode::Own(entries) => numbered(&entries),
                _ => Vec::new(),
            },
            Section::Moa => {
                if self.edits.moa.is_none() && !self.first_save() {
                    return Vec::new();
                }
                self.moa_work()
                    .map(|config| super::validate_moa(&config))
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn open_review(&mut self, section: Section, then: Then) -> Vec<RolesEffect> {
        self.section = section;
        let problems = self.validation(section);
        if !problems.is_empty() {
            self.notice = problems;
            self.mode = Mode::Browse;
            return Vec::new();
        }
        self.mode = Mode::Review(ReviewMode {
            section,
            then,
            fresh: false,
            go: false,
        });
        vec![RolesEffect::FetchFresh { section }]
    }

    pub(super) fn helper_assignments(&self) -> Vec<Assignment> {
        let slots = self.helper_slots();
        let mut edits: Vec<&HelperEdit> = self.edits.helpers.iter().collect();
        edits.sort_by_key(|edit| {
            slots
                .iter()
                .position(|slot| slot.task == edit.task)
                .unwrap_or(usize::MAX)
        });
        edits
            .into_iter()
            .map(|edit| helper_assignment(edit, slots.iter().find(|slot| slot.task == edit.task)))
            .collect()
    }

    pub(super) fn main_assignment(&self, confirmed: bool) -> Assignment {
        let (provider, model, base_url) = match &self.edits.main_route {
            Some(route) => (
                route.provider.clone(),
                route.model.clone(),
                route.base_url.clone(),
            ),
            None => {
                let (provider, model) = self.main_route().unwrap_or_default();
                (provider, model, String::new())
            }
        };
        Assignment {
            scope: AssignScopeView::Main,
            task: String::new(),
            provider,
            model,
            reasoning_effort: EffortChange::Omit,
            base_url,
            confirmed,
        }
    }

    // ----- re-read before writing -----

    pub(super) fn absorb(&mut self, reads: &[Loaded]) {
        for read in reads {
            match read.clone() {
                Loaded::Info(info) => self.info = super::state::Load::Ready(info),
                Loaded::Auxiliary(aux) => self.aux = super::state::Load::Ready(aux),
                Loaded::Moa(moa) => self.moa = super::state::Load::Ready(moa),
                Loaded::Raw(raw) => self.raw = super::state::Load::Ready(raw),
            }
        }
    }

    /// True when a value this apply writes differs from what the page was showing.
    fn conflicts(&self, section: Section, reads: &[Loaded]) -> bool {
        reads.iter().any(|read| match (section, read) {
            (Section::Main, Loaded::Auxiliary(fresh)) if self.compares_main_route() => {
                !self.aux.value().is_some_and(|base| {
                    base.main.provider == fresh.main.provider && base.main.model == fresh.main.model
                })
            }
            (Section::Main, Loaded::Info(fresh)) if self.compares_main_route() => !self
                .info
                .value()
                .is_some_and(|base| base.provider == fresh.provider && base.model == fresh.model),
            (Section::Main, Loaded::Raw(fresh)) => {
                let base = self.raw_value();
                (self.edits.main_context.is_some()
                    && base
                        .is_none_or(|base| base.model_context_length != fresh.model_context_length))
                    || (self.edits.main_effort.is_some()
                        && base.is_none_or(|base| {
                            base.agent_reasoning_effort != fresh.agent_reasoning_effort
                        }))
            }
            (Section::Helpers, Loaded::Auxiliary(fresh)) => self.edits.helpers.iter().any(|edit| {
                let find = |slots: &[AuxSlot]| {
                    slots
                        .iter()
                        .find(|slot| slot.task == edit.task)
                        .map(|slot| {
                            (
                                slot.provider.clone(),
                                slot.model.clone(),
                                slot.base_url.clone(),
                                slot.reasoning_effort.clone(),
                            )
                        })
                };
                let base = self.aux.value().map(|aux| find(&aux.slots));
                base != Some(find(&fresh.slots))
            }),
            (Section::Fallbacks, Loaded::Raw(fresh)) => self
                .raw_value()
                .is_none_or(|base| base.fallback_providers != fresh.fallback_providers),
            (Section::Subagents, Loaded::Raw(fresh)) => {
                let Some(base) = self.raw_value() else {
                    return true;
                };
                let patch = subagent_patch(&self.sub_base(), &self.sub_work());
                patch["delegation"].as_object().is_some_and(|keys| {
                    keys.keys()
                        .any(|key| base.delegation.get(key) != fresh.delegation.get(key))
                })
            }
            (Section::Moa, Loaded::Moa(fresh)) => self.moa.value() != Some(fresh),
            _ => false,
        })
    }

    fn compares_main_route(&self) -> bool {
        self.edits.main_route.is_some() || !self.pending(Section::Main)
    }

    pub(super) fn on_fresh(&mut self, section: Section, reads: &[Loaded]) -> Vec<RolesEffect> {
        let conflict = self.conflicts(section, reads);
        self.absorb(reads);
        if conflict {
            self.section = section;
            self.notice = vec![text::CONFLICT.to_owned()];
            if matches!(self.mode, Mode::Review(_)) {
                self.mode = Mode::Browse;
            }
            return Vec::new();
        }
        let mut write = false;
        if let Mode::Review(review) = &mut self.mode
            && review.section == section
        {
            review.fresh = true;
            write = review.go;
        }
        if write {
            self.begin_write()
        } else {
            Vec::new()
        }
    }

    pub(super) fn on_fresh_failed(&mut self, error: &RolesError) -> Vec<RolesEffect> {
        let mut lines = vec![text::CHECK_FAILED.to_owned()];
        lines.extend(super::errors::read_lines(error, "the current values"));
        self.notice = lines;
        if matches!(self.mode, Mode::Review(_)) {
            self.mode = Mode::Browse;
        }
        Vec::new()
    }

    // ----- writing -----

    pub(super) fn begin_write(&mut self) -> Vec<RolesEffect> {
        let Mode::Review(review) = std::mem::replace(&mut self.mode, Mode::Browse) else {
            return Vec::new();
        };
        let section = review.section;
        if !self.has_changes(section) {
            self.notice = vec![text::NO_CHANGES.to_owned()];
            return self.run_then(review.then);
        }
        let mut run = Run::new(section, review.then);
        let effects = match section {
            Section::Main => {
                let patch = config_patch(self.edits.main_context, self.edits.main_effort);
                if self.edits.main_route.is_some() {
                    let assignment = self.main_assignment(false);
                    run.main_assignment = Some(assignment.clone());
                    run.config_patch = patch;
                    vec![RolesEffect::Assign {
                        step: ApplyStep::MainModel,
                        assignment,
                    }]
                } else {
                    match patch {
                        Some(patch) => vec![RolesEffect::PutConfig {
                            step: ApplyStep::MainConfig,
                            patch,
                        }],
                        None => Vec::new(),
                    }
                }
            }
            Section::Helpers => {
                let mut effects = Vec::new();
                for assignment in self.helper_assignments() {
                    run.jobs.push(Job {
                        task: assignment.task.clone(),
                        assignment: assignment.clone(),
                        status: JobStatus::Queued,
                    });
                    effects.push(RolesEffect::Assign {
                        step: ApplyStep::Helper {
                            task: assignment.task.clone(),
                        },
                        assignment,
                    });
                }
                effects
            }
            Section::Fallbacks => vec![RolesEffect::PutConfig {
                step: ApplyStep::Fallbacks,
                patch: fallback_patch(&self.fallback_work()),
            }],
            Section::Subagents => {
                let work = self.sub_work();
                let mut patch = subagent_patch(&self.sub_base(), &work);
                if let (ChainMode::Own(entries), Some(map)) =
                    (&work.chain, patch["delegation"].as_object_mut())
                    && entries.iter().any(|entry| entry.was_string)
                {
                    map.insert("fallback_providers".to_owned(), chain_value(&work.chain));
                }
                vec![RolesEffect::PutConfig {
                    step: ApplyStep::Subagents,
                    patch,
                }]
            }
            Section::Moa => match self.moa_work() {
                Some(config) => vec![RolesEffect::SaveMoa(config)],
                None => Vec::new(),
            },
        };
        if effects.is_empty() {
            return self.run_then(run.then);
        }
        self.run = Some(run);
        self.mode = Mode::Applying;
        effects
    }

    fn run_for(&mut self, section: Section) -> &mut Run {
        if self.run.as_ref().is_some_and(|run| run.section != section) {
            self.run = None;
        }
        if self.run.is_none() {
            // A result for the section under review ends that review; its follow-up still runs.
            let then = match &self.mode {
                Mode::Review(review) if review.section == section => {
                    let then = review.then.clone();
                    self.mode = Mode::Applying;
                    then
                }
                _ => Then::Nothing,
            };
            self.run = Some(Run::new(section, then));
        }
        self.run
            .get_or_insert_with(|| Run::new(section, Then::Nothing))
    }

    pub(super) fn on_applied(&mut self, step: ApplyStep, reply: ApplyReply) -> Vec<RolesEffect> {
        match step {
            ApplyStep::MainModel => self.main_model_applied(reply),
            ApplyStep::MainConfig => {
                self.run_for(Section::Main);
                vec![RolesEffect::ReadBack {
                    section: Section::Main,
                }]
            }
            ApplyStep::Helper { task } => self.helper_applied(&task, reply),
            ApplyStep::Fallbacks => self.simple_applied(Section::Fallbacks),
            ApplyStep::Subagents => self.simple_applied(Section::Subagents),
            ApplyStep::Moa => {
                if let ApplyReply::MoaSaved(config) = reply {
                    self.moa = super::state::Load::Ready(config);
                }
                self.simple_applied(Section::Moa)
            }
        }
    }

    fn simple_applied(&mut self, section: Section) -> Vec<RolesEffect> {
        self.run_for(section);
        vec![RolesEffect::ReadBack { section }]
    }

    fn main_model_applied(&mut self, reply: ApplyReply) -> Vec<RolesEffect> {
        if let ApplyReply::Assigned(AssignOutcome::ConfirmRequired { message }) = reply {
            self.run_for(Section::Main);
            self.mode = Mode::Prompt(Prompt::Confirm { message });
            return Vec::new();
        }
        let run = self.run_for(Section::Main);
        run.model_saved = true;
        if let ApplyReply::Assigned(AssignOutcome::Main {
            provider,
            model,
            stale,
        }) = reply
        {
            run.saved_route = Some((provider, model));
            run.stale = stale;
        }
        match run.config_patch.take() {
            Some(patch) => vec![RolesEffect::PutConfig {
                step: ApplyStep::MainConfig,
                patch,
            }],
            None => vec![RolesEffect::ReadBack {
                section: Section::Main,
            }],
        }
    }

    fn job_index(&mut self, task: &str) -> usize {
        let run = self.run_for(Section::Helpers);
        if let Some(index) = run.jobs.iter().position(|job| job.task == task) {
            return index;
        }
        run.jobs.push(Job {
            task: task.to_owned(),
            assignment: helper_assignment(
                &HelperEdit {
                    task: task.to_owned(),
                    route: None,
                    effort: None,
                },
                None,
            ),
            status: JobStatus::Queued,
        });
        run.jobs.len() - 1
    }

    fn helper_applied(&mut self, task: &str, reply: ApplyReply) -> Vec<RolesEffect> {
        let index = self.job_index(task);
        if let ApplyReply::Assigned(AssignOutcome::ConfirmRequired { message }) = reply {
            if let Some(run) = self.run.as_mut() {
                for (position, job) in run.jobs.iter_mut().enumerate() {
                    if position == index {
                        job.status = JobStatus::Confirm;
                    } else if position > index && job.status == JobStatus::Queued {
                        job.status = JobStatus::Held;
                    }
                }
            }
            self.mode = Mode::Prompt(Prompt::Confirm { message });
            return Vec::new();
        }
        if let Some(job) = self.run.as_mut().and_then(|run| run.jobs.get_mut(index)) {
            job.status = JobStatus::Saved;
        }
        self.helper_progress()
    }

    /// After a slot is saved: send the slots that were held back, or read back when all are done.
    fn helper_progress(&mut self) -> Vec<RolesEffect> {
        let Some(run) = self.run.as_mut() else {
            return Vec::new();
        };
        if run
            .jobs
            .iter()
            .any(|job| matches!(job.status, JobStatus::Confirm | JobStatus::Queued))
        {
            return Vec::new();
        }
        if !run.failed && run.jobs.iter().any(|job| job.status == JobStatus::Held) {
            let mut effects = Vec::new();
            for job in &mut run.jobs {
                if job.status == JobStatus::Held {
                    job.status = JobStatus::Queued;
                    effects.push(RolesEffect::Assign {
                        step: ApplyStep::Helper {
                            task: job.task.clone(),
                        },
                        assignment: job.assignment.clone(),
                    });
                }
            }
            return effects;
        }
        vec![RolesEffect::ReadBack {
            section: Section::Helpers,
        }]
    }

    fn helper_summary(&self) -> String {
        let Some(run) = &self.run else {
            return String::new();
        };
        let list = |status: &[JobStatus]| {
            let names: Vec<&str> = run
                .jobs
                .iter()
                .filter(|job| status.contains(&job.status))
                .map(|job| job.task.as_str())
                .collect();
            if names.is_empty() {
                "none".to_owned()
            } else {
                names.join(", ")
            }
        };
        let saved = list(&[JobStatus::Saved]);
        let unknown = list(&[JobStatus::Unknown]);
        let rest = list(&[
            JobStatus::Queued,
            JobStatus::Held,
            JobStatus::Failed,
            JobStatus::Confirm,
        ]);
        if unknown == "none" {
            text::HELPERS_PARTIAL
                .replace("{saved}", &saved)
                .replace("{rest}", &rest)
        } else {
            text::HELPERS_UNKNOWN
                .replace("{saved}", &saved)
                .replace("{unknown}", &unknown)
                .replace("{rest}", &rest)
        }
    }

    pub(super) fn on_apply_failed(
        &mut self,
        step: &ApplyStep,
        error: &RolesError,
    ) -> Vec<RolesEffect> {
        let section = match step {
            ApplyStep::MainModel | ApplyStep::MainConfig => Section::Main,
            ApplyStep::Helper { .. } => Section::Helpers,
            ApplyStep::Fallbacks => Section::Fallbacks,
            ApplyStep::Subagents => Section::Subagents,
            ApplyStep::Moa => Section::Moa,
        };
        let mut lines = Vec::new();
        let mut read_back = needs_read_back(error);
        match step {
            ApplyStep::Helper { task } => {
                let index = self.job_index(task);
                if let Some(run) = self.run.as_mut() {
                    if let Some(job) = run.jobs.get_mut(index) {
                        job.status = if is_unknown(error) {
                            JobStatus::Unknown
                        } else {
                            JobStatus::Failed
                        };
                    }
                    run.failed = true;
                }
                lines.push(self.helper_summary());
                read_back = true;
            }
            ApplyStep::MainConfig => {
                let saved = self.run_for(Section::Main).model_saved;
                if saved {
                    let rest = self.main_rest();
                    let line = if is_unknown(error) {
                        text::MAIN_PARTIAL.replace("Not saved", "Unknown")
                    } else {
                        text::MAIN_PARTIAL.to_owned()
                    };
                    lines.push(line.replace("{rest}", &rest));
                    read_back = true;
                }
            }
            _ => {}
        }
        if !matches!(step, ApplyStep::Helper { .. }) {
            self.run_for(section);
        }
        if let Some(run) = self.run.as_mut() {
            run.failed = true;
        }
        lines.extend(write_lines(error));
        self.notice = lines;
        if matches!(self.mode, Mode::Applying | Mode::Prompt(_)) {
            self.mode = Mode::Browse;
        }
        if read_back {
            vec![RolesEffect::ReadBack { section }]
        } else {
            self.run = None;
            Vec::new()
        }
    }

    fn main_rest(&self) -> String {
        let mut keys = Vec::new();
        if self.edits.main_context.is_some() {
            keys.push("context");
        }
        if self.edits.main_effort.is_some() {
            keys.push("effort");
        }
        if keys.is_empty() {
            keys.push("context, effort");
        }
        keys.join(", ")
    }

    // ----- answers to a pause -----

    pub(super) fn confirm_yes(&mut self) -> Vec<RolesEffect> {
        let helper = self.run.as_ref().and_then(|run| {
            run.jobs
                .iter()
                .position(|job| job.status == JobStatus::Confirm)
        });
        self.mode = Mode::Applying;
        if let Some(index) = helper
            && let Some(run) = self.run.as_mut()
        {
            let job = &mut run.jobs[index];
            job.status = JobStatus::Queued;
            job.assignment.confirmed = true;
            return vec![RolesEffect::Assign {
                step: ApplyStep::Helper {
                    task: job.task.clone(),
                },
                assignment: job.assignment.clone(),
            }];
        }
        let mut assignment = self
            .run
            .as_ref()
            .and_then(|run| run.main_assignment.clone())
            .unwrap_or_else(|| self.main_assignment(true));
        assignment.confirmed = true;
        vec![RolesEffect::Assign {
            step: ApplyStep::MainModel,
            assignment,
        }]
    }

    pub(super) fn confirm_no(&mut self) -> Vec<RolesEffect> {
        self.mode = Mode::Browse;
        let has_jobs = self.run.as_ref().is_some_and(|run| !run.jobs.is_empty());
        if !has_jobs {
            self.run = None;
            return Vec::new();
        }
        if let Some(run) = self.run.as_mut() {
            run.failed = true;
            for job in &mut run.jobs {
                if job.status == JobStatus::Confirm {
                    job.status = JobStatus::Failed;
                }
            }
        }
        self.notice = vec![self.helper_summary()];
        vec![RolesEffect::ReadBack {
            section: Section::Helpers,
        }]
    }

    pub(super) fn accept_stale_offer(&mut self, slots: &[super::StaleSlot]) -> Vec<RolesEffect> {
        for slot in slots {
            self.edits.helpers.retain(|edit| edit.task != slot.task);
            self.edits.helpers.push(HelperEdit {
                task: slot.task.clone(),
                route: Some(HelperRoute::Inherit),
                effort: Some(None),
            });
        }
        self.open_review(Section::Helpers, Then::Nothing)
    }

    // ----- read-back -----

    pub(super) fn on_read_back(&mut self, section: Section, reads: &[Loaded]) -> Vec<RolesEffect> {
        self.absorb(reads);
        let run = self.run.take_if(|run| run.section == section);
        self.drop_satisfied(section, run.as_ref());
        if matches!(self.mode, Mode::Applying) {
            self.mode = Mode::Browse;
        }
        let Some(run) = run else {
            return Vec::new();
        };
        if run.failed || self.pending(section) {
            return Vec::new();
        }
        self.notice = vec![text::SAVED_NOTICE.to_owned()];
        if section == Section::Main && !run.stale.is_empty() {
            self.mode = Mode::Prompt(Prompt::StaleOffer(run.stale));
            return Vec::new();
        }
        self.run_then(run.then)
    }

    pub(super) fn on_read_back_failed(
        &mut self,
        section: Section,
        error: &RolesError,
    ) -> Vec<RolesEffect> {
        let run = self.run.take_if(|run| run.section == section);
        let mut lines = vec![text::READ_BACK_FAILED.to_owned()];
        if let RolesError::SignIn = error {
            lines = super::errors::read_lines(error, "the current values");
        }
        self.notice = lines;
        if matches!(self.mode, Mode::Applying) {
            self.mode = Mode::Browse;
        }
        let _ = run;
        Vec::new()
    }

    /// Drops every edit the read-back now shows, so pending is exactly what is not saved yet.
    fn drop_satisfied(&mut self, section: Section, run: Option<&Run>) {
        match section {
            Section::Main => {
                let current = self.main_route();
                if let Some(route) = &self.edits.main_route {
                    let wanted = Some((route.provider.clone(), route.model.clone()));
                    let saved = run.and_then(|run| run.saved_route.clone());
                    if current == wanted || (saved.is_some() && current == saved) {
                        self.edits.main_route = None;
                    }
                }
                if let Some(raw) = self.raw.value() {
                    if self.edits.main_context == Some(raw.model_context_length) && run.is_some() {
                        self.edits.main_context = None;
                    }
                    let (effort, read_only) = effort_from_raw(&raw.agent_reasoning_effort);
                    if self.edits.main_effort == Some(effort) && !read_only && run.is_some() {
                        self.edits.main_effort = None;
                    }
                }
            }
            Section::Helpers => {
                let slots = self.helper_slots();
                self.edits.helpers.retain(|edit| {
                    let Some(slot) = slots.iter().find(|slot| slot.task == edit.task) else {
                        return true;
                    };
                    let route_done = match &edit.route {
                        None => true,
                        Some(HelperRoute::Inherit) => follows_main(&slot.provider),
                        Some(HelperRoute::Pin(route)) => {
                            slot.provider == route.provider && slot.model == route.model
                        }
                    };
                    let stored = slot.reasoning_effort.as_deref().unwrap_or_default();
                    let effort_done = match edit.effort {
                        None => true,
                        Some(None) => stored.is_empty(),
                        Some(Some(level)) => stored == level.as_wire(),
                    };
                    !(route_done && effort_done)
                });
            }
            Section::Fallbacks => {
                if self
                    .edits
                    .fallbacks
                    .as_ref()
                    .is_some_and(|work| same_chain(work, &self.fallback_base()))
                {
                    self.edits.fallbacks = None;
                }
            }
            Section::Subagents => {
                if self
                    .edits
                    .subagents
                    .as_ref()
                    .is_some_and(|work| same_roles(work, &self.sub_base()))
                {
                    self.edits.subagents = None;
                }
            }
            Section::Moa => {
                if self.edits.moa.as_ref() == self.moa.value() {
                    self.edits.moa = None;
                }
            }
        }
    }
}

fn helper_assignment(edit: &HelperEdit, slot: Option<&AuxSlot>) -> Assignment {
    let (provider, model, base_url) = match &edit.route {
        Some(HelperRoute::Inherit) => ("auto".to_owned(), String::new(), String::new()),
        Some(HelperRoute::Pin(route)) => (
            route.provider.clone(),
            route.model.clone(),
            route.base_url.clone(),
        ),
        None => match slot {
            Some(slot) if !slot.provider.trim().is_empty() => {
                (slot.provider.clone(), slot.model.clone(), String::new())
            }
            _ => ("auto".to_owned(), String::new(), String::new()),
        },
    };
    let reasoning_effort = match edit.effort {
        None => EffortChange::Omit,
        Some(None) => EffortChange::Inherit,
        Some(Some(level)) => EffortChange::Level(level),
    };
    Assignment {
        scope: AssignScopeView::Auxiliary,
        task: edit.task.clone(),
        provider,
        model,
        reasoning_effort,
        base_url,
        confirmed: false,
    }
}
