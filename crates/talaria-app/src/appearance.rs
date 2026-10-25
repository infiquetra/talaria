//! Appearance effects: skin commands, theme reports, and the start-up probe colours.
//!
//! The service holds the skin state; the terminal holder draws. Stubs.
//! Behavior contract: issue 227.

use hermes_client::Client;
use talaria_core::appearance::{
    AppearanceChange, AppearanceInput, NotAppliedReason, Polarity, PolaritySource, Skin,
    SkippedValue, TerminalColors, ThemeReport,
};

/// The appearance state for one run.
#[derive(Debug)]
pub struct AppearanceService {
    _private: (),
}

impl AppearanceService {
    /// A service with no skin, capturing the start-up probe colours and the
    /// start-up polarity once. `/theme auto` restores that polarity.
    pub fn new(
        startup_colors: TerminalColors,
        startup_polarity: (Polarity, PolaritySource),
    ) -> Self {
        let _ = (startup_colors, startup_polarity);
        todo!("#227")
    }

    /// The captured start-up probe colours. Nothing else writes them.
    pub fn startup_colors(&self) -> TerminalColors {
        todo!("#227")
    }

    /// Fold one input into the skin state.
    pub fn update(&mut self, input: AppearanceInput) -> AppearanceChange {
        let _ = input;
        todo!("#227")
    }

    /// The current skin, if any.
    pub fn skin(&self) -> Option<&Skin> {
        todo!("#227")
    }

    /// The current skin's skipped values.
    pub fn skipped(&self) -> &[SkippedValue] {
        todo!("#227")
    }

    /// Set or clear the session polarity override. True when the theme changes.
    pub fn set_polarity_override(&mut self, override_: Option<Polarity>) -> bool {
        let _ = override_;
        todo!("#227")
    }

    /// The session polarity override, if any.
    pub fn polarity_override(&self) -> Option<Polarity> {
        todo!("#227")
    }

    /// Run `/theme <arg>`: set or clear the session polarity override. Returns the
    /// notice lines and whether the theme changed; sends nothing to Hermes.
    pub fn theme_command(&mut self, arg: &str) -> (Vec<String>, AppearanceChange) {
        let _ = arg;
        todo!("#227")
    }

    /// Run `/skin` or `/skin <name>`, returning the notice lines.
    pub async fn skin_command(
        &mut self,
        client: &Client,
        name: Option<&str>,
        session: Option<&str>,
        profile: &str,
    ) -> Vec<String> {
        let _ = (client, name, session, profile);
        todo!("#227")
    }

    /// Describe again after a reconnect, retrying once unless superseded.
    pub async fn connection_opened(&mut self, client: &Client, generation: u64) {
        let _ = (client, generation);
        todo!("#227")
    }
}

/// The `/theme` report lines for a resolved theme.
pub fn theme_lines(report: &ThemeReport) -> Vec<String> {
    let _ = report;
    todo!("#227")
}

/// The `/theme light|dark` confirmation line.
pub fn theme_set_line(polarity: Polarity) -> String {
    let _ = polarity;
    todo!("#227")
}

/// The `/theme auto` line for the restored polarity and its source label.
pub fn background_restored_line(polarity: Polarity, source_label: &str) -> String {
    let _ = (polarity, source_label);
    todo!("#227")
}

/// The `/theme` usage line.
pub const THEME_USAGE: &str = "Usage: /theme [light|dark|auto]";

/// The reason a skin is not applied, if any, from a resolved report.
pub fn not_applied(report: &ThemeReport) -> Option<NotAppliedReason> {
    let _ = report;
    todo!("#227")
}
