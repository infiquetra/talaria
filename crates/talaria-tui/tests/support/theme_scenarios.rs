//! Additional stateful production views for palette usage review.
use std::time::Instant;

use crate::view_colours_support::Fixture;
use ratatui::{buffer::Buffer, layout::Rect};
use talaria_tui::{live::LiveSection, theme::ThemeFileResolution};

fn draw(
    name: &'static str,
    family: &'static str,
    owned_screen: bool,
    render: impl FnOnce(Rect, &mut Buffer),
) -> Fixture {
    let area = Rect::new(0, 0, 80, 24);
    let mut buffer = Buffer::empty(area);
    render(area, &mut buffer);
    assert!(
        buffer.content.iter().any(|cell| cell.symbol() != " "),
        "fixture {name} has no visible content"
    );
    Fixture {
        name,
        buffer,
        owned_screen,
        family,
    }
}

pub fn fixtures(resolved: &ThemeFileResolution) -> Vec<Fixture> {
    use talaria_core::settings::{
        ReviewRow, Screen, SettingsEditorView, SettingsRow, SettingsView,
    };
    let views = &resolved.theme.views;
    let mut result = Vec::new();
    let graph_area = Rect::new(0, 0, 120, 30);
    let mut graph = Buffer::empty(graph_area);
    resolved.draw_workflow(
        &crate::workflow_view::mixed_outcome_state(),
        graph_area,
        &mut graph,
    );
    result.push(Fixture {
        name: "workflow-graph",
        family: "workflow",
        owned_screen: true,
        buffer: graph,
    });
    let browse = SettingsView {
        screen: Screen::Browsing,
        profile: "fixture".into(),
        categories: vec!["general".into(), "display".into()],
        rows: vec![SettingsRow {
            key: "display.compact".into(),
            value: "true".into(),
            origin: "set".into(),
            marker: "*".into(),
            effect_label: "applies now".into(),
        }],
        detail: "Compact display. Default: false.".into(),
        ..Default::default()
    };
    let mut edit = browse.clone();
    edit.screen = Screen::Editing;
    edit.editor = Some(SettingsEditorView {
        text: "example value".into(),
        cursor: 7,
        secret: false,
    });
    let mut review = browse.clone();
    review.screen = Screen::Reviewing;
    review.review_rows = vec![ReviewRow {
        key: "display.compact".into(),
        old: "false".into(),
        new: "true".into(),
        effect_label: "applies now".into(),
        warnings: vec!["Review this change".into()],
    }];
    for (name, view) in [
        ("settings-browse", browse),
        ("settings-edit", edit),
        ("settings-review", review),
    ] {
        result.push(draw(name, "settings", true, |area, buf| {
            talaria_tui::settings::draw_settings_with_theme(&view, area, buf, views)
        }));
    }
    use talaria_core::configure::{ConfigureView, Phase, ViewLine};
    let configure = ConfigureView {
        title: "Configure · profile fixture".into(),
        phase: Phase::Question,
        lines: vec![ViewLine {
            text: "Choose a provider and model".into(),
            row: None,
        }],
        confirmation: Some("Use this provider?".into()),
        footer: "Esc close".into(),
        selected_row: None,
        detail_scroll: None,
    };
    let loading = ConfigureView {
        phase: Phase::Gathering,
        lines: vec![ViewLine {
            text: "Reading profile fixture…".into(),
            row: None,
        }],
        confirmation: None,
        ..configure.clone()
    };
    for (name, view) in [
        ("configure-loading", loading),
        ("configure-question", configure),
    ] {
        result.push(draw(name, "configure", true, |area, buf| {
            talaria_tui::configure::draw_configure_with_theme(&view, area, buf, views)
        }));
    }
    use talaria_core::doctor::{Check, CheckId, Report, Severity};
    let mut doctor = talaria_core::doctor::DoctorView::new();
    doctor.running = false;
    doctor.report = Some(Report {
        profile: "fixture".into(),
        checks: vec![
            Check {
                id: CheckId::Talaria,
                severity: Severity::Ok,
                summary: "client ready".into(),
                details: vec![],
                fix: None,
            },
            Check {
                id: CheckId::Gateway,
                severity: Severity::Warn,
                summary: "starting".into(),
                details: vec![],
                fix: Some("Check again".into()),
            },
            Check {
                id: CheckId::HermesVersion,
                severity: Severity::Fail,
                summary: "incompatible".into(),
                details: vec![],
                fix: Some("Update Hermes".into()),
            },
        ],
    });
    result.push(draw("doctor", "doctor", true, |area, buf| {
        talaria_tui::doctor_view::draw_doctor_with_theme(
            &doctor,
            &talaria_core::doctor::RestartState::Idle,
            area,
            buf,
            views,
        )
    }));
    use talaria_core::memory::{MemoryView, RowView, Section};
    let memory = MemoryView {
        title: "Memory · profile fixture".into(),
        banner: None,
        providers: Section::Ready(vec![RowView {
            marker: '●',
            name: "built-in".into(),
            description: "Local memory".into(),
            label: "in use".into(),
        }]),
        active_text: "active: built-in".into(),
        files: Section::Loading,
        curator: Section::Failed("Unavailable".into()),
        cursor: 0,
        card: None,
        confirm: None,
        following: None,
        editing: false,
        footer: vec![
            ("enter".into(), "open".into()),
            ("esc".into(), "close".into()),
        ],
    };
    result.push(draw("memory", "memory", true, |area, buf| {
        talaria_tui::memory::draw_memory_with_theme(&memory, area, buf, views)
    }));
    use talaria_core::kanban::{BoardCard, FrameCard, FrameColumn, KanbanFrame};
    let card = BoardCard {
        id: "one".into(),
        title: "Review the palette".into(),
        assignee: "fixture".into(),
        status: "ready".into(),
        created_at_ms: 1_792_993_000,
        started_at_ms: None,
        parent_titles: vec![],
        child_titles: vec![],
        body: "Review the visual output and summarize findings.".into(),
        latest_summary: Some("Review underway".into()),
        comments: vec![],
    };
    let board = KanbanFrame {
        board: "fixture".into(),
        shown: 1,
        total: 1,
        live: "12".into(),
        columns: vec![FrameColumn {
            label: "Ready".into(),
            cards: vec![FrameCard {
                id: "one".into(),
                title: "Review the palette".into(),
                assignee: "fixture".into(),
                status: "ready".into(),
                age: "1d".into(),
                selected: true,
            }],
        }],
        status_line: "live:12".into(),
        detail: Some(card.clone()),
    };
    result.push(draw("kanban-live", "kanban", true, |area, buf| {
        talaria_tui::kanban_view::draw_kanban_with_theme(&board, area, buf, views)
    }));
    let mut board_state = talaria_core::kanban::KanbanBoardState::blank();
    board_state.load_cards(vec![card]);
    board_state.detail_id = Some("one".into());
    board_state.detail_fields = vec!["Status: ready".into(), "Comments: 0".into()];
    result.push(draw("kanban-board-detail", "kanban", true, |area, buf| {
        talaria_tui::kanban_view::KanbanView::draw_with_theme(&board_state, area, buf, views)
    }));
    use talaria_core::directory::{
        rows::{DirectoryRow, RowKey, RowOrigin},
        state::{PickerInput, PickerState, update},
    };
    let key = RowKey {
        profile: "fixture".into(),
        session_id: "session-1".into(),
    };
    let mut directory = PickerState::new();
    update(
        &mut directory,
        PickerInput::Opened {
            profiles: vec!["fixture".into()],
            foreground: Some(key.clone()),
            scope_note: None,
            cached: vec![],
            generation: 1,
        },
        20,
    );
    update(
        &mut directory,
        PickerInput::Listed {
            generation: 1,
            profile: "fixture".into(),
            origin: RowOrigin::Live,
            rows: vec![DirectoryRow {
                key,
                title: "Review the parser".into(),
                source: Some("tui".into()),
                started_at: Some(1_792_699_900.0),
                message_count: 12,
                running: true,
                model: Some("example-model".into()),
                origin: RowOrigin::Live,
            }],
            skipped: 0,
        },
        20,
    );
    result.push(draw("session-directory", "directory", true, |area, buf| {
        talaria_tui::session_picker::SessionPickerView::draw_with_theme(
            &directory,
            area,
            buf,
            1_792_700_000.0,
            views,
        )
    }));
    use talaria_core::fleet::{FleetBackend, FleetRow, ProfileStatus, RowState};
    use talaria_core::fleet_view::{FleetViewInput, FleetViewState};
    let rows = [RowState::NeedsYou, RowState::Working, RowState::Idle]
        .into_iter()
        .enumerate()
        .map(|(i, state)| FleetRow {
            terminal_location: None,
            key: RowKey {
                profile: "fixture".into(),
                session_id: format!("session-{i}"),
            },
            title: format!("Example session {i}"),
            model: Some("example-model".into()),
            source: Some("tui".into()),
            started_at: Some(1_792_699_900.0),
            message_count: 2,
            state,
            state_since: 1_792_699_990.0,
            outcome: None,
            foreground: false,
            subscribed: false,
            stale: false,
            gone: false,
            unlisted: false,
            activity: None,
            cost_usd: Some(1.25),
            backend: FleetBackend::Hermes,
            agent: None,
            occupancy: None,
            cost: None,
        })
        .collect();
    let mut fleet = FleetViewState::new(None, 1_792_700_000.0);
    talaria_core::fleet_view::update(
        &mut fleet,
        FleetViewInput::Snapshot {
            version: 1,
            rows,
            queue: vec![],
            profiles: vec![("fixture".into(), ProfileStatus::Listed { truncated: false })],
            scope_note: None,
            link_note: None,
        },
        8,
    );
    result.push(draw("fleet-rows", "fleet", true, |area, buf| {
        talaria_tui::fleet_view::FleetView::draw_with_theme(
            &fleet,
            None,
            area,
            buf,
            &talaria_tui::fleet_view::FleetTheme::default(),
            views,
        )
    }));
    use talaria_core::session::{CellId, PromptCell, PromptKey, PromptKind, PromptState};
    for (name, kind, multi) in [
        ("prompt-approval", PromptKind::Approval, false),
        ("prompt-single", PromptKind::Clarify, false),
        ("prompt-multi", PromptKind::Clarify, true),
        ("prompt-custom", PromptKind::Clarify, false),
    ] {
        let cell = PromptCell {
            id: CellId::new(1),
            key: PromptKey {
                session_id: "fixture".into(),
                prompt_id: name.into(),
                execution_generation: 1,
            },
            kind,
            command: Some("run checks".into()),
            description: Some("Review the command".into()),
            question: Some("Which option?".into()),
            choices: vec!["First".into(), "Second".into()],
            multi_select: multi,
            edit: None,
            timeout_s: None,
            state: PromptState::Open,
            sealed: false,
        };
        let mut stack = talaria_tui::prompts::PromptStack::new();
        let now = Instant::now();
        stack.sync(Some("fixture"), &[&cell], &[], true, now);
        if name == "prompt-custom" {
            use talaria_tui::keys::{Key, KeyCode, KeyKind, Mods};
            let at = now + std::time::Duration::from_millis(300);
            for code in [KeyCode::Char('3'), KeyCode::Char('c'), KeyCode::Char('i')] {
                stack.handle_key(
                    Key {
                        code,
                        mods: Mods::default(),
                        kind: KeyKind::Press,
                    },
                    at,
                );
            }
        }
        result.push(draw(name, "prompt", false, |area, buf| {
            stack
                .section()
                .expect("prompt section")
                .with_theme(views)
                .render(area, buf)
        }));
    }
    inline_scenarios(&mut result, resolved);
    result
}

fn inline_scenarios(result: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use ratatui::widgets::{Paragraph, Widget};
    use talaria_core::commands::{ChoiceItem, ChoiceList};
    use talaria_tui::palette::{ChoiceSection, EffortScaleSection, scale_key_labels};
    let views = &resolved.theme.views;
    let list = ChoiceList::new(vec![
        ChoiceItem {
            id: "ember".into(),
            name: "Ember".into(),
            description: "Warm colours".into(),
            group: Some("Themes".into()),
            current: true,
        },
        ChoiceItem {
            id: "paper".into(),
            name: "Paper".into(),
            description: "Light colours".into(),
            group: Some("Themes".into()),
            current: false,
        },
    ]);
    let choice = ChoiceSection::new(&list, 24).with_theme(views.clone());
    result.push(draw("theme-inline", "choice", false, |area, buf| {
        choice.render(Rect::new(0, 0, area.width, choice.height(area.width)), buf)
    }));
    use talaria_core::models::{Effort, EffortScale, ScaleFacts};
    let scale = EffortScale::new(ScaleFacts {
        model: Some("example-model".into()),
        current: Some(Effort::High),
        wire: None,
        offered: Some(true),
        can_disable: None,
        hivecode: false,
        profile: "fixture".into(),
    });
    let effort = EffortScaleSection::new(
        &scale,
        scale_key_labels(&talaria_tui::keys::KeyMap::defaults()),
        24,
    )
    .with_theme(views.clone());
    result.push(draw("effort-inline", "effort", false, |area, buf| {
        effort.render(Rect::new(0, 0, area.width, effort.height(area.width)), buf)
    }));
    let mention = talaria_core::attachments::MentionView {
        rows: vec![
            ("src/main.rs".into(), "file".into()),
            ("notes.md".into(), "file".into()),
        ],
        selected: Some(0),
        status: None,
        footer: "tab complete · enter insert · esc close",
    };
    let mention = talaria_tui::mention::MentionSection::new(&mention).with_theme(views.clone());
    result.push(draw("mention", "mention", false, |area, buf| {
        mention.render(Rect::new(0, 0, area.width, mention.height(area.width)), buf)
    }));
    use talaria_core::queue::{
        AdmissionId, Controls, QueueView, QueuedRow, RowStatus, SubmissionId,
    };
    let queue = QueueView {
        waiting_ages: Default::default(),
        rows: vec![QueuedRow {
            admission_id: AdmissionId("a".into()),
            input_id: SubmissionId("input-a".into()),
            text: "Review the changes".into(),
            status: RowStatus::Queued,
            outcome: None,
            controls: Controls {
                steer: true,
                replace: true,
            },
            steering: false,
            sequence: 1,
            execution_generation: Some(1),
        }],
        steering: vec![],
        selected: Some(0),
        paused: false,
        running: true,
        editing: None,
        sending: 0,
    };
    result.push(draw("queue-preview", "queue", false, |area, buf| {
        talaria_tui::markdown::section::MarkdownTailSection::new(
            talaria_tui::queue::render_preview_with_theme(&queue, area.width, true, views),
        )
        .render(area, buf)
    }));
    let status = talaria_tui::status::StatusInputs {
        connection: talaria_tui::status::Connection::Connected,
        model: Some("example-model".into()),
        voice: talaria_tui::voice_view::recording_segment(
            std::time::Duration::from_secs(65),
            false,
        ),
        ..Default::default()
    };
    result.push(draw("voice-recording", "voice", false, |area, buf| {
        talaria_tui::status::StatusSection::new(&status, None, &resolved.theme.status)
            .render_into(area, buf)
    }));
    let activity_style = talaria_tui::activity::ActivityStyle::from_theme(
        &resolved.theme,
        talaria_tui::theme::TerminalColors::default(),
    );
    let activity = talaria_tui::activity::ActivityLine {
        activity: Some("Reviewing the changes"),
        elapsed_ms: 1200,
        interrupt: Some("Esc"),
        interrupt_armed: false,
        style: &activity_style,
    };
    result.push(draw("activity-working", "activity", false, |area, buf| {
        activity.render(Rect::new(0, 0, area.width, 1), buf)
    }));
    let mut stream =
        talaria_tui::markdown::MarkdownStream::new_with_theme(Default::default(), views.clone());
    let update = stream.push("The **review** is underway.\n\n- One item\n- Second item");
    result.push(draw(
        "streaming-markdown",
        "streaming",
        false,
        |area, buf| Paragraph::new(update.tail).render(area, buf),
    ));
    use talaria_tui::prompts::{LiveMaskedTarget, MaskedView, PromptStack};
    let masked = MaskedView {
        request_id: "masked-1".into(),
        session_id: Some("fixture".into()),
        method: "sudo".into(),
        fields: vec![("command".into(), "sudo example".into())],
        target: LiveMaskedTarget {
            token: "masked-1".into(),
            session_id: Some("fixture".into()),
            generation: 1,
            instance_id: 1,
            service_id: 1,
        },
    };
    let mut stack = PromptStack::new();
    stack.sync(Some("fixture"), &[], &[masked], true, Instant::now());
    result.push(draw("prompt-masked", "prompt", false, |area, buf| {
        stack
            .section()
            .expect("masked section")
            .with_theme(views)
            .render(area, buf)
    }));
}
