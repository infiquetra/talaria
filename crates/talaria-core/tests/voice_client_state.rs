use talaria_core::voice::client::{ClientVoice, Effect, Input};

#[test]
fn final_text_is_drafted_once_and_stale_results_are_ignored() {
    let mut state = ClientVoice::default();
    assert_eq!(state.update(Input::Start), [Effect::Capture { epoch: 1 }]);
    assert_eq!(
        state.update(Input::CaptureFinished { epoch: 1 }),
        [Effect::Transcribe { epoch: 1 }]
    );
    assert!(
        state
            .update(Input::Text {
                epoch: 0,
                text: "old".into()
            })
            .is_empty()
    );
    assert_eq!(
        state.update(Input::Text {
            epoch: 1,
            text: "hello".into()
        }),
        [Effect::Draft("hello".into())]
    );
    assert!(
        state
            .update(Input::Text {
                epoch: 1,
                text: "duplicate".into()
            })
            .is_empty()
    );
    assert_eq!(state.update(Input::Start), [Effect::Capture { epoch: 2 }]);
    state.update(Input::Detach);
    assert!(
        state
            .update(Input::Text {
                epoch: 2,
                text: "late".into()
            })
            .is_empty()
    );
}

#[test]
fn reply_from_a_previous_capture_cannot_change_the_next_capture() {
    let mut state = ClientVoice::default();
    state.update(Input::Start);
    state.update(Input::CaptureFinished { epoch: 1 });
    state.update(Input::Text {
        epoch: 1,
        text: "first".into(),
    });
    assert_eq!(state.update(Input::Start), [Effect::Capture { epoch: 2 }]);
    assert!(
        state
            .update(Input::Text {
                epoch: 1,
                text: "late".into()
            })
            .is_empty()
    );
    assert!(state.recording());
}
