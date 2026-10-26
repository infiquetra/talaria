//! What a finished launch reports on the terminal and in the fleet list.

use std::path::PathBuf;

use crate::worktree::WorktreeRecord;

/// A launch that Hermes admitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launched {
    /// The live session id.
    pub session_id: String,
    /// Falls back to [`Self::session_id`] when the snapshot had none.
    pub stored_session_id: String,
    /// The profile the session was created in.
    pub profile: String,
    /// The admission id.
    pub admission_id: String,
    /// The admission status word, copied as the gateway sent it.
    pub admission_status: String,
    /// The model the snapshot reported.
    pub model: Option<String>,
    /// The folder the session is running in.
    pub cwd: PathBuf,
    /// Set when this launch created a worktree.
    pub worktree: Option<WorktreeRecord>,
    /// Warnings that do not fail the launch.
    pub warnings: Vec<String>,
    /// The prompt as it was sent, for the fleet view's notice.
    pub prompt: String,
}

/// Why a launch did not start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchError {
    /// The sentence after `talaria:`.
    pub what: String,
    /// The sentence after `Next step:`.
    pub next_step: String,
}

/// The four results a launch can finish with. Usage errors never reach here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchOutcome {
    /// Hermes admitted the prompt.
    Launched(Launched),
    /// Nothing was admitted.
    NotStarted {
        /// Why.
        error: LaunchError,
        /// A session that was created and then closed, when one exists.
        closed_session: Option<String>,
        /// The worktree this launch made was removed.
        tree_removed: bool,
        /// The rollback guard left the tree, and why.
        tree_kept: Option<(PathBuf, String)>,
    },
    /// The submit was sent and the reply was lost or unreadable. The tree is kept.
    Unknown {
        /// The live id, when the create was confirmed.
        session_id: Option<String>,
        /// The stored id, when the create was confirmed.
        stored_session_id: Option<String>,
        /// The profile the launch targeted.
        profile: String,
        /// The tree, when this launch made one.
        worktree: Option<WorktreeRecord>,
    },
    /// Interrupted before the submit was sent.
    Interrupted {
        /// The worktree this launch made was removed.
        tree_removed: bool,
        /// The rollback guard left the tree, and why.
        tree_kept: Option<(PathBuf, String)>,
    },
}

/// The process exit code for a finished launch.
#[must_use]
pub fn exit_code(outcome: &LaunchOutcome) -> u8 {
    match outcome {
        LaunchOutcome::Launched(_) => 0,
        LaunchOutcome::Unknown { .. } => 1,
        LaunchOutcome::NotStarted { .. } => 4,
        LaunchOutcome::Interrupted { .. } => 130,
    }
}

/// `(stdout, stderr)` for a finished launch.
#[must_use]
pub fn render_text(outcome: &LaunchOutcome) -> (String, String) {
    match outcome {
        LaunchOutcome::Launched(launched) => (success_text(launched), warning_text(launched)),
        _ => {
            let (what, next_step) = failure_text(outcome);
            (
                String::new(),
                format!("talaria: {what}\nNext step: {next_step}\n"),
            )
        }
    }
}

/// One JSON line for a finished launch.
#[must_use]
pub fn render_json(outcome: &LaunchOutcome) -> String {
    let status = match outcome {
        LaunchOutcome::Launched(_) => "launched",
        LaunchOutcome::Unknown { .. } => "unknown",
        LaunchOutcome::NotStarted { .. } => "not_started",
        LaunchOutcome::Interrupted { .. } => "interrupted",
    };
    let mut fields = vec![
        ("format", json_text("talaria.bg.v1")),
        ("status", json_text(status)),
    ];
    match outcome {
        LaunchOutcome::Launched(launched) => fields.extend([
            ("session_id", json_text(&launched.session_id)),
            ("stored_session_id", json_text(&launched.stored_session_id)),
            ("profile", json_text(&launched.profile)),
            ("admission_id", json_text(&launched.admission_id)),
            ("admission_status", json_text(&launched.admission_status)),
            ("model", json_text(&launched.model)),
            ("cwd", json_text(&launched.cwd.to_string_lossy())),
            ("worktree", tree_json(launched.worktree.as_ref())),
            ("warnings", json_text(&launched.warnings)),
            ("error", NULL.to_owned()),
        ]),
        _ => {
            let (session_id, stored_session_id, profile, worktree) = match outcome {
                LaunchOutcome::NotStarted { closed_session, .. } => {
                    (closed_session.as_ref(), None, None, None)
                }
                LaunchOutcome::Unknown {
                    session_id,
                    stored_session_id,
                    profile,
                    worktree,
                } => (
                    session_id.as_ref(),
                    stored_session_id.as_ref(),
                    Some(profile),
                    worktree.as_ref(),
                ),
                _ => (None, None, None, None),
            };
            let (what, next_step) = failure_text(outcome);
            fields.extend([
                ("session_id", json_text(&session_id)),
                ("stored_session_id", json_text(&stored_session_id)),
                ("profile", json_text(&profile)),
                ("admission_id", NULL.to_owned()),
                ("admission_status", NULL.to_owned()),
                ("model", NULL.to_owned()),
                ("cwd", NULL.to_owned()),
                ("worktree", tree_json(worktree)),
                ("warnings", "[]".to_owned()),
                (
                    "error",
                    object_text(&[
                        ("what", json_text(&what)),
                        ("next_step", json_text(&next_step)),
                    ]),
                ),
            ]);
        }
    }
    object_text(&fields)
}

/// The fleet-list notice for a finished launch.
///
/// A launch that started reads `Launched in <profile>: "<prompt summary>"`, plus its worktree.
#[must_use]
pub fn notice_text(outcome: &LaunchOutcome) -> String {
    match outcome {
        LaunchOutcome::Launched(launched) => launched_notice(launched, Some(&launched.prompt)),
        LaunchOutcome::NotStarted { error, .. } => {
            format!("Background launch failed: {}", sanitize(&error.what))
        }
        LaunchOutcome::Unknown { session_id, .. } => match session_id {
            Some(id) => format!(
                "Background launch may have started ({}); check the fleet before launching again.",
                sanitize(id)
            ),
            None => "Background launch may have started; check the fleet before launching again."
                .to_owned(),
        },
        LaunchOutcome::Interrupted { .. } => {
            "Background launch stopped before the prompt was sent.".to_owned()
        }
    }
}

/// The success notice: `Launched in <profile>: "<prompt summary>"`, then the worktree branch.
#[must_use]
pub fn launched_notice(launched: &Launched, prompt: Option<&str>) -> String {
    let mut text = format!("Launched in {}", sanitize(&launched.profile));
    if let Some(prompt) = prompt {
        text.push_str(&format!(": \"{}\"", prompt_summary(prompt)));
    }
    if let Some(tree) = &launched.worktree {
        text.push_str(&format!(" (worktree {})", sanitize(&tree.branch)));
    }
    text
}

/// The prompt as a notice shows it: line breaks and tabs become one space, controls are made
/// visible, and what is left is cut to 60 characters with `…` added when cut.
#[must_use]
pub fn prompt_summary(prompt: &str) -> String {
    let mut flat = String::with_capacity(prompt.len());
    let mut in_break = false;
    for character in prompt.chars() {
        if matches!(character, '\r' | '\n' | '\t') {
            if !in_break {
                flat.push(' ');
            }
            in_break = true;
        } else {
            flat.push(character);
            in_break = false;
        }
    }
    let clean = sanitize(&flat);
    let mut summary: String = clean.chars().take(SUMMARY_CHARS).collect();
    if clean.chars().nth(SUMMARY_CHARS).is_some() {
        summary.push('…');
    }
    summary
}

/// Longest prompt summary in a notice, in characters.
const SUMMARY_CHARS: usize = 60;

fn success_text(launched: &Launched) -> String {
    let profile_flag = profile_flag(&launched.profile);
    let id = sanitize(&launched.stored_session_id);
    let mut text = format!(
        "Launched session {id} in profile {}.\nFolder: {}\n",
        sanitize(&launched.profile),
        sanitize(&launched.cwd.to_string_lossy()),
    );
    if let Some(tree) = &launched.worktree {
        let commit: String = tree.base_commit.chars().take(12).collect();
        text.push_str(&format!(
            "Worktree: branch {} from {} at {}\n",
            sanitize(&tree.branch),
            sanitize(&tree.base_ref),
            sanitize(&commit),
        ));
    }
    text.push_str(&format!(
        "Follow it: talaria fleet list{profile_flag}\nOpen it: talaria --resume {id}{profile_flag}\n"
    ));
    text
}

fn warning_text(launched: &Launched) -> String {
    launched
        .warnings
        .iter()
        .map(|warning| format!("{}\n", sanitize(warning)))
        .collect()
}

/// ` --profile <name>`, left out for the default profile.
fn profile_flag(profile: &str) -> String {
    if profile == "default" {
        String::new()
    } else {
        format!(" --profile {}", sanitize(profile))
    }
}

/// The sentence after `talaria:` and the sentence after `Next step:` for a launch that did
/// not finish as launched.
fn failure_text(outcome: &LaunchOutcome) -> (String, String) {
    match outcome {
        LaunchOutcome::NotStarted {
            error, tree_kept, ..
        } => (
            sanitize(&error.what),
            with_kept_tree(sanitize(&error.next_step), tree_kept.as_ref()),
        ),
        LaunchOutcome::Interrupted { tree_kept, .. } => (
            "stopped before the prompt was sent; nothing was started.".to_owned(),
            with_kept_tree(
                "Run the command again when ready.".to_owned(),
                tree_kept.as_ref(),
            ),
        ),
        LaunchOutcome::Unknown {
            session_id,
            profile,
            ..
        } => {
            let mut next_step = format!(
                "Check with talaria fleet list{} before launching again",
                profile_flag(profile)
            );
            match session_id {
                Some(id) => next_step.push_str(&format!("; the session is {}.", sanitize(id))),
                None => next_step.push('.'),
            }
            (
                "the prompt may have started, but the reply from Hermes was lost.".to_owned(),
                next_step,
            )
        }
        LaunchOutcome::Launched(_) => (String::new(), String::new()),
    }
}

fn with_kept_tree(next_step: String, kept: Option<&(PathBuf, String)>) -> String {
    match kept {
        Some((path, reason)) => format!(
            "{next_step} The worktree {} was kept: {}.",
            sanitize(&path.to_string_lossy()),
            sanitize(reason),
        ),
        None => next_step,
    }
}

const NULL: &str = "null";

fn tree_json(tree: Option<&WorktreeRecord>) -> String {
    let Some(tree) = tree else {
        return NULL.to_owned();
    };
    object_text(&[
        ("path", json_text(&tree.path.to_string_lossy())),
        ("branch", json_text(&tree.branch)),
        ("base_ref", json_text(&tree.base_ref)),
        ("base_commit", json_text(&tree.base_commit)),
        ("base_fetched", json_text(&tree.base_fetched)),
    ])
}

/// A value as JSON text. These types always serialise.
fn json_text(value: &(impl serde::Serialize + ?Sized)) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| NULL.to_owned())
}

// The format fixes the key order and serde_json's map is sorted, so objects are joined by hand
// from already-serialised values.
fn object_text(fields: &[(&str, String)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(key, value)| format!("{}:{value}", json_text(key)))
        .collect();
    format!("{{{}}}", body.join(","))
}

// Keep terminal text safe without crossing the application's presentation dependency boundary.
// The visible replacements match the terminal history sanitizer.
fn sanitize(text: &str) -> String {
    let mut sanitized = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = u32::from(ch);
        match code {
            0x09 => sanitized.push_str("    "),
            0x00..=0x1f => {
                if let Some(picture) = char::from_u32(0x2400 + code) {
                    sanitized.push(picture);
                }
            }
            0x7f => sanitized.push('\u{2421}'),
            0x80..=0x9f
            | 0x061c
            | 0x200e
            | 0x200f
            | 0x2028..=0x202e
            | 0x2066..=0x2069
            | 0xe0000..=0xe007f => {
                sanitized.push_str(&format!("‹U+{code:04X}›"));
            }
            _ => sanitized.push(ch),
        }
    }
    sanitized
}
