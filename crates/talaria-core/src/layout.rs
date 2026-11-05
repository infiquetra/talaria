//! Client-owned optional layout preferences and pure layout rules.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutPreferences {
    pub work_panel: bool,
    pub menu_bar: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelVisibility {
    #[default]
    Hidden,
    Docked,
    OverlayOpen,
    Dismissed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutState {
    pub preferences: LayoutPreferences,
    pub panel: PanelVisibility,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutPlan {
    pub transcript_columns: u16,
    pub panel_columns: u16,
    pub menu_rows: u16,
}

impl LayoutState {
    pub fn new(preferences: LayoutPreferences, _columns: u16, _rows: u16) -> Self {
        Self {
            preferences,
            ..Self::default()
        }
    }

    pub fn workspace_enabled(&self) -> bool {
        false
    }
    pub fn toggle_panel(&mut self, _columns: u16, _rows: u16) {}
    pub fn dismiss_panel(&mut self) -> bool {
        false
    }
    pub fn resize(&mut self, _columns: u16, _rows: u16) {}
    pub fn plan(&self, columns: u16, _rows: u16) -> LayoutPlan {
        LayoutPlan {
            transcript_columns: columns,
            ..LayoutPlan::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutAction {
    Panel,
}

pub fn parse_layout_args(_argument: &str) -> Result<LayoutAction, String> {
    Err("Usage: /layout panel".into())
}
