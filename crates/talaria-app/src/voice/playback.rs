//! Sentence playback. The helper confirms quiet before a capture may start.

use std::path::PathBuf;
use std::process::Stdio;

use super::transport::Playback;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Child;
use tokio::process::Command;

/// What a stop or quit did. Network calls stay at zero on this path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StopReport {
    /// Gateway methods issued while stopping. This path issues none.
    pub network_methods: usize,
    /// Whether the owned playback process was reaped within the deadline.
    pub quiet: bool,
}

/// Queued sentences for one playback helper.
pub struct SentencePlayback {
    queued: Vec<(u64, String)>,
    child: Option<Child>,
    player: Option<Playback>,
    log: PathBuf,
    audio_epoch: u64,
    stopped_epoch: Option<u64>,
    quiet: bool,
    quit: bool,
}

impl SentencePlayback {
    /// Start the playback helper. `argv` is a program plus arguments, never a shell line.
    pub async fn start(argv: &[String], log: PathBuf) -> std::io::Result<Self> {
        let (program, args) = argv.split_first().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "helper command is empty")
        })?;
        let mut command = Command::new(program);
        command
            .args(args)
            .arg(&log)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let child = loop {
            let mut attempt = 0;
            break loop {
                match command.spawn() {
                    Ok(child) => break child,
                    Err(error)
                        if attempt < 9
                            && (error.kind() == std::io::ErrorKind::ExecutableFileBusy
                                || error.raw_os_error() == Some(26)) =>
                    {
                        attempt += 1;
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => return Err(error),
                }
            };
        };
        Ok(Self {
            queued: Vec::new(),
            child: Some(child),
            player: None,
            log,
            audio_epoch: 1,
            stopped_epoch: None,
            quiet: true,
            quit: false,
        })
    }

    /// Create the owner used by the audio relay. Audio helpers consume binary stdin.
    pub fn for_audio(audio_epoch: u64) -> Self {
        Self {
            queued: Vec::new(),
            child: None,
            player: None,
            log: PathBuf::new(),
            audio_epoch,
            stopped_epoch: None,
            quiet: true,
            quit: false,
        }
    }

    /// Take the next queued sentence for synthesis.
    pub fn next_sentence(&mut self) -> Option<String> {
        if self.queued.is_empty() {
            None
        } else {
            Some(self.queued.remove(0).1)
        }
    }

    /// Start the configured binary-audio playback helper.
    pub fn start_audio(&mut self, argv: &[String]) -> std::io::Result<()> {
        self.player = Some(Playback::start(argv)?);
        Ok(())
    }

    /// Play one synthesized sentence through the owned helper.
    pub async fn play_audio(&mut self, audio: &[u8]) -> std::io::Result<()> {
        self.player
            .as_mut()
            .ok_or_else(|| std::io::Error::other("playback not started"))?
            .play(audio)
            .await
    }

    /// Reap the helper after a sentence finishes.
    pub async fn finish_audio(&mut self) -> std::io::Result<()> {
        if let Some(mut player) = self.player.take() {
            player.close().await?;
        }
        Ok(())
    }

    /// Queue a sentence unless its audio epoch has already been stopped.
    pub fn enqueue(&mut self, audio_epoch: u64, text: impl Into<String>) {
        if !self.quit
            && self
                .stopped_epoch
                .is_none_or(|stopped| audio_epoch > stopped)
        {
            self.queued.push((audio_epoch, text.into()));
        }
    }

    /// Sentences still waiting to play.
    #[must_use]
    pub fn queued(&self) -> Vec<String> {
        self.queued.iter().map(|(_, text)| text.clone()).collect()
    }

    /// The audio epoch a stop invalidates.
    #[must_use]
    pub fn audio_epoch(&self) -> u64 {
        self.audio_epoch
    }

    /// Stop playback and discard queued sentences.
    pub async fn stop(&mut self) -> StopReport {
        self.queued.clear();
        self.stopped_epoch = Some(self.audio_epoch);
        self.quiet = false;
        let mut quiet = true;
        if let Some(mut player) = self.player.take() {
            quiet = player.close().await.is_ok();
        }
        if let Some(mut child) = self.child.take() {
            let deadline = tokio::time::Instant::now() + Duration::from_millis(100);
            if let Some(mut input) = child.stdin.take() {
                let _ = input.write_all(b"stop\n").await;
                let _ = input.flush().await;
                drop(input);
            }
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    _ if tokio::time::Instant::now() < deadline => {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                    _ => {
                        quiet = false;
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        break;
                    }
                }
            }
        }
        StopReport {
            network_methods: 0,
            quiet,
        }
    }

    /// Stop and permanently prevent capture on this owner.
    pub async fn quit(&mut self) -> StopReport {
        self.quit = true;
        self.stop().await
    }

    /// Whether the helper log contains a stop. False until the control path writes one.
    #[must_use]
    pub fn saw_stop(&self) -> bool {
        std::fs::read_to_string(&self.log)
            .ok()
            .is_some_and(|text| text.contains("stop"))
    }

    /// Whether a capture may open. True until quiet is required.
    #[must_use]
    pub fn capture_allowed(&self) -> bool {
        self.quiet && !self.quit
    }

    /// Accept a quiet confirmation, allowing a later capture.
    pub async fn confirm_quiet(&mut self) {
        if !self.quit {
            self.quiet = true;
        }
    }
}

impl Drop for SentencePlayback {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
    }
}
