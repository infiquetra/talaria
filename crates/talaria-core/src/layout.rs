//! Client-owned optional layout preferences and pure layout rules.

/// Columns allocated to the panel, including its divider.
pub const PANEL_COLUMNS: u16 = 33;
/// Smallest terminal that can dock the panel beside a useful transcript.
pub const DOCK_COLUMNS: u16 = 112;
/// Smallest terminal height that can dock the panel.
pub const DOCK_ROWS: u16 = 28;
/// Below this height, optional chrome reserves no space.
pub const MIN_ROWS: u16 = 16;

/// Client view preferences; they are independent of a gateway session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutPreferences {
    pub work_panel: bool,
    pub menu_bar: bool,
}

/// The current panel presentation, separate from its enabled preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelVisibility {
    #[default]
    Hidden,
    Docked,
    OverlayOpen,
    Dismissed,
}

/// Application-owned runtime layout. Resizing never opens an overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutState {
    pub preferences: LayoutPreferences,
    pub panel: PanelVisibility,
}

/// Pure column allocation. A later menu can use the separate row allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutPlan {
    pub transcript_columns: u16,
    pub panel_columns: u16,
    pub menu_rows: u16,
}

impl LayoutState {
    /// Startup never covers a draft with a panel overlay.
    pub fn new(preferences: LayoutPreferences, columns: u16, rows: u16) -> Self {
        let mut state = Self {
            preferences,
            panel: PanelVisibility::Hidden,
        };
        state.resize(columns, rows);
        state
    }

    /// Either independent preference requires retained full-screen rendering.
    pub fn workspace_enabled(&self) -> bool {
        self.preferences.work_panel || self.preferences.menu_bar
    }

    /// Toggle the preference, except that a dismissed panel is reopened on demand.
    pub fn toggle_panel(&mut self, columns: u16, rows: u16) {
        if self.preferences.work_panel && self.panel != PanelVisibility::Dismissed {
            self.preferences.work_panel = false;
            self.panel = PanelVisibility::Hidden;
            return;
        }
        self.preferences.work_panel = true;
        self.panel = if docked(columns, rows) {
            PanelVisibility::Docked
        } else if rows >= MIN_ROWS {
            PanelVisibility::OverlayOpen
        } else {
            PanelVisibility::Dismissed
        };
    }

    /// Dismiss only the panel overlay; never disable the preference or interrupt work.
    pub fn dismiss_panel(&mut self) -> bool {
        if self.panel != PanelVisibility::OverlayOpen {
            return false;
        }
        self.panel = PanelVisibility::Dismissed;
        true
    }

    /// Dock when both thresholds hold; otherwise keep an explicitly opened overlay
    /// only while it has enough rows. A dock becoming small is always dismissed.
    pub fn resize(&mut self, columns: u16, rows: u16) {
        self.panel = if !self.preferences.work_panel {
            PanelVisibility::Hidden
        } else if docked(columns, rows) {
            PanelVisibility::Docked
        } else if rows >= MIN_ROWS && self.panel == PanelVisibility::OverlayOpen {
            PanelVisibility::OverlayOpen
        } else {
            PanelVisibility::Dismissed
        };
    }

    /// An overlay leaves the transcript full width. No menu row ships with the panel.
    pub fn plan(&self, columns: u16, rows: u16) -> LayoutPlan {
        let panel_columns = if self.preferences.work_panel
            && self.panel == PanelVisibility::Docked
            && docked(columns, rows)
        {
            PANEL_COLUMNS
        } else {
            0
        };
        LayoutPlan {
            transcript_columns: columns.saturating_sub(panel_columns),
            panel_columns,
            menu_rows: 0,
        }
    }
}

fn docked(columns: u16, rows: u16) -> bool {
    columns >= DOCK_COLUMNS && rows >= DOCK_ROWS
}

/// A local layout command. Menu actions are added with the menu itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutAction {
    Panel,
}

/// Whether the inline subagent dock is drawn.
/// The screen asks this before adding that dock: the work panel hosts the same summary.
#[must_use]
pub fn inline_subagent_dock_visible(panel_visible: bool) -> bool {
    let _ = panel_visible;
    true
}

/// Parse exactly one supported argument and return a single-line usage notice otherwise.
pub fn parse_layout_args(argument: &str) -> Result<LayoutAction, String> {
    match argument.trim() {
        "panel" => Ok(LayoutAction::Panel),
        _ => Err("Usage: /layout panel".into()),
    }
}
