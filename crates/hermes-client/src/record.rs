//! Redacting frame recorder, frame log version 3.
//!
//! `record_inbound` and `record_outbound` push raw text onto a bounded queue and return. One
//! writer thread owns the [`Redactor`], so frames are redacted in arrival order and raw text
//! never leaves memory. A full queue drops the frame and counts it; the file says how many were
//! lost. See `docs/formats/frame-log.md` for the format.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Instant;

use serde::Serialize;
use serde_json::Value;

use crate::redact::{Redaction, Redactor, redact_url};

/// The frame log layout this recorder writes.
pub const FRAME_LOG_VERSION: u32 = 3;

/// How many frames wait for the writer before new ones are dropped.
const DEFAULT_QUEUE_CAPACITY: usize = 4_096;

const TICKET_PROTOCOL_PREFIX: &str = "hermes-gateway-ticket.";

/// The gateway build identifiers that `identify` reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayIdentity {
    pub code_version: String,
    pub code_sha: String,
}

/// What the header records and how a recording is processed.
#[derive(Debug, Clone, Default)]
pub struct RecorderOptions {
    pub endpoint: String,
    pub subprotocols: Vec<String>,
    pub gateway: Option<GatewayIdentity>,
    pub scenario: Option<String>,
    pub expected_statuses: Vec<String>,
    pub substitutions: Vec<(String, String)>,
    /// Frames the queue holds before it drops; `None` means 4,096.
    pub queue_capacity: Option<usize>,
}

/// What `finish` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordSummary {
    pub frames: usize,
    pub redactions: usize,
    pub dropped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecordError {
    #[error("the recording file already exists")]
    AlreadyExists,
    #[error("a substitution needle is shorter than four characters")]
    NeedleTooShort,
    #[error("recording failed: {0:?}")]
    Io(std::io::ErrorKind),
}

impl From<std::io::Error> for RecordError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}

/// A cheap, cloneable handle on one recording.
#[derive(Debug, Clone)]
pub struct Recorder {
    shared: Arc<Shared>,
}

#[derive(Clone, Copy)]
enum Direction {
    In,
    Out,
}

struct Item {
    direction: Direction,
    generation: u64,
    at: Instant,
    text: String,
}

/// What the writer thread counted.
struct Written {
    frames: usize,
    redactions: usize,
}

struct Shared {
    /// `None` once the recording is finished; every clone then records nothing.
    sender: Mutex<Option<SyncSender<Item>>>,
    /// Frames not queued because the queue was full.
    dropped: Arc<AtomicUsize>,
    /// Set by the writer when a write failed.
    stopped: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<Result<Written, RecordError>>>>,
    outcome: Mutex<Option<Result<RecordSummary, RecordError>>>,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Recorder")
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

impl Shared {
    /// Close the queue for every clone, wait for the writer, and keep the outcome.
    fn finish(&self) -> Result<RecordSummary, RecordError> {
        let mut outcome = lock(&self.outcome);
        if let Some(done) = outcome.as_ref() {
            return done.clone();
        }
        lock(&self.sender).take();
        let worker = lock(&self.worker).take();
        let result = match worker.map(JoinHandle::join) {
            Some(Ok(Ok(written))) => Ok(RecordSummary {
                frames: written.frames,
                redactions: written.redactions,
                dropped: self.dropped.load(Ordering::SeqCst),
            }),
            Some(Ok(Err(error))) => Err(error),
            Some(Err(_)) | None => Err(RecordError::Io(std::io::ErrorKind::Other)),
        };
        *outcome = Some(result.clone());
        result
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Header<'a> {
    kind: &'static str,
    version: u32,
    started_at: String,
    endpoint: String,
    subprotocols: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gateway: Option<HeaderGateway<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenario: Option<&'a str>,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    expected_statuses: &'a [String],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeaderGateway<'a> {
    code_version: &'a str,
    code_sha: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FrameRecord<'a> {
    kind: &'static str,
    seq: u64,
    t_ms: u64,
    generation: u64,
    dir: &'static str,
    frame: &'a Option<Value>,
    #[serde(skip_serializing_if = "<[RedactionEntry]>::is_empty")]
    redactions: Vec<RedactionEntry<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parse_error: Option<&'a str>,
}

#[derive(Serialize)]
struct RedactionEntry<'a> {
    path: &'a str,
    reason: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DroppedRecord {
    kind: &'static str,
    seq: u64,
    t_ms: u64,
    count: usize,
}

impl Recorder {
    /// Create `path` (never overwriting it, private to its owner) and write the header.
    pub fn create(path: &Path, options: RecorderOptions) -> Result<Recorder, RecordError> {
        let redactor = Redactor::new(options.substitutions.clone())?;
        let mut open = OpenOptions::new();
        open.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open.mode(0o600);
        }
        let file = open.open(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                RecordError::AlreadyExists
            } else {
                RecordError::from(error)
            }
        })?;
        Self::start(file, options, redactor)
    }

    /// Record into any writer. The header is written before this returns.
    pub fn from_writer<W: Write + Send + 'static>(
        writer: W,
        options: RecorderOptions,
    ) -> Result<Recorder, RecordError> {
        let redactor = Redactor::new(options.substitutions.clone())?;
        Self::start(writer, options, redactor)
    }

    fn start<W: Write + Send + 'static>(
        mut writer: W,
        options: RecorderOptions,
        redactor: Redactor,
    ) -> Result<Recorder, RecordError> {
        let created = Instant::now();
        let header = Header {
            kind: "header",
            version: FRAME_LOG_VERSION,
            started_at: started_at(),
            endpoint: redactor.substitute(&redact_url(&options.endpoint)),
            subprotocols: options
                .subprotocols
                .iter()
                .map(String::as_str)
                .filter(|protocol| !protocol.starts_with(TICKET_PROTOCOL_PREFIX))
                .collect(),
            gateway: options.gateway.as_ref().map(|gateway| HeaderGateway {
                code_version: &gateway.code_version,
                code_sha: &gateway.code_sha,
            }),
            scenario: options.scenario.as_deref(),
            expected_statuses: &options.expected_statuses,
        };
        write_line(&mut writer, &header)?;

        let capacity = options
            .queue_capacity
            .unwrap_or(DEFAULT_QUEUE_CAPACITY)
            .max(1);
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let dropped = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let mut thread = WriterThread {
            writer,
            redactor,
            created,
            seq: 0,
            last_t_ms: 0,
            dropped_written: 0,
            dropped: Arc::clone(&dropped),
            stopped: Arc::clone(&stopped),
            written: Written {
                frames: 0,
                redactions: 0,
            },
        };
        let worker = std::thread::Builder::new()
            .name("frame-recorder".to_owned())
            .spawn(move || thread.run(receiver))?;
        Ok(Recorder {
            shared: Arc::new(Shared {
                sender: Mutex::new(Some(sender)),
                dropped,
                stopped,
                worker: Mutex::new(Some(worker)),
                outcome: Mutex::new(None),
            }),
        })
    }

    /// Queue a frame read from the gateway. Never blocks.
    pub fn record_inbound(&self, generation: u64, text: &str) {
        self.record(Direction::In, generation, text);
    }

    /// Queue a frame written to the gateway. Never blocks.
    pub fn record_outbound(&self, generation: u64, text: &str) {
        self.record(Direction::Out, generation, text);
    }

    fn record(&self, direction: Direction, generation: u64, text: &str) {
        if self.shared.stopped.load(Ordering::SeqCst) {
            return;
        }
        let sender = lock(&self.shared.sender);
        let Some(sender) = sender.as_ref() else {
            return;
        };
        let item = Item {
            direction,
            generation,
            at: Instant::now(),
            text: text.to_owned(),
        };
        if let Err(TrySendError::Full(_)) = sender.try_send(item) {
            self.shared.dropped.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Close the queue for every clone, write what is pending, and report.
    pub fn finish(self) -> Result<RecordSummary, RecordError> {
        self.shared.finish()
    }
}

struct WriterThread<W> {
    writer: W,
    redactor: Redactor,
    created: Instant,
    seq: u64,
    last_t_ms: u64,
    dropped_written: usize,
    dropped: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
    written: Written,
}

impl<W: Write> WriterThread<W> {
    fn run(&mut self, receiver: Receiver<Item>) -> Result<Written, RecordError> {
        let result = self.drain(&receiver);
        if result.is_err() {
            self.stopped.store(true, Ordering::SeqCst);
        }
        // Dropping the receiver makes every later send fail without counting as a drop.
        drop(receiver);
        result.map(|()| Written {
            frames: self.written.frames,
            redactions: self.written.redactions,
        })
    }

    fn drain(&mut self, receiver: &Receiver<Item>) -> Result<(), RecordError> {
        while let Ok(item) = receiver.recv() {
            self.write_dropped(item.at)?;
            self.write_frame(item)?;
        }
        // The queue is closed: no frame follows, so a pending drop is written now.
        self.write_dropped(Instant::now())
    }

    fn t_ms(&mut self, at: Instant) -> u64 {
        let elapsed = at.saturating_duration_since(self.created).as_millis();
        let t_ms = u64::try_from(elapsed)
            .unwrap_or(u64::MAX)
            .max(self.last_t_ms);
        self.last_t_ms = t_ms;
        t_ms
    }

    fn write_dropped(&mut self, at: Instant) -> Result<(), RecordError> {
        let total = self.dropped.load(Ordering::SeqCst);
        let count = total.saturating_sub(self.dropped_written);
        if count == 0 {
            return Ok(());
        }
        self.seq += 1;
        let record = DroppedRecord {
            kind: "dropped",
            seq: self.seq,
            t_ms: self.t_ms(at),
            count,
        };
        write_line(&mut self.writer, &record)?;
        self.dropped_written = total;
        Ok(())
    }

    fn write_frame(&mut self, item: Item) -> Result<(), RecordError> {
        let redacted = match item.direction {
            Direction::In => self.redactor.redact_inbound(&item.text),
            Direction::Out => self.redactor.redact_outbound(&item.text),
        };
        drop(item.text);
        self.seq += 1;
        let record = FrameRecord {
            kind: "frame",
            seq: self.seq,
            t_ms: self.t_ms(item.at),
            generation: item.generation,
            dir: match item.direction {
                Direction::In => "in",
                Direction::Out => "out",
            },
            frame: &redacted.frame,
            redactions: redacted
                .redactions
                .iter()
                .map(|Redaction { path, reason }| RedactionEntry { path, reason })
                .collect(),
            parse_error: redacted.parse_error.as_deref(),
        };
        write_line(&mut self.writer, &record)?;
        self.written.frames += 1;
        self.written.redactions += redacted.redactions.len();
        Ok(())
    }
}

/// One JSON line, written in one call and flushed.
fn write_line<W: Write, T: Serialize>(writer: &mut W, record: &T) -> Result<(), RecordError> {
    let mut line =
        serde_json::to_vec(record).map_err(|_| RecordError::Io(std::io::ErrorKind::InvalidData))?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()?;
    Ok(())
}

/// Now, as ISO 8601 in UTC with milliseconds.
fn started_at() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.millisecond()
    )
}
