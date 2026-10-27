use super::*;

impl MemoryState {
    pub fn view(&self, now: Instant) -> MemoryView {
        let providers = self.status_section(|status| {
            let mut rows = vec![RowView {
                marker: if status.active.is_empty() {
                    '●'
                } else {
                    '○'
                },
                name: "built-in only".into(),
                description: "Hermes's own MEMORY.md and USER.md".into(),
                label: if status.active.is_empty() {
                    "in use"
                } else {
                    "available"
                }
                .into(),
            }];
            for provider in &status.providers {
                let active = provider.name == status.active;
                rows.push(RowView {
                    marker: if active { '●' } else { '○' },
                    name: provider.name.clone(),
                    description: provider.description.clone(),
                    label: if active {
                        "in use".into()
                    } else {
                        provider_label(&provider.status)
                    },
                });
            }
            if !status.active.is_empty()
                && !status.providers.iter().any(|p| p.name == status.active)
            {
                rows.push(RowView {
                    marker: '●',
                    name: status.active.clone(),
                    description: String::new(),
                    label: "not installed".into(),
                });
            }
            rows
        });
        let files = self.status_section(|status| {
            vec![
                RowView {
                    marker: ' ',
                    name: "Agent memory (MEMORY.md)".into(),
                    description: String::new(),
                    label: format_bytes(status.builtin_files.memory),
                },
                RowView {
                    marker: ' ',
                    name: "User profile (USER.md)".into(),
                    description: String::new(),
                    label: format_bytes(status.builtin_files.user),
                },
            ]
        });
        let curator = if !self.connected {
            Section::Failed(DISCONNECTED.into())
        } else {
            match &self.curator {
                None => Section::Loading,
                Some(Err(reason)) => Section::Failed(format!(
                    "Could not read curator status: {}. ctrl+r tries again.",
                    reason.0
                )),
                Some(Ok(curator)) => {
                    let (status, schedule, last_run) = curator_lines(curator);
                    Section::Ready(CuratorRowView {
                        status,
                        schedule,
                        last_run,
                    })
                }
            }
        };
        MemoryView {
            title: format!("Memory · profile {}", self.profile),
            banner: self.banner.clone(),
            providers,
            active_text: format!("active: {}", provider_name(self.active())),
            files,
            curator,
            cursor: self.cursor,
            card: self.card_view(),
            confirm: self.confirm.as_ref().map(|confirm| confirm.view.clone()),
            following: self.follow.as_ref().map(|follow| FollowView {
                elapsed_s: now.saturating_duration_since(follow.started).as_secs(),
                lines: follow.lines.clone(),
            }),
            editing: self.editor.is_some(),
            footer: self.footer(),
        }
    }

    fn status_section<T>(&self, ready: impl FnOnce(&StatusView) -> T) -> Section<T> {
        if !self.connected {
            return Section::Failed(DISCONNECTED.into());
        }
        match &self.status {
            None => Section::Loading,
            Some(Err(reason)) => Section::Failed(format!(
                "Could not read memory status: {}. ctrl+r tries again.",
                reason.0
            )),
            Some(Ok(status)) => Section::Ready(ready(status)),
        }
    }
    fn card_view(&self) -> Option<CardView> {
        let card = self.card.as_ref()?;
        let provider = self.provider(&card.name);
        let status = provider.map_or("missing", |provider| provider.status.as_str());
        let mut needs = Vec::new();
        if let Some(provider) = provider {
            let setup = &provider.setup;
            if !setup.required_env.is_empty() {
                needs.push(format!(
                    "Needs environment value: {}",
                    setup.required_env.join(", ")
                ));
            }
            if !setup.dependencies_installed {
                if !setup.external_dependencies.is_empty() {
                    needs.push(format!(
                        "Needs program: {}",
                        setup
                            .external_dependencies
                            .iter()
                            .map(|d| d.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                if !setup.pip_dependencies.is_empty() {
                    needs.push(format!(
                        "Needs Python packages: {}",
                        setup.pip_dependencies.join(", ")
                    ));
                }
            }
        }
        match &card.config {
            Some(Err(reason)) => needs.push(format!(
                "Could not read settings for {}: {}. ctrl+r tries again.",
                card.name, reason.0
            )),
            Some(Ok(config)) if config.fields.is_empty() => needs.push(format!(
                "{} has no settings Talaria can show. Set it up with {}.",
                card.name,
                self.setup_command(&card.name)
            )),
            None => needs.push(format!("Loading settings for {}…", card.name)),
            _ => {}
        }
        if let Some(editor) = &self.editor
            && let Some(field) = self
                .config()
                .and_then(|config| config.fields.iter().find(|field| field.key == editor.key))
            && field.kind == FieldKind::Select
        {
            let selected = match &editor.text {
                EditorText::Plain(text) => text.as_str(),
                EditorText::Secret(_) => "",
            };
            for option in &field.options {
                let marker = if option.value == selected { '>' } else { ' ' };
                let label = if option.label.is_empty() {
                    &option.value
                } else {
                    &option.label
                };
                let description = if option.description.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", option.description)
                };
                needs.push(format!("{marker} {label}{description}"));
            }
        }
        let fields = self
            .visible_fields()
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let pending = card.pending.plain.contains_key(&field.key)
                    || card.pending.secrets.contains_key(&field.key);
                let editing = self
                    .editor
                    .as_ref()
                    .filter(|editor| editor.key == field.key);
                let value = if let Some(editor) = editing {
                    match &editor.text {
                        EditorText::Secret(secret) => masked(secret.expose()),
                        EditorText::Plain(text) => text.clone(),
                    }
                } else if field.kind == FieldKind::Secret {
                    if card.pending.secrets.contains_key(&field.key) {
                        "•••••••• (new)".into()
                    } else if field.is_set {
                        format!("set · clear it with {}", self.setup_command(&card.name))
                    } else {
                        "not set".into()
                    }
                } else {
                    let value = card.pending.plain.get(&field.key).unwrap_or(&field.value);
                    if pending && value_text(value).is_empty() {
                        "(default)".into()
                    } else if field.kind == FieldKind::Bool {
                        if value_text(value) == "true" {
                            "on".into()
                        } else {
                            "off".into()
                        }
                    } else {
                        shown_value(value)
                    }
                };
                let mut label = field.label.clone();
                if field.required
                    && self
                        .config()
                        .is_some_and(|config| config.surface == ConfigSurface::Plain)
                {
                    label.push_str(" *");
                }
                if card.failed.contains(&field.key) {
                    label.insert_str(0, "! ");
                }
                FieldRowView {
                    label,
                    value,
                    pending,
                    selected: index == card.cursor,
                }
            })
            .collect();
        Some(CardView {
            name: card.name.clone(),
            label: provider_label(status),
            description: provider.map_or_else(String::new, |p| p.description.clone()),
            needs,
            fields,
            setup: if matches!(status, "needs_config" | "unavailable" | "missing") {
                Some(format!(
                    "Set up outside Talaria: {}",
                    self.setup_command(&card.name)
                ))
            } else {
                None
            },
            result_lines: card.results.clone(),
        })
    }

    fn footer(&self) -> Vec<(String, String)> {
        let pairs: Vec<(&str, &str)> = if self.confirm.is_some() {
            vec![
                ("left/right", "choose"),
                ("enter", "confirm"),
                ("esc", "cancel"),
            ]
        } else if self.editor.is_some() {
            vec![("enter", "apply"), ("esc", "cancel")]
        } else if self.card.is_some() {
            vec![
                ("enter", "edit"),
                ("ctrl+s", "save"),
                ("u", "use"),
                ("c", "copy command"),
                ("s", "memory settings"),
                ("k", "curator settings"),
                ("ctrl+r", "reload"),
                ("esc", "back"),
            ]
        } else {
            let count = self.provider_names().len();
            let mut pairs = if self.cursor < count {
                vec![("enter", "open"), ("u", "use")]
            } else if self.cursor < count + 2 {
                vec![("d", "delete file"), ("D", "delete both")]
            } else {
                vec![("p", "pause"), ("R", "run now")]
            };
            pairs.extend([
                ("s", "memory settings"),
                ("k", "curator settings"),
                ("ctrl+r", "reload"),
                ("esc", "close"),
            ]);
            pairs
        };
        pairs
            .into_iter()
            .map(|(key, label)| (key.into(), label.into()))
            .collect()
    }
}

fn masked(text: &str) -> String {
    let count = text.chars().count();
    let mut masked = "•".repeat(count.min(40));
    if count > 40 {
        masked.push('…');
    }
    masked
}
