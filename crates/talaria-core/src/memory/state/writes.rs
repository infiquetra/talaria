use super::*;

impl MemoryState {
    pub(super) fn write_reply(
        &mut self,
        outcome: WriteOutcome,
        route: &str,
        now: Instant,
    ) -> Vec<MemoryEffect> {
        if self.write.is_none() || self.verifying {
            return Vec::new();
        }
        let read_back = !matches!(
            &outcome,
            WriteOutcome::NotSent(_)
                | WriteOutcome::Refused {
                    status: 401 | 403 | 404,
                    ..
                }
        );
        let unknown = outcome == WriteOutcome::Unknown;
        let error = match &outcome {
            WriteOutcome::Ok(_) => None,
            WriteOutcome::Unknown => Some(UNKNOWN.into()),
            WriteOutcome::Failed(reason) | WriteOutcome::NotSent(reason) => Some(reason.0.clone()),
            WriteOutcome::Refused { status, detail } => {
                Some(self.refusal(*status, detail.as_deref(), route))
            }
        };
        self.banner = error.clone();
        if !read_back {
            self.write = None;
            return if !self.open {
                error.map(MemoryEffect::Notice).into_iter().collect()
            } else {
                Vec::new()
            };
        }
        self.verifying = true;
        match &mut self.write {
            Some(Write::Switch(_)) => vec![MemoryEffect::LoadStatus],
            Some(Write::Save(save)) => {
                save.detail = error;
                vec![
                    MemoryEffect::LoadConfig {
                        name: save.before.name.clone(),
                    },
                    MemoryEffect::LoadStatus,
                ]
            }
            Some(Write::Reset(_)) => {
                let mut effects = Vec::new();
                if let WriteOutcome::Ok(reply) = outcome {
                    let names: Vec<&str> = reply
                        .get("deleted")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .filter(|name| matches!(*name, "MEMORY.md" | "USER.md"))
                        .collect();
                    let message = if names.is_empty() {
                        "Nothing was deleted: the file did not exist.".into()
                    } else {
                        format!(
                            "Deleted {} for profile {}.",
                            names.join(" and "),
                            self.profile
                        )
                    };
                    self.banner = Some(message.clone());
                    effects.push(MemoryEffect::Notice(message));
                }
                effects.push(MemoryEffect::LoadStatus);
                effects
            }
            Some(Write::Pause(_)) => vec![MemoryEffect::LoadCurator],
            Some(Write::Run) => {
                self.write = None;
                self.verifying = false;
                self.next_follow = self.next_follow.wrapping_add(1);
                let id = self.next_follow;
                if let WriteOutcome::Ok(reply) = outcome {
                    if !self.open {
                        return self.report(
                            "Curator run started in Hermes; Talaria is not following it.".into(),
                        );
                    }
                    self.follow = Some(Follow {
                        id,
                        pid: reply.get("pid").and_then(Value::as_i64),
                        started: now,
                        polls: 0,
                        failures: 0,
                        lines: Vec::new(),
                    });
                    self.banner = None;
                    vec![MemoryEffect::PollAction {
                        follow: id,
                        after: Duration::from_millis(1200),
                    }]
                } else if unknown {
                    self.check = Some(RunCheck::UnknownStart(id));
                    vec![MemoryEffect::PollAction {
                        follow: id,
                        after: Duration::ZERO,
                    }]
                } else {
                    if !self.open {
                        error.map(MemoryEffect::Notice).into_iter().collect()
                    } else {
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        }
    }

    pub(super) fn report(&mut self, message: String) -> Vec<MemoryEffect> {
        self.banner = Some(message.clone());
        vec![MemoryEffect::Notice(message)]
    }

    fn refusal(&self, status: u16, detail: Option<&str>, route: &str) -> String {
        let detail: String = detail
            .unwrap_or("request refused")
            .chars()
            .take(200)
            .collect();
        match status {
            401 => "Hermes refused Talaria's ticket. Reconnect, then try again.".into(),
            403 => format!(
                "Hermes refused this for profile {} ({detail}).",
                self.profile
            ),
            404 => format!(
                "This Hermes has no {}. Talaria needs Hermes v0.22.4 or newer.",
                if detail == "request refused" {
                    route
                } else {
                    detail.as_str()
                }
            ),
            500..=599 if matches!(self.write, Some(Write::Reset(_))) => {
                "Hermes could not delete the file.".into()
            }
            500..=599 => format!("Hermes failed with status {status}."),
            _ => format!("Hermes refused: {detail}"),
        }
    }

    pub(super) fn status_readback(&mut self) -> Vec<MemoryEffect> {
        if !self.verifying {
            return Vec::new();
        }
        match &mut self.write {
            Some(Write::Save(save)) => {
                save.status_answered = true;
                self.finish_save()
            }
            Some(Write::Switch(_)) => {
                let Some(Write::Switch(target)) = self.write.take() else {
                    return Vec::new();
                };
                self.verifying = false;
                let Some(status) = self.status() else {
                    return self.report(format!("Could not confirm the memory provider for profile {} because Hermes's status could not be read.", self.profile));
                };
                let message = if status.active == target {
                    format!(
                        "Memory provider for profile {} is now {}. It may need a restart to take effect.",
                        self.profile,
                        provider_name(&target)
                    )
                } else {
                    format!(
                        "Hermes answered ok, but the memory provider for profile {} is still {}.",
                        self.profile,
                        provider_name(&status.active)
                    )
                };
                self.banner = Some(message.clone());
                vec![MemoryEffect::Notice(message)]
            }
            Some(Write::Reset(_)) => {
                let Some(Write::Reset(target)) = self.write.take() else {
                    return Vec::new();
                };
                self.verifying = false;
                if self.status().is_none() {
                    return self.report(format!("Could not confirm the built-in memory sizes for profile {} because Hermes's status could not be read.", self.profile));
                }
                if self.banner.as_deref() == Some(UNKNOWN)
                    && let Some(status) = self.status()
                {
                    let empty = match target {
                        ResetTarget::Memory => status.builtin_files.memory == 0,
                        ResetTarget::User => status.builtin_files.user == 0,
                        ResetTarget::All => {
                            status.builtin_files.memory == 0 && status.builtin_files.user == 0
                        }
                    };
                    let message = if empty {
                        "The selected built-in memory files are now empty."
                    } else {
                        "The selected built-in memory files still contain data."
                    };
                    self.banner = Some(message.into());
                    return vec![MemoryEffect::Notice(message.into())];
                }
                if !self.open
                    && let Some(message) = self.banner.clone()
                    && !message.starts_with("Deleted ")
                    && !message.starts_with("Nothing was deleted:")
                {
                    return self.report(message);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn curator_readback(&mut self) -> Vec<MemoryEffect> {
        if !self.verifying || !matches!(self.write, Some(Write::Pause(_))) {
            return Vec::new();
        }
        let Some(Write::Pause(target)) = self.write.take() else {
            return Vec::new();
        };
        self.verifying = false;
        let Some(curator) = self.curator() else {
            return self.report(format!("Could not confirm whether the curator is paused for profile {} because Hermes's status could not be read.", self.profile));
        };
        let mut message = if curator.paused == target {
            format!(
                "Curator {} for profile {}.",
                if target { "paused" } else { "resumed" },
                self.profile
            )
        } else {
            format!(
                "Hermes answered ok, but the curator is still {}.",
                curator_lines(curator).0
            )
        };
        if target && curator.paused && self.follow.is_some() {
            message.push_str(" A run already going is not stopped.");
        }
        self.banner = Some(message.clone());
        vec![MemoryEffect::Notice(message)]
    }

    pub(super) fn finish_save(&mut self) -> Vec<MemoryEffect> {
        if !matches!(&self.write, Some(Write::Save(save)) if save.config_answered && save.status_answered)
        {
            return Vec::new();
        }
        let Some(Write::Save(save)) = self.write.take() else {
            return Vec::new();
        };
        self.verifying = false;
        let Some(Ok(after)) = &save.after else {
            let edits = if self.card.is_some() {
                " The edits are kept for comparison."
            } else {
                ""
            };
            return self.report(format!("Could not confirm the saved settings for {} because Hermes's settings could not be read.{edits}", save.before.name));
        };
        if self.status().is_none() {
            return self.report(format!("Could not confirm the active memory provider after saving settings for {} because Hermes's status could not be read.", save.before.name));
        }
        let mut results = Vec::new();
        let mut saved = Vec::new();
        let mut failed = Vec::new();
        for before in &save.before.fields {
            let sent = save.values.plain.get(&before.key);
            let secret = save.values.secrets.contains_key(&before.key);
            if sent.is_none() && !secret {
                continue;
            }
            let after_field = after.fields.iter().find(|field| field.key == before.key);
            let outcome = if let Some(after_field) = after_field {
                classify_saved(before, sent.unwrap_or(&Value::Null), before, after_field)
            } else if secret {
                SavedOutcome::SecretNotStored
            } else {
                SavedOutcome::NotSaved
            };
            let suffix = match &outcome {
                SavedOutcome::Saved => "saved".into(),
                SavedOutcome::NotSaved => "not saved".into(),
                SavedOutcome::Adjusted(value) => format!("Hermes stored {value}"),
                SavedOutcome::Cleared(value) => format!("cleared; Hermes now shows {value}"),
                SavedOutcome::SecretStored => "stored".into(),
                SavedOutcome::SecretSetUnconfirmed => {
                    "set (Hermes does not show whether the new value replaced the old one)".into()
                }
                SavedOutcome::SecretNotStored => format!(
                    "Hermes did not store it. Set it with {}.",
                    self.setup_command(&save.before.name)
                ),
            };
            results.push(format!("{}: {suffix}", before.label));
            if matches!(
                outcome,
                SavedOutcome::NotSaved | SavedOutcome::SecretNotStored | SavedOutcome::Adjusted(_)
            ) {
                failed.push(before.key.clone());
            } else {
                saved.push(before.key.clone());
            }
        }
        let active = self.active() == save.before.name;
        if active {
            results.push(format!(
                "{} is now the memory provider for profile {}.",
                save.before.name, self.profile
            ));
        } else {
            results.push(format!(
                "{} is not the memory provider: {}",
                save.before.name,
                save.detail
                    .as_deref()
                    .unwrap_or("Hermes did not activate it.")
            ));
        }
        if save.defaults {
            results.push(DEFAULTS.into());
        }
        let message = if failed.is_empty() {
            format!(
                "Saved settings for {} on profile {}.",
                save.before.name, self.profile
            )
        } else {
            format!("Some settings for {} were not saved.", save.before.name)
        };
        if let Some(card) = &mut self.card {
            for key in saved {
                card.pending.plain.remove(&key);
                card.pending.secrets.remove(&key);
            }
            card.failed = failed;
            card.results = results;
        }
        self.banner = Some(message.clone());
        vec![MemoryEffect::Notice(message)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed_write(write: Write, now: Instant) -> MemoryState {
        let mut state = MemoryState {
            profile: "fixture".into(),
            open: true,
            write: Some(write),
            ..MemoryState::new()
        };
        assert!(
            state
                .update(MemoryInput::Key(MemoryKey::Close), now)
                .contains(&MemoryEffect::CloseOverlay)
        );
        assert!(state.is_busy());
        state
    }

    fn assert_terminal_notice(state: &MemoryState, effects: &[MemoryEffect], text: &str) {
        assert!(!state.is_busy());
        assert!(state.follow.is_none());
        assert!(
            matches!(effects, [MemoryEffect::Notice(message)] if message.contains(text)),
            "{effects:?}"
        );
    }

    #[test]
    fn a_run_started_after_close_reports_without_following() {
        let now = Instant::now();
        let mut state = closed_write(Write::Run, now);
        let effects = state.update(
            MemoryInput::RunStarted(WriteOutcome::Ok(serde_json::json!({"pid": 42}))),
            now,
        );
        assert_terminal_notice(&state, &effects, "started in Hermes");
    }

    #[test]
    fn uncertain_run_after_close_keeps_the_readback_until_a_notice() {
        let now = Instant::now();
        for result in [
            Ok(ActionView {
                name: "curator-run".into(),
                running: true,
                pid: Some(42),
                exit_code: None,
                lines: vec![],
            }),
            Ok(ActionView {
                name: "curator-run".into(),
                running: false,
                pid: None,
                exit_code: None,
                lines: vec![],
            }),
            Err(FailText("offline".into())),
        ] {
            let mut state = closed_write(Write::Run, now);
            let effects = state.update(MemoryInput::RunStarted(WriteOutcome::Unknown), now);
            let [MemoryEffect::PollAction { follow, after }] = effects.as_slice() else {
                panic!("expected one readback");
            };
            assert_eq!(*after, Duration::ZERO);
            assert!(state.is_busy());
            state.close_or_discard(None);
            assert!(state.is_busy());
            let running = result.as_ref().is_ok_and(|action| action.running);
            let effects = state.update(
                MemoryInput::ActionPolled {
                    follow: *follow,
                    result,
                },
                now,
            );
            assert_terminal_notice(
                &state,
                &effects,
                if running {
                    "not following it"
                } else {
                    "Could not confirm"
                },
            );
        }
    }

    #[test]
    fn terminal_write_failures_after_close_are_reported() {
        let now = Instant::now();
        for outcome in [
            WriteOutcome::NotSent(FailText("Could not reach Hermes.".into())),
            WriteOutcome::Refused {
                status: 401,
                detail: None,
            },
            WriteOutcome::Refused {
                status: 403,
                detail: None,
            },
            WriteOutcome::Refused {
                status: 404,
                detail: None,
            },
        ] {
            let mut state = closed_write(Write::Switch("sample".into()), now);
            let effects = state.update(MemoryInput::ProviderSet(outcome), now);
            assert_terminal_notice(&state, &effects, "Hermes");
        }
        let mut state = closed_write(Write::Run, now);
        let effects = state.update(
            MemoryInput::RunStarted(WriteOutcome::Failed(FailText(
                "Hermes failed with status 500.".into(),
            ))),
            now,
        );
        assert_terminal_notice(&state, &effects, "Hermes failed");
    }

    #[test]
    fn failed_readbacks_after_close_report_the_unconfirmed_outcome() {
        let now = Instant::now();
        let mut state = closed_write(Write::Switch("sample".into()), now);
        state.update(MemoryInput::ProviderSet(WriteOutcome::Ok(Value::Null)), now);
        let effects = state.update(
            MemoryInput::StatusLoaded(Err(FailText("offline".into()))),
            now,
        );
        assert_terminal_notice(&state, &effects, "Could not confirm the memory provider");

        let mut state = closed_write(Write::Pause(true), now);
        state.update(MemoryInput::PausedSet(WriteOutcome::Ok(Value::Null)), now);
        let effects = state.update(
            MemoryInput::CuratorLoaded(Err(FailText("offline".into()))),
            now,
        );
        assert_terminal_notice(
            &state,
            &effects,
            "Could not confirm whether the curator is paused",
        );

        let before = ConfigView {
            name: "sample".into(),
            label: "Sample".into(),
            surface: ConfigSurface::Plain,
            fields: vec![],
        };
        let mut state = closed_write(
            Write::Save(Box::new(SaveCheck {
                before,
                values: SubmittedValues::default(),
                defaults: false,
                config_answered: false,
                after: None,
                status_answered: false,
                detail: None,
            })),
            now,
        );
        state.update(MemoryInput::ConfigSaved(WriteOutcome::Ok(Value::Null)), now);
        state.update(
            MemoryInput::ConfigLoaded {
                name: "sample".into(),
                result: Err(FailText("offline".into())),
            },
            now,
        );
        assert!(state.is_busy());
        let effects = state.update(
            MemoryInput::StatusLoaded(Err(FailText("offline".into()))),
            now,
        );
        assert_terminal_notice(&state, &effects, "Could not confirm the saved settings");
    }
}
