//! Provider and model rows in Hermes's order.

use hermes_contract::schemas::{ModelOptionProvider, ModelOptionsResult, ProviderLimitScope};

/// Minutes east of UTC used to print a rate-limit clock. Tests pass a fixed offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayZone {
    pub minutes_east: i32,
}

/// One catalogue row a test or the picker can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogRow {
    pub kind: CatalogKind,
    pub text: String,
    pub dim: bool,
    pub provider_slug: String,
    pub provider_name: String,
    pub model: String,
    pub current: bool,
    pub favourite: bool,
    pub badges: Vec<String>,
    pub price: Option<String>,
    pub segments: Vec<String>,
}

/// Which group a row belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogKind {
    Header,
    Favourite,
    Model,
    NotSetUp,
}

/// A row the filter bench times. Headers are not included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterRow {
    pub provider_name: String,
    pub slug: String,
    pub model: String,
}

/// The catalogue Hermes returned, plus the favourite keys the caller supplies when listing rows.
#[derive(Clone, Debug)]
pub struct Catalog {
    pub(crate) result: ModelOptionsResult,
}

impl Catalog {
    pub fn from_result(result: &ModelOptionsResult) -> Self {
        Self {
            result: result.clone(),
        }
    }

    pub fn display_name(&self, slug: &str) -> Option<&str> {
        self.result
            .providers
            .iter()
            .find(|p| p.slug == slug)
            .map(provider_name)
    }

    /// Rows in picker order. `favourites` are `provider::model` keys in starring order.
    pub fn rows(&self, favourites: &[String], zone: DisplayZone) -> Vec<CatalogRow> {
        let mut rows = Vec::new();
        if !favourites.is_empty() {
            rows.push(header("Favourites", None, Vec::new()));
            for key in favourites {
                let Some((slug, model)) = key.split_once("::") else {
                    continue;
                };
                let provider = self.result.providers.iter().find(|p| p.slug == slug);
                let listed = provider.is_some_and(|p| {
                    p.authenticated == Some(true) && p.models.iter().any(|m| m == model)
                });
                let mut row = provider.map_or_else(
                    || bare_row(CatalogKind::Favourite, slug, slug, model),
                    |p| self.model_row(p, model, favourites, zone),
                );
                row.kind = CatalogKind::Favourite;
                row.favourite = true;
                row.text = format!("★ {} / {model}", row.provider_name);
                if !listed {
                    row.badges.push("not listed".into());
                    row.dim = true;
                }
                rows.push(row);
            }
        }
        for p in &self.result.providers {
            if p.authenticated != Some(true) {
                continue;
            }
            let mut segments = Vec::new();
            if p.is_current == Some(true) {
                segments.push("current".into());
            }
            if let Some(usage) = &p.usage {
                if !usage.windows.is_empty() {
                    segments.extend(
                        usage
                            .windows
                            .iter()
                            .take(2)
                            .map(|w| format!("{} {:.0}%", w.label, w.used_percent.round())),
                    );
                } else if let Some(accounts) = &usage.accounts {
                    let ready = accounts
                        .iter()
                        .filter(|a| a.state.to_string() == "ready")
                        .count();
                    let limited = accounts
                        .iter()
                        .filter(|a| a.state.to_string() == "limited")
                        .count();
                    let mut counts = Vec::new();
                    if ready != 0 {
                        counts.push(format!("{ready} ready"));
                    }
                    if limited != 0 {
                        counts.push(format!("{limited} limited"));
                    }
                    segments.push(if counts.is_empty() {
                        format!("{} accounts", accounts.len())
                    } else {
                        format!("{} accounts: {}", accounts.len(), counts.join(", "))
                    });
                }
            }
            if let Some(limit) = &p.limit
                && limit.scope == ProviderLimitScope::Account
            {
                segments.push(limit_text(limit.resets_at.as_deref(), zone));
            }
            if p.pricing_pending == Some(true) {
                segments.push("prices loading".into());
            }
            if p.free_tier == Some(true) {
                segments.push("free tier".into());
            }
            if let Some(total) = p.total_models
                && total > p.models.len() as i64
            {
                segments.push(format!("showing {} of {total}", p.models.len()));
            }
            rows.push(header(provider_name(p), Some(p), segments));
            rows.extend(
                p.models
                    .iter()
                    .map(|model| self.model_row(p, model, favourites, zone)),
            );
        }
        let missing: Vec<_> = self
            .result
            .providers
            .iter()
            .filter(|p| p.authenticated != Some(true))
            .collect();
        if !missing.is_empty() {
            rows.push(header(
                &format!("Not set up ({})", missing.len()),
                None,
                Vec::new(),
            ));
            for p in missing {
                let mut row = bare_row(CatalogKind::NotSetUp, &p.slug, provider_name(p), "");
                row.text = provider_name(p).into();
                if p.is_current == Some(true) {
                    row.text.push_str(" · ● current");
                }
                if let Some(key) = &p.key_env
                    && !key.is_empty()
                {
                    row.text.push_str(&format!(" · needs {key}"));
                }
                row.text.push_str(" · set up with hermes model");
                rows.push(row);
            }
        }
        rows
    }

    fn model_row(
        &self,
        p: &ModelOptionProvider,
        model: &str,
        favourites: &[String],
        zone: DisplayZone,
    ) -> CatalogRow {
        let mut row = bare_row(CatalogKind::Model, &p.slug, provider_name(p), model);
        row.current = self.result.model == model
            && (self.result.provider == p.slug
                || (self.result.provider.is_empty() && p.is_current == Some(true)));
        row.favourite = favourites
            .iter()
            .any(|key| key == &format!("{}::{model}", p.slug));
        row.text = format!(
            "{}{} {model}",
            if row.favourite { "★" } else { " " },
            if row.current { "●" } else { " " }
        );
        if let Some(caps) = p.capabilities.as_ref().and_then(|caps| caps.get(model)) {
            if caps.fast {
                row.badges.push("fast".into());
            }
            if !caps.reasoning {
                row.badges.push("no effort".into());
            }
        }
        if p.unavailable_models
            .as_ref()
            .is_some_and(|models| models.iter().any(|m| m == model))
        {
            row.badges.push("unavailable".into());
            row.dim = true;
        }
        if let Some(limit) = &p.limit
            && limit.scope == ProviderLimitScope::Models
            && let Some(reset) = limit.models.as_ref().and_then(|m| m.get(model))
        {
            row.badges.push(limit_text(Some(reset), zone));
        }
        if let Some(price) = p.pricing.as_ref().and_then(|prices| prices.get(model)) {
            row.price = if price.free {
                Some("free".into())
            } else if !price.input.is_empty()
                && !price.output.is_empty()
                && price.input != "?"
                && price.output != "?"
            {
                Some(format!("{} in · {} out", price.input, price.output))
            } else {
                None
            };
        }
        row
    }
}

fn provider_name(p: &ModelOptionProvider) -> &str {
    if p.name.trim().is_empty() {
        &p.slug
    } else {
        &p.name
    }
}

fn bare_row(kind: CatalogKind, slug: &str, name: &str, model: &str) -> CatalogRow {
    CatalogRow {
        kind,
        text: String::new(),
        dim: false,
        provider_slug: slug.into(),
        provider_name: name.into(),
        model: model.into(),
        current: false,
        favourite: false,
        badges: Vec::new(),
        price: None,
        segments: Vec::new(),
    }
}

fn header(text: &str, provider: Option<&ModelOptionProvider>, segments: Vec<String>) -> CatalogRow {
    let mut row = bare_row(
        CatalogKind::Header,
        provider.map_or("", |p| p.slug.as_str()),
        text,
        "",
    );
    row.text = text.into();
    row.segments = segments;
    row
}

fn limit_text(reset: Option<&str>, zone: DisplayZone) -> String {
    fn clock(reset: &str, zone: DisplayZone) -> Option<String> {
        let (date, rest) = reset.split_once('T')?;
        let mut date_parts = date.split('-');
        let year = date_parts.next()?.parse::<i32>().ok()?;
        let month = date_parts.next()?.parse::<u32>().ok()?;
        let day = date_parts.next()?.parse::<u32>().ok()?;
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            _ => return None,
        };
        if day == 0 || day > days || date_parts.next().is_some() {
            return None;
        }
        let hour = rest.get(0..2)?.parse::<i32>().ok()?;
        let minute = rest.get(3..5)?.parse::<i32>().ok()?;
        let second = rest.get(6..8)?.parse::<u32>().ok()?;
        if rest.get(2..3)? != ":"
            || rest.get(5..6)? != ":"
            || hour > 23
            || minute > 59
            || second > 60
        {
            return None;
        }
        let suffix = rest.get(8..)?;
        let tz = suffix.find(['+', '-', 'Z'])?;
        let offset = &suffix[tz..];
        let east = if offset == "Z" {
            0
        } else {
            let h = offset.get(1..3)?.parse::<i32>().ok()?;
            let m = offset.get(4..6)?.parse::<i32>().ok()?;
            if offset.len() != 6 || offset.get(3..4)? != ":" || h > 23 || m > 59 {
                return None;
            }
            (h * 60 + m) * if offset.starts_with('-') { -1 } else { 1 }
        };
        let total = (hour * 60 + minute - east + zone.minutes_east).rem_euclid(24 * 60);
        Some(format!("{:02}:{:02}", total / 60, total % 60))
    }
    reset.and_then(|s| clock(s, zone)).map_or_else(
        || "rate-limited".into(),
        |time| format!("rate-limited until {time}"),
    )
}

/// Indexes of the model rows whose provider name, slug and model contain every term.
pub fn filter_row_indexes(rows: &[FilterRow], filter: &str) -> Vec<usize> {
    let terms: Vec<_> = filter.split_whitespace().map(str::to_lowercase).collect();
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let text = format!("{} {} {}", row.provider_name, row.slug, row.model).to_lowercase();
            terms
                .iter()
                .all(|term| text.contains(term))
                .then_some(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_times_honor_both_offsets_and_reject_invalid_dates() {
        assert_eq!(
            limit_text(
                Some("2026-10-22T00:15:00+02:00"),
                DisplayZone { minutes_east: -300 }
            ),
            "rate-limited until 17:15"
        );
        assert_eq!(
            limit_text(
                Some("2026-02-30T18:00:00Z"),
                DisplayZone { minutes_east: 0 }
            ),
            "rate-limited"
        );
        assert_eq!(
            limit_text(None, DisplayZone { minutes_east: 0 }),
            "rate-limited"
        );
    }
}
