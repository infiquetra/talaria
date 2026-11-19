pub mod acp_cmd;
pub mod args;
pub mod attach_cmd;
pub mod bench_replay;
pub mod bg;
pub mod capabilities_cmd;
pub mod configure_cmd;
pub mod connectors_wiring;
pub mod doctor_cmd;
pub mod fleet_cmd;
pub mod herdr_cmd;
pub mod mcp_wiring;
pub mod picker;
pub mod print;
pub mod profiles_cmd;
pub mod providers_wiring;
pub mod remote;
pub mod run;
pub mod workflow;
pub mod workflow_roster;
pub mod workflow_skill;

/// Show untrusted text at the terminal without letting it change the screen.
///
/// Newlines and tabs stay so multi-line diagnostics remain readable. Carriage
/// return, escapes, and the other controls `talaria_core::text::sanitize`
/// replaces become visible stand-ins. Callers keep the original values.
pub(crate) fn terminal_text(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            clean.push('\n');
        }
        for (field, part) in line.split('\t').enumerate() {
            if field > 0 {
                clean.push('\t');
            }
            clean.push_str(&talaria_core::text::sanitize(part));
        }
    }
    clean
}
