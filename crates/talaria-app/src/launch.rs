//! Pure launch decisions: modes, identifiers, workspace membership, and the session list.

use std::path::{Path, PathBuf};

use hermes_contract::schemas::RuntimeDescribeResult;

pub const RESUME_HISTORY_CELLS: usize = 50;
pub const MAX_HELD_PROMPTS: usize = 10;
pub const LIST_ROWS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartMode {
    New,
    Continue,
    Resume(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub mode: StartMode,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub effort: Option<String>,
    pub profile: String,
    pub folder: PathBuf,
    pub record: Option<PathBuf>,
    pub bench_replay: Option<PathBuf>,
    pub no_ensure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    pub request_id: String,
    pub source: String,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRequest {
    New(NewSession),
    Latest { workspace: PathBuf },
    ById { session_id: String },
    Pick,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UsageError {
    #[error("new-session options were given with continue or resume")]
    NewSessionOptionsWithResume { given: Vec<&'static str> },
    #[error("unknown effort")]
    UnknownEffort(String),
    #[error("option not offered")]
    OptionNotOffered(&'static str),
    #[error("working folder not offered")]
    CwdNotOffered,
    #[error("session source not offered")]
    NoSessionSource,
}

#[must_use]
pub fn new_request_id() -> String {
    let _ = getrandom::fill;
    todo!("binary")
}

#[must_use]
pub fn new_launch_id() -> String {
    let _ = getrandom::fill;
    todo!("binary")
}

/// The `--effort` values, in the order the refusal lists them.
pub const EFFORT_LEVELS: [&str; 8] = [
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];

/// The session source Talaria creates and continues.
pub const SESSION_SOURCE: &str = "tui";

/// The new-session options the person gave, in the order `--help` lists them.
#[must_use]
pub fn new_session_options(launch: &Launch) -> Vec<&'static str> {
    let _ = launch;
    todo!("binary")
}

pub fn plan(
    launch: &Launch,
    request_id: String,
    describe: &RuntimeDescribeResult,
) -> Result<SessionRequest, UsageError> {
    let _ = (launch, request_id, describe);
    todo!("binary")
}

pub fn parse_effort(value: &str) -> Result<String, UsageError> {
    let _ = value;
    todo!("binary")
}

#[must_use]
pub fn workspace_key(folder: &Path, git_toplevel: Option<&Path>) -> PathBuf {
    let _ = (folder, git_toplevel);
    todo!("binary")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Belongs {
    Yes,
    No,
    Unknown,
}

#[must_use]
pub fn belongs(session_cwd: Option<&str>, workspace: &Path) -> Belongs {
    let _ = (session_cwd, workspace);
    todo!("binary")
}

#[must_use]
pub fn exit_hint(session_id: &str, profile: &str) -> String {
    let _ = (session_id, profile);
    todo!("binary")
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListRow {
    pub session_id: String,
    pub title: Option<String>,
    pub started_at: Option<f64>,
    pub model: Option<String>,
    pub message_count: u64,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Row(usize),
    Id(String),
    Cancel,
    OutOfRange(usize),
}

#[must_use]
pub fn merge_rows(live: Vec<ListRow>, stored: Vec<ListRow>) -> Vec<ListRow> {
    let _ = (live, stored);
    todo!("binary")
}

#[must_use]
pub fn format_rows(rows: &[ListRow], now: f64) -> String {
    let _ = (rows, now);
    todo!("binary")
}

#[must_use]
pub fn parse_choice(line: &str, rows: usize) -> Choice {
    let _ = (line, rows);
    todo!("binary")
}

#[cfg(test)]
mod tests {
    use super::{
        Belongs, Choice, Launch, ListRow, StartMode, UsageError, belongs, exit_hint, format_rows,
        merge_rows, new_launch_id, new_request_id, parse_choice, plan, workspace_key,
    };
    use hermes_contract::schemas::{RuntimeDescribeResult, SessionCreateDescriptor};

    fn describe(parameters: &[&str], sources: &[&str]) -> RuntimeDescribeResult {
        RuntimeDescribeResult {
            authority_epoch: 1,
            capabilities: Vec::new(),
            instance_id: "instance".to_string(),
            profile_id: "fixture".to_string(),
            session_create: SessionCreateDescriptor {
                parameters: parameters.iter().map(|item| (*item).to_string()).collect(),
                sources: sources.iter().map(|item| (*item).to_string()).collect(),
            },
            skin: None,
        }
    }

    fn launch(mode: StartMode) -> Launch {
        Launch {
            mode,
            model: None,
            provider: None,
            effort: None,
            profile: "default".to_string(),
            folder: PathBuf::from("/work/proj"),
            record: None,
            bench_replay: None,
            no_ensure: false,
        }
    }

    use std::path::PathBuf;

    #[test]
    fn workspace_key_and_belongs() {
        let root = tempfile::tempdir().expect("temp dir");
        let folder = root.path().join("sub");
        std::fs::create_dir(&folder).expect("folder");
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&folder, &link).expect("symlink");
        let key = workspace_key(&link, Some(root.path()));
        assert_eq!(key, root.path().canonicalize().expect("root"));
        let plain = workspace_key(&folder, None);
        assert_eq!(plain, folder.canonicalize().expect("folder"));
        let workspace = PathBuf::from("/work/proj");
        assert_eq!(belongs(Some("/work/proj"), &workspace), Belongs::Yes);
        assert_eq!(belongs(Some("/work/proj/sub"), &workspace), Belongs::Yes);
        assert_eq!(belongs(Some("/work/proj2"), &workspace), Belongs::No);
        assert_eq!(belongs(Some("/work/other"), &workspace), Belongs::No);
        assert_eq!(belongs(None, &workspace), Belongs::Unknown);
        let via_link = belongs(
            Some(&link.canonicalize().expect("link").display().to_string()),
            &folder,
        );
        assert_eq!(via_link, Belongs::Yes);
    }

    #[test]
    fn plan_gates_options_on_describe() {
        let mut with_effort = launch(StartMode::New);
        with_effort.effort = Some("high".to_string());
        let missing_effort = plan(
            &with_effort,
            "abc".repeat(8),
            &describe(&["cwd", "model", "provider"], &["tui", "cli"]),
        );
        assert!(matches!(
            missing_effort,
            Err(UsageError::OptionNotOffered("--effort"))
        ));
        let missing_cwd = plan(
            &launch(StartMode::New),
            "abc".repeat(8),
            &describe(&["model"], &["tui"]),
        );
        assert!(matches!(missing_cwd, Err(UsageError::CwdNotOffered)));
        let missing_source = plan(
            &launch(StartMode::New),
            "abc".repeat(8),
            &describe(&["cwd", "model", "provider", "reasoning"], &["cli"]),
        );
        assert!(matches!(missing_source, Err(UsageError::NoSessionSource)));
        let mut full = launch(StartMode::New);
        full.model = Some("minimax-m3.1-flash".to_string());
        full.provider = Some("ollama-cloud".to_string());
        full.effort = Some("high".to_string());
        let planned = plan(
            &full,
            "0123456789abcdef0123456789abcdef".to_string(),
            &describe(&["cwd", "model", "provider", "reasoning"], &["tui"]),
        )
        .expect("source and request id are not gated");
        match planned {
            super::SessionRequest::New(session) => {
                assert_eq!(session.source, "tui");
                assert_eq!(session.cwd, full.folder);
                assert_eq!(session.reasoning.as_deref(), Some("high"));
            }
            other => panic!("expected a new session, got {other:?}"),
        }
    }

    #[test]
    fn exit_hint_text() {
        let session = "local-session-1";
        assert_eq!(
            exit_hint(session, "default"),
            format!("Resume this session with:\n  talaria --resume {session}\n")
        );
        assert_eq!(
            exit_hint(session, "fixture"),
            format!("Resume this session with:\n  talaria --resume {session} --profile fixture\n")
        );
    }

    #[test]
    fn ids_and_picker_rows() {
        let mut requests = std::collections::BTreeSet::new();
        let mut launches = std::collections::BTreeSet::new();
        for _ in 0..1_000 {
            let request = new_request_id();
            assert_eq!(request.len(), 32, "{request}");
            assert!(
                request
                    .chars()
                    .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
            );
            assert!(requests.insert(request));
            let launch_id = new_launch_id();
            assert_eq!(launch_id.len(), 16, "{launch_id}");
            assert!(
                launch_id
                    .chars()
                    .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
            );
            assert!(launches.insert(launch_id));
        }
        let older = ListRow {
            session_id: "older".to_string(),
            title: Some("Older".to_string()),
            started_at: Some(1.0),
            model: None,
            message_count: 1,
            running: false,
        };
        let live = ListRow {
            session_id: "same".to_string(),
            title: Some("Live title".to_string()),
            started_at: Some(50.0),
            model: Some("minimax-m3.1-flash".to_string()),
            message_count: 2,
            running: true,
        };
        let stored = ListRow {
            session_id: "same".to_string(),
            title: Some("Stored title".to_string()),
            started_at: Some(40.0),
            model: Some("other".to_string()),
            message_count: 9,
            running: false,
        };
        let mut extra = Vec::new();
        for index in 0..20 {
            extra.push(ListRow {
                session_id: format!("row-{index}"),
                title: None,
                started_at: Some(10.0 + index as f64),
                model: Some(format!("m{index}")),
                message_count: index,
                running: false,
            });
        }
        let mut live_rows = vec![live.clone()];
        live_rows.extend(extra);
        let merged = merge_rows(live_rows, vec![stored, older.clone()]);
        assert!(merged.len() <= 20);
        assert_eq!(merged[0].session_id, "same");
        assert_eq!(merged[0].title.as_deref(), Some("Live title"));
        assert!(merged[0].running);
        assert!(merged.iter().filter(|row| row.session_id == "same").count() == 1);
        let untitled = ListRow {
            session_id: "bare".to_string(),
            title: None,
            started_at: None,
            model: None,
            message_count: 0,
            running: false,
        };
        let running = ListRow {
            session_id: "work".to_string(),
            title: Some("Hi\u{1b}[0m".to_string()),
            started_at: Some(1_700_000_000.0),
            model: Some("minimax-m3.1-flash".to_string()),
            message_count: 3,
            running: true,
        };
        let drawn = format_rows(&[untitled, running], 1_700_000_030.0);
        assert!(drawn.contains("(untitled)"));
        assert!(drawn.contains("just now") || drawn.contains(" - "));
        assert!(drawn.contains(" working "));
        assert!(!drawn.contains('\u{1b}'));
        assert_eq!(parse_choice("1", 2), Choice::Row(1));
        assert_eq!(parse_choice("2", 2), Choice::Row(2));
        assert_eq!(parse_choice("0", 2), Choice::OutOfRange(0));
        assert_eq!(parse_choice("3", 2), Choice::OutOfRange(3));
        assert_eq!(
            parse_choice("local-session-1", 2),
            Choice::Id("local-session-1".to_string())
        );
        assert_eq!(parse_choice("", 2), Choice::Cancel);
        let _ = older;
    }
}
