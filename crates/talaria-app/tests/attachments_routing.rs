//! Key, paste and session routing for mentions and labels. Issue 224.

use std::path::Path;
use std::time::Duration;

use hermes_contract::schemas::CompletionItem;
use talaria_app::attachments::{
    Action, AttachmentEffect, AttachmentsState, KeyRoute, RouteContext,
};
use talaria_core::attachments::CollapseRule;
use talaria_core::session::SessionId;

/// Enter, Esc, Tab, Down, atomic delete, a slash draft, a session change, and a blank paste.
#[test]
fn keys_paste_and_session_change() {
    let mut state = AttachmentsState::new(CollapseRule::HERMES_DEFAULT, 16 * 1024 * 1024);
    let session = SessionId::new("local-session-1");
    let now = std::time::Instant::now();
    let running = RouteContext {
        turn_running: true,
        palette_open: false,
    };
    let idle = RouteContext {
        turn_running: false,
        palette_open: false,
    };

    let route = state.on_key(Some(Action::Interrupt), "", 0, running, now);
    assert_eq!(route, KeyRoute::PassToComposer);

    state.on_edit("@ca", 3, true, now);
    let route = state.on_key(Some(Action::CompletionClose), "@ca", 3, running, now);
    assert!(matches!(route, KeyRoute::Consumed(_)));
    assert!(!state.mention.is_open());

    state.on_edit("@zzz", 4, true, now);
    let route = state.on_key(Some(Action::Submit), "@zzz", 4, idle, now);
    assert_eq!(route, KeyRoute::PassToComposer);

    state.on_edit("see @calc", 9, true, now);
    let request = state
        .mention
        .tick(now + Duration::from_millis(60))
        .expect("request");
    state.mention.on_items(
        request.generation,
        vec![CompletionItem {
            text: "@file:src/calc.py".to_string(),
            display: "src/calc.py".to_string(),
            meta: String::new(),
            kind: None,
        }],
    );
    let route = state.on_key(Some(Action::Submit), "see @calc", 9, idle, now);
    match route {
        KeyRoute::Consumed(effects) => assert!(effects.iter().any(|effect| matches!(
            effect,
            AttachmentEffect::EditDraft { text, .. } if text.contains("@file:src/calc.py")
        ))),
        KeyRoute::PassToComposer => panic!("enter on a row must not submit"),
    }

    state.on_edit("@f", 2, true, now);
    let request = state
        .mention
        .tick(now + Duration::from_millis(60))
        .expect("request");
    state.mention.on_items(
        request.generation,
        (0..3)
            .map(|index| CompletionItem {
                text: format!("@file:f{index}.py"),
                display: format!("f{index}.py"),
                meta: String::new(),
                kind: None,
            })
            .collect(),
    );
    state.mention.next();
    state.mention.next();
    let route = state.on_key(Some(Action::CompletionNext), "@f", 2, idle, now);
    assert!(matches!(route, KeyRoute::Consumed(_)));
    assert_eq!(state.mention.selected().expect("row").text, "@file:f0.py");

    state.on_edit("@sr", 3, true, now);
    let request = state
        .mention
        .tick(now + Duration::from_millis(60))
        .expect("request");
    state.mention.on_items(
        request.generation,
        vec![CompletionItem {
            text: "@folder:src/".to_string(),
            display: "src".to_string(),
            meta: String::new(),
            kind: None,
        }],
    );
    let route = state.on_key(Some(Action::CompletionAccept), "@sr", 3, idle, now);
    assert!(matches!(route, KeyRoute::Consumed(_)));
    assert!(state.mention.is_open());

    let (_id, label) = state.chips.begin_image("", &session).expect("label");
    let route = state.on_key(Some(Action::DeleteBackward), &label, label.len(), idle, now);
    match route {
        KeyRoute::Consumed(effects) => assert!(effects.iter().any(|effect| matches!(
            effect,
            AttachmentEffect::EditDraft { text, .. } if text.is_empty()
        ))),
        KeyRoute::PassToComposer => panic!("backspace removes the whole label"),
    }
    let (_id, label) = state.chips.begin_image("", &session).expect("label");
    let route = state.on_key(Some(Action::DeleteForward), &label, 0, idle, now);
    assert!(matches!(route, KeyRoute::Consumed(_)));

    let effects = state.on_edit("/help @ca", "/help @ca".len(), true, now);
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, AttachmentEffect::Complete { .. }))
    );
    assert!(!state.mention.is_open());

    let image = state.chips.begin_image("", &session).expect("image").1;
    let paste = state.chips.collapse_paste("a\nb\nc\nd\ne");
    let effects = state.on_session_change(&SessionId::new("other-session"));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        AttachmentEffect::Notice(text)
            if text == "Attachments were removed because the session changed."
    )));
    let draft = format!("{paste} {image}");
    let live = state.chips.live(&draft);
    assert!(live.iter().any(|(chip, _)| chip.label == paste));
    assert!(live.iter().all(|(chip, _)| chip.label != image));

    let effects = state.on_paste(
        "  \n",
        &session,
        Path::new("/srv/talaria-project"),
        &CollapseRule::HERMES_DEFAULT,
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        AttachmentEffect::ReadClipboard { quiet: true, fallback_text: Some(text) }
            if text == "  \n"
    )));
}
