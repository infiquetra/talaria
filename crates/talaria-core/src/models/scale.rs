//! The `/effort` selector: Hermes's effort levels on one scale, low to high.
//!
//! Left and Right move a marker; nothing is written while it moves. Tab stages the Hivecode
//! switch and the save switch stages "also for new sessions". Enter asks the application for
//! exactly what the typed commands would do (`/effort <level> [--default]`, `/hivecode on|off`),
//! so every existing path, its notices and its refusals apply unchanged. Esc drops it all.

use super::Effort;
use crate::hivecode::notice::NAME as HIVECODE;

/// What the application knows when the selector opens, refreshed as reads answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScaleFacts {
    /// The model the status bar names, for the title.
    pub model: Option<String>,
    /// The session's level; `None` when it could not be read yet.
    pub current: Option<Effort>,
    /// The level Hermes actually sends on this route (`reasoning_effort_wire`), when reported.
    pub wire: Option<String>,
    /// Whether the model offers effort levels, when the catalogue says.
    pub offered: Option<bool>,
    /// Whether `none` turns thinking off on this route, when the catalogue says.
    pub can_disable: Option<bool>,
    /// Whether Hivecode is on for the session now.
    pub hivecode: bool,
    /// The profile a saved level belongs to.
    pub profile: String,
}

/// One key for the open selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleKey {
    /// One level lower; stops at the lowest.
    Lower,
    /// One level higher; stops at the highest.
    Higher,
    Lowest,
    Highest,
    /// Stage Hivecode on or off.
    ToggleHivecode,
    /// Stage saving the level for new sessions too.
    ToggleSave,
    /// Enter.
    Apply,
    /// Esc.
    Cancel,
}

/// What the application should do after one key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScaleEffect {
    /// Only the selector changed.
    Redraw,
    /// Close; nothing changes. Also Enter with nothing staged.
    Cancel,
    /// Close and run, in order, `/effort <level>` (with `--default` when `save_default`) when
    /// `level` is set, then `/hivecode on|off` when `hivecode` is set.
    Apply {
        level: Option<Effort>,
        save_default: bool,
        hivecode: Option<bool>,
    },
}

/// The open selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffortScale {
    facts: ScaleFacts,
    selected: Effort,
    /// The person moved the marker, so a later read no longer moves it.
    touched: bool,
    save_default: bool,
    /// The staged Hivecode switch.
    hivecode: bool,
}

impl EffortScale {
    #[must_use]
    pub fn new(facts: ScaleFacts) -> Self {
        Self {
            // Hermes reads an unset level as medium.
            selected: facts.current.unwrap_or(Effort::Medium),
            touched: false,
            save_default: false,
            hivecode: facts.hivecode,
            facts,
        }
    }

    pub fn update(&mut self, key: ScaleKey) -> ScaleEffect {
        let levels = self.levels();
        let at = levels
            .iter()
            .position(|level| *level == self.selected)
            .unwrap_or(0);
        let last = levels.len().saturating_sub(1);
        let to = match key {
            ScaleKey::Lower => at.saturating_sub(1),
            ScaleKey::Higher => (at + 1).min(last),
            ScaleKey::Lowest => 0,
            ScaleKey::Highest => last,
            ScaleKey::ToggleHivecode => {
                self.hivecode = !self.hivecode;
                return ScaleEffect::Redraw;
            }
            ScaleKey::ToggleSave => {
                self.save_default = !self.save_default;
                return ScaleEffect::Redraw;
            }
            ScaleKey::Cancel => return ScaleEffect::Cancel,
            ScaleKey::Apply => return self.apply(),
        };
        if let Some(level) = levels.get(to) {
            self.selected = *level;
            self.touched = true;
        }
        ScaleEffect::Redraw
    }

    fn apply(&self) -> ScaleEffect {
        let moved = self.touched && Some(self.selected) != self.facts.current;
        let level = (moved || self.save_default).then_some(self.selected);
        let hivecode = self.hivecode_changed().then_some(self.hivecode);
        if level.is_none() && hivecode.is_none() {
            return ScaleEffect::Cancel;
        }
        ScaleEffect::Apply {
            level,
            save_default: self.save_default,
            hivecode,
        }
    }

    /// A later read of the session's level. The marker follows it until the person moves it.
    pub fn set_current(&mut self, level: Option<Effort>, wire: Option<String>) {
        self.facts.current = level;
        self.facts.wire = wire;
        if !self.touched
            && let Some(level) = level
        {
            self.selected = level;
        }
    }

    /// The catalogue's answer for the session's model, once it arrives.
    pub fn set_caps(&mut self, offered: Option<bool>, can_disable: Option<bool>) {
        self.facts.offered = offered;
        self.facts.can_disable = can_disable;
    }

    /// The levels on the scale, low to high: all of Hermes's, without `none` when the route
    /// cannot turn thinking off, unless the session or the marker is already there.
    #[must_use]
    pub fn levels(&self) -> Vec<Effort> {
        let keep_none = self.facts.can_disable != Some(false)
            || self.facts.current == Some(Effort::None)
            || self.selected == Effort::None;
        Effort::ALL
            .into_iter()
            .filter(|level| keep_none || *level != Effort::None)
            .collect()
    }

    #[must_use]
    pub fn selected(&self) -> Effort {
        self.selected
    }

    #[must_use]
    pub fn current(&self) -> Option<Effort> {
        self.facts.current
    }

    /// The staged Hivecode switch.
    #[must_use]
    pub fn hivecode(&self) -> bool {
        self.hivecode
    }

    /// Enter would turn Hivecode on or off.
    #[must_use]
    pub fn hivecode_changed(&self) -> bool {
        self.hivecode != self.facts.hivecode
    }

    #[must_use]
    pub fn save_default(&self) -> bool {
        self.save_default
    }

    /// `Effort · {model}`.
    #[must_use]
    pub fn title(&self) -> String {
        match &self.facts.model {
            Some(model) => format!("Effort \u{b7} {model}"),
            None => "Effort".to_owned(),
        }
    }

    /// The selected level's description, and where the session's own level is.
    #[must_use]
    pub fn detail(&self) -> String {
        let text = self.selected.describe();
        match self.facts.current {
            Some(current) if current == self.selected => format!("{text}  (current)"),
            Some(current) => format!("{text}  (current: {})", current.as_wire()),
            None => text.to_owned(),
        }
    }

    /// The save switch, as on or off.
    #[must_use]
    pub fn save_line(&self) -> String {
        format!(
            "Also save for new sessions on {}: {}",
            self.facts.profile,
            on_off(self.save_default)
        )
    }

    /// The Hivecode switch, with what it is now when Enter would change it.
    #[must_use]
    pub fn hivecode_line(&self) -> String {
        if self.hivecode_changed() {
            format!(
                "{HIVECODE} {} (now {})",
                on_off(self.hivecode),
                on_off(self.facts.hivecode)
            )
        } else {
            format!("{HIVECODE} {}", on_off(self.hivecode))
        }
    }

    /// One line the person should know before choosing, if any.
    #[must_use]
    pub fn note(&self) -> Option<String> {
        let model = self.facts.model.as_deref().unwrap_or("This model");
        if self.facts.offered == Some(false) {
            return Some(format!(
                "{model} does not offer effort levels, so a level set here has no effect on it."
            ));
        }
        let Some(current) = self.facts.current else {
            return Some("effort: could not be read".to_owned());
        };
        self.facts
            .wire
            .as_deref()
            .filter(|wire| *wire != current.as_wire())
            .map(|wire| format!("Hermes sends {wire} on this route."))
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}
