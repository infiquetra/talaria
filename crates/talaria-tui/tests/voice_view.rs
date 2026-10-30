//! Microphone segment and the heard line, at several widths.

use std::time::Duration;

use talaria_tui::status::{Connection, Segment, StatusInputs, Tone, plan};
use talaria_tui::voice_view::{
    label_segment, partial_line, read_only_segment, ready_segment, recording_segment,
    transcribing_segment,
};

fn row(text: &str, cols: u16) -> String {
    let mut parser = vt100::Parser::new(1, cols, 0);
    parser.process(text.as_bytes());
    parser
        .screen()
        .rows(0, cols)
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_string()
}

#[test]
fn segment_and_partial_snapshots() {
    let bare = StatusInputs {
        connection: Connection::Connected,
        model: Some("alpha-model".to_owned()),
        needs_you: 2,
        ..StatusInputs::default()
    };
    let mut hidden = bare.clone();
    hidden.voice = None;
    assert_eq!(
        plan(&bare, None, 100).text(),
        plan(&hidden, None, 100).text(),
        "an empty voice value leaves the status line unchanged"
    );

    let recording = recording_segment(Duration::ZERO, false).expect("starting shows REC 0:00");
    assert_eq!(recording.text, "REC 0:00");
    assert_eq!(recording.tone, Tone::Warning);
    let long = recording_segment(Duration::from_secs(61 * 60 + 5), true).expect("REC 61:05");
    assert_eq!(long.text, "REC 61:05 +read");
    assert_eq!(long.tone, Tone::Warning);

    let ready = ready_segment(false).expect("ready shows voice");
    assert_eq!(ready.text, "voice");
    assert_eq!(ready.tone, Tone::Muted);
    assert_eq!(ready_segment(true).expect("ready read").text, "voice +read");
    let transcribing = transcribing_segment(false).expect("transcribing");
    assert_eq!(transcribing.text, "transcribing…");
    assert_eq!(transcribing.tone, Tone::Muted);
    assert_eq!(
        label_segment("warming", false).expect("label").text,
        "warming"
    );
    assert_eq!(read_only_segment().expect("read").text, "read");
    assert_eq!(read_only_segment().expect("read").tone, Tone::Muted);

    let heard = partial_line("what is\nnext", 40).expect("heard line");
    assert_eq!(heard, "heard: what is next");
    let cut = partial_line(&"word ".repeat(30), 20).expect("cut heard line");
    assert!(cut.starts_with("heard: "), "{cut}");
    assert!(cut.ends_with('…'), "{cut}");

    let mut inputs = bare.clone();
    inputs.model = Some("alpha-model-with-a-very-long-provider-name".to_owned());
    inputs.voice = Some(recording.clone());
    for cols in [40_u16, 60, 100, 160] {
        let text = plan(&inputs, None, cols).text();
        let screen = row(&text, cols);
        assert!(
            screen.contains("REC 0:00"),
            "{cols} columns lost the microphone: {screen:?}"
        );
        let heard_row = row(&heard, cols);
        assert!(
            heard_row.contains("heard:"),
            "{cols} columns lost the heard line: {heard_row:?}"
        );
    }
    let narrow = row(&plan(&inputs, None, 40).text(), 40);
    assert!(
        narrow.contains("REC 0:00"),
        "a narrow line keeps the microphone: {narrow:?}"
    );
    assert!(
        !narrow.contains("very-long-provider-name"),
        "a narrow line drops the model before the microphone: {narrow:?}"
    );

    assert!(
        Segment::Voice.priority() > Segment::NeedsYou.priority()
            && Segment::Voice.priority() < Segment::Approvals.priority(),
        "the microphone ranks immediately after prompts that need you"
    );
    assert_eq!(
        Segment::Voice.position() + 1,
        Segment::External.position(),
        "the microphone sits immediately before the external command"
    );
}

#[test]
fn phase_projection_hides_unknown_and_marks_a_pending_stop() {
    use talaria_core::voice::{RecordStatus, VoiceInput, VoiceState};
    use talaria_tui::voice_view::segment_for;

    let mut state = VoiceState::new("fixture-session", true, 1500);
    assert!(segment_for(&state.view()).is_none());
    state.update(VoiceInput::ModeRead(Some("voice_only".into())), true);
    state.update(VoiceInput::Press, true);
    state.update(
        VoiceInput::RecordReply {
            start: true,
            status: RecordStatus::Recording,
            reason: None,
        },
        true,
    );
    state.update(VoiceInput::Press, true);
    assert_eq!(
        segment_for(&state.view()).expect("stopping segment").text,
        "transcribing… +read"
    );
}
