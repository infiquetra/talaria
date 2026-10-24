//! The approval card.

/// The offered choices in display order: once, session, always, deny, then unknown ones.
pub fn display_choices(offered: &[String]) -> Vec<&str> {
    let mut result = Vec::new();
    for known in ["once", "session", "always", "deny"] {
        if offered.iter().any(|choice| choice == known) {
            result.push(known);
        }
    }
    result.extend(
        offered
            .iter()
            .map(String::as_str)
            .filter(|choice| !matches!(*choice, "once" | "session" | "always" | "deny")),
    );
    result
}
