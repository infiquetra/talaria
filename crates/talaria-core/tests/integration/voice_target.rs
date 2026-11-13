//! Binding a recording to one fleet row, and keeping the transcript there.

use talaria_core::fleet::{FleetBackend, NeedId, NeedSlot, RowKey};
use talaria_core::voice::SubmitMode;
use talaria_core::voice::target::{
    QueueCandidate, RecordTarget, RowCandidate, TranscriptPlacement, bind_record_target,
    place_transcript,
};

fn key(session_id: &str) -> RowKey {
    RowKey {
        profile: "fixture".to_owned(),
        session_id: session_id.to_owned(),
    }
}

fn queue_item(session_id: &str, seq: u64, selected: bool, backend: FleetBackend) -> QueueCandidate {
    let key = key(session_id);
    QueueCandidate {
        id: NeedId {
            key,
            slot: NeedSlot::Prompt(session_id.to_owned()),
        },
        backend,
        seq,
        selected,
    }
}

fn row(
    session_id: &str,
    backend: FleetBackend,
    selected: bool,
    foreground: bool,
    working: bool,
) -> RowCandidate {
    RowCandidate {
        key: key(session_id),
        backend,
        selected,
        foreground,
        working,
    }
}

fn placed_on(placement: &TranscriptPlacement) -> (&RowKey, &str) {
    match placement {
        TranscriptPlacement::Draft { key, text } | TranscriptPlacement::NotSent { key, text } => {
            (key, text.as_str())
        }
    }
}

#[test]
fn selected_queue_item_beats_the_row_and_the_foreground() {
    let selected = queue_item("queue-old", 1, true, FleetBackend::Acp);
    let newest = queue_item("queue-new", 9, false, FleetBackend::Hermes);
    let rows = vec![
        row("selected-row", FleetBackend::Herdr, true, false, false),
        row("foreground", FleetBackend::Hermes, false, true, false),
        row("working", FleetBackend::Hermes, false, false, true),
    ];
    let bound = bind_record_target(&[selected.clone(), newest], &rows)
        .expect("the selected queue item is the target");
    assert_eq!(bound.key, selected.id.key);
    assert_eq!(bound.backend, FleetBackend::Acp);
    assert_eq!(bound.need.as_ref(), Some(&selected.id));
    assert_ne!(bound.key.session_id, "queue-new");
}

#[test]
fn selected_row_beats_the_foreground_when_nothing_in_the_queue_is_selected() {
    let newest = queue_item("queue-new", 9, false, FleetBackend::Hermes);
    let rows = vec![
        row("selected-row", FleetBackend::Herdr, true, false, false),
        row("foreground", FleetBackend::Hermes, false, true, false),
        row("working", FleetBackend::Hermes, false, false, true),
    ];
    let bound = bind_record_target(&[newest], &rows).expect("the selected fleet row is the target");
    assert_eq!(bound.key.session_id, "selected-row");
    assert_eq!(bound.backend, FleetBackend::Herdr);
    assert!(bound.need.is_none());
}

#[test]
fn foreground_session_is_the_last_resort() {
    let rows = vec![
        row("working", FleetBackend::Herdr, false, false, true),
        row("foreground", FleetBackend::Hermes, false, true, false),
    ];
    let bound = bind_record_target(&[], &rows).expect("the foreground session is the target");
    assert_eq!(bound.key.session_id, "foreground");
    assert_eq!(bound.backend, FleetBackend::Hermes);
    assert!(bound.need.is_none());
}

#[test]
fn focus_change_keeps_the_transcript_on_the_recorded_row() {
    let recorded = RecordTarget {
        key: key("recorded"),
        backend: FleetBackend::Acp,
        need: None,
    };
    let placement = place_transcript(&recorded, true, "ship the fix", SubmitMode::Direct);
    let (key, text) = placed_on(&placement);
    assert_eq!(key, &recorded.key);
    assert_eq!(text, "ship the fix");
    assert_ne!(key.session_id, "moved-to");
}

#[test]
fn direct_submit_still_holds_a_fleet_transcript_for_enter() {
    let recorded = RecordTarget {
        key: key("recorded"),
        backend: FleetBackend::Herdr,
        need: None,
    };
    let placement = place_transcript(&recorded, false, "ship the fix", SubmitMode::Direct);
    match placement {
        TranscriptPlacement::Draft { key, text } => {
            assert_eq!(key, recorded.key);
            assert_eq!(text, "ship the fix");
        }
        TranscriptPlacement::NotSent { .. } => {
            panic!("a fleet transcript stays in the reply editor until Enter");
        }
    }
}
