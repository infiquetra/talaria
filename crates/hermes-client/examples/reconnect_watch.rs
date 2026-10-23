//! Stream one session through a fault relay and show how the link recovers.
//!
//! ```text
//! cargo run -q -p hermes-client --example reconnect_watch -- \
//!     --profile fixture --model <model> --provider <provider> --prompt "<text>"
//! ```
//!
//! Options: `--home <dir>`, `--profile <name>`, `--model`, `--provider`, `--prompt <text>`,
//! `--resume <session id>`. Standard input takes `drop`, `blackhole`, `refuse`, `restore` and
//! `quit`. The example sends no `unattended` option, and it prints no ticket, no path, and no
//! profile name of its own.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

use hermes_client::liveness::{DialFailure, GatewayDial, Link, LinkConfig, LinkEvent, LinkState};
use hermes_client::replay::Cursor;
use hermes_client::{Client, DiscoverOptions, EnsurePolicy, Inbound, default_home, discover_with};
use hermes_contract::events::{EventFrame, Notification};
use hermes_contract::methods::canonical::{prompt_submit, session_create, session_resume};
use hermes_contract::overrides::prompt_submit_params;
use hermes_contract::schemas::{CanonicalCreateParams, CanonicalResumeParams, CanonicalSnapshot};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::Instant;

#[path = "../tests/support/fault_relay.rs"]
mod fault_relay;

use fault_relay::FaultRelay;

struct Args {
    home: Option<PathBuf>,
    profile: String,
    model: Option<String>,
    provider: Option<String>,
    prompt: Option<String>,
    resume: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = parse_args();
    let home = args.home.clone().unwrap_or_else(default_home);
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(err) => start_failed(&DialFailure::from(err)),
    };
    let target: SocketAddr = match endpoint.api_origin().trim_start_matches("http://").parse() {
        Ok(target) => target,
        Err(_) => {
            println!("START FAILED reason=the gateway's address is not a loopback port");
            println!("next=Check the gateway with `hermes gateway status`.");
            exit(1);
        }
    };
    let relay = FaultRelay::start(target)
        .await
        .unwrap_or_else(|_| fail("the fault relay could not bind a loopback port"));
    let dial = GatewayDial::new(home, args.profile.clone()).via_origin(relay.origin().to_string());
    let (link, mut rx) = match Link::start(dial, LinkConfig::default()).await {
        Ok(started) => started,
        Err(failure) => start_failed(&failure),
    };

    let client = link.client();
    let scratch = tempfile::tempdir().unwrap_or_else(|_| fail("no temporary folder"));
    let snapshot = match &args.resume {
        Some(id) => resume(&client, id).await,
        None => {
            let params = CanonicalCreateParams {
                cwd: Some(scratch.path().to_string_lossy().into_owned()),
                model: args.model.clone(),
                provider: args.provider.clone(),
                ..CanonicalCreateParams::default()
            };
            client.call::<session_create::Marker>(params).await
        }
    }
    .unwrap_or_else(|err| {
        println!("START FAILED reason={err}");
        println!("next={}", err.next_step());
        exit(1);
    });
    let mut watch = Watch::new(&snapshot);
    link.track(&watch.session, cursor_of(&snapshot));

    if let Some(prompt) = &args.prompt {
        let params =
            prompt_submit_params(watch.session.clone(), prompt.clone(), "reconnect-watch-1");
        if let Err(err) = client.call::<prompt_submit::Marker>(params).await {
            watch.line(&format!("SUBMIT FAILED reason={err}"));
        }
    }

    let mut stdin = BufReader::new(tokio::io::stdin()).lines();
    let mut stdin_open = true;
    loop {
        let report_at = watch.report_at;
        let report = async {
            match report_at {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            event = rx.recv() => match event {
                Some(event) => watch.handle(event, &link, &client).await,
                None => break,
            },
            line = stdin.next_line(), if stdin_open => match line {
                Ok(Some(command)) => match command.trim() {
                    "drop" => { watch.line("RELAY drop"); relay.drop() }
                    "blackhole" => { watch.line("RELAY blackhole"); relay.blackhole() }
                    "refuse" => { watch.line("RELAY refuse"); relay.refuse() }
                    "restore" => { watch.line("RELAY restore"); relay.restore() }
                    "quit" => break,
                    "" => {}
                    _ => watch.line("UNKNOWN COMMAND (drop, blackhole, refuse, restore, quit)"),
                },
                _ => stdin_open = false,
            },
            () = report => watch.flush_replayed(),
        }
    }

    link.close().await;
    while let Some(event) = rx.recv().await {
        if matches!(event, LinkEvent::State(LinkState::Closed)) {
            watch.line("STATE closed");
            return;
        }
    }
    watch.line("STATE closed");
}

/// What the example has seen and printed so far.
struct Watch {
    session: String,
    check: SeqCheck,
    mid_line: bool,
    replayed: u64,
    replay_pending: bool,
    report_at: Option<Instant>,
}

impl Watch {
    fn new(snapshot: &CanonicalSnapshot) -> Self {
        let mut check = SeqCheck::default();
        check.restart(&cursor_of(snapshot));
        Self {
            session: snapshot.session_id.clone(),
            check,
            mid_line: false,
            replayed: 0,
            replay_pending: false,
            report_at: None,
        }
    }

    /// One output line; streamed text that did not end in a newline is closed off first.
    fn line(&mut self, text: &str) {
        if self.mid_line {
            println!();
            self.mid_line = false;
        }
        println!("{text}");
    }

    fn flush_replayed(&mut self) {
        self.report_at = None;
        if self.replay_pending {
            self.replay_pending = false;
            let line = format!("REPLAYED {} events", self.replayed);
            self.line(&line);
            self.replayed = 0;
        }
    }

    async fn handle(&mut self, event: LinkEvent, link: &Link, client: &Client) {
        match event {
            LinkEvent::State(LinkState::Connected { generation }) => {
                self.line(&format!("STATE connected generation={generation}"));
            }
            LinkEvent::State(LinkState::Reconnecting {
                attempt,
                retry_in,
                reason,
            }) => self.line(&format!(
                "STATE reconnecting attempt={attempt} retry_in={:.1}s reason={reason}",
                retry_in.as_secs_f64()
            )),
            LinkEvent::State(LinkState::Failed { reason, next_step }) => {
                self.line(&format!("STATE failed reason={reason} next={next_step}"));
            }
            LinkEvent::State(LinkState::Closed) => self.line("STATE closed"),
            LinkEvent::Reattached { session_id, .. } => {
                if session_id == self.session {
                    self.line("REATTACHED");
                    self.replay_pending = true;
                    self.replayed = 0;
                    self.report_at = Some(Instant::now() + Duration::from_secs(1));
                }
            }
            LinkEvent::SessionGone { .. } => self.line("SESSION GONE"),
            LinkEvent::Gap(gap) => {
                self.line(&format!("GAP reason={}", gap.reason.as_str()));
                self.resume_after_gap(link, client, &gap.session_id).await;
            }
            LinkEvent::Inbound {
                item: Inbound::Event(event),
                replayed,
            } => {
                if event.frame.session_id.as_deref() != Some(self.session.as_str()) {
                    return;
                }
                if replayed {
                    self.replayed += 1;
                    self.report_at = Some(Instant::now() + Duration::from_millis(300));
                } else {
                    self.flush_replayed();
                }
                self.check.see(&event.frame);
                self.show(&event.frame);
            }
            LinkEvent::Inbound {
                item: Inbound::ServerRequest(request),
                ..
            } => {
                let _ = request
                    .responder
                    .fail(-32000, "declined by reconnect_watch")
                    .await;
            }
            LinkEvent::Inbound { .. } => {}
        }
    }

    /// The consumer's part of the contract: resume, then resync with the snapshot's cursor.
    async fn resume_after_gap(&mut self, link: &Link, client: &Client, id: &str) {
        match resume(client, id).await {
            Ok(snapshot) => {
                let cursor = cursor_of(&snapshot);
                if snapshot.session_id == id {
                    link.resync(id, cursor.clone());
                } else {
                    link.untrack(id);
                    link.track(&snapshot.session_id, cursor.clone());
                    self.session = snapshot.session_id.clone();
                }
                self.check.restart(&cursor);
                self.line(&format!("RESYNCED at seq {}", cursor.last_sequence));
            }
            Err(err) => self.line(&format!("RESYNC FAILED reason={err}")),
        }
    }

    fn show(&mut self, frame: &EventFrame) {
        match &frame.notification {
            Notification::MessageDelta(payload) => {
                if let Some(text) = payload.text.as_deref().filter(|text| !text.is_empty()) {
                    print!("{text}");
                    self.mid_line = !text.ends_with('\n');
                }
            }
            Notification::MessageComplete(payload) => {
                let status = payload
                    .status
                    .map_or_else(|| "unknown".to_string(), |status| status.to_string());
                self.flush_replayed();
                self.line(&format!("TURN COMPLETE status={status}"));
                self.line(&format!(
                    "SEQ CHECK delivered={} duplicates={} missing={}",
                    self.check.delivered, self.check.duplicates, self.check.missing
                ));
                let verdict = if self.check.duplicates == 0 && self.check.missing == 0 {
                    "PASS"
                } else {
                    "FAIL"
                };
                self.line(&format!("RESULT {verdict}"));
            }
            _ => {}
        }
    }
}

/// Counts events delivered, `seq` values seen twice, and numbers skipped between neighbours.
#[derive(Default)]
struct SeqCheck {
    epoch: Option<String>,
    last: Option<u64>,
    delivered: u64,
    duplicates: u64,
    missing: u64,
}

impl SeqCheck {
    /// Start counting from `cursor`; the jump from the old position is not counted.
    fn restart(&mut self, cursor: &Cursor) {
        self.epoch = Some(cursor.replay_epoch.clone());
        self.last = Some(cursor.last_sequence);
    }

    fn see(&mut self, frame: &EventFrame) {
        let (Some(seq), Some(epoch)) = (frame.seq, frame.replay_epoch.as_deref()) else {
            return;
        };
        if self.epoch.as_deref() != Some(epoch) {
            self.epoch = Some(epoch.to_string());
            self.last = None;
        }
        self.delivered += 1;
        match self.last {
            Some(last) if seq <= last => self.duplicates += 1,
            Some(last) => {
                self.missing += seq - last - 1;
                self.last = Some(seq);
            }
            None => self.last = Some(seq),
        }
    }
}

async fn resume(client: &Client, id: &str) -> Result<CanonicalSnapshot, hermes_client::CallError> {
    let params = CanonicalResumeParams {
        session_id: id.to_string(),
        ..CanonicalResumeParams::default()
    };
    client.call::<session_resume::Marker>(params).await
}

fn cursor_of(snapshot: &CanonicalSnapshot) -> Cursor {
    Cursor {
        replay_epoch: snapshot.replay_epoch.clone(),
        last_sequence: u64::try_from(snapshot.last_sequence).unwrap_or(0),
    }
}

fn start_failed(failure: &DialFailure) -> ! {
    println!("START FAILED reason={}", failure.reason());
    println!("next={}", failure.next_step());
    exit(1);
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    exit(1);
}

fn parse_args() -> Args {
    let mut args = Args {
        home: None,
        profile: "default".to_string(),
        model: None,
        provider: None,
        prompt: None,
        resume: None,
    };
    let mut input = std::env::args().skip(1);
    while let Some(flag) = input.next() {
        let mut value = || input.next().unwrap_or_else(|| usage());
        match flag.as_str() {
            "--home" => args.home = Some(PathBuf::from(value())),
            "--profile" => args.profile = value(),
            "--model" => args.model = Some(value()),
            "--provider" => args.provider = Some(value()),
            "--prompt" => args.prompt = Some(value()),
            "--resume" => args.resume = Some(value()),
            _ => usage(),
        }
    }
    args
}

fn usage() -> ! {
    eprintln!(
        "usage: reconnect_watch [--home DIR] [--profile NAME] [--model M] [--provider P] \
         [--prompt TEXT] [--resume SESSION]"
    );
    exit(2);
}
