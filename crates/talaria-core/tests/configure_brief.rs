#[path = "support/configure_ctx.rs"]
mod configure_ctx;
use configure_ctx::{context, detect, generated_context};
use serde_json::json;
use talaria_core::configure::*;

fn marker_count(text: &str, suffix: &str) -> Option<usize> {
    let before = text.split_once(suffix)?.0;
    before
        .split(|c: char| !c.is_ascii_digit())
        .next_back()?
        .parse()
        .ok()
}

#[test]
fn sections_in_order_and_secrets_masked() {
    let mut ctx = context();
    ctx.raw = json!({"display":{"compact":false},"api_key":"hidden","some_reference":"${NAME}"});
    if let Section::Available(ref mut rows) = ctx.inventory.mcp_servers {
        rows[0].name = "sk-or-v1-talaria-sentinel-0000000000000000".into();
    }
    let brief = build_brief(&ctx, "turn on compact display", detect);
    let parts = [
        "Profile:",
        "Request:",
        "Current state:",
        "Settings set in config.yaml:",
        "Setting keys:",
    ];
    let positions: Vec<_> = parts
        .iter()
        .map(|p| brief.text.find(p).expect("section present"))
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]));
    assert!(positions[0] <= SKILL_MAX_BYTES);
    assert!(brief.text.contains("<secret>"));
    assert!(brief.text.contains("${NAME}"));
    assert!(brief.text.contains(WITHHELD));
    assert!(
        !brief
            .text
            .contains("sk-or-v1-talaria-sentinel-0000000000000000")
    );
    for op in [
        "set",
        "mcp_add",
        "mcp_enable",
        "mcp_remove",
        "toggle",
        "cron_add",
        "cron_pause",
        "cron_resume",
        "cron_remove",
        "main_model",
        "open",
    ] {
        assert!(brief.text[..positions[0]].contains(op), "{op}");
    }
}

#[test]
fn budget_truncates_deterministically() {
    let ctx = generated_context();
    let request = "🦀".repeat(4_000);
    let first = build_brief(&ctx, &request, detect);
    let second = build_brief(&ctx, &request, detect);
    assert_eq!(first, second);
    assert!(first.text.len() <= BRIEF_MAX_BYTES);
    assert!(first.text.contains("omitted") || first.text.contains("more"));
    assert!(first.omitted_keys > 0 || !first.shortened_lists.is_empty());
    let setting_keys = first
        .text
        .split_once("Setting keys:")
        .expect("keys section")
        .1;
    let visible_keys = ctx
        .schema
        .fields
        .iter()
        .filter(|field| setting_keys.contains(&field.key))
        .count();
    let omitted = marker_count(setting_keys, " setting keys omitted;").unwrap_or(0);
    assert_eq!(omitted, ctx.schema.fields.len() - visible_keys);
    assert_eq!(first.omitted_keys, omitted);
    let raw_section = first
        .text
        .split_once("Settings set in config.yaml:")
        .expect("raw section")
        .1
        .split_once("Setting keys:")
        .expect("next section")
        .0;
    let raw_visible = ctx
        .schema
        .fields
        .iter()
        .take(200)
        .filter(|field| raw_section.contains(&field.key))
        .count();
    let raw_omitted = marker_count(raw_section, " more settings set;").unwrap_or(0);
    assert_eq!(raw_omitted, 200 - raw_visible);
    if let Some(line) = first
        .text
        .lines()
        .find(|line| line.starts_with("mcp_servers:"))
    {
        if !line.contains("omitted (ask") {
            let visible = (0..300)
                .filter(|n| line.contains(&format!("{n:03}{}", "x".repeat(197))))
                .count();
            let omitted = marker_count(line, " more").unwrap_or(0);
            assert_eq!(visible + omitted, 300);
        }
    } else {
        panic!("inventory section missing");
    }
    let small = build_brief(&context(), "hello", detect);
    assert_eq!(small.omitted_keys, 0);
    assert!(small.shortened_lists.is_empty());
}

#[test]
fn context_reply_builds_requested_sections() {
    let mut ctx = context();
    ctx.raw = json!({"display":{"compact":"${NAME}"}});
    ctx.effective = json!({"display":{"compact":"expanded"}});
    let need = Need {
        sections: vec![
            "settings:display".into(),
            "mcp_servers".into(),
            "unknown".into(),
        ],
        why: None,
    };
    let text = build_context_reply(&ctx, &need, detect);
    assert!(text.starts_with("Talaria context (requested):"));
    assert!(text.contains("${NAME}"));
    assert!(!text.contains("expanded"));
    assert!(text.contains("alpha"));
    assert!(text.contains("unknown section: unknown"));
    assert!(text.ends_with("Reply with one talaria-configure block."));
    assert!(text.len() <= CONTEXT_REPLY_MAX_BYTES);
}
