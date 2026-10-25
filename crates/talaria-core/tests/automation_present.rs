//! Cleaning Hermes's command replies for display. Behavior contract: issue 236.

use serde_json::json;
use talaria_core::automation::{Presentation, canonical_name, clean_command_text, present_command};
use talaria_core::commands::{
    CatalogResult, CommandCatalog, CommandOutcome, interpret_result, parse_slash_line,
};

fn catalog() -> CommandCatalog {
    let result: CatalogResult = serde_json::from_value(json!({
        "pairs": [
            ["/yolo", "Bypass approvals (usage: /yolo)"],
            ["/loop", "Run a recurring prompt (usage: /loop [interval] <prompt>)"],
            ["/heartbeat", "Run a periodic check (usage: /heartbeat [interval] [prompt])"],
            ["/help", "Show available commands (usage: /help [skills|<filter>])"],
        ],
        "canon": {"/proactive": "/loop", "/hb": "/heartbeat"},
        "commands": {
            "/yolo": {"argument_mode": "text"},
            "/loop": {"argument_mode": "text"},
            "/heartbeat": {"argument_mode": "text"},
            "/help": {"argument_mode": "text"},
        },
        "categories": [],
        "skills": {},
    }))
    .expect("the catalogue decodes");
    CommandCatalog::from_result(&result)
}

fn outcome(typed: &str, result: serde_json::Value) -> CommandOutcome {
    let line = parse_slash_line(typed).expect("a slash line");
    interpret_result(&line, typed, &result)
}

#[test]
fn presented_commands_and_aliases() {
    let catalog = catalog();
    assert_eq!(
        canonical_name("/yolo", Some(&catalog)),
        Some("yolo".to_owned())
    );
    assert_eq!(
        canonical_name("/proactive 45m x", Some(&catalog)),
        Some("loop".to_owned())
    );
    assert_eq!(
        canonical_name("/hb status", Some(&catalog)),
        Some("heartbeat".to_owned())
    );
    assert_eq!(canonical_name("yolo", Some(&catalog)), None);
    assert_eq!(canonical_name("/YOLO", None), Some("yolo".to_owned()));

    let finished = outcome("/help", json!({"type": "exec", "output": "topics"}));
    assert_eq!(present_command("help", "/help", &finished), None);
    let send = outcome(
        "/help x",
        json!({"type": "send", "message": "m", "notice": "n"}),
    );
    assert_eq!(present_command("help", "/help x", &send), None);
}

#[test]
fn output_and_submit_presentations() {
    let on = outcome(
        "/yolo",
        json!({"type": "exec", "output": "⚡ YOLO mode **ON** for this session — all commands auto-approved. Use with caution."}),
    );
    let Presentation::Output(cell) = present_command("yolo", "/yolo", &on).expect("a cell") else {
        panic!("an exec reply presents output");
    };
    assert_eq!(cell.typed, "/yolo");
    assert!(!cell.warning);
    assert!(cell.body.contains("**ON**"));

    let off = outcome(
        "/yolo",
        json!({"type": "exec", "output": "⚠️ YOLO mode **OFF** for this session — dangerous commands will require approval."}),
    );
    let Presentation::Output(cell) = present_command("yolo", "/yolo", &off).expect("a cell") else {
        panic!("an exec reply presents output");
    };
    assert!(cell.warning);
    assert!(cell.body.starts_with("⚠ YOLO mode"));

    let fast = outcome(
        "/fast",
        json!({"type": "exec", "output": "⚡ /fast is only available for OpenAI models that support Priority Processing."}),
    );
    assert!(matches!(
        present_command("fast", "/fast", &fast),
        Some(Presentation::Output(_))
    ));

    let missing = outcome(
        "/loop 45m",
        json!({"type": "exec", "output": "/loop: missing prompt (usage: /loop [interval] <prompt>)", "notice": "usage"}),
    );
    let Presentation::Output(cell) =
        present_command("loop", "/loop 45m", &missing).expect("a cell")
    else {
        panic!("an exec reply presents output");
    };
    assert!(cell.body.contains("\\<prompt\\>"), "{}", cell.body);

    let goal = outcome(
        "/goal x",
        json!({"type": "send", "message": "message", "notice": "n"}),
    );
    let Presentation::Submit {
        cell,
        text,
        display,
    } = present_command("goal", "/goal x", &goal).expect("a submit")
    else {
        panic!("a send reply presents a submit");
    };
    assert!(cell.is_some());
    assert_eq!(text, "message");
    assert_eq!(display, "/goal x");

    let plan = outcome(
        "/plan x",
        json!({"type": "send", "message": "message", "notice": "Planning: x"}),
    );
    assert!(matches!(
        present_command("plan", "/plan x", &plan),
        Some(Presentation::Submit { .. })
    ));

    let refused = CommandOutcome::Refused {
        name: "yolo".to_owned(),
        message: "no".to_owned(),
        dim: false,
    };
    assert_eq!(present_command("yolo", "/yolo", &refused), None);

    let blank = outcome("/yolo", json!({"type": "exec", "output": "   \u{fe0f}  "}));
    assert_eq!(present_command("yolo", "/yolo", &blank), None);

    let empty = outcome("/yolo", json!({"type": "exec", "output": "(no output)"}));
    assert!(matches!(
        present_command("yolo", "/yolo", &empty),
        Some(Presentation::Output(_))
    ));
}

#[test]
fn clean_command_text_rules() {
    assert_eq!(clean_command_text("⚠️ YOLO"), "⚠ YOLO");
    assert_eq!(clean_command_text("a\r\nb\rc"), "a\nb\nc");
    assert_eq!(clean_command_text("body\n\n  \n"), "body");
    assert_eq!(clean_command_text("<prompt>"), "\\<prompt\\>");
    assert_eq!(
        clean_command_text("`/fast <normal|fast>`"),
        "`/fast <normal|fast>`"
    );
    assert_eq!(clean_command_text("``a <b> c``"), "``a <b> c``");
    assert_eq!(clean_command_text("`x <y>"), "`x \\<y\\>");
    assert_eq!(clean_command_text("`a` <b> `c`"), "`a` \\<b\\> `c`");
    assert_eq!(clean_command_text("\x1b[2J"), "\x1b[2J");
    assert_eq!(clean_command_text("**ON** _off_"), "**ON** _off_");
    assert_eq!(clean_command_text("\u{fe0f}"), "");
    assert_eq!(clean_command_text("   \u{fe0f}  "), "     ");
}
