//! The clarification card.

/// The compact JSON array string of ticked labels in choice order, then the other text.
pub fn multi_select_answer(choices: &[String], ticked: &[usize], other: Option<&str>) -> String {
    let mut values: Vec<&str> = choices
        .iter()
        .enumerate()
        .filter(|(index, _)| ticked.contains(index))
        .map(|(_, label)| label.as_str())
        .collect();
    if let Some(other) = other.filter(|value| !value.trim().is_empty()) {
        values.push(other);
    }
    if values.is_empty() {
        return String::new();
    }
    let mut result = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            result.push(',');
        }
        result.push('"');
        for ch in value.chars() {
            match ch {
                '"' => result.push_str("\\\""),
                '\\' => result.push_str("\\\\"),
                ch if ch < ' ' => result.push_str(&format!("\\u{:04x}", ch as u32)),
                ch => result.push(ch),
            }
        }
        result.push('"');
    }
    result.push(']');
    result
}
