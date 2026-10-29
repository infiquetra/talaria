use super::{configure_ctx, events, snapshot_json};

use configure_ctx::{context, detect};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use talaria_core::configure::*;
use talaria_core::session::{PromptKey, SessionId};

fn block(value: Value) -> String {
    format!("```talaria-configure\n{value}\n```")
}

fn proposal(changes: Value) -> String {
    block(json!({"kind":"proposal", "summary":"requested changes", "changes":changes}))
}

fn thinking(now: Instant) -> ConfigureState {
    let mut state = ConfigureState::new(detect);
    state.update(
        ConfigureInput::Open {
            request: "change settings".into(),
            profile: "fixture".into(),
            connected: true,
            model: None,
        },
        now,
    );
    state.update(ConfigureInput::Gathered(Box::new(context())), now);
    let mut snapshot = snapshot_json::base("session-1");
    snapshot["info"] =
        json!({"desktop_protocol":"1", "yolo":false, "tools":{"clarify":["clarify"]}});
    state.update(
        ConfigureInput::Created(Box::new(snapshot_json::snapshot(snapshot))),
        now,
    );
    state.update(ConfigureInput::Submitted, now);
    state
}

fn event(
    state: &mut ConfigureState,
    seq: u64,
    kind: &str,
    payload: Value,
    now: Instant,
) -> Vec<ConfigureEffect> {
    state.update(
        ConfigureInput::Session(events::ev_session("session-1", seq, kind, payload)),
        now,
    )
}

fn complete(
    state: &mut ConfigureState,
    seq: u64,
    status: &str,
    text: &str,
    now: Instant,
) -> Vec<ConfigureEffect> {
    event(
        state,
        seq,
        "message.complete",
        json!({"status":status,"text":text}),
        now,
    )
}

fn text(state: &ConfigureState, now: Instant) -> String {
    state.view(now).text_lines().join("\n")
}

fn key(state: &mut ConfigureState, key: ConfigureKey, now: Instant) -> Vec<ConfigureEffect> {
    state.update(ConfigureInput::Key(key), now)
}

fn review(changes: Value) -> ConfigureState {
    ConfigureState::from_reply(context(), &proposal(changes), detect)
}

#[test]
fn turn_outcomes_map_to_phases() {
    let now = Instant::now();
    let cases = [
        (
            "complete",
            proposal(json!([{"op":"set","key":"display.compact","value":true}])),
            Phase::Review,
            "compact",
        ),
        (
            "complete",
            block(json!({"kind":"answer","text":"Nothing to change"})),
            Phase::Answer,
            "Nothing to change",
        ),
        (
            "complete",
            block(json!({"kind":"cannot","reason":"unsupported"})),
            Phase::Cannot,
            "unsupported",
        ),
        (
            "complete",
            "ordinary prose".into(),
            Phase::NoProposal,
            "without a proposal",
        ),
        (
            "complete",
            "```talaria-configure\n{\n```".into(),
            Phase::NoProposal,
            "could not be read",
        ),
        (
            "complete",
            proposal(json!(
                (0..21)
                    .map(|_| json!({"op":"set","key":"display.compact","value":true}))
                    .collect::<Vec<_>>()
            )),
            Phase::NoProposal,
            "at most 20",
        ),
        ("error", "failed".into(), Phase::Failed, "failed"),
        ("interrupted", "".into(), Phase::Cancelled, "Cancelled"),
    ];
    for (status, reply, phase, expected) in cases {
        let mut state = thinking(now);
        complete(&mut state, 1, status, &reply, now);
        assert_eq!(state.view(now).phase, phase);
        assert!(
            text(&state, now).contains(expected),
            "{}",
            text(&state, now)
        );
        complete(
            &mut state,
            2,
            "complete",
            &proposal(json!([{"op":"set","key":"display.compact","value":true}])),
            now,
        );
        assert_eq!(state.view(now).phase, phase);
    }
}

#[test]
fn tool_and_approval_guards() {
    let now = Instant::now();
    let mut state = thinking(now);
    let effects = event(
        &mut state,
        1,
        "tool.generating",
        json!({"name":"terminal"}),
        now,
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Interrupt { .. }))
    );
    assert_eq!(state.view(now).phase, Phase::Failed);
    let mut state = thinking(now);
    let effects = event(
        &mut state,
        1,
        "approval.request",
        events::approval_request("p1", 1, &["deny"]),
        now,
    );
    let deny = effects
        .iter()
        .position(|e| matches!(e, ConfigureEffect::DenyApproval { .. }))
        .unwrap();
    let interrupt = effects
        .iter()
        .position(|e| matches!(e, ConfigureEffect::Interrupt { .. }))
        .unwrap();
    assert!(deny < interrupt);
    let mut state = thinking(now);
    let effects = state.update(
        ConfigureInput::ServerRequest {
            request_id: "r1".into(),
            method: "secret".into(),
        },
        now,
    );
    assert!(matches!(
        effects.first(),
        Some(ConfigureEffect::RefuseServerRequest { .. })
    ));
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Interrupt { .. }))
    );
    let mut state = thinking(now);
    event(
        &mut state,
        1,
        "clarify.request",
        events::clarify_request("p1", 1, "Which?"),
        now,
    );
    assert_eq!(state.view(now).phase, Phase::Question);
    let key = PromptKey {
        session_id: SessionId::new("session-1"),
        prompt_id: "p1".into(),
        execution_generation: 1,
    };
    let effects = state.update(
        ConfigureInput::ClarifyAnswered {
            key: key.clone(),
            answer: "sk-or-v1-talaria-sentinel-0000000000000000".into(),
        },
        now,
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::ClearEditor))
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::RespondClarify { .. }))
    );
    assert!(text(&state, now).contains("not sent"));
    assert!(
        state
            .update(
                ConfigureInput::ClarifyAnswered {
                    key,
                    answer: "yes".into()
                },
                now
            )
            .iter()
            .any(|e| matches!(e, ConfigureEffect::RespondClarify { .. }))
    );
}

#[test]
fn context_rounds_are_capped() {
    let now = Instant::now();
    let mut state = thinking(now);
    let need = block(json!({"kind":"need","sections":["skills"]}));
    for n in 1..=3 {
        let effects = complete(&mut state, n, "complete", &need, now);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, ConfigureEffect::Submit { .. }))
        );
        state.update(ConfigureInput::Submitted, now);
    }
    complete(
        &mut state,
        4,
        "complete",
        &block(json!({"kind":"answer","text":"done"})),
        now,
    );
    key(&mut state, ConfigureKey::Char('r'), now);
    key(&mut state, ConfigureKey::Paste("try again".into()), now);
    assert!(
        key(&mut state, ConfigureKey::Enter, now)
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Submit { .. }))
    );
    state.update(ConfigureInput::Submitted, now);
    let effects = complete(&mut state, 5, "complete", &need, now);
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Submit { .. }))
    );
    assert!(text(&state, now).contains("asked for more context"));
}

#[test]
fn review_selection_confirm_and_discard() {
    let now = Instant::now();
    let mut state = review(
        json!([{"op":"set","key":"display.compact","value":true,"why":"less clutter"},{"op":"mcp_remove","name":"alpha"}]),
    );
    let lines = text(&state, now);
    assert!(lines.contains("[x]") && lines.contains("[ ]") && lines.contains('!'));
    key(&mut state, ConfigureKey::Char('d'), now);
    assert!(text(&state, now).contains("less clutter"));
    key(&mut state, ConfigureKey::Down, now);
    key(&mut state, ConfigureKey::Space, now);
    key(&mut state, ConfigureKey::Char('a'), now);
    assert!(text(&state, now).contains("Apply 2 change(s)"));
    let view = state.view(now);
    assert!(
        view.confirmation
            .as_deref()
            .unwrap()
            .contains("1 removal(s)")
    );
    assert!(view.footer.contains("y apply") && view.footer.contains("n back"));
    key(&mut state, ConfigureKey::Char('n'), now);
    assert_eq!(state.view(now).phase, Phase::Review);
    key(&mut state, ConfigureKey::Esc, now);
    assert!(text(&state, now).contains("Discard this proposal"));
    let view = state.view(now);
    assert!(view.confirmation.as_deref().unwrap().contains("Discard"));
    assert!(view.footer.contains("y discard") && view.footer.contains("n back"));
    let effects = key(&mut state, ConfigureKey::Char('y'), now);
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Apply { .. }))
    );
}

#[test]
fn refine_replaces_the_proposal() {
    let now = Instant::now();
    let mut state = thinking(now);
    complete(
        &mut state,
        1,
        "complete",
        &proposal(json!([{"op":"set","key":"display.compact","value":true}])),
        now,
    );
    key(&mut state, ConfigureKey::Char('r'), now);
    key(
        &mut state,
        ConfigureKey::Paste("sk-or-v1-talaria-sentinel-0000000000000000".into()),
        now,
    );
    let refused = key(&mut state, ConfigureKey::Enter, now);
    assert!(
        refused
            .iter()
            .any(|e| matches!(e, ConfigureEffect::ClearEditor))
    );
    assert!(
        !refused
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Submit { .. }))
    );
    key(&mut state, ConfigureKey::Paste("only the bell".into()), now);
    let later = now + Duration::from_secs(10);
    assert!(
        key(&mut state, ConfigureKey::Enter, later)
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Submit { .. }))
    );
    assert_eq!(
        state.next_deadline(),
        Some(later + Duration::from_secs(300))
    );
    state.update(ConfigureInput::Submitted, later);
    complete(
        &mut state,
        2,
        "complete",
        &proposal(json!([{"op":"set","key":"display.bell_on_complete","value":true}])),
        later,
    );
    assert!(text(&state, later).contains("bell_on_complete"));
    assert!(!text(&state, later).contains("compact"));
}

#[test]
fn apply_plan_order_and_outcomes() {
    let now = Instant::now();
    let changes = json!([
        {"op":"mcp_add","name":"deepwiki","preset":"deepwiki"},
        {"op":"main_model","provider":"ollama-cloud","model":"another"},
        {"op":"set","key":"display.compact","value":true},
        {"op":"cron_pause","job_id":"j1"}
    ]);
    let mut state = review(changes);
    key(&mut state, ConfigureKey::Down, now);
    key(&mut state, ConfigureKey::Down, now);
    key(&mut state, ConfigureKey::Down, now);
    key(&mut state, ConfigureKey::Space, now);
    key(&mut state, ConfigureKey::Char('a'), now);
    assert!(
        !key(&mut state, ConfigureKey::Char('n'), now)
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Apply { .. }))
    );
    key(&mut state, ConfigureKey::Char('a'), now);
    let effects = key(&mut state, ConfigureKey::Char('y'), now);
    let (index, rows) = effects
        .iter()
        .find_map(|e| match e {
            ConfigureEffect::Apply {
                index,
                step: ApplyStep::Settings { rows, baseline, .. },
            } => {
                assert_eq!(baseline, &context().effective);
                Some((*index, rows.clone()))
            }
            _ => None,
        })
        .expect("settings must run first");
    let effects = state.update(
        ConfigureInput::StepDone {
            index,
            result: StepResult::Rows(vec![(rows[0].0, RowOutcome::Failed("refused".into()))]),
        },
        now,
    );
    let index = effects
        .iter()
        .find_map(|e| match e {
            ConfigureEffect::Apply {
                index,
                step:
                    ApplyStep::MainModel {
                        confirmed: false, ..
                    },
            } => Some(*index),
            _ => None,
        })
        .expect("failure continues to model");
    let effects = state.update(
        ConfigureInput::StepDone {
            index,
            result: StepResult::NeedsConfirm {
                row: 1,
                message: "confirm route".into(),
            },
        },
        now,
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Apply { .. }))
    );
    assert!(text(&state, now).contains("confirm route"));
    let view = state.view(now);
    assert_eq!(view.confirmation.as_deref(), Some("confirm route"));
    assert!(view.footer.contains("y apply") && view.footer.contains("n skip"));
    let effects = key(&mut state, ConfigureKey::Char('y'), now);
    assert!(effects.iter().any(|e| matches!(
        e,
        ConfigureEffect::Apply {
            step: ApplyStep::MainModel {
                confirmed: true,
                ..
            },
            ..
        }
    )));
    let effects = state.update(
        ConfigureInput::StepDone {
            index,
            result: StepResult::Rows(vec![(1, RowOutcome::Unknown("lost".into()))]),
        },
        now,
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Apply { .. }))
    );
    assert_eq!(state.view(now).phase, Phase::Done);
    assert!(text(&state, now).contains("not attempted"));

    let mut state = review(
        json!([{"op":"set","key":"display.compact","value":true},{"op":"set","key":"display.bell_on_complete","value":true}]),
    );
    key(&mut state, ConfigureKey::Char('a'), now);
    let effects = key(&mut state, ConfigureKey::Char('y'), now);
    let index = effects
        .iter()
        .find_map(|e| {
            if let ConfigureEffect::Apply { index, .. } = e {
                Some(*index)
            } else {
                None
            }
        })
        .unwrap();
    state.update(
        ConfigureInput::StepDone {
            index,
            result: StepResult::Rows(vec![(0, RowOutcome::Applied)]),
        },
        now,
    );
    assert!(text(&state, now).contains("1 unknown"));
}

#[test]
fn settings_step_reports_each_key() {
    use talaria_core::settings::{KeyOutcome, build_patch, classify_readback, find_conflicts};
    let now = Instant::now();
    let mut ctx = context();
    ctx.effective["max_live_sessions"] = json!(3);
    ctx.raw["max_live_sessions"] = json!(3);
    ctx.defaults["max_live_sessions"] = json!(3);
    let reply = proposal(json!([
        {"op":"set","key":"display.compact","value":true},
        {"op":"set","key":"display.bell_on_complete","value":true},
        {"op":"set","key":"max_live_sessions","value":5}
    ]));
    let mut state = ConfigureState::from_reply(ctx.clone(), &reply, detect);
    key(&mut state, ConfigureKey::Char('a'), now);
    let effects = key(&mut state, ConfigureKey::Char('y'), now);
    let (index, rows, mut changes, baseline) = effects
        .into_iter()
        .find_map(|e| match e {
            ConfigureEffect::Apply {
                index,
                step:
                    ApplyStep::Settings {
                        rows,
                        changes,
                        baseline,
                    },
            } => Some((index, rows, changes, baseline)),
            _ => None,
        })
        .unwrap();
    let mut fresh = baseline.clone();
    fresh["display"]["compact"] = json!(true);
    let (conflicts, _) = find_conflicts(&changes, &baseline, &fresh);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].key, "display.compact");
    changes.remove("display.compact");
    assert_eq!(
        build_patch(&changes),
        json!({"display":{"bell_on_complete":true},"max_live_sessions":5})
    );
    assert_eq!(
        classify_readback(&json!(true), Some(&json!(false)), Some(&json!(true))),
        KeyOutcome::Applied
    );
    assert_eq!(
        classify_readback(&json!(5), Some(&json!(3)), Some(&json!(4))),
        KeyOutcome::Adjusted(json!(4))
    );
    let results = rows
        .iter()
        .map(|(index, key)| {
            (
                *index,
                match key.as_str() {
                    "display.compact" => RowOutcome::Conflict("changed since the proposal".into()),
                    "display.bell_on_complete" => RowOutcome::Applied,
                    "max_live_sessions" => RowOutcome::Adjusted("4".into()),
                    _ => panic!("unexpected setting"),
                },
            )
        })
        .collect();
    state.update(
        ConfigureInput::StepDone {
            index,
            result: StepResult::Rows(results),
        },
        now,
    );
    let visible = text(&state, now);
    assert!(visible.contains('!') && visible.contains('✓') && visible.contains('~'));
    assert!(visible.contains("2 applied, 1 failed"));
}

#[test]
fn done_summary_notice_and_next_steps() {
    let now = Instant::now();
    let mut state = review(json!([
        {"op":"set","key":"display.compact","value":true},
        {"op":"set","key":"display.bell_on_complete","value":true},
        {"op":"open","screen":"providers","target":"openrouter"}
    ]));
    key(&mut state, ConfigureKey::Char('a'), now);
    let effects = key(&mut state, ConfigureKey::Char('y'), now);
    let index = effects
        .iter()
        .find_map(|e| {
            if let ConfigureEffect::Apply { index, .. } = e {
                Some(*index)
            } else {
                None
            }
        })
        .unwrap();
    state.update(
        ConfigureInput::StepDone {
            index,
            result: StepResult::Rows(vec![
                (0, RowOutcome::Applied),
                (1, RowOutcome::Adjusted("true".into())),
            ]),
        },
        now,
    );
    let visible = text(&state, now);
    assert!(visible.contains("2 applied, 0 failed, 0 unknown, 0 not attempted"));
    assert!(visible.contains("Next steps:"));
    assert!(visible.contains("restart"));
    let effects = key(&mut state, ConfigureKey::Char('1'), now);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Open(_)))
    );
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(e, ConfigureEffect::Notice(_)))
            .count(),
        1
    );

    let mut state = ConfigureState::new(detect);
    state.update(
        ConfigureInput::Open {
            request: "change settings".into(),
            profile: "fixture".into(),
            connected: true,
            model: None,
        },
        now,
    );
    let effects = key(&mut state, ConfigureKey::Esc, now);
    let notices: Vec<_> = effects
        .iter()
        .filter_map(|e| {
            if let ConfigureEffect::Notice(s) = e {
                Some(s)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].contains("nothing was applied"));
    assert!(
        !key(&mut state, ConfigureKey::Esc, now)
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Notice(_)))
    );
}

#[test]
fn apply_plan_orders_all_owner_steps() {
    let mut ctx = context();
    if let Section::Available(routes) = &mut ctx.inventory.routes {
        routes[0].api_url = "https://example.org/api".into();
    }
    let ready = |change| Row {
        change,
        status: RowStatus::Ready,
        destructive: false,
        text: String::new(),
        detail: vec![],
        effect_label: None,
    };
    let rows = vec![
        ready(Change::CronPause {
            job_id: "j1".into(),
            why: None,
        }),
        ready(Change::Toggle {
            kind: CapKind::Skill,
            name: "notes".into(),
            enabled: false,
            why: None,
        }),
        ready(Change::McpRemove {
            name: "alpha".into(),
            why: None,
        }),
        ready(Change::MainModel {
            provider: "ollama-cloud".into(),
            model: "another".into(),
            why: None,
        }),
        ready(Change::Set {
            key: "display.compact".into(),
            value: json!(true),
            why: None,
        }),
        ready(Change::McpAdd {
            name: "deepwiki".into(),
            source: McpSource::Preset("deepwiki".into()),
            why: None,
        }),
        ready(Change::McpEnable {
            name: "alpha".into(),
            enabled: false,
            why: None,
        }),
        ready(Change::CronAdd {
            name: "report".into(),
            schedule: "daily".into(),
            prompt: "report".into(),
            repeat: None,
            deliver: None,
            why: None,
        }),
        ready(Change::Set {
            key: "display.bell_on_complete".into(),
            value: json!(true),
            why: None,
        }),
        ready(Change::Toggle {
            kind: CapKind::Toolset,
            name: "web".into(),
            enabled: false,
            why: None,
        }),
    ];
    let steps = plan_apply(&rows, &vec![true; rows.len()], &ctx);
    assert_eq!(steps.len(), 9);
    match &steps[0] {
        ApplyStep::Settings {
            rows,
            changes,
            baseline,
        } => {
            assert_eq!(
                rows,
                &vec![
                    (4, "display.compact".into()),
                    (8, "display.bell_on_complete".into())
                ]
            );
            assert_eq!(changes.len(), 2);
            assert_eq!(baseline, &ctx.effective);
        }
        _ => panic!("settings must run first"),
    }
    assert!(
        matches!(&steps[1], ApplyStep::MainModel { row: 3, base_url, confirmed: false, .. } if base_url == "https://example.org/api")
    );
    assert!(matches!(&steps[2], ApplyStep::McpRemove { row: 2, .. }));
    assert!(matches!(&steps[3], ApplyStep::McpAdd { row: 5, .. }));
    assert!(matches!(&steps[4], ApplyStep::McpEnable { row: 6, .. }));
    assert!(matches!(&steps[5], ApplyStep::Capability { row: 1, .. }));
    assert!(matches!(&steps[6], ApplyStep::Capability { row: 9, .. }));
    assert!(matches!(
        &steps[7],
        ApplyStep::CronControl {
            row: 0,
            action: CronControl::Pause,
            ..
        }
    ));
    assert!(matches!(&steps[8], ApplyStep::CronAdd { row: 7, deliver, .. } if deliver == "local"));
}

#[test]
fn done_counts_each_outcome_and_only_attempted_selections() {
    use talaria_core::settings::{FieldKind, FieldView};
    let now = Instant::now();
    let mut ctx = context();
    let mut changes = Vec::new();
    for i in 0..8 {
        let name = format!("sample{i}");
        ctx.schema.fields.push(FieldView {
            key: name.clone(),
            kind: FieldKind::Number,
            description: String::new(),
            category: "general".into(),
            options: None,
            searchable: false,
            clearable: false,
        });
        ctx.effective[name.as_str()] = json!(0);
        ctx.raw[name.as_str()] = json!(0);
        ctx.defaults[name.as_str()] = json!(0);
        changes.push(json!({"op":"set","key":name,"value":1}));
    }
    changes.push(json!({"op":"mcp_add","name":"deepwiki","preset":"deepwiki"}));
    changes.push(json!({"op":"set","key":"display.compact","value":false}));
    changes.push(json!({"op":"set","key":"missing.setting","value":true}));
    changes.push(json!({"op":"open","screen":"providers","target":"openrouter"}));
    let mut state = ConfigureState::from_reply(ctx, &proposal(json!(changes)), detect);
    // Leave one otherwise-ready row unselected.
    for _ in 0..7 {
        key(&mut state, ConfigureKey::Down, now);
    }
    key(&mut state, ConfigureKey::Space, now);
    key(&mut state, ConfigureKey::Char('a'), now);
    let effects = key(&mut state, ConfigureKey::Char('y'), now);
    let (index, planned) = effects
        .iter()
        .find_map(|effect| match effect {
            ConfigureEffect::Apply {
                index,
                step: ApplyStep::Settings { rows, .. },
            } => Some((*index, rows)),
            _ => None,
        })
        .unwrap();
    assert_eq!(planned.len(), 7);
    state.update(
        ConfigureInput::StepDone {
            index,
            result: StepResult::Rows(vec![
                (0, RowOutcome::Applied),
                (1, RowOutcome::Adjusted("2".into())),
                (2, RowOutcome::NoChange("already set".into())),
                (3, RowOutcome::Failed("failed".into())),
                (4, RowOutcome::NotApplied("not applied".into())),
                (5, RowOutcome::Conflict("changed".into())),
                (6, RowOutcome::Unknown("lost".into())),
            ]),
        },
        now,
    );
    let visible = text(&state, now);
    assert!(
        visible.contains("3 applied, 3 failed, 1 unknown, 1 not attempted"),
        "{visible}"
    );
    for mark in ['✓', '~', '=', '✗', '!', '?', '-'] {
        assert!(visible.contains(mark), "missing {mark}: {visible}");
    }
    assert!(visible.contains("Next steps:"));
    let effects = key(&mut state, ConfigureKey::Enter, now);
    let notices: Vec<_> = effects
        .iter()
        .filter_map(|effect| {
            if let ConfigureEffect::Notice(message) = effect {
                Some(message)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].contains("3 applied, 3 failed, 1 unknown"));
    assert!(notices[0].contains("next:"));
}
