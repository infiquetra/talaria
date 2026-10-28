#![cfg(feature = "fake-gateway")]
mod support;
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use support::harness::*;
use talaria_workflow::{
    engine::Effort,
    journal::{LaunchResolver, RunId, journal_key, request_id, submission_id},
};

#[test]
fn key_is_stable_and_sensitive() {
    let call = call_id();
    let base = launch();
    let canonical = r#"{"call":{"digest":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","occurrence":2},"launch":{"model":"fixture-model","profile":"fixture","skills":["skill"],"toolsets":["web"]},"v":1}"#;
    assert!(canonical.contains("fixture-model"));
    let golden = "27957e91bca9bb838cd1558c4ce81642";
    assert_eq!(journal_key(&call, &base), golden);
    let mut changed = base.clone();
    changed.model = "other".into();
    assert_ne!(journal_key(&call, &changed), golden);
    changed = base.clone();
    changed.effort = Some(Effort::High);
    assert_ne!(journal_key(&call, &changed), golden);
    changed = base.clone();
    changed.toolsets.push("other".into());
    assert_ne!(journal_key(&call, &changed), golden);
    let mut occurrence = call.clone();
    occurrence.occurrence += 1;
    assert_ne!(journal_key(&occurrence, &base), golden);
    changed = base.clone();
    changed.cwd = Some("relative-worktree".into());
    assert_eq!(journal_key(&call, &changed), golden);
    let resolver = TestResolver::default();
    let mut req = request();
    let before = journal_key(&req.call_id, &resolver.resolve(&req).unwrap());
    req.label = "different".into();
    req.phase = Some("different".into());
    assert_eq!(
        journal_key(&req.call_id, &resolver.resolve(&req).unwrap()),
        before
    );
}

#[test]
fn identifiers_match_the_design_and_limits() {
    let run = RunId::parse("01JB2X9Q4M8Z7K3T5V6W8Y0A1C").unwrap();
    let key = key();
    assert_eq!(
        request_id(&run, &key, 0),
        format!("wf:{}:{key}", run.as_str())
    );
    assert_eq!(
        submission_id(&run, &key, 0, 0),
        format!("wf:{}:{key}:0", run.as_str())
    );
    assert_eq!(
        request_id(&run, &key, 2),
        format!("wf:{}:{key}:a2", run.as_str())
    );
    assert_eq!(
        submission_id(&run, &key, 2, 1),
        format!("wf:{}:{key}:a2:1", run.as_str())
    );
    assert_eq!(request_id(&run, &key, u32::MAX).len(), 74);
    assert_eq!(submission_id(&run, &key, u32::MAX, 1).len(), 76);
    let mut runner = TestRunner::new(Config {
        cases: 256,
        failure_persistence: None,
        ..Config::default()
    });
    runner
        .run(
            &(
                0usize..8,
                prop::collection::vec(0usize..32, 25),
                prop::collection::vec(0usize..16, 32),
                any::<u32>(),
                0u32..2,
            ),
            |(first, run_chars, key_chars, attempt, turn)| {
                let alphabet = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
                let run: String = std::iter::once(first)
                    .chain(run_chars)
                    .map(|i| char::from(alphabet[i]))
                    .collect();
                let key: String = key_chars
                    .into_iter()
                    .map(|i| char::from(b"0123456789abcdef"[i]))
                    .collect();
                let run = RunId::parse(&run).unwrap();
                prop_assert!(request_id(&run, &key, attempt).len() <= 74);
                prop_assert!(submission_id(&run, &key, attempt, turn).len() <= 76);
                Ok(())
            },
        )
        .unwrap();
}
