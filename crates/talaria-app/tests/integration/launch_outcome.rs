//! Text, JSON and exit codes. The strings are copied from the launch rules, not from the renderer.

use std::path::PathBuf;

use talaria_app::bg::{
    LaunchError, LaunchOutcome, Launched, exit_code, notice_text, render_json, render_text,
};
use talaria_app::worktree::WorktreeRecord;

fn tree() -> WorktreeRecord {
    WorktreeRecord {
        main_root: PathBuf::from("repo"),
        path: PathBuf::from("repo/.worktrees/talaria-fix"),
        branch: "talaria/fix".to_owned(),
        base_ref: "origin/main".to_owned(),
        base_commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        base_fetched: true,
        profile: "fixture".to_owned(),
        session_id: Some("sess-1".to_owned()),
        created_at: 1_700_000_000,
    }
}

fn launched() -> LaunchOutcome {
    LaunchOutcome::Launched(Launched {
        session_id: "sess-1".to_owned(),
        stored_session_id: "stored-1".to_owned(),
        profile: "fixture".to_owned(),
        admission_id: "adm-1".to_owned(),
        admission_status: "queued".to_owned(),
        model: Some("fixture-model".to_owned()),
        cwd: PathBuf::from("repo/.worktrees/talaria-fix"),
        worktree: Some(tree()),
        warnings: vec![
            "talaria: warning: could not fetch origin/main (network); branched from the copy fetched earlier at 0123456789ab.".to_owned(),
        ],
        prompt: "fix the flaky test".to_owned(),
    })
}

fn not_started() -> LaunchOutcome {
    LaunchOutcome::NotStarted {
        error: LaunchError {
            what: "git was not found on this machine.".to_owned(),
            next_step: "Install git, or run without --worktree.".to_owned(),
        },
        closed_session: None,
        tree_removed: false,
        tree_kept: None,
    }
}

fn unknown() -> LaunchOutcome {
    LaunchOutcome::Unknown {
        session_id: Some("sess-1".to_owned()),
        stored_session_id: Some("stored-1".to_owned()),
        profile: "fixture".to_owned(),
        worktree: Some(tree()),
    }
}

#[test]
fn text_output_exact() {
    let (stdout, stderr) = render_text(&launched());
    let expected_out = "\
Launched session stored-1 in profile fixture.
Folder: repo/.worktrees/talaria-fix
Worktree: branch talaria/fix from origin/main at 0123456789ab
Follow it: talaria fleet list --profile fixture
Open it: talaria --resume stored-1 --profile fixture
";
    assert_eq!(stdout, expected_out);
    assert_eq!(
        stderr,
        "talaria: warning: could not fetch origin/main (network); branched from the copy fetched earlier at 0123456789ab.\n"
    );

    let (stdout, stderr) = render_text(&not_started());
    assert_eq!(stdout, "");
    assert_eq!(
        stderr,
        "talaria: git was not found on this machine.\nNext step: Install git, or run without --worktree.\n"
    );

    assert_eq!(
        notice_text(&not_started()),
        "Background launch failed: git was not found on this machine."
    );
    assert_eq!(
        notice_text(&unknown()),
        "Background launch may have started (sess-1); check the fleet before launching again."
    );
}

#[test]
fn json_output_exact() {
    let line = render_json(&launched());
    let expected = r#"{"format":"talaria.bg.v1","status":"launched","session_id":"sess-1","stored_session_id":"stored-1","profile":"fixture","admission_id":"adm-1","admission_status":"queued","model":"fixture-model","cwd":"repo/.worktrees/talaria-fix","worktree":{"path":"repo/.worktrees/talaria-fix","branch":"talaria/fix","base_ref":"origin/main","base_commit":"0123456789abcdef0123456789abcdef01234567","base_fetched":true},"warnings":["talaria: warning: could not fetch origin/main (network); branched from the copy fetched earlier at 0123456789ab."],"error":null}"#;
    assert_eq!(line, expected);

    let failed = render_json(&not_started());
    assert!(failed.contains(r#""status":"not_started""#), "{failed}");
    assert!(failed.starts_with(r#"{"format":"talaria.bg.v1","status":"not_started","#));
    assert!(failed.contains(r#""error":{"what":"git was not found on this machine.","next_step":"Install git, or run without --worktree."}"#));
}

#[test]
fn exit_codes() {
    assert_eq!(exit_code(&launched()), 0);
    assert_eq!(exit_code(&unknown()), 1);
    assert_eq!(exit_code(&not_started()), 4);
    assert_eq!(
        exit_code(&LaunchOutcome::Interrupted {
            tree_removed: true,
            tree_kept: None,
        }),
        130
    );
}
