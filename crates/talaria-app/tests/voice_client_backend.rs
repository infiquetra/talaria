use talaria_app::voice::transport::remote_audio_allowed;

#[test]
fn remote_audio_requires_explicit_opt_in() {
    assert!(remote_audio_allowed(false, false));
    assert!(!remote_audio_allowed(true, false));
    assert!(remote_audio_allowed(true, true));
}

#[cfg(unix)]
fn helper(script: &str) -> (tempfile::TempDir, Vec<String>) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("helper directory");
    let path = dir.path().join("helper");
    std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("write helper");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
        .expect("make executable");
    let command = vec![path.to_string_lossy().into_owned()];
    (dir, command)
}

#[tokio::test]
#[cfg(unix)]
async fn capture_collects_audio_only_after_explicit_stop() {
    use talaria_app::voice::transport::Capture;
    let (_dir, command) =
        helper("printf RIFF; read line; [ \"$line\" = stop ] || exit 1; printf WAVE");
    let capture = Capture::start(&command).expect("start capture");
    let audio = capture.stop().await.expect("stop capture");
    assert_eq!(audio, b"RIFFWAVE");
}

#[tokio::test]
#[cfg(unix)]
async fn capture_stop_is_bounded_for_a_helper_that_does_not_exit() {
    use talaria_app::voice::transport::Capture;
    let (_dir, command) = helper("read line; exec /bin/sleep 30");
    let capture = Capture::start(&command).expect("start capture");
    let start = std::time::Instant::now();
    assert_eq!(
        capture.stop().await.expect_err("must time out").kind(),
        std::io::ErrorKind::TimedOut
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}

#[tokio::test]
#[cfg(unix)]
async fn playback_writes_audio_and_has_a_separate_stop_path() {
    use talaria_app::voice::transport::Playback;
    let (dir, mut command) = helper("exec /bin/cat > \"$1\"");
    let output = dir.path().join("audio");
    command.push(output.to_string_lossy().into_owned());
    let mut playback = Playback::start(&command).expect("start playback");
    playback.play(b"audio bytes").await.expect("write audio");
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if std::fs::read(&output).ok().as_deref() == Some(b"audio bytes") {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("helper received bytes");
    playback.close().await.expect("reap playback");
}

#[tokio::test]
#[cfg(unix)]
async fn capture_rejects_audio_over_the_size_limit() {
    use talaria_app::voice::transport::Capture;
    let (_dir, command) = helper("/usr/bin/head -c 16777217 /dev/zero; read line");
    let capture = Capture::start(&command).expect("start capture");
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert_eq!(
        capture.stop().await.expect_err("oversized capture").kind(),
        std::io::ErrorKind::InvalidData
    );
}

#[tokio::test]
#[cfg(unix)]
async fn capture_finalizes_automatically_at_the_time_limit() {
    use talaria_app::voice::transport::Capture;
    let (_dir, command) = helper("read line; [ \"$line\" = stop ] || exit 1; printf audio");
    tokio::time::pause();
    let capture = Capture::start(&command).expect("start capture");
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_secs(120)).await;
    tokio::time::resume();
    assert_eq!(capture.finish().await.expect("automatic finish"), b"audio");
}

#[tokio::test]
#[cfg(unix)]
async fn cancelling_capture_reaps_it_without_waiting_for_stop() {
    use talaria_app::voice::transport::Capture;
    let (_dir, command) = helper("exec /bin/sleep 30");
    let capture = Capture::start(&command).expect("start capture");
    tokio::time::timeout(std::time::Duration::from_millis(500), capture.cancel())
        .await
        .expect("capture cancellation is bounded");
}

#[tokio::test]
#[cfg(unix)]
async fn playback_control_stops_a_helper_blocked_on_audio() {
    use talaria_app::voice::transport::Playback;
    let (_dir, command) = helper("exec /bin/sleep 30");
    let mut playback = Playback::start(&command).expect("start playback");
    let stop = playback.stop_handle();
    let task = tokio::spawn(async move { playback.play(&vec![0; 1024 * 1024]).await });
    tokio::task::yield_now().await;
    stop.stop();
    let result = tokio::time::timeout(std::time::Duration::from_millis(500), task)
        .await
        .expect("playback stop is bounded")
        .expect("playback task");
    assert_eq!(
        result.expect_err("playback was stopped").kind(),
        std::io::ErrorKind::Interrupted
    );
}
