//! Bracketed paste and the burst fallback.

#[path = "composer_support/mod.rs"]
mod composer_support;

use std::time::{Duration, Instant};

use talaria_tui::composer::paste_burst::{CharStep, EnterStep, PasteBurst};
use talaria_tui::composer::{ComposerContext, ComposerOutcome};

use composer_support::{Rig, keys};

#[test]
fn paste_inserts_literally_as_one_change() {
    let text = "x".repeat(10_000);
    let mut rig = Rig::new();
    let outcome = rig.composer.handle_paste(&text);
    assert_eq!(outcome, ComposerOutcome::Changed);
    assert_eq!(rig.composer.text(), text);
    assert_eq!(rig.composer.caret(), text.len());
    assert!(rig.composer.history().is_empty());
}

#[test]
fn paste_cleans_and_never_submits() {
    let esc = char::from(0x1b);
    let del = char::from(0x7f);
    let raw = format!("a\r\nb\rc{esc}{del}\u{0085}d\t");
    let mut rig = Rig::new();
    let outcome = rig.composer.handle_paste(&raw);
    assert_eq!(outcome, ComposerOutcome::Changed);
    assert_eq!(rig.composer.text(), "a\nb\ncd\t");
    assert!(rig.composer.history().is_empty());

    let trailing = rig.composer.handle_paste("hello\n");
    assert_eq!(trailing, ComposerOutcome::Changed);
    assert_eq!(rig.composer.text(), "a\nb\ncd\thello\n");
    assert!(rig.composer.history().is_empty());

    let mut empty = Rig::new();
    let only = format!("{esc}{del}\u{0085}");
    assert_eq!(
        empty.composer.handle_paste(&only),
        ComposerOutcome::Unchanged
    );
    assert_eq!(empty.composer.text(), "");
}

#[test]
fn burst_becomes_one_paste_and_slow_typing_does_not() {
    let text: String = (0..60)
        .map(|index| char::from(b'a' + (index % 26) as u8))
        .collect();
    let start = Instant::now();
    let mut machine = PasteBurst::new();
    let steps: Vec<CharStep> = text
        .chars()
        .enumerate()
        .map(|(index, ch)| machine.on_char(ch, start + Duration::from_millis(index as u64)))
        .collect();
    assert_eq!(steps[0], CharStep::Insert);
    assert_eq!(steps[1], CharStep::Insert);
    assert_eq!(steps[2], CharStep::TakeBack(2));
    assert!(steps[3..].iter().all(|step| *step == CharStep::Append));
    let last = start + Duration::from_millis(59);
    assert_eq!(machine.tick(last + Duration::from_millis(7)), None);
    assert_eq!(
        machine.tick(last + Duration::from_millis(8)).as_deref(),
        Some(text.as_str())
    );

    let mut slow = PasteBurst::new();
    let origin = Instant::now();
    assert_eq!(slow.on_char('a', origin), CharStep::Insert);
    assert_eq!(
        slow.on_char('b', origin + Duration::from_millis(30)),
        CharStep::Insert
    );
    assert_eq!(
        slow.on_char('c', origin + Duration::from_millis(39)),
        CharStep::Insert
    );
    assert_eq!(slow.tick(origin + Duration::from_millis(39 + 8)), None);

    let mut reset = PasteBurst::new();
    let origin = Instant::now();
    assert_eq!(reset.on_char('a', origin), CharStep::Insert);
    assert_eq!(
        reset.on_char('b', origin + Duration::from_millis(1)),
        CharStep::Insert
    );
    assert_eq!(reset.on_other_key(), None);
    assert_eq!(
        reset.on_char('c', origin + Duration::from_millis(2)),
        CharStep::Insert
    );
    assert_eq!(
        reset.on_char('d', origin + Duration::from_millis(3)),
        CharStep::Insert
    );
    assert_eq!(
        reset.on_char('e', origin + Duration::from_millis(4)),
        CharStep::TakeBack(2)
    );

    let mut rig = Rig::new();
    rig.burst(&text);
    rig.advance(8);
    assert_eq!(rig.tick(), ComposerOutcome::Changed);
    assert_eq!(rig.composer.text(), text);
    assert!(rig.composer.history().is_empty());

    let mut typed = Rig::new();
    let origin = typed.now;
    for (index, ch) in ['a', 'b', 'c'].into_iter().enumerate() {
        typed.now = origin + Duration::from_millis(30 * (index as u64 + 1));
        let key = keys(&ch.to_string()).remove(0);
        let _ = typed.composer.handle_key(key, idle(), typed.now);
    }
    typed.advance(8);
    assert_eq!(typed.tick(), ComposerOutcome::Unchanged);
    assert_eq!(typed.composer.text(), "abc");

    let mut broken = Rig::new();
    broken.burst("ab");
    let left = keys("left").remove(0);
    let _ = broken.composer.handle_key(left, idle(), broken.now);
    broken.now += Duration::from_millis(1);
    let see = keys("c").remove(0);
    let _ = broken.composer.handle_key(see, idle(), broken.now);
    assert_eq!(broken.composer.text(), "acb");

    let mut mid = Rig::new();
    mid.burst("abcd");
    assert_eq!(mid.composer.handle_paste("Z"), ComposerOutcome::Changed);
    assert_eq!(mid.composer.text(), "abcdZ");
}

#[test]
fn enter_in_burst_window_and_modified_keys() {
    assert_eq!(enter_step(50), EnterStep::Newline);
    assert_eq!(enter_step(120), EnterStep::Submit);
    assert_eq!(enter_step(200), EnterStep::Submit);
    assert_eq!(enter_after(50), ComposerOutcome::Changed);
    assert_eq!(draft_after_enter(50), "abc\n");
    assert_eq!(enter_after(120), ComposerOutcome::Submit("abc".to_string()));
    assert_eq!(enter_after(200), ComposerOutcome::Submit("abc".to_string()));

    let mut pasted = Rig::new();
    assert_eq!(
        pasted.composer.handle_paste("hello"),
        ComposerOutcome::Changed
    );
    let enter = keys("enter").remove(0);
    assert_eq!(
        pasted.composer.handle_key(enter, idle(), pasted.now),
        ComposerOutcome::Submit("hello".to_string())
    );

    let mut machine = PasteBurst::new();
    let start = Instant::now();
    assert_eq!(machine.on_char('a', start), CharStep::Insert);
    assert_eq!(
        machine.on_char('b', start + Duration::from_millis(1)),
        CharStep::Insert
    );
    assert_eq!(
        machine.on_char('c', start + Duration::from_millis(2)),
        CharStep::TakeBack(2)
    );
    assert_eq!(machine.on_other_key().as_deref(), Some("abc"));

    let mut rig = Rig::new();
    rig.burst("abc");
    let home = keys("ctrl+a").remove(0);
    let _ = rig.composer.handle_key(home, idle(), rig.now);
    assert_eq!(rig.composer.text(), "abc");
    assert_eq!(rig.composer.caret(), 0);
}

fn enter_step(ms_after_last: u64) -> EnterStep {
    let mut machine = PasteBurst::new();
    let start = Instant::now();
    let _ = machine.on_char('a', start);
    let _ = machine.on_char('b', start + Duration::from_millis(1));
    let _ = machine.on_char('c', start + Duration::from_millis(2));
    let _ = machine.tick(start + Duration::from_millis(10));
    machine.on_enter(start + Duration::from_millis(2 + ms_after_last))
}

fn enter_after(ms: u64) -> ComposerOutcome {
    let (outcome, _) = finish_burst(ms);
    outcome
}

fn draft_after_enter(ms: u64) -> String {
    finish_burst(ms).1
}

fn finish_burst(ms_after_last: u64) -> (ComposerOutcome, String) {
    let mut rig = Rig::new();
    rig.burst("abc");
    let last = rig.now;
    rig.now = last + Duration::from_millis(8);
    let _ = rig.tick();
    rig.now = last + Duration::from_millis(ms_after_last);
    let enter = keys("enter").remove(0);
    let outcome = rig.composer.handle_key(enter, idle(), rig.now);
    (outcome, rig.composer.text().to_string())
}

fn idle() -> ComposerContext {
    ComposerContext {
        turn_running: false,
    }
}

#[test]
fn burst_preserves_graphemes_at_both_insertion_boundaries() {
    let mut before_flag = Rig::new();
    before_flag.composer.handle_paste("🇸X");
    before_flag.press("home");
    before_flag.burst("🇺ab");
    assert_eq!(before_flag.composer.text(), "🇸X");
    assert_eq!(before_flag.composer.caret(), 0);
    before_flag.advance(8);
    before_flag.tick();
    assert_eq!(before_flag.composer.text(), "🇺ab🇸X");
    assert_eq!(before_flag.composer.caret(), "🇺ab".len());

    let mut after_base = Rig::new();
    after_base.composer.handle_paste("e");
    after_base.burst("\u{301}ab");
    assert_eq!(after_base.composer.text(), "e");
    assert_eq!(after_base.composer.caret(), 1);
    after_base.advance(8);
    after_base.tick();
    assert_eq!(after_base.composer.text(), "e\u{301}ab");
    assert_eq!(after_base.composer.caret(), "e\u{301}ab".len());
}
