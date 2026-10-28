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
