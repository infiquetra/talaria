//! Temporary fixture-only diagnostics; never a production implementation change.
#[cfg(all(debug_assertions, unix))]
mod enabled {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::Instant;
    static STATE: OnceLock<Mutex<(Instant, usize)>> = OnceLock::new();
    static INCOMPLETE: AtomicBool = AtomicBool::new(false);
    static MARKER_ATTEMPTED: AtomicBool = AtomicBool::new(false);
    static FINISHED: AtomicBool = AtomicBool::new(false);
    fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }
    fn incomplete(root: Option<&Path>, mock: bool) {
        INCOMPLETE.store(true, Ordering::SeqCst);
        // Independent of the event mutex/counter; no repeated stderr channel.
        if !MARKER_ATTEMPTED.swap(true, Ordering::SeqCst)
            && let Some(root) = root
        {
            let role = if mock { "mock" } else { "app" };
            if write_new(
                &root.join(format!("acp-trace-{role}-incomplete")),
                b"incomplete\n",
            )
            .is_err()
            {
                INCOMPLETE.store(true, Ordering::SeqCst);
            }
        }
    }
    fn root(mock: bool) -> Option<PathBuf> {
        if std::env::var("TALARIA_ACP_TEST_CASE").ok().as_deref()
            != Some("failed_tool_then_recovery_stays_working_until_end")
        {
            return None;
        }
        let Some(root) = std::env::var_os("TALARIA_ACP_TEST_ROOT") else {
            incomplete(None, mock);
            return None;
        };
        let root = PathBuf::from(root);
        if !fs::symlink_metadata(&root)
            .is_ok_and(|m| m.is_dir() && m.permissions().mode() & 0o077 == 0)
        {
            incomplete(None, mock);
            return None;
        }
        Some(root)
    }
    pub fn event(mock: bool, event: &str) {
        let Some(root) = root(mock) else { return };
        if FINISHED.load(Ordering::SeqCst)
            || !matches!(
                event,
                "prompt_ack"
                    | "prompt_returned"
                    | "mock_write_begin"
                    | "mock_write_flushed"
                    | "app_update_received"
                    | "prompt_response_received"
                    | "prompt_finish_before"
                    | "prompt_finish_done"
                    | "prompt_finish_other"
                    | "fleet_working"
                    | "fleet_done"
                    | "fleet_failed"
                    | "fleet_other"
                    | "checkpoint_working"
                    | "checkpoint_done"
                    | "checkpoint_other"
            )
        {
            incomplete(Some(&root), mock);
            return;
        }
        let Ok(mut state) = STATE
            .get_or_init(|| Mutex::new((Instant::now(), 0)))
            .try_lock()
        else {
            incomplete(Some(&root), mock);
            return;
        };
        if state.1 >= 32 {
            incomplete(Some(&root), mock);
            return;
        }
        let sequence = state.1;
        state.1 += 1;
        let role = if mock { "mock" } else { "app" };
        let line = format!(
            "{role}\t{sequence}\t{}\t{event}\n",
            state.0.elapsed().as_micros()
        );
        if write_new(
            &root.join(format!("acp-trace-{role}-{sequence:02}")),
            line.as_bytes(),
        )
        .is_err()
        {
            incomplete(Some(&root), mock);
        }
    }
    pub fn finish(mock: bool) {
        let Some(root) = root(mock) else { return };
        if FINISHED.swap(true, Ordering::SeqCst) {
            return;
        }
        let count = match STATE.get().and_then(|s| s.try_lock().ok()) {
            Some(state) => state.1,
            None => {
                incomplete(Some(&root), mock);
                0
            }
        };
        if std::thread::panicking() {
            incomplete(Some(&root), mock);
        }
        let role = if mock { "mock" } else { "app" };
        let status = if INCOMPLETE.load(Ordering::SeqCst) {
            "incomplete"
        } else {
            "complete"
        };
        let pending = root.join(format!("acp-trace-{role}-pending"));
        let terminal = root.join(format!("acp-trace-{role}-status"));
        // Publish only a fully written/synced file, exclusively. An interrupted
        // commit leaves pending or nlink!=1, both refused by parent retention.
        if write_new(&pending, format!("{role}\t{count}\t{status}\n").as_bytes())
            .and_then(|_| fs::hard_link(&pending, &terminal))
            .and_then(|_| fs::remove_file(&pending))
            .is_err()
        {
            incomplete(Some(&root), mock);
        }
    }
}
#[doc(hidden)]
pub fn event(mock: bool, event: &str) {
    #[cfg(all(debug_assertions, unix))]
    enabled::event(mock, event);
    #[cfg(not(all(debug_assertions, unix)))]
    let _ = (mock, event);
}
#[doc(hidden)]
pub struct FixtureTraceGuard(bool);
impl Drop for FixtureTraceGuard {
    fn drop(&mut self) {
        #[cfg(all(debug_assertions, unix))]
        enabled::finish(self.0);
        #[cfg(not(all(debug_assertions, unix)))]
        let _ = self.0;
    }
}
#[doc(hidden)]
pub fn guard(mock: bool) -> FixtureTraceGuard {
    FixtureTraceGuard(mock)
}
