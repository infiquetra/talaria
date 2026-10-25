//! Every user-visible rewind string. Issue 233. Checkpoint rewind and diff.

use super::checkpoints::NoMatch;

/// Which call failed or was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    /// `session.resume`.
    ReadSession,
    /// `rollback.list`.
    ListCheckpoints,
    /// `rollback.diff`.
    CheckpointDiff,
    /// `rollback.session_diff`.
    SessionDiff,
    /// `session.mutate`.
    Mutate,
    /// `rollback.restore`.
    Restore,
}

/// Why a call did not return a decoded success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallFailure {
    /// The gateway refused.
    Refused {
        reason: String,
        field: Option<String>,
        detail: Option<String>,
        revision: Option<i64>,
        execution_generation: Option<i64>,
    },
    /// The reply was lost or timed out.
    Unknown,
    /// The reply did not decode. The gateway may have done the work.
    Unreadable,
    /// Nothing was sent.
    NotSent(String),
}

/// What a restore reply told us. `directory` is never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreOutcome {
    /// The restore reported success.
    pub success: bool,
    /// Short hash, when present.
    pub restored_to: Option<String>,
    /// Checkpoint message, when present.
    pub reason: Option<String>,
    /// The one restored path, when present.
    pub file: Option<String>,
    /// Paths a safe restore wrote.
    pub restored_files: Option<Vec<String>>,
    /// Paths left because the person edited them.
    pub skipped_user_edits: Option<Vec<String>>,
    /// Paths left because they are over the size limit.
    pub skipped_oversize: Option<Vec<String>>,
    /// Paths a delete could not remove.
    pub failed_deletes: Option<Vec<String>>,
    /// Rows Hermes removed after a whole-folder restore.
    pub history_removed: Option<i64>,
    /// Error text when success is false.
    pub error: Option<String>,
}

/// A session diff reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDiffOutcome {
    /// Checkpoints are on.
    pub enabled: bool,
    /// The diff was computed.
    pub success: bool,
    /// Stat text.
    pub stat: String,
    /// Patch text, possibly cut.
    pub diff: String,
    /// Line count of the uncut diff.
    pub diff_lines: u64,
    /// No checkpoint yet, or no changes. The reply does not say which.
    pub empty: bool,
    /// Error text, when present.
    pub error: Option<String>,
}

/// A diff the overlay can draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffDoc {
    /// Checkpoints are off.
    Off,
    /// Nothing to show. The string says both possible causes.
    Empty(String),
    /// The diff could not be computed.
    Failed(String),
    /// A stat and a patch.
    Text {
        /// Title line.
        title: String,
        /// Second line, for the session diff.
        subtitle: Option<String>,
        /// Stat text.
        stat: String,
        /// Patch text.
        diff: String,
        /// Omit the patch body.
        stat_only: bool,
        /// Cut notice, when one is due.
        cut_note: Option<String>,
    },
}

// Fixed texts. Each doc comment names its text id.

/// Text T1. A turn is running, so a rewind cannot open.
pub const TURN_RUNNING: &str = "A reply is running. Stop it with Esc, then rewind.";
/// Text T2. Overlay body while the conversation loads.
pub const LOADING_CONVERSATION: &str = "Loading the conversation…";
/// Text T3. The conversation has no eligible prompt.
pub const NOTHING_TO_REWIND: &str =
    "There is nothing to rewind: this conversation has no prompts yet.";
/// Text T6. An attachment was not put back in the composer.
pub const ATTACHMENTS_NOT_RESTORED: &str =
    "The prompt's attachments were not restored; only its text is back in the composer.";
/// Text T9. The reply was lost and a re-read shows the rewind went through. Followed by T31.
pub const LOST_REPLY_REWIND_WENT_THROUGH: &str =
    "Talaria lost Hermes's reply, but the rewind went through.";
/// Text T10. The reply was lost and a re-read shows nothing changed.
pub const LOST_REPLY_REWIND_DID_NOT_HAPPEN: &str =
    "Talaria lost Hermes's reply. The rewind did not happen; nothing was changed.";
/// Text T11. The reply to a one-file restore was lost.
pub const LOST_REPLY_RESTORE_FILE: &str = "Talaria lost Hermes's reply, so the restore may or may not have happened. Run /diff to see the files now.";
/// Text T11w. The reply to a whole-folder restore was lost.
pub const LOST_REPLY_RESTORE_FOLDER: &str = "Talaria lost Hermes's reply, so the restore may or may not have happened, and Hermes may also have removed the last exchange. Run /diff to see the files and /rewind to see the conversation.";
/// Text T12. The chosen prompt is not in the conversation any more.
pub const PROMPT_GONE: &str = "That prompt is no longer in the conversation (it may have been rewound in another window). Open /rewind again.";
/// Text T13. The revision or generation fence failed.
pub const SESSION_CHANGED: &str = "The session changed while you were choosing (a new turn, or another window). Nothing was rewound. Open /rewind again.";
/// Text T15. Hermes will not rewind while a reply runs or prompts are queued.
pub const REWIND_BLOCKED_BUSY: &str = "Hermes will not rewind while a reply is running or prompts are queued. Stop the reply or clear the queue, then try again.";
/// Text T16. Hermes is compacting the conversation.
pub const REWIND_BLOCKED_COMPACTING: &str =
    "Hermes is compacting this conversation. Wait for it to finish, then rewind.";
/// Text T17. Hermes does not know how the last turn ended.
pub const REWIND_BLOCKED_UNKNOWN_EXECUTION: &str = "Hermes does not know yet how the last turn ended, so it will not rewind. Wait a moment and try again.";
/// Text T18. The prompt was summarized when the conversation was compacted.
pub const PROMPT_COMPACTED: &str = "That prompt was summarized when the conversation was compacted, so Hermes cannot rewind to it. Choose a later prompt.";
/// Text T21. A whole-folder restore needs an idle session. Local only.
pub const RESTORE_NEEDS_IDLE: &str = "Hermes restores the whole folder only between turns. Stop the reply first, or restore one file with /rollback <n> <file>.";
/// Text T23. A whole-folder restore is blocked by queued prompts. Local only.
pub const RESTORE_BLOCKED_BY_QUEUE: &str = "Hermes will not restore the whole folder while prompts are queued: it would restore the files but could not rewind the conversation. Clear the queue first.";
/// Text T24. Usage of `/rollback`.
pub const ROLLBACK_USAGE: &str =
    "Usage: /rollback [list] · /rollback diff <n|hash> · /rollback <n|hash> [file] [--all]";
/// Text T25. Usage of `/undo`.
pub const UNDO_USAGE: &str =
    "Usage: /undo [N]. Removes your last N prompts and their replies (default 1).";
/// Text T27. Nothing to retry, also for an empty prompt text.
pub const NOTHING_TO_RETRY: &str = "Nothing to retry: this conversation has no prompts yet.";
/// Text T28. `/retry` cannot resend attachments.
pub const RETRY_HAS_ATTACHMENTS: &str = "/retry resends text only, and the last prompt has attachments. Use /rewind and send it again instead.";
/// Text T30. `/diff` takes no git arguments.
pub const GIT_DIFF_HINT: &str = "/diff here shows what changed since Hermes's oldest kept checkpoint. For git's view of the folder, run !git diff (or !git diff --staged).";
/// Text T34. The conversation was reloaded after an earlier failed reload.
pub const RELOADED_MARKER: &str = "── Talaria reloaded the conversation. Lines above that are not in it any more were removed by an earlier rewind. ──";
/// Text T36. The target's message count changed between the confirm screen and Enter.
pub const CONVERSATION_CHANGED_WARNING: &str = "The conversation changed since this screen opened. Check the numbers above, then press Enter again.";
/// Text C1. Checkpoints are off.
pub const CHECKPOINTS_OFF: &str = "Checkpoints are off for this session, so Hermes keeps no file snapshots to compare or restore. They are turned on with checkpoints.enabled: true in the profile's config.yaml.";
/// Text C2. Checkpoints are on and none exist.
pub const NO_CHECKPOINTS: &str = "No checkpoints yet. Hermes takes one just before the agent writes or patches a file, or runs a destructive command.";
/// Text C4. The session runs its commands in a container.
pub const CONTAINER_BACKEND: &str =
    "This session runs its commands in a container, so Hermes keeps no checkpoints of its files.";
/// Text C6. A session diff with nothing to show.
pub const NO_CHANGES_TO_SHOW: &str = "No changes to show. Either Hermes has no checkpoint for this folder yet, or the files match the oldest one it keeps.";
/// Text C9. A checkpoint diff at Hermes's 4,000 character limit.
pub const CHECKPOINT_DIFF_CUT_NOTE: &str = "Hermes sends at most 4,000 characters of a checkpoint diff, so this one may be cut. /diff shows the whole session.";
/// Text C10. Turns footer when checkpoints are off.
pub const FOOTER_CHECKPOINTS_OFF: &str = "checkpoints off: files cannot be restored";
/// Text C11. Diff body while loading.
pub const COMPARING_FILES: &str = "Comparing files…";
/// Text C13. Session diff subtitle.
pub const SESSION_DIFF_SUBTITLE: &str =
    "Includes every change in this folder since then: the agent's, yours and other sessions'.";
/// Text C14. Overlay body while a write runs.
pub const WORKING: &str = "Working…";
/// Text W2. Known limits of the checkpoint match, shown on the files action.
pub const CHECKPOINT_LIMITS_WARNING: &str = "Checkpoints belong to the folder: edits other sessions made here since then are undone too. Files changed since Hermes last wrote them (by hand, or by a shell command) are kept; /rollback <n> --all overwrites them.";
/// Text F1. Nothing was sent because there is no connection.
pub const NOT_CONNECTED: &str = "Not connected to Hermes; nothing was sent.";
/// Text F2. A read reply could not be decoded.
pub const REPLY_UNREADABLE: &str = "Hermes's reply could not be read; nothing was changed.";
/// Text F3. A read timed out.
pub const REPLY_TIMED_OUT: &str = "Hermes did not answer in time; nothing was changed. Try again.";
/// Text F4. The verify read after a lost reply also failed.
pub const LOST_REPLY_UNCHECKED: &str = "Talaria lost Hermes's reply and could not check the result. Open /rewind to see the conversation.";

/// Columns the prompt preview may take in a marker or notice.
pub const PREVIEW_COLUMNS: usize = 60;
/// Code points of a prompt kept in text T5.
pub const DRAFT_NOTICE_CODE_POINTS: usize = 200;
/// Code points of a refusal reason shown in a text.
pub const REASON_CODE_POINTS: usize = 64;
/// Code points Hermes sends of a checkpoint diff at most.
pub const CHECKPOINT_DIFF_LIMIT: usize = 4_000;

/// The first `max` code points of `text`.
#[must_use]
pub fn cut_code_points(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// `text` cut to `max_columns` display columns, ending in `…` when anything was cut.
#[must_use]
pub fn cut_columns(text: &str, max_columns: usize) -> String {
    let total: usize = text.chars().map(char_columns).sum();
    if total <= max_columns {
        return text.to_string();
    }
    let budget = max_columns.saturating_sub(1);
    let mut used = 0;
    let mut out = String::new();
    for ch in text.chars() {
        let width = char_columns(ch);
        if used + width > budget {
            break;
        }
        used += width;
        out.push(ch);
    }
    out.push('…');
    out
}

/// A prompt preview cut to 60 display columns, as `<p>` in the texts below.
#[must_use]
pub fn cut_preview(preview: &str) -> String {
    cut_columns(preview, PREVIEW_COLUMNS)
}

/// `1 message`, `2 messages`: the noun with an `s` unless `count` is 1.
#[must_use]
pub fn count_noun(count: u64, singular: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {singular}s")
    }
}

/// Display columns of one character. A small table; wide characters count two.
fn char_columns(ch: char) -> usize {
    let code = u32::from(ch);
    if code < 0x20 || (0x7f..0xa0).contains(&code) {
        return 0;
    }
    const ZERO: [(u32, u32); 9] = [
        (0x0300, 0x036f),
        (0x1ab0, 0x1aff),
        (0x1dc0, 0x1dff),
        (0x200b, 0x200f),
        (0x2060, 0x2064),
        (0x20d0, 0x20ff),
        (0xfe00, 0xfe0f),
        (0xfe20, 0xfe2f),
        (0xfeff, 0xfeff),
    ];
    const WIDE: [(u32, u32); 14] = [
        (0x1100, 0x115f),
        (0x2e80, 0x303e),
        (0x3041, 0x33ff),
        (0x3400, 0x4dbf),
        (0x4e00, 0x9fff),
        (0xa000, 0xa4cf),
        (0xac00, 0xd7a3),
        (0xf900, 0xfaff),
        (0xfe30, 0xfe6f),
        (0xff00, 0xff60),
        (0xffe0, 0xffe6),
        (0x1f300, 0x1f64f),
        (0x1f680, 0x1f9ff),
        (0x20000, 0x3fffd),
    ];
    if ZERO
        .iter()
        .any(|(low, high)| (*low..=*high).contains(&code))
    {
        0
    } else if WIDE
        .iter()
        .any(|(low, high)| (*low..=*high).contains(&code))
    {
        2
    } else {
        1
    }
}

/// Text T4. The marker committed after a conversation-only rewind.
#[must_use]
pub fn rewound_marker(ordinal: usize, preview: &str, removed: u64) -> String {
    format!(
        "── Rewound to before prompt #{ordinal} (\"{}\"). Hermes removed {}. The lines above from that prompt on are no longer part of the conversation. Files were not changed. ──",
        cut_preview(preview),
        count_noun(removed, "message"),
    )
}

/// Text T4f, first line. `removed` is `rewound_count` when Talaria sent the rewind, else
/// `history_removed`. The `restore_summary` lines follow it.
#[must_use]
pub fn restored_and_rewound_marker(
    short_hash: &str,
    checkpoint_message: &str,
    ordinal: usize,
    preview: &str,
    removed: u64,
) -> String {
    format!(
        "── Restored files to checkpoint {short_hash} ({checkpoint_message}) and rewound to before prompt #{ordinal} (\"{}\"). Hermes removed {}. The lines above from that prompt on are no longer part of the conversation. ──",
        cut_preview(preview),
        count_noun(removed, "message"),
    )
}

/// Text T5. The composer already holds a draft, so the removed prompt is only quoted.
#[must_use]
pub fn draft_kept(prompt_text: &str) -> String {
    format!(
        "Your draft was kept. The removed prompt was: {}",
        cut_code_points(prompt_text, DRAFT_NOTICE_CODE_POINTS)
    )
}

/// Text T14. The files were restored but the further rewind was refused.
/// `refusal` is the text of the refusal, for example from `mutate_refusal_text`.
#[must_use]
pub fn restored_but_rewind_failed(short_hash: &str, ordinal: usize, refusal: &str) -> String {
    let stop = if refusal.ends_with('.') { "" } else { "." };
    format!(
        "Restored files to checkpoint {short_hash}, and Hermes removed the last exchange, but rewinding further to prompt #{ordinal} failed: {refusal}{stop} Run /rewind to try again."
    )
}

/// Text T20. Hermes answered a restore with `success: false`.
#[must_use]
pub fn restore_not_done(error: &str) -> String {
    format!("Hermes did not restore anything: {error}")
}

/// Text T26. Fewer prompts exist than `/undo` asked for. `available` is how many there are.
#[must_use]
pub fn too_few_prompts_to_undo(available: usize) -> String {
    match available {
        0 => "There is no prompt to undo.".to_string(),
        1 => "There is only 1 prompt to undo.".to_string(),
        n => format!("There are only {n} prompts to undo."),
    }
}

/// Text T29. The session was rewound in another window. `removed` is `rewound_count`.
#[must_use]
pub fn rewound_elsewhere_marker(removed: Option<u64>) -> String {
    let what = match removed {
        Some(count) => format!("Hermes removed {}", count_noun(count, "message")),
        None => "messages were removed".to_string(),
    };
    format!(
        "── This session was rewound in another window: {what}. Lines above from that point on are no longer part of the conversation. ──"
    )
}

/// Text T31. A re-read shows the prompt and everything after it are gone.
#[must_use]
pub fn conversation_changed_marker(ordinal: usize, preview: &str) -> String {
    format!(
        "── The conversation changed: prompt #{ordinal} (\"{}\") and everything after it are no longer part of it. ──",
        cut_preview(preview)
    )
}

/// Text T32. Hermes's restore also removed an older prompt than the one chosen.
#[must_use]
pub fn restore_removed_older_prompt(ordinal: usize, preview: &str) -> String {
    format!(
        "Hermes's restore also removed prompt #{ordinal} (\"{}\"), which is older than the prompt you chose, because Hermes picks the newest turn by its own rule. Run /rewind to see the conversation.",
        cut_preview(preview)
    )
}

/// Text T33. Hermes changed the conversation and the reload failed.
#[must_use]
pub fn reload_failed(reason: &str) -> String {
    format!(
        "Hermes changed the conversation, but Talaria could not reload it ({reason}). The lines above may still show removed turns as current. Open /rewind to reload it."
    )
}

/// Text T35. The prompt was removed for `/retry` but sending it again failed.
#[must_use]
pub fn resend_failed(reason: &str) -> String {
    format!("The last prompt was removed, but sending it again failed ({reason}).")
}

/// Text C3. The typed target is not a checkpoint.
#[must_use]
pub fn no_such_checkpoint(arg: &str) -> String {
    format!("There is no checkpoint {arg}. /rollback lists them; number 1 is the newest.")
}

/// Text C5. A checkpoint diff came back empty.
#[must_use]
pub fn no_checkpoint_differences(short_hash: &str) -> String {
    format!(
        "Hermes returned no differences for checkpoint {short_hash}. Either the files match it, or Hermes could not read that checkpoint; its reply does not say which."
    )
}

/// Text C7. The session diff could not be computed.
#[must_use]
pub fn session_diff_failed(error: &str) -> String {
    format!("Hermes could not compute the session diff: {error}")
}

/// Text C8. Hermes cut a session diff.
#[must_use]
pub fn session_diff_cut_note(shown: usize, total: u64) -> String {
    format!("Hermes cut this diff to fit one message: showing {shown} of {total} lines.")
}

/// Text C12. Turns footer when the checkpoint list could not be read.
#[must_use]
pub fn footer_checkpoints_unknown(reason: &str) -> String {
    format!("checkpoints unknown: {reason}")
}

/// Text W1. The match is the oldest checkpoint Hermes keeps. `clock` is `clock_text`.
#[must_use]
pub fn oldest_checkpoint_warning(clock: &str) -> String {
    format!(
        "This is the oldest checkpoint Hermes still keeps. Edits made after this prompt but before {clock} may not be undone."
    )
}

/// Text F5. The rewind reply was lost and a re-read cannot say whether the rewind removed the prompt.
#[must_use]
pub fn lost_reply_prompt_gone(ordinal: usize) -> String {
    format!(
        "Talaria lost Hermes's reply. Prompt #{ordinal} is no longer in the conversation, but Talaria cannot tell whether this rewind removed it. Open /rewind to see the conversation."
    )
}

/// Text for a refused `session.mutate` (T12 to T19).
#[must_use]
pub fn mutate_refusal_text(reason: &str, field: Option<&str>, detail: Option<&str>) -> String {
    match reason {
        "revision_conflict" | "stale_generation" => SESSION_CHANGED.to_string(),
        "invalid_params" if detail.is_some_and(|d| d.starts_with("compacted:")) => {
            PROMPT_COMPACTED.to_string()
        }
        "invalid_params" if field == Some("target_message_id") => PROMPT_GONE.to_string(),
        "session_busy" if field == Some("compression") => REWIND_BLOCKED_COMPACTING.to_string(),
        "session_busy" => REWIND_BLOCKED_BUSY.to_string(),
        "unknown_execution" => REWIND_BLOCKED_UNKNOWN_EXECUTION.to_string(),
        "runtime_coordination_required" => {
            "This Hermes gateway cannot rewind this session (runtime_coordination_required)."
                .to_string()
        }
        "runtime_draining" => {
            "Hermes is restarting and refuses changes for now. Try again when it is back."
                .to_string()
        }
        "not_found" => NO_SESSION.to_string(),
        "permission_denied" => {
            "Hermes refused: this connection may not change the session (permission_denied)."
                .to_string()
        }
        "unknown_method" => unknown_method_text("session.mutate"),
        "storage_unavailable" => "Hermes could not use its session store (storage_unavailable). Open /rewind to check what happened.".to_string(),
        other => {
            let mut text = format!(
                "Hermes refused the rewind ({}).",
                cut_code_points(other, REASON_CODE_POINTS)
            );
            push_detail(&mut text, detail);
            text
        }
    }
}

const NO_SESSION: &str = "This session no longer exists on the gateway.";

fn unknown_method_text(method: &str) -> String {
    format!(
        "This Hermes gateway does not offer {method}. Talaria needs the Hermes release it was built for, or later."
    )
}

fn push_detail(text: &mut String, detail: Option<&str>) {
    if let Some(detail) = detail {
        text.push('\n');
        text.push_str(detail);
    }
}

/// The gateway method a call kind names.
fn method_name(call: CallKind) -> &'static str {
    match call {
        CallKind::ReadSession => "session.resume",
        CallKind::ListCheckpoints => "rollback.list",
        CallKind::CheckpointDiff => "rollback.diff",
        CallKind::SessionDiff => "rollback.session_diff",
        CallKind::Mutate => "session.mutate",
        CallKind::Restore => "rollback.restore",
    }
}

/// What a call kind does, for `may not <action>` and `refused to <action>`.
fn action_name(call: CallKind) -> &'static str {
    match call {
        CallKind::ReadSession => "read the session",
        CallKind::ListCheckpoints => "list checkpoints",
        CallKind::CheckpointDiff | CallKind::SessionDiff => "compare files",
        CallKind::Mutate => "change the session",
        CallKind::Restore => "restore files",
    }
}

/// Text for a refused read, from the refusal table (R1).
///
/// For `invalid_checkpoint` the `detail` is the checkpoint as typed; without it the generic
/// refusal line is used. A refused `Mutate` is answered by `mutate_refusal_text`.
#[must_use]
pub fn read_refusal_text(call: CallKind, reason: &str, detail: Option<&str>) -> String {
    if call == CallKind::Mutate {
        return mutate_refusal_text(reason, None, detail);
    }
    match (reason, detail) {
        ("invalid_checkpoint", Some(arg)) => no_such_checkpoint(arg),
        ("unsupported_terminal_backend", _) => CONTAINER_BACKEND.to_string(),
        ("not_found", _) => NO_SESSION.to_string(),
        ("unknown_method", _) => unknown_method_text(method_name(call)),
        ("permission_denied", _) => format!(
            "Hermes refused: this connection may not {} (permission_denied).",
            action_name(call)
        ),
        (other, _) => {
            let mut text = format!(
                "Hermes refused to {} ({}).",
                action_name(call),
                cut_code_points(other, REASON_CODE_POINTS)
            );
            push_detail(&mut text, detail);
            text
        }
    }
}

/// Text for a refused restore (T22 or the refusal table).
#[must_use]
pub fn restore_refusal_text(whole_folder: bool, reason: &str, detail: Option<&str>) -> String {
    let clear_refusal = matches!(
        reason,
        "invalid_params" | "permission_denied" | "not_found" | "unknown_method"
    );
    if whole_folder && !clear_refusal {
        return format!(
            "Hermes reported {} after the restore started. It may have restored the files without rewinding the conversation. Run /diff to check the files and /rewind to check the conversation.",
            cut_code_points(reason, REASON_CODE_POINTS)
        );
    }
    read_refusal_text(CallKind::Restore, reason, detail)
}

/// Text for a failed call (F1 to F3 and the refusal table).
#[must_use]
pub fn failure_text(f: &CallFailure, call: CallKind) -> String {
    match f {
        CallFailure::Refused {
            reason,
            field,
            detail,
            ..
        } => {
            if call == CallKind::Mutate {
                mutate_refusal_text(reason, field.as_deref(), detail.as_deref())
            } else {
                read_refusal_text(call, reason, detail.as_deref())
            }
        }
        CallFailure::Unknown => REPLY_TIMED_OUT.to_string(),
        CallFailure::Unreadable => REPLY_UNREADABLE.to_string(),
        CallFailure::NotSent(_) => NOT_CONNECTED.to_string(),
    }
}

/// At most the first five names, comma separated, then ` and <m> more`.
fn name_list(paths: &[String]) -> String {
    let shown: Vec<&str> = paths.iter().take(5).map(String::as_str).collect();
    let mut text = shown.join(", ");
    if paths.len() > 5 {
        text.push_str(&format!(" and {} more", paths.len() - 5));
    }
    text
}

/// Lines that say what a restore did. `target_label` is the typed target or the row index.
#[must_use]
pub fn restore_summary(r: &RestoreOutcome, target_label: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let checkpoint = r
        .restored_to
        .as_deref()
        .map(|short| format!(" checkpoint {short}"));
    lines.push(match (&r.file, &checkpoint) {
        (Some(file), Some(checkpoint)) => format!("Restored {file} from{checkpoint}."),
        (Some(file), None) => format!("Restored {file}."),
        (None, checkpoint) => {
            let to = checkpoint
                .as_ref()
                .map_or(String::new(), |c| format!(" to{c}"));
            let why = r
                .reason
                .as_deref()
                .map_or(String::new(), |reason| format!(" ({reason})"));
            format!("Restored files{to}{why}.")
        }
    });
    if let Some(files) = &r.restored_files {
        lines.push(format!(
            "Restored {}.",
            count_noun(files.len() as u64, "file")
        ));
    }
    if let Some(files) = r.skipped_user_edits.as_ref().filter(|f| !f.is_empty()) {
        lines.push(format!(
            "Kept {} you edited by hand: {}. Use /rollback {target_label} --all to overwrite them.",
            count_noun(files.len() as u64, "file"),
            name_list(files)
        ));
    }
    if let Some(files) = r.skipped_oversize.as_ref().filter(|f| !f.is_empty()) {
        lines.push(format!(
            "Kept {} larger than the checkpoint size limit: {}.",
            count_noun(files.len() as u64, "file"),
            name_list(files)
        ));
    }
    if let Some(files) = r.failed_deletes.as_ref().filter(|f| !f.is_empty()) {
        lines.push(format!(
            "Could not delete {}: {}.",
            count_noun(files.len() as u64, "file"),
            name_list(files)
        ));
    }
    if let Some(removed) = r.history_removed.filter(|removed| *removed > 0) {
        lines.push(format!(
            "Hermes also removed the last exchange ({}) from the conversation.",
            count_noun(removed.unsigned_abs(), "message")
        ));
    }
    lines
}

/// Age of a prompt relative to `now_ms`.
#[must_use]
pub fn relative_age(timestamp_s: f64, now_ms: u64) -> String {
    let elapsed = now_ms as f64 / 1000.0 - timestamp_s;
    // A timestamp in the future, or one that is not a number, counts as just now.
    if elapsed.is_nan() || elapsed < 60.0 {
        return "just now".to_string();
    }
    let seconds = elapsed.floor() as u64;
    if seconds < 3_600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h ago", seconds / 3_600)
    } else {
        format!("{}d ago", seconds / 86_400)
    }
}

/// The dim line for a prompt with no checkpoint.
#[must_use]
pub fn no_match_text(m: &NoMatch) -> &'static str {
    match m {
        NoMatch::Off => "Restore files: checkpoints are off",
        NoMatch::Unknown => "Restore files: checkpoints could not be listed",
        NoMatch::NoTime => "Restore files: Hermes stored no time for this prompt",
        NoMatch::NoChanges => "Restore files: no file changes recorded since this prompt",
        NoMatch::NoCheckpointForEdits => {
            "Restore files: Hermes recorded no checkpoint before this prompt's edits"
        }
    }
}

/// Line count using the same breaks as Python's `splitlines`.
#[must_use]
pub fn python_line_count(s: &str) -> usize {
    let mut count = 0;
    let mut open_line = false;
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                count += 1;
                open_line = false;
            }
            '\n' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => {
                count += 1;
                open_line = false;
            }
            _ => open_line = true,
        }
    }
    count + usize::from(open_line)
}

/// The document for a session diff (C1, C6, C7, C8, C13).
#[must_use]
pub fn session_diff_doc(r: &SessionDiffOutcome, stat_only: bool) -> DiffDoc {
    if !r.enabled {
        return DiffDoc::Off;
    }
    if !r.success {
        let error = r.error.as_deref().unwrap_or("no reason given");
        return DiffDoc::Failed(session_diff_failed(error));
    }
    if r.empty {
        return DiffDoc::Empty(NO_CHANGES_TO_SHOW.to_string());
    }
    let shown = python_line_count(&r.diff);
    let cut_note =
        (r.diff_lines > shown as u64).then(|| session_diff_cut_note(shown, r.diff_lines));
    DiffDoc::Text {
        title: "Changes since the oldest kept checkpoint".to_string(),
        subtitle: Some(SESSION_DIFF_SUBTITLE.to_string()),
        stat: r.stat.clone(),
        diff: r.diff.clone(),
        stat_only,
        cut_note,
    }
}

/// The document for one checkpoint compared with the files now (C5, C9).
#[must_use]
pub fn checkpoint_diff_doc(short: &str, stat: &str, diff: &str) -> DiffDoc {
    if stat.is_empty() && diff.is_empty() {
        return DiffDoc::Empty(no_checkpoint_differences(short));
    }
    let cut_note = (diff.chars().count() == CHECKPOINT_DIFF_LIMIT)
        .then(|| CHECKPOINT_DIFF_CUT_NOTE.to_string());
    DiffDoc::Text {
        title: format!("Checkpoint {short} compared with the files now"),
        subtitle: None,
        stat: stat.to_string(),
        diff: diff.to_string(),
        stat_only: false,
        cut_note,
    }
}

// Layout texts (section 3.7). They carry no text id of their own.

/// Layout text. Title of the Turns screen.
pub const TURNS_TITLE: &str = "Rewind · choose a prompt to go back to";
/// Layout text. Footer of the Turns screen, before any checkpoint note.
pub const TURNS_FOOTER: &str = "↑↓ choose · enter open · esc close";
/// Layout text. Title of the Checkpoints screen.
pub const CHECKPOINTS_TITLE: &str = "Checkpoints · newest first";
/// Layout text. Footer of the Checkpoints screen.
pub const CHECKPOINTS_FOOTER: &str = "↑↓ choose · enter diff · r restore · esc close";
/// Layout text. Footer of the diff view.
pub const DIFF_FOOTER: &str = "↑↓ scroll · pgup/pgdn page · home/end · esc back";
/// Layout text. Footer while loading.
pub const LOADING_FOOTER: &str = "esc close";
/// Layout text. Title of the session diff.
pub const SESSION_DIFF_TITLE: &str = "Changes since the oldest kept checkpoint";
/// Layout text. Key line of a confirm screen that can proceed.
pub const KEYS_CONFIRM: &str = "enter confirm · esc back";
/// Layout text. Key line of a screen that can only go back.
pub const KEYS_BACK: &str = "esc back";
/// Layout text. Confirm line for a conversation-only rewind.
pub const FILES_STAY: &str = "Files stay as they are.";
/// Layout text. The conversation-only action.
pub const ACTION_REWIND: &str =
    "c  Rewind the conversation to before this prompt (files stay as they are)";
/// Layout text. The diff action.
pub const ACTION_DIFF: &str = "d  Show file changes since this prompt";
/// Layout text. Restore confirm line on the Checkpoints screen.
pub const CHECKPOINT_RESTORE_REMOVES_EXCHANGE: &str =
    "Hermes also removes the last exchange from the conversation.";
/// Text T14, reason part. The reply to the rewind after a restore was lost and a re-read shows
/// the prompt is still there.
pub const LOST_REPLY_AFTER_RESTORE: &str =
    "Talaria lost Hermes's reply, and the rewind did not happen";

/// Layout text. Turns footer with the checkpoint note (C10 or C12) when there is one.
#[must_use]
pub fn turns_footer(note: Option<&str>) -> String {
    match note {
        Some(note) => format!("{TURNS_FOOTER} · {note}"),
        None => TURNS_FOOTER.to_string(),
    }
}

/// Layout text. Title of the Actions screen.
#[must_use]
pub fn actions_title(ordinal: usize, age: &str) -> String {
    if age.is_empty() {
        format!("Prompt #{ordinal}")
    } else {
        format!("Prompt #{ordinal} · {age}")
    }
}

/// Parts joined by `separator`, skipping empty ones.
fn join_parts(parts: &[&str], separator: &str) -> String {
    let kept: Vec<&str> = parts
        .iter()
        .copied()
        .filter(|part| !part.is_empty())
        .collect();
    kept.join(separator)
}

/// Layout text. The files action when a checkpoint matches.
#[must_use]
pub fn action_restore_line(short_hash: &str, clock: &str, message: &str) -> String {
    format!(
        "f  Also restore files to checkpoint {}",
        join_parts(&[short_hash, clock, message], " · ")
    )
}

/// Layout text. The files action when no checkpoint matches, from `no_match_text`.
#[must_use]
pub fn action_restore_unavailable(reason: &str) -> String {
    format!("f  {reason}")
}

/// Layout text. First confirm line.
#[must_use]
pub fn confirm_remove_line(ordinal: usize, later_count: usize) -> String {
    format!(
        "Remove prompt #{ordinal} and everything after it ({}) from the conversation?",
        count_noun(later_count as u64, "message")
    )
}

/// Layout text. Confirm line for a restore with the conversation rewind.
#[must_use]
pub fn confirm_files_line(short_hash: &str, clock: &str, message: &str) -> String {
    format!(
        "Files go back to checkpoint {short_hash} ({}).",
        join_parts(&[clock, message], ", ")
    )
}

/// Layout text. Warning line when the loaded read shows a reply running or prompts queued.
#[must_use]
pub fn confirm_blocked_line(pending: usize) -> String {
    format!("Hermes will not rewind while a reply runs or prompts are queued ({pending} queued).")
}

/// Layout text. Confirm question on the Checkpoints screen.
#[must_use]
pub fn checkpoint_confirm_line(
    index: usize,
    short_hash: &str,
    clock: &str,
    message: &str,
) -> String {
    format!(
        "Restore the files to checkpoint {index} ({})?",
        join_parts(&[short_hash, clock, message], ", ")
    )
}

/// Layout text. Title of a checkpoint diff, also while it loads.
#[must_use]
pub fn checkpoint_diff_title(short_hash: &str) -> String {
    format!("Checkpoint {short_hash} compared with the files now")
}

/// The reason part of text T33 and the Unavailable footer (C12) for a failed call.
#[must_use]
pub fn short_failure_reason(f: &CallFailure) -> String {
    match f {
        CallFailure::Refused { reason, .. } => cut_code_points(reason, REASON_CODE_POINTS),
        CallFailure::Unknown => "Hermes did not answer in time".to_string(),
        CallFailure::Unreadable => "Hermes's reply could not be read".to_string(),
        CallFailure::NotSent(_) => "not connected to Hermes".to_string(),
    }
}
