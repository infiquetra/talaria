//! Background work for client-owned devices and the Hermes audio relay.
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use hermes_client::{
    audio_api::AudioRelay,
    rest::{NativeHttpMinter, RestClient, RestOptions},
};
use talaria_core::voice::client::Input;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::{playback::SentencePlayback, transport::Capture};
use crate::app::AppMessage;

struct CaptureJob {
    stop: Option<oneshot::Sender<()>>,
    cancel: Option<oneshot::Sender<()>>,
    cancelled: Arc<AtomicBool>,
    device_active: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

struct PlaybackJob {
    stop: Option<oneshot::Sender<()>>,
    device_active: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

pub struct SpeechRequest {
    pub command: Vec<String>,
    pub profile: String,
    pub session_id: String,
    pub generation: u64,
    pub audio_epoch: Option<u64>,
    pub sentences: Vec<String>,
}

struct PendingSynthesis(JoinHandle<Result<Vec<u8>, String>>);

impl Drop for PendingSynthesis {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn synthesize(relay: AudioRelay, profile: String, text: String) -> PendingSynthesis {
    PendingSynthesis(tokio::spawn(
        async move { relay.speak(&profile, &text).await },
    ))
}

/// Owns the workers; only messages cross back into the application loop.
pub struct ClientAudio {
    home: PathBuf,
    remote: Option<AudioRelay>,
    remote_gateway: bool,
    remote_audio: bool,
    sender: mpsc::UnboundedSender<AppMessage>,
    capture: Mutex<Option<CaptureJob>>,
    retired_capture: Mutex<Vec<CaptureJob>>,
    capture_barrier: Arc<tokio::sync::Mutex<()>>,
    capture_blocked: Arc<AtomicBool>,
    playback: Mutex<Vec<PlaybackJob>>,
    playback_barrier: Arc<tokio::sync::Mutex<()>>,
    playback_blocked: Arc<AtomicBool>,
}

impl ClientAudio {
    pub fn new(
        home: PathBuf,
        remote: Option<AudioRelay>,
        remote_gateway: bool,
        remote_audio: bool,
        sender: mpsc::UnboundedSender<AppMessage>,
    ) -> Self {
        Self {
            home,
            remote,
            remote_gateway,
            remote_audio,
            sender,
            capture: Mutex::new(None),
            retired_capture: Mutex::new(Vec::new()),
            capture_barrier: Arc::default(),
            capture_blocked: Arc::default(),
            playback: Mutex::new(Vec::new()),
            playback_barrier: Arc::default(),
            playback_blocked: Arc::default(),
        }
    }

    pub fn capture(
        &self,
        command: Vec<String>,
        profile: String,
        session_id: String,
        generation: u64,
        epoch: u64,
    ) {
        if !super::transport::remote_audio_allowed(self.remote_gateway, self.remote_audio) {
            let _ = self.sender.send(AppMessage::ClientVoice {
                session_id,
                generation,
                input: Input::Failed { epoch },
                notice: Some("Remote audio is off.".into()),
            });
            return;
        }
        let (stop, receive_stop) = oneshot::channel();
        let (cancel, mut receive_cancel) = oneshot::channel();
        let home = self.home.clone();
        let remote = self.remote.clone();
        let remote_gateway = self.remote_gateway;
        let sender = self.sender.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let is_cancelled = cancelled.clone();
        let device_active = Arc::new(AtomicBool::new(false));
        let active = device_active.clone();
        let barrier = self.capture_barrier.clone();
        let capture_blocked = self.capture_blocked.clone();
        let task = tokio::spawn(async move {
            let send = |input, notice| {
                let _ = sender.send(AppMessage::ClientVoice {
                    session_id: session_id.clone(),
                    generation,
                    input,
                    notice,
                });
            };
            let health = |source, ok| {
                let _ = sender.send(AppMessage::ClientVoiceHealth {
                    session_id: session_id.clone(),
                    generation,
                    source,
                    ok,
                });
            };
            let device = barrier.lock().await;
            active.store(true, Ordering::Release);
            if is_cancelled.load(Ordering::Acquire) {
                active.store(false, Ordering::Release);
                return;
            }
            if capture_blocked.load(Ordering::Acquire) {
                active.store(false, Ordering::Release);
                send(Input::Failed { epoch }, Some("The previous capture helper could not be reaped; restart Talaria before recording again.".into()));
                return;
            }
            let capture = match Capture::start(&command) {
                Ok(capture) => capture,
                Err(_) => {
                    active.store(false, Ordering::Release);
                    health("capture", false);
                    send(
                        Input::Failed { epoch },
                        Some("The capture helper could not start.".into()),
                    );
                    return;
                }
            };
            health("capture", true);
            let stop_handle = capture.stop_handle();
            let cancel_handle = capture.cancel_handle();
            let stopper = tokio::spawn(async move {
                tokio::select! {
                    biased;
                    _ = &mut receive_cancel => cancel_handle.cancel(),
                    result = receive_stop => {
                        if result.is_ok() {
                            stop_handle.stop();
                            let _ = receive_cancel.await;
                        }
                        cancel_handle.cancel();
                    }
                }
            });
            let audio = match capture.finish().await {
                Ok(audio) => audio,
                Err(error) => {
                    stopper.abort();
                    if super::transport::capture_unreaped(&error) {
                        capture_blocked.store(true, Ordering::Release);
                        health("capture", false);
                        send(
                            Input::Failed { epoch },
                            Some("The capture helper could not be reaped.".into()),
                        );
                        return;
                    }
                    active.store(false, Ordering::Release);
                    if is_cancelled.load(Ordering::Acquire) {
                        return;
                    }
                    health("capture", false);
                    send(
                        Input::Failed { epoch },
                        Some("The capture helper failed.".into()),
                    );
                    return;
                }
            };
            active.store(false, Ordering::Release);
            stopper.abort();
            drop(device);
            if is_cancelled.load(Ordering::Acquire) {
                return;
            }
            send(Input::CaptureFinished { epoch }, None);
            let relay = match relay_for(home, remote, remote_gateway, &profile).await {
                Ok(relay) => relay,
                Err(_) => {
                    health("relay", false);
                    send(
                        Input::Failed { epoch },
                        Some("The audio relay is unavailable.".into()),
                    );
                    return;
                }
            };
            match relay.transcribe(&profile, &audio).await {
                Ok(text) => {
                    health("relay", true);
                    send(Input::Text { epoch, text }, None);
                }
                Err(_) => {
                    health("relay", false);
                    send(
                        Input::Failed { epoch },
                        Some("The audio relay could not transcribe the recording.".into()),
                    );
                }
            }
        });
        if let Ok(mut current) = self.capture.lock() {
            if let Some(mut old) = current.replace(CaptureJob {
                stop: Some(stop),
                cancel: Some(cancel),
                cancelled,
                device_active,
                task,
            }) {
                old.cancelled.store(true, Ordering::Release);
                if let Some(cancel) = old.cancel.take() {
                    let _ = cancel.send(());
                }
                if let Ok(mut retired) = self.retired_capture.lock() {
                    retired.retain(|job| !job.task.is_finished());
                    retired.push(old);
                } else {
                    old.task.abort();
                }
            }
        } else {
            task.abort();
        }
    }

    pub fn finish_capture(&self) {
        if let Ok(mut current) = self.capture.lock()
            && let Some(job) = current.as_mut()
            && let Some(stop) = job.stop.take()
        {
            let _ = stop.send(());
        }
    }

    /// Cancel capture independently from the playback stop path.
    pub fn cancel_capture(&self) {
        if let Ok(mut current) = self.capture.lock()
            && let Some(job) = current.as_mut()
        {
            job.cancelled.store(true, Ordering::Release);
            if let Some(cancel) = job.cancel.take() {
                let _ = cancel.send(());
            }
        }
    }

    /// Cancel pending synthesis and wait at most 100 ms for playback helpers to exit.
    pub fn stop_playback(&self) -> impl std::future::Future<Output = bool> + Send + 'static {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(100);
        let blocked = self.playback_blocked.clone();
        let mut jobs = self
            .playback
            .lock()
            .map(|mut jobs| std::mem::take(&mut *jobs))
            .unwrap_or_default();
        for job in &mut jobs {
            if let Some(stop) = job.stop.take() {
                let _ = stop.send(());
            }
        }
        async move {
            let mut quiet = !blocked.load(Ordering::Acquire);
            for mut job in jobs {
                match tokio::time::timeout_at(deadline, &mut job.task).await {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => {
                        if job.device_active.load(Ordering::Acquire) {
                            blocked.store(true, Ordering::Release);
                        }
                        quiet = false;
                    }
                    Err(_) => {
                        if job.device_active.load(Ordering::Acquire) {
                            blocked.store(true, Ordering::Release);
                        }
                        job.task.abort();
                        let _ = job.task.await;
                        quiet = false;
                    }
                }
            }
            quiet && !blocked.load(Ordering::Acquire)
        }
    }

    pub fn speak(
        &self,
        command: Vec<String>,
        profile: String,
        session_id: String,
        generation: u64,
        text: String,
    ) {
        self.speak_sentences(SpeechRequest {
            command,
            profile,
            session_id,
            generation,
            audio_epoch: None,
            sentences: vec![text],
        });
    }

    pub fn speak_sentences(&self, request: SpeechRequest) {
        let SpeechRequest {
            command,
            profile,
            session_id,
            generation,
            audio_epoch,
            sentences,
        } = request;
        if !super::transport::remote_audio_allowed(self.remote_gateway, self.remote_audio) {
            return;
        }
        let home = self.home.clone();
        let remote = self.remote.clone();
        let remote_gateway = self.remote_gateway;
        let sender = self.sender.clone();
        let barrier = self.playback_barrier.clone();
        let playback_blocked = self.playback_blocked.clone();
        let device_active = Arc::new(AtomicBool::new(false));
        let active = device_active.clone();
        let (stop, mut stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            let epoch = audio_epoch.unwrap_or(1);
            let mut player = SentencePlayback::for_audio(epoch);
            for sentence in sentences {
                player.enqueue(epoch, sentence);
            }
            let health = |source, ok| {
                let _ = sender.send(AppMessage::ClientVoiceHealth {
                    session_id: session_id.clone(),
                    generation,
                    source,
                    ok,
                });
            };
            let result = async {
                let relay = tokio::select! {
                    biased;
                    _ = &mut stopped => { player.stop().await; return Ok(()); },
                    result = relay_for(home, remote, remote_gateway, &profile) => match result { Ok(relay) => relay, Err(()) => { health("relay", false); return Err(()); } },
                };
                let Some(first) = player.next_sentence() else { return Ok(()); };
                let mut pending = synthesize(relay.clone(), profile.clone(), first);
                loop {
                let audio = tokio::select! {
                    biased;
                    _ = &mut stopped => { player.stop().await; return Ok(()); },
                    result = &mut pending.0 => match result { Ok(Ok(audio)) => audio, _ => { health("relay", false); return Err(()); } },
                };
                let mut next = player.next_sentence().map(|text| synthesize(relay.clone(), profile.clone(), text));
                if stopped.try_recv().is_ok() { player.stop().await; return Ok(()); }
                let _device = tokio::select! {
                    biased;
                    _ = &mut stopped => { player.stop().await; return Ok(()); },
                    guard = barrier.lock() => guard,
                };
                active.store(true, Ordering::Release);
                if stopped.try_recv().is_ok() { player.stop().await; active.store(false, Ordering::Release); return Ok(()); }
                if playback_blocked.load(Ordering::Acquire) { active.store(false, Ordering::Release); health("playback", false); return Err(()); }
                health("relay", true);
                match player.start_audio(&command) {
                    Ok(()) => {
                        health("playback", true);
                    }
                    Err(_) => {
                        active.store(false, Ordering::Release);
                        health("playback", false);
                        return Err(());
                    }
                };
                let (result, cancelled) = tokio::select! {
                    biased;
                    _ = &mut stopped => (Ok(()), true),
                    result = player.play_audio(&audio) => (result.map_err(|_| ()), false),
                };
                if cancelled { drop(next.take()); }
                let quiet = if cancelled {
                    player.stop().await.quiet
                } else {
                    player.finish_audio().await.is_ok()
                };
                if !quiet {
                    playback_blocked.store(true, Ordering::Release);
                    health("playback", false);
                    return Err(());
                }
                if cancelled { player.confirm_quiet().await; }
                active.store(false, Ordering::Release);
                result?;
                if cancelled { return Ok(()); }
                match next { Some(next) => pending = next, None => return Ok(()) }
                }
            }
            .await;
            if result.is_err() {
                let _ = sender.send(AppMessage::VoiceNotice {
                    session_id: session_id.clone(),
                    generation,
                    text: "Client read-aloud failed.".into(),
                });
            }
            if let Some(audio_epoch) = audio_epoch {
                let _ = sender.send(AppMessage::ClientVoice {
                    session_id,
                    generation,
                    input: Input::SpeechFinished { audio_epoch },
                    notice: None,
                });
            }
        });
        if let Ok(mut current) = self.playback.lock() {
            current.retain(|job| !job.task.is_finished());
            for old in current.iter_mut() {
                if let Some(stop) = old.stop.take() {
                    let _ = stop.send(());
                }
            }
            current.push(PlaybackJob {
                stop: Some(stop),
                device_active,
                task,
            });
        } else {
            task.abort();
        }
    }

    pub fn close(&self) -> impl std::future::Future<Output = ()> + Send + 'static {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        let capture_blocked = self.capture_blocked.clone();
        let playback_blocked = self.playback_blocked.clone();
        let capture = self
            .capture
            .lock()
            .ok()
            .and_then(|mut current| current.take());
        let mut captures = self
            .retired_capture
            .lock()
            .map(|mut jobs| std::mem::take(&mut *jobs))
            .unwrap_or_default();
        if let Some(job) = capture {
            captures.push(job);
        }
        let mut playback = self
            .playback
            .lock()
            .map(|mut current| std::mem::take(&mut *current))
            .unwrap_or_default();
        for job in &mut playback {
            if let Some(stop) = job.stop.take() {
                let _ = stop.send(());
            }
        }
        for job in &mut captures {
            job.cancelled.store(true, Ordering::Release);
            if let Some(cancel) = job.cancel.take() {
                let _ = cancel.send(());
            }
        }
        async move {
            for mut job in captures {
                if tokio::time::timeout_at(deadline, &mut job.task)
                    .await
                    .is_err()
                {
                    if job.device_active.load(Ordering::Acquire) {
                        capture_blocked.store(true, Ordering::Release);
                    }
                    job.task.abort();
                    let _ = job.task.await;
                }
            }
            for mut job in playback {
                if tokio::time::timeout_at(deadline, &mut job.task)
                    .await
                    .is_err()
                {
                    if job.device_active.load(Ordering::Acquire) {
                        playback_blocked.store(true, Ordering::Release);
                    }
                    job.task.abort();
                    let _ = job.task.await;
                }
            }
        }
    }

    pub fn capture_blocked(&self) -> bool {
        self.capture_blocked.load(Ordering::Acquire)
    }

    pub fn playback_blocked(&self) -> bool {
        self.playback_blocked.load(Ordering::Acquire)
    }
}

impl Drop for ClientAudio {
    fn drop(&mut self) {
        if let Ok(slot) = self.capture.get_mut()
            && let Some(mut job) = slot.take()
        {
            job.cancelled.store(true, Ordering::Release);
            if let Some(cancel) = job.cancel.take() {
                let _ = cancel.send(());
            }
            job.task.abort();
        }
        if let Ok(jobs) = self.retired_capture.get_mut() {
            for mut job in jobs.drain(..) {
                job.cancelled.store(true, Ordering::Release);
                if let Some(cancel) = job.cancel.take() {
                    let _ = cancel.send(());
                }
                job.task.abort();
            }
        }
        if let Ok(jobs) = self.playback.get_mut() {
            for mut job in jobs.drain(..) {
                if let Some(stop) = job.stop.take() {
                    let _ = stop.send(());
                }
                job.task.abort();
            }
        }
    }
}

async fn relay_for(
    home: PathBuf,
    remote: Option<AudioRelay>,
    remote_gateway: bool,
    profile: &str,
) -> Result<AudioRelay, ()> {
    if let Some(remote) = remote {
        return Ok(remote);
    }
    if remote_gateway {
        return Err(());
    }
    let options = hermes_client::DiscoverOptions {
        ensure: hermes_client::EnsurePolicy::Never,
        ..Default::default()
    };
    let endpoint = Arc::new(
        hermes_client::discover_with(&home, &options)
            .await
            .map_err(|_| ())?,
    );
    let id = endpoint.resolve_profile(profile).map_err(|_| ())?;
    let client = RestClient::new(
        endpoint.api_origin(),
        Arc::new(NativeHttpMinter::new(home, endpoint.clone(), id)),
        RestOptions {
            get_deadline: Duration::from_secs(120),
            write_deadline: Duration::from_secs(120),
            max_body: 40 * 1024 * 1024,
        },
    )
    .map_err(|_| ())?;
    Ok(AudioRelay::Local(Arc::new(client)))
}
