//! Keystroke time with the mention list open. Issue 224.

use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use talaria_app::attachments::{Action, AttachmentsState, RouteContext};
use talaria_core::attachments::{CollapseRule, MentionView};
use talaria_tui::composer::{Composer, ComposerContext};
use talaria_tui::keys::{Key, KeyCode, KeyKind, KeyMap, Mods};
use talaria_tui::live::mention_then_composer;
use talaria_tui::mention::MentionSection;

/// A release-build keystroke, composer edit and draw stay within 4 ms and 8 ms.
#[test]
#[ignore]
fn keystroke_with_list_open_stays_in_budget() {
    if cfg!(debug_assertions) {
        println!("SKIPPED: debug build");
        return;
    }
    let rows = (0..30)
        .map(|index| (format!("f{index}.py"), format!("m{index}")))
        .collect();
    let shown = MentionView {
        rows,
        selected: Some(0),
        status: None,
        footer: "tab complete · enter insert · esc close",
    };
    let mut state = AttachmentsState::new(CollapseRule::HERMES_DEFAULT, 16 * 1024 * 1024);
    let mut composer = Composer::new(KeyMap::defaults());
    let draft = "a".repeat(200);
    composer.load(draft.clone());
    let idle = RouteContext {
        turn_running: false,
        palette_open: false,
    };
    let composer_ctx = ComposerContext {
        turn_running: false,
    };
    let mut samples = Vec::with_capacity(1_000);
    for index in 0..1_000 {
        let started = Instant::now();
        if index % 2 == 0 {
            let _ = state.on_key(
                Some(Action::CompletionNext),
                &draft,
                draft.len(),
                idle,
                Instant::now(),
            );
        } else {
            let _ = state.on_edit(&draft, draft.len(), true, Instant::now());
        }
        let key = if index % 2 == 0 {
            Key {
                code: KeyCode::Char('x'),
                mods: Mods::default(),
                kind: KeyKind::Press,
            }
        } else {
            Key {
                code: KeyCode::Backspace,
                mods: Mods::default(),
                kind: KeyKind::Press,
            }
        };
        let _ = composer.handle_key(key, composer_ctx, Instant::now());
        let mention = MentionSection::new(&shown);
        let section = composer.section(80, 22);
        let placed = mention_then_composer(&mention, &section);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| {
                let rects = placed.layout(frame.area().width, 22);
                for (piece, rect) in placed.sections.iter().zip(rects) {
                    piece.render(rect, frame.buffer_mut());
                }
            })
            .expect("draw");
        samples.push(started.elapsed());
    }
    samples.sort();
    let rank = samples
        .len()
        .saturating_mul(95)
        .div_ceil(100)
        .saturating_sub(1);
    let p95 = samples[rank];
    let worst = samples[samples.len() - 1];
    println!("p95 {p95:?} worst {worst:?}");
    assert!(p95 <= Duration::from_millis(4));
    assert!(worst <= Duration::from_millis(8));
}
