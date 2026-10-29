use crate::engine::Effort;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub const DEFAULT_MAX_IN_FLIGHT: u32 = 1;
pub fn talaria_home() -> Option<PathBuf> {
    std::env::var_os("TALARIA_CONFIG_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|s| !s.is_empty())
                .map(|p| PathBuf::from(p).join(".talaria"))
        })
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StaffingTable {
    pub tiers: BTreeMap<String, TierSpec>,
    pub default_tier: Option<String>,
    pub limits: Limits,
    pub providers: BTreeMap<String, ProviderLimits>,
    pub budget: RunBudget,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TierSpec {
    pub model: String,
    pub provider: String,
    pub effort: Option<Effort>,
    pub note: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Limits {
    pub max_in_flight: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProviderLimits {
    pub max_in_flight: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunBudget {
    pub tokens: Option<u64>,
    pub money_usd: Option<f64>,
}
impl RunBudget {
    pub fn resolve(
        cli_tokens: Option<u64>,
        cli_money_usd: Option<f64>,
        table: &StaffingTable,
    ) -> Self {
        Self {
            tokens: cli_tokens.or(table.budget.tokens),
            money_usd: cli_money_usd.or(table.budget.money_usd),
        }
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
fn invalid(key: &str, problem: impl Into<String>) -> StaffingError {
    StaffingError::Invalid {
        key: key.into(),
        problem: problem.into(),
    }
}
type Map = toml::map::Map<String, toml::Value>;
fn object<'a>(v: &'a toml::Value, key: &str, allowed: &[&str]) -> Result<&'a Map, StaffingError> {
    let map = v
        .as_table()
        .ok_or_else(|| invalid(key, "must be a table"))?;
    for k in map.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(invalid(&format!("{key}.{k}"), format!("unknown key '{k}'")));
        }
    }
    Ok(map)
}
fn width(v: Option<&toml::Value>, key: &str, default: u32) -> Result<u32, StaffingError> {
    match v {
        None => Ok(default),
        Some(v) => v
            .as_integer()
            .filter(|n| (1..=64).contains(n))
            .map(|n| n as u32)
            .ok_or_else(|| invalid(key, "max_in_flight must be between 1 and 64")),
    }
}
fn optional_string(map: &Map, field: &str, key: &str) -> Result<Option<String>, StaffingError> {
    map.get(field)
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid(&format!("{key}.{field}"), "must be a string"))
        })
        .transpose()
}
impl StaffingTable {
    pub const TEMPLATE: &'static str = r#"# Talaria staffing table: which Hermes model each workflow tier uses.
version = 1                     # optional; only 1 is accepted

# [defaults]
# tier = "builder"                # optional; used by agent() calls that name neither tier nor model
#
# [tiers.survey]                  # read-only sweeps, counting, listing
# model = "minimax-m3.1-flash"
# provider = "ollama-cloud"
# note = "free; good enough for listing files"   # optional, shown in the roster report
#
# [tiers.builder]
# model = "<a strong coding model>"
# provider = "<its provider slug>"
# effort = "high"                 # optional: none, minimal, low, medium, high, xhigh, max, ultra
#
# [tiers.judge]
# model = "<the strongest model available>"
# provider = "<its provider slug>"
# effort = "high"
#
[limits]
max_in_flight = 1               # 1..=64; default 1

# [providers.ollama-cloud]        # optional per-provider width
# max_in_flight = 4               # 1..=64; default: the global value
#
# [budget]                        # optional defaults; command-line flags (P5-06) win
# money_usd = 5.00                # > 0, finite
# tokens = 2000000                # > 0
"#;
    pub fn parse(source: &str) -> Result<Self, StaffingError> {
        let value: toml::Value = toml::from_str(source).map_err(|e: toml::de::Error| {
            let pos = e.span().map_or(0, |s| s.start).min(source.len());
            let before = &source[..pos];
            StaffingError::Toml {
                line: before.bytes().filter(|b| *b == b'\n').count() as u32 + 1,
                column: before
                    .rsplit('\n')
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .count() as u32
                    + 1,
                message: e.to_string(),
            }
        })?;
        let root = object(
            &value,
            "staffing",
            &[
                "version",
                "defaults",
                "tiers",
                "limits",
                "providers",
                "budget",
            ],
        )?;
        if root
            .get("version")
            .is_some_and(|v| v.as_integer() != Some(1))
        {
            return Err(invalid("version", "version must be 1"));
        }
        let mut table = Self {
            tiers: BTreeMap::new(),
            default_tier: None,
            limits: Limits {
                max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            },
            providers: BTreeMap::new(),
            budget: RunBudget {
                tokens: None,
                money_usd: None,
            },
        };
        if let Some(v) = root.get("tiers") {
            let tiers = v
                .as_table()
                .ok_or_else(|| invalid("tiers", "must be a table"))?;
            for (name, v) in tiers {
                let key = format!("tiers.{name}");
                let mut chars = name.bytes();
                if !(1..=32).contains(&name.len())
                    || !chars.next().is_some_and(|c| c.is_ascii_lowercase())
                    || !chars.all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_'
                    })
                {
                    return Err(invalid(
                        &key,
                        format!(
                            "tier name '{name}' must be lower case letters, digits, '-' or '_', at most 32"
                        ),
                    ));
                }
                let map = object(v, &key, &["model", "provider", "effort", "note"])?;
                let required = |field: &str| -> Result<String, StaffingError> {
                    map.get(field)
                        .and_then(toml::Value::as_str)
                        .filter(|s| {
                            !s.trim().is_empty() && (field != "model" || s.chars().count() <= 256)
                        })
                        .map(str::to_owned)
                        .ok_or_else(|| {
                            invalid(&format!("{key}.{field}"), format!("{field} is required"))
                        })
                };
                let model = required("model")?;
                let provider = required("provider")?;
                let effort=map.get("effort").map(|v| serde_json::from_value::<Effort>(serde_json::to_value(v).unwrap_or_default()).map_err(|_|invalid(&format!("{key}.effort"),"effort must be one of none, minimal, low, medium, high, xhigh, max, ultra"))).transpose()?;
                table.tiers.insert(
                    name.clone(),
                    TierSpec {
                        model,
                        provider,
                        effort,
                        note: optional_string(map, "note", &key)?,
                    },
                );
            }
        }
        if let Some(v) = root.get("defaults") {
            let map = object(v, "defaults", &["tier"])?;
            table.default_tier = optional_string(map, "tier", "defaults")?;
            if let Some(t) = &table.default_tier
                && !table.tiers.contains_key(t)
            {
                return Err(invalid(
                    "defaults.tier",
                    format!("defaults.tier names undefined tier '{t}'"),
                ));
            }
        }
        if let Some(v) = root.get("limits") {
            let map = object(v, "limits", &["max_in_flight"])?;
            table.limits.max_in_flight = width(
                map.get("max_in_flight"),
                "limits.max_in_flight",
                DEFAULT_MAX_IN_FLIGHT,
            )?;
        }
        if let Some(v) = root.get("providers") {
            for (slug, v) in v
                .as_table()
                .ok_or_else(|| invalid("providers", "must be a table"))?
            {
                let key = format!("providers.{slug}");
                let map = object(v, &key, &["max_in_flight"])?;
                table.providers.insert(
                    slug.clone(),
                    ProviderLimits {
                        max_in_flight: width(
                            map.get("max_in_flight"),
                            &format!("{key}.max_in_flight"),
                            table.limits.max_in_flight,
                        )?,
                    },
                );
            }
        }
        if let Some(v) = root.get("budget") {
            let map = object(v, "budget", &["tokens", "money_usd"])?;
            table.budget.tokens = map
                .get("tokens")
                .map(|v| {
                    v.as_integer()
                        .filter(|n| *n > 0)
                        .map(|n| n as u64)
                        .ok_or_else(|| {
                            invalid("budget.tokens", "tokens must be a positive integer")
                        })
                })
                .transpose()?;
            table.budget.money_usd = map
                .get("money_usd")
                .map(|v| {
                    v.as_float()
                        .or_else(|| v.as_integer().map(|n| n as f64))
                        .filter(|n| n.is_finite() && *n > 0.0)
                        .ok_or_else(|| {
                            invalid("budget.money_usd", "money_usd must be a positive number")
                        })
                })
                .transpose()?;
        }
        Ok(table)
    }
    pub fn load(path: &Path) -> Result<Self, StaffingError> {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::parse(&s).map_err(|e| match e {
                StaffingError::Toml {
                    line,
                    column,
                    message,
                } => StaffingError::Toml {
                    line,
                    column,
                    message: format!("{}: {message}", path.display()),
                },
                StaffingError::Invalid { key, problem } => StaffingError::Invalid {
                    key,
                    problem: format!("{}: {problem}", path.display()),
                },
                e => e,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::parse(""),
            Err(e) => Err(StaffingError::Io {
                path: path.display().to_string(),
                message: e.to_string(),
            }),
        }
    }
    pub fn default_path(home: &Path) -> PathBuf {
        home.join("staffing.toml")
    }
}
