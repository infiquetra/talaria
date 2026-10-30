//! Launching an external agent from the fleet view.

use talaria_core::fleet_view::FleetAction;
use talaria_tui::launch_dialog::AcpLaunchDialog;

#[test]
fn new_action_collects_agent_directory_and_first_prompt() {
    let directory = std::env::current_dir().expect("launch directory");
    let mut dialog = AcpLaunchDialog::from_fleet_action(
        FleetAction::New,
        vec![("fixture".into(), "Fixture Agent".into())],
        directory.clone(),
    )
    .expect("new action opens external agent choice");
    dialog.choose_agent("fixture");
    dialog.set_prompt("inspect the project");
    let submitted = dialog.submit().expect("valid launch");
    assert_eq!(submitted.agent_id, "fixture");
    assert_eq!(submitted.cwd, directory);
    assert_eq!(submitted.prompt, "inspect the project");
    assert!(AcpLaunchDialog::from_fleet_action(FleetAction::Stop, vec![], submitted.cwd).is_none());
}

#[test]
fn missing_launch_directory_is_refused() {
    let tmp = tempfile::tempdir().expect("temporary directory");
    let mut dialog = AcpLaunchDialog::from_fleet_action(
        FleetAction::New,
        vec![("fixture".into(), "Fixture Agent".into())],
        tmp.path().to_owned(),
    )
    .expect("launch dialog");
    dialog.choose_agent("fixture");
    dialog.set_cwd(tmp.path().join("missing"));
    assert!(dialog.submit().is_err());
}
