//! Polling and explicit actions for live terminal agents.

use super::HerdrConfig;
use crate::fleet::{FleetHandle, FleetReporter, FleetSnapshot};
use std::collections::HashSet;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use talaria_core::fleet::herdr::{decode_agent, decode_list, project, sanitize_visible};
use talaria_core::fleet::{FleetBackend, RowKey};
use talaria_core::fleet_view::FleetViewEffect;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_OUTPUT: u64 = 256 * 1024;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HerdrError(pub String);

struct Inner {
    config: HerdrConfig,
    fleet: FleetReporter,
    known: Mutex<HashSet<RowKey>>,
    polling: tokio::sync::Mutex<()>,
}

#[derive(Clone)]
pub struct HerdrClient {
    inner: Arc<Inner>,
}

/// Live terminal-agent backend, sharing the fleet service's registry and queue.
pub struct HerdrFleetBackend {
    inner: Option<Arc<Inner>>,
    owner: Option<FleetHandle>,
    worker: Option<tokio::task::JoinHandle<()>>,
    closed_snapshot: Arc<FleetSnapshot>,
}

/// Compatibility name for the terminal-agent service interface.
pub type HerdrService = HerdrFleetBackend;

impl HerdrFleetBackend {
    pub fn start(config: HerdrConfig, fleet: FleetHandle) -> Self {
        let mut service = Self::with_reporter(config, fleet.reporter());
        service.owner = Some(fleet);
        service
    }

    /// Attach to a fleet whose lifecycle is owned by the application.
    pub fn with_reporter(config: HerdrConfig, fleet: FleetReporter) -> Self {
        let closed_snapshot = fleet.snapshot();
        let enabled = config.enabled;
        let interval = Duration::from_millis(config.poll_interval_ms.max(50));
        let inner = Arc::new(Inner {
            config,
            fleet,
            known: Mutex::new(HashSet::new()),
            polling: tokio::sync::Mutex::new(()),
        });
        let weak = Arc::downgrade(&inner);
        let worker = enabled.then(|| {
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(interval).await;
                    let Some(inner) = weak.upgrade() else { break };
                    let _ = poll(&inner).await;
                }
            })
        });
        Self {
            inner: Some(inner),
            owner: None,
            worker,
            closed_snapshot,
        }
    }

    pub fn client(&self) -> Option<HerdrClient> {
        self.inner.as_ref().map(|inner| HerdrClient {
            inner: Arc::clone(inner),
        })
    }

    pub async fn poll_once(&mut self) -> Result<(), HerdrError> {
        poll(
            self.inner
                .as_ref()
                .ok_or_else(|| HerdrError("terminal service is closed".into()))?,
        )
        .await
    }
    pub async fn peek(&mut self, key: &RowKey) -> Result<String, HerdrError> {
        self.client()
            .ok_or_else(|| HerdrError("terminal service is closed".into()))?
            .peek(key)
            .await
    }
    pub async fn prompt(&mut self, key: &RowKey, text: &str) -> Result<(), HerdrError> {
        self.client()
            .ok_or_else(|| HerdrError("terminal service is closed".into()))?
            .prompt(key, text)
            .await
    }
    pub async fn answer(&mut self, key: &RowKey, text: &str) -> Result<(), HerdrError> {
        self.client()
            .ok_or_else(|| HerdrError("terminal service is closed".into()))?
            .answer(key, text)
            .await
    }
    pub async fn interrupt(&mut self, key: &RowKey) -> Result<(), HerdrError> {
        self.client()
            .ok_or_else(|| HerdrError("terminal service is closed".into()))?
            .interrupt(key)
            .await
    }
    pub async fn focus(&mut self, key: &RowKey) -> Result<(), HerdrError> {
        self.client()
            .ok_or_else(|| HerdrError("terminal service is closed".into()))?
            .focus(key)
            .await
    }
    pub async fn route_effect(&mut self, effect: &FleetViewEffect) -> Result<(), HerdrError> {
        self.client()
            .ok_or_else(|| HerdrError("terminal service is closed".into()))?
            .route_effect(effect)
            .await
    }
    pub fn snapshot(&self) -> Arc<FleetSnapshot> {
        self.inner.as_ref().map_or_else(
            || Arc::clone(&self.closed_snapshot),
            |inner| inner.fleet.snapshot(),
        )
    }
    pub async fn close(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.abort();
            let _ = worker.await;
        }
        if let Some(inner) = self.inner.take() {
            self.closed_snapshot = inner.fleet.snapshot();
        }
        if let Some(owner) = self.owner.take() {
            owner.close().await;
        }
    }
}

impl Drop for HerdrFleetBackend {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
    }
}

impl HerdrClient {
    fn target<'a>(&self, key: &'a RowKey) -> Result<&'a str, HerdrError> {
        let valid = key.profile == format!("herdr:{}", key.session_id)
            && self
                .inner
                .fleet
                .snapshot()
                .rows
                .iter()
                .any(|row| row.key == *key && row.backend() == FleetBackend::Herdr);
        if valid {
            Ok(&key.session_id)
        } else {
            Err(HerdrError("terminal agent is no longer available".into()))
        }
    }

    pub async fn peek(&self, key: &RowKey) -> Result<String, HerdrError> {
        let pane = self.target(key)?;
        let text = run(
            &self.inner.config,
            &["pane", "read", pane, "--source", "visible"],
        )
        .await?;
        Ok(sanitize_visible(&text))
    }

    pub async fn prompt(&self, key: &RowKey, text: &str) -> Result<(), HerdrError> {
        let pane = self.target(key)?;
        run(&self.inner.config, &["agent", "prompt", pane, text]).await?;
        Ok(())
    }

    pub async fn answer(&self, key: &RowKey, text: &str) -> Result<(), HerdrError> {
        let pane = self.target(key)?;
        if get(&self.inner.config, pane).await?.status != "blocked" {
            return Err(HerdrError(
                "terminal agent is no longer waiting for an answer".into(),
            ));
        }
        let mut keys = Vec::new();
        for ch in text.chars() {
            if ch.is_control() || (ch.is_whitespace() && ch != ' ') || keys.len() >= 1024 {
                return Err(HerdrError("answer contains unsupported keys".into()));
            }
            keys.push(match ch {
                ' ' => "space".to_owned(),
                '+' => "plus".to_owned(),
                _ => ch.to_string(),
            });
        }
        keys.push("enter".to_owned());
        let mut args = vec!["agent", "send-keys", pane];
        args.extend(keys.iter().map(String::as_str));
        run(&self.inner.config, &args).await?;
        Ok(())
    }

    pub async fn interrupt(&self, key: &RowKey) -> Result<(), HerdrError> {
        let pane = self.target(key)?;
        let initial = get(&self.inner.config, pane).await?.status;
        if !matches!(initial.as_str(), "working" | "blocked") {
            return Err(HerdrError("terminal agent is not running".into()));
        }
        run(&self.inner.config, &["agent", "send-keys", pane, "C-c"]).await?;
        let until = tokio::time::Instant::now() + COMMAND_TIMEOUT;
        loop {
            let current = get(&self.inner.config, pane).await?;
            if current.status != initial && current.status != "unknown" {
                self.inner.fleet.observe_herdr(project(&current, "", now()));
                return Ok(());
            }
            if tokio::time::Instant::now() >= until {
                return Err(HerdrError(
                    "interrupt was sent but the terminal state did not change".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn focus(&self, key: &RowKey) -> Result<(), HerdrError> {
        let pane = self.target(key)?;
        run(&self.inner.config, &["agent", "focus", pane]).await?;
        Ok(())
    }

    pub async fn route_effect(&self, effect: &FleetViewEffect) -> Result<(), HerdrError> {
        match effect {
            FleetViewEffect::Open(key) | FleetViewEffect::OpenInPane(key) => self.focus(key).await,
            FleetViewEffect::SendReply { key, text } => {
                let blocked = self.inner.fleet.snapshot().rows.iter().any(|row| {
                    row.key == *key && row.state == talaria_core::fleet::RowState::NeedsYou
                });
                if blocked {
                    self.answer(key, text).await
                } else {
                    self.prompt(key, text).await
                }
            }
            FleetViewEffect::Stop(key) => self.interrupt(key).await,
            FleetViewEffect::OpenPeek(key) => self.peek(key).await.map(|_| ()),
            _ => Ok(()),
        }
    }
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

async fn poll(inner: &Inner) -> Result<(), HerdrError> {
    if !inner.config.enabled {
        return Ok(());
    }
    let _guard = inner.polling.lock().await;
    let raw = run(&inner.config, &["agent", "list"]).await?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| HerdrError("invalid terminal agent list".into()))?;
    let listed = decode_list(&value).map_err(HerdrError)?;
    let mut next = HashSet::new();
    for listed_pane in listed {
        if inner
            .config
            .workspace_filter
            .as_ref()
            .is_some_and(|filter| filter != &listed_pane.workspace_id)
        {
            continue;
        }
        let key = RowKey {
            profile: format!("herdr:{}", listed_pane.pane_id),
            session_id: listed_pane.pane_id.clone(),
        };
        next.insert(key);
        let pane = match get(&inner.config, &listed_pane.pane_id).await {
            Ok(pane)
                if pane.pane_id == listed_pane.pane_id
                    && pane.workspace_id == listed_pane.workspace_id =>
            {
                pane
            }
            _ => continue,
        };
        let visible = if pane.status == "blocked" {
            run(
                &inner.config,
                &["pane", "read", &pane.pane_id, "--source", "visible"],
            )
            .await
            .unwrap_or_default()
        } else {
            String::new()
        };
        inner.fleet.observe_herdr(project(&pane, &visible, now()));
    }
    let removed = {
        let mut known = inner
            .known
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let removed: Vec<RowKey> = known.difference(&next).cloned().collect();
        *known = next;
        removed
    };
    for key in removed {
        inner.fleet.remove_external(key);
    }
    Ok(())
}

/// Read the available terminal agents without changing focus or submitting input.
pub async fn list_panes(
    config: &HerdrConfig,
    probe: bool,
) -> Result<Vec<talaria_core::fleet::herdr::HerdrPane>, HerdrError> {
    let raw = run(config, &["agent", "list"]).await?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| HerdrError("invalid terminal agent list".into()))?;
    let mut panes = decode_list(&value).map_err(HerdrError)?;
    panes.retain(|pane| {
        config
            .workspace_filter
            .as_ref()
            .is_none_or(|filter| filter == &pane.workspace_id)
    });
    if probe {
        let mut refreshed = Vec::with_capacity(panes.len());
        for pane in panes {
            let result = get(config, &pane.pane_id).await?;
            if result.pane_id != pane.pane_id || result.workspace_id != pane.workspace_id {
                return Err(HerdrError(
                    "terminal agent identity changed during discovery".into(),
                ));
            }
            refreshed.push(result);
        }
        panes = refreshed;
    }
    Ok(panes)
}

async fn get(
    config: &HerdrConfig,
    pane: &str,
) -> Result<talaria_core::fleet::herdr::HerdrPane, HerdrError> {
    let raw = run(config, &["agent", "get", pane]).await?;
    let value = serde_json::from_str(&raw)
        .map_err(|_| HerdrError("invalid terminal agent response".into()))?;
    decode_agent(&value).map_err(HerdrError)
}

async fn bounded<R: AsyncRead + Unpin>(read: R) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    read.take(MAX_OUTPUT + 1).read_to_end(&mut bytes).await?;
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err(std::io::Error::other("terminal response too large"));
    }
    Ok(bytes)
}

async fn run(config: &HerdrConfig, args: &[&str]) -> Result<String, HerdrError> {
    let mut child = Command::new(&config.binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| HerdrError("could not start terminal manager".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| HerdrError("terminal output unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| HerdrError("terminal error output unavailable".into()))?;
    let result = tokio::time::timeout(COMMAND_TIMEOUT, async {
        tokio::try_join!(bounded(stdout), bounded(stderr), child.wait())
    })
    .await
    .map_err(|_| HerdrError("terminal manager timed out".into()))?
    .map_err(|_| HerdrError("terminal manager response failed".into()))?;
    let (stdout, _stderr, status) = result;
    if !status.success() {
        return Err(HerdrError("terminal manager refused the operation".into()));
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}
