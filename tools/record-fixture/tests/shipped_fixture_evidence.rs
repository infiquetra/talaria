//! The shipped fixtures must satisfy the recorder's own evidence check.
//!
//! `record-fixture` refuses to write a recording whose frames do not show what the
//! scenario claims happened. The frames in `fixtures/frames` are the recordings of
//! those same scenarios, so the check that guards a fresh recording has to accept
//! them too. A scenario whose evidence rule drifts away from its recording fails
//! here, before anyone re-records and finds out the hard way.
use hermes_client::ReplaySource;
use record_fixture::{Scenario, check_evidence};
use std::path::PathBuf;

/// The scenarios whose recording is driven by the scripted driver rather than by
/// a plain model prompt. Their evidence rules live in `scripted.rs`.
const SCRIPTED: [&str; 5] = [
    "queue-edit-remove",
    "queue-steer",
    "queue-move-send-now",
    "model-switch",
    "permissions-cd",
];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/frames")
}

#[test]
fn the_shipped_recordings_satisfy_their_own_evidence_check() {
    for name in SCRIPTED {
        let path = fixtures_dir().join(format!("{name}.jsonl"));
        let scenario = Scenario::named(name).unwrap_or_else(|| panic!("{name} is not a scenario"));
        let source =
            ReplaySource::open(&path).unwrap_or_else(|error| panic!("{name}: {error}"));
        if let Err(reason) = check_evidence(&scenario, &source) {
            panic!("{name}: {reason}");
        }
    }
}
