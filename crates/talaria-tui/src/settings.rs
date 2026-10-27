use ratatui::{Frame, layout::Rect};
use talaria_core::settings::SettingsView;
pub fn render(_frame: &mut Frame<'_>, _area: Rect, _view: &SettingsView) {
    todo!("settings stub")
}

use crate::keys::{Key, KeyMap};
use ratatui::buffer::Buffer;
use talaria_core::settings::{ReviewRow, SettingsKey};

pub fn draw_settings(_view: &SettingsView, _area: Rect, _buf: &mut Buffer) {
    todo!("settings stub")
}

pub fn draw_review(_rows: &[ReviewRow], _title: &str, _area: Rect, _buf: &mut Buffer) {
    todo!("settings stub")
}

pub fn route_settings(_view: &SettingsView, _map: &KeyMap, _key: &Key) -> Option<SettingsKey> {
    todo!("settings stub")
}
