//! Fleet notices, the off-screen launch, the three-launch limit, and the fleet keys.
//! Step 0 accepts a launch and tells nobody, and the fleet keys stay unbound.

#[path = "support/git_fixture.rs"]
mod git_fixture;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use mock_gateway::StandaloneGateway;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_app::bg::{
    LaunchNotice, LaunchOrigin, LaunchRequest, LaunchService, Stage, WorktreeWanted,
};
use talaria_app::fleet::FleetHandle;
use talaria_app::worktree::GitRunner;
use talaria_core::fleet_view::{
    FleetAction, FleetViewEffect, FleetViewInput, FleetViewState, update,
};
use talaria_tui::keys::{Action, Key, KeyCode, KeyContext, KeyKind, KeyMap, Mods};
use talaria_tui::launch_dialog::{DialogDefaults, DialogMode, LaunchDialog};

const PROMPT: &str = "Line one is here\tand\nline two continues with enough words to pass sixty";
const NOTICE: &str =
    "Launched in fixture: \"Line one is here and line two continues with enough words to…\"";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn request(worktree: bool) -> LaunchRequest {
    LaunchRequest {
        prompt: PROMPT.to_owned(),
        profile: "fixture".to_owned(),
        folder: PathBuf::from("repo"),
        model: None,
        provider: None,
        effort: None,
        worktree: worktree.then_some(WorktreeWanted { name: None }),
        origin: LaunchOrigin::FleetView,
    }
}

fn service(
    home: PathBuf,
    git: GitRunner,
    notices: &Arc<Mutex<Vec<LaunchNotice>>>,
) -> LaunchService {
    let slot = Arc::clone(notices);
    LaunchService::new(home, git, move |notice| lock(&slot).push(notice))
}

fn wait_notices(notices: &Mutex<Vec<LaunchNotice>>, limit: Duration) -> Vec<LaunchNotice> {
    let deadline = Instant::now() + limit;
    loop {
        let current = lock(notices).clone();
        if !current.is_empty() || Instant::now() >= deadline {
            break current;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn defaults() -> DialogDefaults {
    DialogDefaults {
        profiles: vec!["fixture".to_owned()],
        profile: "fixture".to_owned(),
        profile_note: None,
        folder: PathBuf::from("repo"),
        draft: None,
        model_placeholder: "profile default".to_owned(),
        slug: |text| text.to_owned(),
    }
}

#[test]
fn fleet_is_told_and_row_appears() {
    let home = tempfile::tempdir().expect("home");
    let notices = Arc::new(Mutex::new(Vec::new()));
    let fleet = FleetHandle::new();
    let untouched = FleetHandle::new();
    let service = service(home.path().to_path_buf(), GitRunner::from_env(), &notices);
    service
        .launch(request(false), "fixture".to_owned(), Some(fleet.clone()))
        .expect("accepted");
    let got = wait_notices(&notices, Duration::from_millis(400));
    assert!(!got.is_empty(), "no notice within 400ms");
    let text = got.iter().find_map(|notice| match notice {
        LaunchNotice::Finished { outcome, .. } => Some(talaria_app::bg::notice_text(outcome)),
        LaunchNotice::Progress { .. } => None,
    });
    assert_eq!(text.as_deref(), Some(NOTICE));

    service
        .launch(request(true), "fixture".to_owned(), Some(fleet.clone()))
        .expect("accepted");
    let worktree_notice =
        format!("{NOTICE} (worktree talaria/line-one-is-here-and-line-two-continues)");
    let got = wait_notices(&notices, Duration::from_millis(400));
    let found = got.iter().any(|notice| match notice {
        LaunchNotice::Finished { outcome, .. } => {
            talaria_app::bg::notice_text(outcome) == worktree_notice
        }
        LaunchNotice::Progress { .. } => false,
    });
    assert!(found, "missing worktree notice");

    let deadline = Instant::now() + Duration::from_secs(1);
    let mut rows = fleet.snapshot().rows;
    while rows.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        rows = fleet.snapshot().rows;
    }
    assert!(!rows.is_empty(), "no row within 1s");
    assert!(fleet.calls().contains(&"observe_subscribed"));

    service
        .launch(request(false), "fixture".to_owned(), None)
        .expect("accepted");
    assert!(
        untouched.calls().is_empty(),
        "a launch with no fleet handle called the registry"
    );
}

#[test]
fn launch_runs_off_the_ui_task() {
    let home = tempfile::tempdir().expect("home");
    let program = git_fixture::slow_fetch_git(home.path(), 5);
    let notices = Arc::new(Mutex::new(Vec::new()));
    let service = service(
        home.path().to_path_buf(),
        GitRunner::with_program(program),
        &notices,
    );
    service
        .launch(request(true), "fixture".to_owned(), None)
        .expect("accepted");
    let got = wait_notices(&notices, Duration::from_millis(400));
    let progress = got.iter().any(|notice| {
        matches!(
            notice,
            LaunchNotice::Progress {
                stage: Stage::Fetching,
                ..
            }
        )
    });
    assert!(progress, "no progress notice");

    let mut dialog = LaunchDialog::open(DialogMode::Plain, defaults());
    let area = Rect::new(0, 0, 80, 24);
    let mut buffer = Buffer::empty(area);
    let chord = talaria_tui::keys::KeyChord::parse("tab", KeyContext::Fleet).expect("tab");
    let mut samples = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let _ = dialog.handle_key(chord.clone());
        dialog.draw(dialog.area(area), &mut buffer);
        samples.push(started.elapsed());
    }
    samples.sort();
    let p95 = samples[18];
    assert!(p95 <= Duration::from_millis(16), "keystroke p95 {p95:?}");
}

#[tokio::test]
async fn three_launches_at_most() {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.hold("session.create");
    let notices = Arc::new(Mutex::new(Vec::new()));
    let service = service(home.path().to_path_buf(), GitRunner::from_env(), &notices);
    for _ in 0..3 {
        service
            .launch(request(false), "fixture".to_owned(), None)
            .expect("accepted");
    }
    let fourth = service.launch(request(false), "fixture".to_owned(), None);
    assert!(
        matches!(fourth, Err(talaria_app::bg::Busy)),
        "expected Busy, got {fourth:?}"
    );
    let started = Instant::now();
    let left = service.shutdown(Duration::from_secs(5)).await;
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "shutdown exceeded 5s"
    );
    gateway.release("session.create");
    let got = wait_notices(&notices, Duration::from_millis(400));
    let interrupted = got.iter().filter(|notice| match notice {
        LaunchNotice::Finished { outcome, .. } => {
            matches!(outcome, talaria_app::bg::LaunchOutcome::Interrupted { .. })
        }
        LaunchNotice::Progress { .. } => false,
    });
    assert!(interrupted.count() >= 3, "held launches were not cancelled");
    assert!(left.len() <= 3);
}

#[test]
fn n_and_w_open_the_dialog_from_the_fleet_view() {
    let map = KeyMap::defaults();
    assert_eq!(
        map.label(Action::FleetNew).as_deref(),
        Some("N"),
        "n is unbound"
    );
    assert_eq!(
        map.label(Action::FleetNewWorktree).as_deref(),
        Some("W"),
        "w is unbound"
    );
    let key = Key {
        code: KeyCode::Char('n'),
        mods: Mods::default(),
        kind: KeyKind::Press,
    };
    assert_eq!(map.resolve(&key, KeyContext::Fleet), Some(Action::FleetNew));

    let screen = Rect::new(0, 0, 80, 24);
    let mut buffer = Buffer::empty(screen);
    let mut plain = LaunchDialog::open(DialogMode::Plain, defaults());
    plain.draw(plain.area(screen), &mut buffer);
    let worktree = LaunchDialog::open(DialogMode::Worktree, defaults());
    worktree.draw(worktree.area(screen), &mut buffer);
    let drawn = buffer_text(&buffer, screen);
    assert!(
        drawn.contains("New background session"),
        "dialog was not drawn"
    );

    let esc = talaria_tui::keys::KeyChord::parse("esc", KeyContext::Fleet).expect("esc");
    let action = plain.handle_key(esc);
    assert!(matches!(
        action,
        talaria_tui::launch_dialog::DialogAction::Close { .. }
    ));
    let mut state = FleetViewState::new(None, 0.0);
    let effects = update(&mut state, FleetViewInput::Action(FleetAction::New), 8);
    assert!(!effects.contains(&FleetViewEffect::Close));
    let _ = update(&mut state, FleetViewInput::Action(FleetAction::Filter), 8);
    let typed = update(&mut state, FleetViewInput::Text('n'), 8);
    assert!(
        !typed.contains(&FleetViewEffect::Close),
        "n in the filter opened a dialog"
    );
}

fn buffer_text(buffer: &Buffer, area: Rect) -> String {
    let mut lines = Vec::new();
    for y in area.y..area.y.saturating_add(area.height) {
        let mut line = String::new();
        let mut x = area.x;
        while x < area.x.saturating_add(area.width) {
            line.push_str(buffer[(x, y)].symbol());
            x = x.saturating_add(1);
        }
        lines.push(line);
    }
    lines.join("\n")
}
