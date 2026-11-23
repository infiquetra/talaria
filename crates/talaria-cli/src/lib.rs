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
pub mod workflow_worktrees;

/// Show untrusted text at the terminal without letting it change the screen.
///
/// Newlines stay so multi-line diagnostics remain readable. Tabs and the
/// other controls `talaria_core::text::sanitize` replaces follow that
/// function. Invisible and blank-width characters outside its table become
/// `‹U+XXXX›`. Callers keep the original values.
pub(crate) fn terminal_text(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            clean.push('\n');
        }
        let sanitized = talaria_core::text::sanitize(line);
        push_visible(&mut clean, &sanitized);
    }
    clean
}

fn push_visible(out: &mut String, text: &str) {
    let Some(first) = text
        .char_indices()
        .find(|(_, character)| hidden_code(*character).is_some())
    else {
        out.push_str(text);
        return;
    };
    out.push_str(&text[..first.0]);
    for character in text[first.0..].chars() {
        match hidden_code(character) {
            Some(code) => out.push_str(&format!("\u{2039}U+{code:04X}\u{203a}")),
            None => out.push(character),
        }
    }
}

/// Invisible and blank-width characters outside the shared sanitizer table.
fn hidden_code(character: char) -> Option<u32> {
    let code = u32::from(character);
    let hidden = matches!(
        code,
        0x00ad
            | 0x034f
            | 0x061c
            | 0x115f
            | 0x1160
            | 0x17b4..=0x17b5
            | 0x180b..=0x180f
            | 0x200b..=0x200f
            | 0x202a..=0x202e
            | 0x2060..=0x206f
            | 0x3164
            | 0xfe00..=0xfe0d
            | 0xfeff
            | 0xffa0
            | 0xe0000..=0xe007f
            | 0xe0100..=0xe01ef
            | 0x00a0
            | 0x2000..=0x200a
            | 0x202f
            | 0x205f
            | 0x2800
            | 0x3000
    );
    hidden.then_some(code)
}
