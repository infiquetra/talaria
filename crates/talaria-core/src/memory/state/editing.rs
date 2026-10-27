use super::*;
use zeroize::Zeroize;

impl MemoryState {
    pub(super) fn config(&self) -> Option<&ConfigView> {
        self.card.as_ref()?.config.as_ref()?.as_ref().ok()
    }
    pub(super) fn visible_fields(&self) -> Vec<&FieldView> {
        let Some(config) = self.config() else {
            return Vec::new();
        };
        let mut values: BTreeMap<String, String> = config
            .fields
            .iter()
            .map(|field| (field.key.clone(), value_text(&field.value)))
            .collect();
        if let Some(card) = &self.card {
            values.extend(
                card.pending
                    .plain
                    .iter()
                    .map(|(key, value)| (key.clone(), value_text(value))),
            );
        }
        config
            .fields
            .iter()
            .filter(|field| field_visible(field, &values))
            .collect()
    }
    pub(super) fn start_editor(&mut self) -> Vec<MemoryEffect> {
        if self.toggle_selected_bool() {
            return Vec::new();
        }
        let cursor = self.card.as_ref().map_or(0, |card| card.cursor);
        let Some(field) = self.visible_fields().get(cursor).copied() else {
            return Vec::new();
        };
        if let FieldKind::Other(kind) = &field.kind {
            self.banner = Some(FieldError::ReadOnly(kind.clone()).to_string());
            return Vec::new();
        }
        let key = field.key.clone();
        let text = if field.kind == FieldKind::Secret {
            EditorText::Secret(SecretField::new(String::with_capacity(65_536)))
        } else {
            let value = self
                .card
                .as_ref()
                .and_then(|card| card.pending.plain.get(&key))
                .unwrap_or(&field.value);
            EditorText::Plain(value_text(value))
        };
        let cursor = match &text {
            EditorText::Plain(s) => s.len(),
            EditorText::Secret(_) => 0,
        };
        self.editor = Some(Editor { key, text, cursor });
        self.banner = None;
        Vec::new()
    }

    pub(super) fn toggle_selected_bool(&mut self) -> bool {
        let cursor = self.card.as_ref().map_or(0, |card| card.cursor);
        let Some(field) = self.visible_fields().get(cursor).copied() else {
            return false;
        };
        if field.kind != FieldKind::Bool {
            return false;
        }
        let key = field.key.clone();
        let stored = value_text(&field.value) == "true";
        let Some(card) = &mut self.card else {
            return false;
        };
        let current = card
            .pending
            .plain
            .get(&key)
            .map_or(stored, |value| value_text(value) == "true");
        let toggled = !current;
        if toggled == stored {
            card.pending.plain.remove(&key);
        } else {
            card.pending.plain.insert(key, Value::Bool(toggled));
        }
        self.banner = None;
        true
    }

    pub(super) fn editor_key(&mut self, key: MemoryKey) -> Vec<MemoryEffect> {
        let Some(mut editor) = self.editor.take() else {
            return Vec::new();
        };
        if key == MemoryKey::Close {
            return Vec::new();
        }
        let Some(field) = self
            .config()
            .and_then(|config| config.fields.iter().find(|field| field.key == editor.key))
            .cloned()
        else {
            return Vec::new();
        };
        if key == MemoryKey::Open {
            if let EditorText::Secret(secret) = editor.text {
                if !secret.expose().trim().is_empty()
                    && let Some(card) = &mut self.card
                {
                    let trimmed = SecretField::new(secret.expose().trim().into());
                    card.pending.secrets.insert(editor.key, trimmed);
                }
                return Vec::new();
            }
            let EditorText::Plain(text) = &editor.text else {
                return Vec::new();
            };
            match validate_input(&field, text) {
                Ok(value) => {
                    if let Some(card) = &mut self.card {
                        if value_text(&value) == value_text(&field.value) {
                            card.pending.plain.remove(&editor.key);
                        } else {
                            card.pending.plain.insert(editor.key, value);
                        }
                    }
                    self.banner = None;
                    return Vec::new();
                }
                Err(error) => {
                    self.banner = Some(error.to_string());
                    self.editor = Some(editor);
                    return Vec::new();
                }
            }
        }
        if field.kind == FieldKind::Bool && key == MemoryKey::Space {
            if let EditorText::Plain(text) = &mut editor.text {
                *text = if value_text(&Value::String(text.clone())) == "true" {
                    "false".into()
                } else {
                    "true".into()
                };
                editor.cursor = text.len();
            }
        } else if field.kind == FieldKind::Select
            && matches!(
                key,
                MemoryKey::Up
                    | MemoryKey::Down
                    | MemoryKey::Left
                    | MemoryKey::Right
                    | MemoryKey::Edit(EditOp::Left | EditOp::Right)
            )
        {
            if let EditorText::Plain(text) = &mut editor.text
                && !field.options.is_empty()
            {
                let selected = field
                    .options
                    .iter()
                    .position(|option| option.value == *text)
                    .unwrap_or(0);
                let down = matches!(
                    key,
                    MemoryKey::Down | MemoryKey::Right | MemoryKey::Edit(EditOp::Right)
                );
                *text = field.options[step(selected, down, field.options.len())]
                    .value
                    .clone();
                editor.cursor = text.len();
            }
        } else {
            let text = match &mut editor.text {
                EditorText::Plain(text) => text,
                EditorText::Secret(secret) => secret.text_mut(),
            };
            let secret = field.kind == FieldKind::Secret;
            match key {
                MemoryKey::Char(ch) if !ch.is_control() => {
                    insert(text, &mut editor.cursor, &ch.to_string())
                }
                MemoryKey::Space => insert(text, &mut editor.cursor, " "),
                MemoryKey::Paste(mut pasted) => {
                    let filtered = zeroize::Zeroizing::new(
                        pasted
                            .chars()
                            .filter(|ch| !ch.is_control())
                            .collect::<String>(),
                    );
                    insert(text, &mut editor.cursor, &filtered);
                    if secret {
                        pasted.zeroize();
                    }
                }
                MemoryKey::Edit(EditOp::Home) => editor.cursor = 0,
                MemoryKey::Edit(EditOp::End) => editor.cursor = text.len(),
                MemoryKey::Edit(EditOp::Left) | MemoryKey::Left => {
                    editor.cursor = text[..editor.cursor]
                        .char_indices()
                        .next_back()
                        .map_or(0, |(i, _)| i)
                }
                MemoryKey::Edit(EditOp::Right) | MemoryKey::Right => {
                    editor.cursor = text[editor.cursor..]
                        .chars()
                        .next()
                        .map_or(editor.cursor, |ch| editor.cursor + ch.len_utf8())
                }
                MemoryKey::Edit(EditOp::Backspace) if editor.cursor > 0 => {
                    let start = text[..editor.cursor]
                        .char_indices()
                        .next_back()
                        .map_or(0, |(i, _)| i);
                    text.replace_range(start..editor.cursor, "");
                    editor.cursor = start;
                }
                MemoryKey::Edit(EditOp::Delete) if editor.cursor < text.len() => {
                    if let Some(ch) = text[editor.cursor..].chars().next() {
                        text.replace_range(editor.cursor..editor.cursor + ch.len_utf8(), "");
                    }
                }
                _ => {}
            }
        }
        self.editor = Some(editor);
        Vec::new()
    }

    pub(super) fn ask_save(&mut self) -> Vec<MemoryEffect> {
        let Some(card) = &self.card else {
            return Vec::new();
        };
        if card.pending.plain.is_empty() && card.pending.secrets.is_empty() {
            self.banner = Some("No changes to save.".into());
            return Vec::new();
        }
        let Some(config) = self.config() else {
            return Vec::new();
        };
        if config.surface == ConfigSurface::Plain {
            for field in self.visible_fields() {
                if field.required
                    && !field.is_set
                    && !card.pending.plain.contains_key(&field.key)
                    && !card.pending.secrets.contains_key(&field.key)
                {
                    self.banner = Some(format!(
                        "Fill in {} first: Hermes needs it before it can use {}.",
                        field.label, card.name
                    ));
                    return Vec::new();
                }
            }
        }
        if self.active() != card.name {
            let mut lines = vec![format!(
                "Saving also makes {} the memory provider for profile {} (now {}).",
                card.name,
                self.profile,
                provider_name(self.active())
            )];
            if self.saves_defaults() {
                lines.push(DEFAULTS.into());
            }
            self.confirm(lines, "Save and use", ConfirmAction::Save);
            Vec::new()
        } else {
            self.send_save()
        }
    }
    fn saves_defaults(&self) -> bool {
        self.config()
            .is_some_and(|config| config.surface == ConfigSurface::Plain)
            && self.card.as_ref().is_some_and(|card| {
                self.visible_fields().iter().any(|field| {
                    !field.is_set
                        && !card.pending.plain.contains_key(&field.key)
                        && !card.pending.secrets.contains_key(&field.key)
                })
            })
    }
    pub(super) fn send_save(&mut self) -> Vec<MemoryEffect> {
        let (Some(config), Some(card)) = (self.config(), &self.card) else {
            return Vec::new();
        };
        let values = card.pending.clone();
        let effect = MemoryEffect::SaveConfig {
            name: card.name.clone(),
            surface: config.surface,
            values: values.clone(),
        };
        let mut submitted = values;
        for secret in submitted.secrets.values_mut() {
            *secret = SecretField::default();
        }
        self.write = Some(Write::Save(Box::new(SaveCheck {
            before: config.clone(),
            values: submitted,
            defaults: self.saves_defaults(),
            config_answered: false,
            after: None,
            status_answered: false,
            detail: None,
        })));
        self.verifying = false;
        vec![effect]
    }
}

fn insert(text: &mut String, cursor: &mut usize, inserted: &str) {
    if text.len().saturating_add(inserted.len()) <= 65_536 {
        text.insert_str(*cursor, inserted);
        *cursor += inserted.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_field(kind: FieldKind, value: Value) -> MemoryState {
        let field = FieldView {
            key: "choice".into(),
            label: "Choice".into(),
            kind,
            value,
            is_set: true,
            description: String::new(),
            placeholder: String::new(),
            required: true,
            options: vec![
                FieldOption {
                    value: "local".into(),
                    label: "Local".into(),
                    description: "Uses local storage".into(),
                },
                FieldOption {
                    value: "cloud".into(),
                    label: "Cloud".into(),
                    description: "Uses remote storage".into(),
                },
            ],
            when: None,
            minimum: None,
            maximum: None,
        };
        MemoryState {
            open: true,
            card: Some(Card {
                name: "sample".into(),
                config: Some(Ok(ConfigView {
                    name: "sample".into(),
                    label: "Sample".into(),
                    surface: ConfigSurface::Plain,
                    fields: vec![field],
                })),
                pending: SubmittedValues::default(),
                failed: Vec::new(),
                cursor: 0,
                results: Vec::new(),
            }),
            ..MemoryState::new()
        }
    }

    #[test]
    fn boolean_enter_and_space_toggle_without_a_text_editor() {
        let mut state = state_with_field(FieldKind::Bool, Value::Bool(false));
        let now = Instant::now();
        state.update(MemoryInput::Key(MemoryKey::Open), now);
        assert_eq!(
            state.card.as_ref().unwrap().pending.plain["choice"],
            Value::Bool(true)
        );
        assert!(!state.view(now).editing);
        state.update(MemoryInput::Key(MemoryKey::Space), now);
        assert!(state.card.as_ref().unwrap().pending.plain.is_empty());
        state.update(MemoryInput::Key(MemoryKey::Open), now);
        assert_eq!(
            state.card.as_ref().unwrap().pending.plain["choice"],
            Value::Bool(true)
        );
    }

    #[test]
    fn select_accepts_editor_navigation_and_shows_option_details() {
        let mut state = state_with_field(FieldKind::Select, Value::String("local".into()));
        let now = Instant::now();
        state.update(MemoryInput::Key(MemoryKey::Open), now);
        for key in [
            MemoryKey::Edit(EditOp::Right),
            MemoryKey::Up,
            MemoryKey::Down,
            MemoryKey::Edit(EditOp::Left),
            MemoryKey::Right,
        ] {
            state.update(MemoryInput::Key(key), now);
        }
        let card = state.view(now).card.unwrap();
        assert!(
            card.needs
                .iter()
                .any(|line| line == "> Cloud — Uses remote storage")
        );
        assert!(
            card.needs
                .iter()
                .any(|line| line.contains("Local — Uses local storage"))
        );
        state.update(MemoryInput::Key(MemoryKey::Open), now);
        assert_eq!(
            state.card.as_ref().unwrap().pending.plain["choice"],
            Value::String("cloud".into())
        );
    }

    #[test]
    fn required_marker_is_only_shown_on_plain_settings() {
        let mut state = state_with_field(FieldKind::Text, Value::String(String::new()));
        let now = Instant::now();
        assert_eq!(state.view(now).card.unwrap().fields[0].label, "Choice *");
        let config = state
            .card
            .as_mut()
            .unwrap()
            .config
            .as_mut()
            .unwrap()
            .as_mut()
            .unwrap();
        config.surface = ConfigSurface::Declared;
        assert_eq!(state.view(now).card.unwrap().fields[0].label, "Choice");
    }
}
