//! Pure helpers over screen text: tokens, counts, the approval card, counting lines.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use rand::{Rng, RngExt};
use regex::Regex;

use crate::clock::Clock;
use crate::terminal::{PaneId, TermError, Terminal};

/// The words a reply token is built from.
const WORDS: [&str; 8] = [
    "maple", "cobalt", "juniper", "saffron", "tundra", "walnut", "harbor", "quartz",
];

/// How often the screen is read while waiting.
pub const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How many visible lines the card is looked for in.
const CARD_LINES: usize = 25;
/// How many lines after the command line belong to its block.
const BLOCK_LINES: usize = 12;

/// A number alone on its line: a list marker and a trailing dot are allowed.
const COUNTING_LINE: &str = r"(?m)^[^\p{L}\d\n]{0,6}(\d{1,4})[.)]?\s*$";
/// The exit marker the launch line prints.
const EXIT_MARKER: &str = r"SMOKE_EXIT:(\d+)";
/// The number of a numbered choice line, with the word it holds.
const CHOICE_LINE: &str = r"(?i)^\D{0,6}(\d)[.):]?\s+.*\b{}\b";

/// A random word and number that only a reply holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub word: &'static str,
    pub number: u32,
}

impl Token {
    /// A fresh token that is not in `used`.
    pub fn new(rng: &mut impl Rng, used: &[Token]) -> Token {
        loop {
            let word = WORDS[rng.random_range(0..WORDS.len())];
            let number = rng.random_range(1000..=9999);
            let token = Token { word, number };
            if !used.contains(&token) {
                return token;
            }
        }
    }

    /// `word-number`.
    pub fn join(&self) -> String {
        format!("{}-{}", self.word, self.number)
    }

    /// The prompt that asks for the joined form.
    pub fn prompt(&self) -> String {
        format!(
            "Without using any tools, join these two words with a hyphen and reply with only the \
             result: {} and {}",
            self.word, self.number
        )
    }
}

/// Case-insensitive whole-word count.
pub fn count_token(text: &str, token: &str) -> usize {
    let Some(pattern) = whole_word(token) else {
        return 0;
    };
    pattern.find_iter(text).count()
}

/// The text without the `── Resumed: <title> ──` lines a resumed session opens with.
///
/// Hermes titles a session with a model, and the title can be the joined token itself, so
/// the header would count as a second copy of the history.
pub fn without_resume_header(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let line = line.trim();
            !(line.starts_with("\u{2500}\u{2500} Resumed: ") && line.ends_with(" \u{2500}\u{2500}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A regular expression, kept so that a poll loop does not build it again.
fn cached(key: &str, pattern: &str) -> Option<Regex> {
    static CACHE: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(pattern) = cache.get(key) {
        return Some(pattern.clone());
    }
    let pattern = Regex::new(pattern).ok()?;
    cache.insert(key.to_owned(), pattern.clone());
    Some(pattern)
}

/// A cached regular expression that matches a whole word.
fn whole_word(word: &str) -> Option<Regex> {
    cached(
        &format!("word:{word}"),
        &format!(r"(?i)\b{}\b", regex::escape(word)),
    )
}

/// What the screen shows of an approval card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Card {
    /// No card is open.
    Absent,
    /// A card is open, but for another command.
    OtherCommand,
    /// The card of `command` is open.
    Shown {
        /// The number of the `once` choice, when the layout holds one.
        once_digit: Option<char>,
        /// Whether the first highlighted choice of the block is `once`.
        once_highlighted: bool,
    },
}

/// Looks at the last 25 visible lines.
pub fn approval_card(visible: &str, command: &str) -> Card {
    let lines = normalised_lines(visible);
    let command_at = lines
        .iter()
        .rposition(|line| is_command_line(line, command));

    let Some(at) = command_at else {
        // Without a command line of its own, a card for another command is still a card.
        let numbered_once = lines
            .iter()
            .any(|line| choice_number(line, "once").is_some());
        let numbered_deny = lines
            .iter()
            .any(|line| choice_number(line, "deny").is_some());
        return if numbered_once && numbered_deny {
            Card::OtherCommand
        } else {
            Card::Absent
        };
    };

    let block: Vec<&str> = lines[at..]
        .iter()
        .take(BLOCK_LINES + 1)
        .map(String::as_str)
        .collect();
    let holds_once = block.iter().any(|line| holds_word(line, "once"));
    let holds_deny = block.iter().any(|line| holds_word(line, "deny"));
    if !holds_once || !holds_deny {
        return Card::Absent;
    }

    let once_digit = block.iter().find_map(|line| choice_number(line, "once"));
    let once_highlighted = block
        .iter()
        .find(|line| is_highlighted(line))
        .is_some_and(|line| holds_word(line, "once"));
    Card::Shown {
        once_digit,
        once_highlighted,
    }
}

/// Counting lines after the last echo of the counting prompt.
pub fn counting_values(visible: &str, after_prompt: &str) -> Vec<u32> {
    let lines: Vec<&str> = visible.lines().collect();
    let start = lines
        .iter()
        .rposition(|line| line.contains(after_prompt))
        .map_or(0, |at| at + 1);
    let pattern =
        cached("counting", COUNTING_LINE).unwrap_or_else(|| Regex::new("$^").expect("never"));
    let mut values = Vec::new();
    for line in &lines[start..] {
        for found in pattern.captures_iter(line) {
            let Some(number) = found.get(1) else {
                continue;
            };
            let Ok(value) = number.as_str().parse::<u32>() else {
                continue;
            };
            if (1..=2000).contains(&value) {
                values.push(value);
            }
        }
    }
    values
}

/// The highest counting value on the screen.
pub fn highest_number(visible: &str, after_prompt: &str) -> Option<u32> {
    counting_values(visible, after_prompt).into_iter().max()
}

/// The last `SMOKE_EXIT:<n>` that is not an echo of the command.
pub fn exit_code(text: &str) -> Option<i32> {
    let pattern = cached("exit", EXIT_MARKER)?;
    pattern
        .captures_iter(text)
        .filter_map(|found| found.get(1))
        .filter_map(|number| number.as_str().parse::<i32>().ok())
        .last()
}

/// True when every read over `quiet` is identical.
pub fn settled(
    term: &dyn Terminal,
    clock: &dyn Clock,
    pane: &PaneId,
    quiet: Duration,
) -> Result<bool, TermError> {
    let mut previous: Option<String> = None;
    let mut elapsed = Duration::ZERO;
    loop {
        let current = term.read(pane, crate::terminal::ReadSource::Visible, None)?;
        if let Some(previous) = &previous {
            if previous != &current {
                return Ok(false);
            }
        } else {
            previous = Some(current);
        }
        if elapsed >= quiet {
            return Ok(true);
        }
        clock.sleep(POLL_INTERVAL);
        elapsed += POLL_INTERVAL;
    }
}

/// The last 25 lines, without box drawing and trimmed.
fn normalised_lines(visible: &str) -> Vec<String> {
    let lines: Vec<&str> = visible.lines().collect();
    let start = lines.len().saturating_sub(CARD_LINES);
    lines[start..]
        .iter()
        .map(|line| {
            let plain: String = line
                .chars()
                .filter(|character| !('\u{2500}'..='\u{257F}').contains(character))
                .collect();
            plain.trim().to_owned()
        })
        .collect()
}

/// A line that holds the command, with or without the dollar sign of the card.
///
/// Asked for the exit code, a model may append `; echo $?` or
/// `; echo "EXIT_CODE=$?"`; that suffix is accepted, any other chained command is not.
fn is_command_line(line: &str, command: &str) -> bool {
    let rest = line.strip_prefix("$ ").unwrap_or(line);
    if rest == command {
        return true;
    }
    rest.strip_prefix(command)
        .and_then(|suffix| suffix.strip_prefix("; echo "))
        .is_some_and(|echoed| {
            !echoed.contains("&&")
                && !echoed.contains('|')
                && !echoed.contains(';')
                && (echoed.contains("$?") || echoed.contains("EXIT_CODE"))
        })
}

/// The number of a numbered choice line, when it holds the given word.
fn choice_number(line: &str, word: &str) -> Option<char> {
    let pattern = cached(
        &format!("choice:{word}"),
        &CHOICE_LINE.replace("{}", &regex::escape(word)),
    )?;
    let found = pattern.captures(line)?;
    let number = found.get(1)?.as_str();
    number.chars().next()
}

/// Whether the line holds the whole word.
pub fn holds_word(line: &str, word: &str) -> bool {
    whole_word(word)
        .map(|pattern| pattern.is_match(line))
        .unwrap_or(false)
}

/// Whether the line is the highlighted choice.
fn is_highlighted(line: &str) -> bool {
    line.starts_with('›') || line.starts_with('>') || line.starts_with('❯')
}
