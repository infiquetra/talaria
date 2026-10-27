use super::*;

impl MemoryState {
    pub(super) fn check_run(&mut self, _now: Instant) -> Vec<MemoryEffect> {
        let Some(curator) = self.curator() else {
            return Vec::new();
        };
        if !curator.enabled {
            self.off();
            return Vec::new();
        }
        if self.follow.is_some() {
            self.banner = Some("A curator run is already being followed.".into());
            return Vec::new();
        }
        if self.check.is_some() {
            self.banner = Some(WAITING.into());
            return Vec::new();
        }
        self.next_follow = self.next_follow.wrapping_add(1);
        self.check = Some(RunCheck::BeforeStart(self.next_follow));
        vec![MemoryEffect::PollAction {
            follow: self.next_follow,
            after: Duration::ZERO,
        }]
    }

    pub(super) fn action_polled(
        &mut self,
        id: u64,
        result: Result<ActionView, FailText>,
        now: Instant,
    ) -> Vec<MemoryEffect> {
        if self.check.as_ref().is_some_and(|check| match check {
            RunCheck::BeforeStart(expected) | RunCheck::UnknownStart(expected) => *expected == id,
        }) {
            let unknown = matches!(self.check.take(), Some(RunCheck::UnknownStart(_)));
            match result {
                Ok(action) if action.running && (!unknown || action.pid.is_some()) => {
                    if !self.open {
                        return self.report("A curator run is running in Hermes; Talaria is not following it.".into());
                    }
                    self.follow = Some(Follow { id, pid: action.pid, started: now, polls: 0, failures: 0, lines: tail_after_header(&action.lines, 8) });
                    self.banner = Some("A curator run is already going on this gateway; following it.".into());
                    return vec![MemoryEffect::PollAction { follow: id, after: Duration::from_millis(1200) }];
                }
                _ if unknown => self.banner = Some(format!("Could not confirm that the curator run started. Check with hermes -p {} curator status.", self.profile)),
                Ok(_) => self.confirm(vec![format!("Run the skill curator for profile {} now?", self.profile), format!("It marks unused skills stale and archives old ones; hermes -p {} curator restore <skill> brings one back. It runs even while paused. With curator.consolidate on, it also merges and rewrites skills using the auxiliary model.", self.profile)], "Run", ConfirmAction::Run),
                Err(reason) => self.banner = Some(format!("Could not check for a curator run: {}.", reason.0)),
            }
            if unknown
                && !self.open
                && let Some(message) = self.banner.clone()
            {
                return self.report(message);
            }
            return Vec::new();
        }
        let Some(mut follow) = self.follow.take() else {
            return Vec::new();
        };
        if follow.id != id {
            self.follow = Some(follow);
            return Vec::new();
        }
        follow.polls += 1;
        let mut terminal = None;
        let mut refresh = false;
        match result {
            Err(reason) => {
                follow.failures += 1;
                if follow.failures >= 3 {
                    terminal = Some(format!("Lost track of the curator run: {}.", reason.0));
                }
            }
            Ok(action) => {
                follow.failures = 0;
                follow.lines = tail_after_header(&action.lines, 8);
                if action.pid.is_some() && follow.pid.is_some() && action.pid != follow.pid {
                    terminal = Some("Another curator run started on this gateway; Talaria stopped following this one.".into());
                } else if !action.running {
                    refresh = true;
                    terminal = Some(match action.exit_code {
                        Some(0) => "Curator run finished.".into(),
                        Some(exit) => format!("Curator run failed (exit {exit})."),
                        None => format!(
                            "Hermes no longer reports this curator run (the gateway may have restarted), so Talaria cannot tell how it ended. Check with hermes -p {} curator status.",
                            self.profile
                        ),
                    });
                }
            }
        }
        if terminal.is_none() && follow.polls >= 240 {
            terminal = Some(format!(
                "Still running after about 5 minutes; Talaria stopped following it. Check with hermes -p {} curator status.",
                self.profile
            ));
        }
        if let Some(message) = terminal {
            self.banner = Some(message.clone());
            let mut effects = vec![MemoryEffect::Notice(message)];
            if refresh {
                effects.push(MemoryEffect::LoadCurator);
            }
            effects
        } else {
            self.follow = Some(follow);
            vec![MemoryEffect::PollAction {
                follow: id,
                after: Duration::from_millis(1200),
            }]
        }
    }
}
