#![cfg(feature = "fake-gateway")]
mod support;
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use support::{crash::run_case, harness::new_dir};

#[test]
fn every_crash_point_is_safe() {
    let _ = new_dir();
    let runtime = || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap()
    };
    let mut counts = [0usize; 19];
    let mut maxima = [(0usize, 0usize, 0usize); 19];
    for point in 1..=19u8 {
        let observed = runtime().block_on(run_case(point, 1, false, false, 0));
        let index = usize::from(point - 1);
        counts[index] += 1;
        maxima[index] = observed;
    }
    let totals = std::sync::Mutex::new((counts, maxima));
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256);
    let mut runner = TestRunner::new(Config {
        cases,
        max_shrink_iters: 0,
        failure_persistence: None,
        ..Config::default()
    });
    runner
        .run(
            &(1u8..20, 1usize..7, any::<bool>(), any::<bool>(), 0usize..6),
            |(point, items, pipeline, schema, chosen)| {
                let observed =
                    runtime().block_on(run_case(point, items, pipeline, schema, chosen % items));
                let index = usize::from(point - 1);
                let mut totals = totals.lock().unwrap();
                totals.0[index] += 1;
                totals.1[index].0 = totals.1[index].0.max(observed.0);
                totals.1[index].1 = totals.1[index].1.max(observed.1);
                totals.1[index].2 = totals.1[index].2.max(observed.2);
                Ok(())
            },
        )
        .unwrap();
    let (counts, maxima) = totals.into_inner().unwrap();
    for (index, (&count, &(sessions, admissions, reruns))) in counts.iter().zip(&maxima).enumerate()
    {
        assert!(count > 0);
        println!(
            "C{} cases={} max_sessions_per_attempt={} max_admissions_per_turn={} max_next_attempts_per_rerun={}",
            index + 1,
            count,
            sessions,
            admissions,
            reruns
        );
        assert!(sessions <= 1 && admissions <= 1 && reruns <= 1);
    }
}
