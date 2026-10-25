//! Test 21 (#237): the recorded permissions run reduces to the expected state.

use std::collections::HashMap;
use std::path::PathBuf;

use hermes_client::ReplaySource;
use serde_json::Value;
use talaria_core::mutation::{MutationLedger, MutationOutcome};
use talaria_core::permissions::{
    ApprovalMode, PermissionsCommand, PermissionsEffect, PermissionsInput, PermissionsState,
};
use talaria_core::workspace::{
    CdCommand, CdContext, CdEffect, CdInput, CdState, FolderCheck, SessionFolder,
};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/permissions-cd.jsonl")
}

fn records() -> Vec<Value> {
    let text = std::fs::read_to_string(fixture_path()).expect("the fixture reads");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect()
}

/// The reducer with its pending generations tracked, so replies carry the
/// generation of the read or write they answer.
struct Driver {
    state: PermissionsState,
    profile_read: Option<u64>,
    bypass_read: Option<u64>,
    write: Option<(u64, bool)>,
}

impl Driver {
    fn new() -> Driver {
        Driver {
            state: PermissionsState::new("fixture".to_owned()),
            profile_read: None,
            bypass_read: None,
            write: None,
        }
    }

    fn feed(&mut self, input: PermissionsInput) -> Vec<PermissionsEffect> {
        let effects = self.state.update(input, Some("Shift+Tab"));
        for effect in &effects {
            match effect {
                PermissionsEffect::ReadProfileMode { generation } => {
                    self.profile_read = Some(*generation);
                }
                PermissionsEffect::ReadSessionOff { generation, .. } => {
                    self.bypass_read = Some(*generation);
                }
                PermissionsEffect::SetSessionOff {
                    generation, off, ..
                } => {
                    self.write = Some((*generation, *off));
                }
                _ => {}
            }
        }
        effects
    }
}

#[test]
fn recorded_session_reduces_to_expected_state() {
    let source =
        ReplaySource::open(&fixture_path()).expect("the fixture opens through the replay source");
    assert!(!source.truncated());
    assert_eq!(source.header().scenario.as_deref(), Some("permissions-cd"));

    // The scenario's outbound methods, in order.
    let out: Vec<(Value, String, Value)> = records()
        .iter()
        .filter(|record| record["kind"] == "frame" && record["dir"] == "out")
        .map(|record| {
            (
                record["frame"]["id"].clone(),
                record["frame"]["method"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                record["frame"]["params"].clone(),
            )
        })
        .collect();
    let methods: Vec<&str> = out.iter().map(|(_, method, _)| method.as_str()).collect();
    assert_eq!(
        methods,
        [
            "client.capabilities",
            "runtime.describe",
            "session.create",
            "config.get",
            "config.get",
            "config.set",
            "config.set",
            "config.get",
            "session.mutate",
            "session.detach",
        ]
    );
    let params_of: HashMap<String, Value> = out
        .iter()
        .map(|(id, _, params)| (id.to_string(), params.clone()))
        .collect();

    let mut permissions = Driver::new();
    let mut cd = CdState::new();
    let mut ledger = MutationLedger::new("testlaunch-cd");
    let mut folder = SessionFolder::default();
    let mut session_id = String::new();
    let mut yolo_reads = 0u32;

    permissions.feed(PermissionsInput::ConnectionOpened);
    assert!(permissions.profile_read.is_some());

    for replayed in source.inbound() {
        let frame = &replayed.frame;
        if frame["method"] == "event" {
            let params = &frame["params"];
            if params["session_id"] != session_id {
                continue;
            }
            match params["type"].as_str().unwrap_or_default() {
                "session.info" => {
                    let cwd = params["payload"]["cwd"].as_str().unwrap_or_default();
                    if !cwd.is_empty() {
                        folder.apply_info_cwd(cwd);
                    }
                }
                "session.updated" => {
                    ledger.fences_mut(&session_id).adopt(
                        params["payload"]["revision"].as_i64(),
                        params["payload"]["execution_generation"].as_i64(),
                    );
                }
                _ => {}
            }
            continue;
        }
        let Some(result) = frame.get("result") else {
            continue;
        };
        if result.get("session_id").is_some() && result.get("info").is_some() {
            // The created session opens.
            session_id = result["session_id"].as_str().unwrap_or_default().to_owned();
            assert!(!session_id.is_empty());
            ledger.fences_mut(&session_id).adopt(
                result["revision"].as_i64(),
                result["execution_generation"].as_i64(),
            );
            folder = SessionFolder::from_snapshot(
                result["info"]["cwd"].as_str(),
                result["info"]["branch"].as_str(),
            );
            permissions.feed(PermissionsInput::SessionOpened {
                session_id: session_id.clone(),
            });
            assert!(permissions.bypass_read.is_some());
            continue;
        }
        let id = frame["id"].to_string();
        let params = params_of.get(&id).cloned().unwrap_or(Value::Null);
        if result.get("operation") == Some(&Value::String("workspace".to_owned())) {
            // The committed move.
            let target = params["payload"]["cwd"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let home = PathBuf::from("/home-fixture");
            let context = CdContext {
                session_id: Some(&session_id),
                turn_running: false,
                folder: &folder,
                home: Some(&home),
                interrupt_label: Some("Esc"),
            };
            let effects = cd.update(
                &context,
                &mut ledger,
                CdInput::Command(CdCommand::Move(target.clone())),
            );
            let pending = effects.iter().find_map(|effect| match effect {
                CdEffect::CheckFolder { target, .. } => Some(target.clone()),
                _ => None,
            });
            assert_eq!(pending.as_deref(), Some(target.as_ref()));
            let effects = cd.update(
                &context,
                &mut ledger,
                CdInput::Checked {
                    session_id: session_id.clone(),
                    target: pending.expect("a check"),
                    check: FolderCheck::Directory,
                },
            );
            let sent = effects.iter().find_map(|effect| match effect {
                CdEffect::Send(request) => Some(request.clone()),
                _ => None,
            });
            let sent = sent.expect("the move is sent");
            assert_eq!(sent.operation, "workspace");
            assert_eq!(sent.expected_revision, 0);
            assert_eq!(sent.expected_generation, Some(0));
            assert_eq!(sent.payload["cwd"], params["payload"]["cwd"]);
            let effects = cd.update(
                &context,
                &mut ledger,
                CdInput::Outcome {
                    request: sent,
                    outcome: MutationOutcome::Committed(result.clone()),
                },
            );
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, CdEffect::UpdateFolder { .. }))
            );
            folder.apply_move(
                result["cwd"].as_str().unwrap_or_default(),
                result["branch"].as_str(),
            );
            continue;
        }
        let key = params["key"].as_str().unwrap_or_default();
        if key == "approvals.mode" && params.get("value").is_none() {
            let mode = match result["value"].as_str().unwrap_or_default() {
                "manual" => ApprovalMode::Manual,
                "smart" => ApprovalMode::Smart,
                "off" => ApprovalMode::Off,
                other => panic!("an unknown recorded mode {other:?}"),
            };
            let first = permissions.profile_read.expect("a profile read is pending");
            permissions.feed(PermissionsInput::ProfileModeRead {
                generation: first,
                result: Ok(mode),
            });
            // Opening the session rereads the profile; the reread confirms the same value.
            if permissions.profile_read != Some(first) {
                let second = permissions.profile_read.expect("the reread");
                permissions.feed(PermissionsInput::ProfileModeRead {
                    generation: second,
                    result: Ok(mode),
                });
            }
            continue;
        }
        if key == "yolo" && params.get("value").is_none() {
            yolo_reads += 1;
            if yolo_reads > 1 {
                // The recorded client reread the bypass; a finished turn triggers it here.
                permissions.feed(PermissionsInput::TurnCompleted {
                    session_id: session_id.clone(),
                });
                let profile = permissions.profile_read.expect("a profile reread");
                permissions.feed(PermissionsInput::ProfileModeRead {
                    generation: profile,
                    result: Ok(ApprovalMode::Manual),
                });
            }
            let off = result["value"] == "1";
            let generation = permissions.bypass_read.expect("a bypass read is pending");
            permissions.feed(PermissionsInput::SessionOffRead {
                session_id: session_id.clone(),
                generation,
                result: Ok(off),
            });
            continue;
        }
        if key == "yolo" && params.get("value").is_some() {
            let off = result["value"] == "1";
            let command = if off {
                PermissionsCommand::SessionOff
            } else {
                PermissionsCommand::SessionDefault
            };
            permissions.feed(PermissionsInput::Command {
                command,
                now: std::time::Instant::now(),
            });
            let (generation, sent) = permissions.write.expect("a bypass write is pending");
            assert_eq!(sent, off);
            permissions.feed(PermissionsInput::SessionOffSet {
                session_id: session_id.clone(),
                generation,
                sent,
                result: Ok(off),
            });
        }
    }

    let segment = permissions.state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: manual");
    assert_eq!(folder.cwd.as_deref(), Some("/work/proj-b"));
    assert_eq!(folder.branch.as_deref(), Some("master"));
}
