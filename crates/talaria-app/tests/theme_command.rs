use std::collections::BTreeMap;
use std::fs;

use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::appearance::{THEME_USAGE, ThemeFileSession, theme_file_lines, theme_lines};
use talaria_app::config::{Config, parse};
use talaria_core::appearance::{
    AppearanceChange, ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource,
    ThemeFileReport, ThemeReport,
};
use tempfile::TempDir;

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

fn fixture(config: &str) -> (TempDir, ThemeFileSession) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("themes")).unwrap();
    fs::write(dir.path().join("config.toml"), config).unwrap();
    for id in ["global", "profile", "session"] {
        fs::write(
            dir.path().join(format!("themes/{id}.toml")),
            format!("schema = 1\nname = '{id}'\nextends = 'dark'\n[palette]\naccent = '#ffb86b'\n"),
        )
        .unwrap();
    }
    let loaded = parse(config);
    assert!(loaded.notices.is_empty(), "{:?}", loaded.notices);
    let session = ThemeFileSession::new(
        dir.path().join("config.toml"),
        loaded.config.theme,
        "fixture",
    );
    (dir, session)
}

#[test]
fn selection_order() {
    let (_dir, mut session) =
        fixture("[theme]\nfile='global'\n[theme.profiles.fixture]\nfile='profile'\n");
    assert_eq!(session.selected_id(), Some("profile"));
    assert!(session.startup().is_empty());
    assert_eq!(session.active_id(), Some("profile"));
    let (lines, change) = session.command("use session");
    assert_eq!(change, AppearanceChange::Changed, "{lines:?}");
    assert_eq!(session.selected_id(), Some("session"));
    assert!(session.set_profile("default"));
    assert_eq!(session.selected_id(), Some("session"));
    session.command("use");
    assert_eq!(session.active_id(), Some("global"));
    let (_dir, empty) = fixture("");
    assert_eq!(empty.selected_id(), None);
}

#[test]
fn use_without_id_clears_the_session_choice() {
    let (_dir, mut session) = fixture("[theme]\nfile='global'\n");
    session.startup();
    session.command("use session");
    assert_eq!(session.active_id(), Some("session"));
    session.command("use");
    assert_eq!(session.active_id(), Some("global"));
    let (_dir, mut session) = fixture("");
    session.command("use session");
    session.command("use");
    assert_eq!(session.active_id(), None);
}

#[test]
fn use_leaves_config_unchanged() {
    let (dir, mut session) = fixture("[theme]\nfile='global'\n");
    let path = dir.path().join("config.toml");
    let before = fs::read(&path).unwrap();
    let (_, change) = session.command("use session");
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(session.active_id(), Some("session"));
    session.command("reload");
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn invalid_reload_keeps_last_good() {
    let (dir, mut session) = fixture("[theme]\nfile='global'\n");
    session.startup();
    let before = session.file().unwrap().clone();
    fs::write(
        dir.path().join("themes/global.toml"),
        "schema=1\n[palette]\naccent='#12345'\n",
    )
    .unwrap();
    let (lines, change) = session.command("reload");
    assert_eq!(change, AppearanceChange::Unchanged);
    assert_eq!(session.file(), Some(&before));
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("themes/global.toml"));
    assert!(
        lines[0].contains("accent") && lines[0].contains("line 3") && lines[0].contains("column"),
        "{lines:?}"
    );
}

#[test]
fn valid_reload_reads_the_file_again() {
    let (dir, mut session) = fixture("[theme]\nfile='global'\n");
    session.startup();
    fs::write(
        dir.path().join("themes/global.toml"),
        "schema=1\nname='changed'\nextends='light'\n",
    )
    .unwrap();
    let (_, change) = session.command("reload");
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(session.file().unwrap().extends.as_str(), "light");
}

#[test]
fn failed_use_keeps_the_active_selection() {
    let (_dir, mut session) = fixture("[theme]\nfile='global'\n");
    session.startup();
    let before = session.file().unwrap().clone();
    let (lines, change) = session.command("use missing");
    assert_eq!(change, AppearanceChange::Unchanged);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("themes/missing.toml"));
    assert_eq!(session.active_id(), Some("global"));
    assert_eq!(session.selected_id(), Some("global"));
    assert_eq!(session.file(), Some(&before));
}

#[test]
fn invalid_startup_file_falls_back() {
    let (dir, mut session) = fixture("[theme]\nfile='global'\n");
    fs::write(
        dir.path().join("themes/global.toml"),
        "schema=1\n[unknown]\nx=true\n",
    )
    .unwrap();
    let lines = session.startup();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("themes/global.toml") && lines[0].contains("unknown"));
    assert!(lines[0].contains("line") && lines[0].contains("column"));
    assert!(session.file().is_none());
}

#[test]
fn bad_arguments_report_usage_without_changing_the_active_file() {
    let (_dir, mut session) = fixture("[theme]\nfile='global'\n");
    session.startup();
    for arg in [
        "purple",
        "use ../outside",
        "use session extra",
        "reload extra",
    ] {
        let (lines, change) = session.command(arg);
        assert_eq!(lines, [THEME_USAGE], "{arg}");
        assert_eq!(change, AppearanceChange::Unchanged);
        assert_eq!(session.active_id(), Some("global"));
    }
    assert!(THEME_USAGE.contains("use") && THEME_USAGE.contains("reload"));
}

#[test]
fn file_commands_route_to_effects() {
    for command in ["/theme use session", "/theme use", "/theme reload"] {
        let mut state = AppState::new("fixture");
        let effects = update(
            &mut state,
            AppMessage::User(UserAction::Submit(command.into())),
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::ThemeFileCommand(_))),
            "{command}"
        );
    }
}

#[test]
fn file_report_lists_identity_sources_and_warnings() {
    let report = ThemeFileReport {
        file: "global".into(),
        base: "dark".into(),
        skin_participates: true,
        sources: BTreeMap::from([(
            "composer.border.focused".into(),
            BTreeMap::from([("fg".into(), "palette.accent".into())]),
        )]),
        warnings: vec!["picker.row.selected is reserved".into()],
        unknown_contrast: vec!["transcript.text".into()],
    };
    let text = theme_file_lines(&report).join("\n");
    for value in [
        "global",
        "dark",
        "Hermes",
        "composer.border.focused",
        "palette.accent",
        "picker.row.selected",
        "unknown",
    ] {
        assert!(text.contains(value), "missing {value}: {text}");
    }
}

#[test]
fn no_file_retains_the_five_report_lines() {
    let report = ThemeReport {
        skin: None,
        not_applied: None,
        polarity: (Polarity::Dark, PolaritySource::Default),
        depth: (ColorDepth::TrueColor, DepthSource::Config),
        motion: (Motion::Full, MotionSource::Default),
        ignored: vec![],
    };
    assert_eq!(theme_lines(&report).len(), 5);
    let session = ThemeFileSession::new("config.toml".into(), Config::default().theme, "default");
    assert!(session.selected_id().is_none());
}

enum Observation {
    Appearance(Option<String>),
    Lines(String),
}

struct ThemeScreen {
    files: ThemeFileSession,
    report: Option<ThemeFileReport>,
    observed: tokio::sync::mpsc::UnboundedSender<Observation>,
}

impl talaria_app::app::ComposerDeadline for ThemeScreen {
    fn tick(&mut self, _: std::time::Instant) {}
    fn next_tick(&self) -> Option<std::time::Instant> {
        None
    }
}
impl talaria_app::app::StatusHost for ThemeScreen {
    fn on_tick(&mut self, _: std::time::Instant) {}
    fn next_wake(&self, _: std::time::Instant) -> Option<std::time::Instant> {
        None
    }
    async fn shutdown(&mut self) {}
}
impl talaria_app::app::Screen for ThemeScreen {
    type Input = UserAction;
    fn input(
        &mut self,
        input: UserAction,
        _: &talaria_app::app::View<'_>,
        _: std::time::Instant,
    ) -> Vec<UserAction> {
        vec![input]
    }
    fn draw(&mut self, _: &talaria_app::app::View<'_>) {}
    fn commit_context(&mut self, _: &talaria_app::context::ContextBlock) {}
    fn commit_cells(&mut self, _: &[talaria_core::session::Cell]) {}
    fn load_draft(&mut self, _: &str) {}
    fn restore(&mut self) {}
    fn commit_lines(&mut self, lines: Vec<String>) {
        let _ = self.observed.send(Observation::Lines(lines.join("\n")));
    }
    fn startup_theme_files(&self) -> Option<ThemeFileSession> {
        Some(self.files.clone())
    }
    fn appearance_changed(&mut self, view: &talaria_app::appearance::AppearanceView<'_>) {
        let resolved = talaria_tui::theme::resolve_with_theme_file(
            &talaria_tui::theme::ThemeInputs {
                skin: view.skin,
                skipped: view.skipped,
                follow_hermes_skin: true,
                polarity: view.polarity,
                depth: (ColorDepth::TrueColor, DepthSource::Config),
                motion: (Motion::Full, MotionSource::Default),
                terminal: Default::default(),
            },
            view.file,
        );
        self.report = resolved.report;
        if let Some(report) = &mut self.report {
            report.file = view.file_id.unwrap_or_default().to_owned();
        }
        let _ = self.observed.send(Observation::Appearance(
            view.file.and_then(|file| file.name.clone()),
        ));
    }
    fn theme_report(&self) -> Option<ThemeReport> {
        Some(ThemeReport {
            skin: None,
            not_applied: None,
            polarity: (Polarity::Dark, PolaritySource::Default),
            depth: (ColorDepth::TrueColor, DepthSource::Config),
            motion: (Motion::Full, MotionSource::Default),
            ignored: vec![],
        })
    }
    fn theme_file_report(&self) -> Option<ThemeFileReport> {
        self.report.clone()
    }
}

async fn observed(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<Observation>,
    predicate: impl Fn(&Observation) -> bool,
) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(event) = receiver.recv().await {
            if predicate(&event) {
                return;
            }
        }
        panic!("application observation channel closed");
    })
    .await
    .expect("application must emit the requested theme observation");
}

#[test]
fn application_loop_applies_theme_files_and_reports_reload_results() {
    use talaria_app::app;
    use talaria_app::launch::{Launch, StartMode};
    let (dir, mut files) = fixture("[theme]\nfile='global'\n");
    assert!(files.startup().is_empty());
    let gateway = mock_gateway::StandaloneGateway::start(dir.path());
    gateway.block_on(async {
        let (tx, inputs) = tokio::sync::mpsc::unbounded_channel();
        let (events, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut screen = ThemeScreen { files, report: None, observed: events };
        let launch = Launch {
            mode: StartMode::New, model: None, provider: None, effort: None,
            profile: "fixture".into(), folder: dir.path().to_path_buf(), record: None,
            bench_replay: None, no_ensure: true,
        };
        let setup = app::RunSetup {
            remote_gateway: false, herdr: None, home: dir.path().to_path_buf(),
            launch_id: "theme-file-loop".into(), preopened: None, list_title: None,
            notices: vec![], keys: Default::default(), fleet: None,
        };
        let script = async {
            observed(&mut receiver, |event| matches!(event, Observation::Appearance(Some(name)) if name == "global")).await;
            tx.send(UserAction::Submit("/theme use session".into())).unwrap();
            observed(&mut receiver, |event| matches!(event, Observation::Appearance(Some(name)) if name == "session")).await;
            fs::write(dir.path().join("themes/session.toml"), "schema=1\nname='changed'\nextends='light'\n").unwrap();
            tx.send(UserAction::Submit("/theme reload".into())).unwrap();
            observed(&mut receiver, |event| matches!(event, Observation::Appearance(Some(name)) if name == "changed")).await;
            tx.send(UserAction::Submit("/theme".into())).unwrap();
            observed(&mut receiver, |event| matches!(event, Observation::Lines(lines) if lines.contains("session") && lines.contains("light") && lines.contains("Sources"))).await;
            fs::write(dir.path().join("themes/session.toml"), "schema=1\n[roles.text]\nbold='bad'\n").unwrap();
            tx.send(UserAction::Submit("/theme reload".into())).unwrap();
            observed(&mut receiver, |event| matches!(event, Observation::Lines(lines) if lines.contains("themes/session.toml") && lines.contains("bold") && lines.contains("line 3"))).await;
            tx.send(UserAction::Quit).unwrap();
        };
        let (outcome, ()) = tokio::join!(app::run(launch, setup, &mut screen, inputs), script);
        assert_eq!(outcome.code, 0, "{}", outcome.stderr);
        assert_eq!(screen.report.unwrap().base, "light");
    });
}
