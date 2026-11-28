//! Read-only discovery of live terminal agents.

use super::HerdrConfig;
use std::process::Stdio;
use std::time::Duration;
use talaria_core::fleet::herdr::{decode_agent, decode_list};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_OUTPUT: u64 = 256 * 1024;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HerdrError(pub String);

/// A handle on the configured terminal manager for read-only pane discovery.
#[derive(Clone, Debug)]
pub struct HerdrClient {
    config: HerdrConfig,
}

impl HerdrClient {
    pub fn new(config: HerdrConfig) -> Self {
        Self { config }
    }

    /// Read the available terminal agents without changing focus or submitting input.
    pub async fn list_panes(
        &self,
        probe: bool,
    ) -> Result<Vec<talaria_core::fleet::herdr::HerdrPane>, HerdrError> {
        list_panes(&self.config, probe).await
    }
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
