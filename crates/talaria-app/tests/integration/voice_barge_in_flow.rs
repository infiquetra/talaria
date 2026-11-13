//! Playback stops before a capture, using a fake helper and no network call.

use std::fs;

use talaria_app::voice::playback::SentencePlayback;

#[cfg(unix)]
fn helper(dir: &std::path::Path) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("helper");
    fs::write(
        &path,
        "#!/bin/sh\nlog=$1\nwhile IFS= read -r line; do printf '%s\\n' \"$line\" >> \"$log\"; done\n",
    )
    .expect("write helper");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("make executable");
    vec![path.to_string_lossy().into_owned()]
}

#[tokio::test]
#[cfg(unix)]
async fn stop_and_quit_cancel_playback() {
    let dir = tempfile::tempdir().expect("helper directory");
    let log = dir.path().join("heard");
    fs::write(&log, "").expect("log");
    let mut play = SentencePlayback::start(&helper(dir.path()), log)
        .await
        .expect("start playback");
    play.enqueue(1, "First sentence.");
    play.enqueue(1, "Second sentence.");
    play.enqueue(1, "Third sentence.");
    assert_eq!(play.queued().len(), 3, "three sentences are queued");
    let report = play.quit().await;
    assert_eq!(report.network_methods, 0, "quit makes no network call");
    assert!(play.saw_stop(), "quit tells the playback helper to stop");
    assert!(play.queued().is_empty(), "quit drops queued sentences");
    assert!(!play.capture_allowed(), "quit does not open the microphone");
}

#[tokio::test]
#[cfg(unix)]
async fn recording_starts_only_after_quiet() {
    let dir = tempfile::tempdir().expect("helper directory");
    let log = dir.path().join("heard");
    fs::write(&log, "").expect("log");
    let mut play = SentencePlayback::start(&helper(dir.path()), log)
        .await
        .expect("start playback");
    play.enqueue(1, "First sentence.");
    play.enqueue(1, "Second sentence.");
    let report = play.stop().await;
    assert_eq!(
        report.network_methods, 0,
        "stopping playback makes no network call"
    );
    assert!(play.saw_stop(), "stop uses the helper control path");
    assert!(play.queued().is_empty(), "stop drops queued sentences");
    assert!(
        !play.capture_allowed(),
        "the microphone stays closed until quiet"
    );
    play.enqueue(play.audio_epoch(), "late sentence");
    assert!(
        !play.queued().iter().any(|text| text == "late sentence"),
        "a late sentence from the stopped audio epoch is ignored"
    );
    play.confirm_quiet().await;
    assert!(
        play.capture_allowed(),
        "recording starts only after the helper confirms quiet"
    );
}
