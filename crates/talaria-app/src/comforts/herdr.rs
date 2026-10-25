//! Herdr pane state reporting through the Herdr command line.
//!
//! Behavior contract: issue 228.

use std::process::Stdio;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use talaria_core::attention::{PaneReport, PaneState};

#[derive(Debug)]
enum Work {
    Report(Vec<String>),
    Release(Vec<String>, Duration, mpsc::Sender<()>),
}

/// Reports pane state to Herdr in the background, at most one process at a time.
#[derive(Debug)]
pub struct HerdrReporter {
    sender: UnboundedSender<Work>,
    notices: Mutex<mpsc::Receiver<String>>,
    pane_id: String,
    last: AtomicU64,
    now_ms: fn() -> u64,
}

impl HerdrReporter {
    /// Build a reporter when inside Herdr and reporting is enabled, else none.
    pub fn from_env(
        enabled: bool,
        env: &dyn Fn(&str) -> Option<String>,
        now_ms: fn() -> u64,
    ) -> Option<Self> {
        Self::build(enabled, env, now_ms, None)
    }

    /// Deliver a failure directly to the application's event channel.
    ///
    /// The callback replaces the internal notice queue and must return promptly.
    pub fn from_env_with_notices(
        enabled: bool,
        env: &dyn Fn(&str) -> Option<String>,
        now_ms: fn() -> u64,
        on_notice: impl Fn(String) + Send + 'static,
    ) -> Option<Self> {
        Self::build(enabled, env, now_ms, Some(Box::new(on_notice)))
    }

    fn build(
        enabled: bool,
        env: &dyn Fn(&str) -> Option<String>,
        now_ms: fn() -> u64,
        on_notice: Option<Box<dyn Fn(String) + Send>>,
    ) -> Option<Self> {
        if !enabled || env("HERDR_ENV").as_deref() != Some("1") {
            return None;
        }
        let pane_id = env("HERDR_PANE_ID").filter(|value| !value.is_empty())?;
        let binary = env("HERDR_BIN_PATH").filter(|value| !value.is_empty())?;
        let (sender, receiver) = unbounded_channel();
        let (notice_tx, notices) = mpsc::channel();
        let on_notice = on_notice.unwrap_or_else(|| {
            Box::new(move |notice| {
                let _ = notice_tx.send(notice);
            })
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        std::thread::Builder::new()
            .name("talaria-herdr".to_owned())
            .spawn(move || runtime.block_on(worker(binary, receiver, on_notice)))
            .ok()?;
        Some(Self {
            sender,
            notices: Mutex::new(notices),
            pane_id,
            last: AtomicU64::new(0),
            now_ms,
        })
    }

    /// Report one state. Never blocks; the newest waiting state wins.
    pub fn report(&self, pane: &PaneReport, session: Option<(&str, &str)>) {
        let state = match pane.state {
            PaneState::Idle => "idle",
            PaneState::Working => "working",
            PaneState::Blocked => "blocked",
        };
        let mut args = vec![
            "pane".into(),
            "report-agent".into(),
            "--source".into(),
            "talaria".into(),
            "--agent".into(),
            "talaria".into(),
            "--state".into(),
            state.into(),
            "--seq".into(),
            self.next_seq().to_string(),
        ];
        if pane.state == PaneState::Blocked
            && let Some(message) = &pane.message
        {
            let message = talaria_core::attention::sanitize_alert_field(message, 80);
            args.extend(["--message".into(), message]);
        }
        if let Some((id, _)) = session {
            args.extend(["--agent-session-id".into(), id.to_owned()]);
        }
        args.push(self.pane_id.clone());
        if let Some((id, profile)) = session {
            let mut resume = vec!["talaria".to_owned(), "--resume".into(), id.to_owned()];
            if profile != "default" {
                resume.extend(["--profile".into(), profile.to_owned()]);
            }
            if resume.iter().all(|argument| {
                !argument
                    .chars()
                    .any(|character| character == '\'' || character.is_control())
            }) && resume
                .iter()
                .map(|argument| argument.len() + 1)
                .sum::<usize>()
                <= 8192
            {
                args.push("--".into());
                args.extend(resume);
            }
        }
        let _ = self.sender.send(Work::Report(args));
    }

    /// Release the pane, waiting at most the timeout.
    pub fn release(&self, timeout: Duration) {
        let timeout = timeout.min(Duration::from_millis(500));
        let args = vec![
            "pane".into(),
            "release-agent".into(),
            "--source".into(),
            "talaria".into(),
            "--agent".into(),
            "talaria".into(),
            "--seq".into(),
            self.next_seq().to_string(),
            self.pane_id.clone(),
        ];
        let (done, wait) = mpsc::channel();
        if self.sender.send(Work::Release(args, timeout, done)).is_ok() {
            let _ = wait.recv_timeout(timeout);
        }
    }

    /// Drain queued failures. Empty when a notice callback was supplied.
    pub fn take_notices(&self) -> Vec<String> {
        self.notices
            .lock()
            .map(|receiver| receiver.try_iter().collect())
            .unwrap_or_default()
    }

    fn next_seq(&self) -> u64 {
        let mut last = self.last.load(Ordering::SeqCst);
        loop {
            let next = last.saturating_add(1).max((self.now_ms)());
            match self
                .last
                .compare_exchange(last, next, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return next,
                Err(actual) => last = actual,
            }
        }
    }
}

async fn worker(
    binary: String,
    mut receiver: UnboundedReceiver<Work>,
    on_notice: Box<dyn Fn(String) + Send>,
) {
    let mut pending = None;
    let mut notified = false;
    loop {
        let Some(work) = (if pending.is_some() {
            pending.take()
        } else {
            receiver.recv().await
        }) else {
            break;
        };
        let (args, limit, release) = match work {
            Work::Report(args) => (args, Duration::from_secs(2), None),
            Work::Release(args, limit, done) => (args, limit, Some(done)),
        };
        let result = tokio::process::Command::new(&binary)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn();
        let mut child = match result {
            Ok(child) => child,
            Err(error) => {
                notice_once(
                    &*on_notice,
                    &mut notified,
                    &super::editor::io_reason(&error),
                );
                if let Some(done) = release {
                    let _ = done.send(());
                    break;
                }
                continue;
            }
        };
        let deadline = tokio::time::sleep(limit);
        tokio::pin!(deadline);
        let reason = loop {
            tokio::select! {
                result = child.wait() => break match result { Ok(status) if status.success() => None, Ok(_) => Some("non-zero exit".to_owned()), Err(error) => Some(super::editor::io_reason(&error)) },
                _ = &mut deadline => { let _ = child.kill().await; break Some("timed out".to_owned()); },
                next = receiver.recv(), if release.is_none() => {
                    match next {
                        Some(work @ Work::Release(..)) => { pending = Some(work); let _ = child.kill().await; break None; },
                        Some(work) => pending = Some(work),
                        None => { let _ = child.kill().await; return; }
                    }
                }
            }
        };
        if let Some(reason) = reason {
            notice_once(&*on_notice, &mut notified, &reason);
        }
        if let Some(done) = release {
            let _ = done.send(());
            break;
        }
        while !matches!(pending, Some(Work::Release(..))) {
            let Ok(work) = receiver.try_recv() else {
                break;
            };
            let releasing = matches!(work, Work::Release(..));
            pending = Some(work);
            if releasing {
                break;
            }
        }
    }
}

fn notice_once(on_notice: &dyn Fn(String), notified: &mut bool, reason: &str) {
    if !*notified {
        *notified = true;
        on_notice(format!(
            "Herdr pane state could not be reported ({reason}); continuing without it."
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_delivers_failure_once_without_another_report() {
        let (sender, receiver) = mpsc::channel();
        let env = |name: &str| match name {
            "HERDR_ENV" => Some("1".to_owned()),
            "HERDR_PANE_ID" => Some("test-pane".to_owned()),
            "HERDR_BIN_PATH" => Some("/nonexistent-talaria-herdr-test".to_owned()),
            _ => None,
        };
        let reporter = HerdrReporter::from_env_with_notices(
            true,
            &env,
            || 1,
            move |notice| {
                let _ = sender.send(notice);
            },
        )
        .unwrap();
        let pane = PaneReport {
            state: PaneState::Idle,
            message: None,
        };
        reporter.report(&pane, None);
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            "Herdr pane state could not be reported (not found); continuing without it."
        );
        assert!(reporter.take_notices().is_empty());
        reporter.report(&pane, None);
        assert!(receiver.recv_timeout(Duration::from_millis(200)).is_err());
    }
}
