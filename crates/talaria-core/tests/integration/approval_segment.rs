//! Test 2 (#237): the approvals segment shows every row of its table in order.

use talaria_core::permissions::{
    ApprovalMode, ApprovalTone, CallFailure, HermesCommandResult, PermissionsEffect,
    PermissionsInput, PermissionsState,
};

const PROFILE: &str = "fixture";
const SESSION: &str = "s-1";
const KEY: &str = "Shift+Tab";

fn fresh() -> PermissionsState {
    PermissionsState::new(PROFILE.to_owned())
}

fn update(state: &mut PermissionsState, input: PermissionsInput) -> Vec<PermissionsEffect> {
    state.update(input, Some(KEY))
}

/// The generations the two reads of `effects` were sent with, profile first.
fn read_generations(effects: &[PermissionsEffect]) -> (u64, u64) {
    let mut profile = None;
    let mut session = None;
    for effect in effects {
        match effect {
            PermissionsEffect::ReadProfileMode { generation } => profile = Some(*generation),
            PermissionsEffect::ReadSessionOff { generation, .. } => session = Some(*generation),
            _ => {}
        }
    }
    (
        profile.expect("a profile read"),
        session.expect("a session read"),
    )
}

fn refused(reason: &str) -> CallFailure {
    CallFailure::Refused {
        reason: reason.to_owned(),
        rpc_code: 4001,
        message: reason.to_owned(),
    }
}

/// A state with a session whose first reads are in flight.
fn reading() -> (PermissionsState, u64, u64) {
    let mut state = fresh();
    update(&mut state, PermissionsInput::ConnectionOpened);
    let effects = update(
        &mut state,
        PermissionsInput::SessionOpened {
            session_id: SESSION.to_owned(),
        },
    );
    let (profile, session) = read_generations(&effects);
    (state, profile, session)
}

fn answer_profile(
    state: &mut PermissionsState,
    generation: u64,
    result: Result<ApprovalMode, CallFailure>,
) {
    update(
        state,
        PermissionsInput::ProfileModeRead { generation, result },
    );
}

fn answer_bypass(state: &mut PermissionsState, generation: u64, result: Result<bool, CallFailure>) {
    update(
        state,
        PermissionsInput::SessionOffRead {
            session_id: SESSION.to_owned(),
            generation,
            result,
        },
    );
}

fn text(state: &PermissionsState) -> Option<(String, ApprovalTone)> {
    state.segment().map(|segment| (segment.text, segment.tone))
}

#[test]
fn text_and_tone_for_every_state() {
    // Row 1: no session, nothing read.
    assert_eq!(text(&fresh()), None);

    // Row 7: a session, both first reads in flight.
    let (state, _, _) = reading();
    assert_eq!(text(&state), None);

    // Row 4: manual, bypass off.
    let (mut state, profile, session) = reading();
    answer_profile(&mut state, profile, Ok(ApprovalMode::Manual));
    answer_bypass(&mut state, session, Ok(false));
    assert_eq!(
        text(&state),
        Some(("approvals: manual".to_owned(), ApprovalTone::Muted))
    );

    // Row 5: smart, bypass off.
    let (mut state, profile, session) = reading();
    answer_profile(&mut state, profile, Ok(ApprovalMode::Smart));
    answer_bypass(&mut state, session, Ok(false));
    assert_eq!(
        text(&state),
        Some(("approvals: smart".to_owned(), ApprovalTone::Muted))
    );

    // Row 2: profile off, whatever the bypass is.
    for bypass in [false, true] {
        let (mut state, profile, session) = reading();
        answer_profile(&mut state, profile, Ok(ApprovalMode::Off));
        answer_bypass(&mut state, session, Ok(bypass));
        assert_eq!(
            text(&state),
            Some(("approvals: off".to_owned(), ApprovalTone::Warning)),
            "bypass {bypass}"
        );
    }
    // Row 2 with the bypass not read yet.
    let (mut state, profile, _) = reading();
    answer_profile(&mut state, profile, Ok(ApprovalMode::Off));
    assert_eq!(
        text(&state),
        Some(("approvals: off".to_owned(), ApprovalTone::Warning))
    );

    // Row 3: bypass on, profile manual, smart, unread, or failed.
    let (mut state, profile, session) = reading();
    answer_profile(&mut state, profile, Ok(ApprovalMode::Manual));
    answer_bypass(&mut state, session, Ok(true));
    assert_eq!(
        text(&state),
        Some((
            "approvals: off (this session)".to_owned(),
            ApprovalTone::Warning
        ))
    );
    let (mut state, profile, session) = reading();
    answer_profile(&mut state, profile, Ok(ApprovalMode::Smart));
    answer_bypass(&mut state, session, Ok(true));
    assert_eq!(
        text(&state),
        Some((
            "approvals: off (this session)".to_owned(),
            ApprovalTone::Warning
        ))
    );
    let (mut state, _, session) = reading();
    answer_bypass(&mut state, session, Ok(true));
    assert_eq!(
        text(&state),
        Some((
            "approvals: off (this session)".to_owned(),
            ApprovalTone::Warning
        ))
    );
    let (mut state, profile, session) = reading();
    answer_profile(&mut state, profile, Err(refused("permission_denied")));
    answer_bypass(&mut state, session, Ok(true));
    assert_eq!(
        text(&state),
        Some((
            "approvals: off (this session)".to_owned(),
            ApprovalTone::Warning
        ))
    );

    // Row 6, first way: the profile read failed.
    let (mut state, profile, session) = reading();
    answer_profile(&mut state, profile, Err(refused("permission_denied")));
    answer_bypass(&mut state, session, Ok(false));
    assert_eq!(
        text(&state),
        Some(("approvals: unknown".to_owned(), ApprovalTone::Warning))
    );

    // Row 6, second way: the bypass read failed.
    let (mut state, profile, session) = reading();
    answer_profile(&mut state, profile, Ok(ApprovalMode::Smart));
    answer_bypass(&mut state, session, Err(CallFailure::Unknown));
    assert_eq!(
        text(&state),
        Some(("approvals: unknown".to_owned(), ApprovalTone::Warning))
    );

    // Row 6, third way: a re-read fails after both were known.
    let (mut state, profile, session) = reading();
    answer_profile(&mut state, profile, Ok(ApprovalMode::Smart));
    answer_bypass(&mut state, session, Ok(false));
    assert_eq!(
        text(&state),
        Some(("approvals: smart".to_owned(), ApprovalTone::Muted))
    );
    let effects = update(
        &mut state,
        PermissionsInput::TurnCompleted {
            session_id: SESSION.to_owned(),
        },
    );
    let (profile, _) = read_generations(&effects);
    answer_profile(&mut state, profile, Err(CallFailure::Unreadable));
    assert_eq!(
        text(&state),
        Some(("approvals: unknown".to_owned(), ApprovalTone::Warning))
    );

    // Row 6, fourth way: the bypass write ends unknown.
    let (mut state, profile, session) = reading();
    answer_profile(&mut state, profile, Ok(ApprovalMode::Smart));
    answer_bypass(&mut state, session, Ok(false));
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
            session_id: SESSION.to_owned(),
            generation,
            sent: true,
            result: Err(CallFailure::Unknown),
        },
    );
    assert_eq!(
        text(&state),
        Some(("approvals: unknown".to_owned(), ApprovalTone::Warning))
    );

    // Row 6, fifth way: a Hermes command ends unknown.
    let (mut state, profile, session) = reading();
    answer_profile(&mut state, profile, Ok(ApprovalMode::Smart));
    answer_bypass(&mut state, session, Ok(false));
    update(
        &mut state,
        PermissionsInput::HermesCommandFinished {
            session_id: SESSION.to_owned(),
            name: "yolo".to_owned(),
            result: HermesCommandResult::Unknown,
        },
    );
    assert_eq!(
        text(&state),
        Some(("approvals: unknown".to_owned(), ApprovalTone::Warning))
    );
}
