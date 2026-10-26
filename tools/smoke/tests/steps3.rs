//! The phase 3 run on the fake terminal and the manual clock.
//!
//! Each passing script runs the whole suite and asserts that step passed and one `close_tab`.
//! Each failing script asserts the printed reason exactly and one `close_tab`. A timeout
//! failure also shows the run waited at least the limit named in the reason. All of these
//! fail until the stub `run_smoke_3` is replaced, except `step_limits_sum_below_the_deadline`,
//! which fails because the phase 3 deadline default is still the first suite's 300 s.

mod support;

use std::time::Duration;

use smoke::{STEP_LIMITS, STEP_ORDER, SmokeConfig, Suite};
use support::scripts3::{
    Attach, BgLaunch, Exec3, FleetList, FleetQuiet, FleetView, LocalCommands, NeedsYou, OpenInPane,
    Shapes, Step, WorktreeLaunch, assert_fails_with, assert_one_close_tab, assert_passes, exec_3,
    script_3,
};
use support::{Call, FailBuilder};

fn play(shapes: &Shapes, stop: Step) -> Exec3 {
    let script = script_3(shapes, stop);
    exec_3(&script, |_| {}, |term| term)
}

fn play_cfg(shapes: &Shapes, stop: Step, tune: impl FnOnce(&mut SmokeConfig)) -> Exec3 {
    let script = script_3(shapes, stop);
    exec_3(&script, tune, |term| term)
}

fn full() -> Exec3 {
    play(&Shapes::default(), Step::NeedsYou)
}

fn passes(step: &str) {
    let exec = full();
    assert_passes(&exec, step);
    assert_eq!(exec.report.exit_code(), 0, "lines:\n{}", exec.lines());
    assert_one_close_tab(&exec);
}

fn fails(shapes: Shapes, stop: Step, step: &str, reason: &str) {
    let exec = play(&shapes, stop);
    assert_fails_with(&exec, step, reason);
    assert_one_close_tab(&exec);
}

fn last_call_at(exec: &Exec3, want: impl Fn(&Call) -> bool) -> Duration {
    exec.term
        .calls()
        .into_iter()
        .rev()
        .find_map(|recorded| want(&recorded.call).then_some(recorded.at))
        .expect("the trigger call")
}

fn last_run_at(exec: &Exec3, needle: &str) -> Duration {
    last_call_at(
        exec,
        |call| matches!(call, Call::Run { text, .. } if text.contains(needle)),
    )
}

fn last_keys_at(exec: &Exec3, key: &str) -> Duration {
    last_call_at(
        exec,
        |call| matches!(call, Call::SendKeys { keys, .. } if keys.iter().any(|got| got == key)),
    )
}

fn last_typed_at(exec: &Exec3, needle: &str) -> Duration {
    last_call_at(
        exec,
        |call| matches!(call, Call::SendText { text, .. } if text.contains(needle)),
    )
}

fn assert_waited(exec: &Exec3, trigger: Duration, limit: u64) {
    assert!(
        exec.last_at() >= trigger + Duration::from_secs(limit),
        "the step waited at least {limit} s; lines:\n{}",
        exec.lines()
    );
}

fn fails_after(
    shapes: Shapes,
    stop: Step,
    step: &str,
    reason: &str,
    trigger: impl Fn(&Exec3) -> Duration,
    limit: u64,
) {
    let exec = play(&shapes, stop);
    assert_fails_with(&exec, step, reason);
    assert_waited(&exec, trigger(&exec), limit);
    assert_one_close_tab(&exec);
}

/// The run id taken from a sent line, after `marker`.
fn named_id(exec: &Exec3, marker: &str) -> String {
    exec.run_texts()
        .into_iter()
        .find_map(|text| {
            let start = text.find(marker)? + marker.len();
            let rest = &text[start..];
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric())
                .unwrap_or(rest.len());
            Some(rest[..end].to_owned())
        })
        .unwrap_or_else(|| panic!("no {marker} in the sent lines:\n{}", exec.lines()))
}

fn run_times(exec: &Exec3, needle: &str) -> Vec<Duration> {
    exec.term
        .calls()
        .into_iter()
        .filter_map(|recorded| match recorded.call {
            Call::Run { text, .. } if text.contains(needle) => Some(recorded.at),
            _ => None,
        })
        .collect()
}

const QUIET_EXIT: &str =
    "talaria fleet list exited with code 7 (is the Hermes gateway running for profile fixture?)";
const NARROW: &str = "the smoke tab is 80 columns by 20 rows; phase 3 needs at least 100 by 24";
const BUSY: &str = "the fleet is not quiet (NEEDS YOU (1), WORKING (2)); answer or stop those sessions and run again";
const BG_EXIT: &str = "talaria --bg ended with exit code 1, expected 0";
const BG_OUTPUT: &str =
    "the output was not exactly one line with \"status\":\"launched\" and a session_id";
const BG_SLOW: &str = "talaria --bg took 15 s to return, over the 15 s limit";
const BG_REFUSAL_EXIT: &str = "talaria --bg -p ended with exit code 1, expected 2";
const BG_REFUSAL_TEXT: &str = "talaria --bg -p printed on standard output or did not say \"talaria --bg cannot be combined with -p.\" on standard error";
const WT_EXIT: &str = "talaria --bg --worktree ended with exit code 1, expected 0";
const NOT_FRESH: &str = "the worktree started at \"init\", not at the commit \"two\" that origin had and the checkout did not";
const DIRTY: &str = "the main checkout is no longer clean:  M README";
const LIST_EXIT: &str = "talaria fleet list --json ended with exit code 1, expected 0";
const NEVER_DONE: &str =
    "session local-a1 was not listed as done or idle within 120 s (last state: working)";
const WT_MISSING: &str = "the worktree session local-b2 is not in the fleet list";
const UNSERVED: &str = "--profile nosuch did not exit 2 with the sentence \"profile \"nosuch\" is not served by this gateway\"";
const NO_GATEWAY: &str = "with an empty Hermes home, talaria fleet list did not exit 1 with \"no Hermes gateway is running for this Hermes home\"";
const STARTED: &str = "talaria fleet list wrote into the empty Hermes home (it must start nothing)";
const NO_RESUMED: &str = "talaria attach a1b2c3d4 did not show \"\u{2500}\u{2500} Resumed:\" and the session's first prompt within 15 s";
const AMBIGUOUS: &str = "talaria attach a1b2c3d4 matched several sessions (exit 2): talaria: 2 sessions match \"local-a1b2c3d4\":";
const ATTACH_EXIT: &str =
    "/quit in the attached session did not end talaria with exit code 0 within 15 s";
const TASKS: &str = "/tasks did not say \"No subagents in this session yet.\" within 5 s";
const AGENTS: &str = "/agents frobnicate did not print the unknown-option sentence within 5 s";
const PROCESSES: &str =
    "/processes did not say \"No background processes in this session.\" within 5 s";
const BG_USAGE: &str = "/bg alone did not print its usage line, or started a task";
const FLEET_OPEN: &str =
    "/fleet did not open an overlay with this session's row marked \"(here)\" within 5 s";
const FLEET_FILTER: &str =
    "filtering for zzz-no-match did not say \"No sessions match\" within 5 s";
const FLEET_CLOSE: &str = "Ctrl+C did not close the overlay within 5 s";
const OPEN_NOTICE: &str = "no line \"Opened \"<title>\" in a new Herdr pane.\" within 10 s";
const NOT_IN_HERDR: &str = "talaria did not detect Herdr and printed: Not inside Herdr or tmux";
const NY_LAUNCH: &str =
    "the needs-you launch ended with exit code 1 or printed no single \"status\":\"launched\" line";
const NY_NEVER: &str = "no approval request appeared in the fleet list within 120 s on two tries (session state: working)";
const NY_OLD: &str =
    "the request is 151 s old, too old to answer before Hermes denies it at 300 s; run again";
const NY_NOTICE: &str = "the start-up notice \"1 background session needs you; press alt+a to see it.\" did not appear within 20 s";
const NY_SEGMENT: &str = "the status line did not show \"\u{2691} 1 needs you\" within 20 s";
const NY_PEEK: &str = "alt+a did not open the fleet view on the request (no \"rm -rf ./talaria-smoke-probe\") within 5 s";
const NY_DENY: &str = "pressing a did not show a line \"<n>. deny\" within 5 s";
const NY_CLEARED: &str = "the request was still listed 10 s after deny";
const NY_RAN: &str =
    "the folder talaria-smoke-probe is gone: the command ran although it was denied";
const NY_FLAG: &str = "the status line still showed \"\u{2691}\" after the overlay closed";
const NY_SKIP: &str = "no approval card was asked for (is approvals.mode manual on the profile?)";

#[test]
fn fleet_quiet_passes() {
    passes("fleet-quiet");
}

#[test]
fn fleet_quiet_exit_fails() {
    fails(
        Shapes {
            fleet_quiet: FleetQuiet::Exit(7),
            ..Default::default()
        },
        Step::FleetQuiet,
        "fleet-quiet",
        QUIET_EXIT,
    );
}

#[test]
fn fleet_quiet_narrow_pane_fails() {
    fails(
        Shapes {
            fleet_quiet: FleetQuiet::Narrow,
            ..Default::default()
        },
        Step::FleetQuiet,
        "fleet-quiet",
        NARROW,
    );
}

#[test]
fn fleet_quiet_busy_fleet_fails() {
    fails(
        Shapes {
            fleet_quiet: FleetQuiet::Busy,
            ..Default::default()
        },
        Step::FleetQuiet,
        "fleet-quiet",
        BUSY,
    );
}

#[test]
fn bg_launch_passes() {
    passes("bg-launch");
}

#[test]
fn bg_launch_exit_fails() {
    fails(
        Shapes {
            bg_launch: BgLaunch::Exit(1),
            ..Default::default()
        },
        Step::BgLaunch,
        "bg-launch",
        BG_EXIT,
    );
}

#[test]
fn bg_launch_output_fails() {
    fails(
        Shapes {
            bg_launch: BgLaunch::Output,
            ..Default::default()
        },
        Step::BgLaunch,
        "bg-launch",
        BG_OUTPUT,
    );
}

#[test]
fn bg_launch_slow_fails() {
    fails_after(
        Shapes {
            bg_launch: BgLaunch::Slow,
            ..Default::default()
        },
        Step::BgLaunch,
        "bg-launch",
        BG_SLOW,
        |exec| last_run_at(exec, "bg1.out"),
        15,
    );
}

#[test]
fn bg_launch_refusal_exit_fails() {
    fails(
        Shapes {
            bg_launch: BgLaunch::RefusalExit(1),
            ..Default::default()
        },
        Step::BgLaunch,
        "bg-launch",
        BG_REFUSAL_EXIT,
    );
}

#[test]
fn bg_launch_refusal_text_fails() {
    fails(
        Shapes {
            bg_launch: BgLaunch::RefusalText,
            ..Default::default()
        },
        Step::BgLaunch,
        "bg-launch",
        BG_REFUSAL_TEXT,
    );
}

#[test]
fn worktree_launch_passes() {
    passes("worktree-launch");
}

#[test]
fn worktree_launch_exit_fails() {
    fails(
        Shapes {
            worktree_launch: WorktreeLaunch::Exit(1),
            ..Default::default()
        },
        Step::WorktreeLaunch,
        "worktree-launch",
        WT_EXIT,
    );
}

#[test]
fn worktree_launch_text_fails() {
    let shapes = Shapes {
        worktree_launch: WorktreeLaunch::Text,
        ..Default::default()
    };
    let exec = play(&shapes, Step::WorktreeLaunch);
    let run_id = named_id(&exec, "--name smoke-");
    assert_fails_with(
        &exec,
        "worktree-launch",
        &format!(
            "the output lacked \"Worktree: branch talaria/smoke-{run_id} from origin/main at\" or \"Launched session\""
        ),
    );
    assert_one_close_tab(&exec);
}

#[test]
fn worktree_launch_tree_missing_fails() {
    let shapes = Shapes {
        worktree_launch: WorktreeLaunch::TreeMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::WorktreeLaunch);
    let run_id = named_id(&exec, "--name smoke-");
    assert_fails_with(
        &exec,
        "worktree-launch",
        &format!("the folder .worktrees/talaria-smoke-{run_id} was not made"),
    );
    assert_one_close_tab(&exec);
}

#[test]
fn worktree_launch_not_fresh_fails() {
    fails(
        Shapes {
            worktree_launch: WorktreeLaunch::NotFresh,
            ..Default::default()
        },
        Step::WorktreeLaunch,
        "worktree-launch",
        NOT_FRESH,
    );
}

#[test]
fn worktree_launch_checkout_dirty_fails() {
    fails(
        Shapes {
            worktree_launch: WorktreeLaunch::CheckoutDirty,
            ..Default::default()
        },
        Step::WorktreeLaunch,
        "worktree-launch",
        DIRTY,
    );
}

#[test]
fn fleet_list_passes() {
    passes("fleet-list");
}

#[test]
fn fleet_list_exit_fails() {
    fails(
        Shapes {
            fleet_list: FleetList::Exit(1),
            ..Default::default()
        },
        Step::FleetList,
        "fleet-list",
        LIST_EXIT,
    );
}

#[test]
fn fleet_list_never_done_fails() {
    fails_after(
        Shapes {
            fleet_list: FleetList::NeverDone,
            ..Default::default()
        },
        Step::FleetList,
        "fleet-list",
        NEVER_DONE,
        |exec| last_run_at(exec, "f.json"),
        120,
    );
}

#[test]
fn fleet_list_worktree_session_missing_fails() {
    fails(
        Shapes {
            fleet_list: FleetList::WorktreeMissing,
            ..Default::default()
        },
        Step::FleetList,
        "fleet-list",
        WT_MISSING,
    );
}

#[test]
fn fleet_list_unserved_profile_fails() {
    fails(
        Shapes {
            fleet_list: FleetList::Unserved,
            ..Default::default()
        },
        Step::FleetList,
        "fleet-list",
        UNSERVED,
    );
}

#[test]
fn fleet_list_no_gateway_fails() {
    fails(
        Shapes {
            fleet_list: FleetList::NoGateway,
            ..Default::default()
        },
        Step::FleetList,
        "fleet-list",
        NO_GATEWAY,
    );
}

#[test]
fn fleet_list_no_gateway_started_fails() {
    fails(
        Shapes {
            fleet_list: FleetList::NoGatewayStarted,
            ..Default::default()
        },
        Step::FleetList,
        "fleet-list",
        STARTED,
    );
}

#[test]
fn attach_passes() {
    passes("attach");
}

#[test]
fn attach_no_resumed_fails() {
    fails_after(
        Shapes {
            attach: Attach::NoResumed,
            ..Default::default()
        },
        Step::Attach,
        "attach",
        NO_RESUMED,
        |exec| last_run_at(exec, "attach --no-ensure"),
        15,
    );
}

#[test]
fn attach_ambiguous_fails() {
    fails(
        Shapes {
            attach: Attach::Ambiguous,
            ..Default::default()
        },
        Step::Attach,
        "attach",
        AMBIGUOUS,
    );
}

#[test]
fn attach_exit_fails() {
    fails_after(
        Shapes {
            attach: Attach::Exit,
            ..Default::default()
        },
        Step::Attach,
        "attach",
        ATTACH_EXIT,
        |exec| last_keys_at(exec, "enter"),
        15,
    );
}

#[test]
fn attach_no_match_fails() {
    let shapes = Shapes {
        attach: Attach::NoMatch,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Attach);
    let run_id = named_id(&exec, "zz-none-");
    assert_fails_with(
        &exec,
        "attach",
        &format!(
            "talaria attach zz-none-{run_id} did not exit 1 with \"talaria: no session matches\""
        ),
    );
    assert_waited(&exec, last_run_at(&exec, "zz-none-"), 20);
    assert_one_close_tab(&exec);
}

#[test]
fn local_commands_passes() {
    passes("local-commands");
}

#[test]
fn local_commands_tasks_fails() {
    fails_after(
        Shapes {
            local_commands: LocalCommands::Tasks,
            ..Default::default()
        },
        Step::LocalCommands,
        "local-commands",
        TASKS,
        |exec| last_run_at(exec, "/tasks"),
        5,
    );
}

#[test]
fn local_commands_agents_option_fails() {
    fails_after(
        Shapes {
            local_commands: LocalCommands::AgentsOption,
            ..Default::default()
        },
        Step::LocalCommands,
        "local-commands",
        AGENTS,
        |exec| last_run_at(exec, "/agents frobnicate"),
        5,
    );
}

#[test]
fn local_commands_processes_fails() {
    fails_after(
        Shapes {
            local_commands: LocalCommands::Processes,
            ..Default::default()
        },
        Step::LocalCommands,
        "local-commands",
        PROCESSES,
        |exec| last_run_at(exec, "/processes"),
        5,
    );
}

#[test]
fn local_commands_bg_usage_fails() {
    fails_after(
        Shapes {
            local_commands: LocalCommands::BgUsage,
            ..Default::default()
        },
        Step::LocalCommands,
        "local-commands",
        BG_USAGE,
        |exec| last_run_at(exec, "/bg"),
        5,
    );
}

#[test]
fn fleet_view_passes() {
    passes("fleet-view");
}

#[test]
fn fleet_view_open_fails() {
    fails_after(
        Shapes {
            fleet_view: FleetView::Open,
            ..Default::default()
        },
        Step::FleetView,
        "fleet-view",
        FLEET_OPEN,
        |exec| last_run_at(exec, "/fleet"),
        5,
    );
}

#[test]
fn fleet_view_no_match_fails() {
    fails_after(
        Shapes {
            fleet_view: FleetView::NoMatch,
            ..Default::default()
        },
        Step::FleetView,
        "fleet-view",
        FLEET_FILTER,
        |exec| last_typed_at(exec, "zzz-no-match"),
        5,
    );
}

#[test]
fn fleet_view_close_fails() {
    fails_after(
        Shapes {
            fleet_view: FleetView::Close,
            ..Default::default()
        },
        Step::FleetView,
        "fleet-view",
        FLEET_CLOSE,
        |exec| last_keys_at(exec, "ctrl+c"),
        5,
    );
}

#[test]
fn open_in_pane_passes() {
    passes("open-in-pane");
}

#[test]
fn open_in_pane_notice_fails() {
    fails_after(
        Shapes {
            open_in_pane: OpenInPane::Notice,
            ..Default::default()
        },
        Step::OpenInPane,
        "open-in-pane",
        OPEN_NOTICE,
        |exec| last_typed_at(exec, "o"),
        10,
    );
}

#[test]
fn open_in_pane_not_in_herdr_fails() {
    fails(
        Shapes {
            open_in_pane: OpenInPane::NotInHerdr,
            ..Default::default()
        },
        Step::OpenInPane,
        "open-in-pane",
        NOT_IN_HERDR,
    );
}

#[test]
fn needs_you_passes() {
    use smoke::{KEY_JUMP, KEY_PUSH, KEY_STOP, KEY_SUBAGENTS, KEY_SUBMIT_FALLBACK};
    let exec = full();
    assert_passes(&exec, "needs-you");
    let keys = exec.sent_keys();
    assert!(
        keys.iter().any(|key| key == KEY_JUMP),
        "alt+a is sent: {keys:?}"
    );
    for forbidden in [KEY_SUBAGENTS, KEY_PUSH, KEY_STOP, KEY_SUBMIT_FALLBACK] {
        assert!(
            !keys.iter().any(|key| key == forbidden),
            "{forbidden} is recorded and not sent: {keys:?}"
        );
    }
    assert_one_close_tab(&exec);
}

#[test]
fn needs_you_launch_fails() {
    fails(
        Shapes {
            needs_you: NeedsYou::Launch,
            ..Default::default()
        },
        Step::NeedsYou,
        "needs-you",
        NY_LAUNCH,
    );
}

#[test]
fn needs_you_never_listed_fails() {
    fails(
        Shapes {
            needs_you: NeedsYou::NeverListed,
            ..Default::default()
        },
        Step::NeedsYou,
        "needs-you",
        NY_NEVER,
    );
}

#[test]
fn needs_you_too_old_fails() {
    fails(
        Shapes {
            needs_you: NeedsYou::TooOld,
            ..Default::default()
        },
        Step::NeedsYou,
        "needs-you",
        NY_OLD,
    );
}

#[test]
fn needs_you_notice_fails() {
    fails_after(
        Shapes {
            needs_you: NeedsYou::Notice,
            ..Default::default()
        },
        Step::NeedsYou,
        "needs-you",
        NY_NOTICE,
        |exec| last_run_at(exec, "--provider ollama-cloud;"),
        20,
    );
}

#[test]
fn needs_you_segment_fails() {
    fails_after(
        Shapes {
            needs_you: NeedsYou::Segment,
            ..Default::default()
        },
        Step::NeedsYou,
        "needs-you",
        NY_SEGMENT,
        |exec| last_run_at(exec, "--provider ollama-cloud;"),
        20,
    );
}

#[test]
fn needs_you_peek_fails() {
    fails_after(
        Shapes {
            needs_you: NeedsYou::Peek,
            ..Default::default()
        },
        Step::NeedsYou,
        "needs-you",
        NY_PEEK,
        |exec| last_keys_at(exec, "alt+a"),
        5,
    );
}

#[test]
fn needs_you_no_deny_fails() {
    fails_after(
        Shapes {
            needs_you: NeedsYou::NoDeny,
            ..Default::default()
        },
        Step::NeedsYou,
        "needs-you",
        NY_DENY,
        |exec| last_typed_at(exec, "a"),
        5,
    );
}

#[test]
fn needs_you_not_cleared_fails() {
    fails_after(
        Shapes {
            needs_you: NeedsYou::NotCleared,
            ..Default::default()
        },
        Step::NeedsYou,
        "needs-you",
        NY_CLEARED,
        |exec| last_typed_at(exec, "1"),
        10,
    );
}

#[test]
fn needs_you_command_ran_fails() {
    fails(
        Shapes {
            needs_you: NeedsYou::CommandRan,
            ..Default::default()
        },
        Step::NeedsYou,
        "needs-you",
        NY_RAN,
    );
}

#[test]
fn needs_you_segment_stays_fails() {
    fails(
        Shapes {
            needs_you: NeedsYou::SegmentStays,
            ..Default::default()
        },
        Step::NeedsYou,
        "needs-you",
        NY_FLAG,
    );
}

#[test]
fn needs_you_skips_when_the_command_ran_unasked() {
    let shapes = Shapes {
        needs_you: NeedsYou::SkipUnasked,
        ..Default::default()
    };
    let exec = play(&shapes, Step::NeedsYou);
    assert!(
        exec.lines()
            .contains(&format!("SKIP needs-you ({NY_SKIP})")),
        "lines:\n{}",
        exec.lines()
    );
    assert_eq!(exec.report.exit_code(), 3);
    assert_one_close_tab(&exec);
}

#[test]
fn a_failing_step_stops_the_run_and_cleans_up() {
    let shapes = Shapes {
        fleet_quiet: FleetQuiet::Exit(7),
        ..Default::default()
    };
    let exec = play(&shapes, Step::NeedsYou);
    assert_fails_with(&exec, "fleet-quiet", QUIET_EXIT);
    assert_eq!(
        exec.kinds(),
        ["fleet-quiet:fail"],
        "later steps print nothing"
    );
    assert_eq!(exec.report.exit_code(), 1);
    assert_one_close_tab(&exec);

    let script = script_3(&Shapes::default(), Step::FleetQuiet)
        .fail(FailBuilder::call("pane run", "read_gone").message("gone"));
    let exec = exec_3(&script, |_| {}, |term| term);
    assert_fails_with(&exec, "fleet-quiet", "read_gone: gone");
    assert_one_close_tab(&exec);
}

#[test]
fn steps_run_in_the_documented_order() {
    let exec = full();
    assert_eq!(
        exec.kinds(),
        [
            "fleet-quiet:pass",
            "bg-launch:pass",
            "worktree-launch:pass",
            "fleet-list:pass",
            "attach:pass",
            "startup:pass",
            "plain-answer:pass",
            "local-commands:pass",
            "fleet-view:pass",
            "open-in-pane:pass",
            "quit:pass",
            "needs-you:pass",
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
    assert_one_close_tab(&exec);
}

#[test]
fn only_needs_you_may_skip() {
    let script = script_3(&Shapes::default(), Step::NeedsYou)
        .fail(FailBuilder::call("pane send-keys", "invalid_key").message("unsupported key"));
    let exec = exec_3(&script, |_| {}, |term| term);
    assert_eq!(exec.report.exit_code(), 1, "lines:\n{}", exec.lines());
    assert!(
        exec.kinds().iter().all(|kind| !kind.ends_with(":skip")),
        "a rejected key outside needs-you is a failure, not a skip: {:?}",
        exec.kinds()
    );
    assert!(
        exec.report
            .result_line()
            .starts_with("SMOKE_RESULT: FAILED"),
        "the result is a failure: {}",
        exec.report.result_line()
    );
    assert_one_close_tab(&exec);
}

#[test]
fn needs_you_retries_once_then_fails() {
    let shapes = Shapes {
        needs_you: NeedsYou::RetryThenFail,
        ..Default::default()
    };
    let exec = play(&shapes, Step::NeedsYou);
    assert_fails_with(&exec, "needs-you", NY_NEVER);
    let times = run_times(&exec, "ny.out");
    assert_eq!(
        times.len(),
        2,
        "the first attempt's lines do not satisfy the repeat"
    );
    assert!(
        times[1] - times[0] >= Duration::from_secs(120),
        "the repeat waits out the first 120 s poll: {times:?}"
    );
    assert_one_close_tab(&exec);
}

#[test]
fn skip_exits_3_and_fails_under_require() {
    let shapes = Shapes {
        needs_you: NeedsYou::SkipUnasked,
        ..Default::default()
    };
    let exec = play(&shapes, Step::NeedsYou);
    assert!(
        exec.lines()
            .contains(&format!("SKIP needs-you ({NY_SKIP})")),
        "lines:\n{}",
        exec.lines()
    );
    assert_eq!(
        exec.report.result_line(),
        "SMOKE_RESULT: PASSED WITH SKIPS (needs-you)"
    );
    assert_eq!(exec.report.exit_code(), 3);
    assert_one_close_tab(&exec);

    let exec = play_cfg(&shapes, Step::NeedsYou, |cfg| cfg.require = true);
    let line = format!("FAIL needs-you (skipped: {NY_SKIP}; TALARIA_LIVE_REQUIRE=1)");
    assert!(
        exec.lines().contains(&line),
        "the skip fails under the live requirement; lines:\n{}",
        exec.lines()
    );
    assert_eq!(exec.report.exit_code(), 1);
    assert_one_close_tab(&exec);
}

#[test]
fn shared_steps_behave_as_in_phase_1() {
    let exec = full();
    let launch = exec
        .run_texts()
        .into_iter()
        .find(|text| text.contains("--provider ollama-cloud") && !text.contains("--bg"))
        .expect("the foreground launch");
    for flag in [
        "--no-ensure",
        "--profile fixture",
        "--model minimax-m3.1-flash",
        "--provider ollama-cloud",
    ] {
        assert!(launch.contains(flag), "the launch carries {flag}: {launch}");
    }
    assert!(
        !launch.contains("--bg"),
        "the foreground launch is not a background launch: {launch}"
    );
    assert!(
        exec.run_texts()
            .iter()
            .any(|text| text.contains("join these two words with a hyphen")),
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

#[test]
fn step_limits_sum_below_the_deadline() {
    let sum: u64 = STEP_LIMITS.iter().map(|(_, limit)| limit).sum();
    assert_eq!(sum, 1095, "the twelve limits add up to 1095 s");
    let names: Vec<&str> = STEP_LIMITS.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, STEP_ORDER.to_vec());
    let deadline = Suite::Phase3.default_deadline_secs();
    assert!(
        sum < deadline,
        "the sum {sum} s is under the phase 3 deadline of {deadline} s"
    );
}
