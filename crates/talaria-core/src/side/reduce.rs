//! Building the side events a submit produces.

use crate::session::Event;

/// Turn a side command argument into the event to reduce.
pub fn btw_command(arg: &str, now_ms: u64) -> Event {
    let _ = (arg, now_ms);
    todo!("#225")
}
