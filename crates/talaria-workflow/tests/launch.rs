mod staffing_support;
use std::sync::Arc;
use talaria_workflow::{
    engine::Effort,
    hermes::{ExplicitLaunch, LaunchDefaults},
    journal::LaunchResolver,
    staffing::{StaffedLaunch, StaffingResolver},
};
#[test]
fn staffed_launch_composes_route_and_explicit() {
    let directory = tempfile::tempdir().unwrap();
    let canonical = directory.path().canonicalize().unwrap();
    let table = staffing_support::table();
    let roster = staffing_support::roster(&staffing_support::data());
    let resolver = StaffingResolver::new(Arc::new(table), Arc::new(roster));
    let explicit = ExplicitLaunch::new(LaunchDefaults {
        profile: "fixture".into(),
        cwd: canonical.clone(),
        toolsets: Some(vec!["file".into()]),
        worktrees: None,
    });
    let launch = StaffedLaunch::new(resolver, explicit);
    let mut request = staffing_support::request();
    request.tier = Some("survey".into());
    let result = launch.resolve(&request).unwrap();
    assert_eq!(result.model, "m-small");
    assert_eq!(result.provider.as_deref(), Some("alpha-cloud"));
    assert_eq!(result.profile, "fixture");
    assert_eq!(result.toolsets, vec!["file"]);
    assert_eq!(result.cwd.as_deref(), canonical.to_str());
    assert_eq!(launch.resolve(&request).unwrap(), result);
    request.tier = Some("missing".into());
    let error = launch.resolve(&request).unwrap_err();
    assert!(error.contains("missing"));
    #[derive(Clone)]
    struct Capture(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let output = Arc::new(std::sync::Mutex::new(Vec::new()));
    let writer = output.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_writer(move || Capture(writer.clone()))
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    request.model = Some("m-large".into());
    request.provider = Some("alpha-cloud".into());
    request.effort = Some(Effort::High);
    request.tier = Some("survey".into());
    let explicit = launch.resolve(&request).unwrap();
    assert_eq!(explicit.model, "m-large");
    assert_eq!(explicit.effort, Some(Effort::High));
    assert_eq!(launch.resolve(&request).unwrap(), explicit);
    let warnings = String::from_utf8(output.lock().unwrap().clone()).unwrap();
    assert_eq!(
        warnings
            .matches("tier 'survey' ignored: the call names a model")
            .count(),
        1
    );
}
