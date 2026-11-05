//! Client voice transitions independent of the gateway voice reducer.
use talaria_core::voice::VoicePhase;
use talaria_core::voice::client::{ClientVoice, Effect, Input};

#[test]
fn capture_and_stop_are_single_operations_and_use_the_shared_view() {
    let mut voice = ClientVoice::default();
    assert_eq!(voice.update(Input::Start), [Effect::Capture { epoch: 1 }]);
    assert_eq!(voice.view(true, false).phase, VoicePhase::Recording);
    assert!(voice.update(Input::Start).is_empty());
    assert_eq!(
        voice.update(Input::Finish),
        [Effect::FinishCapture { epoch: 1 }]
    );
    assert_eq!(voice.view(true, false).phase, VoicePhase::Stopping);
    assert!(voice.update(Input::Finish).is_empty());
    assert_eq!(
        voice.update(Input::CaptureFinished { epoch: 1 }),
        [Effect::Transcribe { epoch: 1 }]
    );
    assert_eq!(voice.view(true, false).phase, VoicePhase::Transcribing);
}

#[test]
fn reconnect_invalidates_the_detached_capture() {
    let mut voice = ClientVoice::default();
    voice.update(Input::Start);
    voice.update(Input::Detach);
    voice.update(Input::Reopen);
    assert_eq!(voice.update(Input::Start), [Effect::Capture { epoch: 2 }]);
    assert!(voice.update(Input::CaptureFinished { epoch: 1 }).is_empty());
    assert!(
        voice
            .update(Input::Text {
                epoch: 1,
                text: "late".into()
            })
            .is_empty()
    );
    assert_eq!(voice.view(true, false).phase, VoicePhase::Recording);
}
