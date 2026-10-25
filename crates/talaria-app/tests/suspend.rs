//! Suspend and resume: the resume watch and the wait before the deadline.
//!
//! Behavior contract: issue 228.

#![cfg(unix)]

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use talaria_app::comforts::suspend::{SuspendResult, spawn_sigcont_watch, wait_for_continue};

fn send_continue_to_self() {
    rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::CONT)
        .unwrap();
}

#[test]
fn sigcont_watch_reports_only_unexpected() {
    let expected = Arc::new(AtomicBool::new(false));
    let released = Arc::new(AtomicBool::new(false));
    let continued = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    spawn_sigcont_watch(
        expected.clone(),
        released.clone(),
        continued.clone(),
        move || {
            let _ = tx.send(());
        },
    )
    .unwrap();

    send_continue_to_self();
    rx.recv_timeout(Duration::from_secs(5))
        .expect("an unexpected resume reports");
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "it reports once"
    );

    expected.store(true, std::sync::atomic::Ordering::SeqCst);
    send_continue_to_self();
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "an expected resume stays silent"
    );

    expected.store(false, std::sync::atomic::Ordering::SeqCst);
    released.store(true, std::sync::atomic::Ordering::SeqCst);
    send_continue_to_self();
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "a resume while released stays silent"
    );
}

#[test]
fn flag_is_checked_before_the_deadline() {
    let start = Instant::now();
    let ten_seconds_later = || start + Duration::from_secs(10);

    let flag = AtomicBool::new(true);
    assert_eq!(
        wait_for_continue(&flag, start, &ten_seconds_later, Duration::from_secs(1)),
        SuspendResult::Resumed,
        "a set flag resumes however late the clock runs"
    );

    let flag = AtomicBool::new(false);
    assert_eq!(
        wait_for_continue(&flag, start, &ten_seconds_later, Duration::from_secs(1)),
        SuspendResult::NotStopped,
        "an unset flag stops after a single pass"
    );
}
