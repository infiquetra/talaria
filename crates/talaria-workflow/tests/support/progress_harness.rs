// Example-only mock runner. Never linked into the client or workflow library.
use hermes_client::{CallOptions, Client, DiscoverOptions, EnsurePolicy, Purpose, Scope};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use talaria_workflow::{
    engine::{CallId, EngineEvent, RunOutcomeKind},
    journal::{
        AdmissionState, CallOutcome, FinishSummary, JournalReader, NewManifest, OnUnknown, Record,
        ResolvedLaunch, RunDir, RunId, journal_key, request_id, submission_id, testing::Writer,
    },
    progress::{AgentState, RunControl, RunProgress},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MODEL: &str = "minimax-m3.1-flash";
const PROVIDER: &str = "ollama-cloud";
const PROFILE: &str = "fixture";
const PROMPT: &str = "This is an interactive workflow control exercise. Use the search tool to find information about workflow testing. If a later steer asks you to name one file only, then stop, print exactly one filename first, but continue this exercise afterward: that steer's 'then stop' means stop naming files, not finish the turn. Immediately after the filename, perform eight separate sequential search tool calls, one word per call, in this order: workflow, journal, runner, progress, staffing, worktree, builtins, ultracode. Do not combine or parallelize those calls. Then count from 1 to 2000, one number per line, without abbreviation or skipping numbers. Keep doing that work until the client interrupts you. Do not run shell commands or change files.";
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn launch() -> ResolvedLaunch {
    ResolvedLaunch {
        profile: PROFILE.into(),
        model: MODEL.into(),
        provider: Some(PROVIDER.into()),
        effort: None,
        toolsets: vec!["search".into()],
        skills: vec![],
        max_turns: None,
        isolation: None,
        cwd: None,
    }
}
#[derive(Clone)]
pub struct Admission {
    pub session: String,
    pub id: String,
    pub generation: Option<i64>,
    pub epoch: i64,
    pub status: AdmissionState,
}
pub trait Gateway {
    async fn start(&mut self, request: &str, submission: &str) -> Result<Admission>;
    async fn receipt(&mut self, admission: &Admission) -> Result<Value>;
    async fn interrupt(&mut self, admission: &Admission) -> Result<()>;
}
struct LiveGateway(Client);
async fn call(client: &Client, method: &str, params: Value) -> Result<Value> {
    Ok(client
        .call_raw(
            method,
            params,
            CallOptions {
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .await?)
}
impl Gateway for LiveGateway {
    async fn start(&mut self, request: &str, submission: &str) -> Result<Admission> {
        let created = call(&self.0, "session.create", json!({"profile": PROFILE, "request_id": request,
            "source":"tool", "hidden":true, "model":MODEL, "provider":PROVIDER, "toolsets":["search"]})).await?;
        let session = created["session_id"]
            .as_str()
            .ok_or("create returned no session")?
            .to_owned();
        let mut admitted = call(
            &self.0,
            "prompt.submit",
            json!({"session_id":session,
            "submission_id":submission, "text":PROMPT, "finite":true}),
        )
        .await?;
        if admitted["voice_stopped"] == true {
            return Err("prompt treated as voice stop".into());
        }
        let started = tokio::time::Instant::now();
        while admitted["status"] == "queued" {
            if started.elapsed() >= Duration::from_secs(90) {
                return Err("fixture turn did not start within 90 seconds".into());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
            admitted = call(&self.0, "prompt.receipt", json!({
                "session_id": session, "admission_id": admitted["admission_id"], "include_result":true,
            })).await?;
        }
        if admitted["status"] != "started" {
            return Err(
                "fixture turn ended or became unknown before the live harness was ready".into(),
            );
        }
        Ok(Admission {
            session,
            id: admitted["admission_id"]
                .as_str()
                .ok_or("no admission")?
                .into(),
            generation: admitted["execution_generation"].as_i64(),
            epoch: admitted["authority_epoch"].as_i64().unwrap_or(0),
            status: serde_json::from_value(admitted["status"].clone())?,
        })
    }
    async fn receipt(&mut self, a: &Admission) -> Result<Value> {
        call(
            &self.0,
            "prompt.receipt",
            json!({"session_id":a.session,"admission_id":a.id,"include_result":true}),
        )
        .await
    }
    async fn interrupt(&mut self, a: &Admission) -> Result<()> {
        // Refresh once before interrupting; a completed turn needs no new interrupt.
        let receipt = self.receipt(a).await?;
        if receipt["status"] == "terminal" {
            return Ok(());
        }
        let snapshot = call(&self.0, "session.resume", json!({"session_id":a.session})).await?;
        let generation = snapshot["execution_generation"]
            .as_i64()
            .or(a.generation)
            .ok_or("no generation")?;
        call(
            &self.0,
            "session.interrupt",
            json!({"session_id":a.session,"execution_generation":generation}),
        )
        .await?;
        Ok(())
    }
}
struct Slot {
    key: String,
    label: String,
    call: CallId,
    attempt: u32,
    active: Option<Admission>,
    finished: bool,
    interrupted: bool,
}
pub struct Consumer<G> {
    pub dir: RunDir,
    writer: Writer,
    _lock: fs::File,
    gateway: G,
    slots: Vec<Slot>,
    stopping: bool,
    pub finished: bool,
    heartbeat_ms: i64,
    receipt_ms: i64,
}
impl<G: Gateway> Consumer<G> {
    pub async fn create(path: &Path, gateway: G) -> Result<Self> {
        let id = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("run folder needs a name")?;
        let id = RunId::parse(id).map_err(|_| "run folder must end in a 26-character run id (for example 01JB2X9Q4M8Z7K3T5V6W8Y0A1C)")?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let dir = RunDir::create_with_id(
            parent,
            id,
            NewManifest {
                workflow_name: "progress-control-harness".into(),
                args: None,
                started_at_ms: now_ms(),
                seed: 1,
                profile: PROFILE.into(),
                on_unknown: OnUnknown::Wait,
                talaria_version: env!("CARGO_PKG_VERSION").into(),
                hermes: json!({}),
                roster: json!({}),
                approval_policy: json!({"mode":"finite-only"}),
            },
            "// mock control consumer\n",
        )?;
        fs::create_dir_all(dir.path().join("control"))?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join("run.lock"))?;
        lock.try_lock()?;
        let writer = Writer::spawn(
            fs::OpenOptions::new()
                .append(true)
                .open(dir.path().join("journal.jsonl"))?,
            1,
        );
        writer
            .append(Record::RunStarted {
                run_id: dir.run_id().as_str().into(),
                script_sha256: dir.manifest().workflow.script_sha256.clone(),
            })
            .await?;
        let mut consumer = Self {
            dir,
            writer,
            _lock: lock,
            gateway,
            slots: vec![],
            stopping: false,
            finished: false,
            heartbeat_ms: 0,
            receipt_ms: 0,
        };
        for n in 0..4 {
            let call = CallId {
                digest: format!("{n:064x}"),
                occurrence: 0,
            };
            let key = journal_key(&call, &launch());
            let slot = Slot {
                key,
                call,
                label: if n == 0 {
                    "running".into()
                } else {
                    format!("queued-{n}")
                },
                attempt: 0,
                active: None,
                finished: false,
                interrupted: false,
            };
            consumer.intent(&slot).await?;
            consumer.slots.push(slot);
        }
        consumer.admit_next().await?;
        Ok(consumer)
    }
    pub async fn resume(path: &Path, gateway: G) -> Result<Self> {
        let dir = RunDir::open(path)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join("run.lock"))?;
        lock.try_lock()?;
        let pending = RunControl::pending(&dir);
        let (lines, _) = JournalReader::read_from(&dir.path().join("journal.jsonl"), 0)?;
        let progress = RunProgress::from_records(dir.manifest(), &lines);
        let mut slots = vec![];
        // Read and remove eligible requests before opening the single journal writer.
        for request in pending.restarts {
            let Some(key) = request.key else { continue };
            let agent = progress.agent(&key).ok_or("restart names an unknown key")?;
            if !matches!(agent.state, AgentState::Failed | AgentState::Ended) {
                return Err("restart is only eligible for failed or ended work".into());
            }
            slots.push(Slot {
                key: key.clone(),
                label: agent.label.clone(),
                call: agent.call_id.clone().ok_or("missing call id")?,
                attempt: agent.attempt + 1,
                active: None,
                finished: false,
                interrupted: false,
            });
            fs::remove_file(
                dir.path()
                    .join("control")
                    .join(format!("restart-{key}.json")),
            )?;
        }
        if slots.is_empty() {
            return Err("resume needs an eligible restart request".into());
        }
        fs::File::open(dir.path().join("control"))?.sync_all()?;
        let resume_count = lines
            .iter()
            .filter(|l| matches!(l.record, Record::RunResumed { .. }))
            .count() as u32
            + 1;
        let writer = Writer::spawn(
            fs::OpenOptions::new()
                .append(true)
                .open(dir.path().join("journal.jsonl"))?,
            lines.len() as u64 + 1,
        );
        writer
            .append(Record::RunResumed {
                script_sha256: dir.manifest().workflow.script_sha256.clone(),
                resume_count,
            })
            .await?;
        let consumer = Self {
            dir,
            writer,
            _lock: lock,
            gateway,
            slots,
            stopping: false,
            finished: false,
            heartbeat_ms: 0,
            receipt_ms: 0,
        };
        for slot in &consumer.slots {
            consumer.intent(slot).await?;
        }
        Ok(consumer)
    }
    async fn intent(&self, s: &Slot) -> Result<()> {
        self.writer
            .append(Record::CallIntent {
                key: s.key.clone(),
                attempt: s.attempt,
                call_id: s.call.clone(),
                label: s.label.clone(),
                phase: Some("work".into()),
                workflow_path: vec![],
                launch: launch(),
                request_id: request_id(self.dir.run_id(), &s.key, s.attempt),
            })
            .await?;
        Ok(())
    }
    async fn admit_next(&mut self) -> Result<()> {
        let Some(slot) = self
            .slots
            .iter_mut()
            .find(|s| !s.finished && s.active.is_none())
        else {
            return Ok(());
        };
        let submission = submission_id(self.dir.run_id(), &slot.key, slot.attempt, 0);
        let a = self
            .gateway
            .start(
                &request_id(self.dir.run_id(), &slot.key, slot.attempt),
                &submission,
            )
            .await?;
        self.writer
            .append(Record::SessionBound {
                key: slot.key.clone(),
                attempt: slot.attempt,
                session_id: a.session.clone(),
                stored_session_id: a.session.clone(),
            })
            .await?;
        self.writer
            .append(Record::TurnIntent {
                key: slot.key.clone(),
                attempt: slot.attempt,
                turn: 0,
                submission_id: submission,
                text: Some(PROMPT.into()),
                text_ref: None,
            })
            .await?;
        self.writer
            .append(Record::TurnAdmitted {
                key: slot.key.clone(),
                attempt: slot.attempt,
                turn: 0,
                admission_id: a.id.clone(),
                status: a.status,
                outcome: None,
                execution_generation: a.generation,
                authority_epoch: a.epoch,
            })
            .await?;
        slot.active = Some(a);
        Ok(())
    }
    pub async fn tick(&mut self, at_ms: i64) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        let controls = RunControl::pending(&self.dir);
        if controls.stop.is_some() && !self.stopping {
            self.stopping = true;
            for slot in &mut self.slots {
                if let Some(a) = &slot.active {
                    self.gateway.interrupt(a).await?;
                } else if !slot.finished {
                    self.writer
                        .append(Record::CallFinished {
                            key: slot.key.clone(),
                            attempt: slot.attempt,
                            outcome: CallOutcome::Failed {
                                reason: "cancelled".into(),
                            },
                            turns: 0,
                        })
                        .await?;
                    slot.finished = true;
                }
            }
        }
        if at_ms - self.receipt_ms >= 1000 || self.stopping {
            self.receipt_ms = at_ms;
            for slot in &mut self.slots {
                let Some(a) = &slot.active else { continue };
                let receipt = self.gateway.receipt(a).await?;
                if receipt["status"] != "terminal" {
                    continue;
                }
                let outcome = receipt["outcome"]
                    .as_str()
                    .ok_or("terminal receipt has no outcome")?;
                slot.interrupted = matches!(outcome, "interrupted" | "cancelled");
                self.writer
                    .append(Record::TurnSettled {
                        key: slot.key.clone(),
                        attempt: slot.attempt,
                        turn: 0,
                        admission_id: a.id.clone(),
                        outcome: outcome.into(),
                        final_text: None,
                        final_text_ref: None,
                        result_meta: receipt.get("result").cloned().unwrap_or(json!({})),
                        usage: receipt.get("usage").cloned(),
                    })
                    .await?;
                self.writer
                    .append(Record::CallFinished {
                        key: slot.key.clone(),
                        attempt: slot.attempt,
                        turns: 1,
                        outcome: if outcome == "completed" {
                            CallOutcome::Completed {
                                value: Some(json!("completed")),
                                value_ref: None,
                            }
                        } else {
                            CallOutcome::Failed {
                                reason: outcome.into(),
                            }
                        },
                    })
                    .await?;
                slot.active = None;
                slot.finished = true;
            }
        }
        if self.slots.iter().all(|s| s.finished) {
            let (lines, _) = JournalReader::read_from(&self.dir.path().join("journal.jsonl"), 0)?;
            let progress = RunProgress::from_records(self.dir.manifest(), &lines);
            let counts = &progress.run.counts;
            self.writer
                .append(Record::RunFinished {
                    outcome: if self.stopping {
                        RunOutcomeKind::Aborted
                    } else if counts.failed > 0 {
                        RunOutcomeKind::Failed
                    } else {
                        RunOutcomeKind::Completed
                    },
                    summary: FinishSummary {
                        completed: counts.done,
                        failed: counts.failed,
                        skipped: counts.skipped,
                        parked: 0,
                        unfinished: 0,
                    },
                })
                .await?;
            if self.stopping {
                fs::remove_file(self.dir.path().join("control/stop.json"))?;
                fs::File::open(self.dir.path().join("control"))?.sync_all()?;
            }
            self.finished = true;
            return Ok(());
        }
        if controls.pause.is_none()
            && !self.stopping
            && !self.slots.iter().any(|s| s.active.is_some())
        {
            self.admit_next().await?;
        }
        // A journal heartbeat is observable evidence that the mock runner has held its gate.
        if at_ms - self.heartbeat_ms >= 1000 {
            self.writer
                .append(Record::Engine {
                    event: Some(EngineEvent::Log {
                        message: if controls.pause.is_some() {
                            "mock consumer: admission gate paused".into()
                        } else {
                            "mock consumer: admission gate open".into()
                        },
                        workflow_path: vec![],
                    }),
                    event_ref: None,
                })
                .await?;
            self.heartbeat_ms = at_ms;
        }
        Ok(())
    }
}
async fn connect_live() -> Result<LiveGateway> {
    let home = hermes_client::default_home();
    let mut endpoint = hermes_client::discover_with(
        &home,
        &DiscoverOptions {
            ensure: EnsurePolicy::Never,
            control_timeout: Duration::from_secs(5),
        },
    )
    .await?;
    if let Ok(url) = std::env::var("TALARIA_HERMES_URL") {
        endpoint = endpoint.with_api_origin(url.trim())?;
    }
    if !matches!(endpoint.code_version(), Some("0.22.4" | "v0.22.4")) {
        return Err("gateway release must be 0.22.4".into());
    }
    let profile = endpoint.resolve_profile(PROFILE)?;
    let ticket = hermes_client::mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await?;
    let mut connection = hermes_client::connect(&endpoint, ticket).await?;
    hermes_client::handshake(&mut connection).await?;
    let (client, mut inbound) = Client::start(connection.into_parts());
    tokio::spawn(async move { while inbound.next().await.is_some() {} });
    let options = call(
        &client,
        "model.options",
        json!({"profile":PROFILE,"refresh":true}),
    )
    .await?;
    let provider = options["providers"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|p| p["slug"] == PROVIDER || p["name"] == PROVIDER)
        })
        .ok_or("ollama-cloud unavailable")?;
    if provider["authenticated"] != true {
        return Err("ollama-cloud is not signed in".into());
    }
    let listed = ["models", "featured_models"].iter().any(|field| {
        provider[*field]
            .as_array()
            .is_some_and(|m| m.iter().any(|v| v == MODEL))
    });
    let unavailable = provider["unavailable_models"]
        .as_array()
        .is_some_and(|m| m.iter().any(|v| v == MODEL));
    if !listed || unavailable {
        return Err("minimax-m3.1-flash unavailable".into());
    }
    Ok(LiveGateway(client))
}
pub async fn run(path: PathBuf, seconds: Option<u64>, resume: bool) -> Result<()> {
    let gateway = connect_live().await?;
    let mut consumer = if resume {
        Consumer::resume(&path, gateway).await?
    } else {
        Consumer::create(&path, gateway).await?
    };
    println!("run folder: {}", consumer.dir.run_id().as_str());
    println!(
        "mock consumer active (250 ms); touch <run-folder>/control/harness-stop-consumer to detach it"
    );
    let started = tokio::time::Instant::now();
    let mut timer = tokio::time::interval(Duration::from_millis(250));
    while !consumer.finished {
        timer.tick().await;
        if consumer
            .dir
            .path()
            .join("control/harness-stop-consumer")
            .exists()
            || seconds.is_some_and(|n| started.elapsed() >= Duration::from_secs(n))
        {
            println!("mock consumer stopped; finite gateway turns remain detached");
            break;
        }
        consumer.tick(now_ms()).await?;
    }
    consumer.gateway.0.close().await;
    Ok(())
}
