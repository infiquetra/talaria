use super::{Availability, Roster, StaffingTable, talaria_home};
use crate::engine::{AgentRequest, Effort};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteSource {
    Tier(String),
    Explicit,
    ProfileDefault,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Route {
    pub model: String,
    pub provider: String,
    pub effort: Option<Effort>,
    pub source: RouteSource,
    pub ignored_tier: Option<String>,
}
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct RouteError(pub String);
#[derive(Clone)]
pub struct StaffingResolver {
    table: Arc<StaffingTable>,
    roster: Arc<Roster>,
    path: PathBuf,
}
impl StaffingResolver {
    pub fn new(table: Arc<StaffingTable>, roster: Arc<Roster>) -> Self {
        Self {
            table,
            roster,
            path: StaffingTable::default_path(&talaria_home().unwrap_or_default()),
        }
    }
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = path;
        self
    }
    pub fn route(&self, q: &AgentRequest) -> Result<Route, RouteError> {
        let profile = q.profile.as_deref().unwrap_or(self.roster.profile());
        let same_profile = || {
            if profile == self.roster.profile() {
                Ok(())
            } else {
                Err(RouteError(format!(
                    "profile '{profile}' was not read at run start; name a model and provider, or a tier"
                )))
            }
        };
        let mut route = Route {
            model: String::new(),
            provider: String::new(),
            effort: q.effort,
            source: RouteSource::Explicit,
            ignored_tier: None,
        };
        if let Some(model) = &q.model {
            route.model = model.clone();
            route.ignored_tier = q.tier.clone();
            route.provider = if let Some(p) = &q.provider {
                p.clone()
            } else {
                same_profile()?;
                let candidates: Vec<_> = self
                    .roster
                    .providers()
                    .iter()
                    .filter(|p| p.authenticated && p.models.contains(model))
                    .map(|p| p.slug.clone())
                    .collect();
                match candidates.as_slice() {
                    [] => {
                        return Err(RouteError(format!(
                            "model '{model}' is not offered by any signed-in provider of profile '{profile}'"
                        )));
                    }
                    [p] => p.clone(),
                    _ => {
                        return Err(RouteError(format!(
                            "model '{model}' is offered by {}; name a provider",
                            candidates.join(", ")
                        )));
                    }
                }
            };
        } else if let Some(p) = &q.provider {
            return Err(RouteError(format!(
                "provider '{p}' needs a model; to move a tier to another provider, edit staffing.toml"
            )));
        } else if let Some(tier) = q.tier.as_ref().or(self.table.default_tier.as_ref()) {
            let t = self.table.tiers.get(tier).ok_or_else(|| {
                RouteError(if self.table.tiers.is_empty() {
                    format!(
                        "tier '{tier}' needs a staffing table; create {}",
                        self.path.display()
                    )
                } else {
                    format!(
                        "tier '{tier}' is not in staffing.toml ({}); defined tiers: {}",
                        self.path.display(),
                        self.table
                            .tiers
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
            })?;
            route.model = t.model.clone();
            route.provider = t.provider.clone();
            route.effort = q.effort.or(t.effort);
            route.source = RouteSource::Tier(tier.clone());
        } else {
            same_profile()?;
            let (m, p) = self.roster.current();
            if m.is_empty() || p.is_empty() {
                return Err(RouteError(format!(
                    "no tier, no model, and profile '{profile}' has no default model"
                )));
            }
            route.model = m.into();
            route.provider = p.into();
            route.source = RouteSource::ProfileDefault;
        }
        if profile == self.roster.profile() {
            let a = self
                .roster
                .availability(&route.provider, &route.model, self.roster.taken_at());
            if let Some(reason) = unusable(&a) {
                return Err(RouteError(format!(
                    "model '{}' on '{}' is not usable: {reason}",
                    route.model, route.provider
                )));
            }
        }
        Ok(route)
    }
}
pub(crate) fn unusable(a: &Availability) -> Option<&'static str> {
    match a {
        Availability::Missing => Some("provider is not configured"),
        Availability::Unauthenticated => Some("provider is not signed in"),
        Availability::NotOffered => Some("model is not offered"),
        Availability::Unavailable => Some("model is not available to this account"),
        _ => None,
    }
}
