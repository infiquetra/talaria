//! The install target comes from the control socket, never from a roster request.
#![cfg(unix)]

use crate::mock_gateway::StandaloneGateway;

use std::time::Duration;
use talaria_workflow::skill::resolve_target_in;

#[tokio::test]
async fn named_profile_resolves_while_the_roster_request_is_silent() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_silent("profiles.list", true);
    let connections = gateway.connection_count();

    let resolved = tokio::time::timeout(
        Duration::from_secs(3),
        resolve_target_in("fixture", gateway.home()),
    )
    .await
    .expect("the target resolves without waiting for a roster reply")
    .expect("the target resolves");

    let (target, warnings) = resolved;
    let fixture = std::fs::canonicalize(gateway.home().join("profiles/fixture")).expect("folder");
    assert_eq!(target.profile, "fixture");
    assert_eq!(target.home, fixture);
    assert_eq!(
        target.hermes_root,
        std::fs::canonicalize(gateway.home()).expect("root")
    );
    assert!(target.via_gateway);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert!(gateway.requests("profiles.list").is_empty());
    assert_eq!(gateway.connection_count(), connections);
    assert!(
        gateway
            .control_requests()
            .iter()
            .any(|seen| seen.verb == "identify"),
        "the control socket was asked who it serves"
    );
}

#[tokio::test]
async fn default_profile_resolves_to_the_hermes_root_while_the_roster_request_is_silent() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_silent("profiles.list", true);
    let connections = gateway.connection_count();

    let (target, warnings) = tokio::time::timeout(
        Duration::from_secs(3),
        resolve_target_in("default", gateway.home()),
    )
    .await
    .expect("the target resolves without waiting for a roster reply")
    .expect("the target resolves");

    let root = std::fs::canonicalize(gateway.home()).expect("root");
    assert_eq!(target.profile, "default");
    assert_eq!(target.home, root);
    assert_eq!(target.hermes_root, root);
    assert!(target.via_gateway);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert!(gateway.requests("profiles.list").is_empty());
    assert_eq!(gateway.connection_count(), connections);
}

#[tokio::test]
async fn a_folder_the_gateway_does_not_serve_falls_back_to_the_local_folder() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_silent("profiles.list", true);
    std::fs::create_dir_all(gateway.home().join("profiles/unserved")).expect("a folder");
    let connections = gateway.connection_count();

    let (target, warnings) = tokio::time::timeout(
        Duration::from_secs(3),
        resolve_target_in("unserved", gateway.home()),
    )
    .await
    .expect("the target resolves without waiting for a roster reply")
    .expect("the target resolves");

    let folder = std::fs::canonicalize(gateway.home().join("profiles/unserved")).expect("folder");
    assert_eq!(target.home, folder);
    assert!(!target.via_gateway);
    assert_eq!(
        warnings,
        ["the gateway does not serve profile unserved; using the local profile folder"]
    );
    assert!(gateway.requests("profiles.list").is_empty());
    assert_eq!(gateway.connection_count(), connections);
}

#[tokio::test]
async fn a_home_given_through_a_link_resolves_to_the_same_folder() {
    let real = tempfile::tempdir().expect("a home");
    let links = tempfile::tempdir().expect("a folder for the link");
    let linked = links.path().join("hermes");
    std::os::unix::fs::symlink(real.path(), &linked).expect("a link to the home");
    let gateway = StandaloneGateway::start(real.path());
    gateway.set_silent("profiles.list", true);

    let (target, warnings) = tokio::time::timeout(
        Duration::from_secs(3),
        resolve_target_in("fixture", &linked),
    )
    .await
    .expect("the target resolves without waiting for a roster reply")
    .expect("the target resolves");

    assert_eq!(
        target.home,
        std::fs::canonicalize(real.path().join("profiles/fixture")).expect("folder")
    );
    assert!(target.via_gateway);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert!(gateway.requests("profiles.list").is_empty());
}

#[tokio::test]
async fn a_broken_gateway_socket_names_the_step_that_failed() {
    let home = tempfile::tempdir().expect("a home");
    std::fs::write(home.path().join("gateway.sock"), b"not a socket").expect("a plain file");

    let error = resolve_target_in("default", home.path())
        .await
        .expect_err("a plain file is not a gateway socket");

    let text = error.to_string();
    assert!(
        text.starts_with("could not find the folder for profile default: "),
        "{text}"
    );
    assert!(!text.contains("gateway timeout"), "{text}");
}
