//! A visible write result aborts any screen step immediately.
#[path = "support/scripts4a.rs"]
mod scripts4a;
mod support;

use scripts4a::{exec_4a, script_4a};
use smoke::StepOutcome;

const REASON: &str = "a write banner appeared; the suite must never change anything";
const SCREENS: &[(&str, &[&str])] = &[
    ("config-browse", &["r0", "r1", "r2", "r3", "r4"]),
    (
        "config-invalid-input",
        &["r0", "r1", "r2", "r3", "teardown"],
    ),
    ("config-secret-refused", &["r0", "r1", "r2", "r3"]),
    ("providers-tabs", &["r0", "r1", "r2", "r3", "teardown"]),
    (
        "providers-key-masked",
        &["r0", "r1", "r2", "r3", "r4", "teardown"],
    ),
    (
        "models-page",
        &["r0", "r1", "r2", "r3", "r4", "r5", "teardown"],
    ),
    (
        "models-bad-preset",
        &["r0", "r1", "r2", "r3", "r4", "teardown"],
    ),
    ("profiles-list", &["r0", "r1", "r2", "r3"]),
    (
        "profiles-bad-names",
        &["r0", "r1", "r2", "r3", "r4", "teardown"],
    ),
    ("doctor-overlay", &["r0", "r1", "r2"]),
];

fn check_banner(banner: &str) {
    for (step, reads) in SCREENS {
        for read in *reads {
            let fixture = format!("{step}-{read}.txt");
            let replacement = format!("# synthetic\n{banner}\n");
            let script = script_4a(&[(&fixture, "# synthetic\n", &replacement)]);
            let exec = exec_4a(&script, false, false, 1.0, 900);
            let failed = exec
                .report
                .outcomes
                .iter()
                .find_map(|outcome| match outcome {
                    StepOutcome::Fail { step, reason, .. } => Some((*step, reason.as_str())),
                    _ => None,
                });
            assert_eq!(failed, Some((*step, REASON)), "banner on {fixture}");
        }
    }
}
#[test]
fn a_saved_banner_fails_every_step() {
    for banner in ["Saved the key for OpenRouter.", "Saved a setting."] {
        check_banner(banner);
    }
}
#[test]
fn a_restart_result_fails_every_step() {
    for banner in ["Not restarted", "Gateway restarted"] {
        check_banner(banner);
    }
}
