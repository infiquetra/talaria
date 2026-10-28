use crate::staffing::{Limits, PriceBook, ProviderLimits, RosterError, RunBudget, StaffingTable};
use serde::Serialize;
use std::{collections::BTreeMap, time::Duration};
#[derive(Clone, Debug, PartialEq)]
pub struct SchedulerConfig {
    pub run_profile: String,
    pub limits: Limits,
    pub providers: BTreeMap<String, ProviderLimits>,
    pub budget: RunBudget,
    pub prices: PriceBook,
    pub cooldown: Duration,
    pub roster_min_interval: Duration,
}
impl SchedulerConfig {
    pub fn from_table(
        t: &StaffingTable,
        budget: RunBudget,
        profile: &str,
        prices: PriceBook,
    ) -> Self {
        Self {
            run_profile: profile.into(),
            limits: t.limits.clone(),
            providers: t.providers.clone(),
            budget,
            prices,
            cooldown: Duration::from_secs(60),
            roster_min_interval: Duration::from_secs(30),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MoneySource {
    Hermes,
    Pricing,
    Mixed,
    None,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Spend {
    pub tokens: u64,
    pub money_usd: f64,
    pub money_source: MoneySource,
    pub money_unknown_turns: u32,
    pub tokens_unknown_turns: u32,
}
impl Default for Spend {
    fn default() -> Self {
        Self {
            tokens: 0,
            money_usd: 0.0,
            money_source: MoneySource::None,
            money_unknown_turns: 0,
            tokens_unknown_turns: 0,
        }
    }
}
impl Spend {
    pub fn money_text(&self) -> String {
        if self.money_unknown_turns == 0 {
            format!("${:.2}", self.money_usd)
        } else if self.money_usd > 0.0 {
            format!(
                "${:.2} plus {} unpriced turns",
                self.money_usd, self.money_unknown_turns
            )
        } else {
            format!("unknown ({} unpriced turns)", self.money_unknown_turns)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SchedulerStatus {
    pub global_width: u32,
    pub in_flight: u32,
    pub queued: u32,
    pub lanes: Vec<LaneStatus>,
    pub spend: Spend,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LaneStatus {
    pub profile: String,
    pub provider: String,
    pub width: u32,
    pub in_flight: u32,
    pub queued: u32,
    pub paused_until: Option<String>,
    pub pause_cause: Option<PauseCause>,
}
pub struct SchedulerEvents(pub tokio::sync::mpsc::UnboundedReceiver<TimedEvent>);
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TimedEvent {
    pub at: String,
    #[serde(flatten)]
    pub event: SchedulerEvent,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SchedulerEvent {
    Queued {
        key: String,
        label: String,
        profile: String,
        provider: String,
        model: String,
        reason: WaitReason,
    },
    Admitted {
        key: String,
        profile: String,
        provider: String,
        in_flight: u32,
        lane_in_flight: u32,
        reattached: bool,
    },
    Released {
        key: String,
        profile: String,
        provider: String,
    },
    Skipped {
        key: String,
        label: String,
        cause: SkipCause,
    },
    CapHalved {
        scope: CapScope,
        from: u32,
        to: u32,
    },
    LanePaused {
        profile: String,
        provider: String,
        until: Option<String>,
        cause: PauseCause,
    },
    LaneResumed {
        profile: String,
        provider: String,
    },
    RetryAuthorised {
        key: String,
        attempt: u32,
        reason: String,
    },
    BudgetReached {
        kind: BudgetKind,
        tokens_spent: u64,
        tokens_total: Option<u64>,
        money_spent_usd: f64,
        money_total_usd: Option<f64>,
    },
    RosterRefreshed {
        profile: String,
    },
    RosterReadFailed {
        profile: String,
        message: String,
        persistent: bool,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum WaitReason {
    GlobalCap,
    LaneCap,
    ProviderLimited {
        until: Option<String>,
    },
    ModelLimited {
        until: Option<String>,
    },
    LanePaused {
        until: Option<String>,
        cause: PauseCause,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum SkipCause {
    BudgetReached { budget: BudgetKind },
    OutOfCredit,
}
#[derive(Clone, Debug, PartialEq)]
pub enum CapScope {
    Global,
    Lane { profile: String, provider: String },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseCause {
    RateLimited,
    OutOfCredit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetKind {
    Tokens,
    Money,
}
#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("roster: {0}")]
    Roster(#[from] RosterError),
    #[error("journal: {0}")]
    Journal(String),
    #[error("configuration: {0}")]
    Config(String),
}

impl Serialize for WaitReason {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::GlobalCap => s.serialize_str("global_cap"),
            Self::LaneCap => s.serialize_str("lane_cap"),
            Self::ProviderLimited { until } => {
                serde_json::json!({"kind":"provider_limited","until":until}).serialize(s)
            }
            Self::ModelLimited { until } => {
                serde_json::json!({"kind":"model_limited","until":until}).serialize(s)
            }
            Self::LanePaused { until, cause } => {
                serde_json::json!({"kind":"lane_paused","until":until,"cause":cause}).serialize(s)
            }
        }
    }
}
impl Serialize for SkipCause {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::OutOfCredit => s.serialize_str("out_of_credit"),
            Self::BudgetReached { budget } => {
                serde_json::json!({"kind":"budget_reached","budget":budget}).serialize(s)
            }
        }
    }
}
impl Serialize for CapScope {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Global => s.serialize_str("global"),
            Self::Lane { profile, provider } => {
                serde_json::json!({"kind":"lane","profile":profile,"provider":provider})
                    .serialize(s)
            }
        }
    }
}
