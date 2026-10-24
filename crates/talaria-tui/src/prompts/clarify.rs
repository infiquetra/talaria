//! The clarification card.

/// The compact JSON array string of ticked labels in choice order, then the other text.
pub fn multi_select_answer(choices: &[String], ticked: &[usize], other: Option<&str>) -> String {
    let _ = (choices, ticked, other);
    todo!("prompts")
}
