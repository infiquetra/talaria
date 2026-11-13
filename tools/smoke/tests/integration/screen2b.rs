//! Pure screen readers for the phase 2b steps, on the recorded screens.
//!
//! Each reader runs on its recorded screens and on one stale or empty screen. All fail
//! until the stubs in `src/screen2b.rs` are replaced.

use smoke::{
    ContextHeader, ContextRows, PickerState, PrintCheck, RewindList, UsageBlock, context_header,
    context_rows, folder_segment, lines_after_last, picker_state, print_check, queue_indicator,
    rewind_list, rewound_line, status_approvals, usage_block,
};

fn screens(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/screens/phase2b")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {name}: {e}"))
}

#[test]
fn status_approvals_reads_manual_and_off() {
    assert_eq!(
        status_approvals(&screens("approvals_manual.txt")),
        Some("manual".to_owned()),
        "the manual segment"
    );
    assert_eq!(
        status_approvals(&screens("approvals_off.txt")),
        Some("off (this session)".to_owned()),
        "the off segment"
    );
}

#[test]
fn status_approvals_is_none_without_the_segment() {
    assert_eq!(
        status_approvals(&screens("synthetic_stale.txt")),
        None,
        "a stale shell read holds no status line"
    );
    assert_eq!(
        status_approvals("> /context\nContext \u{00b7} not measured yet\n"),
        None,
        "a transcript without a status line"
    );
}

#[test]
fn picker_state_reads_the_open_picker() {
    assert_eq!(
        picker_state(&screens("picker_open.txt")),
        PickerState {
            title: true,
            groups: vec![
                "Google AI Studio".to_owned(),
                "DeepSeek".to_owned(),
                "Ollama Cloud".to_owned(),
            ],
            has_current: true,
            selected_row: None,
            not_set_up: false,
            effort_footer: true,
            no_match: None,
        }
    );
    assert!(
        picker_state("Switch model · this session\nNot set up (1)\neffort: medium\n").not_set_up,
        "the unconfigured-provider banner is recognised when it is visible"
    );
}

#[test]
fn picker_state_reads_the_no_match_line() {
    let state = picker_state(&screens("picker_no_match.txt"));
    assert!(state.title, "the picker is still open");
    assert_eq!(
        state.no_match,
        Some("No model matches \"zzzz\".".to_owned()),
        "the no-match line"
    );
}

#[test]
fn picker_state_is_closed_on_a_plain_screen() {
    assert_eq!(
        picker_state(&screens("synthetic_stale.txt")),
        PickerState {
            title: false,
            groups: Vec::new(),
            has_current: false,
            selected_row: None,
            not_set_up: false,
            effort_footer: false,
            no_match: None,
        }
    );
}

#[test]
fn context_header_reads_model_and_percent() {
    assert_eq!(
        context_header(&screens("context_full.txt")),
        Some(ContextHeader {
            model: "minimax-m3.1-flash".to_owned(),
            percent: true,
            line_index: 1,
        })
    );
}

#[test]
fn context_header_accepts_the_estimated_form() {
    let estimated = screens("context_full.txt").replace("tokens (1%)", "tokens (~1%)");
    assert_eq!(
        context_header(&estimated),
        Some(ContextHeader {
            model: "minimax-m3.1-flash".to_owned(),
            percent: true,
            line_index: 1,
        }),
        "the estimated percent form"
    );
}

#[test]
fn context_header_is_none_before_a_measure() {
    assert_eq!(
        context_header(&screens("context_empty.txt")),
        None,
        "nothing is measured before the first prompt"
    );
    assert_eq!(
        context_header(&screens("synthetic_stale.txt")),
        None,
        "a stale shell read holds no header"
    );
}

#[test]
fn context_rows_finds_categories_free_space_and_source() {
    assert_eq!(
        context_rows(&screens("context_full.txt"), 1),
        ContextRows {
            categories: true,
            free_space: true,
            source: true,
        }
    );
    assert_eq!(
        context_rows(&screens("synthetic_stale.txt"), 0),
        ContextRows {
            categories: false,
            free_space: false,
            source: false,
        },
        "a stale shell read holds no rows"
    );
}

#[test]
fn usage_block_finds_title_tokens_and_cost() {
    assert_eq!(
        usage_block(&screens("usage_block.txt")),
        Some(UsageBlock {
            title_index: 1,
            tokens_row: true,
            cost_row: true,
        })
    );
}

#[test]
fn usage_block_is_none_without_a_title() {
    assert_eq!(
        usage_block(&screens("context_empty.txt")),
        Some(UsageBlock {
            title_index: 4,
            tokens_row: false,
            cost_row: true,
        }),
        "the empty session has a usage title and cost row, but no tokens row"
    );
    assert_eq!(
        usage_block(&screens("synthetic_stale.txt")),
        None,
        "a stale shell read holds no block"
    );
}

#[test]
fn rewind_list_reads_both_prompts_and_the_selected_one() {
    assert_eq!(
        rewind_list(&screens("rewind_list.txt")),
        RewindList {
            open: true,
            prompts: vec![2, 1],
            selected: Some(2),
        }
    );
}

#[test]
fn rewind_list_is_closed_on_a_plain_screen() {
    assert_eq!(
        rewind_list(&screens("synthetic_stale.txt")),
        RewindList {
            open: false,
            prompts: Vec::new(),
            selected: None,
        }
    );
}

#[test]
fn rewound_line_finds_the_marker_for_the_prompt() {
    assert_eq!(rewound_line(&screens("rewound.txt"), 2), Some(0));
    assert_eq!(
        rewound_line(&screens("synthetic_stale.txt"), 2),
        None,
        "a stale shell read holds no marker"
    );
}

#[test]
fn rewound_line_ignores_other_prompt_numbers() {
    assert_eq!(
        rewound_line(&screens("rewound.txt"), 1),
        None,
        "the marker names prompt 2, not prompt 1"
    );
    assert_eq!(
        rewound_line("\u{2500}\u{2500} Rewound to before prompt #12.\n", 1),
        None,
        "prompt 12 is not prompt 1"
    );
}

#[test]
fn queue_indicator_reads_the_count() {
    assert_eq!(queue_indicator(&screens("queue_preview.txt")), Some(1));
}

#[test]
fn queue_indicator_is_none_when_empty() {
    assert_eq!(
        queue_indicator(&screens("queue_done.txt")),
        None,
        "the drained screen shows no indicator"
    );
    assert_eq!(
        queue_indicator(&screens("synthetic_stale.txt")),
        None,
        "a stale shell read shows no indicator"
    );
}

#[test]
fn folder_segment_reads_the_trailing_folder_and_branch() {
    assert_eq!(
        folder_segment(&screens("cd_notices.txt")),
        Some("<scratch>/b (main)".to_owned()),
        "the moved folder with its branch"
    );
    assert_eq!(
        folder_segment(&screens("approvals_manual.txt")),
        Some("<scratch>/a (main)".to_owned()),
        "the starting folder"
    );
    assert_eq!(
        folder_segment(&screens("synthetic_stale.txt")),
        None,
        "a stale shell read holds no status line"
    );
}

#[test]
fn lines_after_last_cuts_at_the_last_match() {
    assert_eq!(
        lines_after_last("alpha\nneedle\nbeta\nneedle\ngamma", "needle"),
        vec!["gamma"],
        "only the lines after the last match"
    );
    assert_eq!(
        lines_after_last(&screens("queue_preview.txt"), "queued (1)"),
        Vec::<&str>::new(),
        "the real indicator shares the captured preview line, with no following lines"
    );
}

#[test]
fn lines_after_last_is_empty_without_a_match() {
    assert!(
        lines_after_last(&screens("synthetic_stale.txt"), "queued (1)").is_empty(),
        "no match means no lines"
    );
    assert!(
        lines_after_last("", "needle").is_empty(),
        "the empty screen holds no lines"
    );
}

#[test]
fn print_check_accepts_5555_with_empty_standard_error() {
    assert_eq!(
        print_check(&screens("print_ok.txt"), "", Some(0)),
        PrintCheck {
            text_ok: true,
            err_empty: true,
            exit_ok: true,
            not_started: false,
        }
    );
    assert_eq!(
        print_check("", "", None),
        PrintCheck {
            text_ok: false,
            err_empty: true,
            exit_ok: false,
            not_started: false,
        },
        "empty inputs check nothing but the empty error"
    );
}

#[test]
fn print_check_reads_the_not_started_json_line() {
    let recorded = screens("print_not_started.txt");
    let mut lines = recorded.lines();
    let out = lines.next().expect("the JSON line");
    let err: Vec<&str> = lines.collect();
    assert_eq!(
        print_check(out, &err.join("\n"), Some(4)),
        PrintCheck {
            text_ok: false,
            err_empty: false,
            exit_ok: false,
            not_started: true,
        }
    );
}

#[test]
fn print_check_rejects_two_json_lines() {
    let recorded = screens("print_not_started.txt");
    let json = recorded.lines().next().expect("the JSON line");
    let out = format!("{json}\n{json}");
    assert!(
        !print_check(&out, "", Some(4)).not_started,
        "two JSON lines are not the not-started line"
    );
}
