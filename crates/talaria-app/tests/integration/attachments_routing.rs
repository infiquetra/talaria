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

/// Accepting or submitting an `@file` completion uses the same credential-path guard.
#[test]
fn completion_refuses_credential_files() {
    let workspace = tempfile::tempdir().expect("workspace");
    let home = tempfile::tempdir().expect("home");
    let root = workspace.path();
    for name in [".env.prod", "server.pem", "id_ed25519", "notes.txt"] {
        std::fs::write(root.join(name), b"x").expect(name);
    }
    let mut names = vec![".env.prod", "server.pem", "id_ed25519"];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join(".env.prod"), root.join("alias.txt")).expect("alias");
        names.push("alias.txt");
    }

    let mut state = AttachmentsState::new(CollapseRule::HERMES_DEFAULT, 16 * 1024 * 1024);
    state.set_home(home.path().to_path_buf());
    state.set_workspace(root.to_path_buf());
    let now = std::time::Instant::now();
    let idle = RouteContext {
        turn_running: false,
        palette_open: false,
    };

    for name in names {
        let draft = format!("read @{name}");
        state.on_edit(&draft, draft.len(), true, now);
        let request = state
            .mention
            .tick(now + Duration::from_millis(60))
            .expect("request");
        state.mention.on_items(
            request.generation,
            vec![CompletionItem {
                text: format!("@file:{name}"),
                display: name.to_owned(),
                meta: String::new(),
                kind: None,
            }],
        );
        let route = state.on_key(
            Some(Action::CompletionAccept),
            &draft,
            draft.len(),
            idle,
            now,
        );
        match route {
            KeyRoute::Consumed(effects) => {
                assert!(
                    effects.iter().any(|effect| matches!(
                        effect,
                        AttachmentEffect::Notice(text)
                            if text == "Talaria does not attach credential files."
                    )),
                    "{name} completion was accepted: {effects:?}"
                );
                assert!(
                    effects
                        .iter()
                        .all(|effect| !matches!(effect, AttachmentEffect::EditDraft { .. })),
                    "{name} completion changed the draft"
                );
            }
            KeyRoute::PassToComposer => panic!("{name} completion was not consumed"),
        }
        let submitted = state.on_submit(&format!("please read @file:{name}"));
        assert_eq!(
            submitted.expect_err("credential reference"),
            "Talaria does not attach credential files."
        );
    }

    for draft in [
        "please read @file:.env.prod:12",
        "please read @file:.env.prod:12-20,",
        "please read (@file:.env.prod).",
    ] {
        assert_eq!(
            state.on_submit(draft).expect_err("credential reference"),
            "Talaria does not attach credential files."
        );
    }

    let pasted = state
        .chips
        .collapse_paste("please read @file:.env.prod\nline 2\nline 3\nline 4\nline 5");
    assert_eq!(
        state
            .expand_pastes(&pasted)
            .expect_err("credential reference hidden in a paste"),
        "Talaria does not attach credential files."
    );
    assert_eq!(
        state
            .on_submit(&pasted)
            .expect_err("credential reference hidden in a paste"),
        "Talaria does not attach credential files."
    );

    let draft = "read @notes";
    state.on_edit(draft, draft.len(), true, now);
    let request = state
        .mention
        .tick(now + Duration::from_millis(60))
        .expect("request");
    state.mention.on_items(
        request.generation,
        vec![CompletionItem {
            text: "@file:notes.txt".to_owned(),
            display: "notes.txt".to_owned(),
            meta: String::new(),
            kind: None,
        }],
    );
    let route = state.on_key(
        Some(Action::CompletionAccept),
        draft,
        draft.len(),
        idle,
        now,
    );
    match route {
        KeyRoute::Consumed(effects) => assert!(effects.iter().any(|effect| matches!(
            effect,
            AttachmentEffect::EditDraft { text, .. } if text.contains("@file:notes.txt")
        ))),
        KeyRoute::PassToComposer => panic!("notes.txt completion was not accepted"),
    }
    state
        .on_submit("please read @file:notes.txt")
        .expect("ordinary file submits");
}
