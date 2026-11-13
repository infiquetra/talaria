//! Tests 11 and 12 (#237): the profile confirmation and the profile texts.

use std::time::{Duration, Instant};

use talaria_core::permissions::{
    ApprovalMode, CallFailure, PermissionsCommand, PermissionsEffect, PermissionsInput,
    PermissionsState,
};
use talaria_core::session::NoticeKind;

const PROFILE: &str = "fixture";
const S1: &str = "s-1";
const S2: &str = "s-2";
const KEY: &str = "Shift+Tab";

const P4: &str = "This turns approvals off for every session of profile fixture, background \
                  sessions included. Run /permissions off again within 30 seconds to confirm.";

fn fresh() -> PermissionsState {
    PermissionsState::new(PROFILE.to_owned())
}

fn update(state: &mut PermissionsState, input: PermissionsInput) -> Vec<PermissionsEffect> {
    state.update(input, Some(KEY))
}

fn update_unbound(state: &mut PermissionsState, input: PermissionsInput) -> Vec<PermissionsEffect> {
    state.update(input, None)
}

fn notices(effects: &[PermissionsEffect]) -> Vec<(NoticeKind, String)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            PermissionsEffect::Notice { kind, text } => Some((*kind, text.clone())),
            _ => None,
        })
        .collect()
}

fn profile_sets(effects: &[PermissionsEffect]) -> Vec<(u64, ApprovalMode)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            PermissionsEffect::SetProfileMode { generation, mode } => Some((*generation, *mode)),
            _ => None,
        })
        .collect()
}

fn reads(effects: &[PermissionsEffect]) -> (Vec<u64>, Vec<(String, u64)>) {
    let mut profile = Vec::new();
    let mut session = Vec::new();
    for effect in effects {
        match effect {
            PermissionsEffect::ReadProfileMode { generation } => profile.push(*generation),
            PermissionsEffect::ReadSessionOff {
                session_id,
                generation,
            } => session.push((session_id.clone(), *generation)),
            _ => {}
        }
    }
    (profile, session)
}

fn refused(reason: &str, rpc_code: i64, message: &str) -> CallFailure {
    CallFailure::Refused {
        reason: reason.to_owned(),
        rpc_code,
        message: message.to_owned(),
    }
}

fn command(
    state: &mut PermissionsState,
    command: PermissionsCommand,
    now: Instant,
) -> Vec<PermissionsEffect> {
    update(state, PermissionsInput::Command { command, now })
}

fn open_known(state: &mut PermissionsState, session: &str, mode: ApprovalMode, off: bool) {
    update(state, PermissionsInput::ConnectionOpened);
    let effects = update(
        state,
        PermissionsInput::SessionOpened {
            session_id: session.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    update(
        state,
        PermissionsInput::ProfileModeRead {
            generation: profile[0],
            result: Ok(mode),
        },
    );
    update(
        state,
        PermissionsInput::SessionOffRead {
            session_id: session.to_owned(),
            generation: bypass[0].1,
            result: Ok(off),
        },
    );
}

fn answer_profile_mode(
    state: &mut PermissionsState,
    generation: u64,
    sent: ApprovalMode,
    result: Result<ApprovalMode, CallFailure>,
) -> Vec<PermissionsEffect> {
    update(
        state,
        PermissionsInput::ProfileModeSet {
            generation,
            sent,
            result,
        },
    )
}

#[test]
fn profile_off_needs_a_second_command() {
    // First `off`: P4. Second within 29 seconds: sent; its success draws P5.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let start = Instant::now();
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start,
    );
    assert!(profile_sets(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(NoticeKind::Warning, P4.to_owned())]
    );
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start + Duration::from_secs(29),
    );
    let sent = profile_sets(&effects);
    assert_eq!(sent, vec![(sent[0].0, ApprovalMode::Off)]);
    assert!(notices(&effects).is_empty());
    let effects = answer_profile_mode(
        &mut state,
        sent[0].0,
        ApprovalMode::Off,
        Ok(ApprovalMode::Off),
    );
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Approvals for every session of profile fixture: off.".to_owned()
        )]
    );

    // A second `off` 31 seconds later is a new first: P4 again.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let start = Instant::now();
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start,
    );
    assert_eq!(
        notices(&effects),
        vec![(NoticeKind::Warning, P4.to_owned())]
    );
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start + Duration::from_secs(31),
    );
    assert!(profile_sets(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(NoticeKind::Warning, P4.to_owned())]
    );

    // `/permissions`, `/permissions smart`, the toggle key, a new session and a new
    // connection close the window: the second `off` gives P4 again.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let start = Instant::now();
    command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start,
    );
    command(
        &mut state,
        PermissionsCommand::Show,
        start + Duration::from_secs(1),
    );
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start + Duration::from_secs(2),
    );
    assert!(profile_sets(&effects).is_empty());
    assert_eq!(notices(&effects).len(), 1);

    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let start = Instant::now();
    command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start,
    );
    command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Smart),
        start + Duration::from_secs(1),
    );
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start + Duration::from_secs(2),
    );
    assert!(profile_sets(&effects).is_empty());
    assert_eq!(notices(&effects).len(), 1);

    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let start = Instant::now();
    command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start,
    );
    update(&mut state, PermissionsInput::ToggleKey);
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start + Duration::from_secs(2),
    );
    assert!(profile_sets(&effects).is_empty());
    assert_eq!(notices(&effects).len(), 1);

    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let start = Instant::now();
    command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start,
    );
    update(
        &mut state,
        PermissionsInput::SessionOpened {
            session_id: S2.to_owned(),
        },
    );
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start + Duration::from_secs(2),
    );
    assert!(profile_sets(&effects).is_empty());
    assert_eq!(notices(&effects).len(), 1);

    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let start = Instant::now();
    command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start,
    );
    update(&mut state, PermissionsInput::ConnectionOpened);
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start + Duration::from_secs(2),
    );
    assert!(profile_sets(&effects).is_empty());
    assert_eq!(notices(&effects).len(), 1);

    // Read replies and a finished turn leave the window open.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let start = Instant::now();
    command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start,
    );
    let effects = update(
        &mut state,
        PermissionsInput::TurnCompleted {
            session_id: S1.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    update(
        &mut state,
        PermissionsInput::ProfileModeRead {
            generation: profile[0],
            result: Ok(ApprovalMode::Smart),
        },
    );
    update(
        &mut state,
        PermissionsInput::SessionOffRead {
            session_id: S1.to_owned(),
            generation: bypass[0].1,
            result: Ok(false),
        },
    );
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Off),
        start + Duration::from_secs(29),
    );
    assert_eq!(profile_sets(&effects).len(), 1);
}

#[test]
fn show_profile_text_and_profile_write_outcomes() {
    // P3 with no session.
    let mut state = fresh();
    let effects = command(&mut state, PermissionsCommand::Show, Instant::now());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Approvals in this session: no session is open.\n\
             Profile fixture: not read yet. This applies to every session of the profile.\n\
             No session is open. /permissions manual, smart or off changes the profile."
                .to_owned()
        )]
    );

    // P3 with the profile manual and each bypass reading.
    for (off, first, third) in [
        (
            false,
            "Approvals in this session: manual.",
            "This session: follows the profile. Shift+Tab switches this session; /permissions \
             manual, smart or off changes the profile.",
        ),
        (
            true,
            "Approvals in this session: off (this session).",
            "This session: has approvals off for itself only. Shift+Tab switches this session; \
             /permissions manual, smart or off changes the profile.",
        ),
    ] {
        let mut state = fresh();
        update(&mut state, PermissionsInput::ConnectionOpened);
        let effects = update(
            &mut state,
            PermissionsInput::SessionOpened {
                session_id: S1.to_owned(),
            },
        );
        let (profile, bypass) = reads(&effects);
        update(
            &mut state,
            PermissionsInput::ProfileModeRead {
                generation: profile[0],
                result: Ok(ApprovalMode::Manual),
            },
        );
        update(
            &mut state,
            PermissionsInput::SessionOffRead {
                session_id: S1.to_owned(),
                generation: bypass[0].1,
                result: Ok(off),
            },
        );
        let effects = command(&mut state, PermissionsCommand::Show, Instant::now());
        assert_eq!(
            notices(&effects),
            vec![(
                NoticeKind::System,
                format!(
                    "{first}\nProfile fixture: manual. This applies to every session of the \
                     profile.\n{third}"
                )
            )],
            "bypass {off}"
        );
    }

    // P3 with the bypass still pending.
    let mut state = fresh();
    update(&mut state, PermissionsInput::ConnectionOpened);
    let effects = update(
        &mut state,
        PermissionsInput::SessionOpened {
            session_id: S1.to_owned(),
        },
    );
    let (profile, _) = reads(&effects);
    update(
        &mut state,
        PermissionsInput::ProfileModeRead {
            generation: profile[0],
            result: Ok(ApprovalMode::Manual),
        },
    );
    let effects = command(&mut state, PermissionsCommand::Show, Instant::now());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Approvals in this session: not read yet.\n\
             Profile fixture: manual. This applies to every session of the profile.\n\
             This session: not read yet. Shift+Tab switches this session; /permissions manual, \
             smart or off changes the profile."
                .to_owned()
        )]
    );

    // P3 with the bypass unknown after a lost write.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Manual, false);
    let effects = update(&mut state, PermissionsInput::ToggleKey);
    let generation = effects
        .iter()
        .find_map(|effect| match effect {
            PermissionsEffect::SetSessionOff { generation, .. } => Some(*generation),
            _ => None,
        })
        .expect("a bypass write");
    update(
        &mut state,
        PermissionsInput::SessionOffSet {
            session_id: S1.to_owned(),
            generation,
            sent: true,
            result: Err(CallFailure::Unknown),
        },
    );
    let effects = command(&mut state, PermissionsCommand::Show, Instant::now());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Approvals in this session: unknown.\n\
             Profile fixture: manual. This applies to every session of the profile.\n\
             This session: unknown. Shift+Tab switches this session; /permissions manual, smart \
             or off changes the profile."
                .to_owned()
        )]
    );

    // P3 with the profile off and with the profile failed: no key is named.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Off, false);
    let effects = command(&mut state, PermissionsCommand::Show, Instant::now());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Approvals in this session: off.\n\
             Profile fixture: off. This applies to every session of the profile.\n\
             This session: follows the profile. /permissions session off or /permissions session \
             default sets this session's own setting; /permissions manual, smart or off changes \
             the profile."
                .to_owned()
        )]
    );
    let mut state = fresh();
    update(&mut state, PermissionsInput::ConnectionOpened);
    let effects = update(
        &mut state,
        PermissionsInput::SessionOpened {
            session_id: S1.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    update(
        &mut state,
        PermissionsInput::ProfileModeRead {
            generation: profile[0],
            result: Err(refused("permission_denied", 4001, "permission_denied")),
        },
    );
    update(
        &mut state,
        PermissionsInput::SessionOffRead {
            session_id: S1.to_owned(),
            generation: bypass[0].1,
            result: Ok(false),
        },
    );
    let effects = command(&mut state, PermissionsCommand::Show, Instant::now());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Approvals in this session: unknown.\n\
             Profile fixture: unknown. This applies to every session of the profile.\n\
             This session: follows the profile. /permissions session off or /permissions session \
             default sets this session's own setting; /permissions manual, smart or off changes \
             the profile."
                .to_owned()
        )]
    );

    // P3 with no key bound.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Manual, false);
    let effects = update_unbound(
        &mut state,
        PermissionsInput::Command {
            command: PermissionsCommand::Show,
            now: Instant::now(),
        },
    );
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Approvals in this session: manual.\n\
             Profile fixture: manual. This applies to every session of the profile.\n\
             This session: follows the profile. /permissions session off or /permissions session \
             default switches this session; /permissions manual, smart or off changes the profile."
                .to_owned()
        )]
    );

    // P5 with each suffix.
    for (mode, off, bound, text) in [
        (
            ApprovalMode::Smart,
            true,
            true,
            "Approvals for every session of profile fixture: smart. This session still has \
             approvals off for itself; Shift+Tab turns them back on.",
        ),
        (
            ApprovalMode::Smart,
            true,
            false,
            "Approvals for every session of profile fixture: smart. This session still has \
             approvals off for itself; /permissions session default turns them back on.",
        ),
        (
            ApprovalMode::Off,
            true,
            true,
            "Approvals for every session of profile fixture: off. This session also has \
             approvals off for itself.",
        ),
        (
            ApprovalMode::Smart,
            false,
            true,
            "Approvals for every session of profile fixture: smart.",
        ),
    ] {
        let mut state = fresh();
        open_known(&mut state, S1, ApprovalMode::Manual, off);
        let effects = command(
            &mut state,
            PermissionsCommand::Profile(mode),
            Instant::now(),
        );
        // The `off` write needs its confirmation first.
        let sent = if mode == ApprovalMode::Off {
            assert!(profile_sets(&effects).is_empty());
            let effects = command(
                &mut state,
                PermissionsCommand::Profile(mode),
                Instant::now(),
            );
            profile_sets(&effects)
        } else {
            profile_sets(&effects)
        };
        assert_eq!(sent.len(), 1);
        let reply = PermissionsInput::ProfileModeSet {
            generation: sent[0].0,
            sent: mode,
            result: Ok(mode),
        };
        let effects = if bound {
            update(&mut state, reply)
        } else {
            update_unbound(&mut state, reply)
        };
        assert_eq!(
            notices(&effects),
            vec![(NoticeKind::System, text.to_owned())],
            "mode {mode:?} off {off} bound {bound}"
        );
    }

    // P14: a second profile command while a write is in flight.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Manual, false);
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Smart),
        Instant::now(),
    );
    assert_eq!(profile_sets(&effects).len(), 1);
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Manual),
        Instant::now(),
    );
    assert!(profile_sets(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "A change to the profile's approval mode is still being applied. Try again in a \
             moment."
                .to_owned()
        )]
    );

    // P10 for each write row of the words table.
    for (failure, words) in [
        (
            refused("permission_denied", 4001, "permission_denied"),
            "this connection may not change settings",
        ),
        (
            refused("invalid_params", 4001, "invalid_params"),
            "Hermes did not accept the request",
        ),
        (
            refused("profile_mismatch", 4001, "profile_mismatch"),
            "the request named a different profile",
        ),
        (
            refused("storage_unavailable", 4001, "storage_unavailable"),
            "Hermes could not save the profile's settings (an administrator may manage them, or \
             the file may be unreadable)",
        ),
        (
            refused("storage_unavailable", 5001, "storage_unavailable"),
            "Hermes could not save the profile's settings (an administrator may manage them, or \
             the file may be unreadable)",
        ),
        (
            refused("not_found", 4001, "not_found"),
            "the session no longer exists",
        ),
        (
            refused("unknown_method", -32601, "unknown method: config.set"),
            "this Hermes gateway does not offer config.set",
        ),
        (refused("", -32602, "boom"), "error -32602: boom"),
    ] {
        let mut state = fresh();
        open_known(&mut state, S1, ApprovalMode::Manual, false);
        let effects = command(
            &mut state,
            PermissionsCommand::Profile(ApprovalMode::Smart),
            Instant::now(),
        );
        let sent = profile_sets(&effects);
        assert_eq!(sent.len(), 1);
        let effects = answer_profile_mode(&mut state, sent[0].0, ApprovalMode::Smart, Err(failure));
        assert_eq!(
            notices(&effects),
            vec![(
                NoticeKind::Error,
                format!("Hermes refused the change: {words}.")
            )],
            "words {words}"
        );
    }

    // A profile write with no session sends, and names the profile on success.
    let mut state = fresh();
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Smart),
        Instant::now(),
    );
    let sent = profile_sets(&effects);
    assert_eq!(sent.len(), 1);
    let effects = answer_profile_mode(
        &mut state,
        sent[0].0,
        ApprovalMode::Smart,
        Ok(ApprovalMode::Smart),
    );
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Approvals for every session of profile fixture: smart.".to_owned()
        )]
    );

    // A profile write for the mode already known is still sent.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let effects = command(
        &mut state,
        PermissionsCommand::Profile(ApprovalMode::Smart),
        Instant::now(),
    );
    assert_eq!(profile_sets(&effects).len(), 1);
}
