//! Sentence playback. The helper confirms quiet before a capture may start.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Child;
use tokio::process::Command;

/// What a stop or quit did. Network calls stay at zero on this path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StopReport {
    /// Gateway methods issued while stopping. This path issues none.
    pub network_methods: usize,
}

/// Queued sentences for one playback helper.
pub struct SentencePlayback {
    queued: Vec<(u64, String)>,
    child: Option<Child>,
    log: PathBuf,
}

impl SentencePlayback {
    /// Start the playback helper. `argv` is a program plus arguments, never a shell line.
    pub async fn start(argv: &[String], log: PathBuf) -> std::io::Result<Self> {
        let (program, args) = argv.split_first().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "helper command is empty")
        })?;
        let child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        Ok(Self {
            queued: Vec::new(),
            child: Some(child),
            log,
        })
    }

    /// Remember one sentence. The audio epoch is not consulted yet.
    pub fn enqueue(&mut self, audio_epoch: u64, text: impl Into<String>) {
        self.queued.push((audio_epoch, text.into()));
    }

    /// Sentences still waiting to play.
    #[must_use]
    pub fn queued(&self) -> Vec<String> {
        self.queued.iter().map(|(_, text)| text.clone()).collect()
    }

    /// The audio epoch a stop would invalidate. It does not move yet.
    #[must_use]
    pub fn audio_epoch(&self) -> u64 {
        let _ = self;
        0
    }

    /// Ask the helper to go quiet. The queue is left as it is.
    pub async fn stop(&mut self) -> StopReport {
        let _ = self;
        StopReport { network_methods: 0 }
    }

    /// Quit uses the same unfinished stop.
    pub async fn quit(&mut self) -> StopReport {
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
        let _ = self;
        true
    }

    /// Accept a quiet confirmation. It does not open a capture yet.
    pub async fn confirm_quiet(&mut self) {
        let _ = self;
    }
}

impl Drop for SentencePlayback {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
    }
}
