use super::{MoneySource, Spend};
use crate::{journal::ResolvedLaunch, staffing::PriceBook};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub(super) struct Accounting {
    pub spend: Spend,
    pub launches: BTreeMap<(String, u32), ResolvedLaunch>,
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
        let tokens = u["total_tokens"]
            .as_u64()
            .or_else(|| input.zip(output).map(|(i, o)| i.saturating_add(o)));
        if let Some(n) = tokens {
            self.spend.tokens = self.spend.tokens.saturating_add(n);
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
    fn add(&mut self, amount: f64) {
        self.spend.money_usd = (self.spend.money_usd + amount).min(f64::MAX);
    }
}
