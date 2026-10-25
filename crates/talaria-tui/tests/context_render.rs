//! Red-run tests 7 and 26 to 31 (#234): rendering context blocks.

mod support;

use std::path::PathBuf;

use hermes_contract::schemas::{SessionContextBreakdownResult, SessionStats, UsageModel};
use proptest::prelude::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use serde_json::Value;
use support::{ByteSink, Vt, probe};
use talaria_core::context::breakdown::Breakdown;
use talaria_core::context::compact::{CompactResult, Summary};
use talaria_core::context::text::ReadError;
use talaria_core::context::tracker::{CompactingLine, ContextNotice};
use talaria_core::context::usage::UsageReport;
use talaria_core::session::Usage;
use talaria_tui::context_view::{
    CompactingSection, ContextStyle, compact_block, context_block, error_block, notice_block,
    prebuild_block, usage_block,
};
use talaria_tui::live::LiveSection;
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

fn core_data(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../talaria-core/tests/data/context")
        .join(name);
    std::fs::read_to_string(&path).expect("context test data ships with the repo")
}

fn breakdown_result() -> SessionContextBreakdownResult {
    serde_json::from_str(&core_data("breakdown.json")).expect("breakdown fits its result type")
}

fn breakdown() -> Breakdown {
    Breakdown::from_result(&breakdown_result())
}

fn stats() -> SessionStats {
    serde_json::from_str(&core_data("stats.json")).expect("stats fit their result type")
}

fn healthy_plan() -> UsageModel {
    serde_json::from_str(&core_data("bars_healthy.json")).expect("plan fits its model type")
}

fn stored() -> Usage {
    Usage::default()
}

fn gateway_report() -> UsageReport {
    let live = stats();
    UsageReport::build(Ok(&live), &stored(), None, false)
}

fn texts(lines: &[Line<'static>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn test_07_file_statuses_show_glyphs_and_notes() {
    let mut value: Value =
        serde_json::from_str(&core_data("breakdown.json")).expect("test data parses");
    let files = [
        ("a.md", "loaded"),
        ("b.md", "truncated"),
        ("c.md", "flagged"),
        ("d.md", "shadowed"),
        ("e.md", "blocked"),
        ("f.md", "empty"),
        ("g.md", "unreadable"),
        ("h.md", "suppressed"),
        ("i.md", "weird"),
    ];
    value["context_files"] = files
        .iter()
        .map(|(label, status)| {
            serde_json::json!({
                "label": label,
                "path": "<cwd>",
                "chars": 100,
                "est_tokens": 25,
                "loaded": true,
                "status": status,
            })
        })
        .collect();
    let result: SessionContextBreakdownResult =
        serde_json::from_value(value).expect("files fit the result type");
    let parsed = Breakdown::from_result(&result);
    assert!(matches!(
        parsed.files[8].status,
        talaria_core::context::breakdown::FileStatus::Other(ref raw) if raw == "weird"
    ));

    let style = ContextStyle::plain();
    let rows = texts(&context_block("/context", &parsed, 100, &style));
    let file_rows: Vec<&str> = rows
        .iter()
        .skip_while(|row| *row != "Context files")
        .skip(1)
        .map(String::as_str)
        .collect();
    assert_eq!(file_rows.len(), 9);
    let expected: [(&str, &str, Option<&str>); 9] = [
        ("a.md", "✓", None),
        ("b.md", "◐", Some("truncated — over context_file_max_chars")),
        (
            "c.md",
            "⚠",
            Some("loaded — matched prompt-injection pattern(s); review the file"),
        ),
        (
            "d.md",
            "○",
            Some("not loaded — higher-priority context type wins"),
        ),
        (
            "e.md",
            "✗",
            Some("not loaded — blocked by the prompt-injection scan"),
        ),
        ("f.md", "○", Some("not loaded — empty file")),
        ("g.md", "✗", Some("not loaded — could not be read")),
        (
            "h.md",
            "○",
            Some("not loaded — cwd fell back to the Hermes install tree"),
        ),
        ("i.md", "?", Some("status weird")),
    ];
    for (row, (label, glyph, note)) in file_rows.iter().zip(expected) {
        assert!(row.contains(label), "row {row} names {label}");
        assert!(row.contains(glyph), "row {row} shows {glyph}");
        match note {
            Some(note) => assert!(row.contains(note), "row {row} shows its note"),
            None => assert!(!row.contains('·'), "row {row} has no note"),
        }
    }
}

#[test]
fn test_26_context_golden_rows() {
    let style = ContextStyle::plain();
    let rows = texts(&context_block("/context", &breakdown(), 100, &style));
    assert_eq!(
        rows,
        [
            "/context",
            "Context · minimax-m3.1-flash · 34,120 / 200,000 tokens (17%)",
            "■ ■ ▣ ▣ ▣ ▣ ▣ ▣ ▩ ▤ ▧ ▨ ▨ ▨ ▨ ▨ ▨ ▨ ▨ ·",
            "· · · · · · · · · · · · · · · · · · · ·",
            "· · · · · · · · · · · · · · · · · · · ·",
            "· · · · · · · · · · · · · · · · · · · ·",
            "· · · · · · · · · · · · · · · · · · · ·",
            "Estimated usage by category",
            "■ System prompt       ~3,210 tokens   1.6%",
            "▣ Tool definitions   ~11,840 tokens   5.9%",
            "▩ Rules               ~1,200 tokens   0.6%",
            "▤ Skills              ~2,400 tokens   1.2%",
            "▧ Memory                ~320 tokens   0.2%",
            "▨ Conversation       ~15,120 tokens   7.6%",
            "· Free space        ~165,910 tokens  83.0%",
            "Source: provider usage; category counts are local estimates.",
            "Context files",
            "  ✓ AGENTS.md  ~1,200 tokens",
            "  ○ CLAUDE.md  ~800 tokens · not loaded — higher-priority context type wins",
        ]
    );
}

fn last_reading_report() -> UsageReport {
    let mut value: Value =
        serde_json::from_str(&core_data("stats.json")).expect("test data parses");
    value["api_calls"] = Value::from(0);
    value["compression_count"] = Value::from(1);
    let live: SessionStats = serde_json::from_value(value).expect("stats fit their type");
    let kept = Usage {
        model: Some("minimax-m3.1-flash".to_owned()),
        input: Some(4_100),
        output: Some(30),
        total: Some(4_130),
        calls: Some(3),
        ..Usage::default()
    };
    UsageReport::build(Ok(&live), &kept, None, false)
}

fn some_summary() -> Summary {
    Summary {
        headline: "Compressed: 32 → 24 messages".to_owned(),
        token_line: Some("Approx request size: ~15,400 → ~4,100 tokens".to_owned()),
        note: None,
        noop: false,
        aborted: false,
        refused_would_grow: false,
        fallback_used: false,
    }
}

fn every_result() -> Vec<CompactResult> {
    vec![
        CompactResult::Preview {
            lines: vec!["Preview — no changes made.".to_owned()],
        },
        CompactResult::Preview { lines: Vec::new() },
        CompactResult::NoChange {
            summary: Some(some_summary()),
        },
        CompactResult::NoChange { summary: None },
        CompactResult::Compacted {
            summary: Some(some_summary()),
            message_count: Some(24),
            revision: 14,
        },
        CompactResult::Compacted {
            summary: None,
            message_count: Some(24),
            revision: 14,
        },
        CompactResult::Compacted {
            summary: None,
            message_count: None,
            revision: 14,
        },
        CompactResult::Refused {
            text: "Hermes cannot compact while the agent is working.".to_owned(),
        },
        CompactResult::Unknown,
        CompactResult::NotSent {
            text: "Talaria could not send the compaction.".to_owned(),
        },
        CompactResult::UnknownStatus {
            status: "queued".to_owned(),
        },
        CompactResult::Unreadable,
    ]
}

fn render_input(which: usize, width: u16, style: &ContextStyle) -> Vec<Line<'static>> {
    let parsed = breakdown();
    let figure = talaria_core::context::usage::ContextFigure {
        used: 15_400,
        max: 200_000,
        percent: 8,
        estimated: false,
    };
    match which {
        0 => context_block("/context", &parsed, width, style),
        1 => prebuild_block("/context", Some(&figure), width, style),
        2 => prebuild_block("/context", None, width, style),
        3 => usage_block("/usage", &gateway_report(), width, style),
        4 => usage_block("/usage", &last_reading_report(), width, style),
        // The sixth input of section 10 row 27: the report of a failed read with
        // no last reading. The match named five inputs before, and its next arm
        // indexed from six, so the sixth input fell into it and underflowed.
        5 => usage_block(
            "/usage",
            &UsageReport::build(
                Err(&ReadError::TimedOut { secs: 15 }),
                &stored(),
                None,
                false,
            ),
            width,
            style,
        ),
        n if n < 6 + every_result().len() => {
            compact_block("/compact", &every_result()[n - 6], width, style)
        }
        n if n < 6 + every_result().len() + 2 => {
            let which = n - 6 - every_result().len();
            let notice = if which == 0 {
                ContextNotice::AutoCompacted
            } else {
                ContextNotice::CompactedElsewhere {
                    revision: 15,
                    headline: Some("Compressed: 40 → 31 messages".to_owned()),
                }
            };
            notice_block(&notice, width, style)
        }
        n => {
            let errors = [
                ReadError::MethodMissing,
                ReadError::NotFound,
                ReadError::PermissionDenied,
                ReadError::Refused {
                    reason: "busy".to_owned(),
                },
                ReadError::TimedOut { secs: 15 },
                ReadError::NotConnected,
                ReadError::Unreadable,
                ReadError::NotSent,
            ];
            let error = &errors[(n - 6 - every_result().len() - 2) % errors.len()];
            error_block(
                "/usage",
                "/usage",
                "session.stats",
                "s-1",
                error,
                width,
                style,
            )
        }
    }
}

const INPUT_COUNT: usize = 6 + 12 + 2 + 8;

#[test]
fn test_27_rows_fit_every_width() {
    assert_eq!(INPUT_COUNT, 28);
    let style = ContextStyle::plain();

    let wide = texts(&context_block("/context", &breakdown(), 39, &style));
    assert!(wide.iter().any(|row| row.contains('■')));
    let narrow = texts(&context_block("/context", &breakdown(), 38, &style));
    assert!(!narrow.iter().any(|row| row.contains('■')));
    for width in [39_u16, 38, 20] {
        for which in 0..INPUT_COUNT {
            for line in render_input(which, width, &style) {
                assert!(
                    line.width() as u16 <= width,
                    "input {which} overflows width {width}: {}",
                    texts(&[line])[0]
                );
            }
        }
    }
    for which in 0..INPUT_COUNT {
        assert!(
            render_input(which, 0, &style).is_empty(),
            "input {which} renders at width 0"
        );
    }

    let unknown = texts(&compact_block(
        "/compact",
        &CompactResult::Unknown,
        60,
        &style,
    ));
    let joined = unknown.join(" ");
    let words: Vec<&str> = "No answer from Hermes within 11 minutes, or the connection dropped. The compaction may still finish in Hermes. Run the same /compact line again: if it has finished, Hermes returns its result without compacting twice; if it is still running, Talaria says so."
        .split(' ')
        .collect();
    let mut at = 0;
    for word in words {
        let found = joined[at..].find(word).expect("every word appears");
        at += found + word.len();
    }

    proptest!(ProptestConfig::with_cases(64), |(width in 1..=200u16, which in 0..INPUT_COUNT)| {
        for line in render_input(which, width, &style) {
            prop_assert!(
                line.width() as u16 <= width,
                "input {which} overflows width {width}"
            );
        }
    });
}

#[test]
fn test_28_usage_golden_rows() {
    let style = ContextStyle::plain();

    let rows = texts(&usage_block("/usage", &gateway_report(), 200, &style));
    assert_eq!(
        rows,
        [
            "/usage",
            "Usage · this session",
            "  Model        minimax-m3.1-flash (ollama-cloud)",
            "  Tokens       input 30,210 · output 410 · reasoning 0 · total 30,620",
            "  Cache        read 18,000 · write 0 · hit 60%",
            "  API calls    2 · average latency 1.4 s · 38.5 tokens/s",
            "  Context      15,400 / 200,000 tokens (8%)",
            "  Compactions  0",
            "  Rate limits  requests per minute 58 of 60 left · tokens per minute 84,590 of 100,000 left (as of the last API call)",
            "  Cost         not reported by Hermes for this session",
        ]
    );

    let rows = texts(&usage_block("/usage", &last_reading_report(), 200, &style));
    assert_eq!(
        rows,
        [
            "/usage",
            "Usage · this session",
            "  (Hermes has not rebuilt this session's agent since its last compaction or restart; these are the last figures this view received.)",
            "  Model        minimax-m3.1-flash",
            "  Tokens       input 4,100 · output 30 · total 4,130",
            "  API calls    3",
            "  Compactions  1",
            "  Cost         not reported by Hermes for this session",
        ]
    );

    let error = ReadError::TimedOut { secs: 15 };
    let kept = Usage {
        model: Some("minimax-m3.1-flash".to_owned()),
        input: Some(4_100),
        output: Some(30),
        total: Some(4_130),
        calls: Some(3),
        ..Usage::default()
    };
    let failed = UsageReport::build(Err(&error), &kept, None, false);
    let rows = texts(&usage_block("/usage", &failed, 200, &style));
    assert_eq!(
        rows[2],
        "  (Hermes did not answer session.stats: no answer within 15 s; these are the last figures this view received.)"
    );

    let empty = UsageReport::build(Err(&error), &stored(), None, false);
    let rows = texts(&usage_block("/usage", &empty, 200, &style));
    assert_eq!(rows[2], "  Hermes did not answer within 15 s. Try again.");
    assert!(rows.iter().any(|row| row.contains("Cost")));

    let live = stats();
    let planned = UsageReport::build(Ok(&live), &stored(), Some(&healthy_plan()), false);
    let rows = texts(&usage_block("/usage", &planned, 200, &style));
    assert!(
        rows.contains(
            &"  Plan         Plus · healthy · $14.00 spendable · renews Jul 1".to_owned()
        )
    );
    assert!(rows.contains(
        &"               plan [■■■■■■■■■■■■■■······] $14.00 of $20.00 left · 30% used".to_owned()
    ));

    let costs: [(Option<f64>, Option<&str>, &str); 4] = [
        (Some(0.421_3), Some("estimated"), "$0.42 (estimated)"),
        (Some(0.004), Some("actual"), "$0.0040 (actual)"),
        (Some(0.0), Some("actual"), "$0.00 (actual)"),
        (Some(1_234.567), None, "$1,234.57"),
    ];
    for (usd, status, expected) in costs {
        let kept = Usage {
            cost_usd: usd,
            cost_status: status.map(str::to_owned),
            ..Usage::default()
        };
        let live = stats();
        let report = UsageReport::build(Ok(&live), &kept, None, false);
        let rows = texts(&usage_block("/usage", &report, 200, &style));
        let cost = rows
            .iter()
            .find(|row| row.contains("Cost"))
            .expect("a cost row");
        assert!(cost.contains(expected), "cost row {cost} shows {expected}");
    }
}

#[test]
fn test_29_compact_results_render_their_rows() {
    let style = ContextStyle::plain();
    let dim = "  The model now sees a summary of the earlier conversation; your scrollback above is unchanged.";
    let cases: [(CompactResult, Vec<&str>); 9] = [
        (
            CompactResult::Preview {
                lines: vec!["first".to_owned(), "second".to_owned()],
            },
            vec!["/compact", "  first", "  second"],
        ),
        (
            CompactResult::Preview { lines: Vec::new() },
            vec!["/compact", "  Hermes sent an empty preview."],
        ),
        (
            CompactResult::NoChange {
                summary: Some(some_summary()),
            },
            vec![
                "/compact",
                "  Compressed: 32 → 24 messages",
                "    Approx request size: ~15,400 → ~4,100 tokens",
            ],
        ),
        (
            CompactResult::NoChange { summary: None },
            vec!["/compact", "  Hermes made no change."],
        ),
        (
            CompactResult::Compacted {
                summary: Some(some_summary()),
                message_count: Some(24),
                revision: 14,
            },
            vec![
                "/compact",
                "  ✓ Compressed: 32 → 24 messages",
                "    Approx request size: ~15,400 → ~4,100 tokens",
                dim,
            ],
        ),
        (
            CompactResult::Compacted {
                summary: None,
                message_count: Some(24),
                revision: 14,
            },
            vec![
                "/compact",
                "  ✓ Compacted: the conversation now has 24 messages.",
                dim,
            ],
        ),
        (
            CompactResult::Compacted {
                summary: None,
                message_count: None,
                revision: 14,
            },
            vec!["/compact", "  ✓ Compacted.", dim],
        ),
        (
            CompactResult::UnknownStatus {
                status: "queued".to_owned(),
            },
            vec![
                "/compact",
                "  Hermes answered with status \"queued\", which this Talaria does not know. Nothing is assumed to have changed.",
            ],
        ),
        (
            CompactResult::Unreadable,
            vec![
                "/compact",
                "  Hermes's answer could not be read. Run /usage to see whether a compaction was counted.",
            ],
        ),
    ];
    for (result, expected) in cases {
        let rows = texts(&compact_block("/compact here 2", &result, 200, &style));
        assert_eq!(rows[0], "/compact here 2");
        assert_eq!(&rows[1..], &expected[1..]);
    }

    let refused = texts(&compact_block(
        "/compact",
        &CompactResult::Refused {
            text: "Hermes refused the compaction: busy.".to_owned(),
        },
        200,
        &style,
    ));
    assert_eq!(refused[1], "  Hermes refused the compaction: busy.");
    let unknown = texts(&compact_block(
        "/compact",
        &CompactResult::Unknown,
        200,
        &style,
    ));
    assert!(unknown[1].contains("No answer from Hermes within 11 minutes"));
    let unsent = texts(&compact_block(
        "/compact",
        &CompactResult::NotSent {
            text: "Talaria could not send the compaction (gone). Nothing was changed.".to_owned(),
        },
        200,
        &style,
    ));
    assert_eq!(
        unsent[1],
        "  Talaria could not send the compaction (gone). Nothing was changed."
    );
}

#[test]
fn test_30_hostile_text_shows_stand_ins_and_clears_nothing() {
    let mut value: Value =
        serde_json::from_str(&core_data("breakdown.json")).expect("test data parses");
    value["categories"][2]["label"] = Value::from("Rules\u{1b}[2J\u{202e}");
    let result: SessionContextBreakdownResult =
        serde_json::from_value(value).expect("label fits the result type");
    let parsed = Breakdown::from_result(&result);

    let style = ContextStyle::plain();
    let rows = texts(&context_block("/context", &parsed, 100, &style));
    let rules = rows
        .iter()
        .find(|row| row.contains("Rules"))
        .expect("a rules row");
    assert!(!rules.contains('\u{1b}'));
    assert!(rules.contains('␛'));
    assert!(rules.contains("‹U+202E›"));

    let hostile = CompactResult::NoChange {
        summary: Some(Summary {
            headline: "Done\u{7}".to_owned(),
            token_line: None,
            note: None,
            noop: true,
            aborted: false,
            refused_would_grow: false,
            fallback_used: false,
        }),
    };
    let rows = texts(&compact_block("/compact", &hostile, 100, &style));
    assert!(rows[1].contains('␇'));
    assert!(!rows[1].contains('\u{7}'));

    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        probe(false, false, None),
        RestoreLink::detached(),
    )
    .expect("terminal starts");
    let setup = sink.take();
    let mut vt = Vt::new(80, 24);
    vt.feed(&setup);
    let before = vt.rows();
    let mut lines = context_block("/context", &parsed, 80, &style);
    lines.extend(compact_block("/compact", &hostile, 80, &style));
    term.commit_lines(lines).expect("commit works");
    let bytes = sink.take();
    assert!(!bytes.windows(4).any(|w| w == b"\x1b[2J"));
    assert!(!bytes.windows(3).any(|w| w == b"\x1b]0" || w == b"\x1b]2"));
    vt.feed(&bytes);
    assert!(!vt.alternate());
    let after = vt.rows();
    assert_eq!(before.len(), after.len());
    assert!(after.iter().any(|row| row.contains('␛')));
}

#[test]
fn test_31_compacting_line_shows_elapsed_seconds() {
    for (line, words) in [
        (
            CompactingLine::Manual { since_ms: 0 },
            "Compacting conversation",
        ),
        (CompactingLine::Auto { since_ms: 0 }, "Compacting context"),
    ] {
        for (now_ms, secs) in [(0_u64, "0s"), (12_000, "12s"), (125_000, "125s")] {
            if words == "Compacting context" && secs != "12s" {
                continue;
            }
            let section = CompactingSection {
                line: Some(line),
                now_ms,
            };
            assert_eq!(section.height(80), 1);
            let area = Rect::new(0, 0, 80, 1);
            let mut buf = Buffer::empty(area);
            section.render(area, &mut buf);
            let row: String = (0..80)
                .map(|x| buf[(x, 0)].symbol().to_owned())
                .collect::<String>()
                .trim_end()
                .to_owned();
            assert_eq!(row, format!("✻ {words} · {secs}"));
        }
    }
    let hidden = CompactingSection {
        line: None,
        now_ms: 0,
    };
    assert_eq!(hidden.height(80), 0);
    assert_eq!(
        CompactingSection {
            line: Some(CompactingLine::Auto { since_ms: 0 }),
            now_ms: 0,
        }
        .height(0),
        0
    );
}
