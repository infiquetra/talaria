//! Switching the foreground session, branching and writing exports (#231).

use hermes_client::liveness::LinkEvent;
use serde_json::{Map, Value};
use talaria_core::directory::fences::branch_may_retry;
use talaria_core::directory::rows::RowKey;
use talaria_core::mutation::{MutationOutcome, MutationRequest};
use talaria_core::session::{PromptKind, PromptState};

use super::app::{AppState, session_title};
use super::connection::{ConnectionService, OpenConnection};
use super::directory::ExportTarget;
use super::launch::SessionRequest;
use super::mutation::MutationClient;
use super::sessions::{OpenError, OpenedSession, SessionService};

/// The line a branch gets while the agent is working.
pub const BRANCH_BUSY_TEXT: &str = "Hermes cannot branch while the agent is working. Wait for the reply to finish, or press Esc to stop it, then run /branch again.";
/// The line a branch gets while the conversation is being compacted.
pub const BRANCH_COMPRESSING_TEXT: &str = "Hermes is already compacting this conversation. Wait for it to finish, then run /branch again.";
/// The line a branch gets when the last turn's outcome is unknown.
pub const BRANCH_UNKNOWN_TEXT: &str =
    "Hermes cannot branch while the outcome of the last turn is unknown. Nothing was changed.";
/// The line a branch gets when the session changed under it.
pub const BRANCH_CHANGED_TEXT: &str =
    "The session changed while the branch was being prepared. Run /branch again.";
/// The line a branch gets when Hermes refused for another reason.
pub const BRANCH_FAILED_TEXT: &str = "Hermes could not branch the session.";
/// The line a switch gets when prompts are still held.
pub const HELD_PROMPTS_TEXT: &str =
    "Finish or discard the prompt you typed first: switches do not carry unsent prompts.";
/// The line a branch gets when the session's fences are not known yet.
pub const FENCES_UNKNOWN_TEXT: &str =
    "Talaria has not learned this session's revision yet; try again in a moment.";

/// How a switch ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchOutcome {
    Switched { notice: String },
    AlreadyHere,
    Failed { text: String },
}

/// How a branch ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchOutcome {
    Branched { notice: String },
    Refused { text: String },
}

/// Open the target first; only after that lets go of the old session.
/// The one way to change the foreground session.
pub async fn switch_foreground(app: &mut AppState, target: RowKey) -> SwitchOutcome {
    // The foreground session already is the target: nothing is sent.
    if app
        .opened
        .as_ref()
        .is_some_and(|opened| opened.session_id == target.session_id)
    {
        return SwitchOutcome::AlreadyHere;
    }
    // A switch does not carry the prompts that were typed but not sent.
    if !app.held.is_empty() {
        return SwitchOutcome::Failed {
            text: HELD_PROMPTS_TEXT.to_owned(),
        };
    }
    let Some(old) = app.opened.clone() else {
        return SwitchOutcome::Failed {
            text: "There is no session to leave.".to_owned(),
        };
    };
    let Some(old_service) = app.service.clone() else {
        return SwitchOutcome::Failed {
            text: "Talaria is not connected to Hermes.".to_owned(),
        };
    };
    if target.profile == app.profile {
        // The same profile: the target opens on the link that is already open.
        let opened = match old_service
            .open(SessionRequest::ById {
                session_id: target.session_id.clone(),
            })
            .await
        {
            Ok(opened) => opened,
            Err(error) => return failed_switch(app, &target, &error),
        };
        // The old session lets go only after the new one is open.
        stop_voice(app, &old_service, &old.session_id).await;
        old_service.detach(&old).await;
        let notice = switch_notice(app, &opened, &old);
        app.opened = Some(opened);
        return SwitchOutcome::Switched { notice };
    }
    // Another profile: a new profile-scoped link carries the target, and the old link
    // is let go once the target is open.
    let Some(launch) = app.launch.clone() else {
        return SwitchOutcome::Failed {
            text: "Talaria does not know which profile to open.".to_owned(),
        };
    };
    let mut other = launch;
    other.profile = target.profile.clone();
    let connection = match ConnectionService::open(&other, &app.hermes_home, None).await {
        Ok(connection) => connection,
        Err(_) => {
            return SwitchOutcome::Failed {
                text: format!(
                    "Could not open \"{}\": Talaria could not reach Hermes for the profile {}. Still in the old session.",
                    target.session_id, target.profile
                ),
            };
        }
    };
    let OpenConnection {
        link,
        inbound,
        describe,
        profile_name,
        ..
    } = connection;
    let service = SessionService::with_client(&app.launch_id, link.client())
        .with_create_defaults(&profile_name, &describe.session_create.parameters);
    let opened = match service
        .open(SessionRequest::ById {
            session_id: target.session_id.clone(),
        })
        .await
    {
        Ok(opened) => opened,
        Err(error) => {
            link.close().await;
            return failed_switch(app, &target, &error);
        }
    };
    // The old session lets go on the old link; the old link closes after that.
    stop_voice(app, &old_service, &old.session_id).await;
    old_service.detach(&old).await;
    let notice = switch_notice(app, &opened, &old);
    app.opened = Some(opened);
    app.service = Some(service);
    app.link = Some(link);
    app.profile = profile_name;
    forward_events(app, inbound);
    SwitchOutcome::Switched { notice }
}

/// A branch the gateway refused: the old session stays in front.
fn failed_switch(app: &mut AppState, target: &RowKey, error: &OpenError) -> SwitchOutcome {
    let reason = refusal_reason(error);
    app.switch_reason = Some(reason.clone());
    SwitchOutcome::Failed {
        text: format!(
            "Could not open \"{}\": Hermes refused the resume ({reason}). Still in the old session.",
            target.session_id
        ),
    }
}

/// Start a new session on another served profile in `cwd`: a new profile-scoped link
/// carries it, and the old session is let go only once the new one is open. The old
/// session keeps running in Hermes, so `/resume` returns to it.
pub async fn start_in_profile(
    app: &mut AppState,
    profile: String,
    cwd: std::path::PathBuf,
) -> SwitchOutcome {
    let failed = |reason: &str| SwitchOutcome::Failed {
        text: format!(
            "Could not start a session on profile {profile}: {reason}. Still in the old session."
        ),
    };
    if !app.held.is_empty() {
        return SwitchOutcome::Failed {
            text: HELD_PROMPTS_TEXT.to_owned(),
        };
    }
    let (Some(old), Some(old_service), Some(launch)) =
        (app.opened.clone(), app.service.clone(), app.launch.clone())
    else {
        return failed("Talaria is not connected to Hermes");
    };
    let mut other = launch;
    other.profile = profile.clone();
    let Ok(connection) = ConnectionService::open(&other, &app.hermes_home, None).await else {
        return failed("Talaria could not reach Hermes for that profile");
    };
    let OpenConnection {
        link,
        inbound,
        describe,
        profile_name,
        ..
    } = connection;
    let service = SessionService::with_client(&app.launch_id, link.client())
        .with_create_defaults(&profile_name, &describe.session_create.parameters);
    let request = super::launch::NewSession {
        request_id: super::launch::new_request_id(),
        source: super::launch::SESSION_SOURCE.into(),
        cwd,
        model: None,
        provider: None,
        reasoning: None,
    };
    let opened = match service.open(SessionRequest::New(request)).await {
        Ok(opened) => opened,
        Err(error) => {
            link.close().await;
            return failed(&format!("Hermes refused ({})", refusal_reason(&error)));
        }
    };
    // The old session lets go on the old link; the old link closes after that.
    stop_voice(app, &old_service, &old.session_id).await;
    old_service.detach(&old).await;
    let old_title = session_title(
        old.title.as_deref(),
        app.list_title.as_deref(),
        old.state.cells(),
    );
    let notice = format!(
        "Connected to profile {profile_name} and started a new session in this folder. Left \"{old_title}\"; it keeps running in Hermes. /resume returns to it. To start on this profile next time: talaria --profile {profile_name}"
    );
    app.opened = Some(opened);
    app.service = Some(service);
    app.link = Some(link);
    app.profile = profile_name;
    forward_events(app, inbound);
    SwitchOutcome::Switched { notice }
}

/// The gateway's reason for refusing the resume.
pub(crate) fn refusal_reason(error: &OpenError) -> String {
    match error {
        OpenError::NotFound => "not_found".to_owned(),
        OpenError::Refused { reason } => reason.clone(),
        OpenError::MismatchedSession { .. } => "session_mismatch".to_owned(),
        OpenError::NotInFolder { .. } => "not_in_folder".to_owned(),
        OpenError::FolderUnknown => "folder_unknown".to_owned(),
        OpenError::Unknown => "the connection dropped before Hermes answered".to_owned(),
        OpenError::Link(what) => what.clone(),
    }
}

/// The notice a switch commits: the new session is in front, the old one keeps running.
fn switch_notice(app: &AppState, new: &OpenedSession, old: &OpenedSession) -> String {
    let new_title = session_title(
        new.title.as_deref(),
        app.list_title.as_deref(),
        new.state.cells(),
    );
    let old_title = session_title(
        old.title.as_deref(),
        app.list_title.as_deref(),
        old.state.cells(),
    );
    let mut notice = format!(
        "Now showing \"{new_title}\". Left \"{old_title}\"; it keeps running in Hermes. /resume returns to it."
    );
    if old
        .state
        .open_prompts()
        .any(|prompt| prompt.state == PromptState::Open && prompt.kind != PromptKind::ServerRequest)
    {
        notice.push_str(" It is waiting for your answer.");
    }
    notice
}

/// Send the events of a new link to the loop.
fn forward_events(app: &AppState, mut inbound: tokio::sync::mpsc::Receiver<LinkEvent>) {
    let Some(sender) = app.sender.clone() else {
        return;
    };
    tokio::spawn(async move {
        while let Some(event) = inbound.recv().await {
            if sender.send(super::app::AppMessage::Link(event)).is_err() {
                break;
            }
        }
    });
}

/// Branch the foreground session on the foreground link, then switch into
/// the child.
pub async fn branch_foreground(app: &mut AppState, title: Option<String>) -> BranchOutcome {
    let name = title.clone();
    let request = match branch_request(app, title) {
        Ok(request) => request,
        Err(refused) => return refused,
    };
    let Some(client) = app
        .service
        .as_ref()
        .and_then(|service| service.client().cloned())
    else {
        return BranchOutcome::Refused {
            text: "Talaria is not connected to Hermes.".to_owned(),
        };
    };
    let mutation = MutationClient::new(client);
    let mut outcome = mutation.send(&request).await;
    app.ledger.settle(&request, &outcome);
    // A fence refusal is retried once, and only when the turn did not change.
    if let MutationOutcome::FenceRefused { .. } = &outcome {
        let sent = request.expected_generation.unwrap_or_default();
        if branch_may_retry(sent, &outcome)
            && let Some(retry) = app.ledger.retry_after_fence(&request, &outcome)
        {
            outcome = mutation.send(&retry).await;
            app.ledger.settle(&retry, &outcome);
        }
    }
    match outcome {
        MutationOutcome::Committed(receipt) => {
            open_branch_child(app, &receipt, name.as_deref()).await
        }
        outcome => branch_refusal(&outcome),
    }
}

/// The branch request for the foreground session, built with the session's ledger so its
/// request id is never used twice. `Err` is the refusal when nothing may be sent.
pub fn branch_request(
    app: &mut AppState,
    title: Option<String>,
) -> Result<MutationRequest, BranchOutcome> {
    // A turn that is running cannot be branched.
    if app
        .opened
        .as_ref()
        .is_some_and(|opened| opened.state.turn() == &talaria_core::session::TurnState::Running)
    {
        return Err(BranchOutcome::Refused {
            text: BRANCH_BUSY_TEXT.to_owned(),
        });
    }
    let Some(opened) = app.opened.as_ref() else {
        return Err(BranchOutcome::Refused {
            text: "There is no session to branch.".to_owned(),
        });
    };
    let session_id = opened.session_id.clone();
    let mut payload = Map::new();
    if let Some(title) = title {
        payload.insert("title".to_owned(), Value::String(title));
    }
    app.ledger
        .build(&session_id, "branch", payload, true)
        .map_err(|_| BranchOutcome::Refused {
            text: FENCES_UNKNOWN_TEXT.to_owned(),
        })
}

/// What a branch Hermes did not commit tells the operator.
pub fn branch_refusal(outcome: &MutationOutcome) -> BranchOutcome {
    BranchOutcome::Refused {
        text: match outcome {
            MutationOutcome::FenceRefused { .. } => BRANCH_CHANGED_TEXT.to_owned(),
            MutationOutcome::Refused { reason, field, .. } => {
                match (reason.as_str(), field.as_deref()) {
                    ("session_busy", Some("compression")) => BRANCH_COMPRESSING_TEXT.to_owned(),
                    ("session_busy", _) => BRANCH_BUSY_TEXT.to_owned(),
                    ("unknown_execution", _) => BRANCH_UNKNOWN_TEXT.to_owned(),
                    _ => BRANCH_FAILED_TEXT.to_owned(),
                }
            }
            _ => BRANCH_FAILED_TEXT.to_owned(),
        },
    }
}

/// The notice a branch commits after the child's replayed history: where the operator is,
/// where the branch came from, how to go back, and, when text followed `/branch`, that it
/// named the branch and was not sent.
fn branch_notice(parent: &str, child: &str, copied: u64, name: Option<&str>) -> String {
    let mut notice = format!(
        "Branched into \"{child}\" with {copied} earlier messages. You are now in the new session.\n\"{parent}\" is unchanged; /resume returns to it."
    );
    if name.is_some_and(|name| !name.trim().is_empty()) {
        notice.push_str(
            "\nThe text after /branch is the new session's name, not a prompt; nothing was sent.",
        );
    }
    notice
}

/// Open the child a committed branch made, then let go of the parent. When the child does
/// not open, the parent stays in front and nothing is detached.
pub async fn open_branch_child(
    app: &mut AppState,
    receipt: &Value,
    name: Option<&str>,
) -> BranchOutcome {
    let child_id = receipt
        .get("branched_session_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let copied = receipt
        .get("copied_messages")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let (Some(parent), Some(service)) = (app.opened.clone(), app.service.clone()) else {
        return BranchOutcome::Refused {
            text: "Talaria is not connected to Hermes.".to_owned(),
        };
    };
    match service
        .open(SessionRequest::ById {
            session_id: child_id.clone(),
        })
        .await
    {
        Ok(child) => {
            // The parent lets go only after the child is open.
            stop_voice(app, &service, &parent.session_id).await;
            service.detach(&parent).await;
            let title = session_title(child.title.as_deref(), None, child.state.cells());
            let parent_title = session_title(parent.title.as_deref(), None, parent.state.cells());
            app.opened = Some(child);
            BranchOutcome::Branched {
                notice: branch_notice(&parent_title, &title, copied, name),
            }
        }
        Err(_) => BranchOutcome::Refused {
            text: format!(
                "Branch made \"{child_id}\" but Hermes would not open it. Resume it from the picker when Hermes is reachable again."
            ),
        },
    }
}

/// Write export content to its target without ever overwriting. Returns the
/// path written and its size.
pub fn write_export_file(
    target: &ExportTarget,
    name: &str,
    content: &str,
) -> Result<(std::path::PathBuf, u64), String> {
    match target {
        ExportTarget::Auto { folder } => {
            for candidate in talaria_core::directory::export::candidate_names(name) {
                let path = folder.join(candidate);
                if !path.exists() {
                    return write_new(&path, content);
                }
            }
            Err(format!("There is no free name next to {name} (tried 99)."))
        }
        ExportTarget::Explicit { path } => write_new(path, content),
    }
}

/// Write a file that does not exist yet, owner-only, and remove it when the
/// write fails.
fn write_new(path: &std::path::Path, content: &str) -> Result<(std::path::PathBuf, u64), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("the export file could not be created: {error}"))?;
    if let Err(error) = file.write_all(content.as_bytes()) {
        let _ = std::fs::remove_file(path);
        return Err(format!("the export could not be written: {error}"));
    }
    Ok((path.to_path_buf(), content.len() as u64))
}

pub(crate) async fn stop_voice(app: &mut AppState, service: &SessionService, session_id: &str) {
    app.voice_detaching(session_id);
    app.close_client_voice().await;
    if app.uses_client_voice() {
        return;
    }
    if let Some(client) = service.client() {
        let mut warning = Vec::new();
        app.voice
            .stop_before_detach(client, session_id, &mut warning)
            .await;
        if !warning.is_empty() {
            app.voice_warning(session_id, String::from_utf8_lossy(&warning).into_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::branch_notice;

    #[test]
    fn the_branch_notice_is_two_lines_without_a_name_and_three_with_one() {
        let plain = branch_notice("Launch title", "Launch title #2", 9, None);
        assert_eq!(plain.lines().count(), 2, "{plain}");
        assert!(
            plain.contains("\"Launch title\" is unchanged; /resume returns to it."),
            "the notice names the session it came from: {plain}"
        );
        assert!(
            plain.starts_with("Branched into \"Launch title #2\""),
            "{plain}"
        );

        let blank = branch_notice("Launch title", "Launch title #2", 9, Some("  "));
        assert_eq!(
            blank.lines().count(),
            2,
            "a blank name adds no note: {blank}"
        );

        let named = branch_notice("Launch title", "poem", 9, Some("poem"));
        assert_eq!(named.lines().count(), 3, "{named}");
        assert!(named.contains("not a prompt"), "{named}");
        assert!(
            named.contains("\"Launch title\" is unchanged"),
            "the original, not the branch, is the unchanged one: {named}"
        );
    }
}
