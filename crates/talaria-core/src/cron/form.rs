use super::text::{TARGET_FAILURE, clean};
use super::{
    AddForm, AddRequest, CronKey, DeliveryTarget, FieldError, FieldView, FormField, FormView,
};

pub fn validate_form(form: &AddForm) -> Result<AddRequest, Vec<FieldError>> {
    let mut errors = Vec::new();
    let mut fail = |field, text: &str| {
        errors.push(FieldError {
            field,
            text: text.into(),
        })
    };
    if form.name.trim().chars().count() > 100 {
        fail(FormField::Name, "Name: at most 100 characters.");
    }
    if form.schedule.trim().is_empty() {
        fail(FormField::Schedule, "Schedule: required.");
    }
    if form.prompt.trim().is_empty() {
        fail(FormField::Prompt, "Prompt: required.");
    }
    if form.prompt.trim().chars().count() > 16000 {
        fail(FormField::Prompt, "Prompt: at most 16,000 characters.");
    }
    let digits = form.repeat.trim();
    let repeat = if digits.is_empty() {
        None
    } else {
        match digits.parse::<u32>() {
            Ok(n) if (1..=9999).contains(&n) && digits.bytes().all(|b| b.is_ascii_digit()) => {
                Some(n)
            }
            _ => {
                fail(
                    FormField::Repeat,
                    "Repeat: empty for forever, or a whole number from 1 to 9999.",
                );
                None
            }
        }
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(AddRequest {
        name: (!form.name.trim().is_empty()).then(|| form.name.trim().into()),
        schedule: form.schedule.trim().into(),
        prompt: form.prompt.trim_end().into(),
        repeat,
        deliver: form.deliver.clone(),
    })
}

pub(super) fn default_form() -> AddForm {
    AddForm {
        name: String::new(),
        schedule: String::new(),
        prompt: String::new(),
        repeat: String::new(),
        deliver: "local".into(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Editor {
    pub form: AddForm,
    pub focus: usize,
    cursors: [usize; 4],
    pub targets: Vec<DeliveryTarget>,
    pub errors: Vec<FieldError>,
    pub status: Vec<String>,
    pub locked: bool,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            form: default_form(),
            focus: 0,
            cursors: [0; 4],
            targets: vec![local()],
            errors: Vec::new(),
            status: Vec::new(),
            locked: false,
        }
    }
    pub fn dirty(&self) -> bool {
        self.form != default_form()
    }
    fn field(&self) -> &String {
        match self.focus {
            0 => &self.form.name,
            1 => &self.form.schedule,
            2 => &self.form.prompt,
            _ => &self.form.repeat,
        }
    }
    fn field_mut(&mut self) -> &mut String {
        match self.focus {
            0 => &mut self.form.name,
            1 => &mut self.form.schedule,
            2 => &mut self.form.prompt,
            _ => &mut self.form.repeat,
        }
    }
    pub fn load_targets(&mut self, result: Result<Vec<DeliveryTarget>, super::RestFailure>) {
        match result {
            Ok(targets) => {
                self.targets = vec![local()];
                for target in targets {
                    if target.id != "local" && !self.targets.iter().any(|t| t.id == target.id) {
                        self.targets.push(target);
                    }
                }
                if !self
                    .targets
                    .iter()
                    .any(|t| t.id == self.form.deliver && t.home_target_set)
                {
                    self.form.deliver = "local".into();
                }
            }
            Err(_) => {
                self.targets = vec![local()];
                self.form.deliver = "local".into();
                self.status = vec![TARGET_FAILURE.into()];
            }
        }
    }
    fn choose(&mut self, delta: isize) {
        let index = self
            .targets
            .iter()
            .position(|t| t.id == self.form.deliver)
            .unwrap_or(0);
        for step in 1..=self.targets.len() {
            let candidate = (index as isize + delta * step as isize)
                .rem_euclid(self.targets.len() as isize) as usize;
            if self.targets[candidate].home_target_set {
                self.form.deliver = self.targets[candidate].id.clone();
                break;
            }
        }
    }
    fn insert(&mut self, text: &str) {
        let text = if self.focus == 2 {
            text.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            text.replace("\r\n", " ").replace(['\r', '\n'], " ")
        };
        let cursor = self.cursors[self.focus].min(self.field().chars().count());
        let byte = self
            .field()
            .char_indices()
            .nth(cursor)
            .map_or(self.field().len(), |(b, _)| b);
        self.field_mut().insert_str(byte, &text);
        self.cursors[self.focus] = cursor + text.chars().count();
    }
    fn vertical(&mut self, delta: isize) {
        let text: Vec<_> = self.field().chars().collect();
        let cursor = self.cursors[self.focus].min(text.len());
        let line_start = text[..cursor]
            .iter()
            .rposition(|c| *c == '\n')
            .map_or(0, |p| p + 1);
        let column = cursor - line_start;
        let mut starts = vec![0];
        starts.extend(
            text.iter()
                .enumerate()
                .filter_map(|(i, c)| (*c == '\n').then_some(i + 1)),
        );
        let line = starts.partition_point(|p| *p <= cursor).saturating_sub(1);
        let target = line.saturating_add_signed(delta).min(starts.len() - 1);
        let end = starts.get(target + 1).map_or(text.len(), |p| p - 1);
        self.cursors[self.focus] = (starts[target] + column).min(end);
    }
    pub fn edit(&mut self, key: CronKey) {
        if self.locked {
            return;
        }
        if key == CronKey::Tab {
            self.focus = (self.focus + 1) % 5;
            return;
        }
        if key == CronKey::BackTab {
            self.focus = (self.focus + 4) % 5;
            return;
        }
        if self.focus == 4 {
            match key {
                CronKey::Left | CronKey::Up => self.choose(-1),
                CronKey::Right | CronKey::Down => self.choose(1),
                CronKey::Enter => self.focus = 0,
                _ => {}
            }
            return;
        }
        let cursor = self.cursors[self.focus].min(self.field().chars().count());
        let chars: Vec<char> = self.field().chars().collect();
        match key {
            CronKey::Char(c) if !c.is_control() => self.insert(&c.to_string()),
            CronKey::Paste(text) => self.insert(&text),
            CronKey::Enter if self.focus == 2 => self.insert("\n"),
            CronKey::Enter | CronKey::Down if self.focus != 2 => self.focus = (self.focus + 1) % 5,
            CronKey::Up if self.focus != 2 => self.focus = (self.focus + 4) % 5,
            CronKey::Up => self.vertical(-1),
            CronKey::Down => self.vertical(1),
            CronKey::PageUp if self.focus == 2 => self.vertical(-10),
            CronKey::PageDown if self.focus == 2 => self.vertical(10),
            CronKey::Left => self.cursors[self.focus] = cursor.saturating_sub(1),
            CronKey::Right => self.cursors[self.focus] = (cursor + 1).min(chars.len()),
            CronKey::Home => {
                self.cursors[self.focus] = chars[..cursor]
                    .iter()
                    .rposition(|c| *c == '\n')
                    .map_or(0, |p| p + 1)
            }
            CronKey::End => {
                self.cursors[self.focus] = chars[cursor..]
                    .iter()
                    .position(|c| *c == '\n')
                    .map_or(chars.len(), |p| cursor + p)
            }
            CronKey::Backspace if cursor > 0 => {
                let mut next = chars;
                next.remove(cursor - 1);
                *self.field_mut() = next.into_iter().collect();
                self.cursors[self.focus] = cursor - 1;
            }
            CronKey::Delete if cursor < chars.len() => {
                let mut next = chars;
                next.remove(cursor);
                *self.field_mut() = next.into_iter().collect();
            }
            _ => {}
        }
    }
    pub fn view(&self) -> FormView {
        let specs = [
            (FormField::Name, "Name", &self.form.name, None),
            (
                FormField::Schedule,
                "Schedule",
                &self.form.schedule,
                Some(
                    "Examples: 30m · every 2h · in 30m (once) · every monday 9am · weekdays at 9am · 0 9 * * * · 2026-11-01T09:00",
                ),
            ),
            (
                FormField::Prompt,
                "Prompt",
                &self.form.prompt,
                Some(
                    "Runs in a fresh session with no chat history. Its final answer is what gets delivered.",
                ),
            ),
            (
                FormField::Repeat,
                "Repeat",
                &self.form.repeat,
                Some("empty = forever"),
            ),
            (FormField::Deliver, "Deliver", &self.form.deliver, None),
        ];
        let fields = specs
            .iter()
            .map(|(field, label, value, hint)| FieldView {
                field: *field,
                label: (*label).into(),
                value: super::split_and_sanitise(value, usize::MAX).join("\n"),
                hint: hint.map(String::from),
                error: self
                    .errors
                    .iter()
                    .find(|e| e.field == *field)
                    .map(|e| clean(&e.text)),
            })
            .collect();
        let cursor = if self.focus == 4 {
            None
        } else {
            let prefix: String = self
                .field()
                .chars()
                .take(self.cursors[self.focus])
                .collect();
            Some((
                prefix.chars().filter(|c| *c == '\n').count(),
                prefix.rsplit('\n').next().unwrap_or("").chars().count(),
            ))
        };
        FormView {
            fields,
            focus: specs[self.focus].0,
            targets: self
                .targets
                .iter()
                .map(|t| {
                    (
                        format!(
                            "{} ({}){}",
                            clean(&t.name),
                            clean(&t.id),
                            if t.home_target_set {
                                ""
                            } else {
                                " · no home channel set"
                            }
                        ),
                        t.home_target_set,
                    )
                })
                .collect(),
            deliver_index: self
                .targets
                .iter()
                .position(|t| t.id == self.form.deliver)
                .unwrap_or(0),
            status: self
                .status
                .iter()
                .flat_map(|s| super::split_and_sanitise(s, 9))
                .collect(),
            locked: self.locked,
            cursor,
        }
    }
}

fn local() -> DeliveryTarget {
    DeliveryTarget {
        id: "local".into(),
        name: "Local (save only)".into(),
        home_target_set: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_moves_unicode_cursor_by_line_and_keeps_field_cursors() {
        let mut editor = Editor::new();
        editor.edit(CronKey::Paste("é\r\nλ".into()));
        assert_eq!(editor.form.name, "é λ");
        editor.edit(CronKey::Left);
        editor.edit(CronKey::Delete);
        assert_eq!(editor.form.name, "é ");
        editor.edit(CronKey::Tab);
        editor.edit(CronKey::Tab);
        editor.edit(CronKey::Paste("αβ\n1234\nxy".into()));
        assert_eq!(editor.view().cursor, Some((2, 2)));
        editor.edit(CronKey::Up);
        assert_eq!(editor.view().cursor, Some((1, 2)));
        editor.edit(CronKey::Home);
        editor.edit(CronKey::Char('!'));
        assert_eq!(editor.form.prompt, "αβ\n!1234\nxy");
        editor.edit(CronKey::PageUp);
        assert_eq!(editor.view().cursor, Some((0, 1)));
        editor.edit(CronKey::End);
        editor.edit(CronKey::Enter);
        assert_eq!(editor.form.prompt, "αβ\n\n!1234\nxy");
        editor.edit(CronKey::BackTab);
        editor.edit(CronKey::BackTab);
        assert_eq!(editor.view().cursor, Some((0, 2)));
        editor.locked = true;
        let before = editor.form.clone();
        editor.edit(CronKey::Char('x'));
        assert_eq!(editor.form, before);
    }

    #[test]
    fn delivery_choices_skip_unset_targets_and_collapse_to_local_on_failure() {
        let mut editor = Editor::new();
        editor.load_targets(Ok(vec![
            DeliveryTarget {
                id: "unset".into(),
                name: "Unset".into(),
                home_target_set: false,
            },
            DeliveryTarget {
                id: "ready".into(),
                name: "Ready".into(),
                home_target_set: true,
            },
        ]));
        editor.focus = 4;
        editor.edit(CronKey::Right);
        assert_eq!(editor.form.deliver, "ready");
        editor.edit(CronKey::Right);
        assert_eq!(editor.form.deliver, "local");
        editor.load_targets(Err(super::super::RestFailure::Unknown));
        assert_eq!(editor.targets.len(), 1);
        assert_eq!(editor.status, [TARGET_FAILURE]);
    }
}
