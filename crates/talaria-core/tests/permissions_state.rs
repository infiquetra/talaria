//! Tests 3 to 9 (#237): the permissions reducer reads, writes and notices.

use std::time::Instant;

use talaria_core::permissions::{
    ApprovalMode, ApprovalTone, CallFailure, HermesCommandResult, PermissionsCommand,
    PermissionsEffect, PermissionsInput, PermissionsState, Reading,
};
use talaria_core::session::NoticeKind;

const PROFILE: &str = "fixture";
const S1: &str = "s-1";
const S2: &str = "s-2";
const KEY: &str = "Shift+Tab";

fn fresh() -> PermissionsState {
    PermissionsState::new(PROFILE.to_owned())
}

fn update(state: &mut PermissionsState, input: PermissionsInput) -> Vec<PermissionsEffect> {
    state.update(input, Some(KEY))
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

fn sets(effects: &[PermissionsEffect]) -> Vec<(String, u64, bool)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            PermissionsEffect::SetSessionOff {
                session_id,
                generation,
                off,
            } => Some((session_id.clone(), *generation, *off)),
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

fn answer_profile(
    state: &mut PermissionsState,
    generation: u64,
    result: Result<ApprovalMode, CallFailure>,
) -> Vec<PermissionsEffect> {
    update(
        state,
        PermissionsInput::ProfileModeRead { generation, result },
    )
}

fn answer_bypass(
    state: &mut PermissionsState,
    session: &str,
    generation: u64,
    result: Result<bool, CallFailure>,
) -> Vec<PermissionsEffect> {
    update(
        state,
        PermissionsInput::SessionOffRead {
            session_id: session.to_owned(),
            generation,
            result,
        },
    )
}

/// Open a session and answer both first reads; returns the read generations.
fn open_known(
    state: &mut PermissionsState,
    session: &str,
    mode: ApprovalMode,
    off: bool,
) -> (u64, u64) {
    update(state, PermissionsInput::ConnectionOpened);
    let effects = update(
        state,
        PermissionsInput::SessionOpened {
            session_id: session.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    assert_eq!(profile.len(), 1);
    assert_eq!(bypass.len(), 1);
    answer_profile(state, profile[0], Ok(mode));
    answer_bypass(state, session, bypass[0].1, Ok(off));
    (profile[0], bypass[0].1)
}

#[test]
fn reads_on_open() {
    let mut state = fresh();
    // With no session only the profile mode is read.
    let effects = update(&mut state, PermissionsInput::ConnectionOpened);
    assert_eq!(
        effects,
        vec![PermissionsEffect::ReadProfileMode { generation: 0 }]
    );
    assert_eq!(state.segment(), None);

    // Opening a session reads both.
    let effects = update(
        &mut state,
        PermissionsInput::SessionOpened {
            session_id: S1.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    assert_eq!(profile.len(), 1);
    assert_eq!(bypass, vec![(S1.to_owned(), bypass[0].1)]);
    assert_eq!(state.segment(), None);

    // The segment stays hidden until both answer, then shows manual.
    answer_profile(&mut state, profile[0], Ok(ApprovalMode::Manual));
    assert_eq!(state.segment(), None);
    answer_bypass(&mut state, S1, bypass[0].1, Ok(false));
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: manual");
}

#[test]
fn stale_session_replies_are_dropped() {
    let mut state = fresh();
    let (profile1, bypass1) = open_known(&mut state, S1, ApprovalMode::Smart, false);
    // Press while known: a write goes out for s-1.
    let effects = update(&mut state, PermissionsInput::ToggleKey);
    let write1 = sets(&effects);
    assert_eq!(write1.len(), 1);
    assert_eq!(write1[0].0, S1);
    assert!(write1[0].2);

    // Open s-2: its reads carry higher generations than s-1's.
    let effects = update(
        &mut state,
        PermissionsInput::SessionOpened {
            session_id: S2.to_owned(),
        },
    );
    let (profile2, bypass2) = reads(&effects);
    assert_eq!(profile2.len(), 1);
    assert_eq!(bypass2.len(), 1);
    assert!(profile2[0] > profile1);
    assert!(bypass2[0].1 > bypass1);
    assert_eq!(bypass2[0].0, S2);

    // The s-1 read reply and the s-1 write reply arrive late: no change, no notice.
    let effects = answer_bypass(&mut state, S1, bypass1, Ok(true));
    assert!(notices(&effects).is_empty());
    let effects = update(
        &mut state,
        PermissionsInput::SessionOffSet {
            session_id: S1.to_owned(),
            generation: write1[0].1,
            sent: true,
            result: Ok(true),
        },
    );
    assert!(notices(&effects).is_empty());
    assert!(sets(&effects).is_empty());
    assert_eq!(state.segment(), None);

    // s-2 reads normally afterwards.
    answer_profile(&mut state, profile2[0], Ok(ApprovalMode::Smart));
    answer_bypass(&mut state, S2, bypass2[0].1, Ok(false));
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: smart");
}

#[test]
fn toggle_coalesces_presses() {
    // Press, press, press, reply on: one send, one P1.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let first = update(&mut state, PermissionsInput::ToggleKey);
    assert_eq!(sets(&first).len(), 1);
    let generation = sets(&first)[0].1;
    assert!(sets(&update(&mut state, PermissionsInput::ToggleKey)).is_empty());
    assert!(sets(&update(&mut state, PermissionsInput::ToggleKey)).is_empty());
    let effects = update(
        &mut state,
        PermissionsInput::SessionOffSet {
            session_id: S1.to_owned(),
            generation,
            sent: true,
            result: Ok(true),
        },
    );
    assert!(sets(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Warning,
            "Approvals off for this session only: Hermes will run commands it would normally \
             ask about. Rules that always block a command still apply. Shift+Tab turns approvals \
             back on."
                .to_owned()
        )]
    );

    // Press, press, reply on: the second send goes out; its reply draws one P2.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let first = update(&mut state, PermissionsInput::ToggleKey);
    let generation = sets(&first)[0].1;
    assert!(sets(&update(&mut state, PermissionsInput::ToggleKey)).is_empty());
    let effects = update(
        &mut state,
        PermissionsInput::SessionOffSet {
            session_id: S1.to_owned(),
            generation,
            sent: true,
            result: Ok(true),
        },
    );
    let second = sets(&effects);
    assert_eq!(second.len(), 1);
    assert!(!second[0].2);
    assert!(notices(&effects).is_empty());
    let effects = update(
        &mut state,
        PermissionsInput::SessionOffSet {
            session_id: S1.to_owned(),
            generation: second[0].1,
            sent: false,
            result: Ok(false),
        },
    );
    assert!(sets(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Approvals back on for this session: it follows profile fixture (smart).".to_owned()
        )]
    );

    // Press, press, reply refused: P10, wanted cleared; a later success sends nothing more.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let first = update(&mut state, PermissionsInput::ToggleKey);
    let generation = sets(&first)[0].1;
    assert!(sets(&update(&mut state, PermissionsInput::ToggleKey)).is_empty());
    let effects = update(
        &mut state,
        PermissionsInput::SessionOffSet {
            session_id: S1.to_owned(),
            generation,
            sent: true,
            result: Err(refused("permission_denied", 4001, "permission_denied")),
        },
    );
    assert!(sets(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Error,
            "Hermes refused the change: this connection may not change settings.".to_owned()
        )]
    );
    // A later press sends once more; its success draws the notice and sends nothing more.
    let effects = update(&mut state, PermissionsInput::ToggleKey);
    let sent = sets(&effects);
    assert_eq!(sent.len(), 1);
    let effects = update(
        &mut state,
        PermissionsInput::SessionOffSet {
            session_id: S1.to_owned(),
            generation: sent[0].1,
            sent: true,
            result: Ok(true),
        },
    );
    assert!(sets(&effects).is_empty());
    assert_eq!(notices(&effects).len(), 1);

    // `/permissions session off` with each profile reading: only the first names the key.
    for (mode, text) in [
        (
            Ok(ApprovalMode::Smart),
            "Approvals off for this session only: Hermes will run commands it would normally \
             ask about. Rules that always block a command still apply. Shift+Tab turns approvals \
             back on.",
        ),
        (
            Ok(ApprovalMode::Off),
            "This session now has approvals off for itself. Profile fixture also has approvals \
             off for every session; /permissions manual or /permissions smart changes the profile.",
        ),
        (
            Err(CallFailure::Unknown),
            "This session now has approvals off for itself: Hermes will run commands it would \
             normally ask about. Rules that always block a command still apply. /permissions \
             session default makes it follow profile fixture again.",
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
        let (profile, bypass_read) = reads(&effects);
        match mode {
            Ok(mode) => {
                answer_profile(&mut state, profile[0], Ok(mode));
            }
            Err(failure) => {
                answer_profile(&mut state, profile[0], Err(failure));
            }
        }
        answer_bypass(&mut state, S1, bypass_read[0].1, Ok(false));
        let now = Instant::now();
        let effects = update(
            &mut state,
            PermissionsInput::Command {
                command: PermissionsCommand::SessionOff,
                now,
            },
        );
        let sent = sets(&effects);
        assert_eq!(sent.len(), 1);
        assert!(sent[0].2);
        let effects = update(
            &mut state,
            PermissionsInput::SessionOffSet {
                session_id: S1.to_owned(),
                generation: sent[0].1,
                sent: true,
                result: Ok(true),
            },
        );
        assert_eq!(
            notices(&effects),
            vec![(NoticeKind::Warning, text.to_owned())],
            "profile reading"
        );
    }
}

#[test]
fn toggle_and_session_forms_without_a_known_bypass() {
    // P12: no session.
    let mut state = fresh();
    update(&mut state, PermissionsInput::ConnectionOpened);
    let effects = update(&mut state, PermissionsInput::ToggleKey);
    assert!(sets(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "No session is open, so this session's approvals cannot change. /permissions manual, \
             smart or off changes the profile."
                .to_owned()
        )]
    );

    // P7: profile off.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Off, false);
    let effects = update(&mut state, PermissionsInput::ToggleKey);
    assert!(sets(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Profile fixture has approvals off for every session, so this session already runs \
             without asking. Use /permissions manual or /permissions smart to change the profile."
                .to_owned()
        )]
    );

    // P13: bypass still pending. The read already in flight means nothing is sent.
    let mut state = fresh();
    update(&mut state, PermissionsInput::ConnectionOpened);
    let effects = update(
        &mut state,
        PermissionsInput::SessionOpened {
            session_id: S1.to_owned(),
        },
    );
    let (profile, _) = reads(&effects);
    answer_profile(&mut state, profile[0], Ok(ApprovalMode::Smart));
    let effects = update(&mut state, PermissionsInput::ToggleKey);
    assert!(sets(&effects).is_empty());
    let (profile_reads, bypass_reads) = reads(&effects);
    assert!(profile_reads.is_empty());
    assert!(bypass_reads.is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Talaria is still reading this session's approval setting. Try again in a moment."
                .to_owned()
        )]
    );
    // The session subcommands also give P13 while pending, and send nothing.
    for command in [
        PermissionsCommand::SessionOff,
        PermissionsCommand::SessionDefault,
    ] {
        let effects = update(
            &mut state,
            PermissionsInput::Command {
                command,
                now: Instant::now(),
            },
        );
        assert!(sets(&effects).is_empty());
        let (profile_reads, bypass_reads) = reads(&effects);
        assert!(profile_reads.is_empty());
        assert!(bypass_reads.is_empty());
        assert_eq!(notices(&effects).len(), 1);
    }

    // P13 with a failed bypass also rereads it.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let effects = update(
        &mut state,
        PermissionsInput::TurnCompleted {
            session_id: S1.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    answer_profile(&mut state, profile[0], Ok(ApprovalMode::Smart));
    answer_bypass(&mut state, S1, bypass[0].1, Err(CallFailure::Unknown));
    let effects = update(&mut state, PermissionsInput::ToggleKey);
    assert!(sets(&effects).is_empty());
    let (_, bypass_reads) = reads(&effects);
    assert_eq!(bypass_reads.len(), 1);
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Talaria is still reading this session's approval setting. Try again in a moment."
                .to_owned()
        )]
    );

    // With the profile off and the bypass on, `session default` is still sent.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Off, true);
    let effects = update(
        &mut state,
        PermissionsInput::Command {
            command: PermissionsCommand::SessionDefault,
            now: Instant::now(),
        },
    );
    let sent = sets(&effects);
    assert_eq!(sent.len(), 1);
    assert!(!sent[0].2);
    let effects = update(
        &mut state,
        PermissionsInput::SessionOffSet {
            session_id: S1.to_owned(),
            generation: sent[0].1,
            sent: false,
            result: Ok(false),
        },
    );
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Warning,
            "This session follows profile fixture again, which has approvals off for every \
             session."
                .to_owned()
        )]
    );
}

fn redraws(effects: &[PermissionsEffect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, PermissionsEffect::Redraw))
        .count()
}

/// Toggle once from a known bypass-off state; returns the write generation.
fn toggle_on(state: &mut PermissionsState) -> u64 {
    let effects = update(state, PermissionsInput::ToggleKey);
    let sent = sets(&effects);
    assert_eq!(sent.len(), 1);
    assert!(sent[0].2);
    sent[0].1
}

fn answer_write(
    state: &mut PermissionsState,
    generation: u64,
    sent: bool,
    result: Result<bool, CallFailure>,
) -> Vec<PermissionsEffect> {
    update(
        state,
        PermissionsInput::SessionOffSet {
            session_id: S1.to_owned(),
            generation,
            sent,
            result,
        },
    )
}

#[test]
fn bypass_write_outcomes() {
    // Unknown and unreadable: P9, a pending reading, a fresh read, and an unknown
    // segment until that read succeeds.
    for failure in [CallFailure::Unknown, CallFailure::Unreadable] {
        let mut state = fresh();
        open_known(&mut state, S1, ApprovalMode::Smart, false);
        let generation = toggle_on(&mut state);
        let effects = answer_write(&mut state, generation, true, Err(failure));
        assert_eq!(
            notices(&effects),
            vec![(
                NoticeKind::Warning,
                "Talaria could not confirm the change. Reading the setting from Hermes again."
                    .to_owned()
            )]
        );
        assert_eq!(state.session_off(), &Reading::Pending);
        let segment = state.segment().expect("the segment shows");
        assert_eq!(segment.text, "approvals: unknown");
        assert_eq!(segment.tone, ApprovalTone::Warning);
        let (_, bypass) = reads(&effects);
        assert_eq!(bypass.len(), 1);
        let effects = answer_bypass(&mut state, S1, bypass[0].1, Ok(false));
        assert_eq!(state.session_off(), &Reading::Known(false));
        let segment = state.segment().expect("the segment shows");
        assert_eq!(segment.text, "approvals: smart");
        assert_eq!(redraws(&effects), 1);
    }

    // Refused: P10, and the value is kept.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let generation = toggle_on(&mut state);
    let effects = answer_write(
        &mut state,
        generation,
        true,
        Err(refused("permission_denied", 4001, "permission_denied")),
    );
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Error,
            "Hermes refused the change: this connection may not change settings.".to_owned()
        )]
    );
    assert_eq!(state.session_off(), &Reading::Known(false));
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: smart");
    assert!(reads(&effects).0.is_empty());
    assert!(reads(&effects).1.is_empty());

    // Not sent: P16, and the value is kept.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let generation = toggle_on(&mut state);
    let effects = answer_write(&mut state, generation, true, Err(CallFailure::NotSent));
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Error,
            "The change was not sent: Talaria is not connected to Hermes. Nothing changed."
                .to_owned()
        )]
    );
    assert_eq!(state.session_off(), &Reading::Known(false));

    // Success with each profile reading: the three P2 forms.
    for (mode, text, kind) in [
        (
            Some(ApprovalMode::Smart),
            "Approvals back on for this session: it follows profile fixture (smart).",
            NoticeKind::System,
        ),
        (
            Some(ApprovalMode::Off),
            "This session follows profile fixture again, which has approvals off for every \
             session.",
            NoticeKind::Warning,
        ),
        (
            None,
            "This session follows profile fixture again. Talaria has not read the profile's \
             approval mode, so it cannot say whether approvals are on.",
            NoticeKind::Warning,
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
        if let Some(mode) = mode {
            answer_profile(&mut state, profile[0], Ok(mode));
        }
        answer_bypass(&mut state, S1, bypass[0].1, Ok(true));
        let effects = update(
            &mut state,
            PermissionsInput::Command {
                command: PermissionsCommand::SessionDefault,
                now: Instant::now(),
            },
        );
        let sent = sets(&effects);
        assert_eq!(sent.len(), 1);
        let effects = answer_write(&mut state, sent[0].1, false, Ok(false));
        assert_eq!(
            notices(&effects),
            vec![(kind, text.to_owned())],
            "profile reading"
        );
    }
}

#[test]
fn generations_floors_and_reread() {
    // A reply with an older generation changes nothing.
    let mut state = fresh();
    let (profile1, bypass1) = open_known(&mut state, S1, ApprovalMode::Smart, false);
    let effects = update(
        &mut state,
        PermissionsInput::TurnCompleted {
            session_id: S1.to_owned(),
        },
    );
    let (profile2, bypass2) = reads(&effects);
    assert_eq!(profile2.len(), 1);
    assert_eq!(bypass2.len(), 1);
    assert!(profile2[0] > profile1);
    assert!(bypass2[0].1 > bypass1);
    let effects = answer_bypass(&mut state, S1, bypass1, Ok(true));
    assert!(effects.is_empty());
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: smart");
    answer_profile(&mut state, profile2[0], Ok(ApprovalMode::Smart));
    answer_bypass(&mut state, S1, bypass2[0].1, Ok(false));
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: smart");

    // A read sent before a write, answered after the write's success, is dropped.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let effects = update(
        &mut state,
        PermissionsInput::TurnCompleted {
            session_id: S1.to_owned(),
        },
    );
    let (_, bypass) = reads(&effects);
    let stale = bypass[0].1;
    let generation = toggle_on(&mut state);
    let effects = answer_write(&mut state, generation, true, Ok(true));
    assert_eq!(notices(&effects).len(), 1);
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: off (this session)");
    let effects = answer_bypass(&mut state, S1, stale, Ok(false));
    assert!(effects.is_empty());
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: off (this session)");

    // A trigger during an in-flight read sends nothing until the reply, then exactly
    // one read.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Smart, false);
    let effects = update(
        &mut state,
        PermissionsInput::TurnCompleted {
            session_id: S1.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    let effects = update(
        &mut state,
        PermissionsInput::TurnCompleted {
            session_id: S1.to_owned(),
        },
    );
    let (again_profile, again_bypass) = reads(&effects);
    assert!(again_profile.is_empty());
    assert!(again_bypass.is_empty());
    let effects = answer_profile(&mut state, profile[0], Ok(ApprovalMode::Smart));
    let (again_profile, _) = reads(&effects);
    assert!(again_profile.is_empty());
    let effects = answer_bypass(&mut state, S1, bypass[0].1, Ok(false));
    let (again_profile, again_bypass) = reads(&effects);
    assert_eq!(again_profile.len(), 1);
    assert_eq!(again_bypass.len(), 1);
    assert!(again_profile[0] > profile[0]);
    assert!(again_bypass[0].1 > bypass[0].1);
}

#[test]
fn reread_triggers_and_failed_read_notices() {
    // Two turns with the same values: two read pairs, zero redraws.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Manual, false);
    let mut profile_reads = 0;
    let mut bypass_reads = 0;
    let mut redrawn = 0;
    for _ in 0..2 {
        let effects = update(
            &mut state,
            PermissionsInput::TurnCompleted {
                session_id: S1.to_owned(),
            },
        );
        redrawn += redraws(&effects);
        let (profile, bypass) = reads(&effects);
        assert_eq!(profile.len(), 1);
        assert_eq!(bypass.len(), 1);
        profile_reads += 1;
        bypass_reads += 1;
        let effects = answer_profile(&mut state, profile[0], Ok(ApprovalMode::Manual));
        redrawn += redraws(&effects);
        let effects = answer_bypass(&mut state, S1, bypass[0].1, Ok(false));
        redrawn += redraws(&effects);
    }
    assert_eq!((profile_reads, bypass_reads), (2, 2));
    assert_eq!(redrawn, 0);

    // A Hermes command that ran reads both.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Manual, false);
    let effects = update(
        &mut state,
        PermissionsInput::HermesCommandFinished {
            session_id: S1.to_owned(),
            name: "yolo".to_owned(),
            result: HermesCommandResult::Ran,
        },
    );
    let (profile, bypass) = reads(&effects);
    assert_eq!(profile.len(), 1);
    assert_eq!(bypass.len(), 1);

    // A Hermes command of unknown outcome reads both and shows unknown until both
    // succeed.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Manual, false);
    let effects = update(
        &mut state,
        PermissionsInput::HermesCommandFinished {
            session_id: S1.to_owned(),
            name: "approvals".to_owned(),
            result: HermesCommandResult::Unknown,
        },
    );
    let (profile, bypass) = reads(&effects);
    assert_eq!(profile.len(), 1);
    assert_eq!(bypass.len(), 1);
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: unknown");
    answer_profile(&mut state, profile[0], Ok(ApprovalMode::Manual));
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: unknown");
    answer_bypass(&mut state, S1, bypass[0].1, Ok(false));
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: manual");

    // Not run, another name, another session, another session's turn: nothing.
    for input in [
        PermissionsInput::HermesCommandFinished {
            session_id: S1.to_owned(),
            name: "yolo".to_owned(),
            result: HermesCommandResult::NotRun,
        },
        PermissionsInput::HermesCommandFinished {
            session_id: S1.to_owned(),
            name: "compact".to_owned(),
            result: HermesCommandResult::Ran,
        },
        PermissionsInput::HermesCommandFinished {
            session_id: S2.to_owned(),
            name: "yolo".to_owned(),
            result: HermesCommandResult::Ran,
        },
        PermissionsInput::TurnCompleted {
            session_id: S2.to_owned(),
        },
    ] {
        let mut state = fresh();
        open_known(&mut state, S1, ApprovalMode::Manual, false);
        let effects = update(&mut state, input);
        assert!(effects.is_empty());
    }

    // A failed re-read after known turns the reading failed and emits P11; a second
    // failure emits nothing; a success then a failure emits P11 again.
    let mut state = fresh();
    open_known(&mut state, S1, ApprovalMode::Manual, false);
    let effects = update(
        &mut state,
        PermissionsInput::TurnCompleted {
            session_id: S1.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    answer_bypass(&mut state, S1, bypass[0].1, Ok(false));
    let effects = answer_profile(
        &mut state,
        profile[0],
        Err(refused("permission_denied", 4001, "permission_denied")),
    );
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Warning,
            "Talaria could not read the approval setting: this connection may not read \
             settings. The status line shows \"approvals: unknown\" until a read succeeds."
                .to_owned()
        )]
    );
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: unknown");
    let effects = update(
        &mut state,
        PermissionsInput::TurnCompleted {
            session_id: S1.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    answer_bypass(&mut state, S1, bypass[0].1, Ok(false));
    let effects = answer_profile(
        &mut state,
        profile[0],
        Err(refused("permission_denied", 4001, "permission_denied")),
    );
    assert!(notices(&effects).is_empty());
    let effects = update(
        &mut state,
        PermissionsInput::TurnCompleted {
            session_id: S1.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    answer_bypass(&mut state, S1, bypass[0].1, Ok(false));
    let effects = answer_profile(&mut state, profile[0], Ok(ApprovalMode::Manual));
    assert!(notices(&effects).is_empty());
    let segment = state.segment().expect("the segment shows");
    assert_eq!(segment.text, "approvals: manual");
    let effects = update(
        &mut state,
        PermissionsInput::TurnCompleted {
            session_id: S1.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    answer_bypass(&mut state, S1, bypass[0].1, Ok(false));
    let effects = answer_profile(
        &mut state,
        profile[0],
        Err(refused("permission_denied", 4001, "permission_denied")),
    );
    assert_eq!(notices(&effects).len(), 1);

    // A profile read failing before any session emits nothing; failing after the
    // session opens emits P11.
    let mut state = fresh();
    let effects = update(&mut state, PermissionsInput::ConnectionOpened);
    let (profile, _) = reads(&effects);
    let effects = answer_profile(
        &mut state,
        profile[0],
        Err(refused("permission_denied", 4001, "permission_denied")),
    );
    assert!(notices(&effects).is_empty());
    let effects = update(
        &mut state,
        PermissionsInput::SessionOpened {
            session_id: S1.to_owned(),
        },
    );
    let (profile, bypass) = reads(&effects);
    answer_bypass(&mut state, S1, bypass[0].1, Ok(false));
    let effects = answer_profile(
        &mut state,
        profile[0],
        Err(refused("permission_denied", 4001, "permission_denied")),
    );
    assert_eq!(notices(&effects).len(), 1);

    // P11's words cover every failure.
    for (failure, words) in [
        (
            refused("permission_denied", 4001, "permission_denied"),
            "this connection may not read settings",
        ),
        (
            refused("unknown_method", -32601, "unknown method: config.get"),
            "this Hermes gateway does not offer config.get",
        ),
        (CallFailure::Unknown, "Hermes did not answer"),
        (
            CallFailure::Unreadable,
            "Hermes gave an answer Talaria could not read",
        ),
        (CallFailure::NotSent, "Talaria is not connected to Hermes"),
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
        answer_bypass(&mut state, S1, bypass[0].1, Ok(false));
        let effects = answer_profile(&mut state, profile[0], Err(failure));
        assert_eq!(
            notices(&effects),
            vec![(
                NoticeKind::Warning,
                format!(
                    "Talaria could not read the approval setting: {words}. The status line \
                     shows \"approvals: unknown\" until a read succeeds."
                )
            )],
            "words {words}"
        );
    }
}
