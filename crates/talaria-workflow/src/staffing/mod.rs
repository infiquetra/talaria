use crate::engine::Effort;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub const DEFAULT_MAX_IN_FLIGHT: u32 = 10;
pub fn talaria_home() -> Option<PathBuf> {
    todo!()
}
#[derive(Clone, Debug, PartialEq)]
pub struct StaffingTable {
    pub tiers: BTreeMap<String, TierSpec>,
    pub default_tier: Option<String>,
    pub limits: Limits,
    pub providers: BTreeMap<String, ProviderLimits>,
    pub budget: RunBudget,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TierSpec {
    pub model: String,
    pub provider: String,
    pub effort: Option<Effort>,
    pub note: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Limits {
    pub max_in_flight: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderLimits {
    pub max_in_flight: u32,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct RunBudget {
    pub tokens: Option<u64>,
    pub money_usd: Option<f64>,
}
impl RunBudget {
    pub fn resolve(
        _cli_tokens: Option<u64>,
        _cli_money_usd: Option<f64>,
        _table: &StaffingTable,
    ) -> Self {
        todo!()
    }
}
impl StaffingTable {
    pub const TEMPLATE: &'static str = "";
    pub fn parse(_source: &str) -> Result<Self, StaffingError> {
        todo!()
    }
    pub fn load(_path: &Path) -> Result<Self, StaffingError> {
        todo!()
    }
    pub fn default_path(_home: &Path) -> PathBuf {
        todo!()
    }
}
#[derive(Debug, thiserror::Error)]
pub enum StaffingError {
    #[error("{path}: {message}")]
    Io { path: String, message: String },
    #[error("{line}:{column}: {message}")]
    Toml {
        line: u32,
        column: u32,
        message: String,
    },
    #[error("{key}: {problem}")]
    Invalid { key: String, problem: String },
}
#[derive(Clone, Debug, PartialEq)]
pub enum PriceValue {
    Free,
    Usd(f64),
    Unknown,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Price {
    pub input: PriceValue,
    pub output: PriceValue,
    pub cache: PriceValue,
}
pub fn parse_price(_text: &str) -> PriceValue {
    todo!()
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderEntry {
    pub slug: String,
    pub name: String,
    pub authenticated: bool,
    pub models: Vec<String>,
    pub unavailable: std::collections::BTreeSet<String>,
    pub prices: BTreeMap<String, Price>,
    pub pricing_pending: bool,
    pub warning: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum LimitScope {
    Account,
    Model,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Availability {
    Missing,
    Unauthenticated,
    NotOffered,
    Unavailable,
    Limited {
        until: Option<chrono::DateTime<chrono::Utc>>,
        scope: LimitScope,
    },
    NearLimit {
        remaining_percent: u8,
        window: String,
    },
    Ready,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Roster;
#[derive(Debug, thiserror::Error)]
pub enum RosterError {
    #[error("rpc {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("not served: {0}")]
    NotServed(String),
    #[error("timeout")]
    Timeout,
    #[error("transport: {0}")]
    Transport(String),
    #[error("malformed: {0}")]
    Malformed(String),
}
impl Roster {
    pub fn from_model_options(
        _result: &serde_json::Value,
        _profile: &str,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Self, RosterError> {
        todo!()
    }
    pub fn profile(&self) -> &str {
        todo!()
    }
    pub fn current(&self) -> (&str, &str) {
        todo!()
    }
    pub fn providers(&self) -> &[ProviderEntry] {
        todo!()
    }
    pub fn availability(
        &self,
        _provider: &str,
        _model: &str,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> Availability {
        todo!()
    }
    pub fn price(&self, _provider: &str, _model: &str) -> Option<&Price> {
        todo!()
    }
    pub fn snapshot_with(&self, _table: &StaffingTable, _budget: &RunBudget) -> serde_json::Value {
        todo!()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum RouteSource {
    Tier(String),
    Explicit,
    ProfileDefault,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub model: String,
    pub provider: String,
    pub effort: Option<Effort>,
    pub source: RouteSource,
    pub ignored_tier: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RouteError(pub String);
#[derive(Clone)]
pub struct StaffingResolver;
impl StaffingResolver {
    pub fn new(_table: std::sync::Arc<StaffingTable>, _roster: std::sync::Arc<Roster>) -> Self {
        todo!()
    }
    pub fn with_path(self, _path: PathBuf) -> Self {
        todo!()
    }
    pub fn route(&self, _request: &crate::engine::AgentRequest) -> Result<Route, RouteError> {
        todo!()
    }
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct RosterCheck {
    pub problems: Vec<String>,
    pub warnings: Vec<String>,
}
impl RosterCheck {
    pub fn is_ok(&self) -> bool {
        todo!()
    }
}
pub fn check_tiers(
    _table: &StaffingTable,
    _roster: &Roster,
    _budget: &RunBudget,
    _now: chrono::DateTime<chrono::Utc>,
) -> RosterCheck {
    todo!()
}
#[derive(Clone, Debug, PartialEq)]
pub enum CostEstimate {
    Range {
        low_usd: f64,
        high_usd: f64,
        unpriced_tiers: Vec<String>,
    },
    Unknown,
    NeedsTokenBudget {
        tier_prices: Vec<(String, Price)>,
    },
}
impl std::fmt::Display for CostEstimate {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}
pub fn estimate_run_cost(
    _table: &StaffingTable,
    _roster: &Roster,
    _tokens: Option<u64>,
) -> CostEstimate {
    todo!()
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ProviderReport {
    pub slug: String,
    pub name: String,
    pub models: usize,
    pub availability: String,
    pub warning: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct TierReport {
    pub tier: String,
    pub route: String,
    pub price: String,
    pub status: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct LimitsReport {
    pub max_in_flight: u32,
    pub providers: BTreeMap<String, u32>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct RosterReport {
    pub profile: String,
    pub taken_at: String,
    pub providers: Vec<ProviderReport>,
    pub tiers: Vec<TierReport>,
    pub limits: LimitsReport,
    pub budget: RunBudget,
    pub check: RosterCheck,
    pub estimate: String,
}
impl RosterReport {
    pub fn build(
        _table: &StaffingTable,
        _roster: &Roster,
        _budget: &RunBudget,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        todo!()
    }
    pub fn to_text(&self) -> String {
        todo!()
    }
}
pub fn display_safe(_text: &str) -> String {
    todo!()
}
