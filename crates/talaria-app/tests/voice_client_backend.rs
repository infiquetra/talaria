use talaria_app::voice::transport::Helpers;
use talaria_app::voice::transport::remote_audio_allowed;

#[test]
fn remote_audio_requires_explicit_opt_in() {
    assert!(remote_audio_allowed(false, false));
    assert!(!remote_audio_allowed(true, false));
    assert!(remote_audio_allowed(true, true));
}

#[test]
fn close_reaps_both_owned_helpers() {
    let mut helpers = Helpers::start("/bin/cat", "/bin/cat").expect("start helpers");
    let (capture, playback) = helpers.pids();
    assert_ne!(capture, playback);
    helpers.close();
    for pid in [capture, playback] {
        let status = std::process::Command::new("/bin/ps")
            .args(["-p", &pid.to_string()])
            .output()
            .expect("inspect process");
        assert!(!status.status.success(), "helper {pid} is still running");
    }
}
