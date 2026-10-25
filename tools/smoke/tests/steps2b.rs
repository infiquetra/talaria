//! The phase 2b run on the fake terminal and the manual clock.
//!
//! Each passing script runs a passing run-up to its step and asserts the pass, the keys sent
//! in order and one `close_tab`. Each failing script asserts the printed reason exactly, the
//! keys sent in order, the times and one `close_tab`. Times are asserted two ways: the reason
//! text names the limit in force, and for waits the run is shown to have waited at least the
//! limit since the step's trigger call. All fail until the stub `run_smoke_2b` is replaced.
//!
//! Reason-to-test map (every exact reason, and the test that asserts it):
//!
//! | Function | Reasons asserted |
//! |---|
//! | `context_empty_passes`, `context_empty_context_line_missing_fails`, `context_empty_usage_line_missing_fails` | context-empty: no context line, no usage line |
//! | `permission_passes`, `permission_start_not_manual_fails`, `permission_no_off_segment_fails`, `permission_no_off_notice_fails`, `permission_no_restore_fails`, `permission_skips_when_both_key_names_are_rejected` | permission: start, first press, notice, second press, skip |
//! | `model_picker_passes`, `model_picker_open_missing_fails`, `model_picker_rows_missing_fails`, `model_picker_no_match_missing_fails`, `model_picker_esc_steps_fails` | model-picker: not open, rows, filter, Esc |
//! | `model_refusal_passes`, `model_refusal_refusal_missing_fails`, `model_refusal_model_changed_fails` | model-refusal: no refusal, model changed |
//! | `rewind_passes`, `rewind_reply_timeout_fails`, `rewind_overlay_missing_fails`, `rewind_list_rows_fails`, `rewind_rewound_missing_fails`, `rewind_composer_not_restored_fails`, `rewind_wrong_code_word_fails`, `rewind_overlay_left_text_fails` | rewind: reply, overlay, rows, no marker, composer, answer, scrollback |
//! | `plain_answer_passes` | the joined token reply (no failing rows) |
//! | `steer_idle_passes`, `steer_idle_notice_missing_fails`, `steer_idle_reply_missing_fails` | steer-idle: notice, reply |
//! | `context_passes`, `context_header_missing_fails`, `context_rows_missing_fails`, `context_usage_missing_fails`, `context_aggressive_missing_fails`, `context_argument_missing_fails`, `context_compressed_appeared_fails` | context: header, rows, usage, aggressive, argument, compressed |
//! | `hermes_commands_passes`, `hermes_commands_fast_missing_fails`, `hermes_commands_loop_missing_fails`, `hermes_commands_loop_line_fails` | hermes-commands: /fast, /loop, loop line |
//! | `rename_export_passes`, `rename_export_renamed_missing_fails`, `rename_export_title_missing_fails`, `rename_export_exported_missing_fails`, `rename_export_file_bad_fails`, `rename_export_bad_name_missing_fails`, `rename_export_bad_name_wrote_fails` | rename-export: rename, title, export, file, bad name, bad name wrote |
//! | `queue_passes`, `queue_indicator_missing_fails`, `queue_preview_twice_fails`, `queue_first_missing_fails`, `queue_omega_missing_fails`, `queue_order_wrong_fails`, `queue_indicator_left_fails` | queue: no indicator, preview twice, first, omega, order, indicator left |
//! | `cd_passes`, `cd_moved_missing_fails`, `cd_segment_wrong_fails`, `cd_show_missing_fails`, `cd_missing_folder_fails` | cd: moved, segment, show, missing folder |
//! | `quit_passes`, `quit_token_missing_fails` | quit: token A |
//! | `headless_passes`, `headless_text_run_fails`, `headless_no_gateway_json_fails`, `headless_no_gateway_text_fails`, `headless_command_timeout_fails` | headless: text run, no gateway json, no gateway text, timeout |
//! | `a_failing_step_stops_the_run_and_cleans_up` | later steps print nothing; one `close_tab`; any: step failure, terminal error (one case each) |
//! | `deadline_names_the_step` | the 1500 s deadline, one step past it: `deadline exceeded in step <name>` |
//! | `a_skip_is_never_a_pass`, `live_require_turns_a_skip_into_a_failure` | `PASSED WITH SKIPS`, exit 3; with `TALARIA_LIVE_REQUIRE=1`: `FAIL permission (skipped: ...; TALARIA_LIVE_REQUIRE=1)`, exit 1 |
//! | `step_scale_leaves_quiet_periods` | with scale 0.5 the 5 s limits become 3 s (rounded) and the 3 s settle stays 3 s |
//! | `steer_idle_and_queue_retry_once_then_fail` | miss-then-hit passes, two misses fail, a miss while working resubmits only after the settle |
//! | `steps_run_in_the_documented_order` | all 15 `PASS` lines in order, `SMOKE_RESULT: PASSED`, exit 0, one `close_tab` |
//! | `shared_steps_behave_as_in_phase_1` | the first suite's launch line, the token prompt submit and the `/quit` run |

mod support;

use std::time::Duration;

use smoke::{SmokeConfig, StepOutcome};
use support::scripts2b::{
    Cd, Context, ContextEmpty, Exec2b, Headless, HermesCommands, ModelPicker, ModelRefusal,
    Permission, Queue, Quit, RenameExport, Rewind, Shapes, Startup, SteerIdle, Step,
    assert_fails_with, assert_keys_exact, assert_keys_in_order, assert_one_close_tab,
    assert_passes, exec_2b, script_2b,
};
use support::{Call, FailBuilder};

// ---- helpers ----

/// The keys each key-sending step sends, in run order.
const K_PERMISSION: &[&str] = &["shift+tab", "shift+tab"];
const K_PICKER: &[&str] = &["esc", "esc"];
const K_REWIND: &[&str] = &["esc", "esc", "enter", "enter", "ctrl+c"];

fn keys<'a>(parts: &[&'a [&'a str]]) -> Vec<&'a str> {
    parts.iter().flat_map(|part| part.iter().copied()).collect()
}

fn play(shapes: &Shapes, stop: Step) -> Exec2b {
    let script = script_2b(shapes, stop);
    exec_2b(&script, |_| {}, |term| term)
}

fn play_cfg(shapes: &Shapes, stop: Step, tune: impl FnOnce(&mut SmokeConfig)) -> Exec2b {
    let script = script_2b(shapes, stop);
    exec_2b(&script, tune, |term| term)
}

/// The time of the last terminal call matching the predicate.
fn last_call_at(exec: &Exec2b, want: impl Fn(&Call) -> bool) -> Duration {
    exec.term
        .calls()
        .into_iter()
        .rev()
        .find_map(|recorded| want(&recorded.call).then_some(recorded.at))
        .expect("the trigger call")
}

/// The time of the last submit holding the needle.
fn last_run_at(exec: &Exec2b, needle: &str) -> Duration {
    last_call_at(
        exec,
        |call| matches!(call, Call::Run { text, .. } if text.contains(needle)),
    )
}

/// The time of the last key press sending this key.
fn last_keys_at(exec: &Exec2b, key: &str) -> Duration {
    last_call_at(
        exec,
        |call| matches!(call, Call::SendKeys { keys, .. } if keys.iter().any(|k| k == key)),
    )
}

/// The time of the last typed text holding the needle.
fn last_typed_at(exec: &Exec2b, needle: &str) -> Duration {
    last_call_at(
        exec,
        |call| matches!(call, Call::SendText { text, .. } if text.contains(needle)),
    )
}

/// The run waited at least the limit since the trigger call.
fn assert_waited(exec: &Exec2b, trigger: Duration, limit: u64) {
    assert!(
        exec.last_at() >= trigger + Duration::from_secs(limit),
        "the step waited at least {limit} s; lines:\n{}",
        exec.lines()
    );
}

/// The elapsed time of the passing step.
fn pass_elapsed(exec: &Exec2b, step: &str) -> Duration {
    exec.report
        .outcomes
        .iter()
        .find_map(|outcome| match outcome {
            StepOutcome::Pass {
                step: passed,
                elapsed,
            } if *passed == step => Some(*elapsed),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{step} must pass; lines:\n{}", exec.lines()))
}

/// The times of the submits holding the needle, oldest first.
fn submit_times(exec: &Exec2b, needle: &str) -> Vec<Duration> {
    exec.term
        .runs()
        .into_iter()
        .filter(|(_, _, text)| text.contains(needle))
        .map(|(at, _, _)| at)
        .collect()
}

// ---- the run ----

#[test]
fn startup_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "startup");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn context_empty_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "context-empty");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn context_empty_context_line_missing_fails() {
    let shapes = Shapes {
        context_empty: ContextEmpty::ContextLineMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ContextEmpty);
    assert_fails_with(
        &exec,
        "context-empty",
        "/context did not show 'Context \u{00b7} not measured yet' within 5 s",
    );
    let trigger = last_run_at(&exec, "/context");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn context_empty_usage_line_missing_fails() {
    let shapes = Shapes {
        context_empty: ContextEmpty::UsageLineMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ContextEmpty);
    assert_fails_with(
        &exec,
        "context-empty",
        "/usage did not show 'No API calls yet in this session.' within 5 s",
    );
    let trigger = last_run_at(&exec, "/usage");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn permission_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "permission");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn permission_start_not_manual_fails() {
    let shapes = Shapes {
        startup: Startup::OffAtStart,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Permission);
    assert_fails_with(
        &exec,
        "permission",
        "the status line did not show 'approvals: manual' at the start",
    );
    assert!(
        exec.sent_keys().is_empty(),
        "no press before the start check: {:?}",
        exec.sent_keys()
    );
    assert_one_close_tab(&exec);
}

#[test]
fn permission_no_off_segment_fails() {
    let shapes = Shapes {
        permission: Permission::NoOffSegment,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Permission);
    assert_fails_with(
        &exec,
        "permission",
        "Shift+Tab did not change the status line to 'approvals: off (this session)' within 5 s",
    );
    assert_keys_in_order(&exec, &["shift+tab"]);
    let trigger = last_keys_at(&exec, "shift+tab");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn permission_no_off_notice_fails() {
    let shapes = Shapes {
        permission: Permission::NoOffNotice,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Permission);
    assert_fails_with(
        &exec,
        "permission",
        "the notice 'Approvals off for this session only' did not appear within 5 s",
    );
    assert_keys_in_order(&exec, &["shift+tab"]);
    let trigger = last_keys_at(&exec, "shift+tab");
    assert_waited(&exec, trigger, 5);
    assert!(
        exec.run_texts()
            .iter()
            .any(|text| text == "/permissions session default"),
        "the cleanup line after the working press: {:?}",
        exec.run_texts()
    );
    assert_one_close_tab(&exec);
}

#[test]
fn permission_no_restore_fails() {
    let shapes = Shapes {
        permission: Permission::NoRestore,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Permission);
    assert_fails_with(
        &exec,
        "permission",
        "the second Shift+Tab did not restore 'approvals: manual' within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION]));
    let trigger = last_keys_at(&exec, "shift+tab");
    assert_waited(&exec, trigger, 5);
    assert!(
        exec.run_texts()
            .iter()
            .any(|text| text == "/permissions session default"),
        "the cleanup line after the working press: {:?}",
        exec.run_texts()
    );
    assert_one_close_tab(&exec);
}

#[test]
fn permission_skips_when_both_key_names_are_rejected() {
    let script = script_2b(&Shapes::default(), Step::Permission).fail(
        FailBuilder::call("pane send-keys", "invalid_key").message("unsupported key\nsecond line"),
    );
    let exec = exec_2b(&script, |_| {}, |term| term);
    assert_eq!(
        exec.kinds(),
        ["startup:pass", "context-empty:pass", "permission:skip"],
        "lines:\n{}",
        exec.lines()
    );
    assert_eq!(
        exec.last_reason().as_deref(),
        Some("Herdr rejected the key names shift+tab and backtab: invalid_key: unsupported key"),
        "the skip names the first line of Herdr's error; lines:\n{}",
        exec.lines()
    );
    assert_one_close_tab(&exec);
}

#[test]
fn model_picker_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "model-picker");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert!(
        exec.typed_texts().iter().any(|text| text == "zzzz"),
        "zzzz is typed without Enter: {:?}",
        exec.typed_texts()
    );
    assert_one_close_tab(&exec);
}

#[test]
fn model_picker_open_missing_fails() {
    let shapes = Shapes {
        model_picker: ModelPicker::OpenMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ModelPicker);
    assert_fails_with(
        &exec,
        "model-picker",
        "/model did not open 'Switch model \u{00b7} this session' within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION]));
    let trigger = last_run_at(&exec, "/model");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn model_picker_rows_missing_fails() {
    let shapes = Shapes {
        model_picker: ModelPicker::RowsMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ModelPicker);
    // The script omits only the footer line, so the reason names only it.
    assert_fails_with(
        &exec,
        "model-picker",
        "the model picker lacks: effort footer",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION]));
    assert_one_close_tab(&exec);
}

#[test]
fn model_picker_no_match_missing_fails() {
    let shapes = Shapes {
        model_picker: ModelPicker::NoMatchMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ModelPicker);
    assert_fails_with(
        &exec,
        "model-picker",
        "typing zzzz did not show 'No model matches \"zzzz\".' within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION]));
    let trigger = last_typed_at(&exec, "zzzz");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn model_picker_esc_steps_fails() {
    let shapes = Shapes {
        model_picker: ModelPicker::EscSteps,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ModelPicker);
    assert_fails_with(
        &exec,
        "model-picker",
        "Esc did not step back: the first Esc must restore the list and the second must close the picker",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, &["esc"]]));
    assert_one_close_tab(&exec);
}

#[test]
fn model_refusal_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "model-refusal");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn model_refusal_refusal_missing_fails() {
    let shapes = Shapes {
        model_refusal: ModelRefusal::RefusalMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ModelRefusal);
    assert_fails_with(
        &exec,
        "model-refusal",
        "the refusal for an unknown model did not appear within 10 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER]));
    let trigger = last_run_at(&exec, "no-such-model-xyz");
    assert_waited(&exec, trigger, 10);
    assert_one_close_tab(&exec);
}

#[test]
fn model_refusal_model_changed_fails() {
    let shapes = Shapes {
        model_refusal: ModelRefusal::ModelChanged,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ModelRefusal);
    assert_fails_with(
        &exec,
        "model-refusal",
        "the status line no longer shows minimax-m3.1-flash after the refused switch",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER]));
    assert_one_close_tab(&exec);
}

#[test]
fn rewind_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "rewind");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert!(
        exec.typed_texts().iter().any(|text| text == "c"),
        "c is typed without Enter: {:?}",
        exec.typed_texts()
    );
    assert_one_close_tab(&exec);
}

#[test]
fn rewind_reply_timeout_fails() {
    let shapes = Shapes {
        rewind: Rewind::ReplyTimeout,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Rewind);
    assert_fails_with(
        &exec,
        "rewind",
        "a reply to a code word prompt did not finish within 90 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER]));
    let trigger = last_run_at(&exec, "Remember the code word heron");
    assert_waited(&exec, trigger, 90);
    assert_one_close_tab(&exec);
}

#[test]
fn rewind_overlay_missing_fails() {
    let shapes = Shapes {
        rewind: Rewind::OverlayMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Rewind);
    assert_fails_with(
        &exec,
        "rewind",
        "Esc twice did not open 'Rewind \u{00b7} choose a prompt to go back to' within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, &["esc", "esc"]]));
    let trigger = last_keys_at(&exec, "esc");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn rewind_list_rows_fails() {
    let shapes = Shapes {
        rewind: Rewind::ListRows,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Rewind);
    assert_fails_with(
        &exec,
        "rewind",
        "the rewind list does not show both prompts #2 and #1",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, &["esc", "esc"]]));
    assert_one_close_tab(&exec);
}

#[test]
fn rewind_rewound_missing_fails() {
    let shapes = Shapes {
        rewind: Rewind::RewoundMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Rewind);
    assert_fails_with(
        &exec,
        "rewind",
        "no line starting '\u{2500}\u{2500} Rewound to before prompt #2' appeared within 10 s",
    );
    assert_keys_in_order(
        &exec,
        &keys(&[K_PERMISSION, K_PICKER, &["esc", "esc", "enter", "enter"]]),
    );
    let trigger = last_keys_at(&exec, "enter");
    assert_waited(&exec, trigger, 10);
    assert_one_close_tab(&exec);
}

#[test]
fn rewind_composer_not_restored_fails() {
    let shapes = Shapes {
        rewind: Rewind::ComposerNotRestored,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Rewind);
    assert_fails_with(
        &exec,
        "rewind",
        "the composer does not hold the second prompt after the rewind",
    );
    assert_keys_in_order(
        &exec,
        &keys(&[K_PERMISSION, K_PICKER, &["esc", "esc", "enter", "enter"]]),
    );
    let trigger = last_keys_at(&exec, "enter");
    assert_waited(&exec, trigger, 3);
    assert_one_close_tab(&exec);
}

#[test]
fn rewind_wrong_code_word_fails() {
    let shapes = Shapes {
        rewind: Rewind::WrongCodeWord,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Rewind);
    assert_fails_with(
        &exec,
        "rewind",
        "the reply after the rewind did not name heron, or named lark",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn rewind_overlay_left_text_fails() {
    let shapes = Shapes {
        rewind: Rewind::OverlayLeftText,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Rewind);
    assert_fails_with(
        &exec,
        "rewind",
        "the rewind overlay left its text in the scrollback",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn plain_answer_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "plain-answer");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert!(
        exec.run_texts()
            .iter()
            .any(|text| text.contains("join these two words")),
        "the token prompt is submitted: {:?}",
        exec.run_texts()
    );
    assert_one_close_tab(&exec);
}

#[test]
fn steer_idle_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "steer-idle");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn steer_idle_notice_missing_fails() {
    let shapes = Shapes {
        steer_idle: SteerIdle::NoticeMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::SteerIdle);
    assert_fails_with(
        &exec,
        "steer-idle",
        "/steer on an idle session did not say 'No turn is running, so this was sent as a new prompt' within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/steer");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn steer_idle_reply_missing_fails() {
    let shapes = Shapes {
        steer_idle: SteerIdle::ReplyMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::SteerIdle);
    assert_fails_with(
        &exec,
        "steer-idle",
        "the prompt sent by /steer did not get an 'IDLE' reply within 90 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let steers = exec
        .run_texts()
        .into_iter()
        .filter(|text| text.starts_with("/steer "))
        .collect::<Vec<_>>();
    assert_eq!(
        steers.len(),
        2,
        "the miss repeats the submit once: {steers:?}"
    );
    let trigger = last_run_at(&exec, "/steer");
    assert_waited(&exec, trigger, 90);
    assert_one_close_tab(&exec);
}

#[test]
fn context_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "context");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn context_header_missing_fails() {
    let shapes = Shapes {
        context: Context::HeaderMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Context);
    assert_fails_with(
        &exec,
        "context",
        "/context did not show a header 'Context \u{00b7} minimax-m3.1-flash \u{00b7} ' ending 'tokens (<n>%)' within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/context");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn context_rows_missing_fails() {
    let shapes = Shapes {
        context: Context::RowsMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Context);
    // The script omits only the source line, so the reason names only it.
    assert_fails_with(&exec, "context", "/context lacks: Source line");
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn context_usage_missing_fails() {
    let shapes = Shapes {
        context: Context::UsageMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Context);
    assert_fails_with(
        &exec,
        "context",
        "/usage did not show 'Usage \u{00b7} this session' with a 'Tokens' row and a 'Cost' row within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/usage");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn context_aggressive_missing_fails() {
    let shapes = Shapes {
        context: Context::AggressiveMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Context);
    assert_fails_with(
        &exec,
        "context",
        "/compact --aggressive did not say 'Hermes does not support --aggressive. Nothing was changed.' within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/compact --aggressive");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn context_argument_missing_fails() {
    let shapes = Shapes {
        context: Context::ArgumentMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Context);
    assert_fails_with(
        &exec,
        "context",
        "/context now did not say '/context does not take an argument.' within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/context now");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn context_compressed_appeared_fails() {
    let shapes = Shapes {
        context: Context::CompressedAppeared,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Context);
    assert_fails_with(
        &exec,
        "context",
        "a '\u{2713} Compressed' line appeared: a refused command changed the conversation",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn hermes_commands_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "hermes-commands");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn hermes_commands_fast_missing_fails() {
    let shapes = Shapes {
        hermes_commands: HermesCommands::FastMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::HermesCommands);
    assert_fails_with(
        &exec,
        "hermes-commands",
        "/fast did not show its refusal '\u{26a1} /fast is only available for OpenAI models that support Priority Processing.' within 10 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/fast");
    assert_waited(&exec, trigger, 10);
    assert_one_close_tab(&exec);
}

#[test]
fn hermes_commands_loop_missing_fails() {
    let shapes = Shapes {
        hermes_commands: HermesCommands::LoopMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::HermesCommands);
    assert_fails_with(
        &exec,
        "hermes-commands",
        "/loop 45m did not show '/loop: missing prompt (usage: /loop [interval] <prompt>)' within 10 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/loop 45m");
    assert_waited(&exec, trigger, 10);
    assert_one_close_tab(&exec);
}

#[test]
fn hermes_commands_loop_line_fails() {
    let shapes = Shapes {
        hermes_commands: HermesCommands::LoopLine,
        ..Default::default()
    };
    let exec = play(&shapes, Step::HermesCommands);
    assert_fails_with(
        &exec,
        "hermes-commands",
        "a loop line appeared after /loop 45m with no prompt",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn rename_export_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "rename-export");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert!(
        !exec.report.scratch.exists(),
        "scratch is removed after the step checks its files"
    );
    assert_one_close_tab(&exec);
}

#[test]
fn rename_export_renamed_missing_fails() {
    let shapes = Shapes {
        rename_export: RenameExport::RenamedMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::RenameExport);
    assert_fails_with(
        &exec,
        "rename-export",
        "/rename did not answer 'Renamed to \"<title>\".' within 10 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/rename fixture smoke");
    assert_waited(&exec, trigger, 10);
    assert_one_close_tab(&exec);
}

#[test]
fn rename_export_title_missing_fails() {
    let shapes = Shapes {
        rename_export: RenameExport::TitleMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::RenameExport);
    assert_fails_with(
        &exec,
        "rename-export",
        "/rename with no title did not say 'This session is called \"<title>\".' within 10 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/rename");
    assert_waited(&exec, trigger, 10);
    assert_one_close_tab(&exec);
}

#[test]
fn rename_export_exported_missing_fails() {
    let shapes = Shapes {
        rename_export: RenameExport::ExportedMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::RenameExport);
    assert_fails_with(
        &exec,
        "rename-export",
        "/export did not print 'Exported \"<title>\" to ./<name>' within 15 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/export");
    assert_waited(&exec, trigger, 15);
    assert_one_close_tab(&exec);
}

#[test]
fn rename_export_file_bad_fails() {
    let shapes = Shapes {
        rename_export: RenameExport::FileBad,
        ..Default::default()
    };
    let exec = play(&shapes, Step::RenameExport);
    assert_fails_with(
        &exec,
        "rename-export",
        "the exported file is missing, is not mode 0600, or lacks the plain-answer prompt",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn rename_export_bad_name_missing_fails() {
    let shapes = Shapes {
        rename_export: RenameExport::BadNameMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::RenameExport);
    assert_fails_with(
        &exec,
        "rename-export",
        "/export ./notes.txt did not say 'Use a file name ending in .md, .json or .html.' within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/export ./notes.txt");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn rename_export_bad_name_wrote_fails() {
    let shapes = Shapes {
        rename_export: RenameExport::BadNameWrote,
        ..Default::default()
    };
    let exec = play(&shapes, Step::RenameExport);
    assert_fails_with(&exec, "rename-export", "/export ./notes.txt created a file");
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn queue_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "queue");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_eq!(
        submit_times(&exec, "sleep 25").len(),
        1,
        "the hit needs no repeat of the long prompt"
    );
    assert_eq!(
        submit_times(&exec, "Reply with the word omega").len(),
        1,
        "the hit needs no repeat of the omega prompt"
    );
    assert_one_close_tab(&exec);
}

#[test]
fn queue_indicator_missing_fails() {
    let shapes = Shapes {
        queue: Queue::IndicatorMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Queue);
    assert_fails_with(
        &exec,
        "queue",
        "'queued (1)' and the preview 'Reply with the word omega' did not appear within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_eq!(
        submit_times(&exec, "sleep 25").len(),
        2,
        "the miss repeats the long prompt once"
    );
    let trigger = last_run_at(&exec, "Reply with the word omega");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn queue_preview_twice_fails() {
    let shapes = Shapes {
        queue: Queue::PreviewTwice,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Queue);
    assert_fails_with(
        &exec,
        "queue",
        "the queued prompt shows more than once on screen before it ran",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn queue_first_missing_fails() {
    let shapes = Shapes {
        queue: Queue::FirstMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Queue);
    assert_fails_with(
        &exec,
        "queue",
        "the first reply 'FIRST' did not appear within 120 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "Reply with the word omega");
    assert_waited(&exec, trigger, 120);
    assert_one_close_tab(&exec);
}

#[test]
fn queue_omega_missing_fails() {
    let shapes = Shapes {
        queue: Queue::OmegaMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Queue);
    assert_fails_with(
        &exec,
        "queue",
        "the queued prompt did not run: 'OMEGA' did not appear within 120 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "Reply with the word omega");
    assert_waited(&exec, trigger, 120);
    assert_one_close_tab(&exec);
}

#[test]
fn queue_order_wrong_fails() {
    let shapes = Shapes {
        queue: Queue::OrderWrong,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Queue);
    assert_fails_with(
        &exec,
        "queue",
        "the order is wrong: the omega prompt and OMEGA must come after FIRST",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn queue_indicator_left_fails() {
    let shapes = Shapes {
        queue: Queue::IndicatorLeft,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Queue);
    assert_fails_with(
        &exec,
        "queue",
        "an indicator 'queued (' is still shown after the queue drained",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn cd_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "cd");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn cd_moved_missing_fails() {
    let shapes = Shapes {
        cd: Cd::MovedMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Cd);
    assert_fails_with(
        &exec,
        "cd",
        "/cd ../b did not print 'This session now works in' with a path ending '/b (main)' within 15 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/cd ../b");
    assert_waited(&exec, trigger, 15);
    assert_one_close_tab(&exec);
}

#[test]
fn cd_segment_wrong_fails() {
    let shapes = Shapes {
        cd: Cd::SegmentWrong,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Cd);
    assert_fails_with(
        &exec,
        "cd",
        "the status line folder segment does not end with 'b (main)'",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn cd_show_missing_fails() {
    let shapes = Shapes {
        cd: Cd::ShowMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Cd);
    assert_fails_with(
        &exec,
        "cd",
        "/cd with no argument did not print 'This session works in' within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/cd");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn cd_missing_folder_fails() {
    let shapes = Shapes {
        cd: Cd::MissingFolder,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Cd);
    assert_fails_with(
        &exec,
        "cd",
        "/cd ./missing-folder-smoke did not print 'No folder at' within 5 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "/cd ./missing-folder-smoke");
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn quit_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "quit");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn quit_token_missing_fails() {
    let shapes = Shapes {
        quit: Quit::TokenMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Quit);
    assert_fails_with(
        &exec,
        "quit",
        "committed output vanished on exit (token A missing)",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn headless_passes() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_passes(&exec, "headless");
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert!(
        !exec.report.scratch.exists(),
        "scratch is removed after the step checks its files"
    );
    assert_one_close_tab(&exec);
}

#[test]
fn headless_text_run_fails() {
    let shapes = Shapes {
        headless: Headless::TextRun,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Headless);
    assert_fails_with(
        &exec,
        "headless",
        "the plain headless run did not print 5555, exit 0 and leave standard error empty (exit 7)",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn headless_no_gateway_json_fails() {
    let shapes = Shapes {
        headless: Headless::NoGatewayJson,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Headless);
    assert_fails_with(
        &exec,
        "headless",
        "the headless run without a gateway did not print one JSON line with \"status\":\"not_started\" and \"exit_code\":4, and exit 4",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn headless_no_gateway_text_fails() {
    let shapes = Shapes {
        headless: Headless::NoGatewayText,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Headless);
    assert_fails_with(
        &exec,
        "headless",
        "the headless run without a gateway did not print 'talaria: no Hermes gateway is running for this home' on standard error",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn headless_command_timeout_fails() {
    let shapes = Shapes {
        headless: Headless::CommandTimeout,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Headless);
    assert_fails_with(
        &exec,
        "headless",
        "a headless command did not finish within 90 s",
    );
    assert_keys_in_order(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    let trigger = last_run_at(&exec, "1234 plus 4321");
    assert_waited(&exec, trigger, 90);
    assert_one_close_tab(&exec);
}

// ---- the run as a whole ----

#[test]
fn a_failing_step_stops_the_run_and_cleans_up() {
    let shapes = Shapes {
        context_empty: ContextEmpty::ContextLineMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Headless);
    assert_fails_with(
        &exec,
        "context-empty",
        "/context did not show 'Context \u{00b7} not measured yet' within 5 s",
    );
    assert_eq!(
        exec.kinds(),
        ["startup:pass", "context-empty:fail"],
        "later steps print nothing"
    );
    assert_one_close_tab(&exec);
    assert_eq!(exec.report.exit_code(), 1);

    let script = script_2b(&Shapes::default(), Step::ContextEmpty)
        .fail(FailBuilder::call("pane read", "read_gone").message("gone"));
    let exec = exec_2b(&script, |_| {}, |term| term);
    assert_fails_with(&exec, "startup", "read_gone: gone");
    assert_one_close_tab(&exec);
}

#[test]
fn deadline_names_the_step() {
    let shapes = Shapes {
        rewind: Rewind::ReplyTimeout,
        ..Default::default()
    };
    let exec = play_cfg(&shapes, Step::Rewind, |cfg| {
        cfg.step_scale = 100.0;
    });
    assert_fails_with(&exec, "rewind", "deadline exceeded in step rewind");
    assert!(
        exec.last_at() >= Duration::from_secs(1500),
        "the run reached the 1500 s deadline"
    );
    assert_one_close_tab(&exec);
}

#[test]
fn a_skip_is_never_a_pass() {
    let script = script_2b(&Shapes::default(), Step::Permission).fail(
        FailBuilder::call("pane send-keys", "invalid_key").message("unsupported key\nsecond line"),
    );
    let exec = exec_2b(&script, |_| {}, |term| term);
    assert_eq!(
        exec.kinds(),
        ["startup:pass", "context-empty:pass", "permission:skip"],
        "lines:\n{}",
        exec.lines()
    );
    assert!(
        exec.kinds().iter().all(|kind| !kind.ends_with(":fail")),
        "nothing fails: {:?}",
        exec.kinds()
    );
    assert_eq!(
        exec.report.result_line(),
        "SMOKE_RESULT: PASSED WITH SKIPS (permission)"
    );
    assert_eq!(exec.report.exit_code(), 3);
    assert_one_close_tab(&exec);
}

#[test]
fn live_require_turns_a_skip_into_a_failure() {
    let script = script_2b(&Shapes::default(), Step::Permission).fail(
        FailBuilder::call("pane send-keys", "invalid_key").message("unsupported key\nsecond line"),
    );
    let exec = exec_2b(&script, |cfg| cfg.require = true, |term| term);
    let line = "FAIL permission (skipped: Herdr rejected the key names shift+tab and backtab: invalid_key: unsupported key; TALARIA_LIVE_REQUIRE=1)";
    assert!(
        exec.lines().contains(line),
        "the skip fails under the live requirement; lines:\n{}",
        exec.lines()
    );
    assert_eq!(exec.report.exit_code(), 1);
    assert!(
        exec.kinds()
            .last()
            .is_some_and(|kind| kind == "permission:fail"),
        "the run stops at the failure: {:?}",
        exec.kinds()
    );
    assert_one_close_tab(&exec);
}

#[test]
fn step_scale_leaves_quiet_periods() {
    let exec = play_cfg(&Shapes::default(), Step::PlainAnswer, |cfg| {
        cfg.step_scale = 0.5;
    });
    assert_passes(&exec, "plain-answer");
    assert!(
        pass_elapsed(&exec, "plain-answer") >= Duration::from_secs(3),
        "the 3 s settle is not scaled"
    );
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        context_empty: ContextEmpty::ContextLineMissing,
        ..Default::default()
    };
    let exec = play_cfg(&shapes, Step::ContextEmpty, |cfg| {
        cfg.step_scale = 0.5;
    });
    assert_fails_with(
        &exec,
        "context-empty",
        "/context did not show 'Context \u{00b7} not measured yet' within 3 s",
    );
    assert_one_close_tab(&exec);
}

#[test]
fn steer_idle_and_queue_retry_once_then_fail() {
    // The steer miss-then-hit passes: the repeat resubmits only after the 90 s wait.
    let shapes = Shapes {
        steer_idle: SteerIdle::MissThenHit,
        ..Default::default()
    };
    let exec = play(&shapes, Step::SteerIdle);
    assert_passes(&exec, "steer-idle");
    let steers = submit_times(&exec, "/steer ");
    assert_eq!(steers.len(), 2, "one repeat");
    assert!(
        steers[1] - steers[0] >= Duration::from_secs(90),
        "the repeat waits out the first 90 s: {steers:?}"
    );
    assert_one_close_tab(&exec);

    // Two steer misses fail with the reply reason.
    let shapes = Shapes {
        steer_idle: SteerIdle::ReplyMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::SteerIdle);
    assert_fails_with(
        &exec,
        "steer-idle",
        "the prompt sent by /steer did not get an 'IDLE' reply within 90 s",
    );
    assert_eq!(submit_times(&exec, "/steer ").len(), 2, "one repeat");
    assert_one_close_tab(&exec);

    // A steer miss while a turn still works resubmits only after the settle.
    let shapes = Shapes {
        steer_idle: SteerIdle::MissWhileWorking,
        ..Default::default()
    };
    let exec = play(&shapes, Step::SteerIdle);
    assert_passes(&exec, "steer-idle");
    let steers = submit_times(&exec, "/steer ");
    assert_eq!(steers.len(), 2, "one repeat");
    assert!(
        steers[1] - steers[0] >= Duration::from_secs(100),
        "the repeat waits past the 90 s miss for the working turn: {steers:?}"
    );
    assert_one_close_tab(&exec);

    // The queue miss-then-hit passes: the first attempt answers in the wrong order, so
    // the pass proves the repeat reads past the last echo of its own long prompt.
    let shapes = Shapes {
        queue: Queue::MissThenHit,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Queue);
    assert_passes(&exec, "queue");
    let longs = submit_times(&exec, "sleep 25");
    assert_eq!(longs.len(), 2, "one repeat of the long prompt");
    assert!(
        longs[1] - longs[0] >= Duration::from_secs(5),
        "the repeat waits out the first 5 s: {longs:?}"
    );
    assert_eq!(
        submit_times(&exec, "Reply with the word omega").len(),
        2,
        "one repeat of the omega prompt"
    );
    assert_one_close_tab(&exec);

    // Two queue misses fail with the indicator reason.
    let shapes = Shapes {
        queue: Queue::TwoMisses,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Queue);
    assert_fails_with(
        &exec,
        "queue",
        "'queued (1)' and the preview 'Reply with the word omega' did not appear within 5 s",
    );
    assert_eq!(submit_times(&exec, "sleep 25").len(), 2, "one repeat");
    assert_one_close_tab(&exec);

    // A queue miss while a turn still works resubmits only after the settle.
    let shapes = Shapes {
        queue: Queue::MissWhileWorking,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Queue);
    assert_passes(&exec, "queue");
    let longs = submit_times(&exec, "sleep 25");
    assert_eq!(longs.len(), 2, "one repeat");
    assert!(
        longs[1] - longs[0] >= Duration::from_secs(10),
        "the repeat waits past the 5 s miss for the working turn: {longs:?}"
    );
    assert_one_close_tab(&exec);
}

#[test]
fn steps_run_in_the_documented_order() {
    let exec = play(&Shapes::default(), Step::Headless);
    assert_eq!(
        exec.kinds(),
        [
            "startup:pass",
            "context-empty:pass",
            "permission:pass",
            "model-picker:pass",
            "model-refusal:pass",
            "rewind:pass",
            "plain-answer:pass",
            "steer-idle:pass",
            "context:pass",
            "hermes-commands:pass",
            "rename-export:pass",
            "queue:pass",
            "cd:pass",
            "quit:pass",
            "headless:pass",
        ],
        "lines:\n{}",
        exec.lines()
    );
    assert_eq!(
        exec.report.result_line(),
        format!(
            "SMOKE_RESULT: PASSED (elapsed {} s)",
            exec.report.elapsed.as_secs()
        )
    );
    assert_eq!(exec.report.exit_code(), 0);
    assert_keys_exact(&exec, &keys(&[K_PERMISSION, K_PICKER, K_REWIND]));
    assert_one_close_tab(&exec);
}

#[test]
fn shared_steps_behave_as_in_phase_1() {
    let exec = play(&Shapes::default(), Step::Headless);
    let launch = exec
        .run_texts()
        .into_iter()
        .next()
        .expect("the launch line");
    for flag in [
        "--no-ensure",
        "--profile fixture",
        "--model minimax-m3.1-flash",
        "--provider ollama-cloud",
    ] {
        assert!(launch.contains(flag), "the launch carries {flag}: {launch}");
    }
    assert!(
        exec.run_texts()
            .iter()
            .any(|text| text.contains("join these two words")),
        "plain-answer submits the token prompt"
    );
    assert!(
        exec.run_texts().iter().any(|text| text == "/quit"),
        "quit sends /quit as a run"
    );
    assert_passes(&exec, "startup");
    assert_passes(&exec, "plain-answer");
    assert_passes(&exec, "quit");
    assert_one_close_tab(&exec);
}
