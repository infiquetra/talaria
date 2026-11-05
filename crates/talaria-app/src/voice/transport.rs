//! Workstation audio transport.

use std::process::Stdio;

pub fn remote_audio_allowed(remote: bool, opt_in: bool) -> bool {
    !remote || opt_in
}

use std::io;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child as AsyncChild, ChildStdin, Command as AsyncCommand};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const AUDIO_LIMIT: usize = 16 * 1024 * 1024;
const CAPTURE_LIMIT: Duration = Duration::from_secs(120);

fn command(argv: &[String]) -> io::Result<AsyncCommand> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "helper command is empty"))?;
    if program.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "helper program is empty",
        ));
    }
    let mut command = AsyncCommand::new(program);
    command.args(args).stderr(Stdio::null()).kill_on_drop(true);
    Ok(command)
}

fn spawn_helper(mut command: AsyncCommand) -> io::Result<AsyncChild> {
    for attempt in 0..10 {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(error)
                if attempt < 9
                    && (error.kind() == io::ErrorKind::ExecutableFileBusy
                        || error.raw_os_error() == Some(26)) =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("loop returns on success or final attempt")
}

async fn kill_and_reap(child: &mut AsyncChild, budget: Duration) -> bool {
    let _ = child.start_kill();
    matches!(tokio::time::timeout(budget, child.wait()).await, Ok(Ok(_)))
}

#[derive(Debug)]
struct CaptureUnreaped;

#[derive(Debug)]
struct PlaybackUnreaped;

impl std::fmt::Display for CaptureUnreaped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("capture helper could not be reaped")
    }
}

impl std::error::Error for CaptureUnreaped {}

impl std::fmt::Display for PlaybackUnreaped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("playback helper could not be reaped")
    }
}

impl std::error::Error for PlaybackUnreaped {}

pub fn capture_unreaped(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<CaptureUnreaped>())
}

/// Captures WAV bytes from stdout. Send `stop\n` on stdin to finalize the WAV.
/// The command is an argument vector, never a shell command.
pub struct Capture {
    stop: mpsc::UnboundedSender<()>,
    cancel: mpsc::UnboundedSender<()>,
    task: Option<JoinHandle<io::Result<Vec<u8>>>>,
}

/// A control path for finalizing a capture while another task awaits its result.
#[derive(Clone)]
pub struct CaptureStop(mpsc::UnboundedSender<()>);

/// A separate cancellation signal that discards the capture and reaps the helper.
#[derive(Clone)]
pub struct CaptureCancel(mpsc::UnboundedSender<()>);

impl CaptureCancel {
    pub fn cancel(&self) {
        let _ = self.0.send(());
    }
}

impl CaptureStop {
    pub fn stop(&self) {
        let _ = self.0.send(());
    }
}

impl Capture {
    pub fn start(argv: &[String]) -> io::Result<Self> {
        let mut cmd = command(argv)?;
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = spawn_helper(cmd)?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("capture stdout unavailable"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("capture stdin unavailable"))?;
        let (stop, mut request) = mpsc::unbounded_channel();
        let (cancel, mut cancelled) = mpsc::unbounded_channel::<()>();
        let task = tokio::spawn(async move {
            let deadline = tokio::time::sleep(CAPTURE_LIMIT);
            tokio::pin!(deadline);
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 8192];
            let result = loop {
                tokio::select! {
                    _ = cancelled.recv() => break Err(io::Error::new(io::ErrorKind::Interrupted, "capture cancelled")),
                    _ = &mut deadline => break Ok(()),
                    _ = request.recv() => break Ok(()),
                    read = stdout.read(&mut buffer) => match read {
                        Ok(0) => break Err(io::Error::new(io::ErrorKind::UnexpectedEof, "capture helper exited before stop")),
                        Ok(count) => if let Err(error) = append_audio(&mut bytes, &buffer[..count]) { break Err(error); },
                        Err(error) => break Err(error),
                    },
                }
            };
            let stopping_started = tokio::time::Instant::now();
            let result = match result {
                Ok(()) => tokio::select! {
                    biased;
                    _ = cancelled.recv() => Err(io::Error::new(io::ErrorKind::Interrupted, "capture cancelled")),
                    result = tokio::time::timeout(Duration::from_millis(450), async {
                    stdin.write_all(b"stop\n").await?;
                    stdin.shutdown().await?;
                    loop {
                        let count = stdout.read(&mut buffer).await?;
                        if count == 0 {
                            break;
                        }
                        append_audio(&mut bytes, &buffer[..count])?;
                    }
                    let status = child.wait().await?;
                    if !status.success() {
                        return Err(io::Error::other("capture helper failed"));
                    }
                    Ok(bytes)
                    }) => match result {
                    Ok(result) => result,
                    Err(_) => Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "capture helper did not stop",
                    )),
                    },
                },
                Err(error) => Err(error),
            };
            if result.is_err() {
                let remaining =
                    Duration::from_millis(500).saturating_sub(stopping_started.elapsed());
                let budget = remaining.min(Duration::from_millis(450));
                if !kill_and_reap(&mut child, budget).await {
                    return Err(io::Error::other(CaptureUnreaped));
                }
            }
            result
        });
        Ok(Self {
            stop,
            cancel,
            task: Some(task),
        })
    }

    pub fn stop_handle(&self) -> CaptureStop {
        CaptureStop(self.stop.clone())
    }

    pub fn cancel_handle(&self) -> CaptureCancel {
        CaptureCancel(self.cancel.clone())
    }

    pub async fn stop(self) -> io::Result<Vec<u8>> {
        self.stop_handle().stop();
        self.finish().await
    }

    pub async fn finish(mut self) -> io::Result<Vec<u8>> {
        let task = self
            .task
            .take()
            .ok_or_else(|| io::Error::other("capture already stopped"))?;
        task.await
            .map_err(|_| io::Error::other("capture task failed"))?
    }

    pub async fn cancel(mut self) {
        let _ = self.cancel.send(());
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

fn append_audio(bytes: &mut Vec<u8>, incoming: &[u8]) -> io::Result<()> {
    if incoming.len() > AUDIO_LIMIT.saturating_sub(bytes.len()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "capture audio exceeds limit",
        ));
    }
    bytes.extend_from_slice(incoming);
    Ok(())
}

/// Playback audio travels on stdin; process termination is an independent control path.
pub struct Playback {
    child: AsyncChild,
    stdin: Option<ChildStdin>,
    stop: mpsc::UnboundedSender<()>,
    requests: mpsc::UnboundedReceiver<()>,
}

/// Independent playback control, usable while audio is being written.
#[derive(Clone)]
pub struct PlaybackStop(mpsc::UnboundedSender<()>);

impl PlaybackStop {
    pub fn stop(&self) {
        let _ = self.0.send(());
    }
}

impl Playback {
    pub fn start(argv: &[String]) -> io::Result<Self> {
        let mut cmd = command(argv)?;
        cmd.stdin(Stdio::piped()).stdout(Stdio::null());
        let mut child = spawn_helper(cmd)?;
        let stdin = child.stdin.take();
        let (stop, requests) = mpsc::unbounded_channel();
        Ok(Self {
            child,
            stdin,
            stop,
            requests,
        })
    }

    pub fn stop_handle(&self) -> PlaybackStop {
        PlaybackStop(self.stop.clone())
    }

    pub async fn play(&mut self, audio: &[u8]) -> io::Result<()> {
        let mut stdin = self
            .stdin
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "playback closed"))?;
        let result = tokio::select! {
            _ = self.requests.recv() => {
                if !kill_and_reap(&mut self.child, Duration::from_millis(100)).await {
                    return Err(io::Error::other(PlaybackUnreaped));
                }
                return Err(io::Error::new(io::ErrorKind::Interrupted, "playback stopped"));
            }
            result = async {
                stdin.write_all(audio).await?;
                stdin.shutdown().await?;
                drop(stdin);
                self.child.wait().await
            } => result,
        };
        let status = result?;
        if !status.success() {
            return Err(io::Error::other("playback helper failed"));
        }
        Ok(())
    }

    pub async fn close(&mut self) -> io::Result<()> {
        self.stdin.take();
        if kill_and_reap(&mut self.child, Duration::from_millis(100)).await {
            Ok(())
        } else {
            Err(io::Error::other(PlaybackUnreaped))
        }
    }
}
