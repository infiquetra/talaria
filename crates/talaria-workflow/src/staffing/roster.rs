use super::{Price, RunBudget, StaffingTable, parse_price};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProviderEntry {
    pub slug: String,
    pub name: String,
    pub authenticated: bool,
    pub models: Vec<String>,
    pub unavailable: BTreeSet<String>,
    pub prices: BTreeMap<String, Price>,
    pub pricing_pending: bool,
    pub warning: Option<String>,
    #[serde(skip)]
    price_strings: BTreeMap<String, (String, String)>,
    #[serde(skip)]
    limit: Option<Value>,
    #[serde(skip)]
    usage: Option<Value>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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
        until: Option<DateTime<Utc>>,
        scope: LimitScope,
    },
    NearLimit {
        remaining_percent: u8,
        window: String,
    },
    Ready,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Roster {
    profile: String,
    current_model: String,
    current_provider: String,
    providers: Vec<ProviderEntry>,
    taken_at: DateTime<Utc>,
}
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
pub(crate) fn parse_time(v: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(v?)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}
fn live(w: &Value, now: DateTime<Utc>) -> bool {
    w["used_percent"].as_f64().is_some_and(f64::is_finite)
        && parse_time(w["resets_at"].as_str()).is_none_or(|t| t > now)
}
fn values(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array().into_iter().flatten()
}
fn strings(v: &Value) -> impl Iterator<Item = String> + '_ {
    values(v).filter_map(Value::as_str).map(str::to_owned)
}
impl Roster {
    pub fn from_model_options(
        result: &Value,
        profile: &str,
        now: DateTime<Utc>,
    ) -> Result<Self, RosterError> {
        let rows = result["providers"]
            .as_array()
            .ok_or_else(|| RosterError::Malformed("providers must be an array".into()))?;
        let mut providers = Vec::new();
        for row in rows {
            if row["slug"].as_str().is_none() || row["name"].as_str().is_none() {
                tracing::warn!("model.options row without slug or name skipped");
                continue;
            }
            let mut prices = BTreeMap::new();
            let mut price_strings = BTreeMap::new();
            if let Some(entries) = row["pricing"].as_object() {
                for (model, p) in entries {
                    let free = p["free"] == true;
                    let input = if free {
                        "free"
                    } else {
                        p["input"].as_str().unwrap_or_default()
                    };
                    let output = if free {
                        "free"
                    } else {
                        p["output"].as_str().unwrap_or_default()
                    };
                    prices.insert(
                        model.clone(),
                        Price {
                            input: parse_price(input),
                            output: parse_price(output),
                            cache: parse_price(p["cache"].as_str().unwrap_or_default()),
                        },
                    );
                    price_strings.insert(model.clone(), (input.to_owned(), output.to_owned()));
                }
            }
            providers.push(ProviderEntry {
                slug: row["slug"].as_str().unwrap_or_default().into(),
                name: row["name"].as_str().unwrap_or_default().into(),
                authenticated: row["authenticated"].as_bool().unwrap_or(true),
                models: strings(&row["models"]).collect(),
                unavailable: strings(&row["unavailable_models"]).collect(),
                prices, price_strings,
                pricing_pending: row["pricing_pending"] == true,
                warning: row["warning"].as_str().map(str::to_owned),
                limit: row.get("limit").filter(|v| v.is_object()).map(|v| json!({"scope":v["scope"], "resets_at":v["resets_at"], "models":v["models"]})),
                usage: row.get("usage").filter(|v| v.is_object()).cloned(),
            });
        }
        Ok(Self {
            profile: profile.into(),
            current_model: result["model"].as_str().unwrap_or_default().into(),
            current_provider: result["provider"].as_str().unwrap_or_default().into(),
            providers,
            taken_at: now,
        })
    }
    pub fn profile(&self) -> &str {
        &self.profile
    }
    pub fn current(&self) -> (&str, &str) {
        (&self.current_model, &self.current_provider)
    }
    pub fn providers(&self) -> &[ProviderEntry] {
        &self.providers
    }
    pub(crate) fn taken_at(&self) -> DateTime<Utc> {
        self.taken_at
    }
    pub fn availability(&self, provider: &str, model: &str, now: DateTime<Utc>) -> Availability {
        let Some(p) = self.providers.iter().find(|p| p.slug == provider) else {
            return Availability::Missing;
        };
        if !p.authenticated {
            return Availability::Unauthenticated;
        }
        if !p.models.iter().any(|m| m == model) {
            return Availability::NotOffered;
        }
        if p.unavailable.contains(model) {
            return Availability::Unavailable;
        }
        if let Some(limit) = &p.limit {
            match limit["scope"].as_str() {
                Some("account") => {
                    let t = parse_time(limit["resets_at"].as_str());
                    if t.is_none_or(|t| t > now) {
                        return Availability::Limited {
                            until: t,
                            scope: LimitScope::Account,
                        };
                    }
                }
                Some("models") => {
                    if let Some(t) = parse_time(limit["models"][model].as_str())
                        && t > now
                    {
                        return Availability::Limited {
                            until: Some(t),
                            scope: LimitScope::Model,
                        };
                    }
                }
                _ => {}
            }
        }
        if let Some(u) = &p.usage {
            let accounts: Vec<_> = values(&u["accounts"]).collect();
            if !accounts.is_empty() {
                if accounts.iter().all(|a| {
                    a["state"] == "limited"
                        && parse_time(a["resets_at"].as_str()).is_none_or(|t| t > now)
                }) {
                    return Availability::Limited {
                        until: accounts
                            .iter()
                            .filter_map(|a| parse_time(a["resets_at"].as_str()))
                            .filter(|t| *t > now)
                            .min(),
                        scope: LimitScope::Account,
                    };
                }
            } else {
                let exhausted: Vec<_> = values(&u["windows"])
                    .filter(|w| {
                        live(w, now)
                            && w["scope"] != "model"
                            && w["used_percent"].as_f64().is_some_and(|p| p >= 100.0)
                    })
                    .collect();
                if !exhausted.is_empty() {
                    let times: Option<Vec<_>> = exhausted
                        .iter()
                        .map(|w| parse_time(w["resets_at"].as_str()))
                        .collect();
                    return Availability::Limited {
                        until: times.and_then(|t| t.into_iter().max()),
                        scope: LimitScope::Account,
                    };
                }
                if let Some(w) = values(&u["windows"])
                    .filter(|w| live(w, now))
                    .max_by(|a, b| {
                        a["used_percent"]
                            .as_f64()
                            .unwrap_or_default()
                            .total_cmp(&b["used_percent"].as_f64().unwrap_or_default())
                    })
                {
                    let remaining = (100.0 - w["used_percent"].as_f64().unwrap_or_default())
                        .round()
                        .clamp(0.0, 100.0) as u8;
                    if remaining <= 20 {
                        return Availability::NearLimit {
                            remaining_percent: remaining,
                            window: w["label"].as_str().unwrap_or_default().into(),
                        };
                    }
                }
            }
        }
        Availability::Ready
    }
    pub fn price(&self, provider: &str, model: &str) -> Option<&Price> {
        self.providers
            .iter()
            .find(|p| p.slug == provider)?
            .prices
            .get(model)
    }
    pub fn snapshot_with(&self, table: &StaffingTable, budget: &RunBudget) -> Value {
        let providers:Vec<_>=self.providers.iter().map(|p|json!({"slug":p.slug,"name":p.name,"authenticated":p.authenticated,"models":p.models.len(),"availability":super::report::availability_text(&self.availability(&p.slug,p.models.first().map(String::as_str).unwrap_or_default(),self.taken_at)),"limit":p.limit})).collect();
        let tiers:BTreeMap<_,_>=table.tiers.iter().map(|(n,t)|{let p=self.price(&t.provider,&t.model).cloned().unwrap_or_else(Price::unknown);(n,json!({"model":t.model,"provider":t.provider,"effort":t.effort,"price":{"input":self.providers.iter().find(|p|p.slug==t.provider).and_then(|p|p.price_strings.get(&t.model)).map_or_else(||p.input.wire(),|s|s.0.clone()),"output":self.providers.iter().find(|p|p.slug==t.provider).and_then(|p|p.price_strings.get(&t.model)).map_or_else(||p.output.wire(),|s|s.1.clone())}}))}).collect();
        let prices:BTreeMap<_,BTreeMap<_,_>>=self.providers.iter().map(|p|(&p.slug,p.prices.iter().filter(|(_,p)|p.input.dollars().is_some()||p.output.dollars().is_some()).map(|(m,price)|(m,json!({"input":p.price_strings.get(m).map_or_else(||price.input.wire(),|s|s.0.clone()),"output":p.price_strings.get(m).map_or_else(||price.output.wire(),|s|s.1.clone())}))).collect())).collect();
        json!({"v":1,"taken_at":self.taken_at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi,true),"profile":self.profile,"current":{"model":self.current_model,"provider":self.current_provider},"providers":providers,"tiers":tiers,"prices":prices,"limits":{"max_in_flight":table.limits.max_in_flight,"providers":table.providers.iter().map(|(p,l)|(p,l.max_in_flight)).collect::<BTreeMap<_,_>>()},"budget":budget})
    }
}

impl Serialize for Availability {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
        Self::Missing=>s.serialize_str("missing"),Self::Unauthenticated=>s.serialize_str("unauthenticated"),Self::NotOffered=>s.serialize_str("not_offered"),Self::Unavailable=>s.serialize_str("unavailable"),Self::Ready=>s.serialize_str("ready"),
        Self::Limited{until,scope}=>serde_json::json!({"kind":"limited","until":until,"scope":scope}).serialize(s),
        Self::NearLimit{remaining_percent,window}=>serde_json::json!({"kind":"near_limit","remaining_percent":remaining_percent,"window":window}).serialize(s),
    }
    }
}
