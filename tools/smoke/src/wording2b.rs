//! The Talaria texts the phase 2b steps depend on.
//!
//! Every line is copied from the final body of its accepted issue. Each group names its
//! source as the issue number and the scenario title.

// #234, scenario "Nothing to measure before the first prompt"
/// The context header and its hint before the first prompt.
pub const CTX_EMPTY: &[&str] = &[
    "Context · not measured yet",
    "Hermes builds this session's agent on the next prompt; until then there is nothing to break down.",
];
// #234, scenario "Nothing to measure before the first prompt"
/// The usage block before the first prompt.
pub const USAGE_EMPTY: &str = "No API calls yet in this session.";

// #237, scenario "see the setting and switch it for this session"
/// The status segment while the profile mode rules.
pub const APPROVALS_MANUAL: &str = "approvals: manual";
// #237, scenario "see the setting and switch it for this session"
/// The status segment while the session bypass rules.
pub const APPROVALS_OFF: &str = "approvals: off (this session)";
// #237, scenario "see the setting and switch it for this session"
/// The start of the notice Talaria prints when the bypass switches on.
pub const NOTICE_OFF: &str = "Approvals off for this session only";

// #230, scenario "see every provider and model"
/// The picker title.
pub const PICKER_TITLE: &str = "Switch model · this session";
// #230, scenario "see every provider and model"
/// The provider group that carries the current marker.
pub const PICKER_GROUP: &str = "Ollama Cloud";
// #230, scenario "see every provider and model"
/// The selected row of the current model.
pub const PICKER_CURRENT_ROW: &str = "● minimax-m3.1-flash";
// #230, scenario "see every provider and model"
/// The start of the group header for providers that are not set up.
pub const PICKER_NOT_SET_UP: &str = "Not set up (";
// #230, scenario "see every provider and model"
/// The start of the effort footer.
pub const PICKER_EFFORT: &str = "effort:";
// #230, scenario "see every provider and model"
/// The empty-picker line for a query that matches nothing.
pub const PICKER_NO_MATCH: &str = "No model matches \"zzzz\".";

// #230, scenario "an unknown model and bad input are refused"
/// The refusal for an unknown model word.
pub const MODEL_REFUSAL: &str = "Hermes could not switch to no-such-model-xyz: it does not know that model on Ollama Cloud, or the provider is not set up. Nothing changed.";

// #233, scenario "Rewind the conversation with Esc twice"
/// The rewind overlay title.
pub const REWIND_TITLE: &str = "Rewind · choose a prompt to go back to";
// #233, scenario "Rewind the conversation with Esc twice"
/// The receipt prefix; the prompt number follows.
pub const REWOUND: &str = "── Rewound to before prompt #";

// #232, scenario "Negative: steer with no running turn, and an empty edit"
/// The start of the steer notice while nothing runs.
pub const STEER_IDLE: &str = "No turn is running, so this was sent as a new prompt";

// #234, scenario "/context shows what fills the window"
/// The context header starts with the first line and holds the second with a percent.
pub const CTX_HEADER: &[&str] = &["Context · minimax-m3.1-flash · ", "tokens ("];
// #234, scenario "/context shows what fills the window"
/// The three category rows under the context header.
pub const CTX_ROWS: &[&str] = &["Estimated usage by category", "Free space", "Source: "];
// #234, scenario "/usage and /cost show the session's figures"
/// The usage block title.
pub const USAGE_TITLE: &str = "Usage · this session";
// #234, scenario "/usage and /cost show the session's figures"
/// The two rows the usage block must hold.
pub const USAGE_ROWS: &[&str] = &["Tokens", "Cost"];

// #234, scenario "Preview, then compact a long conversation"
/// The compaction receipt, which must never appear here.
pub const COMPRESSED: &str = "✓ Compressed";
// #234, scenario "Refusals change nothing"
/// The refusal for the aggressive compact flag.
pub const COMPACT_REFUSAL: &str = "Hermes does not support --aggressive. Nothing was changed.";
// #234, scenario "Refusals change nothing"
/// The refusal for an argument to /context.
pub const CTX_ARG: &str = "/context does not take an argument.";

// #236, scenario "`/fast` on a model without priority processing changes nothing"
/// The refusal for /fast on a model without priority processing.
pub const FAST_REFUSAL: &str =
    "⚡ /fast is only available for OpenAI models that support Priority Processing.";
// #236, scenario "A loop runs, then counts down without needless redraws"
/// The usage line for /loop with no prompt.
pub const LOOP_USAGE: &str = "/loop: missing prompt (usage: /loop [interval] <prompt>)";
// #236, scenario "A loop runs, then counts down without needless redraws"
/// The start of a running loop's countdown line, which must never appear here.
pub const LOOP_LINE: &str = "↻ loop";

// #231, scenario "Rename from the list and with /rename"
/// The rename receipt; `{}` is the new title.
pub const RENAMED: &str = "Renamed to \"{}\".";
// #231, scenario "Rename from the list and with /rename"
/// The /rename answer; `{}` is the current title.
pub const RENAME_SHOW: &str =
    "This session is called \"{}\". Type /rename <new title> to change it.";
// #231, scenario "Export to Markdown"
/// The export receipt starts with this; `{}` are the title and the file name, then the size.
pub const EXPORTED: &str = "Exported \"{}\" to ./{} (";
// #231, scenario "Export to Markdown"
/// The export receipt ends with the size, then this.
pub const EXPORTED_REDACTED: &str = "Markdown, secrets redacted";
// #231, scenario "Cancelled and refused exports write nothing"
/// The usage line for /export with a bad name.
pub const EXPORT_BAD: &str = "Use a file name ending in .md, .json or .html.";

// #232, scenario "Queue while the agent works, then watch it run in order"
/// The queued indicator starts with this; the count and `)` follow.
pub const QUEUE_INDICATOR: &str = "queued (";
// #232, scenario "Queue while the agent works, then watch it run in order"
/// The queued prompt the step reads back.
pub const QUEUE_PREVIEW: &str = "Reply with the word omega";
// #232, scenario "Queue while the agent works, then watch it run in order"
/// The prefix of a submitted prompt line in the unwrapped recent text.
pub const USER_LINE_PREFIX: &str = "> ";

// #237, scenario "move the session with /cd"
/// The moving line starts with this; the path follows.
pub const CD_MOVING: &str = "Moving this session to";
// #237, scenario "move the session with /cd"
/// The moved receipt starts with this; the path follows.
pub const CD_DONE: &str = "This session now works in";
// #237, scenario "move the session with /cd"
/// The /cd answer starts with this; the path follows.
pub const CD_SHOW: &str = "This session works in";
// #237, scenario "move the session with /cd"
/// The missing-folder refusal starts with this; the path follows.
pub const CD_MISSING: &str = "No folder at";

// #235, scenario "A plain answer in a pipeline"
/// The printed answer of the headless run.
pub const PRINT_OK: &str = "5555";
// #235, scenario "No gateway and bad input give clean failures"
/// The two facts of the no-gateway JSON.
pub const NOT_STARTED: &[&str] = &["\"status\":\"not_started\"", "\"exit_code\":4"];
// #235, scenario "No gateway and bad input give clean failures"
/// The start of the no-gateway standard error.
pub const NO_GATEWAY: &str = "talaria: no Hermes gateway is running for this home";
