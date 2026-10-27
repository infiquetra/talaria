//! Any write banner aborts every read of a screen step.
#[path = "support/scripts4b.rs"]
mod scripts4b;
mod support;

use scripts4b::{exec_4b, script_4b};
use smoke::StepOutcome;
use smoke::wording4b::WRITE_BANNERS;

const REASON: &str = "a write banner appeared; the suite must never change anything";
const SCREENS: &[(&str, &[&str])] = &[
    ("mcp-browse", &["r0", "r1", "r2", "r3", "teardown"]),
    ("capabilities-browse", &["r0", "r1", "r2", "r3", "r4", "r5"]),
    ("memory-open", &["r0", "r1", "r2"]),
    ("memory-commands", &["r0", "r1", "r2"]),
    ("cron-browse", &["r0", "r1", "r2"]),
    (
        "messaging-browse",
        &[
            "r0",
            "r1",
            "r2",
            "r3",
            "teardown1",
            "teardown2",
            "teardown3",
        ],
    ),
    ("connectors-consent", &["r0", "r1", "teardown"]),
    ("configure-secret-refused", &["r0", "r1", "r2", "r3"]),
];

fn check_banner(banner: &str) {
    for (step, reads) in SCREENS {
        for read in *reads {
            let fixture = format!("{step}-{read}.txt");
            let replacement = format!("# synthetic\n{banner}\n");
            let script = script_4b(&[(&fixture, "# synthetic\n", &replacement)]);
            let run = exec_4b(&script, false, false, 1.0, 900);
            let failure = run
                .report
                .outcomes
                .iter()
                .find_map(|outcome| match outcome {
                    StepOutcome::Fail { step, reason, .. } => Some((*step, reason.as_str())),
                    _ => None,
                });
            assert_eq!(
                failure,
                Some((*step, REASON)),
                "banner {banner:?} on {fixture}"
            );
        }
    }
}

#[test]
fn a_saved_banner_fails_every_step() {
    let saved: Vec<_> = WRITE_BANNERS
        .iter()
        .copied()
        .filter(|banner| banner.starts_with("Saved"))
        .collect();
    assert_eq!(saved, ["Saved.", "Saved the key"]);
    for banner in saved {
        check_banner(banner);
    }
}

#[test]
fn a_reload_or_install_banner_fails_every_step() {
    for banner in WRITE_BANNERS {
        if !banner.starts_with("Saved") {
            check_banner(banner);
        }
    }
}
