use std::{fs, process::Command};
use talaria_workflow::{
    engine::{ResolveError, WorkflowRef, WorkflowResolver},
    runner::FilesystemResolver,
};

#[test]
fn name_and_path_resolution() {
    const CHILD: &str = "TALARIA_RESOLVER_TEST_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let resolver = FilesystemResolver::new(root.join("workflows"), root.join("script"));
        assert_eq!(
            resolver
                .resolve(&WorkflowRef::ScriptPath("only-cwd.js".into()))
                .unwrap(),
            "from cwd"
        );
        assert_eq!(
            resolver
                .resolve(&WorkflowRef::ScriptPath("both.js".into()))
                .unwrap(),
            "from script folder"
        );
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let workflows = temp.path().join("workflows");
    let script = temp.path().join("script");
    let cwd = temp.path().join("cwd");
    for dir in [&workflows, &script, &cwd] {
        fs::create_dir(dir).unwrap();
    }
    fs::write(workflows.join("review.js"), "named workflow").unwrap();
    fs::write(script.join("both.js"), "from script folder").unwrap();
    fs::create_dir(script.join("nested")).unwrap();
    fs::write(script.join("nested/child.js"), "nested script").unwrap();
    fs::write(cwd.join("both.js"), "from cwd").unwrap();
    fs::write(cwd.join("only-cwd.js"), "from cwd").unwrap();
    fs::write(temp.path().join("outside.js"), "outside").unwrap();
    let resolver = FilesystemResolver::new(workflows.clone(), script.clone());
    assert_eq!(
        resolver
            .resolve(&WorkflowRef::Name("review".into()))
            .unwrap(),
        "named workflow"
    );
    assert!(matches!(
        resolver.resolve(&WorkflowRef::Name("review.js".into())),
        Err(ResolveError::NotFound)
    ));
    assert!(matches!(
        resolver.resolve(&WorkflowRef::Name("../outside".into())),
        Err(ResolveError::NotFound)
    ));
    assert!(matches!(
        resolver.resolve(&WorkflowRef::Name("missing".into())),
        Err(ResolveError::NotFound)
    ));
    assert_eq!(
        resolver
            .resolve(&WorkflowRef::ScriptPath("both.js".into()))
            .unwrap(),
        "from script folder"
    );
    let confined = FilesystemResolver::new(workflows.clone(), script.clone()).with_cwd(cwd.clone());
    assert_eq!(
        confined
            .resolve(&WorkflowRef::ScriptPath("nested/child.js".into()))
            .unwrap(),
        "nested script"
    );
    for path in [
        "../outside.js".to_owned(),
        temp.path()
            .join("outside.js")
            .to_string_lossy()
            .into_owned(),
    ] {
        assert!(matches!(
            confined.resolve(&WorkflowRef::ScriptPath(path)),
            Err(ResolveError::NotFound)
        ));
    }
    let oversized = script.join("oversized.js");
    fs::File::create(&oversized)
        .unwrap()
        .set_len(1024 * 1024 + 1)
        .unwrap();
    assert!(matches!(
        confined.resolve(&WorkflowRef::ScriptPath("oversized.js".into())),
        Err(ResolveError::Unreadable(_))
    ));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(temp.path().join("outside.js"), script.join("linked.js"))
            .unwrap();
        assert!(matches!(
            confined.resolve(&WorkflowRef::ScriptPath("linked.js".into())),
            Err(ResolveError::NotFound)
        ));
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "runner_resolver::name_and_path_resolution",
            "--exact",
            "--nocapture",
        ])
        .env(CHILD, temp.path())
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
