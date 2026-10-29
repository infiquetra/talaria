//! Which fleet read goes next: per-profile flights, the shared slot cap, and the pending set.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use talaria_core::fleet::RowKey;

/// One read the fleet link can make.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Work {
    /// `session.list` for a profile.
    Roster(String),
    /// `session.active_list` for a profile.
    Liveness(String),
    /// `approval.pending` for a session.
    Approvals(RowKey),
    /// The first outcome probe step: the epoch and latest sequence.
    ProbeHead { key: RowKey, probe_id: u64 },
    /// The second step: the last events of that epoch.
    ProbeTail {
        key: RowKey,
        probe_id: u64,
        epoch: String,
        from: u64,
    },
}

impl Work {
    pub(crate) fn profile(&self) -> &str {
        match self {
            Work::Roster(profile) | Work::Liveness(profile) => profile,
            Work::Approvals(key) | Work::ProbeHead { key, .. } | Work::ProbeTail { key, .. } => {
                &key.profile
            }
        }
    }
}

/// Probe steps and approval reads.
pub(crate) const CLASS_PROMPT: u8 = 1;
/// Liveness of a hot profile.
pub(crate) const CLASS_HOT: u8 = 2;
/// Rounds after a connect, `sessions.changed`, the backstop or a refresh.
pub(crate) const CLASS_ROUND: u8 = 3;
/// Liveness of a cold profile.
pub(crate) const CLASS_COLD: u8 = 4;

struct Waiting {
    work: Work,
    class: u8,
    since: Instant,
}

/// Flights, the pending set, and the per-profile memory the timers read.
pub(crate) struct Schedule {
    max_in_flight: usize,
    suspect_cap: usize,
    flying: HashMap<Work, bool>,
    again: HashMap<Work, u8>,
    pending: Vec<Waiting>,
    in_flight: usize,
    suspect_in_flight: usize,
    suspect: HashMap<String, bool>,
    started: HashMap<Work, Instant>,
}

impl Schedule {
    pub(crate) fn new(max_in_flight: usize, suspect_cap: usize) -> Self {
        Self {
            max_in_flight: max_in_flight.max(1),
            suspect_cap: suspect_cap.max(1),
            flying: HashMap::new(),
            again: HashMap::new(),
            pending: Vec::new(),
            in_flight: 0,
            suspect_in_flight: 0,
            suspect: HashMap::new(),
            started: HashMap::new(),
        }
    }

    /// Ask for `work`. A flight of the same work sets "again"; a pending one joins.
    pub(crate) fn trigger(&mut self, work: Work, class: u8, now: Instant) {
        if self.flying.contains_key(&work) {
            let entry = self.again.entry(work).or_insert(class);
            *entry = (*entry).min(class);
            return;
        }
        if let Some(waiting) = self.pending.iter_mut().find(|waiting| waiting.work == work) {
            waiting.class = waiting.class.min(class);
            return;
        }
        self.pending.push(Waiting {
            work,
            class,
            since: now,
        });
    }

    /// When the latest flight of `work` started.
    pub(crate) fn started(&self, work: &Work) -> Option<Instant> {
        self.started.get(work).copied()
    }

    /// True while `work` is in flight or pending.
    #[cfg(test)]
    pub(crate) fn busy(&self, work: &Work) -> bool {
        self.flying.contains_key(work) || self.pending.iter().any(|waiting| waiting.work == *work)
    }

    /// The next work a free slot may take, marked in flight.
    pub(crate) fn next(&mut self, now: Instant) -> Option<(Work, bool)> {
        if self.in_flight >= self.max_in_flight {
            return None;
        }
        let suspect_full = self.suspect_in_flight >= self.suspect_cap;
        let index = self
            .pending
            .iter()
            .enumerate()
            .filter(|(_, waiting)| !(suspect_full && self.is_suspect(waiting.work.profile())))
            .min_by(|(_, left), (_, right)| {
                left.class
                    .cmp(&right.class)
                    .then_with(|| left.since.cmp(&right.since))
            })
            .map(|(index, _)| index)?;
        let waiting = self.pending.remove(index);
        let suspect = self.is_suspect(waiting.work.profile());
        self.in_flight += 1;
        if suspect {
            self.suspect_in_flight += 1;
        }
        self.started.insert(waiting.work.clone(), now);
        self.flying.insert(waiting.work.clone(), suspect);
        Some((waiting.work, suspect))
    }

    /// A flight ended; frees its slot and re-issues an "again".
    pub(crate) fn finish(&mut self, work: &Work, now: Instant) {
        if let Some(suspect) = self.flying.remove(work) {
            self.in_flight = self.in_flight.saturating_sub(1);
            if suspect {
                self.suspect_in_flight = self.suspect_in_flight.saturating_sub(1);
            }
        }
        if let Some(class) = self.again.remove(work) {
            self.trigger(work.clone(), class, now);
        }
    }

    /// Drop pending work that no longer applies, such as probes after a reconnect.
    pub(crate) fn forget(&mut self, keep: impl Fn(&Work) -> bool) {
        self.pending.retain(|waiting| keep(&waiting.work));
        self.again.retain(|work, _| keep(work));
    }

    pub(crate) fn set_suspect(&mut self, profile: &str, suspect: bool) {
        self.suspect.insert(profile.to_owned(), suspect);
    }

    fn is_suspect(&self, profile: &str) -> bool {
        self.suspect.get(profile).copied().unwrap_or(false)
    }

    /// True when `work` started less than `gap` ago.
    pub(crate) fn within(&self, work: &Work, gap: Duration, now: Instant) -> bool {
        self.started(work)
            .is_some_and(|started| now.saturating_duration_since(started) < gap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(session: &str) -> RowKey {
        RowKey {
            profile: "fixture".into(),
            session_id: session.into(),
        }
    }

    #[test]
    fn classes_order_and_joins() {
        let now = Instant::now();
        let mut schedule = Schedule::new(8, 2);
        schedule.trigger(Work::Liveness("cold".into()), CLASS_COLD, now);
        schedule.trigger(Work::Roster("fixture".into()), CLASS_ROUND, now);
        schedule.trigger(Work::Approvals(key("s1")), CLASS_PROMPT, now);
        schedule.trigger(Work::Roster("fixture".into()), CLASS_HOT, now);
        let order: Vec<Work> =
            std::iter::from_fn(|| schedule.next(now).map(|(work, _)| work)).collect();
        assert_eq!(
            order,
            vec![
                Work::Approvals(key("s1")),
                Work::Roster("fixture".into()),
                Work::Liveness("cold".into()),
            ]
        );
    }

    #[test]
    fn flights_set_again_and_free_slots() {
        let now = Instant::now();
        let mut schedule = Schedule::new(1, 1);
        schedule.trigger(Work::Liveness("a".into()), CLASS_HOT, now);
        schedule.trigger(Work::Liveness("b".into()), CLASS_HOT, now);
        assert_eq!(
            schedule.next(now).map(|(work, _)| work),
            Some(Work::Liveness("a".into()))
        );
        assert!(schedule.next(now).is_none(), "one slot");
        schedule.trigger(Work::Liveness("a".into()), CLASS_COLD, now);
        schedule.finish(&Work::Liveness("a".into()), now);
        assert!(
            schedule.busy(&Work::Liveness("a".into())),
            "again re-queued"
        );
        assert_eq!(
            schedule.next(now).map(|(work, _)| work),
            Some(Work::Liveness("b".into()))
        );
        assert!(schedule.within(&Work::Liveness("b".into()), Duration::from_secs(1), now));
    }

    #[test]
    fn suspect_profiles_share_a_small_cap() {
        let now = Instant::now();
        let mut schedule = Schedule::new(8, 2);
        for profile in ["s1", "s2", "s3"] {
            schedule.set_suspect(profile, true);
            schedule.trigger(Work::Liveness(profile.into()), CLASS_COLD, now);
        }
        schedule.trigger(
            Work::Liveness("ok".into()),
            CLASS_COLD,
            now + Duration::from_secs(1),
        );
        let mut taken = Vec::new();
        while let Some((work, _)) = schedule.next(now) {
            taken.push(work);
        }
        assert_eq!(taken.len(), 3, "two suspect flights and the healthy one");
        assert!(taken.contains(&Work::Liveness("ok".into())));
        schedule.forget(|work| work.profile() != "s3");
        assert!(!schedule.busy(&Work::Liveness("s3".into())));
    }
}
