//! Streaming session service.

use crate::headless::PrintOptions;
use crate::headless::run::StopCause;
use crate::headless::service::HeadlessTimings;
use crate::headless::stream_input::InputReader;
use crate::launch::Launch;
use std::io::Write;
use std::path::Path;
use std::time::Instant;
use tokio::sync::mpsc::Receiver;

pub enum StreamSource {
    Prompt(String),
    Lines(InputReader),
}

#[allow(clippy::too_many_arguments)]
pub async fn run_stream<W: Write + Send>(
    _launch: &Launch,
    _options: &PrintOptions,
    _source: StreamSource,
    _home: &Path,
    _started: Instant,
    _stops: Receiver<StopCause>,
    _timings: HeadlessTimings,
    _out: W,
) -> u8 {
    todo!("add stream-json implementation")
}

/// Test instrumentation called as each owned event becomes eligible for projection.
pub type ObservedHook = Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>;

/// Run with an observer of events accepted by the headless state machine.
#[allow(clippy::too_many_arguments)]
pub async fn run_stream_with_observer<W: Write + Send>(
    _launch: &Launch,
    _options: &PrintOptions,
    _source: StreamSource,
    _home: &Path,
    _started: Instant,
    _stops: Receiver<StopCause>,
    _timings: HeadlessTimings,
    _out: W,
    _observer: ObservedHook,
) -> u8 {
    todo!("add stream-json implementation")
}
