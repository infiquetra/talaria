//! Configuration notices and the interactive launch's fleet attach.

#[path = "support/directory_drive.rs"]
mod directory_drive;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::{Duration, Instant};

use directory_drive::{Log, Probe, Rig, eventually, locked};
use mock_gateway::StandaloneGateway;
use talaria_app::app::{UserAction, run};
use talaria_app::config::parse;
use talaria_app::fleet::{
    FleetConfig, FleetService, ForegroundReport, attaches_after_first_frame, foreground_reports,
};
use talaria_app::launch::StartMode;
use talaria_core::fleet::{RowKey, SubscribedTurn, SubscribedView};

#[tokio::test]
async fn app_wiring_and_config() {
    let loaded = parse(
        "[fleet]\nenabled = \"yes\"\nhot_poll_ms = 50\ncold_poll_ms = 999999\nbackstop_s = 1\n",
    );
    let notices = loaded.notices.join("\n");
    assert!(
        notices.contains("[fleet] enabled must be true or false; using true."),
        "{notices}"
    );
    assert!(
        notices
            .contains("[fleet] hot_poll_ms must be a whole number from 1000 to 10000; using 1000."),
        "{notices}"
    );
    assert!(
        notices.contains(
            "[fleet] cold_poll_ms must be a whole number from 2000 to 30000; using 30000."
        ),
        "{notices}"
    );
    assert!(
        notices.contains("[fleet] backstop_s must be a whole number from 10 to 300; using 10."),
        "{notices}"
    );
    assert!(loaded.config.fleet.enabled);
    assert_eq!(loaded.config.fleet.hot_poll, Duration::from_millis(1000));
    assert_eq!(loaded.config.fleet.cold_poll, Duration::from_millis(30_000));
    assert_eq!(loaded.config.fleet.backstop, Duration::from_secs(10));

    let chosen = parse("[fleet]\nenabled = false\nhot_poll_ms = 1500\n");
    assert!(chosen.notices.is_empty(), "{:?}", chosen.notices);
    assert!(!chosen.config.fleet.enabled);
    assert_eq!(chosen.config.fleet.hot_poll, Duration::from_millis(1500));

    let defaults = FleetConfig::default();
    assert!(defaults.enabled);
    assert_eq!(defaults.hot_poll, Duration::from_secs(2));
    assert_eq!(defaults.cold_poll, Duration::from_secs(4));
    assert_eq!(defaults.backstop, Duration::from_secs(30));
    assert_eq!(defaults.roster_gap, Duration::from_secs(3));
    assert_eq!(defaults.liveness_gap, Duration::from_secs(1));
    assert_eq!(defaults.coalesce, Duration::from_millis(250));
    assert_eq!(defaults.call_timeout, Duration::from_secs(10));
    assert_eq!(defaults.max_in_flight, 8);
    assert_eq!(defaults.suspect_in_flight, 2);
    let scaled = FleetConfig::scaled_for_tests();
    assert_eq!(scaled.hot_poll, Duration::from_millis(100));
    assert_eq!(scaled.cold_poll, Duration::from_millis(200));
    assert_eq!(scaled.backstop, Duration::from_millis(1500));
    assert_eq!(scaled.coalesce, Duration::from_micros(12_500));
    assert_eq!(scaled.max_in_flight, 8);

    let home = tempfile::tempdir().expect("a temporary home");
    let gateway = StandaloneGateway::start(home.path());
    assert_eq!(
        gateway.websocket_connections(),
        0,
        "no fleet connection before the first frame"
    );
    assert!(
        !attaches_after_first_frame(true, true),
        "print mode does not attach"
    );
    assert!(
        !attaches_after_first_frame(false, false),
        "a disabled setting does not attach"
    );
    let handle = FleetService::start(
        home.path().to_path_buf(),
        "fixture".into(),
        FleetConfig::default(),
        |_| {},
    );
    let started = Instant::now();
    handle.close().await;
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "close returns within 200 ms when the mock never answers"
    );
    assert_eq!(
        gateway
            .ticket_records()
            .iter()
            .filter(|ticket| ticket.scope == "host")
            .count(),
        0,
        "print mode mints no host ticket"
    );
    assert!(
        attaches_after_first_frame(false, true),
        "an interactive launch with the fleet enabled attaches after the first frame"
    );
}

fn view(turn: SubscribedTurn) -> SubscribedView {
    SubscribedView {
        turn,
        needs: Vec::new(),
        activity: None,
        cost_usd: None,
        model: None,
        title: None,
    }
}

fn row(session: &str) -> RowKey {
    RowKey {
        profile: "fixture".into(),
        session_id: session.into(),
    }
}

#[test]
fn foreground_reports_follow_the_view_and_the_switch() {
    let running = view(SubscribedTurn::Running { started_at: None });
    let idle = view(SubscribedTurn::Idle);
    let first = (row("s1"), running.clone());
    assert_eq!(
        foreground_reports(None, Some(&first)),
        vec![
            ForegroundReport::Foreground(Some(row("s1"))),
            ForegroundReport::Subscribed(row("s1"), running.clone()),
        ],
        "the first foreground session is named and reported"
    );
    assert!(
        foreground_reports(Some(&first), Some(&first)).is_empty(),
        "an unchanged view sends nothing"
    );
    let changed = (row("s1"), idle.clone());
    assert_eq!(
        foreground_reports(Some(&first), Some(&changed)),
        vec![ForegroundReport::Subscribed(row("s1"), idle.clone())],
        "a changed view is sent once"
    );
    let switched = (row("s2"), running.clone());
    assert_eq!(
        foreground_reports(Some(&changed), Some(&switched)),
        vec![
            ForegroundReport::Foreground(Some(row("s2"))),
            ForegroundReport::Unsubscribed(row("s1")),
            ForegroundReport::Subscribed(row("s2"), running),
        ],
        "a switch names the new foreground and lets the old one go"
    );
}

/// Run one interactive launch with these fleet settings until the session opens and `hold`
/// returns, then quit. Returns the screen log and how long the loop took to finish after quit.
async fn launch_once(
    rig: &Rig,
    fleet: Option<FleetConfig>,
    hold: impl Fn(&StandaloneGateway) -> bool,
) -> (Log, Duration) {
    let log = Log::default();
    let mut screen = Probe { log: log.clone() };
    let mut setup = rig.setup();
    setup.fleet = fleet;
    let (inputs, receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut quit_at = None;
    let script = async {
        let opened = log.clone();
        eventually("the session to open", move || {
            locked(&opened).session.is_some()
        })
        .await;
        eventually("the fleet condition", || hold(&rig.gateway)).await;
        quit_at = Some(Instant::now());
        inputs.send(UserAction::Quit).expect("the loop is running");
    };
    let _ = tokio::join!(
        run(rig.launch(StartMode::New), setup, &mut screen, receiver),
        script
    );
    let after = quit_at.map(|at| at.elapsed()).unwrap_or_default();
    (log, after)
}

fn host_tickets(gateway: &StandaloneGateway) -> Vec<Instant> {
    gateway
        .ticket_records()
        .into_iter()
        .filter(|ticket| ticket.scope == "host")
        .map(|ticket| ticket.at)
        .collect()
}

#[tokio::test]
async fn the_interactive_launch_runs_the_fleet_after_its_first_frame() {
    let rig = Rig::start();
    // The fleet reads never answer, so a close that waited for them would take 10 s.
    for profile in ["default", "fixture"] {
        rig.gateway
            .set_silent_profile(profile, "session.active_list", true);
    }
    let (log, after_quit) = launch_once(&rig, Some(FleetConfig::default()), |gateway| {
        !host_tickets(gateway).is_empty()
            && gateway
                .requests("session.active_list")
                .iter()
                .any(|seen| seen.params["profile"] == "fixture")
    })
    .await;
    let first_frame = locked(&log).draw_at[0];
    let tickets = host_tickets(&rig.gateway);
    assert_eq!(tickets.len(), 1, "one host-scoped fleet ticket");
    assert!(
        tickets.iter().all(|at| *at >= first_frame),
        "no fleet ticket before the first frame"
    );
    assert!(locked(&log).restored, "the loop finished");
    assert!(
        after_quit < Duration::from_secs(3),
        "quitting does not wait for the fleet reads: {after_quit:?}"
    );

    let quiet = Rig::start();
    let (log, _) = launch_once(&quiet, None, |_| true).await;
    assert!(locked(&log).restored, "the loop finished");
    assert!(
        host_tickets(&quiet.gateway).is_empty(),
        "a launch without the fleet mints no host ticket"
    );
    assert!(
        !attaches_after_first_frame(true, true),
        "print mode does not run the fleet"
    );
}
