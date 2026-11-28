//! The `/theme` and `/skin` selectors through the application loop: the report prints, then a
//! list with descriptions opens; moving previews, Enter applies and Esc or quitting restores.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde_json::Value;
use talaria_app::app::{self, UserAction};
use talaria_app::appearance::ThemeFileSession;
use talaria_app::config::parse;
use talaria_app::launch::{Launch, StartMode};
use talaria_app::theme_file::{BUILTIN_THEMES, builtin_description};
use talaria_core::appearance::{
    ColorDepth, DepthSource, Motion, MotionSource, NotAppliedReason, Polarity, PolaritySource,
    ThemeReport,
};
use talaria_core::commands::{ChoiceKey, RowKind};
use tempfile::TempDir;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::mock_gateway::{MockGateway, StandaloneGateway};

/// What the selector shows: each row as `name | description`, the selected entry and the note.
#[derive(Clone, Debug, PartialEq)]
struct Shown {
    rows: Vec<String>,
    selected: Option<String>,
    note: Option<String>,
}

#[derive(Debug)]
enum Seen {
    Lines(String),
    Appearance {
        file: Option<String>,
        polarity: Polarity,
        skin: Option<String>,
        /// The look is a `/theme` preview, not the one in force.
        previewing: bool,
    },
    Choice(Option<Shown>),
}

struct ChoiceScreen {
    files: ThemeFileSession,
    not_applied: Option<NotAppliedReason>,
    shown: Option<Shown>,
    seen: UnboundedSender<Seen>,
}

impl app::ComposerDeadline for ChoiceScreen {
    fn tick(&mut self, _: std::time::Instant) {}
    fn next_tick(&self) -> Option<std::time::Instant> {
        None
    }
}

impl app::StatusHost for ChoiceScreen {
    fn on_tick(&mut self, _: std::time::Instant) {}
    fn next_wake(&self, _: std::time::Instant) -> Option<std::time::Instant> {
        None
    }
    async fn shutdown(&mut self) {}
}

impl app::Screen for ChoiceScreen {
    type Input = UserAction;
    fn input(
        &mut self,
        input: UserAction,
        _: &app::View<'_>,
        _: std::time::Instant,
    ) -> Vec<UserAction> {
        vec![input]
    }
    fn draw(&mut self, view: &app::View<'_>) {
        let shown = view.choice.map(|list| Shown {
            rows: list
                .rows()
                .iter()
                .map(|row| match row.kind {
                    RowKind::Command => format!("{} | {}", row.name, row.description),
                    _ => row.name.clone(),
                })
                .collect(),
            selected: list.selected().map(|item| item.id.clone()),
            note: list.note().map(str::to_owned),
        });
        if shown != self.shown {
            self.shown.clone_from(&shown);
            let _ = self.seen.send(Seen::Choice(shown));
        }
    }
    fn commit_context(&mut self, _: &talaria_app::context::ContextBlock) {}
    fn commit_cells(&mut self, _: &[talaria_core::session::Cell]) {}
    fn load_draft(&mut self, _: &str) {}
    fn restore(&mut self) {}
    fn commit_lines(&mut self, lines: Vec<String>) {
        let _ = self.seen.send(Seen::Lines(lines.join("\n")));
    }
    fn startup_theme_files(&self) -> Option<ThemeFileSession> {
        Some(self.files.clone())
    }
    fn appearance_changed(&mut self, view: &talaria_app::appearance::AppearanceView<'_>) {
        let _ = self.seen.send(Seen::Appearance {
            file: view.file_id.map(str::to_owned),
            polarity: view.polarity.0,
            skin: view.skin.map(|skin| skin.name.clone()),
            previewing: view.previewing,
        });
    }
    fn theme_report(&self) -> Option<ThemeReport> {
        Some(ThemeReport {
            skin: Some("default".into()),
            not_applied: self.not_applied,
            polarity: (Polarity::Dark, PolaritySource::Default),
            depth: (ColorDepth::TrueColor, DepthSource::Config),
            motion: (Motion::Full, MotionSource::Default),
            ignored: vec![],
        })
    }
}

fn skin_data(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../talaria-tui/tests/data/skins")
        .join(name);
    serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap()
}

/// A configuration folder with `[theme] file = 'global'` and two user files.
fn home() -> (TempDir, ThemeFileSession) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("themes")).unwrap();
    let config = "[theme]\nfile = 'global'\n";
    fs::write(dir.path().join("config.toml"), config).unwrap();
    for (id, name) in [("global", "Global"), ("quiet-amber", "Quiet amber")] {
        fs::write(
            dir.path().join(format!("themes/{id}.toml")),
            format!("schema = 1\nname = '{name}'\nextends = 'dark'\n"),
        )
        .unwrap();
    }
    let mut files = ThemeFileSession::new(
        dir.path().join("config.toml"),
        parse(config).config.theme,
        "fixture",
    );
    assert!(files.startup().is_empty());
    (dir, files)
}

async fn wait_for(
    receiver: &mut UnboundedReceiver<Seen>,
    what: &str,
    test: impl Fn(&Seen) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(seen) = receiver.recv().await {
            if test(&seen) {
                return;
            }
        }
        panic!("the loop stopped before {what}");
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

fn choice(seen: &Seen) -> Option<&Shown> {
    match seen {
        Seen::Choice(Some(shown)) => Some(shown),
        _ => None,
    }
}

fn selected(id: &'static str) -> impl Fn(&Seen) -> bool {
    move |seen| choice(seen).is_some_and(|shown| shown.selected.as_deref() == Some(id))
}

fn closed(seen: &Seen) -> bool {
    matches!(seen, Seen::Choice(None))
}

fn lines_with(text: &'static str) -> impl Fn(&Seen) -> bool {
    move |seen| matches!(seen, Seen::Lines(lines) if lines.contains(text))
}

/// Run the loop against a mock gateway while `script` drives it, then return the gateway.
fn run_with<F, Fut>(
    not_applied: Option<NotAppliedReason>,
    prepare: impl Fn(&StandaloneGateway),
    script: F,
) -> (StandaloneGateway, TempDir)
where
    F: FnOnce(UnboundedSender<UserAction>, UnboundedReceiver<Seen>, PathBuf) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let (dir, files) = home();
    let gateway = StandaloneGateway::start(dir.path());
    prepare(&gateway);
    gateway.block_on(async {
        let (tx, inputs) = unbounded_channel();
        let (seen, receiver) = unbounded_channel();
        let mut screen = ChoiceScreen {
            files,
            not_applied,
            shown: None,
            seen,
        };
        let launch = Launch {
            mode: StartMode::New,
            model: None,
            provider: None,
            effort: None,
            profile: "fixture".into(),
            folder: dir.path().to_path_buf(),
            record: None,
            bench_replay: None,
            no_ensure: true,
        };
        let setup = app::RunSetup {
            remote_gateway: false,
            remote_host: None,
            herdr: None,
            home: dir.path().to_path_buf(),
            launch_id: "choice-loop".into(),
            preopened: None,
            list_title: None,
            notices: vec![],
            keys: Default::default(),
            fleet: None,
        };
        let (outcome, ()) = tokio::join!(
            app::run(launch, setup, &mut screen, inputs),
            script(tx, receiver, dir.path().to_path_buf())
        );
        assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    });
    (gateway, dir)
}

fn key(tx: &UnboundedSender<UserAction>, key: ChoiceKey) {
    tx.send(UserAction::Choice(key)).unwrap();
}

/// A copy of the gateway for the script, taken while the gateway is prepared.
type GatewaySlot = Arc<OnceLock<MockGateway>>;

fn keep(slot: &GatewaySlot, gateway: &StandaloneGateway) {
    let _ = slot.set(MockGateway::clone(gateway));
}

/// Poll the gateway until `test` holds.
async fn until(gateway: &MockGateway, what: &str, test: impl Fn(&MockGateway) -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !test(gateway) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

/// Receive until `last` is seen, failing on anything `forbidden` matches before it.
async fn none_before(
    receiver: &mut UnboundedReceiver<Seen>,
    what: &str,
    forbidden: impl Fn(&Seen) -> bool,
    last: impl Fn(&Seen) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(seen) = receiver.recv().await {
            assert!(!forbidden(&seen), "unexpected before {what}: {seen:?}");
            if last(&seen) {
                return;
            }
        }
        panic!("the loop stopped before {what}");
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

#[test]
fn bare_theme_opens_selector_with_builtins_and_user_files() {
    run_with(
        None,
        |_| {},
        |tx, mut seen, _| async move {
            wait_for(&mut seen, "the start-up theme", |seen| {
            matches!(seen, Seen::Appearance { file: Some(file), .. } if file == "global")
        })
        .await;
            tx.send(UserAction::Submit("/theme".into())).unwrap();
            wait_for(&mut seen, "the selector", |seen| {
                let mut expected = vec![
                    "Theme".to_owned(),
                    "Configured (current) | Follow config.toml (now themes/global.toml)".to_owned(),
                ];
                expected.extend(
                    BUILTIN_THEMES.map(|id| format!("{id} | {}", builtin_description(id).unwrap())),
                );
                expected.extend(
                    [
                        "global | Theme file themes/global.toml: Global",
                        "quiet-amber | Theme file themes/quiet-amber.toml: Quiet amber",
                        "Background",
                        "light | Use a light terminal background",
                        "dark | Use a dark terminal background",
                        "auto (current) | Follow configuration and terminal detection",
                    ]
                    .map(str::to_owned),
                );
                choice(seen).is_some_and(|shown| {
                    shown.rows == expected && shown.selected.as_deref() == Some("file:")
                })
            })
            .await;
            key(&tx, ChoiceKey::Cancel);
            wait_for(&mut seen, "the selector to close", closed).await;
            tx.send(UserAction::Quit).unwrap();
        },
    );
}

#[test]
fn theme_preview_changes_appearance_and_escape_restores_it() {
    run_with(
        None,
        |_| {},
        |tx, mut seen, dir| async move {
            wait_for(&mut seen, "the start-up theme", |seen| {
            matches!(seen, Seen::Appearance { file: Some(file), .. } if file == "global")
        })
        .await;
            let before = fs::read(dir.join("config.toml")).unwrap();
            tx.send(UserAction::Submit("/theme".into())).unwrap();
            wait_for(&mut seen, "the selector", selected("file:")).await;
            key(&tx, ChoiceKey::Next);
            key(&tx, ChoiceKey::Next);
            wait_for(
                &mut seen,
                "the ember preview",
                |seen| matches!(seen, Seen::Appearance { file: Some(file), previewing: true, .. } if file == "ember"),
            )
            .await;
            // Move past the remaining built-ins and two custom themes to the first Background row.
            for _ in 0..=BUILTIN_THEMES.len() {
                key(&tx, ChoiceKey::Next);
            }
            wait_for(&mut seen, "the light preview", |seen| {
            matches!(seen, Seen::Appearance { file: Some(file), polarity: Polarity::Light, .. } if file == "global")
        })
        .await;
            key(&tx, ChoiceKey::Cancel);
            wait_for(&mut seen, "the restored theme", |seen| {
            matches!(seen, Seen::Appearance { file: Some(file), polarity: Polarity::Dark, previewing: false, .. } if file == "global")
        })
        .await;
            // Nothing reached the configuration file, and the session still follows it.
            assert_eq!(fs::read(dir.join("config.toml")).unwrap(), before);
            assert!(!dir.join("theme-selection.json").exists());
            assert!(!dir.join("theme-selection.lock").exists());
            tx.send(UserAction::Submit("/theme".into())).unwrap();
            wait_for(&mut seen, "the reopened selector", selected("file:")).await;
            key(&tx, ChoiceKey::Cancel);
            tx.send(UserAction::Quit).unwrap();
        },
    );
}

#[test]
fn theme_enter_saves_the_file_for_the_next_launch() {
    run_with(
        None,
        |_| {},
        |tx, mut seen, dir| async move {
            wait_for(&mut seen, "the start-up theme", |seen| {
            matches!(seen, Seen::Appearance { file: Some(file), .. } if file == "global")
        })
        .await;
            let before = fs::read(dir.join("config.toml")).unwrap();
            tx.send(UserAction::Submit("/theme".into())).unwrap();
            wait_for(&mut seen, "the selector", selected("file:")).await;
            for ch in "amber".chars() {
                key(&tx, ChoiceKey::Type(ch));
            }
            wait_for(&mut seen, "the filtered list", selected("file:quiet-amber")).await;
            key(&tx, ChoiceKey::Apply);
            wait_for(
                &mut seen,
                "the saved confirmation",
                lines_with("Theme choice saved for this profile."),
            )
            .await;
            assert_eq!(fs::read(dir.join("config.toml")).unwrap(), before);
            assert_eq!(reopen(&dir).active_id(), Some("quiet-amber"));
            tx.send(UserAction::Submit("/theme".into())).unwrap();
            wait_for(&mut seen, "the reopened selector", |seen| {
                choice(seen).is_some_and(|shown| {
                    shown.selected.as_deref() == Some("file:quiet-amber")
                        && shown
                            .rows
                            .iter()
                            .any(|row| row.starts_with("quiet-amber (current) | "))
                })
            })
            .await;
            key(&tx, ChoiceKey::Cancel);
            tx.send(UserAction::Quit).unwrap();
        },
    );
}

#[test]
fn theme_enter_on_a_file_that_cannot_load_keeps_the_previous_choice() {
    run_with(
        None,
        |_| {},
        |tx, mut seen, dir| async move {
            wait_for(&mut seen, "the start-up theme", |seen| {
                matches!(seen, Seen::Appearance { file: Some(file), .. } if file == "global")
            })
            .await;
            fs::write(dir.join("themes/dracula.toml"), "schema = [\n").unwrap();
            tx.send(UserAction::Submit("/theme".into())).unwrap();
            wait_for(&mut seen, "the selector", selected("file:")).await;
            for ch in "dracula".chars() {
                key(&tx, ChoiceKey::Type(ch));
            }
            wait_for(&mut seen, "the broken row", |seen| {
                choice(seen).is_some_and(|shown| {
                    shown.selected.as_deref() == Some("file:dracula")
                        && shown
                            .rows
                            .iter()
                            .filter(|row| row.starts_with("dracula | "))
                            .count()
                            == 1
                        && shown.rows.iter().any(|row| {
                            row.starts_with("dracula | Cannot be used: themes/dracula.toml: syntax")
                        })
                })
            })
            .await;
            key(&tx, ChoiceKey::Apply);
            // `/theme use dracula` reports the custom file's load error.
            none_before(
                &mut seen,
                "the load error",
                lines_with("Theme choice saved"),
                lines_with("themes/dracula.toml: syntax"),
            )
            .await;
            assert!(!dir.join("theme-selection.json").exists());
            tx.send(UserAction::Quit).unwrap();
        },
    );
}

fn reopen(dir: &std::path::Path) -> ThemeFileSession {
    let path = dir.join("config.toml");
    let config = talaria_app::config::load_file(&path).config.theme;
    let mut files = ThemeFileSession::new(path, config, "fixture");
    assert!(files.startup().is_empty());
    files
}

#[test]
fn background_choices_save_and_auto_restores_startup() {
    run_with(
        None,
        |_| {},
        |tx, mut seen, dir| async move {
            tx.send(UserAction::Submit("/theme".into())).unwrap();
            wait_for(&mut seen, "the selector", selected("file:")).await;
            for _ in 0..BUILTIN_THEMES.len() + 3 {
                key(&tx, ChoiceKey::Next);
            }
            wait_for(&mut seen, "the light preview", selected("background:light")).await;
            assert!(!dir.join("theme-selection.json").exists());
            key(&tx, ChoiceKey::Apply);
            wait_for(
                &mut seen,
                "confirmation",
                lines_with("light background saved"),
            )
            .await;
            let startup = (Polarity::Dark, PolaritySource::Default);
            assert_eq!(
                reopen(&dir).polarity(startup),
                (Polarity::Light, PolaritySource::Command)
            );
            let environment = (Polarity::Dark, PolaritySource::Environment);
            assert_eq!(reopen(&dir).polarity(environment), environment);

            tx.send(UserAction::Submit("/theme auto".into())).unwrap();
            wait_for(
                &mut seen,
                "restored background",
                lines_with("Background detection restored"),
            )
            .await;
            assert_eq!(reopen(&dir).polarity(startup), startup);
            tx.send(UserAction::Quit).unwrap();
        },
    );
}

#[test]
fn accepted_theme_commands_survive_an_immediate_quit() {
    let (_, dir) = run_with(
        None,
        |_| {},
        |tx, _seen, _| async move {
            tx.send(UserAction::Submit("/theme use quiet-amber".into()))
                .unwrap();
            tx.send(UserAction::Submit("/theme light".into())).unwrap();
            tx.send(UserAction::Quit).unwrap();
        },
    );
    let files = reopen(dir.path());
    assert_eq!(files.active_id(), Some("quiet-amber"));
    assert_eq!(
        files.polarity((Polarity::Dark, PolaritySource::Default)),
        (Polarity::Light, PolaritySource::Command),
    );
}

fn skins(gateway: &StandaloneGateway) {
    gateway.set_skin_catalog(HashMap::from([
        ("default".to_owned(), skin_data("default.json")),
        ("ares".to_owned(), skin_data("ares.json")),
    ]));
    gateway.set_skin_names(&["ares", "default", "mono"]);
    gateway.set_skin_descriptions(&[
        ("ares", "Synthetic red and bronze"),
        ("default", "Synthetic gold"),
        ("mono", "Synthetic greys"),
    ]);
}

fn skin_sets(gateway: &MockGateway) -> Vec<String> {
    gateway
        .requests("config.set")
        .iter()
        .filter(|seen| seen.params["key"] == "skin")
        .map(|seen| seen.params["value"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// The launch describe applied the profile's skin, so the session is connected.
async fn connected(seen: &mut UnboundedReceiver<Seen>) {
    wait_for(
        seen,
        "the launch skin",
        |seen| matches!(seen, Seen::Appearance { skin: Some(skin), .. } if skin == "default"),
    )
    .await;
}

const PREVIEW_NOTE: &str = "Previewing changes the Hermes profile's skin until you cancel.";

#[test]
fn bare_skin_reports_then_opens_selector_with_hermes_descriptions() {
    let (gateway, _dir) = run_with(None, skins, |tx, mut seen, _| async move {
        connected(&mut seen).await;
        tx.send(UserAction::Submit("/skin".into())).unwrap();
        wait_for(
            &mut seen,
            "the report",
            lines_with("Skin: default (Hermes profile fixture)"),
        )
        .await;
        wait_for(&mut seen, "the selector", |seen| {
            choice(seen).is_some_and(|shown| {
                shown.rows
                    == [
                        "Skins Hermes has",
                        "ares | Synthetic red and bronze",
                        "default (current) | Synthetic gold",
                        "mono | Synthetic greys",
                    ]
                    && shown.selected.as_deref() == Some("default")
                    && shown.note.as_deref() == Some(PREVIEW_NOTE)
            })
        })
        .await;
        key(&tx, ChoiceKey::Cancel);
        wait_for(&mut seen, "the selector to close", closed).await;
        tx.send(UserAction::Quit).unwrap();
    });
    // Opening and cancelling without moving writes nothing.
    assert!(skin_sets(&gateway).is_empty());
}

#[test]
fn skin_preview_sets_only_after_selection_rests_and_escape_restores() {
    let (gateway, _dir) = run_with(None, skins, |tx, mut seen, _| async move {
        connected(&mut seen).await;
        tx.send(UserAction::Submit("/skin".into())).unwrap();
        wait_for(&mut seen, "the selector", selected("default")).await;
        // Three moves in a burst end on ares; only the resting selection is written.
        key(&tx, ChoiceKey::Next);
        key(&tx, ChoiceKey::Prev);
        key(&tx, ChoiceKey::Prev);
        wait_for(&mut seen, "the ares selection", selected("ares")).await;
        wait_for(
            &mut seen,
            "the ares preview",
            |seen| matches!(seen, Seen::Appearance { skin: Some(skin), .. } if skin == "ares"),
        )
        .await;
        key(&tx, ChoiceKey::Cancel);
        wait_for(
            &mut seen,
            "the restore notice",
            lines_with("Skin preview cancelled; the profile's skin is \"default\" again."),
        )
        .await;
        tx.send(UserAction::Quit).unwrap();
    });
    assert_eq!(skin_sets(&gateway), ["ares", "default"]);
}

#[test]
fn skin_enter_keeps_the_selection_through_the_skin_command() {
    let (gateway, _dir) = run_with(None, skins, |tx, mut seen, _| async move {
        connected(&mut seen).await;
        tx.send(UserAction::Submit("/skin".into())).unwrap();
        wait_for(&mut seen, "the selector", selected("default")).await;
        key(&tx, ChoiceKey::Prev);
        key(&tx, ChoiceKey::Apply);
        wait_for(&mut seen, "the selector to close", closed).await;
        wait_for(
            &mut seen,
            "the confirmation",
            lines_with("Skin set to \"ares\" for profile fixture."),
        )
        .await;
        tx.send(UserAction::Quit).unwrap();
    });
    // Enter before the selection rested sends only the kept skin, never a stale preview.
    assert_eq!(skin_sets(&gateway), ["ares"]);
    assert!(!skin_sets(&gateway).iter().any(|name| name == "default"));
}

#[test]
fn skin_preview_refused_stops_previewing_with_footer_reason() {
    let (gateway, _dir) = run_with(
        None,
        |gateway| {
            skins(gateway);
            gateway.script_error("config.set", None, -32001, "permission_denied");
        },
        |tx, mut seen, _| async move {
            connected(&mut seen).await;
            tx.send(UserAction::Submit("/skin".into())).unwrap();
            wait_for(&mut seen, "the selector", selected("default")).await;
            key(&tx, ChoiceKey::Prev);
            wait_for(&mut seen, "the refusal in the footer", |seen| {
                choice(seen).is_some_and(|shown| {
                    shown.note.as_deref()
                        == Some("Preview stopped: Hermes refused to change the skin (permission_denied). Enter still applies.")
                })
            })
            .await;
            // Later moves no longer write.
            key(&tx, ChoiceKey::Prev);
            wait_for(&mut seen, "the next selection", selected("mono")).await;
            tokio::time::sleep(Duration::from_millis(600)).await;
            key(&tx, ChoiceKey::Cancel);
            wait_for(&mut seen, "the selector to close", closed).await;
            tx.send(UserAction::Quit).unwrap();
        },
    );
    assert_eq!(gateway.requests("config.set").len(), 1);
}

#[test]
fn skin_selector_says_when_theme_ignores_skins() {
    run_with(
        Some(NotAppliedReason::FollowOff),
        skins,
        |tx, mut seen, _| async move {
            connected(&mut seen).await;
            tx.send(UserAction::Submit("/skin".into())).unwrap();
            wait_for(&mut seen, "the selector", |seen| {
            choice(seen).is_some_and(|shown| {
                shown.note.as_deref()
                    == Some("Skins do not show under the current theme (follow_hermes_skin = false); previewing still changes the Hermes profile's skin until you cancel.")
            })
        })
        .await;
            key(&tx, ChoiceKey::Cancel);
            tx.send(UserAction::Quit).unwrap();
        },
    );
}

#[test]
fn quit_during_skin_preview_restores_original() {
    let (gateway, _dir) = run_with(None, skins, |tx, mut seen, _| async move {
        connected(&mut seen).await;
        tx.send(UserAction::Submit("/skin".into())).unwrap();
        wait_for(&mut seen, "the selector", selected("default")).await;
        key(&tx, ChoiceKey::Prev);
        wait_for(
            &mut seen,
            "the ares preview",
            |seen| matches!(seen, Seen::Appearance { skin: Some(skin), .. } if skin == "ares"),
        )
        .await;
        tx.send(UserAction::Quit).unwrap();
    });
    assert_eq!(skin_sets(&gateway), ["ares", "default"]);
}

/// How long the mock takes to answer each `config.set` in the in-flight tests. Requests are
/// still read, and recorded, at once.
const SLOW_SET: Duration = Duration::from_millis(1500);

#[test]
fn skin_escape_while_preview_write_in_flight_restores_after_it_answers() {
    let slot = GatewaySlot::default();
    let kept = slot.clone();
    let (gateway, _dir) = run_with(
        None,
        move |gateway| {
            skins(gateway);
            keep(&kept, gateway);
        },
        |tx, mut seen, _| async move {
            let gateway = slot.get().expect("the gateway").clone();
            connected(&mut seen).await;
            gateway.set_delay("config.set", Some(SLOW_SET));
            tx.send(UserAction::Submit("/skin".into())).unwrap();
            wait_for(&mut seen, "the selector", selected("default")).await;
            key(&tx, ChoiceKey::Prev);
            until(&gateway, "the ares preview write", |gateway| {
                skin_sets(gateway) == ["ares"]
            })
            .await;
            // Esc before the preview write answers: the original waits for that answer, so a
            // late answer cannot leave the profile on the preview.
            key(&tx, ChoiceKey::Cancel);
            wait_for(&mut seen, "the selector to close", closed).await;
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert_eq!(skin_sets(&gateway), ["ares"]);
            wait_for(
                &mut seen,
                "the restore notice",
                lines_with("Skin preview cancelled; the profile's skin is \"default\" again."),
            )
            .await;
            tx.send(UserAction::Quit).unwrap();
        },
    );
    assert_eq!(skin_sets(&gateway), ["ares", "default"]);
}

#[test]
fn skin_enter_while_preview_write_in_flight_keeps_selection_after_it_answers() {
    let slot = GatewaySlot::default();
    let kept = slot.clone();
    let (gateway, _dir) = run_with(
        None,
        move |gateway| {
            skins(gateway);
            keep(&kept, gateway);
        },
        |tx, mut seen, _| async move {
            let gateway = slot.get().expect("the gateway").clone();
            connected(&mut seen).await;
            gateway.set_delay("config.set", Some(SLOW_SET));
            tx.send(UserAction::Submit("/skin".into())).unwrap();
            wait_for(&mut seen, "the selector", selected("default")).await;
            key(&tx, ChoiceKey::Prev);
            until(&gateway, "the ares preview write", |gateway| {
                skin_sets(gateway) == ["ares"]
            })
            .await;
            key(&tx, ChoiceKey::Prev);
            wait_for(&mut seen, "the mono selection", selected("mono")).await;
            key(&tx, ChoiceKey::Apply);
            wait_for(&mut seen, "the selector to close", closed).await;
            // Past the rest, before the preview answers: nothing more is written yet.
            tokio::time::sleep(Duration::from_millis(600)).await;
            assert_eq!(skin_sets(&gateway), ["ares"]);
            until(&gateway, "the kept skin", |gateway| {
                skin_sets(gateway).len() == 2
            })
            .await;
            tx.send(UserAction::Quit).unwrap();
        },
    );
    assert_eq!(skin_sets(&gateway), ["ares", "mono"]);
}

#[test]
fn quit_while_an_entered_skin_waits_writes_the_choice_not_the_original() {
    let slot = GatewaySlot::default();
    let kept = slot.clone();
    let (gateway, _dir) = run_with(
        None,
        move |gateway| {
            skins(gateway);
            keep(&kept, gateway);
        },
        |tx, mut seen, _| async move {
            let gateway = slot.get().expect("the gateway").clone();
            connected(&mut seen).await;
            gateway.set_delay("config.set", Some(SLOW_SET));
            tx.send(UserAction::Submit("/skin".into())).unwrap();
            wait_for(&mut seen, "the selector", selected("default")).await;
            key(&tx, ChoiceKey::Prev);
            until(&gateway, "the ares preview write", |gateway| {
                skin_sets(gateway) == ["ares"]
            })
            .await;
            key(&tx, ChoiceKey::Prev);
            wait_for(&mut seen, "the mono selection", selected("mono")).await;
            key(&tx, ChoiceKey::Apply);
            wait_for(&mut seen, "the selector to close", closed).await;
            // Quit before the preview write answers: Enter made mono the person's choice.
            tx.send(UserAction::Quit).unwrap();
        },
    );
    assert_eq!(skin_sets(&gateway), ["ares", "mono"]);
}

#[test]
fn skin_selector_stays_closed_when_typing_began_before_the_list_arrived() {
    let slot = GatewaySlot::default();
    let kept = slot.clone();
    run_with(
        None,
        move |gateway| {
            skins(gateway);
            keep(&kept, gateway);
        },
        |tx, mut seen, _| async move {
            let gateway = slot.get().expect("the gateway").clone();
            connected(&mut seen).await;
            let asked = gateway.requests("complete.slash").len();
            gateway.hold("complete.slash");
            tx.send(UserAction::Submit("/skin".into())).unwrap();
            until(&gateway, "the held skin list", |gateway| {
                gateway.requests("complete.slash").len() > asked
            })
            .await;
            // The next command is being typed when the list arrives.
            tx.send(UserAction::DraftEdited {
                text: "/q".into(),
                caret: 2,
                typed: true,
                recalled: false,
                now: std::time::Instant::now(),
            })
            .unwrap();
            tx.send(UserAction::Notice("draft typed".into())).unwrap();
            wait_for(&mut seen, "the draft", lines_with("draft typed")).await;
            gateway.release("complete.slash");
            let opened = |seen: &Seen| matches!(seen, Seen::Choice(Some(_)));
            none_before(
                &mut seen,
                "the report",
                opened,
                lines_with("Skin: default (Hermes profile fixture)"),
            )
            .await;
            tx.send(UserAction::Notice("after the report".into()))
                .unwrap();
            none_before(
                &mut seen,
                "the later notice",
                opened,
                lines_with("after the report"),
            )
            .await;
            tx.send(UserAction::Quit).unwrap();
        },
    );
}
