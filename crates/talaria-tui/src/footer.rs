//! Keep the recovery action visible when the secondary hints do not fit.

use crate::tool_cell::{cut_to, text_width};

const SEPARATOR: &str = " · ";

/// Keep the final hint while retaining as many complete leading hints as fit.
pub(crate) fn with_recovery(hints: &str, width: u16) -> String {
    let width = usize::from(width);
    if text_width(hints) <= width {
        return hints.to_owned();
    }
    let Some((actions, recovery)) = hints.rsplit_once(SEPARATOR) else {
        return cut_to(hints, width);
    };
    if text_width(recovery) > width {
        return cut_to(recovery, width);
    }

    let mut result = String::new();
    let mut left = width - text_width(recovery);
    for action in actions.split(SEPARATOR) {
        let needed = text_width(action) + text_width(SEPARATOR);
        if needed > left {
            break;
        }
        result.push_str(action);
        result.push_str(SEPARATOR);
        left -= needed;
    }
    result.push_str(recovery);
    result
}
