//! Everything the terminal draws, built from the state.

use super::items::{Item, Kind, fit, window};
use super::state::{
    CatalogState, ChoiceTarget, InputTarget, Link, Mode, ModelRolesState, PickTarget, PickerMode,
    Prompt, SECTIONS, index_of,
};
use super::{
    ChainMode, DiffLine, ModalView, PickerStep, RoleRoute, RolesView, RoutePickerView, RouteRow,
    RowView, Section, TabView, filter_routes, text,
};

/// One row of the route picker.
pub(super) enum Entry {
    Provider {
        slug: String,
        name: String,
        choosable: bool,
    },
    Model {
        provider: String,
        model: String,
        api_url: String,
    },
    /// The route already stored, which the catalogue does not list.
    Stored { provider: String, model: String },
}

impl ModelRolesState {
    pub(super) fn build_view(&self) -> RolesView {
        let title = format!("{}{}", text::TITLE_PREFIX, self.profile);
        let tabs = SECTIONS
            .iter()
            .map(|section| {
                let stale = self.tab_stale(*section);
                let mut label = text::tab_label(*section);
                if stale {
                    label.push_str(" (stale)");
                }
                TabView {
                    label,
                    current: *section == self.section,
                    stale,
                }
            })
            .collect();
        let mut view = RolesView {
            title,
            tabs,
            rows: Vec::new(),
            detail: None,
            focus_detail: false,
            notice: Vec::new(),
            footer: Vec::new(),
            modal: None,
            picker: None,
            raw_keys: false,
        };
        match &self.link {
            Link::NotConnected => {
                view.rows = vec![plain_row(text::NOT_CONNECTED)];
                return view;
            }
            Link::NotServed(name) => {
                view.rows = vec![plain_row(&text::not_served(name))];
                view.footer = vec!["Ctrl+R retry  Esc close".to_owned()];
                return view;
            }
            Link::Connected => {}
        }
        let section = self.section;
        let index = index_of(section);
        let items = self.items(section);
        let in_detail = section == Section::Moa && self.focus_detail;
        view.rows = window(&items, self.cursor[index], self.scroll[index], !in_detail);
        if section == Section::Moa && self.moa_work().is_some() {
            let detail = self.detail_items();
            view.detail = Some(window(
                &detail,
                self.detail_cursor,
                self.detail_scroll,
                in_detail,
            ));
            view.focus_detail = in_detail;
        }
        view.notice = self.notice_lines();
        view.footer = self.footer_lines();
        view.modal = self.modal_view();
        view.picker = self.picker_view();
        view.raw_keys = matches!(
            self.mode,
            Mode::Prompt(_) | Mode::Input(_) | Mode::Picker(_)
        );
        view
    }

    fn notice_lines(&self) -> Vec<String> {
        let mut out = self.notice.clone();
        if matches!(self.mode, Mode::Applying) {
            out.push(text::APPLYING.to_owned());
        }
        if matches!(&self.mode, Mode::Review(review) if review.go) {
            out.push(text::CHECKING.to_owned());
        }
        let section = self.section;
        if matches!(self.status(section), super::items::Status::Ready) {
            out.extend(self.problem_lines(section));
        }
        match section {
            Section::Main
                if matches!(self.status(section), super::items::Status::Ready)
                    && self.main_route().is_none()
                    && self.edits.main_route.is_none()
                    && (self.info.value().is_some() || self.aux.value().is_some()) =>
            {
                out.push(text::NO_MAIN.to_owned());
            }
            Section::Moa if self.raw_value().is_some_and(|raw| !raw.has_moa_presets) => {
                out.push(text::MOA_NOTICE.to_owned());
            }
            _ => {}
        }
        if self.pending(section) {
            out.push(text::UNAPPLIED.to_owned());
        }
        out
    }

    fn footer_lines(&self) -> Vec<String> {
        if self.help {
            return text::KEYS_HELP
                .iter()
                .map(|line| (*line).to_owned())
                .collect();
        }
        match &self.mode {
            Mode::Prompt(Prompt::Pending { .. }) => {
                return vec!["a apply  d discard  Esc stay".to_owned()];
            }
            Mode::Prompt(_) => return vec!["y yes  n no  Esc cancel".to_owned()],
            Mode::Review(_) => return vec![text::REVIEW_FOOTER.to_owned()],
            Mode::Input(_) => return vec!["Enter accept  Esc cancel".to_owned()],
            Mode::Choice(_) => return vec!["Up Down choose  Enter accept  Esc cancel".to_owned()],
            Mode::Picker(_) => {
                return vec![
                    "Type to filter  Enter choose  Backspace back  Ctrl+R refresh  Esc cancel"
                        .to_owned(),
                ];
            }
            Mode::Applying | Mode::Browse => {}
        }
        let keys = match self.section {
            Section::Main => {
                "Enter change  c context  e effort  Ctrl+S review  u discard  Ctrl+R reload  Esc close  ? keys"
            }
            Section::Fallbacks => {
                "Enter change  a add  x remove  K J move  Ctrl+S review  u discard  Ctrl+R reload  Esc close  ? keys"
            }
            Section::Helpers => {
                "Enter open  e effort  i inherit  R reset stale  Ctrl+S review  u discard  Ctrl+R reload  Esc close  ? keys"
            }
            Section::Subagents => {
                "Enter change  e effort  i inherit  m chain  a add  x remove  Ctrl+S review  u discard  Ctrl+R reload  Esc close  ? keys"
            }
            Section::Moa => {
                "Left Right pane  Enter change  Space toggle  a add  x remove  D default  Ctrl+S review  u discard  Ctrl+R reload  Esc close  ? keys"
            }
        };
        let mut out = vec![keys.to_owned()];
        if self.section != Section::Main {
            out.push(format!("{} {}", text::APPLIES_SHORT, text::RUNNING_FOOTER));
        }
        out
    }

    fn modal_view(&self) -> Option<ModalView> {
        match &self.mode {
            Mode::Browse | Mode::Applying | Mode::Picker(_) => None,
            Mode::Prompt(prompt) => Some(ModalView::Prompt {
                lines: self.prompt_lines(prompt),
            }),
            Mode::Review(review) => {
                let mut lines = self.diff_lines(review.section);
                if lines.is_empty() {
                    lines.push(DiffLine {
                        key: text::NO_CHANGES.to_owned(),
                        before: String::new(),
                        after: String::new(),
                    });
                }
                Some(ModalView::Review {
                    title: format!(
                        "Review changes to {} - profile {}",
                        text::section_name(review.section),
                        self.profile
                    ),
                    lines,
                    footer: text::REVIEW_FOOTER.to_owned(),
                })
            }
            Mode::Input(input) => {
                let label = match input.target {
                    InputTarget::Context => "Context window in tokens (0 or empty for auto): ",
                    InputTarget::PresetName => "Name: ",
                    InputTarget::FanoutEvery => {
                        "Run advisors every how many tool steps (2 or more): "
                    }
                    InputTarget::Timeout => "Advisor timeout in seconds (empty to inherit): ",
                    InputTarget::ReferenceTemperature => {
                        "Reference temperature (empty for provider default): "
                    }
                    InputTarget::AggregatorTemperature => {
                        "Aggregator temperature (empty for provider default): "
                    }
                };
                Some(ModalView::Input {
                    label: label.to_owned(),
                    value: input.value.clone(),
                    error: input.error.clone(),
                })
            }
            Mode::Choice(choice) => {
                let title = match choice.target {
                    ChoiceTarget::Fanout => "Advisors run",
                    _ => "Effort",
                };
                Some(ModalView::Choice {
                    title: title.to_owned(),
                    options: choice.options.clone(),
                    selected: choice.selected,
                })
            }
        }
    }

    fn prompt_lines(&self, prompt: &Prompt) -> Vec<String> {
        match prompt {
            Prompt::Pending { section, .. } => {
                vec![text::pending_prompt(text::section_name(*section))]
            }
            Prompt::Confirm { message } => {
                vec![message.clone(), text::CONFIRM_PROMPT.to_owned()]
            }
            Prompt::StaleOffer(slots) => {
                let names: Vec<String> = slots
                    .iter()
                    .map(|slot| format!("{} ({})", slot.task, slot.provider))
                    .collect();
                vec![text::stale_offer(&names.join(", "))]
            }
            Prompt::DeletePreset(index) => {
                let name = self
                    .moa_work()
                    .and_then(|config| config.presets.get(*index).map(|preset| preset.name.clone()))
                    .unwrap_or_default();
                vec![format!("Delete preset {name}? y/n")]
            }
        }
    }

    /// The route the picker's target holds now, for the "not in the model list" row.
    fn target_route(&self, target: &PickTarget) -> Option<(String, String)> {
        match target {
            PickTarget::Main => self
                .edits
                .main_route
                .as_ref()
                .map(|route| (route.provider.clone(), route.model.clone()))
                .or_else(|| self.main_route()),
            PickTarget::Fallback(Some(index)) => self
                .fallback_work()
                .get(*index)
                .map(|entry| (entry.provider.clone(), entry.model.clone())),
            PickTarget::Helper(task) => self
                .helper_slots()
                .into_iter()
                .find(|slot| &slot.task == task)
                .map(|slot| (slot.provider, slot.model)),
            PickTarget::SubRoute => match self.sub_work().route {
                RoleRoute::Pinned { provider, model } => Some((provider, model)),
                _ => None,
            },
            PickTarget::SubChain(Some(index)) => match self.sub_work().chain {
                ChainMode::Own(entries) => entries
                    .get(*index)
                    .map(|entry| (entry.provider.clone(), entry.model.clone())),
                _ => None,
            },
            PickTarget::MoaReference(index) => self
                .selected_preset()
                .and_then(|(_, preset)| preset.references.get(*index).cloned())
                .map(|slot| (slot.provider, slot.model)),
            PickTarget::MoaAggregator => self
                .selected_preset()
                .map(|(_, preset)| (preset.aggregator.provider, preset.aggregator.model)),
            _ => None,
        }
    }

    pub(super) fn picker_entries(&self, picker: &PickerMode) -> Vec<Entry> {
        let CatalogState::Ready(catalog) = &self.catalog else {
            return Vec::new();
        };
        let mut out: Vec<Entry> = Vec::new();
        if let PickerStep::Models { provider } = &picker.step
            && let Some((stored_provider, stored_model)) = self.target_route(&picker.target)
            && &stored_provider == provider
            && !stored_model.is_empty()
            && !catalog.lists(provider, &stored_model)
            && stored_model
                .to_lowercase()
                .contains(&picker.filter.trim().to_lowercase())
        {
            out.push(Entry::Stored {
                provider: stored_provider,
                model: stored_model,
            });
        }
        out.extend(
            filter_routes(catalog, &picker.step, &picker.filter)
                .into_iter()
                .map(|row| match row {
                    RouteRow::Provider {
                        slug,
                        name,
                        choosable,
                    } => Entry::Provider {
                        slug,
                        name,
                        choosable,
                    },
                    RouteRow::Model {
                        provider,
                        model,
                        api_url,
                    } => Entry::Model {
                        provider,
                        model,
                        api_url,
                    },
                }),
        );
        out
    }

    fn picker_view(&self) -> Option<RoutePickerView> {
        let Mode::Picker(picker) = &self.mode else {
            return None;
        };
        let entries = self.picker_entries(picker);
        let items: Vec<Item> = entries
            .iter()
            .map(|entry| match entry {
                Entry::Provider {
                    name, choosable, ..
                } => {
                    let line = if *choosable {
                        name.clone()
                    } else {
                        format!("{name}  {}", text::NOT_SET_UP)
                    };
                    Item {
                        dim: !*choosable,
                        ..Item::pick(Kind::MainModel, line)
                    }
                }
                Entry::Model { model, .. } => Item::pick(Kind::MainModel, model.clone()),
                Entry::Stored { model, .. } => Item {
                    marker: Some('!'),
                    ..Item::pick(Kind::MainModel, format!("{model} {}", text::NOT_IN_LIST))
                },
            })
            .collect();
        let mut scroll = 0;
        fit(&mut scroll, picker.cursor, items.len());
        let title = match &picker.step {
            PickerStep::Providers => "Choose a provider".to_owned(),
            PickerStep::Models { provider } => {
                let name = match &self.catalog {
                    CatalogState::Ready(catalog) => catalog
                        .provider(provider)
                        .map_or(provider.as_str(), |row| row.name.as_str())
                        .to_owned(),
                    _ => provider.clone(),
                };
                format!("Choose a model - {name}")
            }
        };
        let status = if picker.refreshing {
            Some(text::REFRESHING.to_owned())
        } else {
            match &self.catalog {
                CatalogState::Pending => Some(text::LOADING_MODELS.to_owned()),
                CatalogState::Failed => Some(text::CATALOG_FAILED.to_owned()),
                CatalogState::Ready(_) => None,
            }
        };
        Some(RoutePickerView {
            title,
            filter: picker.filter.clone(),
            rows: window(&items, picker.cursor, scroll, true),
            status,
        })
    }
}

fn plain_row(text: &str) -> RowView {
    RowView {
        text: text.to_owned(),
        selected: false,
        dimmed: false,
        marker: None,
    }
}
