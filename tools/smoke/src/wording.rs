//! The exact Talaria wordings the phase 2a steps depend on, each with its source.
//!
//! Sources cite the issue number and the scenario title; the texts were transcribed from the
//! issues' final bodies. A step that needs a text no source states is changed to need less,
//! or the gap is reported on the source issue.

/// The tool shell header prefix (issue #221, shell output rows scenario).
pub const SHELL_HEADER: &str = "⏺ Shell(";

/// The failure mark of a tool shell header (issue #221, shell output rows scenario).
pub const SHELL_EXIT_MARK: &str = "✗ exit ";

/// The output row of a missing folder (issue #221, shell output rows scenario).
pub const SHELL_NO_SUCH_DIR: &str = "No such file or directory";

/// The header starts of a file-edit tool row (issue #221, numbered diff scenario).
pub const EDIT_HEADERS: &[&str] = &["⏺ Edit", "⏺ Write"];

/// The stat fragment of a one-line edit (issue #221, numbered diff scenario).
pub const EDIT_STAT: &str = "✓ +1 -1";

/// The numbered `+`/`-` rows of the scenario's diff (issue #221, numbered diff scenario).
pub const DIFF_ROWS: &[&str] = &["2 - beta", "2 + gamma"];

/// Fragments a diff must never hold (issue #221, numbered diff scenario).
pub const DEBRIS: &[&str] = &["┊", "\u{1b}", "[38;2"];

/// The palette footer fragment (issue #223, palette layout scenario).
pub const PALETTE_FOOTER: &str = "Tab complete";

/// The palette group of Talaria's own commands (issue #223, palette layout scenario).
pub const PALETTE_GROUP: &str = "Talaria";

/// The tag of Talaria's own commands (issue #223, palette layout scenario).
pub const PALETTE_TAG: &str = "talaria";

/// The four local commands the Talaria group must list (issue #223, palette layout scenario).
pub const LOCAL_COMMANDS: &[&str] = &["/quit", "/btw", "/skin", "/theme"];

/// The file the mention scenario lists (issue #224, mention completion scenario).
pub const MENTION_ROW: &str = "calc.py";

/// The folder beside the mention row (issue #224, mention completion scenario).
pub const MENTION_FOLDER: &str = "src";

/// What Tab inserts for the mention (issue #224, mention completion scenario).
pub const MENTION_INSERT: &str = "@file:src/calc.py";

/// The mention list footer (issue #224, mention completion scenario).
pub const MENTION_FOOTER: &str = "tab complete · enter insert · esc close";

/// The collapsed-paste label: `[Pasted text #{number} +{lines} lines]`
/// (issue #224, large paste collapse scenario).
pub const PASTE_LABEL: &str = "[Pasted text #{} +{} lines]";

/// The dropped-file label: `[File #{number}: {name}]` (issue #224, file drop scenario).
pub const FILE_LABEL: &str = "[File #{}: {}]";

/// The shell cell header prefix (issue #225, shell cell scenario).
pub const SHELL_CELL: &str = "! ";

/// The refusal prefix of a dangerous command (issue #225, dangerous command scenario).
pub const SHELL_REFUSAL: &str = "Hermes refused to run this: it looks dangerous";

/// The side-answer header prefix (issue #225, side question scenario).
pub const SIDE_HEADER: &str = "btw · ";

/// The footer under a side answer (issue #225, side question scenario).
pub const SIDE_FOOTER: &str = "side answer · not part of the conversation";

/// The prefix of a failed side answer (issue #225, side question scenario).
pub const SIDE_FAILED: &str = "The side question failed:";

/// The empty side answer (issue #225, side question scenario).
pub const SIDE_EMPTY: &str = "(no answer)";

/// The row while the side answer is on its way (issue #225, side question scenario).
pub const SIDE_PENDING: &str = "answering";

/// The pager header prefix (issue #222, transcript layout scenario).
pub const PAGER_HEADER: &str = "Transcript ·";

/// The pager footer word while following (issue #222, transcript layout scenario).
pub const PAGER_FOLLOW: &str = "following";

/// The confirmed search footer: `"{query}" {k} of {n}` (issue #222, search scenario).
pub const PAGER_MATCH: &str = "\"{}\" {} of {}";

/// The matchless search footer prefix (issue #222, failed search scenario).
pub const PAGER_NOMATCH: &str = "no match for";

/// The lines of a theme report, in order (issue #227, theme report scenario).
pub const THEME_LINES: &[&str; 5] = &["Theme", "Skin:", "Background:", "Colours:", "Motion:"];

/// The usage line of a bad skin argument (issue #227, bad arguments scenario).
pub const USAGE_SKIN: &str = "Usage: /skin [name].";

/// The usage line of a bad theme argument (issue #227, bad arguments scenario).
pub const USAGE_THEME: &str = "Usage: /theme [light|dark|auto]";

/// The refusal fragment that must never appear after a bad argument
/// (issue #227, bad arguments scenario).
pub const REFUSED: &str = "Hermes refused";

/// The expanded task panel header prefix (issue #226, task panel scenario).
pub const TASK_EXPANDED: &str = "▾ Tasks";

/// The collapsed task panel header prefix (issue #226, task panel scenario).
pub const TASK_COLLAPSED: &str = "▸ Tasks";

/// The collapsed panel's hint (issue #226, task panel scenario).
pub const TASK_EXPAND_HINT: &str = "Ctrl+T to expand";

/// The mark of the in-progress task row (issue #226, task panel scenario).
pub const TASK_ACTIVE: &str = "[>] ";

/// The committed task record: `Tasks {completed}/{total} · ` (issue #226, record scenario).
pub const TASK_RECORD: &str = "Tasks {}/{} · ";

/// The resumed record's marker (issue #226, resume scenario).
pub const TASK_RESUMED: &str = "(from before this resume)";

/// The words a pane-state report must hold (issue #228, pane state scenario).
pub const AGENT_WORDS: &[&str] = &["talaria", "idle", "working"];

/// The resumed-history marker (issue #209, resume step).
pub const RESUMED_LINE: &str = "── Resumed:";

/// The gutter before a system line, such as a command's output or a notice (seen on the live
/// binary; issue #223 shows `/version` output this way).
pub const SYSTEM_GUTTER: &str = "·";

/// The history box's line when nothing matches (issue #228, history search scenario).
pub const HISTORY_NO_MATCH: &str = "No earlier prompt matches";
