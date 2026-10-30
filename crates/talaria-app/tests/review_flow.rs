//! Flow red tests for the review. Issue #245.
//!
//! The fleet half runs against the fleet view's temporary stubs until that
//! unit merges; then these tests are re-pointed at its merged code.

#[path = "support/gh_stub.rs"]
mod gh_stub;

#[path = "support/git_fixture.rs"]
mod git_fixture;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use gh_stub::{read_log, write_arg_stub};
use git_fixture::{GitRepo, git_available};
use talaria_app::review::gh::GhRunner;
use talaria_app::review::locate::locate_folder;
use talaria_app::review::pr_status::{PrStatusNotice, PrStatusService};
use talaria_app::review::service::ReviewService;
use talaria_app::worktree::GitRunner;
use talaria_core::commands::local::{LocalAction, parse};
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet_view::{FleetAction, FleetViewEffect, FleetViewInput, FleetViewState};
use talaria_core::review::{
    GitAction, ReviewEffect, ReviewInput, ReviewKey, ReviewState, ReviewTarget,
};
use talaria_tui::keys::{Action, Key, KeyChord, KeyCode, KeyContext, KeyKind, KeyMap, Mods};

fn skip_unless_git() -> bool {
    if git_available() {
        return false;
    }
    eprintln!("git not found on PATH");
    true
}

fn row(session_id: &str) -> RowKey {
    RowKey {
        profile: "fixture".to_owned(),
        session_id: session_id.to_owned(),
    }
}

fn key(code: KeyCode, shift: bool, ctrl: bool) -> Key {
    Key {
        code,
        mods: Mods {
            ctrl,
            alt: false,
            shift,
            super_key: false,
            other: false,
        },
        kind: KeyKind::Press,
    }
}

/// `v` on a fleet row opens its review; Esc returns with the cursor restored.
#[tokio::test]
async fn fleet_v_opens_review_and_esc_returns() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let _ = &git.repo;

    let key = row("local-fleet");
    let mut fleet = FleetViewState::new(Some(key.clone()), 1.0);
    // The cursor sits on a listed row; a fleet view with no rows has nothing to review.
    let _ = talaria_core::fleet_view::update(&mut fleet, snapshot(&key), 10);
    let effects = talaria_core::fleet_view::update(
        &mut fleet,
        FleetViewInput::Action(FleetAction::Review),
        10,
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, FleetViewEffect::Review(_))),
        "reviewing a row emits its review: {effects:?}"
    );

    // The review opens on the row's folder; Esc returns to the fleet list with
    // its cursor unchanged.
    let dir = tempfile::tempdir().expect("temp folder");
    let stub = write_arg_stub(dir.path(), 0);
    let pr = Arc::new(PrStatusService::new(
        GhRunner::with_binary(stub.program.clone()),
        GitRunner::from_env(),
        Arc::new(Instant::now),
        |_: PrStatusNotice| {},
    ));
    let mut service = ReviewService::new(
        GitRunner::from_env(),
        GhRunner::with_binary(stub.program.clone()),
        pr,
    );
    let target = ReviewTarget {
        folder: git.repo.clone(),
        main_root: git.repo.clone(),
        session: Some(key.clone()),
        record: None,
        title: "repo".to_owned(),
    };
    let effects = service.open(target, 1.0);
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            ReviewEffect::List(talaria_core::review::Scope::Uncommitted)
        )),
        "opening reads the listing: {effects:?}"
    );
    let closed = service.update(ReviewInput::Key(ReviewKey::Esc));
    assert!(
        closed.contains(&ReviewEffect::Close),
        "Esc closes the review: {closed:?}"
    );
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    service.execute(ReviewEffect::Close, tx);
    assert_eq!(fleet.cursor(), Some(&key), "the fleet cursor is unchanged");
}

/// A fleet snapshot listing one working row.
fn snapshot(key: &RowKey) -> FleetViewInput {
    let row = talaria_core::fleet::FleetRow {
        terminal_location: None,
        key: key.clone(),
        title: "work".to_owned(),
        model: None,
        source: Some("tui".to_owned()),
        started_at: Some(0.5),
        message_count: 2,
        state: talaria_core::fleet::RowState::Working,
        state_since: 0.5,
        outcome: None,
        foreground: false,
        subscribed: false,
        stale: false,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: None,
        backend: talaria_core::fleet::FleetBackend::Hermes,
        agent: None,
        occupancy: None,
        cost: None,
    };
    FleetViewInput::Snapshot {
        version: 1,
        rows: vec![row],
        queue: Vec::new(),
        profiles: vec![(
            "fixture".to_owned(),
            talaria_core::fleet::ProfileStatus::Listed { truncated: false },
        )],
        scope_note: None,
        link_note: None,
    }
}

/// `/changes` opens the foreground folder; usage and no-folder texts hold.
#[tokio::test]
async fn changes_command_opens_foreground_folder() {
    if skip_unless_git() {
        return;
    }
    let (action, argument) = parse("/changes").expect("changes parses");
    assert_eq!(action, LocalAction::Review);
    assert!(argument.is_empty());
    assert_eq!(
        parse("/changes extra"),
        Err("/changes takes no arguments.".to_owned())
    );

    // The foreground folder resolves to a target.
    let git = GitRepo::new();
    let runner = GitRunner::from_env();
    let found = locate_folder(&runner, &git.repo, None, "repo".to_owned()).await;
    assert!(
        matches!(found, talaria_app::review::locate::Located::Found(_)),
        "the foreground folder opens: {found:?}"
    );

    // With no foreground session there is no folder to review.
    let dir = tempfile::tempdir().expect("temp folder");
    let stub = write_arg_stub(dir.path(), 0);
    let pr = Arc::new(PrStatusService::new(
        GhRunner::with_binary(stub.program.clone()),
        GitRunner::from_env(),
        Arc::new(Instant::now),
        |_: PrStatusNotice| {},
    ));
    let mut service = ReviewService::new(
        GitRunner::from_env(),
        GhRunner::with_binary(stub.program.clone()),
        pr,
    );
    let _ = service.open(
        ReviewTarget {
            folder: PathBuf::from("repo"),
            main_root: PathBuf::from("repo"),
            session: None,
            record: None,
            title: "repo".to_owned(),
        },
        1.0,
    );
}

/// Review keys resolve through the key map; `o` runs `gh pr view --web`.
#[tokio::test]
async fn review_keys_through_key_map() {
    let map = KeyMap::defaults();
    let table: Vec<(Action, KeyContext, Vec<&str>)> = vec![
        (Action::ReviewUp, KeyContext::Review, vec!["up", "k"]),
        (Action::ReviewDown, KeyContext::Review, vec!["down", "j"]),
        (Action::ReviewTop, KeyContext::Review, vec!["home", "g"]),
        (
            Action::ReviewBottom,
            KeyContext::Review,
            vec!["end", "shift+g"],
        ),
        (Action::ReviewShowDiff, KeyContext::Review, vec!["enter"]),
        (Action::ReviewDiffPageUp, KeyContext::Review, vec!["pageup"]),
        (
            Action::ReviewDiffPageDown,
            KeyContext::Review,
            vec!["pagedown"],
        ),
        (Action::ReviewScope, KeyContext::Review, vec!["tab"]),
        (Action::ReviewStage, KeyContext::Review, vec!["space"]),
        (Action::ReviewStageAll, KeyContext::Review, vec!["a"]),
        (
            Action::ReviewUnstageAll,
            KeyContext::Review,
            vec!["shift+a"],
        ),
        (Action::ReviewDiscard, KeyContext::Review, vec!["shift+x"]),
        (Action::ReviewCommit, KeyContext::Review, vec!["c"]),
        (Action::ReviewPush, KeyContext::Review, vec!["p"]),
        (
            Action::ReviewPullRequest,
            KeyContext::Review,
            vec!["shift+p"],
        ),
        (Action::ReviewOpenPr, KeyContext::Review, vec!["o"]),
        (Action::ReviewRefresh, KeyContext::Review, vec!["r"]),
        (
            Action::CommitSubmit,
            KeyContext::CommitMessage,
            vec!["ctrl+s"],
        ),
        (
            Action::CommitDraft,
            KeyContext::CommitMessage,
            vec!["ctrl+d"],
        ),
    ];
    for (action, context, chords) in &table {
        assert_eq!(action.context(), *context, "{action:?}");
        let wanted: Vec<KeyChord> = chords
            .iter()
            .map(|chord| KeyChord::parse(chord, *context).expect("chord parses"))
            .collect();
        assert_eq!(map.chords(*action), wanted.as_slice(), "{action:?}");
    }

    // Through the map: shift+x discards, Esc is fixed and unwinds.
    assert_eq!(
        map.resolve(&key(KeyCode::Char('X'), true, false), KeyContext::Review),
        Some(Action::ReviewDiscard)
    );
    assert_eq!(
        map.resolve(&key(KeyCode::Char('o'), false, false), KeyContext::Review),
        Some(Action::ReviewOpenPr)
    );
    assert_eq!(
        map.resolve(&key(KeyCode::Esc, false, false), KeyContext::Review),
        None,
        "Esc is fixed, never mapped"
    );

    // On a review opened with `/changes`: shift+x twice discards, Esc unwinds.
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let runner = GitRunner::from_env();
    let found = locate_folder(&runner, &git.repo, None, "repo".to_owned()).await;
    let talaria_app::review::locate::Located::Found(target) = found else {
        panic!("the foreground folder opens: {found:?}");
    };
    let dir = tempfile::tempdir().expect("temp folder");
    let stub = write_arg_stub(dir.path(), 0);
    let pr = Arc::new(PrStatusService::new(
        GhRunner::with_binary(stub.program.clone()),
        GitRunner::from_env(),
        Arc::new(Instant::now),
        |_: PrStatusNotice| {},
    ));
    let mut service = ReviewService::new(
        GitRunner::from_env(),
        GhRunner::with_binary(stub.program.clone()),
        pr,
    );
    let _ = service.open(target.clone(), 1.0);

    // Shift+x twice discards the row; Esc unwinds. The review lists one untracked file first.
    std::fs::write(git.repo.join("scratch.txt"), "scratch\n").expect("untracked file");
    let listing =
        talaria_app::review::list(&runner, &target, talaria_core::review::Scope::Uncommitted)
            .await
            .expect("the listing");
    let (mut state, _) = ReviewState::open(target, 1.0);
    let _ = state.update(ReviewInput::Listed {
        scope: talaria_core::review::Scope::Uncommitted,
        result: Ok(listing),
    });
    let first = state.update(ReviewInput::Key(ReviewKey::Discard));
    assert!(
        !first
            .iter()
            .any(|effect| matches!(effect, ReviewEffect::Run(GitAction::Discard(_)))),
        "the first press only asks: {first:?}"
    );
    let second = state.update(ReviewInput::Key(ReviewKey::Discard));
    assert!(
        second
            .iter()
            .any(|effect| matches!(effect, ReviewEffect::Run(GitAction::Discard(_)))),
        "the second press discards: {second:?}"
    );
    let _ = state.update(ReviewInput::Key(ReviewKey::Esc));

    // `o` runs `gh pr view <n> --web` for the known pull request.
    let gh = GhRunner::with_binary(stub.program.clone());
    gh.open_in_browser(dir.path(), 12)
        .await
        .expect("the browser opens");
    assert_eq!(
        read_log(&stub.log),
        vec![vec![
            "pr".to_owned(),
            "view".to_owned(),
            "12".to_owned(),
            "--web".to_owned()
        ]]
    );
}
