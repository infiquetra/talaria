//! Fixed, offline inputs for cell-level appearance regression tests.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use hermes_contract::events::EventFrame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};
use serde_json::json;
use talaria_core::checklist::Checklist;
use talaria_tui::live::LiveSection;
use talaria_tui::theme::{Theme, ThemeFileResolution};

pub struct Fixture {
    pub name: &'static str,
    pub buffer: Buffer,
    pub owned_screen: bool,
    pub family: &'static str,
}

const AREA: Rect = Rect::new(0, 0, 80, 24);

fn draw(
    name: &'static str,
    family: &'static str,
    owned_screen: bool,
    render: impl FnOnce(Rect, &mut Buffer),
) -> Fixture {
    let mut buffer = Buffer::empty(AREA);
    render(AREA, &mut buffer);
    assert!(
        buffer.content.iter().any(|cell| cell.symbol() != " "),
        "fixture {name} must exercise a visible production renderer"
    );
    Fixture {
        name,
        buffer,
        owned_screen,
        family,
    }
}

fn lines(name: &'static str, family: &'static str, rendered: Vec<Line<'static>>) -> Fixture {
    draw(name, family, false, |area, buffer| {
        Paragraph::new(rendered).render(area, buffer);
    })
}

fn checklist() -> Checklist {
    Checklist::from_values(
        1,
        &[
            json!({"id":"1","content":"Read the inputs","status":"completed"}),
            json!({"id":"2","content":"Compare the cells","status":"in_progress"}),
            json!({"id":"3","content":"Run focused checks","status":"pending"}),
            json!({"id":"4","content":"Discard the draft","status":"cancelled"}),
        ],
    )
}

fn children() -> talaria_core::subagents::SubagentTree {
    use talaria_core::subagents::{Clock, SubagentEvent, SubagentTree};
    let mut tree = SubagentTree::new();
    for (id, goal) in [
        ("child-1", "Inspect the diff"),
        ("child-2", "Run focused checks"),
    ] {
        let frame = EventFrame::from_params(&json!({
            "type":"subagent.start", "session_id":"fixture-session", "seq":1,
            "replay_epoch":"fixture-epoch", "payload": {
                "subagent_id":id, "goal":goal, "task_index":0, "task_count":2,
                "model":"example-model"
            }
        }))
        .expect("synthetic subagent frame has the recorded contract shape");
        let event = SubagentEvent::from_notification(&frame.notification).expect("start event");
        tree.apply(
            &event,
            Clock {
                mono_ms: 1_000,
                wall_s: 1.0,
            },
        );
    }
    tree
}

/// The caller warms the syntax highlighter before collecting fixtures.
pub fn fixtures(resolved: &ThemeFileResolution) -> Vec<Fixture> {
    let theme = &resolved.theme;
    let mut fixtures = Vec::new();
    automation(&mut fixtures, resolved);
    background(&mut fixtures, resolved);
    let tasks = checklist();
    let task_style = resolved.checklist_style();
    fixtures.push(draw("checklist", "work.task", false, |area, buffer| {
        resolved.draw_checklist(
            &talaria_tui::checklist::ChecklistSection::new(
                &tasks,
                true,
                8,
                Some("Ctrl+T".into()),
                &task_style,
            ),
            area,
            buffer,
        );
    }));
    connectors(&mut fixtures, resolved);
    context(&mut fixtures, resolved);
    cron(&mut fixtures, resolved);
    launch(&mut fixtures, resolved);
    markdown(&mut fixtures, resolved);
    pager(&mut fixtures, resolved);
    palette(&mut fixtures, resolved);
    profiles(&mut fixtures, resolved);
    review(&mut fixtures, resolved);
    side(&mut fixtures, resolved);
    status(&mut fixtures, theme);
    let tree = children();
    let agents_style = resolved.subagent_style();
    fixtures.push(draw("subagent-dock", "agents", false, |area, buffer| {
        resolved.draw_subagent_dock(
            &talaria_tui::subagents::SubagentDock::new(
                &tree,
                tree.counts(),
                None,
                talaria_core::subagents::Spawning::On(None),
                Some("Alt+S"),
                &agents_style,
                8,
                24,
            ),
            area,
            buffer,
        );
    }));
    tools(&mut fixtures, resolved);
    workflow(&mut fixtures, resolved);
    workspace(&mut fixtures, &tasks, &tree, resolved);
    capabilities(&mut fixtures, resolved);
    history_search(&mut fixtures, resolved);
    mcp(&mut fixtures, resolved);
    picker(&mut fixtures, resolved);
    providers(&mut fixtures, resolved);
    rewind(&mut fixtures, resolved);
    composer(&mut fixtures, resolved);
    model_roles(&mut fixtures, resolved);
    fleet_peek(&mut fixtures, resolved);
    fixtures
}

fn automation(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::automation::{AutomationEvent, AutomationState, HermesCommandCell, update};
    use talaria_tui::automation::AutomationSection;
    let palette = resolved.automation_theme();
    let mut state = AutomationState::default();
    let _ = update(
        &mut state,
        AutomationEvent::Attached {
            session_id: "fixture-session".into(),
        },
    );
    let _ = update(
        &mut state,
        AutomationEvent::ControlUpdated {
            session_id: "fixture-session".into(),
            raw: json!({"goal": {
            "title":"Compare appearance", "status":"active", "turns_used":2,
            "max_turns":20, "subgoals":[], "gates":[],
            "contract":{"outcome":"","verification":"","constraints":"","boundaries":"","stop_when":""}
        }, "loop":null, "heartbeat":null, "revision":"fixture-1", "updated_at":1000}),
        },
    );
    fixtures.push(draw("automation", "automation", false, |area, buffer| {
        resolved.draw_automation(
            &AutomationSection {
                state: &state,
                now_ms: 1_000_000,
                theme: &palette,
            },
            area,
            buffer,
        );
    }));
    fixtures.push(lines(
        "automation-warning",
        "automation",
        resolved.hermes_command_lines(
            &HermesCommandCell {
                name: "loop".into(),
                typed: "/loop".into(),
                body: "Automation paused: **check the input**.".into(),
                warning: true,
            },
            80,
        ),
    ));
}

fn background(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::background::{BackgroundCell, BgOrigin, BgResult};
    fixtures.push(lines(
        "background",
        "background",
        resolved.background_cell_lines(
            &BackgroundCell {
                task_id: "fixture-task".into(),
                prompt: Some("Inspect the build".into()),
                origin: BgOrigin::Here,
                result: BgResult::Failed("The build needs attention".into()),
            },
            80,
        ),
    ));
    let work = talaria_core::background::BackgroundWork::default();
    let view = talaria_tui::background::ProcessesView::default();
    fixtures.push(draw(
        "background-processes",
        "background",
        true,
        |area, buffer| {
            resolved.draw_background_processes(&view, &work, (42_000, 42.0), area, buffer);
        },
    ));
}

fn connectors(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::connectors::{ConnectorsTab, ConnectorsView, Row, Screen};
    let view = ConnectorsView {
        title: "Connectors and messaging · profile fixture".into(),
        tab: ConnectorsTab::Connectors,
        screen: Screen::ConnectorList {
            rows: vec![
                Row {
                    label: "Example connector".into(),
                    state: "connected".into(),
                    marker: None,
                },
                Row {
                    label: "Other connector".into(),
                    state: "needs setup".into(),
                    marker: None,
                },
            ],
            selected: 0,
            filter: None,
        },
        notice: None,
        footer: vec![("esc".into(), "close".into())],
        review_scroll: 0,
    };
    fixtures.push(draw("connectors", "status", true, |area, buffer| {
        resolved.draw_connectors(&view, &BTreeMap::new(), area, buffer);
    }));
}

fn context(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use hermes_contract::schemas::SessionContextBreakdownResult;
    use talaria_core::context::breakdown::Breakdown;
    let result: SessionContextBreakdownResult = serde_json::from_str(include_str!(
        "../../../talaria-core/tests/data/context/breakdown.json"
    ))
    .expect("checked-in synthetic context result");
    fixtures.push(lines(
        "context",
        "context",
        resolved.context_block("/context", &Breakdown::from_result(&result), 80),
    ));
}

fn cron(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::cron::{
        CardFocus, CronBody, CronView, DetailView, FieldView, FormField, FormView, ListView,
        RowView, RunRowView, RunsView,
    };
    let bodies = [
        (
            "cron-list",
            CronBody::List(ListView {
                filter: Some("daily".into()),
                status: Some("2 jobs".into()),
                selected: Some(0),
                rows: vec![RowView {
                    job_id: "fixture-job".into(),
                    name: "Daily review".into(),
                    schedule: "0 9 * * *".into(),
                    state: "active".into(),
                    last_run: "ok 1h ago".into(),
                }],
            }),
        ),
        (
            "cron-detail",
            CronBody::Detail(DetailView {
                fields: vec![
                    ("Name".into(), vec!["Daily review".into()]),
                    ("State".into(), vec!["active".into()]),
                ],
                runs: RunsView::Rows {
                    pending: None,
                    selected: Some(0),
                    rows: vec![RunRowView {
                        start: "1h ago".into(),
                        status: "ok".into(),
                        duration: "30s".into(),
                        tokens: "123".into(),
                        title: "Review complete".into(),
                    }],
                },
                run_card: Some(vec![("Preview".into(), vec!["Checked the draft".into()])]),
                focus: CardFocus::History,
                fields_scroll: 0,
                run_scroll: 0,
            }),
        ),
        (
            "cron-history",
            CronBody::Detail(DetailView {
                fields: vec![("Name".into(), vec!["Daily review".into()])],
                runs: RunsView::Rows {
                    pending: None,
                    selected: Some(0),
                    rows: vec![RunRowView {
                        start: "1h ago".into(),
                        status: "ok".into(),
                        duration: "30s".into(),
                        tokens: "123".into(),
                        title: "Review complete".into(),
                    }],
                },
                run_card: None,
                focus: CardFocus::History,
                fields_scroll: 0,
                run_scroll: 0,
            }),
        ),
        (
            "cron-form",
            CronBody::Form(FormView {
                fields: vec![FieldView {
                    field: FormField::Name,
                    label: "Name".into(),
                    value: "Daily review".into(),
                    hint: Some("A short name".into()),
                    error: Some("Choose a unique name".into()),
                }],
                focus: FormField::Name,
                targets: vec![("fixture-destination".into(), true)],
                deliver_index: 0,
                status: vec!["Draft only".into()],
                locked: false,
                cursor: Some((0, 3)),
            }),
        ),
    ];
    for (name, body) in bodies {
        let view = CronView {
            title: "Scheduled jobs · profile fixture".into(),
            banner: Some("Offline fixture".into()),
            body,
            notice: Some("Check the schedule".into()),
            question: Some("Apply this change?".into()),
            hints: "enter details · esc close".into(),
        };
        fixtures.push(draw(name, "cron", true, |area, buffer| {
            resolved.draw_cron(&view, area, buffer);
        }));
    }
}

fn launch(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_tui::launch_dialog::{DialogDefaults, DialogMode, LaunchDialog};
    let mut dialog = LaunchDialog::open(
        DialogMode::Worktree,
        DialogDefaults {
            profiles: vec!["fixture".into()],
            profile: "fixture".into(),
            profile_note: None,
            folder: PathBuf::from("fixture-project"),
            draft: Some("Review the draft".into()),
            model_placeholder: "profile default".into(),
            slug: |_| "review-the-draft".into(),
        },
    );
    dialog.show_error("The folder is unavailable", "Choose another folder");
    fixtures.push(draw("launch", "launch", true, |area, buffer| {
        resolved.draw_launch(&dialog, area, buffer)
    }));
}

fn markdown(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    let theme = &resolved.theme;
    use talaria_tui::markdown::MarkdownOptions;
    let options = MarkdownOptions {
        width: 80,
        theme: theme.markdown.clone(),
        color: theme.color,
        ..MarkdownOptions::default()
    };
    fixtures.push(lines("markdown", "transcript", resolved.render_markdown(
        "# Heading\n## Subheading\n### Detail\n**Strong** *emphasis* ~~deleted~~ `inline`\n\n[Link](https://example.invalid)\n\n> Quoted content\n\n---\n\n| Name | State |\n| --- | --- |\n| Item | ready |\n", &options,
    )));
    fixtures.push(lines("syntax", "code.syntax", resolved.render_markdown(
        "```rust\nfn main() {\n    let count = 42; // a comment\n    let name = \"example\";\n    println!(\"{}\", count);\n}\n```\n", &options,
    )));
}

fn pager(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_tui::pager::{ConnectionBanner, Pager, PagerContext, SearchIndex};
    let context = PagerContext {
        cells: &[],
        source_epoch: 1,
        title: "Fixture conversation",
        running: false,
        needs_answer: true,
        connection: ConnectionBanner::Disconnected,
        history_truncated: true,
    };
    let mut pager = Pager::open(&context, 80, 24, &mut SearchIndex::default());
    fixtures.push(draw("pager", "pager", true, |area, buffer| {
        resolved.draw_pager(
            &mut pager,
            area,
            buffer,
            &context,
            &talaria_tui::history::PlainCellLines,
        );
    }));
}

fn palette(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::commands::{CommandSource, PaletteRow, PaletteState, RowKind, SkillOrigin};
    let state = PaletteState {
        open: true,
        selected: 0,
        total: 2,
        rows: vec![
            PaletteRow {
                kind: RowKind::Command,
                name: "/inspect".into(),
                description: "Inspect a fixture".into(),
                source: Some(CommandSource::Talaria),
                refused: false,
                takes_argument: false,
                group: None,
            },
            PaletteRow {
                kind: RowKind::Command,
                name: "/example".into(),
                description: "Example skill".into(),
                source: Some(CommandSource::Skill {
                    origin: SkillOrigin::Local,
                }),
                refused: false,
                takes_argument: true,
                group: None,
            },
        ],
        ..PaletteState::closed()
    };
    let section = talaria_tui::palette::PaletteSection::new(&state, 24);
    fixtures.push(draw("palette", "palette", false, |area, buffer| {
        resolved.draw_palette(&section, area, buffer)
    }));
}

fn profiles(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::profiles::{DetailView, Panel, ProfilesView, RowTone, RowView};
    let mut view = ProfilesView {
        loading: false,
        header: "Profiles · connected fixture".into(),
        rows: vec![RowView {
            name_cell: "fixture".into(),
            label: "needs attention".into(),
            tone: RowTone::Warning,
            model_cell: "example-model".into(),
            next_step: "check setup".into(),
        }],
        selected: Some(0),
        filter: None,
        panel: Panel::None,
        notice: Some("Review the configuration".into()),
        confirm: None,
    };
    fixtures.push(draw("profiles", "profiles", true, |area, buffer| {
        resolved.draw_profiles(&view, area, buffer);
    }));
    view.confirm = Some("Remove fixture? y remove · n cancel".into());
    fixtures.push(draw(
        "profiles-confirm",
        "profiles",
        true,
        |area, buffer| {
            resolved.draw_profiles(&view, area, buffer);
        },
    ));
    view.confirm = None;
    view.panel = Panel::NarrowDetail(DetailView {
        title: "fixture · needs attention".into(),
        state_text: "Configuration available".into(),
        lines: vec!["Model: example-model".into()],
    });
    fixtures.push(draw("profiles-detail", "profiles", true, |area, buffer| {
        resolved.draw_profiles(&view, area, buffer);
    }));
}

fn review(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::review::{
        BranchHead, ChangedFile, FileStatus, Listing, ReviewInput, ReviewState, ReviewTarget, Scope,
    };
    let (mut state, _) = ReviewState::open(
        ReviewTarget {
            folder: PathBuf::from("fixture-project"),
            main_root: PathBuf::from("fixture-project"),
            session: None,
            record: None,
            title: "fixture-project".into(),
        },
        1.0,
    );
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(Listing {
            head: BranchHead {
                oid: Some("abc".into()),
                head: Some("fixture-branch".into()),
                upstream: None,
                ahead: 0,
                behind: 0,
            },
            files: vec![ChangedFile {
                path: "example.rs".into(),
                orig_path: None,
                status: FileStatus::Modified,
                staged: true,
                unstaged: false,
                added: Some(1),
                removed: Some(1),
                is_dir: false,
            }],
            total: 1,
            base: None,
            default_branch: Some("main".into()),
            has_origin: false,
            hooks: 0,
        }),
    });
    let _ = state.update(ReviewInput::DiffLoaded {
        path: "example.rs".into(),
        scope: Scope::Uncommitted,
        result: Ok("@@ -1 +1 @@\n-old\n+new\n".into()),
    });
    fixtures.push(draw("review", "review", true, |area, buffer| {
        resolved.draw_review(&state.view(), area, buffer);
    }));
    use talaria_core::review::{LabelTone, PrLabel};
    let labels = [
        (LabelTone::Success, "#1 open passing"),
        (LabelTone::Error, "#2 open failing"),
        (LabelTone::Warning, "#3 open pending"),
        (LabelTone::Accent, "#4 merged"),
        (LabelTone::Muted, "#5 closed"),
    ]
    .into_iter()
    .map(|(tone, text)| {
        Line::from(resolved.draw_pr_label(
            &PrLabel {
                text: text.into(),
                tone,
            },
            80,
        ))
    })
    .collect();
    fixtures.push(lines("review-labels", "review", labels));
}

fn side(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::side::{ShellCell, ShellOutcome};
    fixtures.push(lines(
        "side",
        "side",
        resolved.shell_cell_lines(
            &ShellCell {
                command: "check-fixture".into(),
                duration_ms: 250,
                outcome: ShellOutcome::Ok {
                    stdout: "one checked item\n".into(),
                    stderr: "one warning\n".into(),
                    code: 3,
                },
            },
            80,
        ),
    ));
}

fn status(fixtures: &mut Vec<Fixture>, theme: &Theme) {
    use talaria_tui::status::{Connection, ExternalSegment, StatusInputs, StatusSection, Tone};
    let inputs = StatusInputs {
        connection: Connection::Connected,
        model: Some("example-model".into()),
        reasoning_effort: Some("medium".into()),
        context_percent: Some(34),
        needs_you: 2,
        working: 3,
        ..StatusInputs::default()
    };
    let external = ExternalSegment {
        text: "one warning".into(),
        tone: Tone::Warning,
    };
    fixtures.push(draw("status", "status", false, |area, buffer| {
        StatusSection::new(&inputs, Some(&external), &theme.status).render(area, buffer);
    }));
}

fn tools(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    let theme = &resolved.theme;
    use talaria_core::session::{CellId, ToolCell, ToolStatus};
    use talaria_tui::tool_cell::{Detail, ToolRenderOptions};
    let mut cell = ToolCell {
        id: CellId::new(1),
        tool_id: "fixture-call".into(),
        name: "terminal".into(),
        context: None,
        args: None,
        labels: vec![],
        status: ToolStatus::Failed,
        progress: None,
        summary: Some("Check failed".into()),
        result: Some("A synthetic failure".into()),
        diff: None,
        duration_s: Some(0.25),
    };
    let options = ToolRenderOptions {
        width: 80,
        theme: theme.tool.clone(),
        color: theme.color,
        detail: Detail::Full,
        expand_hint: Some("Ctrl+O".into()),
        elapsed: None,
    };
    fixtures.push(lines("tool", "tool", resolved.tool_lines(&cell, &options)));
    cell.status = ToolStatus::Succeeded;
    cell.name = "patch".into();
    cell.diff = Some("diff --git a/example.rs b/example.rs\n--- a/example.rs\n+++ b/example.rs\n@@ -1,2 +1,2 @@\n context\n-old\n+new\n".into());
    fixtures.push(lines("diff", "diff", resolved.tool_lines(&cell, &options)));
}

fn workflow(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_workflow::engine::{AgentRequest, CallId, EngineEvent};
    use talaria_workflow::journal::{Manifest, Record, RecordLine, ResolvedLaunch};
    use talaria_workflow::progress::RunProgress;
    use talaria_workflow::progress::live::ProgressState;
    let manifest: Manifest = serde_json::from_value(json!({
        "v":1, "run_id":"01JB2X9Q4M8Z7K3T5V6W8Y0A1C", "created_at_ms":1000,
        "workflow":{"name":"fixture-review","script_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
        "args":null, "started_at_ms":1000, "seed":42, "profile":"fixture", "on_unknown":"wait",
        "talaria_version":"0.0.0", "hermes":{}, "roster":{}, "approval_policy":{}
    })).expect("synthetic manifest");
    let state = ProgressState::blank(&manifest);
    fixtures.push(draw("workflow", "workflow", true, |area, buffer| {
        resolved.draw_workflow(&state, area, buffer);
    }));
    let key = "11111111111111111111111111111111";
    let call_id = CallId {
        digest: key.into(),
        occurrence: 0,
    };
    let records = [
        Record::Engine {
            event: Some(EngineEvent::AgentStarted {
                request: AgentRequest {
                    call_id: call_id.clone(),
                    label: "fixture-review".into(),
                    phase: Some("Inspect".into()),
                    prompt: "Inspect the fixture".into(),
                    schema: None,
                    tier: None,
                    model: Some("example-model".into()),
                    provider: Some("example".into()),
                    effort: None,
                    profile: Some("fixture".into()),
                    toolsets: None,
                    skills: None,
                    isolation: None,
                    max_turns: None,
                    workflow_path: vec![],
                },
                replayed: false,
            }),
            event_ref: None,
        },
        Record::CallIntent {
            key: key.into(),
            attempt: 0,
            call_id,
            label: "fixture-review".into(),
            phase: Some("Inspect".into()),
            workflow_path: vec![],
            launch: ResolvedLaunch {
                profile: "fixture".into(),
                model: "example-model".into(),
                provider: Some("example".into()),
                effort: None,
                toolsets: vec![],
                skills: vec![],
                max_turns: None,
                isolation: None,
                cwd: None,
            },
            request_id: format!("wf:01JB2X9Q4M8Z7K3T5V6W8Y0A1C:{key}"),
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(index, record)| RecordLine {
        v: 1,
        seq: index as u64 + 1,
        at_ms: 1_000 + index as i64 * 100,
        record,
    })
    .collect::<Vec<_>>();
    let mut selected = ProgressState::new(RunProgress::from_records(&manifest, &records));
    selected.select_agent(key);
    assert!(
        selected.progress.agent(key).is_some(),
        "selected fixture agent exists"
    );
    fixtures.push(draw(
        "workflow-selected",
        "workflow",
        true,
        |area, buffer| {
            resolved.draw_workflow(&selected, area, buffer);
        },
    ));
    let directory = tempfile::tempdir().expect("isolated empty workflow fixture directory");
    let picker = talaria_workflow::runner::ProgressPicker::load(directory.path());
    fixtures.push(draw("workflow-picker", "workflow", true, |area, buffer| {
        resolved.draw_workflow_picker(&picker, area, buffer);
    }));
}

fn workspace(
    fixtures: &mut Vec<Fixture>,
    tasks: &Checklist,
    tree: &talaria_core::subagents::SubagentTree,
    resolved: &ThemeFileResolution,
) {
    use talaria_core::layout::PanelVisibility;
    use talaria_tui::live::{LiveView, PlainTextSection};
    use talaria_tui::workspace::{WorkspaceView, menu::MenuState, panel::WorkContents};
    let footer_text = PlainTextSection::new("Fixture composer\nFixture status");
    let footer = LiveView {
        sections: vec![&footer_text],
    };
    let view = WorkspaceView {
        entries: &[],
        details: talaria_tui::pager::Details::Expanded,
        panel: PanelVisibility::Hidden,
        menu: MenuState::shown(),
        checklist: Some(tasks),
        checklist_expanded: true,
        footer: &footer,
        work: None,
    };
    fixtures.push(draw("workspace", "workspace", true, |area, buffer| {
        resolved.draw_workspace(&view, area, buffer, &talaria_tui::history::PlainCellLines);
    }));
    fixtures.push(draw("menu", "menu", false, |area, buffer| {
        resolved.draw_menu(&MenuState::shown(), area, buffer)
    }));
    let mut menu = MenuState::shown();
    menu.focused = true;
    menu.dropdown_open = true;
    menu.menu_index = 2;
    fixtures.push(draw("menu-dropdown", "menu", false, |area, buffer| {
        resolved.draw_menu(&menu, area, buffer);
    }));
    let contents = WorkContents {
        needs_you: 2,
        goal: None,
        turn: talaria_tui::status::TurnState::Streaming,
        verdict_previous_turn: false,
        checklist: Some(tasks),
        checklist_expanded: true,
        waiting: &[],
        agents: tree,
        parent_model: None,
        recent_limit: 3,
        voice_phase: None,
        read_aloud: None,
        voice_target: None,
    };
    fixtures.push(draw("work-panel", "work", true, |area, buffer| {
        resolved.draw_work(area, buffer, &contents);
    }));
}

fn capabilities(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::capabilities::{CapListRow, CapView};
    let view = CapView {
        profile: "fixture".into(),
        title: "Capabilities · profile fixture".into(),
        tabs: vec!["Skills".into(), "Toolsets".into()],
        rows: vec![CapListRow {
            name: "fixture-skill".into(),
            label: "bundled".into(),
            source: "bundled".into(),
            description: "Inspect a synthetic fixture".into(),
            marker: "[x]".into(),
            usage: 3,
            ..CapListRow::default()
        }],
        selected: 0,
        detail: vec!["fixture-skill".into(), "Inspect a synthetic fixture".into()],
        hints: "Space on/off · Esc close".into(),
        open: true,
        ..CapView::default()
    };
    fixtures.push(draw("capabilities", "list", true, |area, buffer| {
        resolved.draw_capabilities(&view, area, buffer);
    }));
}

fn history_search(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::prompt_history::HistoryEntry;
    let search = talaria_tui::history_search::HistorySearch::open(
        Arc::from(vec![
            HistoryEntry {
                text: "Inspect the fixture".into(),
                ts: 1000,
                this_folder: true,
            },
            HistoryEntry {
                text: "Review the draft".into(),
                ts: 900,
                this_folder: true,
            },
        ]),
        "saved draft".into(),
        vec![],
    );
    fixtures.push(draw(
        "history-search",
        "transcript.search",
        false,
        |area, buffer| {
            resolved.draw_history_search(
                &search,
                SystemTime::UNIX_EPOCH + Duration::from_secs(1060),
                area,
                buffer,
            );
        },
    ));
}

fn mcp(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::mcp::{CardView, McpTab, McpView};
    let mut view = McpView {
        title: "MCP servers · profile fixture".into(),
        tab: McpTab::Servers,
        banner: Some("Offline fixture".into()),
        filter: String::new(),
        placeholder: Some("No servers configured".into()),
        rows: vec![],
        selected: None,
        status_line: None,
        card: None,
        pending_line: None,
        notice: Some("Configuration unchanged".into()),
        footer: "enter details · esc close".into(),
        secret_focus: None,
    };
    fixtures.push(draw("mcp", "dialog", true, |area, buffer| {
        resolved.draw_mcp(&view, &|_| 0, area, buffer);
    }));
    view.card = Some(CardView {
        title: "Example server".into(),
        lines: vec!["No connection is attempted".into()],
        fields: vec![],
        footer: "esc back".into(),
    });
    fixtures.push(draw("mcp-card", "dialog", true, |area, buffer| {
        resolved.draw_mcp(&view, &|_| 0, area, buffer);
    }));
}

fn picker(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::models::{
        BrowseCounts, BrowseView, ListScope, PickerRow, PickerView, RowKey,
    };
    let rows = [PickerRow {
        key: Some(RowKey::Model {
            provider: "example".into(),
            model: "example-model".into(),
        }),
        text: "Example model".into(),
        badges: vec!["effort ?".into()],
        price: None,
        dim: false,
        header: false,
    }];
    let details = [
        "Selected: example / example-model".into(),
        "Support: unverified".into(),
    ];
    let mut view = PickerView {
        title: "Switch model · this session",
        filter: "",
        effort: "effort: medium",
        hints: "Enter switch · Esc close",
        refreshing: false,
        rows: &rows,
        selected: Some(0),
        banner: None,
        browse: Some(BrowseView {
            scope: ListScope::All,
            counts: BrowseCounts {
                models: 1,
                providers: 1,
                starred: 0,
                configured: 1,
                setup: 0,
            },
            details: &details,
            details_open: false,
            details_scroll: 0,
        }),
    };
    fixtures.push(draw("picker", "picker", true, |area, buffer| {
        resolved.draw_picker_frame(&view, area, buffer);
    }));
    view.browse.as_mut().expect("browse fixture").details_open = true;
    fixtures.push(draw("picker-details", "picker", true, |area, buffer| {
        resolved.draw_picker_frame(&view, area, buffer);
    }));
}

fn providers(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::providers::browse::{
        BrowseCounts, BrowseKey, BrowseRow, BrowseView, ProviderStatus, SetupMethod,
    };
    use talaria_core::providers::{CardKind, CardView, ProvidersTab, ProvidersView};
    let mut view = ProvidersView {
        title: "Providers · profile fixture".into(),
        tab: ProvidersTab::All,
        rows: vec!["Example provider · key saved".into()],
        selected: Some(0),
        filter: String::new(),
        card: None,
        notice: None,
        banner: Some("Offline fixture".into()),
        secret_focus: false,
        footer: "Enter set up · Esc close".into(),
        browse: Some(BrowseView {
            rows: vec![BrowseRow {
                key: Some(BrowseKey::Provider("example".into())),
                label: "Example provider".into(),
                status: Some(ProviderStatus::KeySaved),
                methods: vec![SetupMethod::Key],
                details: vec!["Reachability unknown".into()],
            }],
            selected: Some(0),
            counts: BrowseCounts {
                providers: 1,
                complete: true,
            },
            ..BrowseView::default()
        }),
    };
    fixtures.push(draw("providers", "picker", true, |area, buffer| {
        resolved.draw_providers(&view, 0, area, buffer);
    }));
    view.card = Some(CardView {
        kind: CardKind::Key,
        step: Some("Step 1 of 3: Paste key".into()),
        lines: vec!["Example provider".into()],
        footer: "Enter save · Esc cancel".into(),
    });
    fixtures.push(draw("providers-card", "providers", true, |area, buffer| {
        resolved.draw_providers(&view, 0, area, buffer);
    }));
}

fn rewind(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::rewind::{LineStyle, RewindScreen, RewindView, ViewLine, ViewRow};
    for (name, colour) in [("rewind", true), ("rewind-off", false)] {
        let mut view = RewindView {
            screen: RewindScreen::Turns,
            title: "Rewind · choose a prompt".into(),
            rows: vec![ViewRow {
                mark: true,
                label: "#1".into(),
                age: "1m ago".into(),
                clock: String::new(),
                text: "Inspect the fixture",
                tail: "files",
            }],
            selected: 0,
            first_visible: 0,
            body: vec![ViewLine {
                text: "Uncommitted edits may be replaced".into(),
                style: LineStyle::Warning,
            }],
            diff: None,
            diff_scroll: 0,
            footer: "enter actions · esc close".into(),
        };
        fixtures.push(draw(name, "rewind", true, |area, buffer| {
            resolved.draw_rewind(&view, colour, area, buffer)
        }));
        view.screen = RewindScreen::Actions;
        view.body.push(ViewLine {
            text: "Continue from this prompt".into(),
            style: LineStyle::Accent,
        });
        let name = if colour {
            "rewind-actions"
        } else {
            "rewind-actions-off"
        };
        fixtures.push(draw(name, "rewind", true, |area, buffer| {
            resolved.draw_rewind(&view, colour, area, buffer);
        }));
    }
}

fn composer(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    let mut composer = talaria_tui::composer::Composer::new(talaria_tui::keys::KeyMap::defaults());
    composer.load("Inspect the fixture".into());
    composer.set_style(resolved.theme.composer);
    fixtures.push(draw("composer", "composer", false, |area, buffer| {
        resolved.draw_composer(
            &composer.section(80, 24).with_border_preset(resolved.border),
            area,
            buffer,
        );
    }));
}

fn model_roles(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::model_roles::{ModalView, RolesView, RowView, TabView};
    let row = |text: &str| RowView {
        text: text.into(),
        selected: true,
        dimmed: false,
        marker: None,
    };
    let mut view = RolesView {
        title: "Model roles · profile fixture".into(),
        tabs: vec![TabView {
            label: "Main".into(),
            current: true,
            stale: false,
        }],
        rows: vec![row("Example preset")],
        detail: Some(vec![row("Model: example-model")]),
        focus_detail: false,
        notice: vec!["Offline fixture".into()],
        footer: vec!["Esc close".into()],
        modal: None,
        picker: None,
        raw_keys: false,
    };
    fixtures.push(draw("model-roles", "settings", true, |area, buffer| {
        resolved.draw_model_roles(&view, area, buffer);
    }));
    view.modal = Some(ModalView::Choice {
        title: "Choose a route".into(),
        options: vec!["Profile default".into(), "Example model".into()],
        selected: 0,
    });
    fixtures.push(draw("model-roles-modal", "dialog", true, |area, buffer| {
        resolved.draw_model_roles(&view, area, buffer);
    }));
}

fn fleet_peek(fixtures: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::directory::rows::RowKey;
    use talaria_core::fleet::{FleetBackend, FleetRow, ProfileStatus, RowState};
    use talaria_core::fleet_view::{FleetViewInput, FleetViewState, PeekStatus, update};
    let key = RowKey {
        profile: "fixture".into(),
        session_id: "fixture-session".into(),
    };
    let row = FleetRow {
        terminal_location: None,
        key: key.clone(),
        title: "Inspect fixture".into(),
        model: Some("example-model".into()),
        source: Some("tui".into()),
        started_at: Some(1000.0),
        message_count: 2,
        state: RowState::Working,
        state_since: 1000.0,
        outcome: None,
        foreground: false,
        subscribed: false,
        stale: false,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: None,
        backend: FleetBackend::Hermes,
        agent: None,
        occupancy: None,
        cost: None,
    };
    let mut state = FleetViewState::new(None, 1060.0);
    let _ = update(
        &mut state,
        FleetViewInput::Snapshot {
            version: 1,
            rows: vec![row],
            queue: vec![],
            profiles: vec![("fixture".into(), ProfileStatus::Listed { truncated: false })],
            scope_note: None,
            link_note: None,
        },
        8,
    );
    let _ = update(
        &mut state,
        FleetViewInput::Peek {
            key,
            status: PeekStatus::Failed {
                text: "Synthetic opening failure".into(),
            },
        },
        8,
    );
    fixtures.push(draw("fleet-peek", "fleet", true, |area, buffer| {
        resolved.draw_fleet(
            &state,
            None,
            area,
            buffer,
            &talaria_tui::fleet_view::FleetTheme::default(),
        );
    }));
}

/// The profiles divider exists only in the wide layout.
pub fn wide_profile_divider(resolved: &ThemeFileResolution) -> Buffer {
    use talaria_core::profiles::{DetailView, Panel, ProfilesView, RowTone, RowView};
    let view = ProfilesView {
        loading: false,
        header: "Profiles · connected fixture".into(),
        rows: vec![RowView {
            name_cell: "fixture".into(),
            label: "needs attention".into(),
            tone: RowTone::Warning,
            model_cell: "example-model".into(),
            next_step: "check setup".into(),
        }],
        selected: Some(0),
        filter: None,
        panel: Panel::Detail(DetailView {
            title: "fixture · needs attention".into(),
            state_text: "Configuration available".into(),
            lines: vec!["Model: example-model".into()],
        }),
        notice: None,
        confirm: None,
    };
    let area = Rect::new(0, 0, 120, 24);
    let mut buffer = Buffer::empty(area);
    resolved.draw_profiles(&view, area, &mut buffer);
    buffer
}
