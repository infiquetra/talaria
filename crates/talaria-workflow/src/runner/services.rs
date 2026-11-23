//! Composition of the gateway, staffing, and scheduler owned by their respective modules.

use std::{path::Path, sync::Arc, time::Duration};

use futures::future::BoxFuture;
use serde_json::{Map, Value};
use tokio::sync::mpsc;

use crate::{
    hermes::{
        AgentFeedReceiver, Cursor, HermesPort, LinkWireFactory, PortConfig, Wire, WireError,
        WireEvent, WireFactory,
    },
    staffing::{Clock, LinkRosterSource, RosterCache, RosterSource, SystemClock},
};

/// Reads a profile's effective Hermes configuration, so a run can warn about settings that
/// break workflow agents. A read that fails adds no warning and never stops a run.
pub trait ConfigSource: Send + Sync {
    fn effective(&self, profile: &str) -> BoxFuture<'static, Result<Value, String>>;
}
/// The source used when a caller supplies none: the configuration cannot be read.
struct NoConfig;
impl ConfigSource for NoConfig {
    fn effective(&self, _profile: &str) -> BoxFuture<'static, Result<Value, String>> {
        Box::pin(async { Err("no configuration source".into()) })
    }
}
/// Reads the configuration over the gateway's REST route, the way the `/config` screens do, so it
/// also works against a gateway on another machine. It never reads `config.yaml` from disk.
pub struct RestConfigSource {
    home: std::path::PathBuf,
}
impl RestConfigSource {
    pub fn new(home: std::path::PathBuf) -> Self {
        Self { home }
    }
}
impl ConfigSource for RestConfigSource {
    fn effective(&self, profile: &str) -> BoxFuture<'static, Result<Value, String>> {
        use hermes_client::rest::{NativeHttpMinter, RestClient, RestOptions};
        use hermes_client::{DiscoverOptions, EnsurePolicy, discover_with};
        let home = self.home.clone();
        let profile = profile.to_owned();
        Box::pin(async move {
            let endpoint = Arc::new(
                discover_with(
                    &home,
                    &DiscoverOptions {
                        ensure: EnsurePolicy::Never,
                        ..DiscoverOptions::default()
                    },
                )
                .await
                .map_err(|e| e.to_string())?,
            );
            let served = endpoint
                .resolve_profile(&profile)
                .map_err(|e| e.to_string())?;
            let rest = RestClient::new(
                endpoint.api_origin(),
                Arc::new(NativeHttpMinter::new(home, endpoint.clone(), served)),
                RestOptions::default(),
            )
            .map_err(|e| e.to_string())?;
            hermes_client::config_api::config_read(&rest, true)
                .await
                .map_err(|e| e.to_string())
        })
    }
}

#[derive(Clone)]
pub struct RunnerServices {
    pub wires: Arc<dyn WireFactory>,
    pub config: Arc<dyn ConfigSource>,
    pub rosters: RosterCache,
    pub clock: Arc<dyn Clock>,
    pub port_config: PortConfig,
    recording: Option<hermes_client::Recorder>,
}
impl RunnerServices {
    pub fn new(
        wires: Arc<dyn WireFactory>,
        source: Arc<dyn RosterSource>,
        clock: Arc<dyn Clock>,
        port_config: PortConfig,
    ) -> Self {
        Self {
            wires: Arc::new(ReceiptOptionsFactory(wires)),
            rosters: RosterCache::new(source, clock.clone(), Duration::from_secs(30)),
            config: Arc::new(NoConfig),
            clock,
            port_config,
            recording: None,
        }
    }
    /// The same services with a configuration source, for the warnings before a run.
    #[must_use]
    pub fn with_config(mut self, config: Arc<dyn ConfigSource>) -> Self {
        self.config = config;
        self
    }
    pub(crate) fn finish_recording(&self) -> Result<(), String> {
        if let Some(recording) = &self.recording {
            recording.clone().finish().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    /// The gateway port and its agent event feed, which carries live usage for the budget.
    pub(crate) async fn port(
        &self,
        profile: &str,
    ) -> Result<(HermesPort, AgentFeedReceiver), String> {
        HermesPort::with_wires(self.port_config.clone(), self.wires.clone(), profile)
            .await
            .map_err(|e| e.to_string())
    }
}

struct ReceiptOptionsFactory(Arc<dyn WireFactory>);
impl WireFactory for ReceiptOptionsFactory {
    fn open(&self, profile: &str) -> BoxFuture<'static, Result<Arc<dyn Wire>, WireError>> {
        let opened = self.0.open(profile);
        Box::pin(async move { Ok(Arc::new(ReceiptOptionsWire(opened.await?)) as Arc<dyn Wire>) })
    }
}
struct ReceiptOptionsWire(Arc<dyn Wire>);
impl Wire for ReceiptOptionsWire {
    fn call(
        &self,
        method: &'static str,
        mut params: Map<String, Value>,
        timeout: Duration,
    ) -> BoxFuture<'static, Result<Value, WireError>> {
        if method == "prompt.receipt" {
            // Make the contract's false default explicit for every receipt reader.
            params.entry("include_result").or_insert(Value::Bool(false));
        }
        self.0.call(method, params, timeout)
    }

    fn generation(&self) -> u64 {
        self.0.generation()
    }
    fn track(&self, session_id: &str, cursor: Cursor) {
        self.0.track(session_id, cursor);
    }
    fn untrack(&self, session_id: &str) {
        self.0.untrack(session_id);
    }
    fn resync(&self, session_id: &str, cursor: Cursor) {
        self.0.resync(session_id, cursor);
    }
    fn take_events(&self) -> Option<mpsc::Receiver<WireEvent>> {
        self.0.take_events()
    }
    fn dropped_items(&self) -> u64 {
        self.0.dropped_items()
    }
}
pub(crate) fn port_config() -> PortConfig {
    PortConfig::default()
}
pub(crate) fn production(record: Option<&Path>, profile: &str) -> Result<RunnerServices, String> {
    production_recording(record, profile, false)
}
pub(crate) fn production_recording(
    record: Option<&Path>,
    profile: &str,
    append: bool,
) -> Result<RunnerServices, String> {
    #[cfg(feature = "mock")]
    if std::env::var("TALARIA_WORKFLOW_MOCK").is_ok_and(|v| v == "1") {
        return super::mock_support::services(profile);
    }
    let _ = profile;
    if !append && record.is_some_and(Path::exists) {
        return Err("recording already exists; choose a new path".into());
    }
    let config = port_config();
    let mut factory = LinkWireFactory::new(config.home.clone());
    let mut recording = None;
    if let Some(path) = record {
        let recorder = super::recording::open(path, append)?;
        let tap = recorder.clone();
        factory = factory.on_connect(Arc::new(move |connection, generation| {
            connection.set_recorder(tap.clone(), generation)
        }));
        recording = Some(recorder);
    }
    let wires: Arc<dyn WireFactory> = Arc::new(factory);
    let source = Arc::new(LinkRosterSource::new(wires.clone()));
    let home = config.home.clone();
    let mut services = RunnerServices::new(wires, source, Arc::new(SystemClock), config)
        .with_config(Arc::new(RestConfigSource::new(home)));
    services.recording = recording;
    Ok(services)
}
