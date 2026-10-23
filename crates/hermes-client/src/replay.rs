//! Per-session replay cursor, hold, and gap decisions.
//!
//! This module has no clocks and no sockets. The link drives it.

use std::marker::PhantomData;

/// The last `(replay_epoch, seq)` the consumer has accepted for one session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// Epoch the cursor's sequence numbers belong to.
    pub replay_epoch: String,
    /// Highest delivered sequence in that epoch.
    pub last_sequence: u64,
}

/// The fields a live frame contributes to the tracker. Absent fields pass through.
#[derive(Clone, Copy, Debug)]
pub struct Stamp<'a> {
    /// Session the frame belongs to, when the wire carried one.
    pub session_id: Option<&'a str>,
    /// Sequence in `replay_epoch`, when the wire carried one.
    pub seq: Option<u64>,
    /// Numbering epoch, when the wire carried one.
    pub replay_epoch: Option<&'a str>,
    /// `true` for a server `session.replay_gap` (no `seq` of its own).
    pub server_gap: bool,
}

/// Why one session cannot be proven complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GapReason {
    /// The ring said `snapshot_required` or `truncated`.
    SnapshotRequired,
    /// `replay_epoch` changed.
    EpochChanged,
    /// The next sequence skipped ahead.
    SeqJump,
    /// The gateway process epoch changed.
    BackendRestarted,
    /// The server sent `session.replay_gap`.
    ServerOverflow,
    /// Resume or replay failed, timed out, or named another session.
    ReplayFailed,
    /// `runtime.describe` did not offer `event-replay-v1`.
    ReplayUnsupported,
    /// The hold dropped frames before `resync`.
    HoldOverflow,
}

impl GapReason {
    /// Snake-case name used in `GAP reason=` lines.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SnapshotRequired => "snapshot_required",
            Self::EpochChanged => "epoch_changed",
            Self::SeqJump => "seq_jump",
            Self::BackendRestarted => "backend_restarted",
            Self::ServerOverflow => "server_overflow",
            Self::ReplayFailed => "replay_failed",
            Self::ReplayUnsupported => "replay_unsupported",
            Self::HoldOverflow => "hold_overflow",
        }
    }
}

/// One gap for one session. At most one is raised per incident until `resync`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayGap {
    /// Session the gap belongs to.
    pub session_id: String,
    /// Why catching up stopped.
    pub reason: GapReason,
    /// Newest sequence the tracker knew, when it had one.
    pub latest_seq: Option<u64>,
    /// Epoch the gap was raised under, when it had one.
    pub replay_epoch: Option<String>,
}

/// One tracker result. `replayed` is true only for events taken from a replay window.
#[derive(Debug, PartialEq, Eq)]
pub enum Out<T> {
    /// Deliver `item` to the consumer.
    Deliver {
        /// The frame or test item.
        item: T,
        /// True when the item came from `session.events.since`.
        replayed: bool,
    },
    /// One gap. The consumer resumes and then calls `resync`.
    Gap(ReplayGap),
    /// The gateway reported `not_found`. The session is forgotten.
    Gone(String),
}

/// A `session.events.since` result, already decoded, or a demand for a snapshot.
#[derive(Debug, PartialEq, Eq)]
pub enum ReplayWindow<T> {
    /// Events in ascending `seq`. The string is each event's `replay_epoch`.
    Events(Vec<(u64, String, T)>),
    /// `snapshot_required` or `truncated`. Nothing in the window is delivered.
    SnapshotRequired {
        /// Newest sequence the result reported.
        latest_seq: u64,
    },
}

/// Why a replay call did not produce a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayFailure {
    /// `RpcError.reason` was `not_found`.
    NotFound,
    /// Any other error, timeout, or decode failure.
    Other,
}

/// Identifies one in-flight replay. Replies carrying an older token are ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayToken(pub u64);

/// Per-session cursor, hold, and passed-through memory.
pub struct ReplayTracker<T> {
    hold_limit: usize,
    _item: PhantomData<fn() -> T>,
}

impl<T> ReplayTracker<T> {
    /// Remember `hold_limit` items per session before the oldest is dropped.
    pub fn new(hold_limit: usize) -> Self {
        Self {
            hold_limit,
            _item: PhantomData,
        }
    }

    /// Start tracking `session_id` at `cursor`.
    ///
    /// A higher passed-through sequence of the same epoch replaces `cursor`.
    pub fn track(&mut self, session_id: &str, cursor: Cursor) {
        let _ = (self.hold_limit, session_id, cursor);
        todo!("P1-03")
    }

    /// Forget `session_id`. Sends nothing.
    pub fn untrack(&mut self, session_id: &str) {
        let _ = (self, session_id);
        todo!("P1-03")
    }

    /// Apply one live frame.
    pub fn on_live(&mut self, stamp: Stamp<'_>, item: T) -> Vec<Out<T>> {
        let _ = (self, stamp, item);
        todo!("P1-03")
    }

    /// Record the process epoch. The first call records only.
    ///
    /// A later epoch gaps every Tracking session with `BackendRestarted` and
    /// re-raises the existing gap of every AwaitingSnapshot session.
    pub fn on_new_connection(&mut self, process_epoch: &str) -> Vec<Out<T>> {
        let _ = (self, process_epoch);
        todo!("P1-03")
    }

    /// Gap every Tracking session with `ReplayUnsupported`.
    pub fn on_replay_unsupported(&mut self) -> Vec<Out<T>> {
        let _ = self;
        todo!("P1-03")
    }

    /// Tracking sessions to re-attach, and move them to Replaying.
    pub fn begin_replay(&mut self) -> Vec<(String, Cursor, ReplayToken)> {
        let _ = self;
        todo!("P1-03")
    }

    /// Apply a resume snapshot's epoch. A mismatch is one `EpochChanged` gap.
    pub fn on_reattached(
        &mut self,
        session_id: &str,
        token: ReplayToken,
        snapshot_epoch: &str,
    ) -> Vec<Out<T>> {
        let _ = (self, session_id, token, snapshot_epoch);
        todo!("P1-03")
    }

    /// Apply a replay window, then flush the hold through the same gate.
    pub fn on_replay_window(
        &mut self,
        session_id: &str,
        token: ReplayToken,
        window: ReplayWindow<T>,
    ) -> Vec<Out<T>> {
        let _ = (self, session_id, token, window);
        todo!("P1-03")
    }

    /// `NotFound` forgets the session. `Other` is one `ReplayFailed` gap.
    pub fn on_replay_failed(
        &mut self,
        session_id: &str,
        token: ReplayToken,
        failure: ReplayFailure,
    ) -> Vec<Out<T>> {
        let _ = (self, session_id, token, failure);
        todo!("P1-03")
    }

    /// A dropped socket. Replaying sessions return to Tracking with an empty hold.
    pub fn on_connection_lost(&mut self) {
        let _ = self;
        todo!("P1-03")
    }

    /// Install `cursor` and flush held frames of that epoch above it.
    pub fn resync(&mut self, session_id: &str, cursor: Cursor) -> Vec<Out<T>> {
        let _ = (self, session_id, cursor);
        todo!("P1-03")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(epoch: &str, seq: u64) -> Cursor {
        Cursor {
            replay_epoch: epoch.to_owned(),
            last_sequence: seq,
        }
    }

    fn live<'a>(session: Option<&'a str>, seq: Option<u64>, epoch: Option<&'a str>) -> Stamp<'a> {
        Stamp {
            session_id: session,
            seq,
            replay_epoch: epoch,
            server_gap: false,
        }
    }

    fn server_gap<'a>(session: &'a str, epoch: Option<&'a str>) -> Stamp<'a> {
        Stamp {
            session_id: Some(session),
            seq: None,
            replay_epoch: epoch,
            server_gap: true,
        }
    }

    fn delivered(outs: &[Out<u64>]) -> Vec<(u64, bool)> {
        outs.iter()
            .filter_map(|out| match out {
                Out::Deliver { item, replayed } => Some((*item, *replayed)),
                _ => None,
            })
            .collect()
    }

    fn gaps(outs: &[Out<u64>]) -> Vec<&ReplayGap> {
        outs.iter()
            .filter_map(|out| match out {
                Out::Gap(gap) => Some(gap),
                _ => None,
            })
            .collect()
    }

    fn start_replay(tracker: &mut ReplayTracker<u64>, epoch: &str, seq: u64) -> ReplayToken {
        tracker.track("s", cursor(epoch, seq));
        let _ = tracker.on_new_connection("p");
        let _ = tracker.on_new_connection("p");
        let plan = tracker.begin_replay();
        assert_eq!(plan.len(), 1, "one tracking session");
        plan[0].2
    }

    #[test]
    fn live_frames_advance_and_duplicates_drop() {
        let mut tracker = ReplayTracker::new(10_000);
        tracker.track("s", cursor("e", 4));
        let mut got = Vec::new();
        for seq in [5, 6, 6, 4, 7] {
            got.extend(delivered(
                &tracker.on_live(live(Some("s"), Some(seq), Some("e")), seq),
            ));
        }
        assert_eq!(got, vec![(5, false), (6, false), (7, false)]);
        let jump = tracker.on_live(live(Some("s"), Some(9), Some("e")), 9);
        assert!(delivered(&jump).is_empty());
        let found = gaps(&jump);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reason, GapReason::SeqJump);
        assert_eq!(found[0].session_id, "s");
    }

    #[test]
    fn sessionless_and_untracked_pass_through() {
        let mut tracker = ReplayTracker::new(10_000);
        tracker.track("s", cursor("e", 4));
        let frames = [
            tracker.on_live(live(None, Some(1), Some("e")), 1),
            tracker.on_live(live(Some("u"), Some(3), Some("e")), 3),
            tracker.on_live(server_gap("u", Some("e")), 0),
            tracker.on_live(live(Some("s"), Some(5), None), 5),
        ];
        for (out, item) in frames.iter().zip([1, 3, 0, 5]) {
            assert_eq!(delivered(out), vec![(item, false)]);
            assert!(gaps(out).is_empty());
        }
    }

    #[test]
    fn track_after_passed_through_frames_starts_after_them() {
        let mut tracker = ReplayTracker::new(10_000);
        for seq in 1..=5 {
            let out = tracker.on_live(live(Some("s"), Some(seq), Some("e")), seq);
            assert_eq!(delivered(&out), vec![(seq, false)]);
        }
        tracker.track("s", cursor("e", 3));
        let _ = tracker.on_new_connection("p");
        let plan = tracker.begin_replay();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].0, "s");
        assert_eq!(plan[0].1, cursor("e", 5));
    }

    #[test]
    fn replay_window_then_hold_in_order_once() {
        let mut tracker = ReplayTracker::new(10_000);
        let token = start_replay(&mut tracker, "e", 5);
        assert!(delivered(&tracker.on_live(live(Some("s"), Some(9), Some("e")), 9)).is_empty());
        assert!(delivered(&tracker.on_live(live(Some("s"), Some(10), Some("e")), 10)).is_empty());
        let window = ReplayWindow::Events(vec![
            (6, "e".to_owned(), 6),
            (7, "e".to_owned(), 7),
            (8, "e".to_owned(), 8),
            (9, "e".to_owned(), 9),
            (10, "e".to_owned(), 10),
        ]);
        let out = tracker.on_replay_window("s", token, window);
        assert_eq!(
            delivered(&out),
            vec![(6, true), (7, true), (8, true), (9, true), (10, true)]
        );
        assert!(gaps(&out).is_empty());
    }

    #[test]
    fn replay_never_regresses_cursor() {
        let mut tracker = ReplayTracker::new(10_000);
        let token = start_replay(&mut tracker, "e", 10);
        let window = ReplayWindow::Events(vec![
            (8, "e".to_owned(), 8),
            (9, "e".to_owned(), 9),
            (10, "e".to_owned(), 10),
        ]);
        let out = tracker.on_replay_window("s", token, window);
        assert!(delivered(&out).is_empty());
        assert!(gaps(&out).is_empty());
        let next = tracker.on_live(live(Some("s"), Some(11), Some("e")), 11);
        assert_eq!(delivered(&next), vec![(11, false)]);
    }

    #[test]
    fn snapshot_required_delivers_nothing_and_one_gap() {
        let mut tracker = ReplayTracker::new(10_000);
        let token = start_replay(&mut tracker, "e", 4);
        let out = tracker.on_replay_window(
            "s",
            token,
            ReplayWindow::SnapshotRequired { latest_seq: 40 },
        );
        assert!(delivered(&out).is_empty());
        let found = gaps(&out);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reason, GapReason::SnapshotRequired);
    }

    #[test]
    fn new_epoch_on_live_frame_is_a_gap() {
        let mut tracker = ReplayTracker::new(10_000);
        tracker.track("s", cursor("e", 4));
        let out = tracker.on_live(live(Some("s"), Some(1), Some("f")), 1);
        assert!(delivered(&out).is_empty());
        let found = gaps(&out);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reason, GapReason::EpochChanged);
    }

    #[test]
    fn held_new_epoch_frame_at_flush_is_one_gap() {
        let mut tracker = ReplayTracker::new(10_000);
        let token = start_replay(&mut tracker, "e", 5);
        assert!(delivered(&tracker.on_live(live(Some("s"), Some(1), Some("f")), 1)).is_empty());
        let window = ReplayWindow::Events(vec![(6, "e".to_owned(), 6)]);
        let out = tracker.on_replay_window("s", token, window);
        assert_eq!(delivered(&out), vec![(6, true)]);
        let found = gaps(&out);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reason, GapReason::EpochChanged);
        let again = tracker.resync("s", cursor("f", 0));
        assert_eq!(delivered(&again), vec![(1, false)]);
    }

    #[test]
    fn server_replay_gap_is_one_gap() {
        let mut tracker = ReplayTracker::new(10_000);
        tracker.track("s", cursor("e", 4));
        let out = tracker.on_live(server_gap("s", Some("e")), 0);
        assert!(delivered(&out).is_empty());
        let found = gaps(&out);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reason, GapReason::ServerOverflow);
    }

    #[test]
    fn second_trigger_is_absorbed_until_resync() {
        let mut tracker = ReplayTracker::new(10_000);
        tracker.track("s", cursor("e", 4));
        let first = tracker.on_live(server_gap("s", Some("e")), 0);
        assert_eq!(gaps(&first).len(), 1);
        let second = tracker.on_live(live(Some("s"), Some(9), Some("e")), 9);
        assert!(
            second.is_empty(),
            "a second trigger inside the incident is absorbed"
        );
    }

    #[test]
    fn resync_flushes_only_newer_frames_of_the_new_epoch() {
        let mut tracker = ReplayTracker::new(10_000);
        let token = start_replay(&mut tracker, "e", 4);
        let _ = tracker.on_live(live(Some("s"), Some(2), Some("old")), 2);
        let _ = tracker.on_live(live(Some("s"), Some(5), Some("e")), 5);
        let _ = tracker.on_live(live(Some("s"), Some(3), Some("n")), 3);
        let _ = tracker.on_live(live(Some("s"), Some(6), Some("n")), 6);
        let _ = token;
        let out = tracker.resync("s", cursor("n", 5));
        assert_eq!(delivered(&out), vec![(6, false)]);
        assert!(gaps(&out).is_empty());
    }

    #[test]
    fn connection_lost_during_replay_discards_hold_keeps_cursor() {
        let mut tracker = ReplayTracker::new(10_000);
        let token = start_replay(&mut tracker, "e", 4);
        assert!(delivered(&tracker.on_live(live(Some("s"), Some(9), Some("e")), 9)).is_empty());
        tracker.on_connection_lost();
        let plan = tracker.begin_replay();
        assert_eq!(plan[0].1, cursor("e", 4));
        let late = tracker.on_replay_window(
            "s",
            token,
            ReplayWindow::Events(vec![(5, "e".to_owned(), 5), (9, "e".to_owned(), 9)]),
        );
        assert!(late.is_empty(), "the retired token's window is ignored");
        let out = tracker.on_replay_window(
            "s",
            plan[0].2,
            ReplayWindow::Events(vec![(5, "e".to_owned(), 5)]),
        );
        assert_eq!(delivered(&out), vec![(5, true)]);
    }

    #[test]
    fn late_replay_reply_is_ignored() {
        let mut tracker = ReplayTracker::new(10_000);
        let token = start_replay(&mut tracker, "e", 4);
        tracker.on_connection_lost();
        let stale = tracker.on_replay_window(
            "s",
            token,
            ReplayWindow::Events(vec![(5, "e".to_owned(), 5)]),
        );
        assert!(stale.is_empty());
        let fresh = tracker.begin_replay();
        let _ = tracker.on_replay_window(
            "s",
            fresh[0].2,
            ReplayWindow::SnapshotRequired { latest_seq: 9 },
        );
        let ignored = tracker.on_replay_window(
            "s",
            fresh[0].2,
            ReplayWindow::Events(vec![(5, "e".to_owned(), 5)]),
        );
        assert!(
            ignored.is_empty(),
            "a session awaiting a snapshot ignores replay replies"
        );
    }

    #[test]
    fn not_found_forgets_the_session() {
        let mut tracker = ReplayTracker::new(10_000);
        let token = start_replay(&mut tracker, "e", 4);
        let out = tracker.on_replay_failed("s", token, ReplayFailure::NotFound);
        assert!(matches!(&out[..], [Out::Gone(id)] if id == "s"));
        let passed = tracker.on_live(live(Some("s"), Some(5), Some("e")), 5);
        assert_eq!(delivered(&passed), vec![(5, false)]);
    }

    #[test]
    fn other_failure_is_a_gap() {
        let mut tracker = ReplayTracker::new(10_000);
        let token = start_replay(&mut tracker, "e", 4);
        let out = tracker.on_replay_failed("s", token, ReplayFailure::Other);
        assert!(delivered(&out).is_empty());
        let found = gaps(&out);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reason, GapReason::ReplayFailed);
    }

    #[test]
    fn new_connection_gaps_and_re_raises() {
        let mut tracker = ReplayTracker::new(10_000);
        tracker.track("a", cursor("e", 1));
        tracker.track("b", cursor("e", 1));
        assert!(
            tracker.on_new_connection("p1").is_empty(),
            "the first epoch is recorded only"
        );
        let _ = tracker.on_live(server_gap("b", Some("e")), 0);
        let raised = tracker.on_new_connection("p2");
        let reasons: Vec<_> = gaps(&raised)
            .iter()
            .map(|gap| (gap.session_id.as_str(), gap.reason))
            .collect();
        assert!(reasons.contains(&("a", GapReason::BackendRestarted)));
        assert!(reasons.contains(&("b", GapReason::ServerOverflow)));
        let plan = tracker.begin_replay();
        assert!(
            plan.iter().all(|(id, _, _)| id != "b"),
            "an awaiting session is not re-attached"
        );
    }

    #[test]
    fn hold_overflow_becomes_a_gap() {
        let mut replaying = ReplayTracker::new(3);
        replaying.track("s", cursor("e", 4));
        let _ = replaying.on_new_connection("p");
        let _ = replaying.on_new_connection("p");
        let _ = replaying.begin_replay();
        let mut saw = None;
        for seq in [5, 6, 7, 8] {
            let out = replaying.on_live(live(Some("s"), Some(seq), Some("e")), seq);
            if !gaps(&out).is_empty() {
                saw = Some((seq, gaps(&out)[0].reason));
            } else {
                assert!(delivered(&out).is_empty());
            }
        }
        assert_eq!(saw, Some((8, GapReason::HoldOverflow)));

        let mut awaiting = ReplayTracker::new(3);
        awaiting.track("s", cursor("e", 4));
        let first = awaiting.on_live(server_gap("s", Some("e")), 0);
        assert_eq!(gaps(&first)[0].reason, GapReason::ServerOverflow);
        for seq in [5, 6, 7, 8] {
            let out = awaiting.on_live(live(Some("s"), Some(seq), Some("e")), seq);
            assert!(out.is_empty(), "awaiting absorbs seq {seq}");
        }
        let low = awaiting.resync("s", cursor("e", 4));
        assert!(delivered(&low).is_empty());
        assert_eq!(gaps(&low)[0].reason, GapReason::HoldOverflow);
        let high = awaiting.resync("s", cursor("e", 5));
        assert_eq!(delivered(&high), vec![(6, false), (7, false), (8, false)]);
        assert!(gaps(&high).is_empty());
    }

    proptest::proptest! {
        #[test]
        fn exactly_once_in_order_property(
            count in 1usize..=3,
            n0 in 1u64..=60,
            n1 in 1u64..=60,
            n2 in 1u64..=60,
            raw_cuts in proptest::collection::vec(1u64..=60, 0..=3),
            force_snapshot in proptest::collection::vec(proptest::prelude::any::<bool>(), 3),
        ) {
            let lengths = [n0, n1, n2];
            let sessions: Vec<(String, u64)> = (0..count)
                .map(|index| (format!("s{index}"), lengths[index]))
                .collect();
            let max_n = sessions.iter().map(|(_, n)| *n).max().unwrap_or(1);
            let mut cuts: Vec<u64> = raw_cuts.into_iter().filter(|cut| *cut < max_n).collect();
            cuts.sort_unstable();
            cuts.dedup();
            drive(&sessions, &cuts, &force_snapshot);
        }
    }

    struct Log {
        delivered: Vec<u64>,
        gap_at: Vec<usize>,
        timeline: Vec<Observation>,
    }

    enum Observation {
        Delivered(u64),
        Gap,
        Resync(u64),
    }

    fn drive(sessions: &[(String, u64)], cuts: &[u64], force_snapshot: &[bool]) {
        let mut tracker = ReplayTracker::<u64>::new(10_000);
        assert!(tracker.on_new_connection("process").is_empty());
        for (id, _) in sessions {
            tracker.track(
                id,
                Cursor {
                    replay_epoch: "e".to_owned(),
                    last_sequence: 0,
                },
            );
        }
        let mut logs: Vec<Log> = sessions
            .iter()
            .map(|_| Log {
                delivered: Vec::new(),
                gap_at: Vec::new(),
                timeline: Vec::new(),
            })
            .collect();
        let mut published = vec![0u64; sessions.len()];
        let mut boundaries = cuts.to_vec();
        boundaries.push(sessions.iter().map(|(_, n)| *n).max().unwrap_or(1));
        let mut from = 0u64;
        for (index, end) in boundaries.into_iter().enumerate() {
            if index == 0 {
                publish_live(&mut tracker, sessions, &mut published, &mut logs, from, end);
            } else {
                reconnect(
                    &mut tracker,
                    sessions,
                    &mut published,
                    &mut logs,
                    from,
                    end,
                    force_snapshot.get(index - 1).copied().unwrap_or(false),
                );
            }
            from = end;
        }
        for (log, (_, n)) in logs.iter().zip(sessions) {
            let mut prior = 0u64;
            for seq in &log.delivered {
                assert!(*seq > prior, "delivered sequence went backwards");
                prior = *seq;
            }
            if log.gap_at.is_empty() {
                assert_eq!(log.delivered, (1..=*n).collect::<Vec<_>>());
            }
            let prefix_end = log.gap_at.first().copied().unwrap_or(log.delivered.len());
            let prefix = &log.delivered[..prefix_end.min(log.delivered.len())];
            assert_eq!(prefix, &(1..=prefix.len() as u64).collect::<Vec<_>>());
            let mut awaiting_snapshot = false;
            let mut expect = 1;
            for observation in &log.timeline {
                match observation {
                    Observation::Delivered(seq) => {
                        assert!(
                            !awaiting_snapshot,
                            "delivered inside an unresolved incident"
                        );
                        assert_eq!(*seq, expect, "delivery after resync must be contiguous");
                        expect += 1;
                    }
                    Observation::Gap => {
                        assert!(!awaiting_snapshot, "duplicate gap inside one incident");
                        awaiting_snapshot = true;
                    }
                    Observation::Resync(cursor) => {
                        awaiting_snapshot = false;
                        expect = cursor + 1;
                    }
                }
            }
        }
    }

    fn note(logs: &mut [Log], session_index: usize, outs: Vec<Out<u64>>) {
        for out in outs {
            match out {
                Out::Deliver { item, .. } => {
                    logs[session_index].delivered.push(item);
                    logs[session_index]
                        .timeline
                        .push(Observation::Delivered(item));
                }
                Out::Gap(_) => {
                    logs[session_index]
                        .gap_at
                        .push(logs[session_index].delivered.len());
                    logs[session_index].timeline.push(Observation::Gap);
                }
                Out::Gone(_) => {}
            }
        }
    }

    fn publish_live(
        tracker: &mut ReplayTracker<u64>,
        sessions: &[(String, u64)],
        published: &mut [u64],
        logs: &mut [Log],
        from: u64,
        to: u64,
    ) {
        for seq in (from + 1)..=to {
            for (index, (id, n)) in sessions.iter().enumerate() {
                if seq > *n || published[index] >= seq {
                    continue;
                }
                published[index] = seq;
                let outs = tracker.on_live(
                    Stamp {
                        session_id: Some(id),
                        seq: Some(seq),
                        replay_epoch: Some("e"),
                        server_gap: false,
                    },
                    seq,
                );
                note(logs, index, outs);
            }
        }
    }

    fn reconnect(
        tracker: &mut ReplayTracker<u64>,
        sessions: &[(String, u64)],
        published: &mut [u64],
        logs: &mut [Log],
        from: u64,
        to: u64,
        force_snapshot: bool,
    ) {
        tracker.on_connection_lost();
        for seq in (from + 1)..=to {
            for (index, (_, n)) in sessions.iter().enumerate() {
                if seq <= *n {
                    published[index] = seq;
                }
            }
        }
        let _ = tracker.on_new_connection("process");
        let plan = tracker.begin_replay();
        for (id, cursor, token) in plan {
            let Some(index) = sessions.iter().position(|(name, _)| name == &id) else {
                continue;
            };
            let missed: Vec<u64> = ((cursor.last_sequence + 1)..=published[index]).collect();
            if let Some(&first) = missed.first() {
                let _ = tracker.on_live(
                    Stamp {
                        session_id: Some(&id),
                        seq: Some(first),
                        replay_epoch: Some("e"),
                        server_gap: false,
                    },
                    first,
                );
            }
            let window = if force_snapshot && cursor.last_sequence < published[index] {
                ReplayWindow::SnapshotRequired {
                    latest_seq: published[index],
                }
            } else {
                ReplayWindow::Events(
                    missed
                        .into_iter()
                        .map(|seq| (seq, "e".to_owned(), seq))
                        .collect(),
                )
            };
            let outs = tracker.on_replay_window(&id, token, window);
            let gapped = outs.iter().any(|out| matches!(out, Out::Gap(_)));
            note(logs, index, outs);
            if gapped {
                let tip = published[index];
                logs[index].timeline.push(Observation::Resync(tip));
                let flushed = tracker.resync(
                    &id,
                    Cursor {
                        replay_epoch: "e".to_owned(),
                        last_sequence: tip,
                    },
                );
                note(logs, index, flushed);
            }
        }
    }
}
