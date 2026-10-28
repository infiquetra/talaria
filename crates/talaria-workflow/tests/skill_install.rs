use std::{fs, path::Path, time::Duration};
use talaria_workflow::skill::{
    InstallAction, InstallOptions, InstallRecords, SkillState, SkillTarget, inspect, install,
    normalise_profile, render, uninstall,
};
use tempfile::TempDir;
struct Fixture {
    _root: TempDir,
    home: std::path::PathBuf,
    talaria: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let canonical_root = fs::canonicalize(root.path()).unwrap();
        let home = canonical_root.join("hermes");
        let talaria = canonical_root.join("talaria");
        fs::create_dir_all(home.join("profiles/fixture")).unwrap();
        fs::create_dir_all(&talaria).unwrap();
        Self {
            _root: root,
            home,
            talaria,
        }
    }
    fn target(&self) -> SkillTarget {
        SkillTarget::resolve_local("fixture", &self.home).unwrap()
    }
    fn opts(&self) -> InstallOptions {
        InstallOptions {
            command: "talaria".into(),
            force: false,
            dry_run: false,
            talaria_home: self.talaria.clone(),
            now: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            lock_wait: Duration::from_millis(100),
        }
    }
}
#[cfg(unix)]
fn root_user() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .is_some_and(|o| o.stdout == b"0\n")
}
#[test]
fn install_into_missing_writes_files_and_record() {
    let f = Fixture::new();
    let t = f.target();
    let opts = f.opts();
    assert_eq!(install(&t, &opts).unwrap().action, InstallAction::Write);
    let rendered = render("fixture", "talaria", &f.home).unwrap();
    for file in &rendered {
        assert_eq!(fs::read(t.dir.join(file.path)).unwrap(), file.bytes);
    }
    let records = InstallRecords::load(&f.talaria).unwrap();
    let entry = records.entry(&t.dir).unwrap();
    assert_eq!(entry.target, t.dir);
    assert_eq!(entry.hermes_root, f.home);
    assert_eq!(entry.profile, "fixture");
    assert_eq!(entry.command, "talaria");
    assert_eq!(entry.skill_version, "1.0.0");
    assert_eq!(entry.files.len(), 4);
    use sha2::{Digest, Sha256};
    for file in &rendered {
        assert_eq!(
            entry.files[file.path],
            hex::encode(Sha256::digest(&file.bytes))
        );
    }
    assert_eq!(
        inspect(&t, &records, &rendered).unwrap().state,
        SkillState::Current
    );
}
#[test]
fn current_is_a_noop() {
    let f = Fixture::new();
    let t = f.target();
    let opts = f.opts();
    install(&t, &opts).unwrap();
    let before = fs::read(t.dir.join("SKILL.md")).unwrap();
    let record = fs::read(f.talaria.join("skills/installs.json")).unwrap();
    assert_eq!(install(&t, &opts).unwrap().action, InstallAction::None);
    assert_eq!(fs::read(t.dir.join("SKILL.md")).unwrap(), before);
    assert_eq!(
        fs::read(f.talaria.join("skills/installs.json")).unwrap(),
        record
    );
}
#[test]
fn outdated_unmodified_is_replaced() {
    let f = Fixture::new();
    let t = f.target();
    let mut opts = f.opts();
    install(&t, &opts).unwrap();
    opts.command = "talaria-alt".into();
    assert_eq!(install(&t, &opts).unwrap().action, InstallAction::Replace);
    assert_eq!(
        InstallRecords::load(&f.talaria)
            .unwrap()
            .entry(&t.dir)
            .unwrap()
            .command,
        "talaria-alt"
    );
}
#[test]
fn modified_refused_then_forced_with_backup() {
    let f = Fixture::new();
    let t = f.target();
    let mut opts = f.opts();
    install(&t, &opts).unwrap();
    fs::write(t.dir.join("SKILL.md"), b"edited").unwrap();
    let report = install(&t, &opts).unwrap();
    assert_eq!(report.action, InstallAction::Refuse);
    assert!(
        report
            .before
            .changed_files
            .iter()
            .any(|n| n == "changed:SKILL.md")
    );
    opts.force = true;
    let report = install(&t, &opts).unwrap();
    assert_eq!(report.action, InstallAction::BackupAndReplace);
    assert_eq!(
        fs::read(report.backup.unwrap().join("talaria-workflows/SKILL.md")).unwrap(),
        b"edited"
    );
}
#[test]
fn foreign_and_modified_edge_cases() {
    let f = Fixture::new();
    let t = f.target();
    let mut opts = f.opts();
    fs::create_dir_all(&t.dir).unwrap();
    for file in render("fixture", "talaria", &f.home).unwrap() {
        let p = t.dir.join(file.path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, file.bytes).unwrap();
    }
    let records = InstallRecords::load(&f.talaria).unwrap();
    assert_eq!(
        inspect(
            &t,
            &records,
            &render("fixture", "talaria", &f.home).unwrap()
        )
        .unwrap()
        .state,
        SkillState::Foreign
    );
    assert_eq!(install(&t, &opts).unwrap().action, InstallAction::Refuse);
    opts.force = true;
    install(&t, &opts).unwrap();
    fs::write(t.dir.join("extra"), b"extra").unwrap();
    let records = InstallRecords::load(&f.talaria).unwrap();
    let changed = inspect(
        &t,
        &records,
        &render("fixture", "talaria", &f.home).unwrap(),
    )
    .unwrap();
    assert_eq!(changed.state, SkillState::Modified);
    assert!(changed.changed_files.iter().any(|v| v == "added:extra"));
    fs::remove_file(t.dir.join("extra")).unwrap();
    fs::remove_file(t.dir.join("SKILL.md")).unwrap();
    let missing = inspect(
        &t,
        &records,
        &render("fixture", "talaria", &f.home).unwrap(),
    )
    .unwrap();
    assert!(
        missing
            .changed_files
            .iter()
            .any(|v| v == "removed:SKILL.md")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink("cycle", t.dir.join("cycle")).unwrap();
        let linked = inspect(
            &t,
            &records,
            &render("fixture", "talaria", &f.home).unwrap(),
        )
        .unwrap();
        assert!(linked.changed_files.iter().any(|v| v == "link:cycle"));
        let report = install(&t, &opts).unwrap();
        assert_eq!(report.action, InstallAction::BackupAndReplace);
        let link = report.backup.unwrap().join("talaria-workflows/cycle");
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(link).unwrap(), Path::new("cycle"));
        let foreign = Fixture::new();
        let target = foreign.target();
        let mut options = foreign.opts();
        let outside = foreign.home.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"safe").unwrap();
        fs::create_dir_all(target.dir.parent().unwrap()).unwrap();
        symlink(&outside, &target.dir).unwrap();
        assert_eq!(
            install(&target, &options).unwrap().action,
            InstallAction::Refuse
        );
        options.force = true;
        let result = install(&target, &options).unwrap();
        assert_eq!(result.action, InstallAction::BackupAndReplace);
        assert!(
            fs::symlink_metadata(result.backup.unwrap().join("talaria-workflows"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"safe");
    }
}
#[cfg(unix)]
#[test]
fn failure_leaves_old_folder_intact() {
    use std::os::unix::fs::PermissionsExt;
    if root_user() {
        eprintln!("skipped: running as root");
        return;
    }
    let f = Fixture::new();
    let t = f.target();
    let opts = f.opts();
    install(&t, &opts).unwrap();
    let old = fs::read(t.dir.join("SKILL.md")).unwrap();
    let skills = t.home.join("skills");
    fs::set_permissions(&skills, fs::Permissions::from_mode(0o500)).unwrap();
    let mut changed = f.opts();
    changed.command = "talaria-alt".into();
    let outcome = install(&t, &changed);
    fs::set_permissions(&skills, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(outcome.is_err());
    assert_eq!(fs::read(t.dir.join("SKILL.md")).unwrap(), old);
}
#[cfg(unix)]
#[test]
fn record_commit_failure_restores_old_folder_and_record() {
    use std::os::unix::fs::PermissionsExt;
    if root_user() {
        eprintln!("skipped: running as root");
        return;
    }
    let f = Fixture::new();
    let t = f.target();
    let opts = f.opts();
    install(&t, &opts).unwrap();
    let old = fs::read(t.dir.join("SKILL.md")).unwrap();
    let record_path = f.talaria.join("skills/installs.json");
    let record = fs::read(&record_path).unwrap();
    let record_dir = f.talaria.join("skills");
    fs::set_permissions(&record_dir, fs::Permissions::from_mode(0o500)).unwrap();
    let mut changed = f.opts();
    changed.command = "talaria-alt".into();
    let outcome = install(&t, &changed);
    fs::set_permissions(&record_dir, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(outcome.is_err());
    assert_eq!(fs::read(t.dir.join("SKILL.md")).unwrap(), old);
    assert_eq!(fs::read(record_path).unwrap(), record);
}
#[test]
fn concurrent_installs_serialize() {
    use talaria_workflow::skill::{InstallStage, install_with_hook};
    let f = Fixture::new();
    let home = f.home.clone();
    let talaria = f.talaria.clone();
    let (inspected_tx, inspected_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let first_home = home.clone();
    let first_talaria = talaria.clone();
    let first = std::thread::spawn(move || {
        let target = SkillTarget::resolve_local("fixture", &first_home).unwrap();
        let opts = InstallOptions {
            command: "talaria-a".into(),
            force: false,
            dry_run: false,
            talaria_home: first_talaria,
            now: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            lock_wait: Duration::from_secs(5),
        };
        install_with_hook(&target, &opts, &mut |stage| {
            if stage == InstallStage::AfterInspection {
                inspected_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }
        })
        .unwrap()
    });
    inspected_rx.recv().unwrap();
    let (starting_tx, starting_rx) = std::sync::mpsc::channel();
    let second = std::thread::spawn(move || {
        let target = SkillTarget::resolve_local("fixture", &home).unwrap();
        let opts = InstallOptions {
            command: "talaria-b".into(),
            force: false,
            dry_run: false,
            talaria_home: talaria,
            now: chrono::DateTime::from_timestamp(1_700_000_001, 0).unwrap(),
            lock_wait: Duration::from_secs(5),
        };
        starting_tx.send(()).unwrap();
        install(&target, &opts).unwrap()
    });
    starting_rx.recv().unwrap();
    release_tx.send(()).unwrap();
    assert_eq!(first.join().unwrap().action, InstallAction::Write);
    assert_eq!(second.join().unwrap().action, InstallAction::Replace);
    let t = f.target();
    let records = InstallRecords::load(&f.talaria).unwrap();
    let entry = records.entry(&t.dir).unwrap();
    assert_eq!(entry.command, "talaria-b");
    let rendered = render("fixture", &entry.command, &f.home).unwrap();
    assert_eq!(
        inspect(&t, &records, &rendered).unwrap().state,
        SkillState::Current
    );
}
#[test]
fn interrupted_install_recovered_under_lock() {
    let f = Fixture::new();
    let t = f.target();
    let opts = f.opts();
    install(&t, &opts).unwrap();
    let original = fs::read(t.dir.join("SKILL.md")).unwrap();
    let stage = t
        .home
        .join("skills/.locks/talaria-staging-0123456789abcdef");
    fs::create_dir_all(&stage).unwrap();
    fs::rename(&t.dir, stage.join("old")).unwrap();
    fs::create_dir_all(&t.dir).unwrap();
    fs::write(t.dir.join("SKILL.md"), "interrupted").unwrap();
    fs::write(
        stage.join("intent.json"),
        serde_json::to_vec(
            &serde_json::json!({"kind":"install","target":t.dir,"had_old":true,"new_files":{}}),
        )
        .unwrap(),
    )
    .unwrap();
    let warnings = talaria_workflow::skill::recover(&t, &opts).unwrap();
    assert!(warnings.iter().any(|s| s.contains("recovered")));
    assert_eq!(fs::read(t.dir.join("SKILL.md")).unwrap(), original);
    assert!(!stage.exists());
    let committed = t
        .home
        .join("skills/.locks/talaria-staging-1111111111111111");
    fs::create_dir_all(committed.join("old")).unwrap();
    fs::write(committed.join("old/SKILL.md"), "old").unwrap();
    fs::write(
        committed.join("intent.json"),
        serde_json::to_vec(
            &serde_json::json!({"kind":"install","target":t.dir,"had_old":true,"new_files":{}}),
        )
        .unwrap(),
    )
    .unwrap();
    talaria_workflow::skill::recover(&t, &opts).unwrap();
    assert_eq!(fs::read(t.dir.join("SKILL.md")).unwrap(), original);
    assert!(!committed.exists());
    let fresh = Fixture::new();
    let fresh_target = fresh.target();
    let fresh_opts = fresh.opts();
    let fresh_stage = fresh_target
        .home
        .join("skills/.locks/talaria-staging-2222222222222222");
    fs::create_dir_all(&fresh_stage).unwrap();
    let mut hashes = std::collections::BTreeMap::new();
    use sha2::{Digest, Sha256};
    for file in render("fixture", "talaria", &fresh.home).unwrap() {
        let path = fresh_target.dir.join(file.path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &file.bytes).unwrap();
        hashes.insert(
            file.path.to_owned(),
            hex::encode(Sha256::digest(&file.bytes)),
        );
    }
    fs::write(fresh_stage.join("intent.json"),serde_json::to_vec(&serde_json::json!({"kind":"install","target":fresh_target.dir,"had_old":false,"new_files":hashes})).unwrap()).unwrap();
    talaria_workflow::skill::recover(&fresh_target, &fresh_opts).unwrap();
    assert!(!fresh_target.dir.exists());
    assert!(!fresh_stage.exists());
}
#[test]
fn profile_names_homes_and_roots() {
    let f = Fixture::new();
    assert_eq!(normalise_profile(" Fixture ").unwrap(), "fixture");
    for invalid in ["", "../bad", "bad.name", "a/b", "a b"] {
        assert!(normalise_profile(invalid).is_err());
    }
    let t = f.target();
    assert_eq!(t.home, f.home.join("profiles/fixture"));
    assert_eq!(t.hermes_root, f.home);
    assert_eq!(t.dir, t.home.join("skills/talaria-workflows"));
    assert!(SkillTarget::resolve_local("missing", &f.home).is_err());
    let rooted = talaria_workflow::skill::hermes_root_of(&t.home, Path::new("/unused"));
    assert_eq!(rooted, f.home);
}
#[test]
fn home_from_gateway_row() {
    let f = Fixture::new();
    let path = f.home.join("profiles/fixture");
    let row = serde_json::json!({"name":"fixture","path":path});
    let t = SkillTarget::from_profile_row("fixture", &row).unwrap();
    assert_eq!(t.home, path);
    assert_eq!(t.hermes_root, f.home);
    assert!(t.via_gateway);
    assert!(SkillTarget::from_profile_row("default", &row).is_err());
}
#[test]
fn discovery_never_runs_ensure() {
    let f = Fixture::new();
    let t = f.target();
    let mut options = f.opts();
    options.dry_run = true;
    let report = install(&t, &options).unwrap();
    assert_eq!(report.before.state, SkillState::Missing);
    assert!(!t.dir.exists());
}
#[test]
fn uninstall_rules() {
    let f = Fixture::new();
    let t = f.target();
    let mut o = f.opts();
    install(&t, &o).unwrap();
    assert_eq!(uninstall(&t, &o).unwrap().action, InstallAction::Remove);
    assert!(!t.dir.exists());
    install(&t, &o).unwrap();
    fs::write(t.dir.join("SKILL.md"), b"modified").unwrap();
    assert_eq!(uninstall(&t, &o).unwrap().action, InstallAction::Refuse);
    o.force = true;
    let report = uninstall(&t, &o).unwrap();
    assert_eq!(report.action, InstallAction::BackupAndRemove);
    assert_eq!(
        fs::read(report.backup.unwrap().join("talaria-workflows/SKILL.md")).unwrap(),
        b"modified"
    );
    assert_eq!(uninstall(&t, &o).unwrap().action, InstallAction::None);
}
#[test]
fn json_output_golden() {
    let f = Fixture::new();
    let t = f.target();
    let mut options = f.opts();
    options.dry_run = true;
    let report = install(&t, &options).unwrap();
    assert_eq!(report.before.state, SkillState::Missing);
    assert_eq!(report.action, InstallAction::Write);
    assert!(report.before.installed_version.is_none());
    assert!(report.before.changed_files.is_empty());
    assert!(report.backup.is_none());
    assert!(!t.dir.exists());
}
#[test]
fn dry_run_creates_nothing() {
    let f = Fixture::new();
    let t = f.target();
    let mut o = f.opts();
    o.dry_run = true;
    install(&t, &o).unwrap();
    assert!(!t.dir.exists());
    assert!(!f.talaria.join("skills/installs.json").exists());
}
