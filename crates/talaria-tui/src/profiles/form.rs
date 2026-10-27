use ratatui::layout::Rect;
use talaria_core::profiles::{FormField, FormView, Panel, ProfilesView};
use unicode_width::UnicodeWidthStr;

use crate::history::sanitize;

pub(super) fn lines(panel: &Panel) -> Vec<String> {
    match panel {
        Panel::Form(form) => form_lines(form),
        Panel::Review(lines) => {
            let mut out = vec!["Review".into()];
            out.extend(lines.iter().cloned());
            out.push("Enter apply · Esc back".into());
            out
        }
        Panel::DescriptionInput {
            name, text, error, ..
        } => {
            let mut out = vec![format!("Description of {name}"), format!("> {text}")];
            if let Some(error) = error {
                out.push(error.clone());
            }
            out.push("Enter review · Esc cancel".into());
            out
        }
        _ => Vec::new(),
    }
}

fn form_lines(form: &FormView) -> Vec<String> {
    let marker = |field| if form.focus == field { "▸ " } else { "  " };
    let mut out = vec![
        "New profile".into(),
        format!("{}Name: {}", marker(FormField::Name), form.name),
    ];
    if let Some(error) = &form.name_error {
        out.push(format!("    {error}"));
    }
    out.push(format!(
        "{}Description: {}",
        marker(FormField::Description),
        form.description
    ));
    let choice = form
        .start_from
        .get(form.start_index)
        .map_or("", String::as_str);
    out.push(format!(
        "{}Start from: ‹ {choice} ›",
        marker(FormField::StartFrom)
    ));
    let checked = if form.copy_keys { "x" } else { " " };
    out.push(format!(
        "{}[{checked}] Copy provider keys from {}",
        marker(FormField::CopyKeys),
        form.copy_from
    ));
    out.push("Enter next · Tab move · Esc cancel".into());
    out
}

/// Return the insertion point when a panel has a focused text field.
#[must_use]
pub fn cursor_position(view: &ProfilesView, area: Rect) -> Option<(u16, u16)> {
    if view.confirm.is_some() || view.loading {
        return None;
    }
    let panel_y = if area.width >= 100 {
        let list = view
            .rows
            .len()
            .min(usize::from(area.height.saturating_sub(2) / 2))
            .max(1);
        area.y
            .saturating_add(2)
            .saturating_add(u16::try_from(list).ok()?)
    } else {
        area.y.saturating_add(1)
    };
    let (prefix, text, cursor, row) = match &view.panel {
        Panel::Form(form) if form.focus == FormField::Name => {
            ("▸ Name: ", &form.name, form.name_cursor, 1)
        }
        Panel::Form(form) if form.focus == FormField::Description => (
            "▸ Description: ",
            &form.description,
            form.description_cursor,
            2 + u16::from(form.name_error.is_some()),
        ),
        Panel::DescriptionInput { text, cursor, .. } => ("> ", text, *cursor, 1),
        _ => return None,
    };
    let before: String = text.chars().take(cursor).collect();
    let x = UnicodeWidthStr::width(prefix) + UnicodeWidthStr::width(sanitize(&before).as_ref());
    let x = area.x.saturating_add(u16::try_from(x).ok()?);
    let y = panel_y.saturating_add(row);
    let notice_top = area.bottom().saturating_sub(
        u16::try_from(super::render::notice_height(view, area)).unwrap_or(u16::MAX),
    );
    (x < area.right() && y < notice_top).then_some((x, y))
}
