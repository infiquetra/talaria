//! Record the seven live fixtures, replay one, and scan a directory for secrets.
//!
//! The script never starts a gateway, never runs `hermes`, never calls `config.set`, and never
//! sends `unattended`. Discovery uses `EnsurePolicy::Never`. A missing precondition is exit 2
//! with a plain message; nothing is written.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use hermes_client::{
    CallOptions, Client, ConnectOptions, DiscoverOptions, EnsurePolicy, Inbound, InboundStream,
    Purpose, ReplaySource, Scope, connect_with, discover_with, handshake, mint_ticket,
};
use serde_json::{Value, json};

/// The seven scenarios of the fixture set, in the order the script records them.
pub const SCENARIOS: [&str; 7] = [
    "plain-answer",
    "tool-diff",
    "approval",
    "clarification",
    "interrupted",
    "subagent-batch",
    "queued-prompt",
];

/// The scenarios that need more than a prompt: they call other gateway methods or look for events
/// the seven above never raise. `record --scenario <name>` names them; a bare `record` does not.
pub const EXTRA_SCENARIOS: [&str; 4] = [
    "slash-palette",
    "shell-and-btw",
    "todo-checklist",
    "attachments-mention-image",
];

/// The prompt of the todo checklist scenario. It holds no double quote.
const TODO_PROMPT: &str = "Use your todo_list tool to write a plan with exactly three items: create a.txt, run sleep 30, create b.txt. Before each step mark it in_progress with the todo_list tool, and after it mark it completed. Do the steps in this folder. Then reply with the single word: done.";

/// The text the attachments scenario submits, and the question it asks about the image.
const IMAGE_PROMPT: &str = "[Image #1] What colour is this image? One word.";

/// A 1 by 1 red PNG (69 bytes), base64 encoded.
const RED_PNG_BASE64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

/// How long the side question may take to answer.
const SIDE_ANSWER_TIMEOUT: Duration = Duration::from_secs(180);

/// The shell command the approval scenario asks for. `manual` mode raises an approval for it.
const DELETE_COMMAND: &str = "rm -rf .pytest_cache";

/// What the operator must set when the approval scenario ran its command without a card.
const NO_APPROVAL_CARD: &str =
    "no approval card appeared; the profile `fixture` must set `approvals.mode: manual`";

/// How long each cleanup call may take.
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);

/// A scenario: its file name stem, prompts and what a good recording shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    pub name: String,
    pub prompts: Vec<String>,
    pub expected_statuses: Vec<String>,
}

impl Scenario {
    /// The scenario named `name`, or `None` when it is not one of [`SCENARIOS`].
    pub fn named(name: &str) -> Option<Scenario> {
        let (prompts, statuses): (&[&str], &[&str]) = match name {
            "plain-answer" => (
                &["Reply with exactly the single word: pong. Do not use any tools."],
                &["complete"],
            ),
            "tool-diff" => (
                &[
                    "In notes.txt change the word beta to gamma. Use your file editing tool, then reply with the single word: done.",
                ],
                &["complete"],
            ),
            "approval" => (
                &[
                    "Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache",
                ],
                &["complete"],
            ),
            "clarification" => (
                &[
                    "Before anything else, use the clarify tool to ask me which colour I prefer, with the choices red and blue. Then tell me the colour I chose.",
                ],
                &["complete"],
            ),
            "interrupted" => (
                &["Count from 1 to 500, one number per line, and do not stop early."],
                &["interrupted"],
            ),
            "subagent-batch" => (
                &[
                    "Use the delegate_task tool to run two subagents in parallel: one lists the files in the current directory, the other reads notes.txt. Then summarise both results in one sentence.",
                ],
                &["complete"],
            ),
            "queued-prompt" => (
                &[
                    "Count from 1 to 30, one number per line.",
                    "Reply with the single word: second.",
                ],
                &["complete", "complete"],
            ),
            "todo-checklist" => (&[TODO_PROMPT], &["complete"]),
            // These three send their own requests; the driver submits their turns itself.
            "slash-palette" | "shell-and-btw" | "attachments-mention-image" => (&[], &["complete"]),
            _ => return None,
        };
        Some(Scenario {
            name: name.to_owned(),
            prompts: prompts.iter().map(|text| (*text).to_owned()).collect(),
            expected_statuses: statuses.iter().map(|text| (*text).to_owned()).collect(),
        })
    }
}

/// Why the script stops with exit code 2. The message is plain text for the operator.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct PreconditionError {
    pub message: String,
}

impl PreconditionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Everything `preflight` needs, injected so tests control it.
#[derive(Debug, Clone)]
pub struct PreflightInput {
    /// The Hermes home (`--home`), where discovery looks for the gateway.
    pub hermes_home: PathBuf,
    /// `--profile`.
    pub profile: String,
    pub login_name: String,
    pub host_name: String,
}

/// What a passing `preflight` learned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preflight {
    pub profile_id: String,
    pub api_origin: String,
    pub code_sha: String,
    pub other_profiles: Vec<String>,
}

/// Discovery facts the substitution list is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryFacts {
    pub profile_id: String,
    pub api_origin: String,
}

/// Facts about the machine the script runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvFacts {
    pub home: String,
    pub login_name: String,
}

/// The settings a driver run needs.
#[derive(Debug, Clone)]
pub struct DriverConfig {
    pub model: String,
    pub provider: String,
    pub scratch: PathBuf,
    pub attempt_timeout: std::time::Duration,
}

/// How one attempt at a scenario ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptResult {
    Ok,
    Failed(String),
    PreconditionAbsent(String),
}

/// The scratch project directory: `notes.txt` and `.pytest_cache`. Removed on drop.
#[derive(Debug)]
pub struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    pub fn create() -> std::io::Result<ScratchDir> {
        let path = tempfile::Builder::new()
            .prefix("record-fixture-")
            .tempdir()?
            .keep();
        let scratch = ScratchDir { path };
        std::fs::write(scratch.path.join("notes.txt"), "alpha\nbeta\n")?;
        std::fs::create_dir(scratch.path.join(".pytest_cache"))?;
        Ok(scratch)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if std::fs::remove_dir_all(&self.path).is_err() && self.path.exists() {
            eprintln!("note: the scratch project could not be removed");
        }
    }
}

/// The minimum length of a login or host name: shorter ones can be neither replaced nor proved
/// absent.
const MIN_NAME: usize = 4;

/// Discovery that never starts a gateway.
pub fn never_ensure() -> DiscoverOptions {
    DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    }
}

/// Check the preconditions without starting anything. Runs discovery with
/// `EnsurePolicy::Never`, so no `hermes` process is ever run.
pub async fn preflight(input: &PreflightInput) -> Result<Preflight, PreconditionError> {
    if input.login_name.chars().count() < MIN_NAME {
        return Err(PreconditionError::new(
            "the login name is unset or under four characters, so a recording cannot be proved \
             free of it; record as a user whose login name has four or more characters",
        ));
    }
    if input.host_name.chars().count() < MIN_NAME {
        return Err(PreconditionError::new(
            "the host name is under four characters, so a recording cannot be proved free of \
             it; record on a machine whose host name has four or more characters",
        ));
    }
    let endpoint = discover_with(&input.hermes_home, &never_ensure())
        .await
        .map_err(|error| {
            PreconditionError::new(format!(
                "{error}. Set up the gateway first: {}",
                error.next_step()
            ))
        })?;
    let pinned = hermes_contract::pinned::COMMIT;
    let code_sha = endpoint.code_sha().unwrap_or_default().to_lowercase();
    if code_sha.is_empty() {
        return Err(PreconditionError::new(format!(
            "the gateway did not report its code commit; run Hermes at the pinned commit {} \
             and restart its gateway",
            &pinned[..12]
        )));
    }
    if code_sha.len() < 7 || !pinned.starts_with(&code_sha) {
        return Err(PreconditionError::new(format!(
            "the gateway runs a different Hermes commit; update Hermes to the pinned commit {} \
             and restart its gateway",
            &pinned[..12]
        )));
    }
    let profile = endpoint.resolve_profile(&input.profile).map_err(|error| {
        PreconditionError::new(format!(
            "{error}. The gateway must serve the profile `{}`: {}",
            input.profile,
            error.next_step()
        ))
    })?;

    let unreachable = |what: &str, next: &str| {
        PreconditionError::new(format!("the gateway could not be reached ({what}). {next}"))
    };
    let ticket = mint_ticket(
        &input.hermes_home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .map_err(|error| unreachable(&error.to_string(), error.next_step()))?;
    let mut connection = connect_with(&endpoint, ticket, &ConnectOptions::default())
        .await
        .map_err(|error| unreachable(&error.to_string(), error.next_step()))?;
    handshake(&mut connection)
        .await
        .map_err(|error| unreachable(&error.to_string(), error.next_step()))?;
    let (client, _inbound) = Client::start(connection.into_parts());
    // No `profile`: the ticket already binds the connection to the profile.
    let mode = client
        .call_raw(
            "config.get",
            json!({"key": "approvals.mode"}),
            call_options(Duration::from_secs(30)),
        )
        .await;
    client.close().await;
    let mode = mode.map_err(|error| unreachable(&error.to_string(), error.next_step()))?;
    if mode.get("value").and_then(Value::as_str) != Some("manual") {
        return Err(PreconditionError::new(format!(
            "the profile `{}` must set `approvals.mode: manual`; the gateway reports another \
             mode, so a guardian model could approve the scenario's command",
            input.profile
        )));
    }

    let mut other_profiles: Vec<String> = endpoint
        .profiles()
        .iter()
        .map(|served| served.name.clone())
        .chain(endpoint.parked().iter().map(|(name, _)| name.clone()))
        .filter(|name| *name != input.profile && name != "default")
        .collect();
    other_profiles.sort();
    other_profiles.dedup();
    Ok(Preflight {
        profile_id: profile.as_ref().to_owned(),
        api_origin: endpoint.api_origin().to_owned(),
        code_sha,
        other_profiles,
    })
}

fn call_options(timeout: Duration) -> CallOptions {
    CallOptions {
        timeout: Some(timeout),
    }
}

/// `(event type, params)` of every inbound event frame of a recording.
fn recorded_events(source: &ReplaySource) -> Vec<(String, Value)> {
    source
        .inbound()
        .filter(|frame| frame.frame["method"] == "event")
        .filter_map(|frame| {
            let params = &frame.frame["params"];
            Some((params["type"].as_str()?.to_owned(), params.clone()))
        })
        .collect()
}

fn position(events: &[(String, Value)], kind: &str, from: usize) -> Option<usize> {
    events
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, (event, _))| event == kind)
        .map(|(index, _)| index)
}

/// The required evidence and final statuses of section 8.4 over a recorded file.
pub fn check_evidence(scenario: &Scenario, source: &ReplaySource) -> Result<(), String> {
    let events = recorded_events(source);
    let statuses: Vec<String> = events
        .iter()
        .filter(|(kind, _)| kind == "message.complete")
        .map(|(_, params)| {
            params["payload"]["status"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    if statuses != scenario.expected_statuses {
        return Err(format!(
            "the turns ended {statuses:?}, expected {:?}",
            scenario.expected_statuses
        ));
    }
    let header = &source.header().expected_statuses;
    if !header.is_empty() && *header != scenario.expected_statuses {
        return Err("the header lists other expected statuses".to_owned());
    }
    let has = |kind: &str| position(&events, kind, 0).is_some();
    let ordered = |first: &str, then: &str| {
        position(&events, first, 0).is_some_and(|at| position(&events, then, at + 1).is_some())
    };
    match scenario.name.as_str() {
        "plain-answer" => {
            if !has("message.delta") {
                return Err("no message.delta".to_owned());
            }
            if has("tool.start") {
                return Err("a tool was used".to_owned());
            }
        }
        "tool-diff" => {
            let diff = events.iter().any(|(kind, params)| {
                kind == "tool.complete"
                    && params["payload"]["inline_diff"]
                        .as_str()
                        .is_some_and(|diff| !diff.is_empty())
            });
            if !diff {
                return Err("no tool.complete with a non-empty inline_diff".to_owned());
            }
        }
        "approval" => {
            if !ordered("approval.request", "approval.settled") {
                return Err("no approval.request followed by approval.settled".to_owned());
            }
        }
        "clarification" => {
            if !ordered("clarify.request", "clarify.settled") {
                return Err("no clarify.request followed by clarify.settled".to_owned());
            }
        }
        "interrupted" => {}
        "subagent-batch" => {
            if !has("subagent.start") || !has("subagent.complete") {
                return Err("no subagent.start and subagent.complete".to_owned());
            }
        }
        "queued-prompt" => {
            let first_complete = position(&events, "message.complete", 0).unwrap_or(usize::MAX);
            let second_start = first_complete
                .checked_add(1)
                .and_then(|from| position(&events, "message.start", from));
            if second_start.is_none() {
                return Err("the second turn did not start after the first completed".to_owned());
            }
        }
        "slash-palette" => {
            let replies = |method: &str| {
                source
                    .outbound()
                    .filter(|frame| frame.frame["method"] == method)
                    .count()
            };
            let catalogue_has_skills = source.inbound().any(|frame| {
                frame.frame["result"]["skills"]
                    .as_object()
                    .is_some_and(|skills| !skills.is_empty())
            });
            if !catalogue_has_skills {
                return Err("no commands.catalog result with a skill".to_owned());
            }
            if replies("complete.slash") < 2 {
                return Err("fewer than two complete.slash calls".to_owned());
            }
            if replies("slash.exec") < 5 {
                return Err("fewer than five slash.exec calls".to_owned());
            }
        }
        "shell-and-btw" => {
            if !has("btw.complete") {
                return Err("no btw.complete".to_owned());
            }
            let shell_calls = source
                .outbound()
                .filter(|frame| frame.frame["method"] == "shell.exec")
                .count();
            if shell_calls < 3 {
                return Err("fewer than three shell.exec calls".to_owned());
            }
        }
        "todo-checklist" => {
            if !has("todo.updated") {
                return Err("the model did not use the todo tool".to_owned());
            }
        }
        "attachments-mention-image" => {
            let attached = source
                .inbound()
                .any(|frame| frame.frame["result"]["attached"] == true);
            if !attached {
                return Err("no image.attach_bytes result with attached true".to_owned());
            }
            let resumed = source
                .outbound()
                .any(|frame| frame.frame["method"] == "session.resume");
            if !resumed {
                return Err("no session.resume call".to_owned());
            }
        }
        other => return Err(format!("unknown scenario {other}")),
    }
    Ok(())
}

/// The `(needle, replacement)` pairs of section 4.7.
pub fn build_substitutions(
    facts: &DiscoveryFacts,
    env: &EnvFacts,
    scratch: &Path,
) -> Vec<(String, String)> {
    let authority = facts
        .api_origin
        .split_once("://")
        .map_or(facts.api_origin.as_str(), |(_, rest)| rest);
    let authority = authority.split('/').next().unwrap_or(authority);
    let scratch_text = scratch.to_string_lossy().into_owned();
    let resolved = std::fs::canonicalize(scratch)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| scratch_text.clone());
    let candidates = [
        (facts.profile_id.clone(), "<profile>"),
        (env.home.clone(), "<home>"),
        (scratch_text, "<tmp>"),
        (resolved, "<tmp>"),
        (authority.to_owned(), "<gateway>"),
        (env.login_name.clone(), "<user>"),
    ];
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (needle, replacement) in candidates {
        if needle.chars().count() >= MIN_NAME
            && needle != "fixture"
            && !pairs.iter().any(|(known, _)| *known == needle)
        {
            pairs.push((needle, replacement.to_owned()));
        }
    }
    pairs
}

/// Rewrite a finished recording so it holds nothing that names the host's own setup. Runs before
/// the evidence check and the scan; an error stops the attempt and nothing is kept.
pub fn scrub_recording(scenario: &str, text: &str) -> Result<String, String> {
    // A model sometimes quotes the working folder in its own words, shortened or not.
    let folder =
        regex::Regex::new(r#"(/private)?/var/folders/([^/\s"\\]+/)*record-fixture-[A-Za-z0-9]+"#)
            .map_err(|_| "the temporary folder pattern did not compile".to_owned())?;
    let text = folder.replace_all(text, "<tmp>");
    let shortened = regex::Regex::new(r"(/private)?/var/folders/\.\.\.")
        .map_err(|_| "the shortened folder pattern did not compile".to_owned())?;
    let text = shortened.replace_all(&text, "<tmp>");
    // A model also names the isolated Hermes home it can see; it is only a stand-in for `~/.hermes`.
    let text = text.replace(".hermes-talaria-test", ".hermes");
    // A path a model streams can be cut between two deltas: the end of the first, then the
    // start of the second, which the recorder's whole-path substitutions cannot match.
    let cut_end = regex::Regex::new(r#"(/private)?/var/folders(/[A-Za-z0-9_.-]*)*""#)
        .map_err(|_| "the cut folder pattern did not compile".to_owned())?;
    let text = cut_end.replace_all(&text, "<tmp>\"");
    let cut_start = regex::Regex::new(r#"("text":")[A-Za-z0-9_]+/T/record-fixture-[A-Za-z0-9]+"#)
        .map_err(|_| "the cut folder pattern did not compile".to_owned())?;
    let text = cut_start.replace_all(&text, "$1").into_owned();
    if text.contains("var/folders") {
        return Err("a temporary folder path survived the scrub".to_owned());
    }
    match scenario {
        "slash-palette" | "shell-and-btw" | "todo-checklist" | "attachments-mention-image" => {
            scrub_skill_names(&text)
        }
        _ => Ok(text),
    }
}

/// One line of a recording split around its `frame` value, so a rewrite leaves every other
/// byte of the line (and the order of its keys) alone.
struct FrameLine<'a> {
    before: &'a str,
    frame: Value,
    after: &'a str,
}

fn split_frame(line: &str) -> Option<FrameLine<'_>> {
    const KEY: &str = ",\"frame\":";
    let start = line.find(KEY)? + KEY.len();
    let mut stream = serde_json::Deserializer::from_str(&line[start..]).into_iter::<Value>();
    let frame = stream.next()?.ok()?;
    let end = start + stream.byte_offset();
    Some(FrameLine {
        before: &line[..start],
        frame,
        after: &line[end..],
    })
}

/// The skills of the host, numbered in order of first appearance.
#[derive(Default)]
struct SkillNames {
    order: Vec<String>,
    /// Patterns for the names that cannot be ordinary prose, built by `prepare_prose`.
    prose: Vec<(regex::Regex, String)>,
}

impl SkillNames {
    fn learn(&mut self, name: &str) {
        let name = name.trim_start_matches('/');
        if !name.is_empty() && !self.order.iter().any(|known| known == name) {
            self.order.push(name.to_owned());
        }
    }

    fn number(&self, name: &str) -> Option<usize> {
        let name = name.trim_start_matches('/');
        self.order
            .iter()
            .position(|known| known == name)
            .map(|at| at + 1)
    }

    fn placeholder(&self, name: &str) -> Option<String> {
        self.number(name)
            .map(|number| format!("fixture-skill-{number}"))
    }

    fn description(&self, name: &str) -> Option<String> {
        self.number(name)
            .map(|number| format!("Fixture skill {number}"))
    }

    /// Build the patterns that find a distinctive skill name in a model's own words. Short plain
    /// words (`box`, `codex`) also occur in public registry descriptions, so they are left out.
    fn prepare_prose(&mut self) -> Result<(), String> {
        self.prose.clear();
        for name in &self.order {
            if !(name.contains('-') || name.chars().count() >= 8) {
                continue;
            }
            let pattern = regex::Regex::new(&format!(
                "(?P<before>^|[^A-Za-z0-9_-]){}(?P<after>$|[^A-Za-z0-9_-])",
                regex::escape(name)
            ))
            .map_err(|_| "a skill name did not make a pattern".to_owned())?;
            let placeholder = self.placeholder(name).unwrap_or_default();
            self.prose.push((pattern, placeholder));
        }
        Ok(())
    }

    /// `text` with every distinctive skill name in it replaced.
    fn replace_prose(&self, text: &str) -> Option<String> {
        let mut current: Option<String> = None;
        for (pattern, placeholder) in &self.prose {
            // Two names side by side share a boundary character, so repeat until none is left.
            while pattern.is_match(current.as_deref().unwrap_or(text)) {
                let replaced = pattern
                    .replace_all(
                        current.as_deref().unwrap_or(text),
                        format!("${{before}}{placeholder}${{after}}"),
                    )
                    .into_owned();
                current = Some(replaced);
            }
        }
        current
    }

    /// `name` with the skill's own name swapped for its placeholder, leading slashes kept.
    fn rename(&self, name: &str) -> Option<String> {
        let bare = name.trim_start_matches('/');
        let slashes = &name[..name.len() - bare.len()];
        self.placeholder(bare)
            .map(|placeholder| format!("{slashes}{placeholder}"))
    }

    /// A command line whose first word is a skill, with that word renamed.
    fn rename_leading_word(&self, line: &str) -> Option<String> {
        let end = line.find(char::is_whitespace).unwrap_or(line.len());
        self.rename(&line[..end])
            .map(|renamed| format!("{renamed}{}", &line[end..]))
    }
}

/// Learn the skills a catalogue result names, in the order of its `pairs` and then its `skills`.
/// Refuses a catalogue with a command that is neither a registry command nor a skill: that is a
/// plugin or user command, which the scrub does not rename.
fn learn_catalogue(names: &mut SkillNames, result: &Value) -> Result<(), String> {
    let skills = result["skills"]
        .as_object()
        .ok_or("the catalogue has no skills")?;
    if result["plugin_commands"]
        .as_object()
        .is_some_and(|map| !map.is_empty())
    {
        return Err(
            "the catalogue lists plugin commands, which the scrub does not rename".to_owned(),
        );
    }
    let registry: Vec<&str> = ["canon", "commands"]
        .iter()
        .filter_map(|key| result[*key].as_object())
        .flat_map(|map| map.keys().map(|name| name.trim_start_matches('/')))
        .collect();
    for pair in result["pairs"].as_array().into_iter().flatten() {
        let Some(command) = pair[0].as_str().map(|name| name.trim_start_matches('/')) else {
            continue;
        };
        if skills.contains_key(&format!("/{command}")) {
            names.learn(command);
        } else if !registry.contains(&command) {
            return Err(
                "the catalogue lists a command that is neither registry nor skill".to_owned(),
            );
        }
    }
    for name in skills.keys() {
        names.learn(name);
    }
    Ok(())
}

/// Learn skills named anywhere else: the session's `info.skills` lists, completion items of kind
/// `skill` and `skill` directives.
fn learn_elsewhere(names: &mut SkillNames, value: &Value) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| learn_elsewhere(names, item)),
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some("skill")
                && let Some(text) = map.get("text").and_then(Value::as_str)
            {
                names.learn(text);
            }
            if map.get("type").and_then(Value::as_str) == Some("skill")
                && let Some(name) = map.get("name").and_then(Value::as_str)
            {
                names.learn(name);
            }
            for (key, child) in map {
                if key == "skills"
                    && let Some(groups) = child.as_object()
                {
                    for list in groups.values().filter_map(Value::as_array) {
                        list.iter()
                            .filter_map(Value::as_str)
                            .for_each(|name| names.learn(name));
                    }
                }
                learn_elsewhere(names, child);
            }
        }
        _ => {}
    }
}

/// The replacement for one string value, or `None` to leave it as it is.
fn renamed_text(
    names: &SkillNames,
    key: &str,
    text: &str,
    kind: ObjectKind,
    own: Option<&str>,
) -> Option<String> {
    match (key, own, kind) {
        ("text", Some(name), ObjectKind::Item) => names.placeholder(name),
        ("meta", Some(name), ObjectKind::Item) => {
            let mark = if text.starts_with("\u{26a1} ") {
                "\u{26a1} "
            } else {
                ""
            };
            names
                .description(name)
                .map(|description| format!("{mark}{description}"))
        }
        ("name", Some(name), ObjectKind::Directive) => names.placeholder(name),
        ("message", Some(name), ObjectKind::Directive) => {
            let placeholder = names.placeholder(name)?;
            let header = text.lines().next().unwrap_or_default();
            let header = header.replace(&format!("\"{name}\""), &format!("\"{placeholder}\""));
            Some(format!("{header}\nFixture skill body."))
        }
        ("notice", Some(name), ObjectKind::Directive) => names
            .placeholder(name)
            .map(|placeholder| text.replace(name, &placeholder)),
        ("display" | "command", _, _) => names.rename_leading_word(text),
        ("text", _, _) if text.starts_with('/') => names.rename_leading_word(text),
        _ => None,
    }
}

/// What an object is, for the names it holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ObjectKind {
    /// A completion item of kind `skill`.
    Item,
    /// A `skill` directive of a slash reply.
    Directive,
    Other,
}

fn rewrite_names(value: &mut Value, names: &SkillNames) {
    match value {
        Value::String(text) => {
            if let Some(replaced) = names.replace_prose(text) {
                *text = replaced;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| rewrite_names(item, names)),
        Value::Object(map) => {
            let kind = if map.get("kind").and_then(Value::as_str) == Some("skill") {
                ObjectKind::Item
            } else if map.get("type").and_then(Value::as_str) == Some("skill") {
                ObjectKind::Directive
            } else {
                ObjectKind::Other
            };
            let own = match kind {
                ObjectKind::Item => map.get("text"),
                ObjectKind::Directive => map.get("name"),
                ObjectKind::Other => None,
            }
            .and_then(Value::as_str)
            .map(str::to_owned);
            let old = std::mem::take(map);
            for (key, mut child) in old {
                let new_key = names.rename(&key).unwrap_or_else(|| key.clone());
                match (key.as_str(), &mut child) {
                    // Public registry data: the argument choices of registry commands.
                    ("sub", _) => {}
                    ("warning" | "plugin_discovery_error", Value::String(text)) => {
                        if !text.is_empty() {
                            "fixture warning".clone_into(text);
                        }
                    }
                    ("skills", Value::Object(groups)) => {
                        for list in groups.values_mut().filter_map(Value::as_array_mut) {
                            for entry in list.iter_mut() {
                                if let Some(placeholder) =
                                    entry.as_str().and_then(|name| names.placeholder(name))
                                {
                                    *entry = Value::String(placeholder);
                                }
                            }
                        }
                        rewrite_names(&mut child, names);
                    }
                    ("pairs", Value::Array(pairs)) => {
                        for pair in pairs {
                            let renamed = pair[0]
                                .as_str()
                                .and_then(|name| names.rename(name).zip(names.description(name)));
                            if let Some((command, description)) = renamed {
                                *pair = json!([command, description]);
                            }
                        }
                    }
                    (key, Value::String(text)) => {
                        let renamed = renamed_text(names, key, text, kind, own.as_deref());
                        if let Some(replaced) = renamed {
                            *text = replaced;
                        } else if let Some(replaced) = names.replace_prose(text) {
                            *text = replaced;
                        }
                    }
                    _ => rewrite_names(&mut child, names),
                }
                map.insert(new_key, child);
            }
        }
        _ => {}
    }
}

/// The skill renaming of the slash palette recording: every skill becomes `fixture-skill-<n>`,
/// its description `Fixture skill <n>`, and a skill directive's body `Fixture skill body.`
fn scrub_skill_names(text: &str) -> Result<String, String> {
    let mut lines: Vec<(&str, Option<FrameLine<'_>>)> =
        text.lines().map(|line| (line, split_frame(line))).collect();
    let mut names = SkillNames::default();
    let catalogue = lines
        .iter()
        .filter_map(|(_, parsed)| parsed.as_ref())
        .find(|line| line.frame["result"]["skills"].is_object());
    if let Some(catalogue) = catalogue {
        learn_catalogue(&mut names, &catalogue.frame["result"])?;
    }
    for parsed in lines.iter().filter_map(|(_, parsed)| parsed.as_ref()) {
        learn_elsewhere(&mut names, &parsed.frame);
    }
    names.prepare_prose()?;
    let mut output = String::with_capacity(text.len());
    for (line, parsed) in &mut lines {
        match parsed {
            Some(parsed) => {
                rewrite_names(&mut parsed.frame, &names);
                let frame = serde_json::to_string(&parsed.frame)
                    .map_err(|_| "a rewritten frame did not encode".to_owned())?;
                output.push_str(parsed.before);
                output.push_str(&frame);
                output.push_str(parsed.after);
            }
            None => output.push_str(line),
        }
        output.push('\n');
    }
    for name in &names.order {
        // Short plain words (`box`, `codex`) also occur in public registry descriptions, so only
        // names that cannot be ordinary prose are searched for in the text.
        if !(name.contains('-') || name.chars().count() >= 8) {
            continue;
        }
        let token = regex::Regex::new(&format!(
            "(^|[^A-Za-z0-9_-]){}($|[^A-Za-z0-9_-])",
            regex::escape(name)
        ))
        .map_err(|_| "a skill name did not make a pattern".to_owned())?;
        if token.is_match(&output) {
            return Err("a skill name survived the scrub".to_owned());
        }
    }
    Ok(output)
}

/// What the driver has seen of the session so far.
#[derive(Debug, Default)]
struct Session {
    session_id: String,
    subscription_id: Value,
    execution_generation: Value,
    /// `(prompt_id, execution_generation)` of approvals not yet settled or cancelled.
    pending_approvals: Vec<(Value, Value)>,
    pending_clarifications: Vec<(Value, Value)>,
    turn_running: bool,
    statuses: Vec<String>,
    approval_seen: bool,
    approval_answered: bool,
    clarify_answered: bool,
    delete_ran: bool,
    interrupt_sent: bool,
    submitted: usize,
    subagent_complete_seen: bool,
}

/// How long the driver keeps listening after the last turn ends for a delegated child that
/// is still running: the gateway returns from the delegation call at once, so the child's
/// completion can arrive after the turn's own.
const SUBAGENT_LINGER: Duration = Duration::from_secs(60);

/// A submission id no other submission of this process shares.
fn submission_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!(
        "record-fixture-{started:x}-{}",
        NEXT.fetch_add(1, Ordering::SeqCst)
    )
}

/// `prompt.submit` with exactly `session_id`, `text` and `submission_id`; never `unattended`.
fn submit_params(session_id: &str, text: &str) -> Result<Value, String> {
    let params =
        hermes_contract::overrides::prompt_submit_params(session_id, text, submission_id());
    hermes_contract::overrides::check_prompt_submit(&params).map_err(|error| error.to_string())?;
    let encoded = serde_json::to_value(&params).map_err(|_| "prompt.submit did not encode")?;
    let mut kept = serde_json::Map::new();
    for key in ["session_id", "text", "submission_id"] {
        if let Some(value) = encoded.get(key) {
            kept.insert(key.to_owned(), value.clone());
        }
    }
    Ok(Value::Object(kept))
}

/// Drive one attempt over a connected client and its inbound stream (the events come
/// only from the stream `Client::start` returned). Never writes the file itself.
pub async fn run_scenario(
    client: &Client,
    inbound: &mut InboundStream,
    scenario: &Scenario,
    config: &DriverConfig,
) -> AttemptResult {
    let mut session = Session::default();
    let outcome = tokio::time::timeout(
        config.attempt_timeout,
        drive(client, inbound, scenario, config, &mut session),
    )
    .await
    .unwrap_or_else(|_| {
        AttemptResult::Failed(format!(
            "the attempt ran longer than {} s",
            config.attempt_timeout.as_secs()
        ))
    });
    if !session.session_id.is_empty() {
        cleanup(client, &mut session).await;
    }
    outcome
}

/// Every attempt ends alike: deny or skip what is pending, interrupt only a running turn, then
/// detach. Never interrupt while an approval is pending.
async fn cleanup(client: &Client, session: &mut Session) {
    let options = call_options(CLEANUP_TIMEOUT);
    for (prompt_id, generation) in std::mem::take(&mut session.pending_approvals) {
        let _ = client
            .call_raw(
                "approval.respond",
                json!({"session_id": session.session_id, "execution_generation": generation,
                       "prompt_id": prompt_id, "choice": "deny"}),
                options,
            )
            .await;
    }
    for (prompt_id, generation) in std::mem::take(&mut session.pending_clarifications) {
        let _ = client
            .call_raw(
                "clarify.respond",
                json!({"session_id": session.session_id, "execution_generation": generation,
                       "prompt_id": prompt_id, "answer": "skip"}),
                options,
            )
            .await;
    }
    if session.turn_running {
        let _ = client
            .call_raw(
                "session.interrupt",
                json!({"session_id": session.session_id,
                       "execution_generation": session.execution_generation}),
                options,
            )
            .await;
    }
    let _ = client
        .call_raw(
            "session.detach",
            json!({"session_id": session.session_id,
                   "subscription_id": session.subscription_id}),
            options,
        )
        .await;
}

fn same_path(left: &str, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// The provenance check of section 8.4 on the snapshot's `info` block.
fn check_info(info: &Value, config: &DriverConfig) -> Result<(), String> {
    let provider = info["provider"].as_str();
    if provider != Some(config.provider.as_str()) {
        return Err("the session reports another provider than requested".to_owned());
    }
    let model_matches = info["model"].as_str().is_some_and(|model| {
        model == config.model || model.rsplit('/').next() == Some(config.model.as_str())
    });
    if !model_matches {
        return Err("the session reports another model than requested".to_owned());
    }
    if !info["cwd"]
        .as_str()
        .is_some_and(|cwd| same_path(cwd, &config.scratch))
    {
        return Err("the session reports another working directory than requested".to_owned());
    }
    Ok(())
}

async fn drive(
    client: &Client,
    inbound: &mut InboundStream,
    scenario: &Scenario,
    config: &DriverConfig,
    session: &mut Session,
) -> AttemptResult {
    let failed = |reason: &str| AttemptResult::Failed(reason.to_owned());
    let created = client
        .call_raw(
            "session.create",
            json!({"source": "cli", "cwd": config.scratch.to_string_lossy(),
                   "model": config.model, "provider": config.provider}),
            CallOptions::default(),
        )
        .await;
    let snapshot = match created {
        Ok(snapshot) => snapshot,
        Err(error) => return AttemptResult::Failed(format!("session.create failed: {error}")),
    };
    let Some(session_id) = snapshot["session_id"].as_str() else {
        return failed("the session snapshot has no session_id");
    };
    session.session_id = session_id.to_owned();
    session.subscription_id = snapshot["subscription_id"].clone();
    session.execution_generation = snapshot["execution_generation"].clone();
    if let Err(reason) = check_info(&snapshot["info"], config) {
        return AttemptResult::Failed(reason);
    }

    if scenario.prompts.is_empty() {
        if let Err(reason) = run_steps(client, inbound, scenario, config, session).await {
            return AttemptResult::Failed(reason);
        }
    } else {
        if let Err(reason) = submit(client, session, &scenario.prompts[0]).await {
            return AttemptResult::Failed(reason);
        }
        if let Err(reason) = pump(client, inbound, scenario, session, |session, _| {
            session.statuses.len() >= scenario.expected_statuses.len()
        })
        .await
        {
            return AttemptResult::Failed(reason);
        }
    }
    if scenario.name == "subagent-batch" {
        let deadline = tokio::time::Instant::now() + SUBAGENT_LINGER;
        while !session.subagent_complete_seen {
            match tokio::time::timeout_at(deadline, inbound.next()).await {
                Ok(Some(Inbound::Event(event))) => {
                    if event.raw["type"] == "subagent.complete" {
                        session.subagent_complete_seen = true;
                    }
                }
                Ok(Some(Inbound::ServerRequest(request))) => {
                    let _ = request
                        .responder
                        .fail(-32601, "the fixture script does not answer this request")
                        .await;
                }
                Ok(Some(Inbound::Malformed(_))) => {}
                Ok(Some(Inbound::Closed(_)) | None) | Err(_) => break,
            }
        }
    }
    if scenario.name == "approval" && !session.approval_seen && session.delete_ran {
        return AttemptResult::PreconditionAbsent(NO_APPROVAL_CARD.to_owned());
    }
    if session.statuses != scenario.expected_statuses {
        return AttemptResult::Failed(format!(
            "the turns ended {:?}, expected {:?}",
            session.statuses, scenario.expected_statuses
        ));
    }
    AttemptResult::Ok
}

/// Read the session's events until `done` says the wait is over. Requests the script does not
/// expect are refused, as in every scenario.
async fn pump(
    client: &Client,
    inbound: &mut InboundStream,
    scenario: &Scenario,
    session: &mut Session,
    mut done: impl FnMut(&Session, &Value) -> bool,
) -> Result<(), String> {
    while !done(session, &Value::Null) {
        let Some(item) = inbound.next().await else {
            return Err("the inbound stream ended".to_owned());
        };
        match item {
            Inbound::Event(event) => {
                if let Some(
                    AttemptResult::Failed(reason) | AttemptResult::PreconditionAbsent(reason),
                ) = on_event(client, scenario, session, &event.raw).await
                {
                    return Err(reason);
                }
                if done(session, &event.raw) {
                    return Ok(());
                }
            }
            Inbound::ServerRequest(request) => {
                let _ = request
                    .responder
                    .fail(-32601, "the fixture script does not answer this request")
                    .await;
            }
            Inbound::Closed(_) => return Err("the connection closed during the attempt".to_owned()),
            Inbound::Malformed(_) => {}
        }
    }
    Ok(())
}

/// Wait for the next event of type `kind` from this session, for at most `limit`.
async fn wait_for_event(
    client: &Client,
    inbound: &mut InboundStream,
    scenario: &Scenario,
    session: &mut Session,
    kind: &str,
    limit: Duration,
) -> Result<(), String> {
    let mut seen = false;
    let waited = tokio::time::timeout(
        limit,
        pump(client, inbound, scenario, session, |_, event| {
            seen |= event["type"] == kind;
            seen
        }),
    )
    .await;
    match waited {
        Ok(result) => result,
        Err(_) => Err(format!("no {kind} within {} s", limit.as_secs())),
    }
}

/// Submit `text` and wait until that turn's `message.complete` has arrived.
async fn submit_and_wait(
    client: &Client,
    inbound: &mut InboundStream,
    scenario: &Scenario,
    session: &mut Session,
    text: &str,
) -> Result<(), String> {
    submit(client, session, text).await?;
    let target = session.submitted;
    pump(client, inbound, scenario, session, |session, _| {
        session.statuses.len() >= target
    })
    .await
}

/// A call whose failure fails the attempt.
async fn call(client: &Client, method: &str, params: Value) -> Result<Value, String> {
    client
        .call_raw(method, params, CallOptions::default())
        .await
        .map_err(|error| format!("{method} failed: {error}"))
}

/// A call whose refusal is part of the recording: the error is kept, not raised.
async fn call_either(client: &Client, method: &str, params: Value) -> Value {
    client
        .call_raw(method, params, CallOptions::default())
        .await
        .unwrap_or(Value::Null)
}

/// The requests of the scenarios that send more than a prompt.
async fn run_steps(
    client: &Client,
    inbound: &mut InboundStream,
    scenario: &Scenario,
    config: &DriverConfig,
    session: &mut Session,
) -> Result<(), String> {
    let id = session.session_id.clone();
    match scenario.name.as_str() {
        "slash-palette" => {
            let catalogue = call(client, "commands.catalog", json!({"session_id": id})).await?;
            let skill = catalogue["skills"]
                .as_object()
                .and_then(|skills| skills.keys().next().cloned())
                .ok_or("the catalogue holds no skill")?;
            let skill = skill.trim_start_matches('/').to_owned();
            call(
                client,
                "complete.slash",
                json!({"session_id": id, "text": "/he"}),
            )
            .await?;
            let personality = call(
                client,
                "complete.slash",
                json!({"session_id": id, "text": "/personality "}),
            )
            .await?;
            if personality["items"].as_array().is_none_or(Vec::is_empty) {
                call(
                    client,
                    "complete.slash",
                    json!({"session_id": id, "text": "/help "}),
                )
                .await?;
            }
            call_either(
                client,
                "slash.exec",
                json!({"session_id": id, "command": "version"}),
            )
            .await;
            let plan = call_either(
                client,
                "slash.exec",
                json!({"session_id": id, "command": "plan write a haiku about rain"}),
            )
            .await;
            let message = find_send_message(&plan).ok_or("/plan returned no message to send")?;
            submit_and_wait(client, inbound, scenario, session, &message).await?;
            call_either(
                client,
                "slash.exec",
                json!({"session_id": id, "command": format!("{skill} tidy")}),
            )
            .await;
            call_either(
                client,
                "slash.exec",
                json!({"session_id": id, "command": "worktree"}),
            )
            .await;
            call_either(
                client,
                "slash.exec",
                json!({"session_id": id, "command": "nonexistent-talaria-check"}),
            )
            .await;
        }
        "shell-and-btw" => {
            for command in [
                "echo talaria-shell-ok",
                "echo oops >&2; exit 3",
                "rm -r ./talaria-uat-does-not-exist",
            ] {
                call_either(
                    client,
                    "shell.exec",
                    json!({"session_id": id, "command": command}),
                )
                .await;
            }
            submit_and_wait(
                client,
                inbound,
                scenario,
                session,
                "Reply with the single word ready.",
            )
            .await?;
            call(
                client,
                "prompt.btw",
                json!({"session_id": id, "text": "What single word did you just reply with?"}),
            )
            .await?;
            wait_for_event(
                client,
                inbound,
                scenario,
                session,
                "btw.complete",
                SIDE_ANSWER_TIMEOUT,
            )
            .await?;
        }
        "attachments-mention-image" => {
            std::fs::create_dir_all(config.scratch.join("src"))
                .and_then(|()| {
                    std::fs::write(
                        config.scratch.join("src/calc.py"),
                        "def add(a, b):\n    return a + b\n",
                    )
                })
                .map_err(|_| "the scratch project could not hold src/calc.py".to_owned())?;
            call(
                client,
                "complete.path",
                json!({"session_id": id, "word": "@calc"}),
            )
            .await?;
            let staged = call(
                client,
                "image.attach_bytes",
                json!({"session_id": id, "content_base64": RED_PNG_BASE64,
                       "filename": "red-1x1.png"}),
            )
            .await?;
            let path = staged["path"]
                .as_str()
                .ok_or("the staged image has no path")?;
            let mime = staged["mime"].as_str().unwrap_or("image/png");
            let mut params = submit_params(&id, IMAGE_PROMPT)?;
            params["attachments"] = json!([{"path": path, "mime": mime}]);
            session.submitted += 1;
            session.turn_running = true;
            call(client, "prompt.submit", params).await?;
            let target = session.submitted;
            pump(client, inbound, scenario, session, |session, _| {
                session.statuses.len() >= target
            })
            .await?;
            call(client, "session.resume", json!({"session_id": id})).await?;
        }
        other => return Err(format!("{other} has no steps")),
    }
    Ok(())
}

/// The `message` of the first `send` directive anywhere in a slash reply.
fn find_send_message(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("send")
                && let Some(message) = map.get("message").and_then(Value::as_str)
            {
                return Some(message.to_owned());
            }
            map.values().find_map(find_send_message)
        }
        Value::Array(items) => items.iter().find_map(find_send_message),
        _ => None,
    }
}

async fn submit(client: &Client, session: &mut Session, text: &str) -> Result<(), String> {
    let params = submit_params(&session.session_id, text)?;
    session.submitted += 1;
    // A submitted prompt is a running turn until its `message.complete`, even before
    // `message.start` arrives: a cleanup that runs in between must still interrupt it.
    session.turn_running = true;
    client
        .call_raw("prompt.submit", params, CallOptions::default())
        .await
        .map(|_| ())
        .map_err(|error| format!("prompt.submit failed: {error}"))
}

/// Handle one event of the session. `Some` ends the attempt.
async fn on_event(
    client: &Client,
    scenario: &Scenario,
    session: &mut Session,
    params: &Value,
) -> Option<AttemptResult> {
    if params["session_id"]
        .as_str()
        .is_some_and(|id| id != session.session_id)
    {
        return None;
    }
    if !params["execution_generation"].is_null() {
        session.execution_generation = params["execution_generation"].clone();
    }
    let payload = &params["payload"];
    match params["type"].as_str().unwrap_or_default() {
        "subagent.complete" => session.subagent_complete_seen = true,
        "message.start" => session.turn_running = true,
        "message.delta" => {
            if scenario.name == "interrupted" && !session.interrupt_sent {
                session.interrupt_sent = true;
                let interrupted = client
                    .call_raw(
                        "session.interrupt",
                        json!({"session_id": session.session_id,
                               "execution_generation": session.execution_generation}),
                        CallOptions::default(),
                    )
                    .await;
                if let Err(error) = interrupted {
                    return Some(AttemptResult::Failed(format!(
                        "session.interrupt failed: {error}"
                    )));
                }
            }
            if scenario.prompts.len() > session.submitted
                && let Err(reason) =
                    submit(client, session, &scenario.prompts[session.submitted]).await
            {
                return Some(AttemptResult::Failed(reason));
            }
        }
        "message.complete" => {
            session
                .statuses
                .push(payload["status"].as_str().unwrap_or_default().to_owned());
            session.turn_running = session.statuses.len() < session.submitted;
            if session.submitted < scenario.prompts.len() {
                return Some(AttemptResult::Failed(
                    "a turn completed before the next prompt was queued".to_owned(),
                ));
            }
        }
        "tool.complete" => {
            let mentions = |value: &Value| {
                value
                    .as_str()
                    .is_some_and(|text| text.contains(DELETE_COMMAND))
            };
            if mentions(&payload["args"]["command"])
                || mentions(&payload["summary"])
                || mentions(&payload["context"])
            {
                session.delete_ran = true;
            }
        }
        "approval.request" => {
            session.approval_seen = true;
            let prompt = (
                payload["prompt_id"].clone(),
                payload_generation(payload, session),
            );
            session.pending_approvals.push(prompt.clone());
            let expected = scenario.name == "approval" && !session.approval_answered;
            let harmless = scenario.name == "todo-checklist"
                && payload["command"]
                    .as_str()
                    .is_some_and(is_plain_todo_command);
            let choice = if expected || harmless { "once" } else { "deny" };
            session.approval_answered |= expected;
            let answered = client
                .call_raw(
                    "approval.respond",
                    json!({"session_id": session.session_id, "execution_generation": prompt.1,
                           "prompt_id": prompt.0, "choice": choice}),
                    CallOptions::default(),
                )
                .await;
            // These scenarios' turns may ask to write a file or run a command; denying it still
            // ends the turn, so it does not fail the attempt.
            let tolerant = EXTRA_SCENARIOS.contains(&scenario.name.as_str());
            if !expected && !harmless && !tolerant {
                return Some(AttemptResult::Failed(
                    "an approval nobody expected appeared; it was denied".to_owned(),
                ));
            }
            if let Err(error) = answered {
                return Some(AttemptResult::Failed(format!(
                    "approval.respond failed: {error}"
                )));
            }
        }
        "approval.settled" => {
            let prompt_id = &payload["prompt_id"];
            session
                .pending_approvals
                .retain(|(pending, _)| pending != prompt_id);
        }
        "approval.cancelled" => session.pending_approvals.clear(),
        "clarify.request" => {
            let prompt = (
                payload["prompt_id"].clone(),
                payload_generation(payload, session),
            );
            session.pending_clarifications.push(prompt.clone());
            let expected = scenario.name == "clarification" && !session.clarify_answered;
            let answer = if expected { "blue" } else { "skip" };
            session.clarify_answered |= expected;
            let answered = client
                .call_raw(
                    "clarify.respond",
                    json!({"session_id": session.session_id, "execution_generation": prompt.1,
                           "prompt_id": prompt.0, "answer": answer}),
                    CallOptions::default(),
                )
                .await;
            if !expected {
                return Some(AttemptResult::Failed(
                    "a clarification nobody expected appeared; it was skipped".to_owned(),
                ));
            }
            if let Err(error) = answered {
                return Some(AttemptResult::Failed(format!(
                    "clarify.respond failed: {error}"
                )));
            }
        }
        "clarify.settled" => {
            let prompt_id = &payload["prompt_id"];
            session
                .pending_clarifications
                .retain(|(pending, _)| pending != prompt_id);
        }
        _ => {}
    }
    None
}

/// A command the todo scenario's plan can ask for (`sleep 30`, creating `a.txt` or `b.txt`):
/// plain words and file names only, no pipes, substitutions, removals or network tools.
fn is_plain_todo_command(command: &str) -> bool {
    let command = command.trim();
    let first = command.split_whitespace().next().unwrap_or_default();
    ["sleep", "touch", "echo", "printf", "cat", "ls", "pwd"].contains(&first)
        && !command.contains("rm ")
        && command
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || " ._/-=:\"'><".contains(ch))
}

fn payload_generation(payload: &Value, session: &Session) -> Value {
    if payload["execution_generation"].is_null() {
        session.execution_generation.clone()
    } else {
        payload["execution_generation"].clone()
    }
}
