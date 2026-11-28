//! How a run counts tokens. Cache reads are cheap, so they are kept as their own number and the
//! token budget does not count them; every other token, cache writes included, counts.
//!
//! Hermes reports a turn's tokens in two shapes. A turn that has settled carries the tokens it
//! used with cache reads folded in, plus the session's running cache read total. A live
//! `session.usage` event carries the session's running totals, again with cache reads folded into
//! `total_tokens` and also given on their own. This module turns both into [`Tokens`].
use hermes_contract::schemas::SessionUsagePayload;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// The key a settled turn's journaled usage uses for the session's running cache read total, as
/// Hermes reported it with the turn. It is a running total, not this turn's share.
pub const SESSION_CACHE_READ_KEY: &str = "session_cache_read_tokens";

mod stored;
pub(crate) use stored::recover_stored_cache;

/// A token count split the way the budget counts it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Tokens {
    /// Everything except cache reads. This is what the token budget measures. When a turn's
    /// cache reads are unknown, the whole turn is here.
    pub counted: u64,
    /// Cache reads Hermes reported.
    pub cache_read: u64,
}

impl Tokens {
    pub fn total(self) -> u64 {
        self.counted.saturating_add(self.cache_read)
    }
    pub fn plus(self, other: Self) -> Self {
        Self {
            counted: self.counted.saturating_add(other.counted),
            cache_read: self.cache_read.saturating_add(other.cache_read),
        }
    }
    /// What this reading holds beyond `settled`: a settled turn replaces its live share rather
    /// than adding to it. The subtraction is made on the whole, then the cache reads that remain
    /// are taken out, so a settled turn whose cache reads were unknown (counted whole) never
    /// leaves its cache reads behind to be shown a second time.
    pub fn beyond(self, settled: Self) -> Self {
        let total = self.total().saturating_sub(settled.total());
        let cache_read = self
            .cache_read
            .saturating_sub(settled.cache_read)
            .min(total);
        Self {
            counted: total - cache_read,
            cache_read,
        }
    }
}

/// A session's running token total as Hermes streams it, with its cache reads when it says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    /// Every token, cache reads included.
    pub total: u64,
    /// How many of them were cache reads; `None` when the event did not say.
    pub cache_read: Option<u64>,
}

impl Reading {
    /// The reading split for the budget. Unknown cache reads count as none, so the whole total
    /// is counted.
    pub fn tokens(self) -> Tokens {
        let cache_read = self.cache_read.unwrap_or(0).min(self.total);
        Tokens {
            counted: self.total - cache_read,
            cache_read,
        }
    }
    /// The later of two readings of one session. Readings only grow, so a lower total is a stale
    /// event; an equal total replaces an old reading only to add the cache reads it lacked.
    pub fn latest(old: Option<Self>, new: Self) -> Self {
        match old {
            Some(old) if new.total < old.total => old,
            Some(old) if new.total == old.total && old.cache_read.is_some() => old,
            _ => new,
        }
    }
    /// A `session.usage` event: the running totals on the event, else the status bar view inside
    /// it. Hermes folds cache reads into `total_tokens` and `prompt_tokens` but not into
    /// `input_tokens`, so a total rebuilt from input and output adds them back.
    pub fn from_payload(p: &SessionUsagePayload) -> Option<Self> {
        let big = |n: i64| u64::try_from(n).ok();
        let cache_read = p
            .cache_read_tokens
            .or_else(|| p.usage.as_ref().and_then(|u| u.cache_read))
            .and_then(big);
        let cache_write = p.cache_write_tokens.and_then(big).unwrap_or(0);
        let total = p
            .total_tokens
            .and_then(big)
            .or_else(|| big(p.prompt_tokens?)?.checked_add(big(p.completion_tokens?)?))
            .or_else(|| {
                big(p.input_tokens?)?
                    .checked_add(big(p.output_tokens?)?)?
                    .checked_add(cache_read.unwrap_or(0))?
                    .checked_add(cache_write)
            })?;
        Some(Self { total, cache_read })
    }
    /// The status bar view Hermes attaches to `message.complete`.
    pub fn from_status_view(v: &Value) -> Option<Self> {
        let cache_read = v["cache_read"].as_u64();
        let cache_write = v["cache_write"].as_u64().unwrap_or(0);
        let total = v["total"]
            .as_u64()
            .or_else(|| v["prompt"].as_u64()?.checked_add(v["completion"].as_u64()?))
            .or_else(|| {
                v["input"]
                    .as_u64()?
                    .checked_add(v["output"].as_u64()?)?
                    .checked_add(cache_read.unwrap_or(0))?
                    .checked_add(cache_write)
            })?;
        Some(Self { total, cache_read })
    }
    /// The answer to `session.stats`: running counters for the whole session.
    pub fn from_stats(v: &Value) -> Option<Self> {
        Some(Self {
            total: v["total_tokens"].as_u64()?,
            cache_read: v["cache_read_tokens"].as_u64(),
        })
    }
}

/// The tokens of one settled turn, cache reads included: `total_tokens`, else input plus output.
pub fn turn_total(usage: &Value) -> Option<u64> {
    usage["total_tokens"].as_u64().or_else(|| {
        usage["input_tokens"]
            .as_u64()?
            .checked_add(usage["output_tokens"].as_u64()?)
    })
}

/// Accept a session reading for a single turn only when the gross prompt and output
/// counts agree with its receipt. The prompt includes cache reads and writes.
pub(crate) fn matching_cache_read(
    usage: &Value,
    prompt: u64,
    output: u64,
    cache_read: u64,
) -> Option<u64> {
    let total = prompt.checked_add(output)?;
    (total > 0
        && turn_total(usage) == Some(total)
        && usage["input_tokens"].as_u64() == Some(prompt)
        && usage["output_tokens"].as_u64() == Some(output)
        && cache_read <= prompt)
        .then_some(cache_read)
}

/// One settled turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settled {
    pub tokens: Tokens,
    /// Whether Hermes reported the turn's cache reads. When it did not, the whole turn is in
    /// `tokens.counted`.
    pub cache_known: bool,
}

/// Turns a settled turn's usage into a split. Hermes gives a settled turn's tokens for that turn
/// but its cache reads as the session's running total, so this remembers the last running total
/// of each scope (one agent attempt) and takes the difference. Feed it a run's turns in journal
/// order and it gives the same answer live and on resume.
///
/// A running total that went down means the counter started again, as it does when the gateway
/// restarts or builds a new agent for the session, so the new value is this turn's own. A counter
/// that started again and passed its old value looks like a small increase; that understates
/// cache reads, which overstates the counted tokens. A turn's cache reads never exceed its total.
#[derive(Clone, Debug, Default)]
pub struct CacheTracker<K: Ord> {
    previous: BTreeMap<K, u64>,
}

impl<K: Ord> CacheTracker<K> {
    pub fn new() -> Self {
        Self {
            previous: BTreeMap::new(),
        }
    }
    /// `None` when the turn reported no token total at all.
    pub fn settle(&mut self, scope: K, usage: &Value) -> Option<Settled> {
        let Some(total) = turn_total(usage) else {
            // Keep the running total current, or the next turn would take this turn's cache
            // reads for its own.
            if let Some(running) = usage[SESSION_CACHE_READ_KEY].as_u64() {
                self.previous.insert(scope, running);
            }
            return None;
        };
        Some(match usage[SESSION_CACHE_READ_KEY].as_u64() {
            Some(running) => {
                let before = self.previous.insert(scope, running).unwrap_or(0);
                let share = if running >= before {
                    running - before
                } else {
                    running
                };
                let cache_read = share.min(total);
                Settled {
                    tokens: Tokens {
                        counted: total - cache_read,
                        cache_read,
                    },
                    cache_known: true,
                }
            }
            None => Settled {
                tokens: Tokens {
                    counted: total,
                    cache_read: 0,
                },
                cache_known: false,
            },
        })
    }
}

/// Token totals as a person reads them, split the way the budget counts. `show` formats one
/// count, so a caller chooses whole digits or "1.2k".
pub fn tokens_text(
    counted: u64,
    cache_read: u64,
    cache_unknown_turns: u32,
    show: impl Fn(u64) -> String,
) -> String {
    let base = if cache_unknown_turns > 0 && cache_read == 0 {
        format!("up to {} tokens", show(counted))
    } else if cache_unknown_turns > 0 {
        format!(
            "up to {} tokens + {} known cache reads",
            show(counted),
            show(cache_read)
        )
    } else {
        format!(
            "{} tokens + {} cache reads",
            show(counted),
            show(cache_read)
        )
    };
    match cache_unknown_turns {
        0 => base,
        n => format!(
            "{base} (cache reads unknown for {n} {}, counted in full)",
            if n == 1 { "turn" } else { "turns" }
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn usage(total: u64, running_cache: Option<u64>) -> Value {
        match running_cache {
            Some(c) => json!({"total_tokens": total, SESSION_CACHE_READ_KEY: c}),
            None => json!({"total_tokens": total}),
        }
    }

    #[test]
    fn a_turn_with_no_total_still_moves_the_running_cache_figure_on() {
        let mut tracker = CacheTracker::new();
        tracker.settle("a", &usage(1000, Some(400))).unwrap();
        // Hermes can send a turn with a running cache figure and no token total.
        assert!(
            tracker
                .settle("a", &json!({ SESSION_CACHE_READ_KEY: 900 }))
                .is_none()
        );
        let turn = tracker.settle("a", &usage(1000, Some(1000))).unwrap();
        assert_eq!(
            turn.tokens,
            Tokens {
                counted: 900,
                cache_read: 100
            }
        );
    }

    #[test]
    fn a_known_split_takes_cache_reads_out_of_the_counted_tokens() {
        let mut tracker = CacheTracker::new();
        let turn = tracker.settle("a", &usage(3170, Some(1601))).unwrap();
        assert!(turn.cache_known);
        assert_eq!(
            turn.tokens,
            Tokens {
                counted: 1569,
                cache_read: 1601
            }
        );
    }

    #[test]
    fn each_turn_takes_the_difference_of_the_running_cache_total() {
        let mut tracker = CacheTracker::new();
        let first = tracker.settle("a", &usage(1000, Some(800))).unwrap();
        let second = tracker.settle("a", &usage(1500, Some(1800))).unwrap();
        assert_eq!((first.tokens.counted, first.tokens.cache_read), (200, 800));
        assert_eq!(
            (second.tokens.counted, second.tokens.cache_read),
            (500, 1000)
        );
        // Another scope has its own running total.
        let other = tracker.settle("b", &usage(1000, Some(900))).unwrap();
        assert_eq!(other.tokens.cache_read, 900);
    }

    #[test]
    fn a_running_total_that_went_down_started_again() {
        let mut tracker = CacheTracker::new();
        tracker.settle("a", &usage(2000, Some(1500))).unwrap();
        let after_restart = tracker.settle("a", &usage(900, Some(400))).unwrap();
        assert_eq!(after_restart.tokens.cache_read, 400);
        assert_eq!(after_restart.tokens.counted, 500);
    }

    #[test]
    fn a_turns_cache_reads_never_exceed_its_tokens() {
        let mut tracker = CacheTracker::new();
        let turn = tracker.settle("a", &usage(100, Some(5000))).unwrap();
        assert_eq!(
            turn.tokens,
            Tokens {
                counted: 0,
                cache_read: 100
            }
        );
    }

    #[test]
    fn an_unreported_split_counts_the_whole_turn_and_says_so() {
        let mut tracker = CacheTracker::new();
        let turn = tracker.settle("a", &usage(1500, None)).unwrap();
        assert!(!turn.cache_known);
        assert_eq!(
            turn.tokens,
            Tokens {
                counted: 1500,
                cache_read: 0
            }
        );
        // Input plus output stands in for a missing total, as it always has.
        let rebuilt = tracker
            .settle("a", &json!({"input_tokens": 7, "output_tokens": 3}))
            .unwrap();
        assert_eq!(rebuilt.tokens.counted, 10);
        assert!(tracker.settle("a", &json!({})).is_none());
        assert!(tracker.settle("a", &Value::Null).is_none());
    }

    #[test]
    fn a_live_reading_without_cache_reads_counts_whole() {
        let known = Reading {
            total: 3170,
            cache_read: Some(1601),
        };
        assert_eq!(known.tokens().counted, 1569);
        let unknown = Reading {
            total: 3170,
            cache_read: None,
        };
        assert_eq!(
            unknown.tokens(),
            Tokens {
                counted: 3170,
                cache_read: 0
            }
        );
    }

    #[test]
    fn a_later_reading_wins_and_an_equal_one_only_adds_what_was_missing() {
        let bare = Reading {
            total: 10,
            cache_read: None,
        };
        let full = Reading {
            total: 10,
            cache_read: Some(4),
        };
        assert_eq!(Reading::latest(None, bare), bare);
        assert_eq!(Reading::latest(Some(bare), full), full);
        assert_eq!(Reading::latest(Some(full), bare), full);
        let lower = Reading {
            total: 9,
            cache_read: Some(9),
        };
        assert_eq!(Reading::latest(Some(full), lower), full);
    }

    #[test]
    fn beyond_never_shows_settled_cache_reads_twice() {
        let live = Reading {
            total: 3170,
            cache_read: Some(1601),
        }
        .tokens();
        // The settled turn did not report its cache reads, so all of it was counted.
        let settled = Tokens {
            counted: 3170,
            cache_read: 0,
        };
        assert_eq!(live.beyond(settled), Tokens::default());
        // Settled with a split: nothing is left over either.
        assert_eq!(live.beyond(live), Tokens::default());
        // Some live reading past the settled part keeps its own cache reads.
        let more = Reading {
            total: 4000,
            cache_read: Some(2000),
        }
        .tokens();
        assert_eq!(
            more.beyond(live),
            Tokens {
                counted: 431,
                cache_read: 399
            }
        );
    }

    #[test]
    fn the_session_usage_event_in_a_recording_splits_as_hermes_reported_it() {
        let text = include_str!("../tests/data/hermes/recorded/live_text_agent_on_fixture.jsonl");
        let mut readings = Vec::new();
        for line in text.lines() {
            let v: Value = serde_json::from_str(line).unwrap();
            let params = &v["frame"]["params"];
            if params["type"] == "session.usage" {
                let payload: SessionUsagePayload =
                    serde_json::from_value(params["payload"].clone()).unwrap();
                readings.push(Reading::from_payload(&payload).unwrap());
            }
        }
        assert_eq!(
            readings,
            [Reading {
                total: 3170,
                cache_read: Some(1601)
            }]
        );
        assert_eq!(readings[0].tokens().counted, 1569);
    }

    #[test]
    fn a_total_rebuilt_from_input_and_output_adds_the_cache_reads_back() {
        let payload = SessionUsagePayload {
            input_tokens: Some(1567),
            output_tokens: Some(2),
            cache_read_tokens: Some(1601),
            cache_write_tokens: Some(0),
            ..SessionUsagePayload::default()
        };
        let reading = Reading::from_payload(&payload).unwrap();
        assert_eq!(reading.total, 3170);
        assert_eq!(reading.cache_read, Some(1601));
        let status = json!({"input": 1567, "output": 2, "cache_read": 1601});
        assert_eq!(Reading::from_status_view(&status), Some(reading));
        let stats = json!({"total_tokens": 3170, "cache_read_tokens": 1601});
        assert_eq!(Reading::from_stats(&stats), Some(reading));
    }

    #[test]
    fn the_text_names_cache_reads_and_the_turns_where_they_are_unknown() {
        let digits = |n: u64| n.to_string();
        assert_eq!(tokens_text(10, 90, 0, digits), "10 tokens + 90 cache reads");
        assert_eq!(
            tokens_text(10, 0, 2, digits),
            "up to 10 tokens (cache reads unknown for 2 turns, counted in full)"
        );
        assert_eq!(
            tokens_text(10, 90, 1, digits),
            "up to 10 tokens + 90 known cache reads (cache reads unknown for 1 turn, counted in full)"
        );
    }
}
