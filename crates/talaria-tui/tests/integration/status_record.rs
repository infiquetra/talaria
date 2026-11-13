//! Encoding and lifecycle emission of 7501 status records.

use std::io;
use std::sync::{Arc, Mutex};

use talaria_core::{BlockKind, SeatState};
use talaria_tui::terminal::{
    InlineTerminal, ProbeReply, RestoreLink, StatusSupport, clear_status_record, status_record,
};

const IDLE_RECORD: &[u8] = b"\x1b]7501;state=idle:app=talaria\x07";
const WORKING_RECORD: &[u8] = b"\x1b]7501;state=working:app=talaria\x07";

#[derive(Clone, Default)]
struct ByteSink(Arc<Mutex<Vec<u8>>>);

impl ByteSink {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.0.lock().expect("sink lock"))
    }
}

impl io::Write for ByteSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("sink lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn each_state_and_kind_has_exact_bytes() {
    let full = StatusSupport {
        idle: true,
        working: true,
        done: true,
        blocked: true,
        error: true,
        permission: true,
        question: true,
        auth: true,
    };

    assert_eq!(
        status_record(SeatState::Idle, full),
        Some(IDLE_RECORD.to_vec())
    );
    assert_eq!(
        status_record(SeatState::Working, full),
        Some(WORKING_RECORD.to_vec())
    );
    assert_eq!(
        status_record(SeatState::Done, full),
        Some(b"\x1b]7501;state=done:app=talaria\x07".to_vec())
    );
    assert_eq!(
        status_record(SeatState::Error, full),
        Some(b"\x1b]7501;state=error:app=talaria\x07".to_vec())
    );
    assert_eq!(
        status_record(SeatState::Blocked(Some(BlockKind::Permission)), full),
        Some(b"\x1b]7501;state=blocked:app=talaria:kind=permission\x07".to_vec())
    );
    assert_eq!(
        status_record(SeatState::Blocked(Some(BlockKind::Question)), full),
        Some(b"\x1b]7501;state=blocked:app=talaria:kind=question\x07".to_vec())
    );
    assert_eq!(
        status_record(SeatState::Blocked(Some(BlockKind::Auth)), full),
        Some(b"\x1b]7501;state=blocked:app=talaria:kind=auth\x07".to_vec())
    );
    assert_eq!(
        status_record(SeatState::Blocked(None), full),
        Some(b"\x1b]7501;state=blocked:app=talaria\x07".to_vec())
    );
    assert_eq!(clear_status_record(), b"\x1b]7501;state=clear\x07");
}

#[test]
fn unlisted_state_falls_back_or_writes_nothing() {
    let idle_only = StatusSupport {
        idle: true,
        ..Default::default()
    };
    assert_eq!(
        status_record(SeatState::Done, idle_only),
        Some(IDLE_RECORD.to_vec())
    );
    assert_eq!(
        status_record(SeatState::Error, idle_only),
        Some(IDLE_RECORD.to_vec())
    );

    let none_supported = StatusSupport::default();
    assert_eq!(status_record(SeatState::Done, none_supported), None);
    assert_eq!(status_record(SeatState::Error, none_supported), None);
    assert_eq!(status_record(SeatState::Working, none_supported), None);

    let blocked_only = StatusSupport {
        blocked: true,
        ..Default::default()
    };
    assert_eq!(
        status_record(
            SeatState::Blocked(Some(BlockKind::Permission)),
            blocked_only
        ),
        Some(b"\x1b]7501;state=blocked:app=talaria\x07".to_vec())
    );
    assert_eq!(
        status_record(
            SeatState::Blocked(Some(BlockKind::Permission)),
            none_supported
        ),
        None
    );
}

#[test]
fn report_seat_writes_nothing_without_support() {
    let sink = ByteSink::default();
    let mut unsupported = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        1,
        ProbeReply::default(),
        RestoreLink::detached(),
    )
    .expect("inline terminal");
    unsupported
        .report_seat(SeatState::Working)
        .expect("report seat");
    let bytes = sink.take();
    assert!(
        !contains_bytes(&bytes, b"]7501;state="),
        "unsupported terminal must write no status record: {bytes:?}"
    );

    let sink = ByteSink::default();
    let probe = ProbeReply {
        status: Some(StatusSupport {
            working: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut supported =
        InlineTerminal::new(sink.clone(), (80, 24), 1, probe, RestoreLink::detached())
            .expect("inline terminal");
    sink.take();

    supported
        .report_seat(SeatState::Working)
        .expect("report working");
    assert_eq!(sink.take(), WORKING_RECORD);

    supported
        .report_seat(SeatState::Working)
        .expect("repeat working");
    assert!(sink.take().is_empty(), "unchanged state writes nothing");
}

#[test]
fn clear_is_written_once_after_a_record() {
    let probe = ProbeReply {
        status: Some(StatusSupport {
            idle: true,
            working: true,
            ..Default::default()
        }),
        ..Default::default()
    };

    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(sink.clone(), (80, 24), 1, probe, RestoreLink::detached())
        .expect("inline terminal");
    sink.take();

    term.report_seat(SeatState::Working)
        .expect("report working");
    assert_eq!(sink.take(), WORKING_RECORD);

    term.release().expect("release");
    let release_bytes = sink.take();
    assert!(
        !contains_bytes(&release_bytes, b"state=clear"),
        "release must not clear status: {release_bytes:?}"
    );

    term.report_seat(SeatState::Idle)
        .expect("report idle while released");
    let while_released = sink.take();
    assert!(
        !contains_bytes(&while_released, b"]7501;state="),
        "report_seat must write nothing while released: {while_released:?}"
    );

    term.reacquire(&mut || Ok(probe)).expect("reacquire");
    let reacquire_bytes = sink.take();
    assert!(
        contains_bytes(&reacquire_bytes, IDLE_RECORD),
        "reacquire must write the remembered idle record: {reacquire_bytes:?}"
    );
    assert!(
        !contains_bytes(&reacquire_bytes, WORKING_RECORD),
        "reacquire must not write the stale working record: {reacquire_bytes:?}"
    );
    assert_ne!(
        reacquire_bytes, IDLE_RECORD,
        "reacquire output also includes setup bytes"
    );

    term.report_seat(SeatState::Idle)
        .expect("report idle after reacquire");
    assert!(
        sink.take().is_empty(),
        "report_seat of the state already written on reacquire writes nothing"
    );

    let sink2 = ByteSink::default();
    let mut term2 = InlineTerminal::new(sink2.clone(), (80, 24), 1, probe, RestoreLink::detached())
        .expect("inline terminal");
    sink2.take();
    term2
        .report_seat(SeatState::Working)
        .expect("report working");
    sink2.take();
    term2.release().expect("release");
    sink2.take();
    term2.reacquire(&mut || Ok(probe)).expect("reacquire");
    let reacquire2_bytes = sink2.take();
    assert!(
        contains_bytes(&reacquire2_bytes, WORKING_RECORD),
        "reacquire with no held state writes the last written state: {reacquire2_bytes:?}"
    );

    let sink3 = ByteSink::default();
    let mut term3 = InlineTerminal::new(sink3.clone(), (80, 24), 1, probe, RestoreLink::detached())
        .expect("inline terminal");
    sink3.take();
    term3.release().expect("release");
    sink3.take();
    term3.reacquire(&mut || Ok(probe)).expect("reacquire");
    let reacquire3_bytes = sink3.take();
    assert!(
        !contains_bytes(&reacquire3_bytes, b"]7501;state="),
        "reacquire with no reported state writes no record: {reacquire3_bytes:?}"
    );

    let plain_sink = ByteSink::default();
    let mut plain = InlineTerminal::new(
        plain_sink.clone(),
        (80, 24),
        1,
        ProbeReply::default(),
        RestoreLink::detached(),
    )
    .expect("inline terminal");
    plain_sink.take();
    plain.restore().expect("plain restore");
    let plain_restore = plain_sink.take();
    assert!(!plain_restore.is_empty());
    assert!(
        !contains_bytes(&plain_restore, b"]7501"),
        "restore without a prior record must write no 7501 sequence: {plain_restore:?}"
    );

    term.restore().expect("first restore");
    let restored = sink.take();
    assert!(
        restored.starts_with(clear_status_record()),
        "restore after a record must start with clear_status_record: {restored:?}"
    );
    assert_eq!(
        &restored[clear_status_record().len()..],
        plain_restore.as_slice(),
        "restore must continue with the existing restore sequence"
    );

    term.restore().expect("second restore");
    assert!(sink.take().is_empty(), "second restore writes nothing");
}
