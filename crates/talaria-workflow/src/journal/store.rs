use super::{
    replay::{CallHistory, Index},
    run_dir::{ResolutionFile, private_file},
    *,
};
use crate::engine::{CallId, EngineConfig, EngineEvent, RunOutcomeKind};
use std::{
    fs::{self, File},
    io::{Seek, SeekFrom},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

#[cfg(feature = "fake-gateway")]
type AppendObserver = Arc<dyn Fn(&Record) -> futures::future::BoxFuture<'static, ()> + Send + Sync>;
pub struct RunJournal {
    dir: RunDir,
    writer: testing::Writer,
    index: Arc<Mutex<Index>>,
    changed: watch::Sender<u64>,
    notices: broadcast::Sender<JournalNotice>,
    _lock: Arc<File>,
    poll: Mutex<Option<tokio::task::JoinHandle<()>>>,
    appends: tokio::sync::RwLock<()>,
    #[cfg(feature = "fake-gateway")]
    observer: Mutex<Option<AppendObserver>>,
}
impl RunJournal {
    pub async fn open(
        dir: RunDir,
        source: &str,
    ) -> Result<(Self, ReplaySummary, Vec<JournalWarning>), JournalError> {
        let source = source.to_owned();
        let input = dir.clone();
        let (lock, file, index, hash, mut warnings) = tokio::task::spawn_blocking(move || {
            let lock = private_file(&input.path().join("run.lock"), false)?;
            match lock.try_lock() {
                Ok(()) => {}
                Err(std::fs::TryLockError::WouldBlock) => {
                    return Err(JournalError::Locked {
                        run_id: input.run_id().as_str().into(),
                    });
                }
                Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
            }
            let path = input.path().join("journal.jsonl");
            let bytes = fs::read(&path)?;
            let (records, offset, warnings) = reader::parse(&bytes, true)?;
            let mut index = Index::default();
            for record in records {
                index.apply(record, input.path())?;
            }
            if let Some(RecordLine {
                record: Record::RunStarted { run_id, .. },
                ..
            }) = index.records.first()
                && run_id != input.run_id().as_str()
            {
                return Err(corrupt(1, "run id differs from manifest"));
            }
            let hash = input.store_script(&source)?;
            let mut file = private_file(&path, false)?;
            if offset < bytes.len() as u64 {
                file.set_len(offset)?;
                file.sync_data()?;
            }
            file.seek(SeekFrom::End(0))?;
            Ok((Arc::new(lock), file, index, hash, warnings))
        })
        .await
        .map_err(io_error)??;
        let next_seq = index.records.len() as u64 + 1;
        let resumed = next_seq > 1;
        let resume_count = index.execution + u32::from(resumed);
        let index = Arc::new(Mutex::new(index));
        let (changed, _) = watch::channel(0u64);
        let (notices, _) = broadcast::channel(256);
        let callback_index = index.clone();
        let callback_changed = changed.clone();
        let callback_dir = dir.clone();
        let callback_notices = notices.clone();
        let writer_lock = lock.clone();
        let writer = testing::Writer::spawn_with(file, next_seq, move |lines, result| {
            let _lock_held = &writer_lock;
            if let Ok(mut index) = callback_index.lock() {
                if let Err(reason) = result {
                    index.failure = Some(reason.into());
                } else {
                    for line in lines {
                        if let Err(e) = index.apply(line.clone(), callback_dir.path()) {
                            index.failure = Some(e.to_string());
                            break;
                        }
                        match &line.record {
                            Record::TurnSettled {
                                key,
                                attempt,
                                turn,
                                admission_id,
                                usage,
                                ..
                            } => {
                                let _ = callback_notices.send(JournalNotice::TurnSettled {
                                    key: key.clone(),
                                    attempt: *attempt,
                                    turn: *turn,
                                    admission_id: admission_id.clone(),
                                    usage: usage.clone(),
                                });
                            }
                            Record::CallResolved {
                                key,
                                attempt,
                                episode,
                                resolution,
                                ..
                            } => {
                                let _ = callback_notices.send(JournalNotice::Resolved {
                                    key: key.clone(),
                                    attempt: *attempt,
                                    episode: *episode,
                                    resolution: *resolution,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
            callback_changed.send_modify(|v| *v = v.wrapping_add(1));
        });
        let journal = Self {
            dir,
            writer,
            index,
            changed,
            notices,
            _lock: lock,
            poll: Mutex::new(None),
            appends: tokio::sync::RwLock::new(()),
            #[cfg(feature = "fake-gateway")]
            observer: Mutex::new(None),
        };
        let record = if resumed {
            Record::RunResumed {
                script_sha256: hash,
                resume_count,
            }
        } else {
            Record::RunStarted {
                run_id: journal.dir.run_id().as_str().into(),
                script_sha256: hash,
            }
        };
        journal.append(record).await?;
        warnings.extend(journal.poll_resolutions().await?);
        let summary = journal.summary();
        Ok((journal, summary, warnings))
    }
    pub fn dir(&self) -> &RunDir {
        &self.dir
    }
    /// Inject a pause after a synced append, to exercise actual crash boundaries offline.
    #[cfg(feature = "fake-gateway")]
    #[doc(hidden)]
    pub fn observe_appends(
        &self,
        observer: impl Fn(&Record) -> futures::future::BoxFuture<'static, ()> + Send + Sync + 'static,
    ) {
        if let Ok(mut slot) = self.observer.lock() {
            *slot = Some(Arc::new(observer));
        }
    }
    /// Cancel a paused resolution poller when simulating process death.
    #[cfg(feature = "fake-gateway")]
    #[doc(hidden)]
    pub fn stop_resolution_poller(&self) {
        if let Ok(mut task) = self.poll.lock()
            && let Some(task) = task.take()
        {
            task.abort();
        }
    }
    /// Queue explicit operator restarts before starting the resumed engine.
    ///
    /// The runner must establish that ordinary in-flight turns have ended before
    /// selecting them. Completed/skipped calls and unknown keys are unchanged.
    /// A synced failed marker makes the next attempt survive another runner crash.
    /// Parked calls use a rerun resolution and still recover any saved result first.
    /// Repeated requests for a key in one execution are idempotent.
    pub async fn request_restart_keys(&self, keys: &[String]) -> Result<(), JournalError> {
        let _exclusive = self.appends.write().await;
        self.check()?;
        for key in keys {
            let history = self.history(key)?;
            let Some(last) = history.last() else { continue };
            if matches!(
                &last.record,
                Record::CallFinished {
                    outcome: CallOutcome::Completed { .. } | CallOutcome::Skipped,
                    ..
                }
            ) {
                continue;
            }
            if self
                .index
                .lock()
                .map_err(io_error)?
                .restart_attempts
                .contains_key(key)
            {
                continue;
            }
            let attempt = history.attempt();
            match &last.record {
                Record::CallAttention { episode, .. } => {
                    self.append_inner(Record::CallResolved {
                        key: key.clone(),
                        attempt,
                        episode: *episode,
                        resolution: Resolution::Rerun,
                        source: ResolutionSource::Operator,
                    })
                    .await?;
                }
                Record::CallFinished {
                    outcome: CallOutcome::Failed { .. },
                    ..
                }
                | Record::CallResolved { .. } => {}
                _ => {
                    let turns = history.turns(self.dir.path())?.len() as u32;
                    self.append_inner(Record::CallFinished {
                        key: key.clone(),
                        attempt,
                        outcome: CallOutcome::Failed {
                            reason: "operator requested restart".into(),
                        },
                        turns,
                    })
                    .await?;
                }
            }
            self.index
                .lock()
                .map_err(io_error)?
                .restart_attempts
                .insert(key.clone(), attempt);
        }
        Ok(())
    }
    pub(crate) fn restart_requested(&self, key: &str, attempt: u32) -> Result<bool, JournalError> {
        self.check()?;
        Ok(self
            .index
            .lock()
            .map_err(io_error)?
            .restart_attempts
            .get(key)
            == Some(&attempt))
    }
    pub fn engine_config(
        &self,
        resolver: &dyn LaunchResolver,
    ) -> Result<EngineConfig, JournalError> {
        self.check()?;
        self.index
            .lock()
            .map_err(io_error)?
            .config(&self.dir, resolver)
    }
    pub async fn record_engine_event(&self, e: &EngineEvent) -> Result<(), JournalError> {
        self.append(Record::Engine {
            event: Some(e.clone()),
            event_ref: None,
        })
        .await
        .map(|_| ())
    }
    pub fn forward_events(
        self: &Arc<Self>,
        mut rx: mpsc::UnboundedReceiver<EngineEvent>,
    ) -> tokio::task::JoinHandle<Result<(), JournalError>> {
        self.ensure_polling();
        let journal = self.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                journal.record_engine_event(&event).await?;
            }
            Ok(())
        })
    }
    pub async fn started_durable(&self, call: &CallId) -> Result<(), JournalError> {
        let mut changed = self.changed.subscribe();
        loop {
            self.check()?;
            if self.index.lock().map_err(io_error)?.started.contains(call) {
                return Ok(());
            }
            changed.changed().await.map_err(|_| JournalError::Closed)?;
        }
    }
    pub async fn finish(&self, outcome: RunOutcomeKind) -> Result<(), JournalError> {
        let _exclusive = self.appends.write().await;
        self.check()?;
        let summary = self.index.lock().map_err(io_error)?.finish_summary();
        self.append_inner(Record::RunFinished { outcome, summary })
            .await?;
        {
            let mut index = self.index.lock().map_err(io_error)?;
            index.closed = true;
            index.active = true;
        }
        self.writer.stop_and_wait();
        if let Ok(mut task) = self.poll.lock()
            && let Some(task) = task.take()
        {
            task.abort();
        }
        self.changed.send_modify(|v| *v = v.wrapping_add(1));
        Ok(())
    }
    pub fn notices(&self) -> broadcast::Receiver<JournalNotice> {
        self.notices.subscribe()
    }
    pub fn summary(&self) -> ReplaySummary {
        self.index.lock().map(|i| i.summary()).unwrap_or_default()
    }
    pub(crate) fn check(&self) -> Result<(), JournalError> {
        let index = self.index.lock().map_err(io_error)?;
        if let Some(e) = &index.failure {
            return Err(io_error(e));
        }
        if index.closed {
            return Err(JournalError::Closed);
        }
        Ok(())
    }
    pub(crate) async fn append(&self, record: Record) -> Result<u64, JournalError> {
        let _shared = self.appends.read().await;
        self.check()?;
        self.append_inner(record).await
    }
    async fn append_inner(&self, mut record: Record) -> Result<u64, JournalError> {
        let dir = self.dir.path().to_owned();
        record = tokio::task::spawn_blocking(move || {
            blobs::externalize(&dir, &mut record)?;
            Ok::<_, JournalError>(record)
        })
        .await
        .map_err(io_error)??;
        #[cfg(feature = "fake-gateway")]
        let observed = self
            .observer
            .lock()
            .map_err(io_error)?
            .clone()
            .map(|observer| (observer, record.clone()));
        let seq = self.writer.append(record).await?;
        self.check()?;
        #[cfg(feature = "fake-gateway")]
        if let Some((observer, record)) = observed {
            observer(&record).await;
        }
        Ok(seq)
    }
    pub(crate) fn history(&self, key: &str) -> Result<CallHistory, JournalError> {
        self.check()?;
        Ok(self
            .index
            .lock()
            .map_err(io_error)?
            .calls
            .get(key)
            .cloned()
            .unwrap_or_default())
    }
    pub(crate) fn execution(&self) -> u32 {
        self.index.lock().map(|i| i.execution).unwrap_or_default()
    }
    pub(crate) fn encounter(&self, key: &str) -> Result<(), JournalError> {
        self.check()?;
        let mut i = self.index.lock().map_err(io_error)?;
        i.active = true;
        i.encountered.insert(key.into());
        Ok(())
    }
    pub(crate) fn notice(&self, notice: JournalNotice) {
        let _ = self.notices.send(notice);
    }
    pub(crate) fn ensure_polling(self: &Arc<Self>) {
        if let Ok(mut slot) = self.poll.lock()
            && slot.is_none()
        {
            let weak = Arc::downgrade(self);
            *slot = Some(tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    let Some(journal) = weak.upgrade() else { break };
                    if journal.check().is_err() {
                        break;
                    }
                    let parked = journal.summary().parked;
                    if !parked.is_empty()
                        && let Err(e) = journal.poll_resolutions().await
                    {
                        journal.fail(e.to_string());
                        break;
                    }
                }
            }));
        }
    }
    fn fail(&self, error: String) {
        if let Ok(mut i) = self.index.lock() {
            i.failure = Some(error);
        }
        self.changed.send_modify(|v| *v = v.wrapping_add(1));
    }
    pub(crate) async fn await_resolution(
        self: &Arc<Self>,
        p: &ParkedCall,
    ) -> Result<(Resolution, ResolutionSource, u32), JournalError> {
        self.ensure_polling();
        let mut changed = self.changed.subscribe();
        loop {
            let h = self.history(&p.key)?;
            if let Some(RecordLine {
                record:
                    Record::CallResolved {
                        episode,
                        resolution,
                        source,
                        ..
                    },
                ..
            }) = h.last()
                && *episode == p.episode
                && !self.index.lock().map_err(io_error)?.resolving.contains(&(
                    p.key.clone(),
                    p.attempt,
                    p.episode,
                ))
            {
                return Ok((*resolution, *source, h.execution));
            }
            if self.dir.manifest().on_unknown == OnUnknown::Fail {
                self.append(Record::CallResolved {
                    key: p.key.clone(),
                    attempt: p.attempt,
                    episode: p.episode,
                    resolution: Resolution::Fail,
                    source: ResolutionSource::Policy,
                })
                .await?;
                return Ok((Resolution::Fail, ResolutionSource::Policy, self.execution()));
            }
            changed.changed().await.map_err(|_| JournalError::Closed)?;
        }
    }
    async fn poll_resolutions(&self) -> Result<Vec<JournalWarning>, JournalError> {
        let folder = self.dir.path().join("resolutions");
        let files = tokio::task::spawn_blocking(move || {
            let mut files = Vec::new();
            for entry in fs::read_dir(folder)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(".tmp-") || name.ends_with(".rejected") {
                    continue;
                }
                files.push((entry.path(), name));
            }
            Ok::<_, std::io::Error>(files)
        })
        .await
        .map_err(io_error)??;
        let mut warnings = Vec::new();
        for (path, name) in files {
            let input = path.clone();
            let bytes = tokio::task::spawn_blocking(move || fs::read(input))
                .await
                .map_err(io_error)??;
            let parsed = serde_json::from_slice::<ResolutionFile>(&bytes);
            let parsed = match parsed {
                Ok(r)
                    if r.v == 1
                        && run_dir::valid_key(&r.key)
                        && name == format!("{}.{}.{}.json", r.key, r.attempt, r.episode) =>
                {
                    Ok(r)
                }
                Ok(_) => Err("resolution name, version or key differs".into()),
                Err(e) => Err(e.to_string()),
            };
            match parsed {
                Err(reason) => {
                    let warning = JournalWarning::BadResolution {
                        file: name.clone(),
                        reason,
                    };
                    tokio::task::spawn_blocking(move || {
                        fs::rename(&path, path.with_file_name(format!("{name}.rejected")))
                    })
                    .await
                    .map_err(io_error)??;
                    warnings.push(warning);
                }
                Ok(r) => {
                    let h = self.history(&r.key)?;
                    let consumed=h.records.iter().any(|l|matches!(&l.record,Record::CallResolved{attempt,episode,..} if *attempt==r.attempt && *episode==r.episode));
                    let parked = matches!(h.last().map(|l|&l.record),Some(Record::CallAttention{attempt,episode,..}) if *attempt==r.attempt && *episode==r.episode);
                    if !consumed && parked {
                        self.index.lock().map_err(io_error)?.resolving.insert((
                            r.key.clone(),
                            r.attempt,
                            r.episode,
                        ));
                        self.append(Record::CallResolved {
                            key: r.key.clone(),
                            attempt: r.attempt,
                            episode: r.episode,
                            resolution: r.resolution,
                            source: ResolutionSource::Operator,
                        })
                        .await?;
                    } else if !consumed {
                        warnings.push(JournalWarning::StaleResolution { key: r.key.clone() });
                    }
                    tokio::task::spawn_blocking(move || {
                        fs::remove_file(&path)?;
                        if let Some(parent) = path.parent() {
                            run_dir::sync_dir(parent)?;
                        }
                        Ok::<_, std::io::Error>(())
                    })
                    .await
                    .map_err(io_error)??;
                    self.index
                        .lock()
                        .map_err(io_error)?
                        .resolving
                        .remove(&(r.key, r.attempt, r.episode));
                    self.changed.send_modify(|v| *v = v.wrapping_add(1));
                }
            }
        }
        for w in &warnings {
            self.notice(JournalNotice::Warning(w.clone()));
        }
        Ok(warnings)
    }
}
impl Drop for RunJournal {
    fn drop(&mut self) {
        if let Ok(mut task) = self.poll.lock()
            && let Some(task) = task.take()
        {
            task.abort();
        }
        let _ = self._lock.unlock();
    }
}

/// The production writer with an injectable durable-file boundary.
#[doc(hidden)]
pub mod testing {
    use super::*;
    use std::io::Write;
    pub trait JournalFile: Send + 'static {
        fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()>;
        fn sync_data(&mut self) -> std::io::Result<()>;
    }
    impl JournalFile for File {
        fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
            Write::write_all(self, buf)
        }
        fn sync_data(&mut self) -> std::io::Result<()> {
            File::sync_data(self)
        }
    }
    // Keep one blocking marker while a batch awaits disk, so paused runtimes do not
    // advance watchdogs ahead of queued file work. An idle writer holds no marker.
    #[derive(Default)]
    struct Pending {
        state: Mutex<(usize, bool)>,
        changed: std::sync::Condvar,
    }
    struct PendingGuard(Arc<Pending>);
    impl Pending {
        fn begin(self: &Arc<Self>) -> PendingGuard {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.0 += 1;
            if !state.1 {
                state.1 = true;
                let pending = self.clone();
                tokio::task::spawn_blocking(move || {
                    let mut state = pending.state.lock().unwrap_or_else(|e| e.into_inner());
                    while state.0 > 0 {
                        state = pending
                            .changed
                            .wait(state)
                            .unwrap_or_else(|e| e.into_inner());
                    }
                    state.1 = false;
                });
            }
            PendingGuard(self.clone())
        }
    }
    impl Drop for PendingGuard {
        fn drop(&mut self) {
            let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
            state.0 -= 1;
            if state.0 == 0 {
                self.0.changed.notify_all();
            }
        }
    }
    struct Append {
        record: Record,
        reply: oneshot::Sender<Result<u64, String>>,
        pending: PendingGuard,
    }
    enum Message {
        Append(Box<Append>),
        Stop(std::sync::mpsc::Sender<()>),
    }
    pub struct Writer {
        tx: mpsc::UnboundedSender<Message>,
        pending: Arc<Pending>,
    }
    impl Writer {
        pub fn spawn<F: JournalFile>(file: F, next_seq: u64) -> Self {
            Self::spawn_with(file, next_seq, |_, _| {})
        }
        pub(super) fn spawn_with<F: JournalFile>(
            mut file: F,
            mut next_seq: u64,
            mut committed: impl FnMut(&[RecordLine], Result<(), &str>) + Send + 'static,
        ) -> Self {
            let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
            let runtime = tokio::runtime::Handle::current();
            std::thread::spawn(move || {
                let mut failure: Option<String> = None;
                while let Some(message) = rx.blocking_recv() {
                    let first = match message {
                        Message::Append(a) => a,
                        Message::Stop(reply) => {
                            drop(file);
                            drop(committed);
                            let _ = reply.send(());
                            return;
                        }
                    };
                    let mut stop = None;
                    let mut batch = vec![first];
                    while let Ok(next) = rx.try_recv() {
                        match next {
                            Message::Append(a) => batch.push(a),
                            Message::Stop(reply) => {
                                stop = Some(reply);
                                break;
                            }
                        }
                    }
                    if let Some(e) = &failure {
                        for a in batch {
                            let _ = a.reply.send(Err(e.clone()));
                        }
                        if let Some(reply) = stop {
                            drop(file);
                            drop(committed);
                            let _ = reply.send(());
                            return;
                        }
                        continue;
                    }
                    let mut records = Vec::new();
                    let mut replies = Vec::new();
                    let mut bytes = Vec::new();
                    for append in batch {
                        let line = RecordLine {
                            v: 1,
                            seq: next_seq,
                            at_ms: now_ms(),
                            record: append.record,
                        };
                        let encoded = serde_json::to_vec(&line);
                        match encoded {
                            Ok(encoded) if encoded.len() < blobs::RECORD_LIMIT => {
                                bytes.extend(encoded);
                                bytes.push(b'\n');
                                next_seq += 1;
                                replies.push((append.reply, append.pending));
                                records.push(line);
                            }
                            Ok(encoded) => {
                                let _ = append.reply.send(Err(format!(
                                    "record_too_large:{}:{}",
                                    line.record.kind(),
                                    encoded.len()
                                )));
                            }
                            Err(e) => {
                                let _ = append.reply.send(Err(e.to_string()));
                            }
                        }
                    }
                    if records.is_empty() {
                        if let Some(reply) = stop {
                            drop(file);
                            drop(committed);
                            let _ = reply.send(());
                            return;
                        }
                        continue;
                    }
                    let synced = runtime.block_on(runtime.spawn_blocking(move || {
                        let result = file
                            .write_all(&bytes)
                            .and_then(|()| file.sync_data())
                            .map_err(|e| e.to_string());
                        committed(
                            &records,
                            result.as_ref().map(|_| ()).map_err(String::as_str),
                        );
                        (file, committed, records, result)
                    }));
                    let (next_file, next_committed, records, result) = match synced {
                        Ok(batch) => batch,
                        Err(error) => {
                            for (reply, _pending) in replies {
                                let _ = reply.send(Err(error.to_string()));
                            }
                            return;
                        }
                    };
                    file = next_file;
                    committed = next_committed;
                    match result {
                        Ok(()) => {
                            for ((reply, _pending), line) in replies.into_iter().zip(records) {
                                let _ = reply.send(Ok(line.seq));
                            }
                        }
                        Err(e) => {
                            for (reply, _pending) in replies {
                                let _ = reply.send(Err(e.clone()));
                            }
                            failure = Some(e);
                        }
                    }
                    if let Some(reply) = stop {
                        drop(file);
                        drop(committed);
                        let _ = reply.send(());
                        return;
                    }
                }
            });
            Self {
                tx,
                pending: Arc::new(Pending::default()),
            }
        }
        pub(super) fn stop_and_wait(&self) {
            let (reply, rx) = std::sync::mpsc::channel();
            if self.tx.send(Message::Stop(reply)).is_ok() {
                let _ = rx.recv();
            }
        }
        pub async fn append(&self, record: Record) -> Result<u64, JournalError> {
            let (reply, rx) = oneshot::channel();
            self.tx
                .send(Message::Append(Box::new(Append {
                    record,
                    reply,
                    pending: self.pending.begin(),
                })))
                .map_err(|_| JournalError::Closed)?;
            match rx.await.map_err(|_| JournalError::Closed)? {
                Ok(seq) => Ok(seq),
                Err(e) => {
                    if let Some(parts) = e.strip_prefix("record_too_large:")
                        && let Some((t, bytes)) = parts.split_once(':')
                    {
                        return Err(JournalError::RecordTooLarge {
                            t: t.into(),
                            bytes: bytes.parse().unwrap_or_default(),
                        });
                    }
                    Err(io_error(e))
                }
            }
        }
    }
}

impl Drop for testing::Writer {
    fn drop(&mut self) {
        self.stop_and_wait();
    }
}
