//! Client voice stops speech before it opens the microphone.

use talaria_core::voice::client::{ClientVoice, Cutoff, Effect, Input};

fn stops(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::StopPlayback { .. }))
}

fn captures(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::Capture { .. }))
}

#[test]
fn alt_r_stops_speech_before_recording() {
    let mut voice = ClientVoice::default();
    let epoch = voice.audio_epoch();
    voice.update(Input::SpeechStarted);
    let effects = voice.update(Input::Cutoff(Cutoff::Record));
    assert!(
        stops(&effects),
        "Alt+R asks playback to stop before the microphone opens"
    );
    assert!(
        !captures(&effects),
        "the microphone stays closed until quiet is confirmed"
    );
    assert!(
        voice.audio_epoch() > epoch,
        "Alt+R invalidates the audio epoch"
    );
}

#[test]
fn quiet_confirmation_gates_the_microphone() {
    let mut voice = ClientVoice::default();
    voice.update(Input::SpeechStarted);
    let stopped = voice.update(Input::Cutoff(Cutoff::Record));
    assert!(!captures(&stopped), "a stop without quiet never records");
    let epoch = voice.audio_epoch();
    let effects = voice.update(Input::Quiet { audio_epoch: epoch });
    assert!(
        captures(&effects),
        "recording starts only after the helper confirms quiet"
    );
    let stale = voice.update(Input::Quiet {
        audio_epoch: epoch.saturating_sub(1),
    });
    assert!(
        !captures(&stale),
        "a quiet report from an older audio epoch does not record"
    );
}

#[test]
fn voice_stop_clears_speech_and_keeps_the_draft() {
    let mut voice = ClientVoice::default();
    assert_eq!(voice.update(Input::Start), [Effect::Capture { epoch: 1 }]);
    voice.update(Input::CaptureFinished { epoch: 1 });
    assert_eq!(
        voice.update(Input::Text {
            epoch: 1,
            text: "keep this".into(),
        }),
        [Effect::Draft("keep this".into())]
    );
    voice.update(Input::SpeechStarted);
    let effects = voice.update(Input::Cutoff(Cutoff::Stop));
    assert!(stops(&effects), "/voice stop cancels playback");
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Draft(text) if text.is_empty())),
        "/voice stop keeps the draft already accepted"
    );
    assert!(
        !captures(&effects),
        "/voice stop does not start a recording"
    );
}

#[test]
fn read_off_stops_client_speech() {
    let mut voice = ClientVoice::default();
    voice.update(Input::SpeechStarted);
    let effects = voice.update(Input::Cutoff(Cutoff::ReadOff));
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Notice(text) if text == "Read-aloud stopped."
        )),
        "read off tells the operator that client speech stopped"
    );
    assert!(
        stops(&effects),
        "read off stops speech that is already playing"
    );
}

#[test]
fn escape_cancels_the_voice_operation() {
    let mut voice = ClientVoice::default();
    voice.update(Input::SpeechStarted);
    let effects = voice.update(Input::Cutoff(Cutoff::Escape));
    assert!(stops(&effects), "Esc cancels the active voice operation");
    assert!(!captures(&effects), "Esc does not open the microphone");
}

#[test]
fn target_change_and_quit_stop_speech() {
    let mut voice = ClientVoice::default();
    voice.update(Input::SpeechStarted);
    let moved = voice.update(Input::Cutoff(Cutoff::TargetChanged));
    assert!(
        stops(&moved),
        "changing target stops speech before another capture"
    );
    assert!(
        !captures(&moved),
        "a target change does not open the microphone"
    );

    let mut voice = ClientVoice::default();
    voice.update(Input::Start);
    let quit = voice.update(Input::Cutoff(Cutoff::Quit));
    assert!(
        stops(&quit)
            || quit
                .iter()
                .any(|effect| matches!(effect, Effect::FinishCapture { .. })),
        "quit stops playback and ends a capture"
    );
    assert!(!voice.recording(), "quit leaves the microphone closed");
}

#[test]
fn transcribing_press_is_busy() {
    let mut voice = ClientVoice::default();
    voice.update(Input::Start);
    let epoch = voice.audio_epoch();
    voice.update(Input::CaptureFinished { epoch });
    let effects = voice.update(Input::Cutoff(Cutoff::Record));
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Notice(text) if text.to_ascii_lowercase().contains("busy")
        )),
        "Alt+R during transcription shows a busy notice"
    );
    assert!(
        !captures(&effects),
        "a press during transcription does not open another capture"
    );
}

#[test]
fn late_synthesis_from_the_old_audio_epoch_is_dropped() {
    let mut voice = ClientVoice::default();
    let epoch = voice.audio_epoch();
    voice.update(Input::SpeechStarted);
    voice.update(Input::Cutoff(Cutoff::Stop));
    assert!(
        voice.audio_epoch() > epoch,
        "stop invalidates the audio epoch"
    );
    let late = voice.update(Input::LateSynthesis {
        audio_epoch: epoch,
        text: "old sentence".into(),
    });
    assert!(
        late.is_empty(),
        "a synthesis result from the old audio epoch is dropped"
    );
}
