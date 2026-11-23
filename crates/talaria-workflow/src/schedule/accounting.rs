use super::{MoneySource, Spend};
use crate::{
    journal::ResolvedLaunch,
    staffing::PriceBook,
    tokens::{CacheTracker, Reading, Tokens},
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub(super) struct Accounting {
    pub spend: Spend,
    pub launches: BTreeMap<(String, u32), ResolvedLaunch>,
    pub labels: BTreeMap<(String, u32), String>,
    pub sessions: BTreeMap<(String, u32), String>,
    /// Settled tokens and the latest cumulative live reading, per Hermes session.
    settled: BTreeMap<String, Tokens>,
    pub live: BTreeMap<String, Reading>,
    cache: CacheTracker<(String, u32)>,
    last: BTreeMap<(String, u32), f64>,
    seen: BTreeSet<(String, u32, u32)>,
    hermes: bool,
    pricing: bool,
}
impl Accounting {
    pub fn settle(
        &mut self,
        key: &str,
        attempt: u32,
        turn: u32,
        usage: Option<&Value>,
        book: &PriceBook,
    ) {
        if !self.seen.insert((key.into(), attempt, turn)) {
            return;
        }
        let u = usage.unwrap_or(&Value::Null);
        let input = u["input_tokens"].as_u64();
        let output = u["output_tokens"].as_u64();
        if let Some(turn_tokens) = self.cache.settle((key.into(), attempt), u) {
            let n = turn_tokens.tokens;
            self.spend.tokens = self.spend.tokens.saturating_add(n.counted);
            self.spend.cache_read_tokens =
                self.spend.cache_read_tokens.saturating_add(n.cache_read);
            if !turn_tokens.cache_known {
                self.spend.cache_unknown_turns = self.spend.cache_unknown_turns.saturating_add(1);
            }
            if let Some(session) = self.sessions.get(&(key.into(), attempt)) {
                let settled = self.settled.entry(session.clone()).or_default();
                *settled = settled.plus(n);
            }
        } else {
            self.spend.tokens_unknown_turns = self.spend.tokens_unknown_turns.saturating_add(1);
        }
        let cost = &u["session_cost"];
        let known = matches!(
            cost["cost_status"].as_str(),
            Some("actual" | "estimated" | "included")
        );
        let amount = cost["estimated_cost_usd"]
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0.0);
        if let Some(amount) = amount.filter(|_| known) {
            let last = self.last.entry((key.into(), attempt)).or_default();
            let increment = if amount >= *last {
                amount - *last
            } else {
                amount
            };
            *last = amount;
            self.add(increment);
            self.hermes = true;
        } else {
            let price = self.launches.get(&(key.into(), attempt)).and_then(|l| {
                book.price(
                    &l.profile,
                    l.provider.as_deref().unwrap_or_default(),
                    &l.model,
                )
            });
            let priced = price
                .and_then(|p| Some((p.input.dollars()?, p.output.dollars()?)))
                .and_then(|(i, o)| {
                    Some(input? as f64 * i / 1_000_000.0 + output? as f64 * o / 1_000_000.0)
                });
            if let Some(cost) = priced.filter(|v| v.is_finite()) {
                self.add(cost);
                self.pricing = true;
            } else {
                self.spend.money_unknown_turns = self.spend.money_unknown_turns.saturating_add(1);
            }
        }
        self.spend.money_source = match (self.hermes, self.pricing) {
            (true, true) => MoneySource::Mixed,
            (true, false) => MoneySource::Hermes,
            (false, true) => MoneySource::Pricing,
            _ => MoneySource::None,
        };
    }
    /// Keep the highest cumulative reading; returns whether the tokens the budget counts grew.
    pub fn observe(&mut self, session: &str, reading: Reading) -> bool {
        let old = self.live.get(session).copied();
        let kept = Reading::latest(old, reading);
        self.live.insert(session.into(), kept);
        kept.tokens().counted > old.map_or(0, |r| r.tokens().counted)
    }
    /// Settled spend plus each session's live reading beyond what has settled. A session's
    /// readings are cumulative, so a settled turn replaces its live share instead of adding to it.
    /// `tokens` leaves out cache reads, which are counted in `cache_read_tokens`.
    pub fn current(&self) -> Spend {
        let live = self
            .live
            .iter()
            .map(|(s, r)| {
                r.tokens()
                    .beyond(self.settled.get(s).copied().unwrap_or_default())
            })
            .fold(Tokens::default(), Tokens::plus);
        Spend {
            tokens: self.spend.tokens.saturating_add(live.counted),
            cache_read_tokens: self.spend.cache_read_tokens.saturating_add(live.cache_read),
            ..self.spend.clone()
        }
    }
    fn add(&mut self, amount: f64) {
        self.spend.money_usd = (self.spend.money_usd + amount).min(f64::MAX);
    }
}
