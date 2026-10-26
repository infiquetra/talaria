//! The screen script: one JSON document that drives both the in-process `FakeTerminal` and the
//! stateful `fake-herdr` executable, so a step test and a binary test see the same terminal.
//!
//! This file is included by path from both users (`#[path = "script.rs"] mod script;`) and holds
//! the whole machine: parsing, the rules, time (counted in reads of a pane), the text of each
//! pane and the planned failures. It uses only `regex`, `serde_json` and the standard library.
//!
//! # The document
//!
//! ```json
//! {"panes":{"first":{"initial":"$ ","foreground":["-bash"],"rules":[ ... ]},
//!           "second":{"initial":"$ ","foreground":["-bash"],"rules":[ ... ]}},
//!  "fail":[{"call":"pane read","code":"pane_not_found"}]}
//! ```
//!
//! A pane that is missing from the document is a plain shell: initial text `$ `, foreground
//! `["-bash"]`, no rules. The pane `first` is the root pane of the tab; `second` is the pane made
//! by a split.
//!
//! # Rules
//!
//! A rule fires on typed input and changes the pane, step by step in time. Time here is the number
//! of reads of that pane since the rule fired (every `pane read` of the pane is one read).
//!
//! | Field | Meaning |
//! |---|---|
//! | `on` | `run`, `send_text` or `send_keys` |
//! | `matches` | a regular expression searched in the typed text (for `send_keys`, the key name, one key at a time); capture groups are `$1`, `$2`; `$0` is the whole text |
//! | `once` | fire at most once (default true) |
//! | `after_polls` | the effects below appear only after this many reads of the pane (default 0) |
//! | `append` | text added to the pane text, with `$n` substituted; it always starts on a new line |
//! | `churn_polls` | for this many reads after firing, every read appends one new distinct line (a running turn: the screen never settles); then it stops |
//! | `foreground` | the pane's foreground command lines become this list (`$0` allowed); an empty list means the shell alone again |
//! | `remove_in_cwd` | delete this path under the pane's working folder (what a real `rm` does) |
//! | `clear` | clear the visible screen first (the old text stays in the recent text) |
//!
//! Rules are tried in file order; the first rule whose `on` and `matches` fit and which is not used
//! up fires. The pane's visible text is the last 44 lines of its text since the last clear; the
//! recent text and the unwrapped recent text are all of it (a `--lines` limit keeps the last
//! lines); process information answers from `foreground`. A typed key or text that matches no rule
//! is logged by the caller and has no effect. `fail` makes the named call return the recorded
//! error. The fake executable logs every call, one per line, to `$FAKE_HERDR_DIR/log`.
//!
//! # Extensions used by the scenario builder
//!
//! * `then`: a list of further stages. The fields of the rule itself are the first stage; each
//!   entry of `then` has `after_polls`, `append`, `churn_polls`, `foreground`, `remove_in_cwd`,
//!   `clear` and the fields below, and counts its reads from the moment the rule fired.
//! * `churn_line`, `churn_start`, `churn_step`: the text of a churn line, with `{n}` replaced by
//!   `churn_start + churn_step * (index)`. The default line is `working... {n}`; a counting
//!   script uses `{n}` alone so every read shows one more number.
//! * `stop_churn`: stop every running churn of this pane (a key that interrupts a turn).
//! * `wipe`: drop the whole text of the pane, the recent text included.
//! * `remember`: a template pushed to a list shared by all panes when the rule fires. In any
//!   template `${mem}` is every remembered value joined by newlines, `${mem:N}` the value at
//!   index N and `${mem:last}` the newest one; `$$` is a dollar sign.
//! * In `fail` entries: `message` (default: the recorded message for the code, or the code),
//!   `pane` (`first` or `second`, only for calls that name a pane), `after` (let this many
//!   matching calls through first) and `times` (stop failing after this many failures).
//!
//! The call names of a `fail` entry are `tab create`, `tab list`, `tab close`, `pane split`,
//! `pane run`, `pane send-text`, `pane send-keys`, `pane read`, `pane process-info` and `version`.
//!
//! The phase 2a suite adds three script features. A rule with `on: "paste"` fires on text the
//! fake paster feeds the machine, and matching runs over the pasted text. A stage with `get`
//! sets the text `pane get` reports for the pane from then on. A stage with `write_in_cwd`
//! (`{path, text}`) writes the file under the pane's working folder, for the marker file the
//! wrapped talaria would write on exit.
//!
//! The phase 2b suite adds two `write_in_cwd` features. Its `path` expands `$<n>` captures and
//! remembered values like any other template, and an optional numeric `mode` sets the file mode
//! after writing, for the exported file the rename step checks.
//!
//! The phase 3 suite adds two fields that use the scratch root instead of the pane's working
//! folder. `write_in_scratch` is `{path, text}` and writes that file under the scratch root.
//! `remove_in_scratch` deletes a path under the scratch root. Both follow `write_in_cwd` and
//! `remove_in_cwd`: the path is expanded, an absolute path is ignored, and `remove_in_scratch`
//! also ignores a path that would leave the root. The scratch root is the parent of the pane's
//! working folder when that folder is named `a` (the session folder the suite opens); any other
//! working folder is its own root, so a script that never sets these fields is unchanged. A
//! scripted shell line is a `run` rule whose `matches` names the line's output file. The rule
//! writes that file with `write_in_scratch` and appends its marker with `append: "SMOKE_EXIT:<n>\n"`.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use regex::Regex;
use serde_json::Value;

/// Rows of a pane; the visible text is the last this many lines.
pub const VISIBLE_ROWS: usize = 44;

/// The pane that is the root of the tab.
pub const FIRST: &str = "first";
/// The pane that a split makes.
pub const SECOND: &str = "second";

/// The ids the recorded fixtures use, shared by both fakes.
pub const WORKSPACE_ID: &str = "w1";
pub const TAB_ID: &str = "w1:t5";
pub const FIRST_PANE_ID: &str = "w1:p1G";
pub const SECOND_PANE_ID: &str = "w1:p2H";

/// The pane name behind a pane id of the fixtures.
pub fn pane_name(id: &str) -> Option<&'static str> {
    match id {
        FIRST_PANE_ID => Some(FIRST),
        SECOND_PANE_ID => Some(SECOND),
        _ => None,
    }
}

/// The kind of typed input a rule listens to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Run,
    SendText,
    SendKeys,
    Paste,
}

impl Input {
    pub fn parse(text: &str) -> Result<Input, String> {
        match text {
            "run" => Ok(Input::Run),
            "send_text" => Ok(Input::SendText),
            "send_keys" => Ok(Input::SendKeys),
            "paste" => Ok(Input::Paste),
            other => Err(format!("unknown input kind '{other}'")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Input::Run => "run",
            Input::SendText => "send_text",
            Input::SendKeys => "send_keys",
            Input::Paste => "paste",
        }
    }
}

/// One group of effects of a rule, due after some reads of the pane.
#[derive(Clone, Debug)]
pub struct Stage {
    pub after_polls: u32,
    pub append: Option<String>,
    pub churn_polls: u32,
    pub churn_line: String,
    pub churn_start: i64,
    pub churn_step: i64,
    pub foreground: Option<Vec<String>>,
    pub remove_in_cwd: Option<String>,
    pub clear: bool,
    pub wipe: bool,
    pub stop_churn: bool,
    pub write_in_cwd: Option<WriteFile>,
    pub write_in_scratch: Option<WriteFile>,
    pub remove_in_scratch: Option<String>,
    pub get: Option<String>,
}

/// A file a stage writes under the pane's working folder.
#[derive(Clone, Debug)]
pub struct WriteFile {
    /// The path under the working folder.
    pub path: String,
    /// The content.
    pub text: String,
    /// The file mode to set after writing, when the script names one.
    pub mode: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub on: Input,
    pub matches: Regex,
    pub once: bool,
    pub remember: Option<String>,
    /// The rule's own fields first, then each `then` entry.
    pub stages: Vec<Stage>,
}

#[derive(Clone, Debug)]
pub struct PaneScript {
    pub initial: String,
    pub foreground: Vec<String>,
    pub rules: Vec<Rule>,
}

impl Default for PaneScript {
    fn default() -> Self {
        PaneScript {
            initial: "$ ".to_owned(),
            foreground: vec!["-bash".to_owned()],
            rules: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FailEntry {
    pub call: String,
    pub code: String,
    pub message: Option<String>,
    pub pane: Option<String>,
    pub after: u32,
    pub times: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct Script {
    pub panes: BTreeMap<String, PaneScript>,
    pub fail: Vec<FailEntry>,
}

/// A planned error answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: String,
    pub message: Option<String>,
}

fn field<'a>(object: &'a Value, name: &str) -> Option<&'a Value> {
    object.get(name).filter(|v| !v.is_null())
}

fn text_field(object: &Value, name: &str) -> Result<Option<String>, String> {
    match field(object, name) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("'{name}' must be a string")),
    }
}

fn number_field(object: &Value, name: &str) -> Result<Option<i64>, String> {
    match field(object, name) {
        None => Ok(None),
        Some(v) => v
            .as_i64()
            .map(Some)
            .ok_or_else(|| format!("'{name}' must be a whole number")),
    }
}

fn bool_field(object: &Value, name: &str) -> Result<Option<bool>, String> {
    match field(object, name) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(format!("'{name}' must be true or false")),
    }
}

fn list_field(object: &Value, name: &str) -> Result<Option<Vec<String>>, String> {
    match field(object, name) {
        None => Ok(None),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("'{name}' must hold strings"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(format!("'{name}' must be a list")),
    }
}

fn parse_stage(object: &Value) -> Result<Stage, String> {
    let count = |name: &str| -> Result<u32, String> {
        Ok(number_field(object, name)?
            .unwrap_or(0)
            .clamp(0, i64::from(u32::MAX)) as u32)
    };
    Ok(Stage {
        after_polls: count("after_polls")?,
        append: text_field(object, "append")?,
        churn_polls: count("churn_polls")?,
        churn_line: text_field(object, "churn_line")?
            .unwrap_or_else(|| "working... {n}".to_owned()),
        churn_start: number_field(object, "churn_start")?.unwrap_or(1),
        churn_step: number_field(object, "churn_step")?.unwrap_or(1),
        foreground: list_field(object, "foreground")?,
        remove_in_cwd: text_field(object, "remove_in_cwd")?,
        clear: bool_field(object, "clear")?.unwrap_or(false),
        wipe: bool_field(object, "wipe")?.unwrap_or(false),
        stop_churn: bool_field(object, "stop_churn")?.unwrap_or(false),
        write_in_cwd: parse_write_in_cwd(object)?,
        write_in_scratch: parse_write_in_scratch(object)?,
        remove_in_scratch: text_field(object, "remove_in_scratch")?,
        get: text_field(object, "get")?,
    })
}

fn parse_write_in_cwd(object: &Value) -> Result<Option<WriteFile>, String> {
    let Some(entry) = field(object, "write_in_cwd") else {
        return Ok(None);
    };
    let mode = match number_field(entry, "mode")? {
        None => None,
        Some(mode) => Some(u32::try_from(mode).map_err(|_| "write_in_cwd mode is negative")?),
    };
    Ok(Some(WriteFile {
        path: text_field(entry, "path")?.ok_or("write_in_cwd needs 'path'")?,
        text: text_field(entry, "text")?.ok_or("write_in_cwd needs 'text'")?,
        mode,
    }))
}

fn parse_write_in_scratch(object: &Value) -> Result<Option<WriteFile>, String> {
    let Some(entry) = field(object, "write_in_scratch") else {
        return Ok(None);
    };
    Ok(Some(WriteFile {
        path: text_field(entry, "path")?.ok_or("write_in_scratch needs 'path'")?,
        text: text_field(entry, "text")?.ok_or("write_in_scratch needs 'text'")?,
        mode: None,
    }))
}

static REGEX_CACHE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::BTreeMap<String, Regex>>,
> = std::sync::OnceLock::new();

fn cached_regex(pattern: &str) -> Result<Regex, String> {
    let cache =
        REGEX_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::BTreeMap::new()));
    let mut map = cache
        .lock()
        .map_err(|_| "regex cache lock poisoned".to_owned())?;
    if let Some(found) = map.get(pattern) {
        return Ok(found.clone());
    }
    let compiled = Regex::new(pattern).map_err(|e| format!("bad regular expression: {e}"))?;
    map.insert(pattern.to_owned(), compiled.clone());
    Ok(compiled)
}

fn parse_rule(object: &Value) -> Result<Rule, String> {
    let on = Input::parse(&text_field(object, "on")?.ok_or("a rule needs 'on'")?)?;
    let pattern = text_field(object, "matches")?.ok_or("a rule needs 'matches'")?;
    let matches = cached_regex(&pattern)?;
    let mut stages = vec![parse_stage(object)?];
    if let Some(Value::Array(more)) = field(object, "then") {
        for entry in more {
            stages.push(parse_stage(entry)?);
        }
    }
    Ok(Rule {
        on,
        matches,
        once: bool_field(object, "once")?.unwrap_or(true),
        remember: text_field(object, "remember")?,
        stages,
    })
}

impl Script {
    /// A script with two plain shell panes and no rules.
    pub fn plain() -> Script {
        let mut script = Script::default();
        script.panes.insert(FIRST.to_owned(), PaneScript::default());
        script
            .panes
            .insert(SECOND.to_owned(), PaneScript::default());
        script
    }

    pub fn parse(json: &str) -> Result<Script, String> {
        let root: Value = serde_json::from_str(json).map_err(|e| format!("bad script: {e}"))?;
        Script::from_value(&root)
    }

    pub fn from_value(root: &Value) -> Result<Script, String> {
        let mut script = Script::plain();
        if let Some(Value::Object(panes)) = field(root, "panes") {
            for (name, pane) in panes {
                let mut parsed = PaneScript::default();
                if let Some(initial) = text_field(pane, "initial")? {
                    parsed.initial = initial;
                }
                if let Some(foreground) = list_field(pane, "foreground")? {
                    parsed.foreground = foreground;
                }
                if let Some(Value::Array(rules)) = field(pane, "rules") {
                    for rule in rules {
                        parsed.rules.push(parse_rule(rule)?);
                    }
                }
                script.panes.insert(name.clone(), parsed);
            }
        }
        if let Some(Value::Array(entries)) = field(root, "fail") {
            for entry in entries {
                script.fail.push(FailEntry {
                    call: text_field(entry, "call")?.ok_or("a fail entry needs 'call'")?,
                    code: text_field(entry, "code")?.ok_or("a fail entry needs 'code'")?,
                    message: text_field(entry, "message")?,
                    pane: text_field(entry, "pane")?,
                    after: number_field(entry, "after")?.unwrap_or(0).max(0) as u32,
                    times: number_field(entry, "times")?.map(|t| t.max(0) as u32),
                });
            }
        }
        Ok(script)
    }
}

struct StageRun {
    stage: Stage,
    applied: bool,
    churned: u32,
}

struct Fired {
    caps: Vec<String>,
    reads: u32,
    stages: Vec<StageRun>,
}

struct PaneState {
    open: bool,
    text: String,
    /// Byte offset in `text` where the visible screen starts.
    cleared: usize,
    foreground: Vec<String>,
    cwd: PathBuf,
    used: Vec<bool>,
    fired: Vec<Fired>,
    /// The text `pane get` reports; rules change it with `get`.
    pane_get: String,
}

/// The state machine behind both fakes. It is deterministic: the same calls in the same order give
/// the same screens, which is how the stateful executable rebuilds it on every invocation.
pub struct Machine {
    script: Script,
    touch_fs: bool,
    panes: BTreeMap<String, PaneState>,
    memory: Vec<String>,
    /// For each `fail` entry: matching calls seen and failures given.
    seen: Vec<(u32, u32)>,
}

impl Machine {
    /// `touch_fs` is false when replaying earlier events: files were already removed then.
    pub fn new(script: Script, touch_fs: bool) -> Machine {
        let panes = script
            .panes
            .iter()
            .map(|(name, pane)| {
                (
                    name.clone(),
                    PaneState {
                        open: false,
                        text: String::new(),
                        cleared: 0,
                        foreground: pane.foreground.clone(),
                        cwd: PathBuf::new(),
                        used: vec![false; pane.rules.len()],
                        fired: Vec::new(),
                        pane_get: String::new(),
                    },
                )
            })
            .collect();
        let seen = vec![(0, 0); script.fail.len()];
        Machine {
            script,
            touch_fs,
            panes,
            memory: Vec::new(),
            seen,
        }
    }

    /// Registers a call and says whether it must fail.
    pub fn call(&mut self, call: &str, pane: Option<&str>) -> Option<Failure> {
        let mut answer = None;
        for (index, entry) in self.script.fail.iter().enumerate() {
            if entry.call != call {
                continue;
            }
            if let Some(wanted) = &entry.pane
                && Some(wanted.as_str()) != pane
            {
                continue;
            }
            let (seen, given) = &mut self.seen[index];
            *seen += 1;
            let due = *seen > entry.after && entry.times.is_none_or(|limit| *given < limit);
            if due && answer.is_none() {
                *given += 1;
                answer = Some(Failure {
                    code: entry.code.clone(),
                    message: entry.message.clone(),
                });
            }
        }
        answer
    }

    /// Makes the pane exist with its initial text; `cwd` is its working folder.
    pub fn open_pane(&mut self, name: &str, cwd: &Path) {
        let definition = self.script.panes.get(name).cloned().unwrap_or_default();
        let used = vec![false; definition.rules.len()];
        let state = self
            .panes
            .entry(name.to_owned())
            .or_insert_with(|| PaneState {
                open: false,
                text: String::new(),
                cleared: 0,
                foreground: Vec::new(),
                cwd: PathBuf::new(),
                used: Vec::new(),
                fired: Vec::new(),
                pane_get: String::new(),
            });
        state.open = true;
        state.text = definition.initial.clone();
        state.cleared = 0;
        state.foreground = definition.foreground.clone();
        state.cwd = cwd.to_path_buf();
        state.used = used;
        state.fired.clear();
    }

    pub fn is_open(&self, name: &str) -> bool {
        self.panes.get(name).is_some_and(|p| p.open)
    }

    pub fn set_touch_fs(&mut self, on: bool) {
        self.touch_fs = on;
    }

    /// Closes every pane (the tab was closed).
    pub fn close_all(&mut self) {
        for pane in self.panes.values_mut() {
            pane.open = false;
        }
    }

    pub fn cwd(&self, name: &str) -> PathBuf {
        self.panes
            .get(name)
            .map(|p| p.cwd.clone())
            .unwrap_or_default()
    }

    /// Delivers typed input; true when a rule fired.
    pub fn typed(&mut self, name: &str, input: Input, text: &str) -> bool {
        let Some(definition) = self.script.panes.get(name).cloned() else {
            return false;
        };
        let Some(state) = self.panes.get(name) else {
            return false;
        };
        let mut chosen = None;
        for (index, rule) in definition.rules.iter().enumerate() {
            if rule.on != input || (rule.once && state.used[index]) {
                continue;
            }
            if let Some(found) = rule.matches.captures(text) {
                // `$0` is the whole typed text, not just the part that matched.
                let mut caps = vec![text.to_owned()];
                caps.extend((1..found.len()).map(|i| {
                    found
                        .get(i)
                        .map(|m| m.as_str().to_owned())
                        .unwrap_or_default()
                }));
                chosen = Some((index, caps));
                break;
            }
        }
        let Some((index, caps)) = chosen else {
            return false;
        };
        let rule = &definition.rules[index];
        if let Some(state) = self.panes.get_mut(name) {
            state.used[index] = true;
        }
        if let Some(template) = &rule.remember {
            let value = self.expand(template, &caps);
            self.memory.push(value);
        }
        let stages: Vec<StageRun> = rule
            .stages
            .iter()
            .map(|stage| StageRun {
                stage: stage.clone(),
                applied: false,
                churned: 0,
            })
            .collect();
        if let Some(state) = self.panes.get_mut(name) {
            state.fired.push(Fired {
                caps,
                reads: 0,
                stages,
            });
        }
        self.run_due_stages(name);
        true
    }

    /// One read of the pane is about to be answered: effects that were due appear, churn lines
    /// are added, and the pane's clock moves on.
    pub fn tick_read(&mut self, name: &str) {
        self.run_due_stages(name);
        let Some(state) = self.panes.get_mut(name) else {
            return;
        };
        let mut lines = Vec::new();
        for fired in &mut state.fired {
            for run in &mut fired.stages {
                if run.churned < run.stage.churn_polls {
                    let n = run.stage.churn_start + run.stage.churn_step * i64::from(run.churned);
                    lines.push(run.stage.churn_line.replace("{n}", &n.to_string()));
                    run.churned += 1;
                }
            }
            fired.reads += 1;
        }
        for line in lines {
            push_line(state, &line);
        }
    }

    fn run_due_stages(&mut self, name: &str) {
        let mut due: Vec<(Stage, Vec<String>)> = Vec::new();
        if let Some(state) = self.panes.get_mut(name) {
            for fired in &mut state.fired {
                for run in &mut fired.stages {
                    if !run.applied && fired.reads >= run.stage.after_polls {
                        run.applied = true;
                        due.push((run.stage.clone(), fired.caps.clone()));
                    }
                }
            }
        }
        for (stage, caps) in due {
            self.apply(name, &stage, &caps);
        }
    }

    fn apply(&mut self, name: &str, stage: &Stage, caps: &[String]) {
        let appended = stage.append.as_ref().map(|t| self.expand(t, caps));
        let pane_get = stage.get.as_ref().map(|t| self.expand(t, caps));
        let written = stage.write_in_cwd.as_ref().map(|file| {
            (
                self.expand(&file.path, caps),
                self.expand(&file.text, caps),
                file.mode,
            )
        });
        let scratch_written = stage.write_in_scratch.as_ref().map(|file| {
            (
                self.expand(&file.path, caps),
                self.expand(&file.text, caps),
                file.mode,
            )
        });
        let scratch_removed = stage
            .remove_in_scratch
            .as_ref()
            .map(|path| self.expand(path, caps));
        let foreground = stage.foreground.as_ref().map(|list| {
            list.iter()
                .map(|t| self.expand(t, caps))
                .collect::<Vec<_>>()
        });
        let initial = self.script.panes.get(name).map(|p| p.foreground.clone());
        let touch_fs = self.touch_fs;
        let Some(state) = self.panes.get_mut(name) else {
            return;
        };
        if stage.wipe {
            state.text.clear();
            state.cleared = 0;
        }
        if stage.clear {
            state.cleared = state.text.len();
        }
        if let Some(list) = foreground {
            state.foreground = if list.is_empty() {
                initial.unwrap_or_default()
            } else {
                list
            };
        }
        if stage.stop_churn {
            for fired in &mut state.fired {
                for run in &mut fired.stages {
                    run.churned = run.stage.churn_polls;
                }
            }
        }
        if let (true, Some(relative)) = (touch_fs, &stage.remove_in_cwd) {
            remove_under(&state.cwd, relative);
        }
        if let (true, Some((path, text, mode))) = (touch_fs, &written) {
            write_under(&state.cwd, path, text, *mode);
        }
        let scratch = scratch_root(&state.cwd);
        if let (true, Some(relative)) = (touch_fs, &scratch_removed) {
            remove_under(&scratch, relative);
        }
        if let (true, Some((path, text, mode))) = (touch_fs, &scratch_written) {
            write_under(&scratch, path, text, *mode);
        }
        if let Some(text) = pane_get {
            state.pane_get = text;
        }
        if let Some(text) = appended {
            push_line(state, &text);
        }
    }

    fn expand(&self, template: &str, caps: &[String]) -> String {
        let mut out = String::new();
        let chars: Vec<char> = template.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] != '$' {
                out.push(chars[i]);
                i += 1;
                continue;
            }
            match chars.get(i + 1) {
                Some('$') => {
                    out.push('$');
                    i += 2;
                }
                Some(d) if d.is_ascii_digit() => {
                    let n = d.to_digit(10).unwrap_or(0) as usize;
                    out.push_str(caps.get(n).map_or("", String::as_str));
                    i += 2;
                }
                Some('{') => {
                    let rest: String = chars[i + 2..].iter().collect();
                    if let Some(end) = rest.find('}') {
                        let inner = &rest[..end];
                        out.push_str(&self.memory_value(inner));
                        i += 2 + rest[..=end].chars().count();
                    } else {
                        out.push('$');
                        i += 1;
                    }
                }
                _ => {
                    out.push('$');
                    i += 1;
                }
            }
        }
        out
    }

    fn memory_value(&self, inner: &str) -> String {
        match inner {
            "mem" => self.memory.join("\n"),
            "mem:last" => self.memory.last().cloned().unwrap_or_default(),
            other => other
                .strip_prefix("mem:")
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| self.memory.get(n).cloned())
                .unwrap_or_default(),
        }
    }

    /// The last 44 lines since the last clear (or the last `lines`, if fewer).
    pub fn visible(&self, name: &str, lines: Option<usize>) -> String {
        let Some(state) = self.panes.get(name) else {
            return String::new();
        };
        let shown = &state.text[state.cleared..];
        join_tail(shown, lines.map_or(VISIBLE_ROWS, |n| n.min(VISIBLE_ROWS)))
    }

    /// All of the text, or its last `lines` lines.
    pub fn recent(&self, name: &str, lines: Option<usize>) -> String {
        let Some(state) = self.panes.get(name) else {
            return String::new();
        };
        join_tail(&state.text, lines.unwrap_or(usize::MAX))
    }

    pub fn foreground(&self, name: &str) -> Vec<String> {
        self.panes
            .get(name)
            .map(|p| p.foreground.clone())
            .unwrap_or_default()
    }

    /// The text `pane get` reports for the pane; rules change it with `get`.
    pub fn pane_get_text(&self, name: &str) -> String {
        self.panes
            .get(name)
            .map(|p| p.pane_get.clone())
            .unwrap_or_default()
    }

    /// Every typed text that fired a rule on the pane, oldest first.
    pub fn typed_texts(&self, name: &str) -> Vec<String> {
        self.panes
            .get(name)
            .map(|p| {
                p.fired
                    .iter()
                    .filter_map(|f| f.caps.first().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Pushes a value to the remembered list (the fake paster notes dropped-file tokens).
    pub fn note(&mut self, value: String) {
        self.memory.push(value);
    }

    /// The remembered values, oldest first.
    pub fn memory(&self) -> &[String] {
        &self.memory
    }
}

fn push_line(state: &mut PaneState, text: &str) {
    if !state.text.is_empty() && !state.text.ends_with('\n') {
        state.text.push('\n');
    }
    state.text.push_str(text);
}

fn join_tail(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(count);
    let mut out = lines[start..].join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// The scratch root for a pane. The suite's session folder is named `a`, and the scratch
/// root is its parent. Any other working folder is used as its own root.
fn scratch_root(cwd: &Path) -> PathBuf {
    if cwd.file_name().is_some_and(|name| name == "a") {
        cwd.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| cwd.to_path_buf())
    } else {
        cwd.to_path_buf()
    }
}

/// Removes a relative path under `cwd`; anything that could leave the folder is ignored.
fn remove_under(cwd: &Path, relative: &str) {
    let path = Path::new(relative);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return;
    }
    let target = cwd.join(path);
    if target.is_dir() {
        let _ = std::fs::remove_dir_all(&target);
    } else {
        let _ = std::fs::remove_file(&target);
    }
}

/// Writes `text` to `relative` under `cwd`, creating parent folders. Absolute paths are
/// ignored, like [`remove_under`]; relative paths may ascend with `..` (the headless
/// outputs land in the scratch root, one folder above the tab's working folder).
fn write_under(cwd: &Path, relative: &str, text: &str, mode: Option<u32>) {
    let path = Path::new(relative);
    if path.components().any(|c| {
        !matches!(
            c,
            Component::Normal(_) | Component::CurDir | Component::ParentDir
        )
    }) {
        return;
    }
    let target = cwd.join(path);
    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&target, text);
    if let Some(mode) = mode {
        let _ = fs::set_permissions(&target, fs::Permissions::from_mode(mode));
    }
}
