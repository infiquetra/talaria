//! Timing red tests for the review. Issue #245.

#[path = "support/gh_stub.rs"]
mod gh_stub;

#[path = "support/git_fixture.rs"]
mod git_fixture;

use std::sync::Arc;
use std::time::{Duration, Instant};

use gh_stub::write_arg_stub;
use git_fixture::{GitRepo, git_available};
use talaria_app::review::gh::GhRunner;
use talaria_app::review::pr_status::{PrDemand, PrStatusNotice, PrStatusService};
use talaria_app::review::service::ReviewService;
use talaria_app::worktree::GitRunner;
use talaria_core::review::{ReviewTarget, Scope};

fn skip_unless_git() -> bool {
    if git_available() {
        return false;
    }
    eprintln!("git not found on PATH");
    true
}

fn service(stub: &std::path::Path) -> ReviewService {
    let pr = Arc::new(PrStatusService::new(
        GhRunner::with_binary(stub.to_owned()),
        GitRunner::from_env(),
        Arc::new(Instant::now),
        |_: PrStatusNotice| {},
    ));
    ReviewService::new(
        GitRunner::from_env(),
        GhRunner::with_binary(stub.to_owned()),
        pr,
    )
}

/// Open to first drawn list frame with 200 changed files: at most 300 ms.
#[tokio::test]
async fn open_to_first_list_frame_200_files() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    for index in 0..200 {
        std::fs::write(git.repo.join(format!("file-{index:03}.txt")), "change\n")
            .expect("change file");
    }
    let dir = tempfile::tempdir().expect("temp folder");
    let stub = write_arg_stub(dir.path(), 0);
    let mut service = service(&stub.program);

    let target = ReviewTarget {
        folder: git.repo.clone(),
        main_root: git.repo.clone(),
        session: None,
        record: None,
        title: "repo".to_owned(),
    };
    let started = Instant::now();
    let effects = service.open(target.clone(), 1.0);
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            talaria_core::review::ReviewEffect::List(Scope::Uncommitted)
        )),
        "opening reads the listing: {effects:?}"
    );
    // The listing the effect asks for, with real git, then the first frame that draws it.
    let listing = talaria_app::review::list(&GitRunner::from_env(), &target, Scope::Uncommitted)
        .await
        .expect("the listing");
    assert_eq!(listing.total, 200, "all 200 files are listed");
    let _ = service.update(talaria_core::review::ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(listing),
    });
    let theme = talaria_tui::tool_cell::ToolTheme::default();
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).expect("terminal");
    let view = service.view().expect("the review is open");
    terminal
        .draw(|frame| {
            talaria_tui::review_view::draw_review(&view, &theme, frame.area(), frame.buffer_mut());
        })
        .expect("draw");
    let elapsed = started.elapsed();
    let drawn: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(drawn.contains("file-000.txt"), "the list is drawn");
    println!("open to first list frame: {} ms", elapsed.as_millis());
    assert!(
        elapsed <= Duration::from_millis(300),
        "over budget: {} ms",
        elapsed.as_millis()
    );
}

/// Keystrokes stay in budget while `gh` sleeps: 16 ms at the 95th percentile,
/// 33 ms worst.
#[tokio::test]
async fn keystroke_budget_while_gh_sleeps() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    git.ok(&git.repo, &["checkout", "-q", "-b", "talaria/work"]);
    let dir = tempfile::tempdir().expect("temp folder");
    let stub = write_arg_stub(dir.path(), 2);
    let pr = Arc::new(PrStatusService::new(
        GhRunner::with_binary(stub.program.clone()),
        GitRunner::from_env(),
        Arc::new(Instant::now),
        |_: PrStatusNotice| {},
    ));
    pr.want(PrDemand::Review(git.repo.clone(), true));
    pr.tick();

    let mut service = ReviewService::new(
        GitRunner::from_env(),
        GhRunner::with_binary(stub.program.clone()),
        pr,
    );
    let target = ReviewTarget {
        folder: git.repo.clone(),
        main_root: git.repo.clone(),
        session: None,
        record: None,
        title: "repo".to_owned(),
    };
    let _ = service.open(target.clone(), 1.0);

    // One cursor key is a reducer update plus a drawn frame, timed while the
    // status refresh still sleeps.
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use talaria_core::review::{
        BranchHead, ChangedFile, FileStatus, Listing, ReviewInput, ReviewKey, ReviewState,
    };
    use talaria_tui::review_view::draw_review;
    use talaria_tui::tool_cell::ToolTheme;

    let (mut state, _) = ReviewState::open(target, 1.0);
    let files: Vec<ChangedFile> = (0..200)
        .map(|index| ChangedFile {
            path: format!("file-{index:03}.txt"),
            orig_path: None,
            status: FileStatus::Modified,
            staged: false,
            unstaged: true,
            added: Some(1),
            removed: Some(0),
            is_dir: false,
        })
        .collect();
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(Listing {
            head: BranchHead {
                oid: Some("abc".to_owned()),
                head: Some("talaria/work".to_owned()),
                upstream: Some("origin/talaria/work".to_owned()),
                ahead: 0,
                behind: 0,
            },
            total: files.len(),
            files,
            base: None,
            default_branch: Some("main".to_owned()),
            has_origin: true,
            hooks: 0,
        }),
    });
    let theme = ToolTheme::default();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
    let mut samples: Vec<Duration> = Vec::with_capacity(200);
    for index in 0..200 {
        let started = Instant::now();
        let _ = state.update(ReviewInput::Key(if index % 2 == 0 {
            ReviewKey::Down
        } else {
            ReviewKey::Up
        }));
        let view = state.view();
        terminal
            .draw(|frame| draw_review(&view, &theme, frame.area(), frame.buffer_mut()))
            .expect("draw");
        samples.push(started.elapsed());
    }
    samples.sort();
    let worst = samples.last().copied().unwrap_or_default();
    let p95 = samples[(samples.len() as f64 * 0.95) as usize];
    println!(
        "keystroke p95: {} ms, worst: {} ms",
        p95.as_millis(),
        worst.as_millis()
    );
    assert!(
        p95 <= Duration::from_millis(16),
        "p95 over budget: {} ms",
        p95.as_millis()
    );
    assert!(
        worst <= Duration::from_millis(33),
        "worst over budget: {} ms",
        worst.as_millis()
    );
}
