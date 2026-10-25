//! Red-run tests 26 to 31 (#231): exports and their files.

#[path = "support/directory_drive.rs"]
mod drive;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use drive::{committed_text, resume_snapshot, settle_short};
use serde_json::json;
use talaria_app::directory::ExportTarget;
use talaria_app::launch::StartMode;
use talaria_app::switch::write_export_file;

#[test]
fn write_never_overwrites() {
    let folder = tempfile::tempdir().expect("a folder");
    let target = ExportTarget::Auto {
        folder: folder.path().to_owned(),
    };
    let (path, bytes) = write_export_file(&target, "hermes_session_s-1.md", "# hello\n")
        .expect("the first write lands");
    assert_eq!(path, folder.path().join("hermes_session_s-1.md"));
    assert_eq!(bytes, 8);
    assert_eq!(
        std::fs::read_to_string(&path).expect("readable"),
        "# hello\n"
    );

    let (second, _) = write_export_file(&target, "hermes_session_s-1.md", "# again\n")
        .expect("the second write lands");
    assert_eq!(second, folder.path().join("hermes_session_s-1 (2).md"));

    let explicit = ExportTarget::Explicit {
        path: folder.path().join("notes.md"),
    };
    let (written, _) = write_export_file(&explicit, "ignored.md", "x").expect("explicit lands");
    assert_eq!(written, folder.path().join("notes.md"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "export files are owner-only");
    }
}

#[tokio::test]
async fn params_carry_profile_format_and_redaction() {
    for (refuse_host, command, redact) in [
        (false, "/export", true),
        (true, "/export --no-redact out.md", false),
    ] {
        let rig = drive::Rig::start();
        rig.gateway.set_refuse_host_scope(refuse_host);
        let folder = rig.folder.to_string_lossy().to_string();
        let mut snapshot = resume_snapshot(&folder);
        snapshot["info"]["title"] = json!("Launch title");
        rig.gateway.set_resume_snapshot(snapshot);

        let (_outcome, log) = rig
            .drive(StartMode::New, |driver| async move {
                driver.opened().await;
                driver.submit(command);
                driver.line("Exported").await;
                driver.quit();
                driver.finished().await;
            })
            .await;

        let frames = rig.gateway.requests("session.export");
        assert_eq!(frames.len(), 1);
        let params = &frames[0].params;
        if refuse_host {
            assert!(params.get("profile").is_none());
        } else {
            assert_eq!(params["profile"], json!("fixture"));
        }
        assert_eq!(params["format"], json!("md"));
        assert_eq!(params["redact"], json!(redact));
        let lines = committed_text(&log);
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("Exported ") && line.contains("Markdown")),
            "the export notice: {lines:?}"
        );
    }
}

#[tokio::test]
async fn names_fall_back_and_refusals_send_nothing() {
    // A taken automatic name falls back to the numbered candidate.
    let rig = drive::Rig::start();
    std::fs::write(
        rig.folder.join("hermes_session_local-session-1.md"),
        "taken",
    )
    .expect("taken");
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/export");
            driver.line("Exported").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    assert!(
        rig.folder
            .join("hermes_session_local-session-1 (2).md")
            .exists(),
        "the numbered candidate is written"
    );
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line.contains("(2).md")),
        "the notice names the candidate: {lines:?}"
    );

    // An explicit path that exists is refused without a call.
    let rig = drive::Rig::start();
    std::fs::write(rig.folder.join("taken.md"), "taken").expect("taken");
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/export taken.md");
            driver.line("already exists").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    assert!(
        rig.gateway.requests("session.export").is_empty(),
        "no call for an existing path"
    );
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line.contains("already exists")),
        "the refusal text: {lines:?}"
    );

    // A missing folder is refused without a call.
    let rig = drive::Rig::start();
    let (_outcome, _log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/export no-such-folder/out.md");
            driver.line("The folder").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    assert!(
        rig.gateway.requests("session.export").is_empty(),
        "no call for a missing folder"
    );

    // All 99 candidates taken.
    let rig = drive::Rig::start();
    std::fs::write(
        rig.folder.join("hermes_session_local-session-1.md"),
        "taken",
    )
    .expect("taken");
    for n in 2..=99 {
        std::fs::write(
            rig.folder
                .join(format!("hermes_session_local-session-1 ({n}).md")),
            "taken",
        )
        .expect("taken");
    }
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/export");
            driver.line("no free name").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line.contains("no free name")),
        "the exhaustion text: {lines:?}"
    );
}

#[tokio::test]
async fn refusal_texts() {
    // not_found: the session has no saved messages.
    let rig = drive::Rig::start();
    rig.gateway.set_export_empty("local-session-1", true);
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/export");
            driver.line("nothing to export").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line
            == "This session has nothing to export yet: Hermes has no saved messages for it."),
        "the empty text: {lines:?}"
    );

    // export_too_large: JSON only.
    let rig = drive::Rig::start();
    rig.gateway.set_export_too_large("local-session-1", true);
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/export out.json");
            driver.line("too large").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line
            == "This session is too large to export as JSON. Export it as Markdown instead."),
        "the too-large text: {lines:?}"
    );

    // invalid_params from the gateway.
    let rig = drive::Rig::start();
    rig.gateway.queue_behavior(
        "session.export",
        mock_gateway::Behavior::Refuse("invalid_params".to_owned()),
    );
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/export");
            driver.line("could not export").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines
            .iter()
            .any(|line| line == "Hermes could not export this session (invalid_params)."),
        "the invalid text: {lines:?}"
    );

    // A gateway from before exports.
    let rig = drive::Rig::start();
    rig.gateway.set_export_unsupported(true);
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/export");
            driver.line("no session.export").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line
            == "This Hermes gateway has no session.export. Ask the host to upgrade its Hermes."),
        "the unsupported text: {lines:?}"
    );

    // The reply cannot be read.
    let rig = drive::Rig::start();
    rig.gateway.set_export_malformed("local-session-1", true);
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/export");
            driver.line("could not be read").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line
            == "Hermes's export answer could not be read, so nothing was written."),
        "the unreadable text: {lines:?}"
    );

    // The connection drops mid-call.
    let rig = drive::Rig::start();
    rig.gateway
        .queue_behavior("session.export", mock_gateway::Behavior::DropBeforeReply);
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/export");
            driver.line("No answer").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line
            == "No answer from Hermes within 60 seconds, or the connection dropped. Run /export again."),
        "the dropped text: {lines:?}"
    );
}

#[tokio::test]
async fn export_timeout_is_sixty_seconds() {
    let rig = drive::Rig::start();
    rig.gateway
        .set_delay("session.export", Some(std::time::Duration::from_secs(61)));
    let gateway = &rig.gateway;
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            // The handshakes run in real time: a paused clock would pass their
            // timeouts while the gateway thread is still answering.
            driver.opened().await;
            driver.submit("/export");
            drive::eventually("session.export to arrive", || {
                gateway.requests("session.export").len() == 1
            })
            .await;
            tokio::time::pause();
            tokio::time::advance(std::time::Duration::from_secs(61)).await;
            tokio::time::resume();
            driver.line("No answer").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line
            == "No answer from Hermes within 60 seconds, or the connection dropped. Run /export again."),
        "the timeout text: {lines:?}"
    );
}

#[tokio::test]
async fn oversized_export_keeps_the_foreground() {
    let rig = drive::Rig::start();
    rig.gateway
        .set_export_content("local-session-1", &"x".repeat(9 << 20));
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/export");
            driver.line("too large to hold").await;
            driver.submit("still here");
            settle_short().await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line
            == "Hermes's export was too large to hold. Export it as Markdown instead, or in smaller pieces."),
        "the too-large text: {lines:?}"
    );
    assert_eq!(rig.gateway.requests("prompt.submit").len(), 1);
}

#[test]
fn write_failure_writes_nothing() {
    let folder = tempfile::tempdir().expect("a folder");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let ro = folder.path().join("ro");
        std::fs::create_dir(&ro).expect("a folder");
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).expect("read-only");
        let target = ExportTarget::Auto { folder: ro.clone() };
        let failed = write_export_file(&target, "hermes_session_s-1.md", "x");
        assert!(failed.is_err(), "a read-only folder fails the write");
        assert!(
            std::fs::read_dir(&ro).expect("readable").next().is_none(),
            "no partial file is left"
        );
    }
    let _ = folder;
}
