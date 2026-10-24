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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Belongs {
    Yes,
    No,
    Unknown,
}

/// The `--effort` values, in the order the refusal lists them.
pub const EFFORT_LEVELS: [&str; 8] = [
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];

/// The session source Talaria creates and continues.
pub const SESSION_SOURCE: &str = "tui";

fn random_hex<const N: usize>() -> String {
    let mut bytes = [0u8; N];
    // Without system randomness there is no safe id to send, so the launch cannot go on.
    getrandom::fill(&mut bytes).expect("system randomness is available");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 32 lower-case hex characters, made once per launch for `session.create`.
#[must_use]
pub fn new_request_id() -> String {
    random_hex::<16>()
}

/// 16 lower-case hex characters, the launch's part of every `submission_id`.
#[must_use]
pub fn new_launch_id() -> String {
    random_hex::<8>()
}

/// The new-session options the person gave, in the order `--help` lists them.
#[must_use]
pub fn new_session_options(launch: &Launch) -> Vec<&'static str> {
    let mut given = Vec::new();
    if launch.model.is_some() {
        given.push("--model");
    }
    if launch.provider.is_some() {
        given.push("--provider");
    }
    if launch.effort.is_some() {
        given.push("--effort");
    }
    given
}

/// Decide the session request. The descriptor gates only option keys: `source` and `request_id`
/// are not checked, as in Hermes's own client. For continue, the workspace is the launch folder;
/// the caller replaces it with [`workspace_key`] once git has answered.
pub fn plan(
    launch: &Launch,
    request_id: String,
    describe: &RuntimeDescribeResult,
) -> Result<SessionRequest, UsageError> {
    let given = new_session_options(launch);
    if launch.mode != StartMode::New && !given.is_empty() {
        return Err(UsageError::NewSessionOptionsWithResume { given });
    }
    match &launch.mode {
        StartMode::Continue => {
            return Ok(SessionRequest::Latest {
                workspace: launch.folder.clone(),
            });
        }
        StartMode::Resume(Some(id)) => {
            return Ok(SessionRequest::ById {
                session_id: id.trim().to_string(),
            });
        }
        StartMode::Resume(None) => return Ok(SessionRequest::Pick),
        StartMode::New => {}
    }
    let descriptor = &describe.session_create;
    if !descriptor
        .sources
        .iter()
        .any(|source| source == SESSION_SOURCE)
    {
        return Err(UsageError::NoSessionSource);
    }
    let offered = |key: &str| descriptor.parameters.iter().any(|name| name == key);
    if !offered("cwd") {
        return Err(UsageError::CwdNotOffered);
    }
    let reasoning = launch.effort.as_deref().map(parse_effort).transpose()?;
    for (given, key, option) in [
        (launch.model.is_some(), "model", "--model"),
        (launch.provider.is_some(), "provider", "--provider"),
        (reasoning.is_some(), "reasoning", "--effort"),
    ] {
        if given && !offered(key) {
            return Err(UsageError::OptionNotOffered(option));
        }
    }
    Ok(SessionRequest::New(NewSession {
        request_id,
        source: SESSION_SOURCE.to_string(),
        cwd: launch.folder.clone(),
        model: launch.model.clone(),
        provider: launch.provider.clone(),
        reasoning,
    }))
}

/// One of the eight lower-case levels, exactly as typed.
pub fn parse_effort(value: &str) -> Result<String, UsageError> {
    if EFFORT_LEVELS.contains(&value) {
        Ok(value.to_string())
    } else {
        Err(UsageError::UnknownEffort(value.to_string()))
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// The git top level when there is one, else the folder, canonical when it exists.
#[must_use]
pub fn workspace_key(folder: &Path, git_toplevel: Option<&Path>) -> PathBuf {
    canonical(git_toplevel.unwrap_or(folder))
}

/// Whether a session's folder is the workspace or inside it, by whole path components.
#[must_use]
pub fn belongs(session_cwd: Option<&str>, workspace: &Path) -> Belongs {
    let Some(cwd) = session_cwd.filter(|cwd| !cwd.is_empty()) else {
        return Belongs::Unknown;
    };
    if canonical(Path::new(cwd)).starts_with(canonical(workspace)) {
        Belongs::Yes
    } else {
        Belongs::No
    }
}

/// The lines printed after a quit. Profile `default` needs no flag. Escape sequences, control
/// characters and bidirectional marks are removed from both values.
#[must_use]
pub fn exit_hint(session_id: &str, profile: &str) -> String {
    // Printed after the terminal is restored, so nothing from the gateway may carry an escape
    // sequence or a control character into it.
    let session_id = plain(session_id);
    let profile = plain(profile);
    let flag = if profile == "default" {
        String::new()
    } else {
        format!(" --profile {profile}")
    };
    format!("Resume this session with:\n  talaria --resume {session_id}{flag}\n")
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

/// Merge the two list scopes: one row per session, the live row winning, newest first.
#[must_use]
pub fn merge_rows(live: Vec<ListRow>, stored: Vec<ListRow>) -> Vec<ListRow> {
    let mut rows: Vec<ListRow> = Vec::new();
    for row in live.into_iter().chain(stored) {
        if !rows.iter().any(|kept| kept.session_id == row.session_id) {
            rows.push(row);
        }
    }
    rows.sort_by(|a, b| match (a.started_at, b.started_at) {
        (Some(a), Some(b)) => b.total_cmp(&a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    rows.truncate(LIST_ROWS);
    rows
}

/// Text from the gateway with escape sequences and control characters removed.
pub(crate) fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        let bidi = matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
        if !ch.is_control() && !bidi {
            out.push(ch);
        }
    }
    out
}

fn age(started_at: Option<f64>, now: f64) -> String {
    let Some(started) = started_at else {
        return "-".to_string();
    };
    let seconds = now - started;
    if !seconds.is_finite() || seconds < 60.0 {
        return "just now".to_string();
    }
    // Whole units only; the float is a count of seconds well inside u64.
    let whole = |unit: f64| (seconds / unit).floor() as u64;
    if seconds < 3_600.0 {
        format!("{}m ago", whole(60.0))
    } else if seconds < 86_400.0 {
        format!("{}h ago", whole(3_600.0))
    } else {
        format!("{}d ago", whole(86_400.0))
    }
}

/// One numbered line per row: title, age, model, message count, `working`, id.
#[must_use]
pub fn format_rows(rows: &[ListRow], now: f64) -> String {
    let mut out = String::new();
    for (index, row) in rows.iter().enumerate() {
        let title = row
            .title
            .as_deref()
            .map(plain)
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "(untitled)".to_string());
        let model = row
            .model
            .as_deref()
            .map(plain)
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| "-".to_string());
        let working = if row.running { "  working" } else { "" };
        out.push_str(&format!(
            "{:>3}  {title}  {}  {model}  {} messages{working}  {}\n",
            index + 1,
            age(row.started_at, now),
            row.message_count,
            plain(&row.session_id),
        ));
    }
    out
}

/// A row number, an id, or an empty line to cancel.
#[must_use]
pub fn parse_choice(line: &str, rows: usize) -> Choice {
    let line = line.trim();
    if line.is_empty() {
        return Choice::Cancel;
    }
    if line.chars().all(|ch| ch.is_ascii_digit()) {
        let number = line.parse::<usize>().unwrap_or(usize::MAX);
        return if (1..=rows).contains(&number) {
            Choice::Row(number)
        } else {
            Choice::OutOfRange(number)
        };
    }
    Choice::Id(line.to_string())
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
