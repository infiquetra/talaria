use serde_json::json;
use talaria_core::configure::*;

fn block(body: &str) -> String {
    format!("```talaria-configure\n{body}\n```")
}

#[test]
fn extracts_exactly_one_block() {
    assert_eq!(parse_reply("hello"), Err(ReplyError::NoBlock));
    assert_eq!(parse_reply("```json\n{}\n```"), Err(ReplyError::NoBlock));
    assert_eq!(
        parse_reply("```talaria-configure\n{}"),
        Err(ReplyError::NoBlock)
    );
    let one = block(r#"{"kind":"answer","text":"done"}"#);
    assert_eq!(
        parse_reply(&format!("before\n{one}\nafter")),
        Ok(Reply::Answer("done".into()))
    );
    assert_eq!(
        prose_without_block(&format!("before\n{one}\nafter")),
        "before\nafter"
    );
    assert_eq!(
        parse_reply(
            " \t```talaria-configure \r\n{\"kind\":\"answer\",\"text\":\"ok\"}\r\n \t```\r\n"
        ),
        Ok(Reply::Answer("ok".into()))
    );
    assert_eq!(
        parse_reply(&format!("{one}\n{one}")),
        Err(ReplyError::SeveralBlocks(2))
    );
    let oversized = block(&"x".repeat(REPLY_BLOCK_MAX_BYTES + 1));
    assert!(matches!(
        parse_reply(&oversized),
        Err(ReplyError::TooLarge(_))
    ));
}

#[test]
fn parses_each_reply_kind() {
    assert_eq!(
        parse_reply(&block(r#"{"kind":"answer","text":"ok"}"#)),
        Ok(Reply::Answer("ok".into()))
    );
    assert_eq!(
        parse_reply(&block(r#"{"kind":"cannot","reason":"no"}"#)),
        Ok(Reply::Cannot("no".into()))
    );
    assert_eq!(
        parse_reply(&block(
            r#"{"kind":"need","sections":["skills"],"why":"inspect"}"#
        )),
        Ok(Reply::Need(Need {
            sections: vec!["skills".into()],
            why: Some("inspect".into())
        }))
    );
    assert!(matches!(
        parse_reply(&block(
            r#"{"kind":"proposal","summary":"ok","changes":[{"op":"set","key":"display.compact","value":true}]}"#
        )),
        Ok(Reply::Proposal(_))
    ));
    assert!(matches!(parse_reply(&block("{")), Err(ReplyError::Json(_))));
    assert_eq!(parse_reply(&block("[]")), Err(ReplyError::NotAnObject));
    assert_eq!(
        parse_reply(&block(r#"{"kind":"other"}"#)),
        Err(ReplyError::UnknownKind("other".into()))
    );
    for body in [
        r#"{}"#,
        r#"{"kind":"answer"}"#,
        r#"{"kind":"cannot","reason":1}"#,
        r#"{"kind":"need","sections":[1]}"#,
        r#"{"kind":"proposal","summary":"s","changes":false}"#,
    ] {
        assert!(
            matches!(parse_reply(&block(body)), Err(ReplyError::MissingField(_))),
            "{body}"
        );
    }
    let changes = vec![json!({"op":"open","screen":"settings"}); 21];
    let body = json!({"kind":"proposal","summary":"s","changes":changes}).to_string();
    assert_eq!(
        parse_reply(&block(&body)),
        Err(ReplyError::TooManyChanges(21))
    );
}

#[test]
fn parses_every_change_op() {
    let changes = json!([
        {"op":"set","key":"display.compact","value":true},
        {"op":"mcp_add","name":"a","preset":"deepwiki"},
        {"op":"mcp_add","name":"b","command":"run","args":["arg"]},
        {"op":"mcp_add","name":"c","url":"https://127.0.0.1","auth":"oauth"},
        {"op":"mcp_enable","name":"a","enabled":true}, {"op":"mcp_remove","name":"a"},
        {"op":"toggle","kind":"skill","name":"notes","enabled":false},
        {"op":"cron_add","name":"daily","schedule":"0 9 * * *","prompt":"hello","repeat":1},
        {"op":"cron_pause","job_id":"j1"}, {"op":"cron_resume","job_id":"j1"}, {"op":"cron_remove","job_id":"j1"},
        {"op":"main_model","provider":"ollama-cloud","model":"model"}, {"op":"open","screen":"settings"},
        {"op":"unknown"}, {"op":"mcp_enable","name":"a","enabled":"yes"},
        {"op":"mcp_add","name":"a","preset":"x","command":"run","args":[]},
        {"op":"mcp_add","name":"a","env":{"TOKEN":"sensitive"},"command":42}
    ]);
    let body = json!({"kind":"proposal","summary":"s","changes":changes}).to_string();
    let Ok(Reply::Proposal(p)) = parse_reply(&block(&body)) else {
        panic!("proposal expected")
    };
    assert!(matches!(&p.changes[0], Change::Set { .. }));
    assert!(matches!(
        &p.changes[1],
        Change::McpAdd {
            source: McpSource::Preset(_),
            ..
        }
    ));
    assert!(matches!(
        &p.changes[2],
        Change::McpAdd {
            source: McpSource::Command { .. },
            ..
        }
    ));
    assert!(matches!(
        &p.changes[3],
        Change::McpAdd {
            source: McpSource::Url { oauth: true, .. },
            ..
        }
    ));
    assert!(matches!(&p.changes[4], Change::McpEnable { .. }));
    assert!(matches!(&p.changes[5], Change::McpRemove { .. }));
    assert!(matches!(
        &p.changes[6],
        Change::Toggle {
            kind: CapKind::Skill,
            ..
        }
    ));
    assert!(matches!(&p.changes[7], Change::CronAdd { .. }));
    assert!(matches!(&p.changes[8], Change::CronPause { .. }));
    assert!(matches!(&p.changes[9], Change::CronResume { .. }));
    assert!(matches!(&p.changes[10], Change::CronRemove { .. }));
    assert!(matches!(&p.changes[11], Change::MainModel { .. }));
    assert!(matches!(
        &p.changes[12],
        Change::Open {
            screen: Screen::Settings,
            ..
        }
    ));
    assert!(matches!(&p.changes[13], Change::Unknown { .. }));
    assert!(matches!(
        &p.changes[14],
        Change::Malformed {
            field: "enabled",
            ..
        }
    ));
    assert!(matches!(
        &p.changes[15],
        Change::Malformed {
            field: "source",
            ..
        }
    ));
    assert!(matches!(
        &p.changes[16],
        Change::McpAdd {
            source: McpSource::Forbidden("env"),
            ..
        }
    ));
}
