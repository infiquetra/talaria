//! A stateful fake `herdr` executable for the binary level tests.
//!
//! It answers every call the smoke test makes from output recorded off the real `herdr`
//! (`tests/fixtures/herdr`), with the exact argument lists of the recorded table, and drives its
//! panes from the screen script described in `script.rs`.
//!
//! Environment:
//!
//! * `FAKE_HERDR_DIR`: the folder that holds `script.json` (the screen script, optional), the
//!   `log` (one JSON array of arguments per call, in order), the `events` file (what the fake
//!   needs to rebuild its state on the next call) and `cwd_at_run.txt`.
//! * `FAKE_HERDR_FIXTURES`: the folder of recorded output; by default the one in this package.
//!
//! The fake never reads `HERDR_ENV` or `HERDR_WORKSPACE_ID`. The tab is present from `tab create`
//! until `tab close`. At every `pane run`, before any rule fires, it writes `cwd_at_run.txt`: the
//! first line is the octal permission bits of the tab's working folder, then one line per entry
//! under it (paths relative to that folder, sorted; a folder ends with a slash).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

#[path = "script.rs"]
mod script;

use script::{FIRST, Failure, Input, Machine, SECOND, Script, TAB_ID};

struct Ctx {
    dir: PathBuf,
    fixtures: PathBuf,
    machine: Machine,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = std::env::var_os("FAKE_HERDR_DIR").map(PathBuf::from) else {
        eprintln!("fake-herdr: FAKE_HERDR_DIR is not set");
        return ExitCode::from(2);
    };
    let fixtures = std::env::var_os("FAKE_HERDR_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/herdr"));
    let script = match fs::read_to_string(dir.join("script.json")) {
        Ok(text) => match Script::parse(&text) {
            Ok(script) => script,
            Err(e) => {
                eprintln!("fake-herdr: {e}");
                return ExitCode::from(2);
            }
        },
        Err(_) => Script::plain(),
    };
    append_line(&dir.join("log"), &json!(args));
    let mut machine = Machine::new(script, false);
    replay(&dir, &mut machine);
    machine.set_touch_fs(true);
    let mut ctx = Ctx {
        dir,
        fixtures,
        machine,
    };
    match handle(&mut ctx, &args) {
        Ok(out) => {
            print!("{out}");
            let _ = std::io::stdout().flush();
            ExitCode::SUCCESS
        }
        Err(Stop::Server(code, message)) => {
            eprintln!("{}", ctx.error_json(&code, message.as_deref()));
            ExitCode::from(1)
        }
        Err(Stop::Syntax(message)) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

enum Stop {
    /// A server error: exit 1 with JSON on stderr.
    Server(String, Option<String>),
    /// A syntax error: exit 2.
    Syntax(String),
}

fn append_line(path: &Path, value: &Value) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{value}");
    }
}

fn replay(dir: &Path, machine: &mut Machine) {
    let Ok(text) = fs::read_to_string(dir.join("events")) else {
        return;
    };
    for line in text.lines() {
        let Ok(Value::Array(event)) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let part = |i: usize| event.get(i).and_then(Value::as_str).unwrap_or("");
        match part(0) {
            "call" => {
                let pane = event.get(2).and_then(Value::as_str);
                let _ = machine.call(part(1), pane);
            }
            "open" => machine.open_pane(part(1), Path::new(part(2))),
            "typed" => {
                if let Ok(input) = Input::parse(part(2)) {
                    machine.typed(part(1), input, part(3));
                }
            }
            "read" => machine.tick_read(part(1)),
            "closed" => machine.close_all(),
            _ => {}
        }
    }
}

impl Ctx {
    fn event(&self, event: Value) {
        append_line(&self.dir.join("events"), &event);
    }

    /// Registers the call; an error answer if the script says it must fail.
    fn call(&mut self, name: &str, pane: Option<&str>) -> Result<(), Stop> {
        self.event(json!(["call", name, pane]));
        match self.machine.call(name, pane) {
            Some(Failure { code, message }) => Err(Stop::Server(code, message)),
            None => Ok(()),
        }
    }

    fn fixture(&self, name: &str) -> String {
        fs::read_to_string(self.fixtures.join(name)).unwrap_or_default()
    }

    fn error_json(&self, code: &str, message: Option<&str>) -> String {
        let recorded = self.fixture(&format!("error_{code}.json"));
        if let Ok(mut doc) = serde_json::from_str::<Value>(&recorded) {
            if let Some(message) = message {
                doc["error"]["message"] = json!(message);
            }
            return doc.to_string();
        }
        json!({"error": {"code": code, "message": message.unwrap_or(code)}, "id": "cli:request"})
            .to_string()
    }

    fn tab_open(&self) -> bool {
        self.machine.is_open(FIRST)
    }

    /// The pane named by an id, which must exist.
    fn pane(&self, id: &str) -> Result<&'static str, Stop> {
        match script::pane_name(id) {
            Some(name) if self.machine.is_open(name) => Ok(name),
            _ => Err(Stop::Server(
                "pane_not_found".to_owned(),
                Some(format!("pane {id} not found")),
            )),
        }
    }
}

/// Arguments split into positionals (which must come first) and options.
struct Parsed {
    positionals: Vec<String>,
    options: Vec<(String, Option<String>)>,
}

/// `flags` are options without a value, `valued` take one. `required` names must be present.
fn parse(
    rest: &[String],
    positionals: std::ops::RangeInclusive<usize>,
    valued: &[&str],
    flags: &[&str],
    required: &[&str],
) -> Result<Parsed, Stop> {
    let mut parsed = Parsed {
        positionals: Vec::new(),
        options: Vec::new(),
    };
    let mut i = 0;
    while i < rest.len() {
        let arg = &rest[i];
        if let Some(name) = arg.strip_prefix("--") {
            let name = format!("--{name}");
            if parsed.options.iter().any(|(n, _)| *n == name) {
                return Err(Stop::Syntax(format!("option {name} given twice")));
            }
            if valued.contains(&name.as_str()) {
                let Some(value) = rest.get(i + 1) else {
                    return Err(Stop::Syntax(format!("option {name} needs a value")));
                };
                parsed.options.push((name, Some(value.clone())));
                i += 2;
            } else if flags.contains(&name.as_str()) {
                parsed.options.push((name, None));
                i += 1;
            } else {
                return Err(Stop::Syntax(format!("unknown option {name}")));
            }
        } else {
            if !parsed.options.is_empty() {
                return Err(Stop::Syntax(format!(
                    "positional argument '{arg}' after options"
                )));
            }
            parsed.positionals.push(arg.clone());
            i += 1;
        }
    }
    if !positionals.contains(&parsed.positionals.len()) {
        return Err(Stop::Syntax(format!(
            "expected {}..={} positional arguments, got {}",
            positionals.start(),
            positionals.end(),
            parsed.positionals.len()
        )));
    }
    for name in required {
        if !parsed.options.iter().any(|(n, _)| n == name) {
            return Err(Stop::Syntax(format!("missing option {name}")));
        }
    }
    Ok(parsed)
}

impl Parsed {
    fn value(&self, name: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, v)| v.as_deref())
    }
}

const KEY_NAMES: &[&str] = &[
    "enter",
    "esc",
    "escape",
    "tab",
    "space",
    "backspace",
    "delete",
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "pageup",
    "pagedown",
];

fn key_is_known(key: &str) -> bool {
    if KEY_NAMES.contains(&key) {
        return true;
    }
    if key.chars().count() == 1 {
        return true;
    }
    if let Some((modifier, rest)) = key.split_once('+') {
        return matches!(modifier, "ctrl" | "alt" | "shift") && key_is_known(rest);
    }
    key.strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n))
}

fn handle(ctx: &mut Ctx, args: &[String]) -> Result<String, Stop> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["--version"] => {
            ctx.call("version", None)?;
            Ok(ctx.fixture("version.txt"))
        }
        ["tab", "create", ..] => tab_create(ctx, &args[2..]),
        ["tab", "list", ..] => {
            let parsed = parse(&args[2..], 0..=0, &["--workspace"], &[], &["--workspace"])?;
            let _ = parsed;
            ctx.call("tab list", None)?;
            Ok(ctx.fixture(if ctx.tab_open() {
                "tab_list.json"
            } else {
                "tab_list_without.json"
            }))
        }
        ["tab", "close", ..] => {
            let parsed = parse(&args[2..], 1..=1, &[], &[], &[])?;
            ctx.call("tab close", None)?;
            let tab = &parsed.positionals[0];
            if tab != TAB_ID || !ctx.tab_open() {
                return Err(Stop::Server(
                    "tab_not_found".to_owned(),
                    Some(format!("tab {tab} not found")),
                ));
            }
            ctx.machine.close_all();
            ctx.event(json!(["closed"]));
            Ok(ctx.fixture("tab_close_ok.json"))
        }
        ["pane", "split", ..] => pane_split(ctx, &args[2..]),
        ["pane", "run", ..] => pane_type(ctx, &args[2..], Input::Run),
        ["pane", "send-text", ..] => pane_type(ctx, &args[2..], Input::SendText),
        ["pane", "send-keys", ..] => pane_keys(ctx, &args[2..]),
        ["pane", "read", ..] => pane_read(ctx, &args[2..]),
        ["pane", "process-info", ..] => pane_info(ctx, &args[2..]),
        ["pane", "get", ..] => pane_get(ctx, &args[2..]),
        _ => Err(Stop::Syntax(format!("unknown command: {}", args.join(" ")))),
    }
}

fn tab_create(ctx: &mut Ctx, rest: &[String]) -> Result<String, Stop> {
    let parsed = parse(
        rest,
        0..=0,
        &["--workspace", "--cwd", "--label"],
        &["--no-focus"],
        &["--workspace", "--cwd", "--label", "--no-focus"],
    )?;
    ctx.call("tab create", None)?;
    let cwd = parsed.value("--cwd").unwrap_or_default().to_owned();
    ctx.machine.open_pane(FIRST, Path::new(&cwd));
    ctx.event(json!(["open", FIRST, cwd]));
    Ok(ctx.fixture("tab_create.json"))
}

fn pane_split(ctx: &mut Ctx, rest: &[String]) -> Result<String, Stop> {
    let parsed = parse(
        rest,
        1..=1,
        &["--direction", "--cwd"],
        &["--no-focus"],
        &["--direction", "--cwd", "--no-focus"],
    )?;
    if parsed.value("--direction") != Some("right") && parsed.value("--direction") != Some("down") {
        return Err(Stop::Syntax("--direction must be right or down".to_owned()));
    }
    let pane = ctx.pane(&parsed.positionals[0])?;
    ctx.call("pane split", Some(pane))?;
    let cwd = parsed.value("--cwd").unwrap_or_default().to_owned();
    ctx.machine.open_pane(SECOND, Path::new(&cwd));
    ctx.event(json!(["open", SECOND, cwd]));
    Ok(ctx.fixture("pane_split.json"))
}

fn pane_type(ctx: &mut Ctx, rest: &[String], input: Input) -> Result<String, Stop> {
    let parsed = parse(rest, 2..=2, &[], &[], &[])?;
    let pane = ctx.pane(&parsed.positionals[0])?;
    let text = &parsed.positionals[1];
    let (call, fixture) = match input {
        Input::Run => ("pane run", "pane_run_ok.txt"),
        _ => ("pane send-text", "send_text_ok.txt"),
    };
    ctx.call(call, Some(pane))?;
    if input == Input::Run {
        write_cwd_at_run(ctx);
    }
    ctx.machine.typed(pane, input, text);
    ctx.event(json!(["typed", pane, input.name(), text]));
    Ok(ctx.fixture(fixture))
}

fn pane_keys(ctx: &mut Ctx, rest: &[String]) -> Result<String, Stop> {
    let parsed = parse(rest, 2..=64, &[], &[], &[])?;
    let pane = ctx.pane(&parsed.positionals[0])?;
    ctx.call("pane send-keys", Some(pane))?;
    for key in &parsed.positionals[1..] {
        if !key_is_known(key) {
            return Err(Stop::Server(
                "invalid_key".to_owned(),
                Some(format!("unsupported key {key}")),
            ));
        }
    }
    for key in &parsed.positionals[1..] {
        ctx.machine.typed(pane, Input::SendKeys, key);
        ctx.event(json!(["typed", pane, Input::SendKeys.name(), key]));
    }
    Ok(ctx.fixture("send_keys_ok.txt"))
}

fn pane_read(ctx: &mut Ctx, rest: &[String]) -> Result<String, Stop> {
    let parsed = parse(rest, 1..=1, &["--source", "--lines"], &[], &["--source"])?;
    let pane = ctx.pane(&parsed.positionals[0])?;
    let source = parsed.value("--source").unwrap_or("recent").to_owned();
    if !matches!(
        source.as_str(),
        "visible" | "recent" | "recent-unwrapped" | "detection"
    ) {
        return Err(Stop::Syntax(format!("unknown source {source}")));
    }
    let lines = match parsed.value("--lines") {
        Some(n) => Some(
            n.parse::<usize>()
                .map_err(|_| Stop::Syntax(format!("bad --lines {n}")))?,
        ),
        None => None,
    };
    ctx.call("pane read", Some(pane))?;
    ctx.machine.tick_read(pane);
    ctx.event(json!(["read", pane]));
    Ok(if source == "visible" {
        ctx.machine.visible(pane, lines)
    } else {
        ctx.machine.recent(pane, lines)
    })
}

fn pane_get(ctx: &mut Ctx, rest: &[String]) -> Result<String, Stop> {
    let parsed = parse(rest, 1..=1, &[], &[], &[])?;
    let pane = ctx.pane(&parsed.positionals[0])?;
    ctx.call("pane get", Some(pane))?;
    // A read-only call: it changes no state, so it logs no event.
    Ok(ctx.machine.pane_get_text(pane))
}

fn pane_info(ctx: &mut Ctx, rest: &[String]) -> Result<String, Stop> {
    let parsed = parse(rest, 0..=0, &["--pane"], &[], &["--pane"])?;
    let pane = ctx.pane(parsed.value("--pane").unwrap_or_default())?;
    ctx.call("pane process-info", Some(pane))?;
    let list = ctx.machine.foreground(pane);
    let mut doc: Value = serde_json::from_str(&ctx.fixture("pane_process_info_shell.json"))
        .map_err(|e| Stop::Syntax(format!("bad shell fixture: {e}")))?;
    let template = doc["result"]["process_info"]["foreground_processes"][0].clone();
    let entries: Vec<Value> = list
        .iter()
        .enumerate()
        .map(|(i, cmdline)| {
            if cmdline == "-bash" {
                return template.clone();
            }
            let argv: Vec<&str> = cmdline.split_whitespace().collect();
            let argv0 = argv.first().copied().unwrap_or("");
            let name = argv0
                .rsplit('/')
                .next()
                .unwrap_or("")
                .trim_start_matches('-');
            let mut entry = template.clone();
            entry["argv"] = json!(argv);
            entry["argv0"] = json!(argv0);
            entry["cmdline"] = json!(cmdline);
            entry["name"] = json!(name);
            entry["pid"] = json!(4242 + i);
            entry
        })
        .collect();
    doc["result"]["process_info"]["foreground_processes"] = Value::Array(entries);
    doc["result"]["process_info"]["pane_id"] = json!(parsed.value("--pane"));
    Ok(format!("{doc}\n"))
}

fn write_cwd_at_run(ctx: &Ctx) {
    let cwd = ctx.machine.cwd(FIRST);
    let mode = fs::metadata(&cwd)
        .map(|m| format!("{:o}", m.permissions().mode() & 0o777))
        .unwrap_or_else(|_| "missing".to_owned());
    let mut entries = Vec::new();
    walk(&cwd, &cwd, &mut entries);
    entries.sort();
    let mut text = format!("{mode}\n");
    for entry in entries {
        text.push_str(&entry);
        text.push('\n');
    }
    let _ = fs::write(ctx.dir.join("cwd_at_run.txt"), text);
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.is_dir() {
            out.push(format!("{relative}/"));
            walk(root, &path, out);
        } else {
            out.push(relative);
        }
    }
}
