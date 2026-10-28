use std::{fs, path::Path};
use talaria_workflow::builtins::{
    BUILTINS, InstallAction, InstallMode, ScriptArg, install, resolve_script_arg,
};
fn setup() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}
#[test]
fn creates_then_reports_unchanged() {
    let t = setup();
    let d = t.path().join("workflows");
    let first = install(&d, InstallMode::MissingOnly).unwrap();
    assert_eq!(first.len(), 4);
    assert!(first.iter().all(|x| x.action == InstallAction::Created));
    for b in BUILTINS {
        assert_eq!(
            fs::read_to_string(d.join(format!("{}.js", b.name))).unwrap(),
            b.source
        );
    }
    assert!(
        install(&d, InstallMode::MissingOnly)
            .unwrap()
            .iter()
            .all(|x| x.action == InstallAction::Unchanged)
    );
}
#[test]
fn keeps_a_modified_file_without_force() {
    let t = setup();
    install(t.path(), InstallMode::MissingOnly).unwrap();
    let f = t.path().join("review-branch.js");
    fs::write(&f, "edited").unwrap();
    let out = install(t.path(), InstallMode::MissingOnly).unwrap();
    assert_eq!(
        out.iter()
            .find(|x| x.name == "review-branch")
            .unwrap()
            .action,
        InstallAction::KeptModified
    );
    assert_eq!(fs::read_to_string(f).unwrap(), "edited");
}
#[test]
fn force_writes_a_backup() {
    let t = setup();
    install(t.path(), InstallMode::MissingOnly).unwrap();
    let f = t.path().join("review-branch.js");
    fs::write(&f, "edited").unwrap();
    let out = install(t.path(), InstallMode::Overwrite).unwrap();
    let report = out.iter().find(|x| x.name == "review-branch").unwrap();
    assert_eq!(report.action, InstallAction::Overwritten);
    assert_eq!(
        fs::read_to_string(report.backup.as_ref().unwrap()).unwrap(),
        "edited"
    );
    assert_eq!(
        fs::read_to_string(f).unwrap(),
        BUILTINS
            .iter()
            .find(|b| b.name == "review-branch")
            .unwrap()
            .source
    );
}
#[cfg(unix)]
#[test]
fn creates_the_folder_private() {
    use std::os::unix::fs::PermissionsExt;
    let t = setup();
    let d = t.path().join("out");
    install(&d, InstallMode::MissingOnly).unwrap();
    assert_eq!(
        fs::metadata(&d).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for b in BUILTINS {
        assert_eq!(
            fs::metadata(d.join(format!("{}.js", b.name)))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[cfg(unix)]
#[test]
fn never_writes_through_a_symlink() {
    use std::os::unix::fs::symlink;
    let t = setup();
    let d = t.path().join("out");
    fs::create_dir(&d).unwrap();
    let sentinel = t.path().join("sentinel");
    fs::write(&sentinel, "untouched").unwrap();
    let target = d.join("review-branch.js");
    symlink(&sentinel, &target).unwrap();
    for mode in [InstallMode::MissingOnly, InstallMode::Overwrite] {
        let report = install(&d, mode).unwrap();
        assert_eq!(
            report
                .iter()
                .find(|x| x.name == "review-branch")
                .unwrap()
                .action,
            InstallAction::KeptModified
        );
        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "untouched");
    }
    fs::remove_file(&target).unwrap();
    fs::write(&target, "edited").unwrap();
    let backup = d.join("review-branch.js.bak");
    symlink(&sentinel, &backup).unwrap();
    install(&d, InstallMode::Overwrite).unwrap();
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "untouched");
    assert!(
        !fs::symlink_metadata(backup)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let other = t.path().join("other");
    fs::create_dir(&other).unwrap();
    let planted = other.join(format!(".review-branch.js.tmp-{}-0", std::process::id()));
    symlink(&sentinel, &planted).unwrap();
    install(&other, InstallMode::MissingOnly).unwrap();
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "untouched");
    assert!(
        fs::symlink_metadata(&planted)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let actual = t.path().join("actual");
    fs::create_dir(&actual).unwrap();
    let linked = t.path().join("linked");
    symlink(&actual, &linked).unwrap();
    install(&linked, InstallMode::MissingOnly).unwrap();
    assert!(actual.join("review-branch.js").is_file());
}
#[test]
fn error_names_the_path() {
    let t = setup();
    let path = t.path().join("not-a-folder");
    fs::write(&path, "file").unwrap();
    let err = install(&path, InstallMode::MissingOnly).unwrap_err();
    assert!(err.to_string().contains(path.to_str().unwrap()));
}
#[test]
fn leaves_other_files_alone() {
    let t = setup();
    let f = t.path().join("unrelated");
    fs::write(&f, "keep").unwrap();
    install(t.path(), InstallMode::Overwrite).unwrap();
    assert_eq!(fs::read_to_string(f).unwrap(), "keep");
}
#[test]
fn resolve_script_arg_rules() {
    let t = setup();
    let d = t.path();
    let arg = resolve_script_arg("review-branch", d).unwrap();
    assert!(
        matches!(arg,ScriptArg::MissingBuiltin { name, path } if name=="review-branch" && path==d.join("review-branch.js"))
    );
    assert!(!d.join("review-branch.js").exists());
    assert!(
        matches!(resolve_script_arg("custom.js",d).unwrap(),ScriptArg::Path(p) if p==Path::new("custom.js"))
    );
    assert!(resolve_script_arg("no-such-builtin", d).is_err());
}
