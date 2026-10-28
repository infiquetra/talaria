use super::*;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RosterCheck {
    pub problems: Vec<String>,
    pub warnings: Vec<String>,
}
impl RosterCheck {
    pub fn is_ok(&self) -> bool {
        self.problems.is_empty()
    }
}
pub(crate) fn time_text(until: &Option<DateTime<Utc>>) -> String {
    until.map_or_else(
        || "an unknown time".into(),
        |t| t.format("%H:%M UTC").to_string(),
    )
}
pub(crate) fn availability_text(a: &Availability) -> String {
    match a {
        Availability::Limited { until, .. } => format!("limited until {}", time_text(until)),
        Availability::NearLimit {
            remaining_percent,
            window,
        } => format!("near limit ({remaining_percent}% of '{window}' left)"),
        Availability::Unauthenticated => "not signed in".into(),
        Availability::Missing => "not configured".into(),
        Availability::NotOffered => "model not offered".into(),
        Availability::Unavailable => "model unavailable".into(),
        Availability::Ready => "ready".into(),
    }
}
fn tier_problem(t: &TierSpec, a: &Availability, profile: &str) -> Option<String> {
    Some(match a {
        Availability::Missing => format!(
            "provider '{}' is not configured in profile '{profile}'",
            t.provider
        ),
        Availability::Unauthenticated => format!(
            "provider '{}' is not signed in; sign in with Talaria's provider screen or 'hermes model'",
            t.provider
        ),
        Availability::NotOffered => format!(
            "provider '{}' does not offer model '{}'",
            t.provider, t.model
        ),
        Availability::Unavailable => format!(
            "model '{}' is not available to this account on '{}'",
            t.model, t.provider
        ),
        _ => return None,
    })
}
pub fn check_tiers(
    table: &StaffingTable,
    roster: &Roster,
    budget: &RunBudget,
    now: DateTime<Utc>,
) -> RosterCheck {
    let mut c = RosterCheck {
        problems: vec![],
        warnings: vec![],
    };
    for (n, t) in &table.tiers {
        let a = roster.availability(&t.provider, &t.model, now);
        if let Some(p) = tier_problem(t, &a, roster.profile()) {
            c.problems.push(display_safe(&format!("tier '{n}': {p}")));
        }
        match a {
            Availability::Limited { until, .. } => c.warnings.push(display_safe(&format!(
                "tier '{n}': '{}' is rate-limited until {}; its agents will wait",
                t.provider,
                time_text(&until)
            ))),
            Availability::NearLimit {
                remaining_percent,
                window,
            } => c.warnings.push(display_safe(&format!(
                "tier '{n}': '{}' has {remaining_percent}% of '{window}' left",
                t.provider
            ))),
            _ => {}
        }
        if budget.money_usd.is_some()
            && roster
                .price(&t.provider, &t.model)
                .is_none_or(|p| !p.known())
        {
            c.warnings.push(display_safe(&format!(
                "tier '{n}': '{}' has no price; the money budget cannot see its cost",
                t.model
            )));
        }
    }
    c
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => f.write_str("unknown"),
            Self::Range {
                low_usd,
                high_usd,
                unpriced_tiers,
            } => {
                write!(f, "${low_usd:.2} to ${high_usd:.2}")?;
                if !unpriced_tiers.is_empty() {
                    write!(
                        f,
                        ", plus {} unpriced tiers (unknown)",
                        unpriced_tiers.len()
                    )?;
                }
                Ok(())
            }
            Self::NeedsTokenBudget { tier_prices } => write!(
                f,
                "no estimate without a token budget; tier prices: {}",
                tier_prices
                    .iter()
                    .map(|(n, p)| format!("{} {}", display_safe(n), p.text()))
                    .collect::<Vec<_>>()
                    .join(", ")
                    + " per million tokens"
            ),
        }
    }
}
pub fn estimate_run_cost(
    table: &StaffingTable,
    roster: &Roster,
    tokens: Option<u64>,
) -> CostEstimate {
    let entries: Vec<_> = table
        .tiers
        .iter()
        .map(|(n, t)| {
            (
                n.clone(),
                roster
                    .price(&t.provider, &t.model)
                    .cloned()
                    .unwrap_or_else(Price::unknown),
            )
        })
        .collect();
    let Some(tokens) = tokens else {
        return CostEstimate::NeedsTokenBudget {
            tier_prices: entries,
        };
    };
    let mut unknown = vec![];
    let mut low = f64::INFINITY;
    let mut high = 0.0_f64;
    for (n, p) in entries {
        if let (Some(i), Some(o)) = (p.input.dollars(), p.output.dollars()) {
            low = low.min(i.min(o));
            high = high.max(i.max(o));
        } else {
            unknown.push(n);
        }
    }
    if !low.is_finite() {
        CostEstimate::Unknown
    } else {
        CostEstimate::Range {
            low_usd: tokens as f64 * low / 1_000_000.0,
            high_usd: tokens as f64 * high / 1_000_000.0,
            unpriced_tiers: unknown,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProviderReport {
    pub slug: String,
    pub name: String,
    pub models: usize,
    pub availability: String,
    pub warning: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TierReport {
    pub tier: String,
    pub route: String,
    pub price: String,
    pub status: String,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LimitsReport {
    pub max_in_flight: u32,
    pub providers: BTreeMap<String, u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
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
        table: &StaffingTable,
        roster: &Roster,
        budget: &RunBudget,
        now: DateTime<Utc>,
    ) -> Self {
        let providers = roster
            .providers()
            .iter()
            .map(|p| ProviderReport {
                slug: display_safe(&p.slug),
                name: display_safe(&p.name),
                models: p.models.len(),
                availability: display_safe(&availability_text(&roster.availability(
                    &p.slug,
                    p.models.first().map(String::as_str).unwrap_or_default(),
                    now,
                ))),
                warning: p.warning.as_deref().map(display_safe),
            })
            .collect();
        let tiers = table
            .tiers
            .iter()
            .map(|(n, t)| {
                let a = roster.availability(&t.provider, &t.model, now);
                let price = roster
                    .price(&t.provider, &t.model)
                    .cloned()
                    .unwrap_or_else(Price::unknown);
                let status = if let Some(problem) = tier_problem(t, &a, roster.profile()) {
                    format!("problem: {problem}")
                } else {
                    match a {
                        Availability::Limited { until, .. } => {
                            format!("warning: rate-limited until {}", time_text(&until))
                        }
                        Availability::NearLimit {
                            remaining_percent,
                            window,
                        } => format!("warning: {remaining_percent}% of '{window}' left"),
                        _ if budget.money_usd.is_some() && !price.known() => {
                            "warning: no price".into()
                        }
                        _ => "ok".into(),
                    }
                };
                TierReport {
                    tier: display_safe(n),
                    route: display_safe(&format!(
                        "{} on {}{}",
                        t.model,
                        t.provider,
                        t.effort.map_or_else(String::new, |e| format!(
                            " (effort {})",
                            serde_json::to_value(e)
                                .ok()
                                .and_then(|v| v.as_str().map(str::to_owned))
                                .unwrap_or_default()
                        ))
                    )),
                    price: price.text(),
                    status: display_safe(&status),
                }
            })
            .collect();
        Self {
            profile: display_safe(roster.profile()),
            taken_at: now.format("%Y-%m-%d %H:%M UTC").to_string(),
            providers,
            tiers,
            limits: LimitsReport {
                max_in_flight: table.limits.max_in_flight,
                providers: table
                    .providers
                    .iter()
                    .map(|(n, v)| (display_safe(n), v.max_in_flight))
                    .collect(),
            },
            budget: budget.clone(),
            check: check_tiers(table, roster, budget, now),
            estimate: estimate_run_cost(table, roster, budget.tokens).to_string(),
        }
    }
    pub fn to_text(&self) -> String {
        let mut s = format!(
            "Roster for profile {} at {}\n\nProviders (signed in)\n",
            display_safe(&self.profile),
            display_safe(&self.taken_at)
        );
        // Model counts align independently of the human-readable provider name.
        let wp = self
            .providers
            .iter()
            .map(|p| p.slug.chars().count())
            .max()
            .unwrap_or(0);
        for p in &self.providers {
            s += &format!(
                "  {:wp$}  {:>2} models   {}\n",
                display_safe(&p.slug),
                p.models,
                display_safe(&p.availability)
            );
            if let Some(w) = &p.warning {
                s += &format!("    {}\n", display_safe(w));
            }
        }
        s += "\nTiers\n";
        let w1 = self
            .tiers
            .iter()
            .map(|t| t.tier.chars().count())
            .max()
            .unwrap_or(0);
        let w2 = self
            .tiers
            .iter()
            .map(|t| t.route.chars().count())
            .max()
            .unwrap_or(0);
        let w3 = self
            .tiers
            .iter()
            .map(|t| t.price.chars().count())
            .max()
            .unwrap_or(0);
        // Preserve the published report's per-row spacing, including its tier allowance.
        for t in &self.tiers {
            let route_width = w2 + t.tier.chars().count();
            s += &format!(
                "  {:w1$}   {:route_width$}{:w3$}  {}\n",
                display_safe(&t.tier),
                display_safe(&t.route),
                display_safe(&t.price),
                display_safe(&t.status)
            );
        }
        s += &format!("\nLimits: {} agents at once", self.limits.max_in_flight);
        if !self.limits.providers.is_empty() {
            s += &format!(
                "; per provider: {}",
                self.limits
                    .providers
                    .iter()
                    .map(|(n, w)| format!("{} {w}", display_safe(n)))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        s.push('\n');
        let mut b = vec![];
        if let Some(n) = self.budget.tokens {
            b.push(format!("{n} tokens"));
        }
        if let Some(n) = self.budget.money_usd {
            b.push(format!("${n:.2}"));
        }
        s += &format!(
            "Budget: {}\nEstimate: {}\n",
            if b.is_empty() {
                "none".into()
            } else {
                b.join(", ")
            },
            display_safe(&self.estimate)
        );
        s
    }
}
pub fn display_safe(text: &str) -> String {
    text.chars().map(|c|if c.is_control()||matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'){'?'}else{c}).collect()
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TierDrift {
    pub tier: String,
    pub before: String,
    pub after: String,
}
impl std::fmt::Display for TierDrift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "tier '{}' changed: {} -> {}; finished agents on these tiers will run again",
            display_safe(&self.tier),
            display_safe(&self.before),
            display_safe(&self.after)
        )
    }
}
pub fn staffing_drift(stored: &Value, table: &StaffingTable) -> Vec<TierDrift> {
    let old = stored["tiers"].as_object();
    let names: BTreeSet<_> = table
        .tiers
        .keys()
        .cloned()
        .chain(old.into_iter().flat_map(|m| m.keys().cloned()))
        .collect();
    let mut changes = vec![];
    for tier in names {
        let before = old.and_then(|m| m.get(&tier));
        let after = table
            .tiers
            .get(&tier)
            .and_then(|t| serde_json::to_value(t).ok());
        let sig = |v: Option<&Value>| v.map(|v| json!([v["model"], v["provider"], v["effort"]]));
        if sig(before) == sig(after.as_ref()) {
            continue;
        }
        let label = |v: Option<&Value>| {
            v.map_or_else(
                || "(absent)".into(),
                |v| {
                    format!(
                        "{} on {}{}",
                        v["model"].as_str().unwrap_or_default(),
                        v["provider"].as_str().unwrap_or_default(),
                        v["effort"]
                            .as_str()
                            .map_or_else(String::new, |e| format!(" (effort {e})"))
                    )
                },
            )
        };
        changes.push(TierDrift {
            tier,
            before: label(before),
            after: label(after.as_ref()),
        });
    }
    changes
}
