//! Pure helpers over screen text: tokens, counts, the approval card, counting lines.

use std::time::Duration;

use rand::Rng;

use crate::clock::Clock;
use crate::terminal::{PaneId, TermError, Terminal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub word: &'static str,
    pub number: u32,
}

impl Token {
    /// A fresh token that is not in `used`.
    pub fn new(rng: &mut impl Rng, used: &[Token]) -> Token {
        let _ = (rng, used);
        todo!("smoke")
    }

    /// `word-number`.
    pub fn join(&self) -> String {
        todo!("smoke")
    }

    /// The prompt that asks for the joined form.
    pub fn prompt(&self) -> String {
        todo!("smoke")
    }
}

/// Case-insensitive whole-word count.
pub fn count_token(text: &str, token: &str) -> usize {
    let _ = (text, token);
    todo!("smoke")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Card {
    Absent,
    OtherCommand,
    Shown {
        once_digit: Option<char>,
        once_highlighted: bool,
    },
}

/// Looks at the last 25 visible lines.
pub fn approval_card(visible: &str, command: &str) -> Card {
    let _ = (visible, command);
    todo!("smoke")
}

/// Counting lines after the last echo of the counting prompt.
pub fn counting_values(visible: &str, after_prompt: &str) -> Vec<u32> {
    let _ = (visible, after_prompt);
    todo!("smoke")
}

pub fn highest_number(visible: &str, after_prompt: &str) -> Option<u32> {
    let _ = (visible, after_prompt);
    todo!("smoke")
}

/// The last `SMOKE_EXIT:<n>` that is not an echo of the command.
pub fn exit_code(text: &str) -> Option<i32> {
    let _ = text;
    todo!("smoke")
}

/// True when every read over `quiet` is identical.
pub fn settled(
    term: &dyn Terminal,
    clock: &dyn Clock,
    pane: &PaneId,
    quiet: Duration,
) -> Result<bool, TermError> {
    let _ = (term, clock, pane, quiet);
    todo!("smoke")
}
