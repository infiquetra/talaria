//! The pure screen helpers: tokens, the approval card, counting lines and the exit marker.
//! Every expected value is a hand written literal.

use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::time::Duration;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use rand::SeedableRng;
use rand::rngs::StdRng;

use smoke::{
    Card, Clock, NewTab, PaneId, ReadSource, TabId, TermError, Terminal, Token, approval_card,
    count_token, counting_values, exit_code, highest_number, settled,
};

const COMMAND: &str = "rm -rf .pytest_cache";
const COUNT_PROMPT: &str = "Count from 1 to 2000";
const WORDS: [&str; 8] = [
    "maple", "cobalt", "juniper", "saffron", "tundra", "walnut", "harbor", "quartz",
];

fn token(word: &'static str, number: u32) -> Token {
    Token { word, number }
}

fn card(lines: &[&str]) -> Card {
    approval_card(&lines.join("\n"), COMMAND)
}

fn shown(once_digit: Option<char>, once_highlighted: bool) -> Card {
    Card::Shown {
        once_digit,
        once_highlighted,
    }
}

fn sorted(mut values: Vec<u32>) -> Vec<u32> {
    values.sort_unstable();
    values
}

// ---------------------------------------------------------------- tokens

#[test]
fn reply_matches_prompt_echo_does_not() {
    let t = token("maple", 4821);
    assert_eq!(t.join(), "maple-4821");
    assert_eq!(
        t.prompt(),
        "Without using any tools, join these two words with a hyphen and reply with only the \
         result: maple and 4821"
    );
    assert!(!t.prompt().contains('\n'));
    assert!(!t.prompt().starts_with('/'));

    let echo = format!("› {}", t.prompt());
    assert_eq!(count_token(&echo, &t.join()), 0);

    let screen = format!("{echo}\n\nmaple-4821\n");
    assert_eq!(count_token(&screen, &t.join()), 1);

    // the echo wrapped over two rows is still no reply
    let wrapped = "› Without using any tools, join these two words with a hyphen and\n  reply with only the result: maple and 4821";
    assert_eq!(count_token(wrapped, "maple-4821"), 0);
}

#[test]
fn token_match_ignores_case_and_punctuation() {
    for text in [
        "maple-4821",
        "MAPLE-4821",
        "Maple-4821",
        "maple-4821.",
        "The answer is maple-4821!",
        "(maple-4821)",
        "`maple-4821`",
        "**Maple-4821**",
        "\"maple-4821\",",
    ] {
        assert_eq!(count_token(text, "maple-4821"), 1, "text: {text}");
    }
    // the query is matched case-insensitively too
    assert_eq!(count_token("maple-4821", "MAPLE-4821"), 1);
    assert_eq!(count_token("", "maple-4821"), 0);
}

#[test]
fn count_token_counts_whole_words_only() {
    assert_eq!(
        count_token(
            "maple-4821 maple-4821x amaple-4821 maple-48210 maple-4821",
            "maple-4821"
        ),
        2
    );
    assert_eq!(
        count_token("maple-4821\nMAPLE-4821\nmaple-4821", "maple-4821"),
        3
    );
    assert_eq!(
        count_token("maple 4821 maple4821 maple-482", "maple-4821"),
        0
    );
    assert_eq!(count_token("cobalt-1000 harbor-9999", "maple-4821"), 0);
}

#[test]
fn token_draw_never_repeats() {
    // Same seed, so the second draw proposes the first token again and must redraw.
    let mut first_rng = StdRng::seed_from_u64(7);
    let first = Token::new(&mut first_rng, &[]);
    let mut second_rng = StdRng::seed_from_u64(7);
    let second = Token::new(&mut second_rng, &[first]);
    assert_ne!(first, second);

    // A long run of draws, each told about the earlier ones.
    let mut rng = StdRng::seed_from_u64(20);
    let mut used: Vec<Token> = Vec::new();
    for _ in 0..300 {
        let t = Token::new(&mut rng, &used);
        assert!(
            WORDS.contains(&t.word),
            "word {} is not one of the eight",
            t.word
        );
        assert!((1000..=9999).contains(&t.number), "number {}", t.number);
        assert!(!used.contains(&t), "token {} was drawn twice", t.join());
        used.push(t);
    }
    let distinct: HashSet<(&str, u32)> = used.iter().map(|t| (t.word, t.number)).collect();
    assert_eq!(distinct.len(), 300);

    // The joined form is word, hyphen, number.
    let t = Token::new(&mut StdRng::seed_from_u64(1), &[]);
    assert_eq!(t.join(), format!("{}-{}", t.word, t.number));
}

#[test]
fn property_token_count_equals_insertions() {
    let config = Config {
        cases: 64,
        max_shrink_iters: 0,
        failure_persistence: None,
        ..Config::default()
    };
    let word = prop::sample::select(WORDS.to_vec());
    let insertion = (
        "[a-z ]{0,20}",
        prop::sample::select(vec!["", "(", "`", "*", "\""]),
        any::<bool>(),
        prop::sample::select(vec!["", ".", ",", "!", ")"]),
    );
    let strategy = (
        word,
        1000u32..=9999,
        prop::collection::vec(insertion, 0..8),
        "[a-z ]{0,20}",
    );
    let mut runner = TestRunner::new(config);
    runner
        .run(&strategy, |(word, number, insertions, tail)| {
            let joined = format!("{word}-{number}");
            let mut text = String::new();
            for (filler, before, upper, after) in &insertions {
                let rendered = if *upper {
                    joined.to_uppercase()
                } else {
                    joined.clone()
                };
                text.push_str(filler);
                text.push(' ');
                text.push_str(before);
                text.push_str(&rendered);
                text.push_str(after);
                text.push(' ');
            }
            text.push_str(&tail);
            prop_assert_eq!(count_token(&text, &joined), insertions.len());
            Ok(())
        })
        .expect("the count equals the number of insertions");
}

// ---------------------------------------------------------------- approval card

#[test]
fn card_needs_once_deny_and_exact_command() {
    // the card as drawn: command with a dollar sign, numbered choices, the first highlighted
    assert_eq!(
        card(&[
            "Some earlier output",
            "Approval needed",
            "$ rm -rf .pytest_cache",
            "› 1. once",
            "  2. deny",
            "  3. session",
            "  4. always",
        ]),
        shown(Some('1'), true)
    );
    // the dollar sign is optional
    assert_eq!(
        card(&["rm -rf .pytest_cache", "› 1. once", "  2. deny"]),
        shown(Some('1'), true)
    );
    // the labels are matched whatever their case
    assert_eq!(
        card(&["$ rm -rf .pytest_cache", "› 1. Once", "  2. DENY"]),
        shown(Some('1'), true)
    );
    // one label alone is no card
    assert_eq!(
        card(&["$ rm -rf .pytest_cache", "› 1. once", "  2. session"]),
        Card::Absent
    );
    assert_eq!(
        card(&["$ rm -rf .pytest_cache", "  1. session", "› 2. deny"]),
        Card::Absent
    );
    // the command alone is no card
    assert_eq!(card(&["$ rm -rf .pytest_cache"]), Card::Absent);
    assert_eq!(card(&["nothing to see here", "", "$ "]), Card::Absent);
    assert_eq!(card(&[]), Card::Absent);
}

#[test]
fn card_not_seen_when_a_reply_says_once() {
    // a reply that mentions "once" and the command inside a sentence
    assert_eq!(
        card(&[
            "› Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache",
            "I ran rm -rf .pytest_cache once and its exit code was 0.",
        ]),
        Card::Absent
    );
    // a reply that puts the command on a line of its own and says "once", with no "deny"
    assert_eq!(
        card(&[
            "$ rm -rf .pytest_cache",
            "That ran once and the exit code was 0.",
        ]),
        Card::Absent
    );
    // a reply that merely mentions once and deny with no command at all
    assert_eq!(
        card(&["You may answer once or deny the request.", "Anything else?"]),
        Card::Absent
    );
}

#[test]
fn card_for_a_longer_command_is_refused() {
    assert_eq!(
        card(&["$ cd ~ && rm -rf .pytest_cache", "› 1. once", "  2. deny"]),
        Card::OtherCommand
    );
    assert_eq!(
        card(&["cd ~ && rm -rf .pytest_cache", "  1. once", "› 2. deny"]),
        Card::OtherCommand
    );
    assert_eq!(
        card(&[
            "$ rm -rf .pytest_cache && echo done",
            "› 1. once",
            "  2. deny"
        ]),
        Card::OtherCommand
    );
    assert_eq!(
        card(&["$ rm -rf .pytest_cache_old", "› 1. once", "  2. deny"]),
        Card::OtherCommand
    );
    // a different command is still some other card
    assert_eq!(
        card(&["$ ls -la", "› 1. once", "  2. deny", "  3. session"]),
        Card::OtherCommand
    );
    // without numbered choices nothing is open
    assert_eq!(
        card(&["$ cd ~ && rm -rf .pytest_cache", "ran once, no deny"]),
        Card::Absent
    );
}

#[test]
fn card_found_in_box_drawn_rows() {
    assert_eq!(
        card(&[
            "╭──────────────────────────────╮",
            "│ Approval needed              │",
            "│ $ rm -rf .pytest_cache       │",
            "│ › 1. once                    │",
            "│   2. deny                    │",
            "│   3. session                 │",
            "╰──────────────────────────────╯",
        ]),
        shown(Some('1'), true)
    );
    // square corners and heavy rules
    assert_eq!(
        card(&[
            "┌━━━━━━━━━━━━━━━━━━━━┐",
            "│ rm -rf .pytest_cache │",
            "│   1. once          │",
            "│ ❯ 2. deny          │",
            "└━━━━━━━━━━━━━━━━━━━━┘",
        ]),
        shown(Some('1'), false)
    );
    // a longer command in a box is still some other command
    assert_eq!(
        card(&[
            "│ $ cd ~ && rm -rf .pytest_cache │",
            "│ › 1. once                      │",
            "│   2. deny                      │",
        ]),
        Card::OtherCommand
    );
}

#[test]
fn card_not_seen_when_choices_are_far() {
    // The block is the command line and the 12 lines after it.
    let build = |once_at: usize, deny_at: usize| {
        let mut lines: Vec<String> = vec!["$ rm -rf .pytest_cache".to_owned()];
        for i in 1..=14 {
            lines.push(match i {
                n if n == once_at => "pick once".to_owned(),
                n if n == deny_at => "or deny".to_owned(),
                n => format!("working on step {n}"),
            });
        }
        approval_card(&lines.join("\n"), COMMAND)
    };
    assert_eq!(
        build(11, 12),
        shown(None, false),
        "both words inside the block"
    );
    assert_eq!(
        build(12, 11),
        shown(None, false),
        "the twelfth line after is inside"
    );
    assert_eq!(
        build(11, 13),
        Card::Absent,
        "deny is the thirteenth line after"
    );
    assert_eq!(
        build(13, 12),
        Card::Absent,
        "once is the thirteenth line after"
    );
    assert_eq!(build(13, 14), Card::Absent, "both are outside");

    // numbered choices that are far from the command are not the card of that command either
    let mut lines: Vec<String> = vec!["$ rm -rf .pytest_cache".to_owned()];
    lines.extend((1..=13).map(|n| format!("working on step {n}")));
    lines.push("› 1. once".to_owned());
    lines.push("  2. deny".to_owned());
    assert_eq!(approval_card(&lines.join("\n"), COMMAND), Card::Absent);

    // only the last 25 visible lines are looked at
    let mut old: Vec<String> = vec![
        "$ rm -rf .pytest_cache".to_owned(),
        "› 1. once".to_owned(),
        "  2. deny".to_owned(),
    ];
    old.extend((1..=40).map(|n| format!("later line {n}")));
    assert_eq!(approval_card(&old.join("\n"), COMMAND), Card::Absent);
}

#[test]
fn choice_number_found_for_once() {
    assert_eq!(
        card(&["$ rm -rf .pytest_cache", "› 1. once", "  2. deny"]),
        shown(Some('1'), true)
    );
    assert_eq!(
        card(&["$ rm -rf .pytest_cache", "  1) once", "  2) deny"]),
        shown(Some('1'), false)
    );
    assert_eq!(
        card(&["$ rm -rf .pytest_cache", "  1: once", "  2: deny"]),
        shown(Some('1'), false)
    );
    assert_eq!(
        card(&["$ rm -rf .pytest_cache", "  1 once", "  2 deny"]),
        shown(Some('1'), false)
    );
    // the number is the one on the line that holds once, not always one
    assert_eq!(
        card(&[
            "$ rm -rf .pytest_cache",
            "  1. deny",
            "› 2. once",
            "  3. session"
        ]),
        shown(Some('2'), true)
    );
    assert_eq!(
        card(&[
            "$ rm -rf .pytest_cache",
            "  1. deny",
            "  2. session",
            "  3. once"
        ]),
        shown(Some('3'), false)
    );
    // inside box drawn rows
    assert_eq!(
        card(&[
            "│ $ rm -rf .pytest_cache │",
            "│ › 1. once │",
            "│   2. deny │"
        ]),
        shown(Some('1'), true)
    );
    // the first line that holds once decides
    assert_eq!(
        card(&[
            "$ rm -rf .pytest_cache",
            "  1. once",
            "  2. deny",
            "  3. once more"
        ]),
        shown(Some('1'), false)
    );
}

#[test]
fn choice_number_absent_gives_none() {
    // bracketed numbers are not read
    assert_eq!(
        card(&["$ rm -rf .pytest_cache", "  [1] once", "  [2] deny"]),
        shown(None, false)
    );
    // no numbers at all
    assert_eq!(
        card(&["$ rm -rf .pytest_cache", "  once", "  deny"]),
        shown(None, false)
    );
    assert_eq!(
        card(&[
            "$ rm -rf .pytest_cache",
            "Allow this command once?",
            "Or deny it."
        ]),
        shown(None, false)
    );
}

#[test]
fn highlighted_choice_is_found() {
    for marker in ["›", ">", "❯"] {
        let once = format!("{marker} 1. once");
        assert_eq!(
            card(&["$ rm -rf .pytest_cache", &once, "  2. deny"]),
            shown(Some('1'), true),
            "marker {marker}"
        );
        let deny = format!("{marker} 2. deny");
        assert_eq!(
            card(&["$ rm -rf .pytest_cache", "  1. once", &deny]),
            shown(Some('1'), false),
            "marker {marker}"
        );
    }
    // the highlight on another choice
    assert_eq!(
        card(&[
            "$ rm -rf .pytest_cache",
            "  1. once",
            "› 2. deny",
            "  3. session"
        ]),
        shown(Some('1'), false)
    );
    // no highlight at all
    assert_eq!(
        card(&["$ rm -rf .pytest_cache", "  1. once", "  2. deny"]),
        shown(Some('1'), false)
    );
    // the highlight inside a box
    assert_eq!(
        card(&[
            "│ $ rm -rf .pytest_cache │",
            "│   1. once │",
            "│ ❯ 2. deny │"
        ]),
        shown(Some('1'), false)
    );
    // with two command lines the last one is used
    assert_eq!(
        card(&[
            "$ rm -rf .pytest_cache",
            "  1. once",
            "› 2. deny",
            "an older card was withdrawn",
            "$ rm -rf .pytest_cache",
            "› 1. once",
            "  2. deny",
        ]),
        shown(Some('1'), true)
    );
}

// ---------------------------------------------------------------- counting lines

#[test]
fn counting_lines_and_highest_number() {
    let screen = [
        "› Count from 1 to 2000, one number per line, and do not stop early.",
        "1",
        "2.",
        "- 12",
        "  7)",
        "1500",
        "maple-4821",
        "0",
        "2001",
        "12345",
        "12 apples",
        "the answer is 9",
    ]
    .join("\n");
    assert_eq!(
        sorted(counting_values(&screen, COUNT_PROMPT)),
        vec![1, 2, 7, 12, 1500]
    );
    assert_eq!(highest_number(&screen, COUNT_PROMPT), Some(1500));

    // the edges of the range
    let edges = ["Count from 1 to 2000", "1", "2000", "2001", "0", "-1500"].join("\n");
    assert_eq!(
        sorted(counting_values(&edges, COUNT_PROMPT)),
        vec![1, 1500, 2000]
    );
    assert_eq!(highest_number(&edges, COUNT_PROMPT), Some(2000));

    // a reply with no counting line
    let none = [
        "Count from 1 to 2000",
        "maple-4821",
        "0",
        "I will count now.",
    ]
    .join("\n");
    assert!(counting_values(&none, COUNT_PROMPT).is_empty());
    assert_eq!(highest_number(&none, COUNT_PROMPT), None);
    assert_eq!(highest_number("", COUNT_PROMPT), None);
}

#[test]
fn counting_ignores_replies_and_earlier_text() {
    // lines before the last echo of the counting prompt are ignored
    let screen = [
        "3",
        "4",
        "› Count from 1 to 2000, one number per line, and do not stop early.",
        "900",
        "› Count from 1 to 2000, one number per line, and do not stop early.",
        "1",
        "2",
        "maple-4821",
        "0",
        "3",
    ]
    .join("\n");
    assert_eq!(
        sorted(counting_values(&screen, COUNT_PROMPT)),
        vec![1, 2, 3]
    );
    assert_eq!(highest_number(&screen, COUNT_PROMPT), Some(3));

    // an earlier answer to another prompt does not count
    let earlier = [
        "reply to the first prompt",
        "maple-4821",
        "7",
        "Count from 1 to 2000",
    ]
    .join("\n");
    assert!(counting_values(&earlier, COUNT_PROMPT).is_empty());
    assert_eq!(highest_number(&earlier, COUNT_PROMPT), None);

    // when the echo has scrolled off the screen the whole text is used
    let scrolled = ["41", "42", "43", "maple-4821", "0"].join("\n");
    assert_eq!(
        sorted(counting_values(&scrolled, COUNT_PROMPT)),
        vec![41, 42, 43]
    );
    assert_eq!(highest_number(&scrolled, COUNT_PROMPT), Some(43));
}

// ---------------------------------------------------------------- exit marker

#[test]
fn exit_marker_ignores_echo() {
    let echo =
        "$ 'talaria' --no-ensure --profile fixture --model m --provider p; echo SMOKE_EXIT:$?";
    assert_eq!(exit_code(echo), None);
    assert_eq!(exit_code(""), None);
    assert_eq!(exit_code("some output\nmore output"), None);

    let done = format!("{echo}\nSMOKE_EXIT:0\n$ ");
    assert_eq!(exit_code(&done), Some(0));

    let failed = format!("{echo}\nstub: no gateway\nSMOKE_EXIT:7\n$ ");
    assert_eq!(exit_code(&failed), Some(7));

    assert_eq!(exit_code("SMOKE_EXIT:130"), Some(130));

    // the last marker wins
    assert_eq!(
        exit_code(&format!("{echo}\nSMOKE_EXIT:1\n{echo}\nSMOKE_EXIT:0\n")),
        Some(0)
    );
    assert_eq!(
        exit_code(&format!("{echo}\nSMOKE_EXIT:0\n{echo}\nSMOKE_EXIT:3\n")),
        Some(3)
    );
    // an echo after a marker does not hide it
    assert_eq!(exit_code(&format!("SMOKE_EXIT:2\n{echo}\n")), Some(2));
}

// ---------------------------------------------------------------- settled

/// A terminal that returns scripted `visible` reads and records what was asked.
struct ScriptedReads {
    reads: RefCell<VecDeque<Result<String, TermError>>>,
    sources: RefCell<Vec<ReadSource>>,
}

impl ScriptedReads {
    /// The reads in order; the last one repeats once the script runs out.
    fn new(reads: Vec<Result<String, TermError>>) -> Self {
        Self {
            reads: RefCell::new(reads.into()),
            sources: RefCell::new(Vec::new()),
        }
    }

    fn text(first: &str, then: &[(usize, &str)]) -> Self {
        // `then` lists (read index, text) switches; every read is `first` until its index.
        let last = then.iter().map(|(i, _)| *i).max().unwrap_or(0);
        let mut reads = Vec::new();
        for i in 0..=last {
            let text = then
                .iter()
                .rfind(|(at, _)| *at <= i)
                .map_or(first, |(_, t)| t);
            reads.push(Ok(text.to_owned()));
        }
        Self::new(reads)
    }
}

impl Terminal for ScriptedReads {
    fn create_tab(&self, _label: &str, _cwd: &Path) -> Result<NewTab, TermError> {
        unreachable!("settled only reads")
    }
    fn split_pane(&self, _pane: &PaneId, _cwd: &Path) -> Result<PaneId, TermError> {
        unreachable!("settled only reads")
    }
    fn run(&self, _pane: &PaneId, _text: &str) -> Result<(), TermError> {
        unreachable!("settled only reads")
    }
    fn send_text(&self, _pane: &PaneId, _text: &str) -> Result<(), TermError> {
        unreachable!("settled only reads")
    }
    fn send_keys(&self, _pane: &PaneId, _keys: &[&str]) -> Result<(), TermError> {
        unreachable!("settled only reads")
    }
    fn read(
        &self,
        _pane: &PaneId,
        source: ReadSource,
        _lines: Option<u32>,
    ) -> Result<String, TermError> {
        self.sources.borrow_mut().push(source);
        let mut reads = self.reads.borrow_mut();
        if reads.len() > 1 {
            reads.pop_front().expect("one read left")
        } else {
            reads.front().cloned().expect("a repeating last read")
        }
    }
    fn foreground(&self, _pane: &PaneId) -> Result<Vec<String>, TermError> {
        unreachable!("settled only reads")
    }
    fn tab_exists(&self, _tab: &TabId) -> Result<bool, TermError> {
        unreachable!("settled only reads")
    }
    fn close_tab(&self, _tab: &TabId) -> Result<(), TermError> {
        unreachable!("settled only reads")
    }
    fn version(&self) -> String {
        "scripted".to_owned()
    }
}

/// A clock that only counts: `sleep` moves time on and remembers each sleep.
#[derive(Default)]
struct CountingClock {
    now: Cell<Duration>,
    sleeps: RefCell<Vec<Duration>>,
}

impl Clock for CountingClock {
    fn now(&self) -> Duration {
        self.now.get()
    }
    fn sleep(&self, d: Duration) {
        self.now.set(self.now.get() + d);
        self.sleeps.borrow_mut().push(d);
    }
}

#[test]
fn settled_needs_identical_reads() {
    let pane = PaneId("w1:p1".to_owned());
    let quiet = Duration::from_secs(3);

    // a still screen over the whole quiet period
    let term = ScriptedReads::new(vec![Ok("maple-4821\n› ".to_owned())]);
    let clock = CountingClock::default();
    assert_eq!(settled(&term, &clock, &pane, quiet), Ok(true));
    assert!(clock.now() >= quiet, "waited only {:?}", clock.now());
    assert!(
        clock
            .sleeps
            .borrow()
            .iter()
            .all(|d| *d == Duration::from_millis(250)),
        "sleeps: {:?}",
        clock.sleeps.borrow()
    );
    assert!(
        term.sources
            .borrow()
            .iter()
            .all(|s| *s == ReadSource::Visible),
        "settled reads the visible text"
    );

    // a screen that changes in the middle of the period
    let term = ScriptedReads::text("thinking .", &[(6, "thinking ..")]);
    let clock = CountingClock::default();
    assert_eq!(settled(&term, &clock, &pane, quiet), Ok(false));

    // a screen that changes between the first two reads
    let term = ScriptedReads::text("a", &[(1, "b")]);
    let clock = CountingClock::default();
    assert_eq!(settled(&term, &clock, &pane, quiet), Ok(false));

    // a screen that flips back to its first text is still not identical
    let term = ScriptedReads::text("a", &[(3, "b"), (4, "a")]);
    let clock = CountingClock::default();
    assert_eq!(settled(&term, &clock, &pane, quiet), Ok(false));

    // a shorter quiet period is shorter
    let term = ScriptedReads::new(vec![Ok("still".to_owned())]);
    let clock = CountingClock::default();
    assert_eq!(
        settled(&term, &clock, &pane, Duration::from_secs(1)),
        Ok(true)
    );
    assert!(clock.now() >= Duration::from_secs(1));
    assert!(
        clock.now() < quiet,
        "waited {:?} for a 1 s period",
        clock.now()
    );

    // a failed read is an error, not a verdict
    let term = ScriptedReads::new(vec![
        Ok("a".to_owned()),
        Err(TermError::Io("herdr went away".to_owned())),
    ]);
    let clock = CountingClock::default();
    assert_eq!(
        settled(&term, &clock, &pane, quiet),
        Err(TermError::Io("herdr went away".to_owned()))
    );
}
