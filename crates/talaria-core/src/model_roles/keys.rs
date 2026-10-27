//! Key handling: the page, its prompts and inputs, and the route picker.

use super::helpers::stale_helpers;
use super::items::{Kind, fit, settle_cursor, step};
use super::moa::{check_name, empty_slot, new_preset};
use super::state::{
    CatalogState, ChoiceMode, ChoiceTarget, ChosenRoute, Edits, HelperEdit, HelperRoute, InputMode,
    InputTarget, Link, Mode, ModelRolesState, PAGE, PickTarget, PickerMode, Prompt, SECTIONS, Then,
    index_of,
};
use super::view::Entry;
use super::{
    ChainMode, FallbackEntry, MoaConfigView, PickerStep, RoleRoute, RolesEffect, RolesKey, Section,
    text,
};
use crate::models::Effort;

impl ModelRolesState {
    pub(super) fn on_key(&mut self, key: RolesKey) -> Vec<RolesEffect> {
        if !matches!(self.link, Link::Connected) {
            return match key {
                RolesKey::Reload => self.reload(),
                RolesKey::Escape => vec![RolesEffect::Close],
                _ => Vec::new(),
            };
        }
        if key == RolesKey::Help {
            self.help = !self.help;
            return Vec::new();
        }
        let help_was_open = self.help;
        self.help = false;
        if help_was_open && key == RolesKey::Escape {
            return Vec::new();
        }
        self.notice.clear();
        match key {
            RolesKey::Reload => return self.key_reload(),
            RolesKey::Review => return self.key_review(),
            _ => {}
        }
        let mode = std::mem::replace(&mut self.mode, Mode::Browse);
        match mode {
            Mode::Browse => self.browse_key(&key),
            Mode::Applying => {
                self.mode = Mode::Applying;
                Vec::new()
            }
            Mode::Prompt(prompt) => self.prompt_key(prompt, &key),
            Mode::Review(review) => self.review_key(review, &key),
            Mode::Picker(picker) => self.picker_key(picker, &key),
            Mode::Input(input) => self.input_key(input, &key),
            Mode::Choice(choice) => self.choice_key(choice, &key),
        }
    }

    fn key_reload(&mut self) -> Vec<RolesEffect> {
        match &mut self.mode {
            Mode::Applying => return Vec::new(),
            Mode::Picker(picker) => {
                picker.refreshing = true;
                return vec![RolesEffect::LoadCatalog { refresh: true }];
            }
            _ => {}
        }
        if let Some(section) = self.pending_section() {
            self.mode = Mode::Prompt(Prompt::Pending {
                section,
                then: Then::Reload,
            });
            return Vec::new();
        }
        self.mode = Mode::Browse;
        self.reload()
    }

    fn key_review(&mut self) -> Vec<RolesEffect> {
        if matches!(self.mode, Mode::Applying) {
            return Vec::new();
        }
        self.mode = Mode::Browse;
        let section = self.section;
        self.open_review(section, Then::Nothing)
    }

    // ----- the page -----

    fn browse_key(&mut self, key: &RolesKey) -> Vec<RolesEffect> {
        match key {
            RolesKey::Section(section) => self.switch_section(*section),
            RolesKey::NextSection | RolesKey::PrevSection => {
                let at = index_of(self.section);
                let to = if *key == RolesKey::NextSection {
                    (at + 1) % SECTIONS.len()
                } else {
                    (at + SECTIONS.len() - 1) % SECTIONS.len()
                };
                self.switch_section(SECTIONS[to])
            }
            RolesKey::Up
            | RolesKey::Down
            | RolesKey::Home
            | RolesKey::End
            | RolesKey::PageUp
            | RolesKey::PageDown => {
                self.navigate(key);
                Vec::new()
            }
            RolesKey::Left => {
                self.focus_detail = false;
                Vec::new()
            }
            RolesKey::Right => {
                if self.section == Section::Moa && !self.detail_items().is_empty() {
                    self.focus_detail = true;
                }
                Vec::new()
            }
            RolesKey::Enter => self.activate(),
            RolesKey::Effort => self.key_effort(),
            RolesKey::Context => {
                if self.section == Section::Main {
                    self.open_input(InputTarget::Context, String::new());
                }
                Vec::new()
            }
            RolesKey::Inherit => self.key_inherit(),
            RolesKey::Add => self.key_add(),
            RolesKey::Remove => self.key_remove(),
            RolesKey::MoveUp | RolesKey::MoveDown => self.key_move(*key == RolesKey::MoveUp),
            RolesKey::Toggle => self.key_toggle(),
            RolesKey::MakeDefault => self.key_make_default(),
            RolesKey::ChainMode => self.key_chain_mode(),
            RolesKey::ResetStale => self.key_reset_stale(),
            RolesKey::Discard => {
                self.edits = Edits::default();
                Vec::new()
            }
            RolesKey::Escape => match self.pending_section() {
                Some(section) => {
                    self.mode = Mode::Prompt(Prompt::Pending {
                        section,
                        then: Then::Close,
                    });
                    Vec::new()
                }
                None => vec![RolesEffect::Close],
            },
            RolesKey::Review
            | RolesKey::Reload
            | RolesKey::Help
            | RolesKey::Char(_)
            | RolesKey::Backspace => Vec::new(),
        }
    }

    fn switch_section(&mut self, target: Section) -> Vec<RolesEffect> {
        if target == self.section {
            return Vec::new();
        }
        if let Some(section) = self.pending_section() {
            self.mode = Mode::Prompt(Prompt::Pending {
                section,
                then: Then::Section(target),
            });
            return Vec::new();
        }
        self.section = target;
        self.focus_detail = false;
        Vec::new()
    }

    fn navigate(&mut self, key: &RolesKey) {
        if self.section == Section::Moa && self.focus_detail {
            let items = self.detail_items();
            self.detail_cursor = step(&items, self.detail_cursor, key);
            fit(&mut self.detail_scroll, self.detail_cursor, items.len());
            return;
        }
        let index = index_of(self.section);
        let items = self.items(self.section);
        let before = self.cursor[index];
        self.cursor[index] = step(&items, before, key);
        fit(&mut self.scroll[index], self.cursor[index], items.len());
        if self.section == Section::Moa && self.cursor[index] != before {
            self.detail_cursor = 0;
            self.detail_scroll = 0;
        }
    }

    /// The kind of the row under the cursor, in whichever pane has focus.
    fn current_kind(&self) -> Kind {
        let (items, cursor) = if self.section == Section::Moa && self.focus_detail {
            (self.detail_items(), self.detail_cursor)
        } else {
            (
                self.items(self.section),
                self.cursor[index_of(self.section)],
            )
        };
        items
            .get(settle_cursor(&items, cursor))
            .map_or(Kind::None, |item| item.kind.clone())
    }

    fn activate(&mut self) -> Vec<RolesEffect> {
        let mut kind = self.current_kind();
        if kind == Kind::None && self.section == Section::Main {
            // The main model can be chosen before the reads answer.
            kind = Kind::MainModel;
        }
        match kind {
            Kind::MainModel => self.open_picker(PickTarget::Main, false),
            Kind::MainContext => self.open_input(InputTarget::Context, String::new()),
            Kind::MainEffort | Kind::SubEffort => {
                self.key_effort();
            }
            Kind::Fallback(index) => self.open_picker(PickTarget::Fallback(Some(index)), false),
            Kind::Helper(index) => {
                if let Some(slot) = self.helper_slots().get(index) {
                    self.open_picker(PickTarget::Helper(slot.task.clone()), false);
                }
            }
            Kind::SubRoute => self.open_picker(PickTarget::SubRoute, false),
            Kind::SubChainMode => return self.key_chain_mode(),
            Kind::SubChain(index) => self.open_picker(PickTarget::SubChain(Some(index)), false),
            Kind::Preset(_) => {
                if !self.detail_items().is_empty() {
                    self.focus_detail = true;
                }
            }
            Kind::MoaEnabled | Kind::MoaPolicy => return self.key_toggle(),
            Kind::MoaReference(index) => self.open_picker(PickTarget::MoaReference(index), false),
            Kind::MoaAggregator => self.open_picker(PickTarget::MoaAggregator, false),
            Kind::MoaFanout => {
                let current =
                    self.selected_preset()
                        .map_or(0, |(_, preset)| {
                            match super::moa::parse_fanout(&preset.fanout) {
                                super::moa::Fanout::UserTurn => 0,
                                super::moa::Fanout::PerIteration => 1,
                                super::moa::Fanout::EveryN(_) => 2,
                            }
                        });
                self.mode = Mode::Choice(ChoiceMode {
                    target: ChoiceTarget::Fanout,
                    options: text::FANOUT_CHOICES
                        .iter()
                        .map(|word| (*word).to_owned())
                        .collect(),
                    selected: current,
                });
            }
            Kind::MoaTimeout => self.open_input(InputTarget::Timeout, String::new()),
            Kind::MoaReferenceTemperature => {
                self.open_input(InputTarget::ReferenceTemperature, String::new());
            }
            Kind::MoaAggregatorTemperature => {
                self.open_input(InputTarget::AggregatorTemperature, String::new());
            }
            Kind::None => {}
        }
        Vec::new()
    }

    fn open_picker(&mut self, target: PickTarget, escape_disables_chain: bool) {
        self.mode = Mode::Picker(PickerMode {
            target,
            step: PickerStep::Providers,
            filter: String::new(),
            cursor: 0,
            refreshing: false,
            escape_disables_chain,
        });
    }

    fn open_input(&mut self, target: InputTarget, value: String) {
        self.mode = Mode::Input(InputMode {
            target,
            value,
            error: None,
        });
    }

    fn open_effort_choice(&mut self, target: ChoiceTarget, current: Option<Option<Effort>>) {
        let selected = match current {
            Some(Some(level)) => text::EFFORT_CHOICES
                .iter()
                .position(|word| *word == level.as_wire())
                .unwrap_or(0),
            _ => 0,
        };
        self.mode = Mode::Choice(ChoiceMode {
            target,
            options: text::EFFORT_CHOICES
                .iter()
                .map(|word| (*word).to_owned())
                .collect(),
            selected,
        });
    }

    fn key_effort(&mut self) -> Vec<RolesEffect> {
        match (self.section, self.current_kind()) {
            (Section::Main, _) => {
                if !self.main_before().effort_read_only {
                    let current = self.edits.main_effort.or(Some(self.main_before().effort));
                    self.open_effort_choice(ChoiceTarget::MainEffort, current);
                }
            }
            (Section::Helpers, Kind::Helper(index)) => {
                if let Some(slot) = self.helper_slots().get(index) {
                    let current = self
                        .edits
                        .helpers
                        .iter()
                        .find(|edit| edit.task == slot.task)
                        .and_then(|edit| edit.effort)
                        .or_else(|| Some(slot.reasoning_effort.as_deref().and_then(Effort::parse)));
                    self.open_effort_choice(ChoiceTarget::HelperEffort(slot.task.clone()), current);
                }
            }
            (Section::Subagents, _) => {
                let current = Some(self.sub_work().effort);
                self.open_effort_choice(ChoiceTarget::SubagentEffort, current);
            }
            (Section::Moa, Kind::MoaReference(index)) => {
                let current = self
                    .selected_preset()
                    .and_then(|(_, preset)| preset.references.get(index).cloned())
                    .map(|slot| slot.reasoning_effort.as_deref().and_then(Effort::parse));
                self.open_effort_choice(ChoiceTarget::ReferenceEffort(index), current);
            }
            (Section::Moa, Kind::MoaAggregator) => {
                let current = self.selected_preset().map(|(_, preset)| {
                    preset
                        .aggregator
                        .reasoning_effort
                        .as_deref()
                        .and_then(Effort::parse)
                });
                self.open_effort_choice(ChoiceTarget::AggregatorEffort, current);
            }
            _ => {}
        }
        Vec::new()
    }

    fn helper_edit(&mut self, task: &str) -> &mut HelperEdit {
        let position = self.edits.helpers.iter().position(|edit| edit.task == task);
        let at = position.unwrap_or_else(|| {
            self.edits.helpers.push(HelperEdit {
                task: task.to_owned(),
                route: None,
                effort: None,
            });
            self.edits.helpers.len() - 1
        });
        &mut self.edits.helpers[at]
    }

    fn sub_edit(&mut self) -> &mut super::SubagentRoles {
        let base = self.sub_base();
        self.edits.subagents.get_or_insert(base)
    }

    fn fallback_edit(&mut self) -> &mut Vec<FallbackEntry> {
        let base = self.fallback_base();
        self.edits.fallbacks.get_or_insert(base)
    }

    fn moa_edit(&mut self) -> Option<&mut MoaConfigView> {
        let base = self.moa.value().cloned()?;
        Some(self.edits.moa.get_or_insert(base))
    }

    fn key_inherit(&mut self) -> Vec<RolesEffect> {
        match (self.section, self.current_kind()) {
            (Section::Helpers, Kind::Helper(index)) => {
                if let Some(slot) = self.helper_slots().get(index).cloned()
                    && slot.inherit_from.is_none()
                {
                    let edit = self.helper_edit(&slot.task);
                    edit.route = Some(HelperRoute::Inherit);
                    edit.effort = Some(None);
                }
            }
            (Section::Subagents, Kind::SubRoute) => {
                let roles = self.sub_edit();
                roles.route = RoleRoute::Inherit;
                roles.direct_endpoint = None;
                roles.endpoint_keys.clear();
            }
            (Section::Subagents, Kind::SubEffort) => self.sub_edit().effort = None,
            (Section::Subagents, Kind::SubChainMode | Kind::SubChain(_)) => {
                self.sub_edit().chain = ChainMode::Inherit;
            }
            _ => {}
        }
        Vec::new()
    }

    fn key_add(&mut self) -> Vec<RolesEffect> {
        match self.section {
            Section::Fallbacks => self.open_picker(PickTarget::Fallback(None), false),
            Section::Subagents => self.open_picker(PickTarget::SubChain(None), false),
            Section::Moa => {
                if self.focus_detail {
                    if let Some((_, preset)) = self.selected_preset() {
                        let index = preset.references.len();
                        let name = preset.name;
                        if let Some(config) = self.moa_edit()
                            && let Some(target) = config.presets.iter_mut().find(|p| p.name == name)
                        {
                            target.references.push(empty_slot());
                        }
                        self.open_picker(PickTarget::MoaReference(index), false);
                    }
                } else if self.moa.value().is_some() {
                    self.open_input(InputTarget::PresetName, String::new());
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn key_remove(&mut self) -> Vec<RolesEffect> {
        match (self.section, self.current_kind()) {
            (Section::Fallbacks, Kind::Fallback(index)) => {
                let list = self.fallback_edit();
                if index < list.len() {
                    list.remove(index);
                }
            }
            (Section::Subagents, Kind::SubChain(index)) => {
                if let ChainMode::Own(entries) = &mut self.sub_edit().chain
                    && index < entries.len()
                {
                    entries.remove(index);
                }
            }
            (Section::Moa, Kind::Preset(index)) if !self.focus_detail => {
                let count = self.moa_work().map_or(0, |config| config.presets.len());
                if count <= 1 {
                    self.notice = vec![text::LAST_PRESET.to_owned()];
                } else {
                    self.mode = Mode::Prompt(Prompt::DeletePreset(index));
                }
            }
            (Section::Moa, Kind::MoaReference(index)) => {
                if let Some((_, preset)) = self.selected_preset() {
                    let name = preset.name;
                    if let Some(config) = self.moa_edit()
                        && let Some(target) = config.presets.iter_mut().find(|p| p.name == name)
                        && index < target.references.len()
                    {
                        target.references.remove(index);
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn key_move(&mut self, up: bool) -> Vec<RolesEffect> {
        let index = match (self.section, self.current_kind()) {
            (Section::Fallbacks, Kind::Fallback(index))
            | (Section::Subagents, Kind::SubChain(index)) => index,
            _ => return Vec::new(),
        };
        let target = if up {
            if index == 0 {
                return Vec::new();
            }
            index - 1
        } else {
            index + 1
        };
        let moved = if self.section == Section::Fallbacks {
            let list = self.fallback_edit();
            if target < list.len() {
                list.swap(index, target);
                true
            } else {
                false
            }
        } else if let ChainMode::Own(entries) = &mut self.sub_edit().chain {
            if target < entries.len() {
                entries.swap(index, target);
                true
            } else {
                false
            }
        } else {
            false
        };
        if moved {
            let at = index_of(self.section);
            self.cursor[at] = if up {
                self.cursor[at].saturating_sub(1)
            } else {
                self.cursor[at] + 1
            };
        }
        Vec::new()
    }

    fn key_toggle(&mut self) -> Vec<RolesEffect> {
        if self.section != Section::Moa {
            return Vec::new();
        }
        let Some((_, preset)) = self.selected_preset() else {
            return Vec::new();
        };
        let name = preset.name.clone();
        let kind = self.current_kind();
        let in_detail = self.focus_detail;
        let Some(config) = self.moa_edit() else {
            return Vec::new();
        };
        let Some(target) = config.presets.iter_mut().find(|p| p.name == name) else {
            return Vec::new();
        };
        match kind {
            Kind::Preset(_) if !in_detail => target.enabled = !target.enabled,
            Kind::MoaEnabled => target.enabled = !target.enabled,
            Kind::MoaReference(index) => {
                if let Some(slot) = target.references.get_mut(index) {
                    slot.enabled = Some(slot.enabled == Some(false));
                }
            }
            Kind::MoaPolicy => {
                target.degraded_reference_policy = if target.degraded_reference_policy == "silent" {
                    "loud".to_owned()
                } else {
                    "silent".to_owned()
                };
            }
            _ => {}
        }
        Vec::new()
    }

    fn key_make_default(&mut self) -> Vec<RolesEffect> {
        if self.section == Section::Moa
            && !self.focus_detail
            && let Some((_, preset)) = self.selected_preset()
            && let Some(config) = self.moa_edit()
        {
            config.default_preset = preset.name;
        }
        Vec::new()
    }

    fn key_chain_mode(&mut self) -> Vec<RolesEffect> {
        if self.section != Section::Subagents {
            return Vec::new();
        }
        let roles = self.sub_edit();
        match roles.chain {
            ChainMode::Inherit => roles.chain = ChainMode::Disabled,
            ChainMode::Disabled => {
                roles.chain = ChainMode::Own(Vec::new());
                self.open_picker(PickTarget::SubChain(None), true);
            }
            ChainMode::Own(_) => roles.chain = ChainMode::Inherit,
        }
        Vec::new()
    }

    fn key_reset_stale(&mut self) -> Vec<RolesEffect> {
        if self.section != Section::Helpers {
            return Vec::new();
        }
        let stale = stale_helpers(&self.helper_slots(), &self.main_provider());
        if stale.is_empty() {
            self.notice = vec![text::NO_STALE.to_owned()];
            return Vec::new();
        }
        for slot in &stale {
            let edit = self.helper_edit(&slot.task);
            edit.route = Some(HelperRoute::Inherit);
            edit.effort = Some(None);
        }
        self.open_review(Section::Helpers, Then::Nothing)
    }

    // ----- prompts -----

    fn prompt_key(&mut self, prompt: Prompt, key: &RolesKey) -> Vec<RolesEffect> {
        match (&prompt, key) {
            (Prompt::Pending { section, then }, RolesKey::Char('a')) => {
                self.open_review(*section, then.clone())
            }
            (Prompt::Pending { then, .. }, RolesKey::Char('d')) => {
                self.edits = Edits::default();
                self.run_then(then.clone())
            }
            (Prompt::Confirm { .. }, RolesKey::Char('y')) => self.confirm_yes(),
            (Prompt::Confirm { .. }, RolesKey::Char('n') | RolesKey::Escape) => self.confirm_no(),
            (Prompt::StaleOffer(slots), RolesKey::Char('y')) => self.accept_stale_offer(slots),
            (Prompt::DeletePreset(index), RolesKey::Char('y')) => {
                self.delete_preset(*index);
                Vec::new()
            }
            (_, RolesKey::Char('n') | RolesKey::Escape) => Vec::new(),
            (Prompt::Pending { .. }, _)
            | (Prompt::Confirm { .. }, _)
            | (Prompt::StaleOffer(_), _)
            | (Prompt::DeletePreset(_), _) => {
                self.mode = Mode::Prompt(prompt);
                Vec::new()
            }
        }
    }

    fn delete_preset(&mut self, index: usize) {
        let Some(config) = self.moa_edit() else {
            return;
        };
        if config.presets.len() <= 1 || index >= config.presets.len() {
            return;
        }
        let removed = config.presets.remove(index);
        if config.default_preset == removed.name {
            config.default_preset = config
                .presets
                .first()
                .map(|preset| preset.name.clone())
                .unwrap_or_default();
        }
        if config.active_preset == removed.name {
            config.active_preset.clear();
        }
    }

    fn review_key(
        &mut self,
        mut review: super::state::ReviewMode,
        key: &RolesKey,
    ) -> Vec<RolesEffect> {
        match key {
            RolesKey::Enter => {
                if !self.has_changes(review.section) {
                    self.notice = vec![text::NO_CHANGES.to_owned()];
                    return self.run_then(review.then);
                }
                let write = review.fresh;
                review.go = true;
                self.mode = Mode::Review(review);
                if write {
                    self.begin_write()
                } else {
                    Vec::new()
                }
            }
            RolesKey::Escape => Vec::new(),
            _ => {
                self.mode = Mode::Review(review);
                Vec::new()
            }
        }
    }

    // ----- inputs and choices -----

    fn input_key(&mut self, mut input: InputMode, key: &RolesKey) -> Vec<RolesEffect> {
        match key {
            RolesKey::Char(ch) => {
                if !ch.is_control() && input.value.chars().count() < 64 {
                    input.value.push(*ch);
                }
                input.error = None;
            }
            RolesKey::Backspace => {
                input.value.pop();
                input.error = None;
            }
            RolesKey::Escape => return Vec::new(),
            RolesKey::Enter => match self.submit_input(&input) {
                Ok(()) => return Vec::new(),
                Err(message) => input.error = Some(message),
            },
            _ => {}
        }
        self.mode = Mode::Input(input);
        Vec::new()
    }

    fn submit_input(&mut self, input: &InputMode) -> Result<(), String> {
        let value = input.value.trim();
        match input.target {
            InputTarget::Context => {
                let number = if value.is_empty() {
                    0
                } else {
                    value
                        .parse::<u64>()
                        .map_err(|_| "Enter a whole number of tokens.".to_owned())?
                };
                self.edits.main_context = Some(number);
            }
            InputTarget::PresetName => {
                let config = self
                    .moa_work()
                    .ok_or_else(|| text::CHOOSE_ANOTHER.to_owned())?;
                let name = check_name(&config, value).map_err(str::to_owned)?;
                if let Some(config) = self.moa_edit() {
                    config.presets.push(new_preset(&name));
                    let at = config.presets.len() - 1;
                    let index = index_of(Section::Moa);
                    self.cursor[index] = at;
                    self.detail_cursor = 0;
                    self.detail_scroll = 0;
                    self.focus_detail = false;
                }
            }
            InputTarget::FanoutEvery => {
                let count = value
                    .parse::<u32>()
                    .ok()
                    .filter(|count| *count >= 2)
                    .ok_or_else(|| "Enter a whole number of 2 or more.".to_owned())?;
                self.edit_selected_preset(|preset| preset.fanout = format!("every_n:{count}"));
            }
            InputTarget::Timeout => {
                let number =
                    parse_number(value, false, "Enter a number above 0, or leave it empty.")?;
                self.edit_selected_preset(|preset| preset.reference_timeout = number);
            }
            InputTarget::ReferenceTemperature => {
                let number = parse_number(
                    value,
                    true,
                    "Enter a number of 0 or more, or leave it empty.",
                )?;
                self.edit_selected_preset(|preset| preset.reference_temperature = number);
            }
            InputTarget::AggregatorTemperature => {
                let number = parse_number(
                    value,
                    true,
                    "Enter a number of 0 or more, or leave it empty.",
                )?;
                self.edit_selected_preset(|preset| preset.aggregator_temperature = number);
            }
        }
        Ok(())
    }

    fn edit_selected_preset(&mut self, change: impl FnOnce(&mut super::MoaPresetView)) {
        let Some((_, preset)) = self.selected_preset() else {
            return;
        };
        if let Some(config) = self.moa_edit()
            && let Some(target) = config.presets.iter_mut().find(|p| p.name == preset.name)
        {
            change(target);
        }
    }

    fn choice_key(&mut self, mut choice: ChoiceMode, key: &RolesKey) -> Vec<RolesEffect> {
        let last = choice.options.len().saturating_sub(1);
        match key {
            RolesKey::Up => choice.selected = choice.selected.saturating_sub(1),
            RolesKey::Down => choice.selected = (choice.selected + 1).min(last),
            RolesKey::Home => choice.selected = 0,
            RolesKey::End => choice.selected = last,
            RolesKey::Escape => return Vec::new(),
            RolesKey::Enter => {
                self.apply_choice(&choice);
                return Vec::new();
            }
            _ => {}
        }
        self.mode = Mode::Choice(choice);
        Vec::new()
    }

    fn apply_choice(&mut self, choice: &ChoiceMode) {
        let word = choice
            .options
            .get(choice.selected)
            .map_or("", String::as_str);
        let level = Effort::parse(word);
        match &choice.target {
            ChoiceTarget::MainEffort => self.edits.main_effort = Some(level),
            ChoiceTarget::HelperEffort(task) => self.helper_edit(task).effort = Some(level),
            ChoiceTarget::SubagentEffort => self.sub_edit().effort = level,
            ChoiceTarget::ReferenceEffort(index) => {
                let index = *index;
                let wire = level.map(|level| level.as_wire().to_owned());
                self.edit_selected_preset(|preset| {
                    if let Some(slot) = preset.references.get_mut(index) {
                        slot.reasoning_effort = wire;
                    }
                });
            }
            ChoiceTarget::AggregatorEffort => {
                let wire = level.map(|level| level.as_wire().to_owned());
                self.edit_selected_preset(|preset| preset.aggregator.reasoning_effort = wire);
            }
            ChoiceTarget::Fanout => match choice.selected {
                0 => self.edit_selected_preset(|preset| preset.fanout = "user_turn".to_owned()),
                1 => self.edit_selected_preset(|preset| preset.fanout = "per_iteration".to_owned()),
                _ => self.open_input(InputTarget::FanoutEvery, String::new()),
            },
        }
    }

    // ----- the route picker -----

    fn picker_key(&mut self, mut picker: PickerMode, key: &RolesKey) -> Vec<RolesEffect> {
        match key {
            RolesKey::Escape => {
                if picker.escape_disables_chain {
                    self.sub_edit().chain = ChainMode::Disabled;
                }
                return Vec::new();
            }
            RolesKey::Char(ch) => {
                if !ch.is_control() && picker.filter.chars().count() < 64 {
                    picker.filter.push(*ch);
                    picker.cursor = 0;
                }
            }
            RolesKey::Backspace => {
                if picker.filter.is_empty() {
                    if matches!(picker.step, PickerStep::Models { .. }) {
                        picker.step = PickerStep::Providers;
                        picker.cursor = 0;
                    }
                } else {
                    picker.filter.pop();
                    picker.cursor = 0;
                }
            }
            RolesKey::Up
            | RolesKey::Down
            | RolesKey::Home
            | RolesKey::End
            | RolesKey::PageUp
            | RolesKey::PageDown => {
                let len = self.picker_entries(&picker).len();
                let last = len.saturating_sub(1);
                picker.cursor = match key {
                    RolesKey::Up => picker.cursor.saturating_sub(1),
                    RolesKey::Down => (picker.cursor + 1).min(last),
                    RolesKey::Home => 0,
                    RolesKey::End => last,
                    RolesKey::PageUp => picker.cursor.saturating_sub(PAGE),
                    _ => (picker.cursor + PAGE).min(last),
                };
            }
            RolesKey::Enter => {
                let entries = self.picker_entries(&picker);
                match entries.into_iter().nth(picker.cursor) {
                    Some(Entry::Provider {
                        slug,
                        choosable: true,
                        ..
                    }) => {
                        picker.step = PickerStep::Models { provider: slug };
                        picker.filter.clear();
                        picker.cursor = 0;
                    }
                    Some(Entry::Model {
                        provider,
                        model,
                        api_url,
                    }) => {
                        self.apply_pick(&picker.target, provider, model, api_url);
                        return Vec::new();
                    }
                    Some(Entry::Stored { provider, model }) => {
                        let api_url = match &self.catalog {
                            CatalogState::Ready(catalog) => catalog
                                .provider(&provider)
                                .map(|row| row.api_url.clone())
                                .unwrap_or_default(),
                            _ => String::new(),
                        };
                        self.apply_pick(&picker.target, provider, model, api_url);
                        return Vec::new();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        self.mode = Mode::Picker(picker);
        Vec::new()
    }

    fn apply_pick(
        &mut self,
        target: &PickTarget,
        provider: String,
        model: String,
        api_url: String,
    ) {
        match target {
            PickTarget::Main => {
                if self.main_route().as_ref() == Some(&(provider.clone(), model.clone())) {
                    self.edits.main_route = None;
                } else {
                    self.edits.main_route = Some(ChosenRoute {
                        provider,
                        model,
                        base_url: api_url,
                    });
                }
            }
            PickTarget::Fallback(slot) => {
                let list = self.fallback_edit();
                let entry = |old: Option<&FallbackEntry>| FallbackEntry {
                    provider: provider.clone(),
                    model: model.clone(),
                    was_string: old.is_some_and(|old| old.was_string),
                    extra: old.map(|old| old.extra.clone()).unwrap_or_default(),
                };
                match slot {
                    Some(index) if *index < list.len() => list[*index] = entry(list.get(*index)),
                    Some(_) => {}
                    None => list.push(entry(None)),
                }
                let at = list.len().saturating_sub(1);
                if slot.is_none() {
                    self.cursor[index_of(Section::Fallbacks)] = at;
                }
            }
            PickTarget::Helper(task) => {
                let edit = self.helper_edit(task);
                edit.route = Some(HelperRoute::Pin(ChosenRoute {
                    provider,
                    model,
                    base_url: api_url,
                }));
            }
            PickTarget::SubRoute => {
                let roles = self.sub_edit();
                roles.route = RoleRoute::Pinned { provider, model };
                roles.direct_endpoint = None;
                roles.endpoint_keys.clear();
            }
            PickTarget::SubChain(slot) => {
                let roles = self.sub_edit();
                let fresh = FallbackEntry {
                    provider,
                    model,
                    was_string: false,
                    extra: serde_json::Map::new(),
                };
                match (&mut roles.chain, slot) {
                    (ChainMode::Own(entries), Some(index)) if *index < entries.len() => {
                        let old = entries[*index].clone();
                        entries[*index] = FallbackEntry {
                            was_string: old.was_string,
                            extra: old.extra,
                            ..fresh
                        };
                    }
                    (ChainMode::Own(entries), _) => entries.push(fresh),
                    (chain, _) => *chain = ChainMode::Own(vec![fresh]),
                }
            }
            PickTarget::MoaReference(index) => {
                let index = *index;
                self.edit_selected_preset(|preset| {
                    if let Some(slot) = preset.references.get_mut(index) {
                        slot.provider = provider;
                        slot.model = model;
                    }
                });
            }
            PickTarget::MoaAggregator => {
                self.edit_selected_preset(|preset| {
                    preset.aggregator.provider = provider;
                    preset.aggregator.model = model;
                });
            }
        }
    }
}

fn parse_number(value: &str, allow_zero: bool, message: &str) -> Result<Option<f64>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<f64>()
        .ok()
        .filter(|number| {
            number.abs() < f64::INFINITY && (*number > 0.0 || (allow_zero && *number == 0.0))
        })
        .map(Some)
        .ok_or_else(|| message.to_owned())
}
