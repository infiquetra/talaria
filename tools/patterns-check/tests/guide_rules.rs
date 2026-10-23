//! Structural checks for the patterns guide.
//!
//! Paths are relative to the repository root. The root is `env!("CARGO_MANIFEST_DIR")`
//! joined with `../..`. `Cargo.lock` and `clippy.toml` are parsed line by line, so this
//! crate takes no TOML dependency. Detector prefixes in the public-safety test are built
//! from parts, so this file does not contain those prefixes as literals.

use std::path::PathBuf;

const GUIDE: &str = "docs/agents/patterns.md";
const LOCK: &str = "Cargo.lock";
const CLIPPY_CONFIG: &str = "clippy.toml";

const HEADINGS: &[&str] = &[
    "# Talaria patterns guide",
    "## Where code goes",
    "## Rules",
    "## Pinned versions",
    "## Interfaces not to use",
    "## Pattern 1: The application event enum and the single channel",
    "## Pattern 2: The pure reducer",
    "## Pattern 3: The effect executor",
    "## Pattern 4: A render function and its snapshot test",
    "## Pattern 5: A transport round-trip test from a recorded frame",
    "## Pattern 6: Adding an overlay",
    "## Checklist for a new unit",
];

const FENCE_TAGS: &[&str] = &["rust", "text", "toml", "bash", "console"];

const PINNED_CRATES: &[&str] = &["ratatui", "crossterm", "tokio"];

const BANNED_INTERFACE_NAMES: &[&str] = &[
    "Viewport::Inline",
    "insert_before",
    "get_cursor_position",
    "ratatui::init",
    "DefaultTerminal",
    "EventStream",
    "println!",
];

/// The eleven paths clippy must ban. Order matches the patterns-guide unit, section 5.
const BANNED_PATHS: &[&str] = &[
    "ratatui::Terminal::insert_before",
    "ratatui::Terminal::get_cursor_position",
    "ratatui::crossterm::cursor::position",
    "ratatui::init",
    "ratatui::try_init",
    "ratatui::init_with_options",
    "ratatui::try_init_with_options",
    "ratatui::run",
    "ratatui::restore",
    "ratatui::try_restore",
    "ratatui::DefaultTerminal",
];

const ESCAPE_SENTENCE: &str = concat!(
    "Only the terminal module of `talaria-tui` writes escape sequences ",
    "or changes terminal modes."
);

const GUIDE_MENTION: &str = "docs/agents/patterns.md";

#[test]
fn guide_exists_and_is_under_400_lines() {
    let text = read_repo_text(GUIDE);
    let line_count = text.bytes().filter(|byte| *byte == b'\n').count();
    assert!(
        line_count <= 399,
        "{GUIDE} has {line_count} lines by wc -l; the maximum is 399"
    );
}

#[test]
fn guide_has_the_required_sections_in_order() {
    let text = read_repo_text(GUIDE);
    let found = headings(&text);
    assert_eq!(
        found, HEADINGS,
        "{GUIDE} headings outside fenced blocks differ from the required list"
    );
}

#[test]
fn every_code_block_is_tagged_and_none_is_ignored() {
    let text = read_repo_text(GUIDE);
    check_fences(&text);
}

#[test]
fn each_pattern_has_a_compiled_example_with_assertions() {
    let text = read_repo_text(GUIDE);
    for number in 1..=6 {
        let prefix = format!("## Pattern {number}:");
        let body = section_with_prefix(&text, &prefix);
        let asserted = rust_bodies(body).into_iter().any(|block| {
            block.contains("assert!(")
                || block.contains("assert_eq!(")
                || block.contains("assert_ne!(")
        });
        assert!(
            asserted,
            "{GUIDE}: {prefix} has no rust block that calls assert!, assert_eq! or assert_ne!"
        );
    }
}

#[test]
fn pinned_versions_match_cargo_lock() {
    let guide = read_repo_text(GUIDE);
    let section = section_eq(&guide, "## Pinned versions");
    let table = pinned_rows(section);
    let locked = package_versions(&read_repo_text(LOCK));
    for name in PINNED_CRATES {
        let pinned = table
            .iter()
            .find(|(crate_name, _)| crate_name == name)
            .unwrap_or_else(|| {
                panic!("{GUIDE} pinned-versions table has no row for {name}");
            });
        let versions: Vec<&str> = locked
            .iter()
            .filter(|(package, _)| package == name)
            .map(|(_, version)| version.as_str())
            .collect();
        let count = versions.len();
        assert_eq!(count, 1, "Cargo.lock lists {name} {count} times");
        let locked_version = versions.first().copied().unwrap_or("");
        assert_eq!(
            pinned.1, locked_version,
            "{GUIDE} pins {name} {} but Cargo.lock has {locked_version}",
            pinned.1
        );
    }
}

#[test]
fn banned_interfaces_are_listed_with_reasons() {
    let guide = read_repo_text(GUIDE);
    let section = section_eq(&guide, "## Interfaces not to use");
    for name in BANNED_INTERFACE_NAMES {
        assert!(
            section.contains(name),
            "{GUIDE} section Interfaces not to use does not name {name}"
        );
    }
    assert!(
        section.contains("u16-stack-bakeoff.md"),
        "{GUIDE} section Interfaces not to use does not link u16-stack-bakeoff.md"
    );
    let mut data_rows = 0;
    for cells in table_rows(section) {
        if cells
            .first()
            .is_some_and(|cell| cell.eq_ignore_ascii_case("interface"))
        {
            continue;
        }
        data_rows += 1;
        let reason = cells.get(1).map(String::as_str).unwrap_or("");
        assert!(
            !reason.is_empty(),
            "{GUIDE} interfaces table has a row with an empty reason: {cells:?}"
        );
    }
    assert!(
        data_rows >= 6,
        "{GUIDE} interfaces table has {data_rows} data rows; six interface rows are required"
    );
}

#[test]
fn clippy_config_bans_exactly_the_verified_paths() {
    let text = read_repo_text(CLIPPY_CONFIG);
    let bans = clippy_bans(&text);
    let mut paths: Vec<&str> = bans.iter().map(|(path, _)| path.as_str()).collect();
    paths.sort_unstable();
    let mut expected: Vec<&str> = BANNED_PATHS.to_vec();
    expected.sort_unstable();
    assert_eq!(
        paths, expected,
        "{CLIPPY_CONFIG} paths differ from the eleven verified bans"
    );
    for (path, reason) in bans {
        assert!(
            reason.contains(GUIDE_MENTION),
            "{CLIPPY_CONFIG} reason for {path} does not name {GUIDE_MENTION}"
        );
    }
}

#[test]
fn escape_sequence_rule_is_stated() {
    let guide = read_repo_text(GUIDE);
    assert!(
        guide.contains(ESCAPE_SENTENCE),
        "{GUIDE} does not contain the escape-sequence rule"
    );
    let section = section_eq(&guide, "## Rules");
    let mut numbers = Vec::new();
    for cells in table_rows(section) {
        let Some(number) = cells.first().and_then(|cell| cell.parse::<u32>().ok()) else {
            continue;
        };
        let guard = cells.last().map(String::as_str).unwrap_or("");
        assert!(
            !guard.is_empty() && guard != number.to_string(),
            "{GUIDE} rules row {number} has an empty guard"
        );
        numbers.push(number);
    }
    assert_eq!(
        numbers,
        (1..=13).collect::<Vec<_>>(),
        "{GUIDE} rules table is not numbered 1 to 13"
    );
}

#[test]
fn guide_is_public_safe() {
    let terms = private_terms();
    let guide = read_repo_text(GUIDE);
    let findings = structural_findings(&guide);
    assert!(
        findings.is_empty(),
        "{GUIDE} public-safety findings:\n{}",
        findings.join("\n")
    );
    if let Some(terms) = terms {
        let hits = private_term_hits(&guide, &terms);
        assert!(
            hits.is_empty(),
            "{GUIDE} public-safety findings:\n{}",
            hits.join("\n")
        );
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_repo_text(relative: &str) -> String {
    let path = repo_root().join(relative);
    match std::fs::read_to_string(&path) {
        Ok(text) => text.replace('\r', ""),
        Err(error) => panic!("{relative} is missing or unreadable ({error})"),
    }
}

fn headings(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut open_ticks = 0usize;
    for line in text.lines() {
        let trimmed = line.trim();
        if open_ticks > 0 {
            if closes_fence(trimmed, open_ticks) {
                open_ticks = 0;
            }
            continue;
        }
        if let Some((ticks, _)) = opens_fence(trimmed) {
            open_ticks = ticks;
            continue;
        }
        if let Some(heading) = heading_line(line) {
            found.push(heading);
        }
    }
    found
}

fn heading_line(line: &str) -> Option<&str> {
    let trimmed = line.trim_end();
    if trimmed.starts_with("###") {
        return None;
    }
    if trimmed.starts_with("# ") || trimmed.starts_with("## ") {
        Some(trimmed)
    } else {
        None
    }
}

fn check_fences(text: &str) {
    let mut open: Option<(usize, String, usize)> = None;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let trimmed = line.trim();
        if let Some((ticks, tag, _)) = open.clone() {
            if closes_fence(trimmed, ticks) {
                open = None;
                continue;
            }
            if tag == "rust" {
                let body = line.trim();
                if body == "#" || body.starts_with("# ") {
                    panic!("{GUIDE}:{number}: hidden doctest line in a rust block");
                }
            }
            continue;
        }
        let Some((ticks, info)) = opens_fence(trimmed) else {
            continue;
        };
        if !FENCE_TAGS.contains(&info) {
            panic!(
                "{GUIDE}:{number}: fence tag {info:?} is not one of rust, text, toml, bash, console"
            );
        }
        open = Some((ticks, info.to_string(), number));
    }
    if let Some((_, _, number)) = open {
        panic!("{GUIDE}:{number}: fenced block is not closed");
    }
}

fn opens_fence(trimmed: &str) -> Option<(usize, &str)> {
    let ticks = trimmed
        .chars()
        .take_while(|character| *character == '`')
        .count();
    if ticks < 3 {
        return None;
    }
    let info = trimmed[ticks..].trim();
    if info.contains('`') {
        return None;
    }
    Some((ticks, info))
}

fn closes_fence(trimmed: &str, open_ticks: usize) -> bool {
    !trimmed.is_empty()
        && trimmed.chars().all(|character| character == '`')
        && trimmed.len() >= open_ticks
}

fn section_eq<'a>(text: &'a str, heading: &str) -> &'a str {
    section_where(text, heading, |line| line.trim_end() == heading)
}

fn section_with_prefix<'a>(text: &'a str, prefix: &str) -> &'a str {
    section_where(text, prefix, |line| line.trim_end().starts_with(prefix))
}

fn section_where<'a>(text: &'a str, label: &str, matches: impl Fn(&str) -> bool) -> &'a str {
    let mut offset = 0usize;
    for line in text.lines() {
        if matches(line) {
            let rest = &text[offset + line.len()..];
            let end = rest.find("\n## ").unwrap_or(rest.len());
            return &text[offset..offset + line.len() + end];
        }
        offset += line.len() + 1;
    }
    panic!("{GUIDE} is missing heading {label}");
}

fn rust_bodies(section: &str) -> Vec<String> {
    let mut bodies = Vec::new();
    let mut mode = FenceMode::Outside;
    for line in section.lines() {
        let trimmed = line.trim();
        match &mut mode {
            FenceMode::Outside => {
                if let Some((ticks, info)) = opens_fence(trimmed) {
                    mode = if info == "rust" {
                        FenceMode::Rust {
                            ticks,
                            body: String::new(),
                        }
                    } else {
                        FenceMode::Other { ticks }
                    };
                }
            }
            FenceMode::Rust { ticks, body } => {
                if closes_fence(trimmed, *ticks) {
                    bodies.push(std::mem::take(body));
                    mode = FenceMode::Outside;
                } else {
                    body.push_str(line);
                    body.push('\n');
                }
            }
            FenceMode::Other { ticks } => {
                if closes_fence(trimmed, *ticks) {
                    mode = FenceMode::Outside;
                }
            }
        }
    }
    bodies
}

enum FenceMode {
    Outside,
    Rust { ticks: usize, body: String },
    Other { ticks: usize },
}

fn pinned_rows(section: &str) -> Vec<(String, String)> {
    table_rows(section)
        .into_iter()
        .filter(|cells| cells.first().is_none_or(|cell| cell != "crate"))
        .filter_map(|cells| {
            let name = cells.first()?.clone();
            let version = cells.get(1)?.clone();
            if name.is_empty() || version.is_empty() {
                None
            } else {
                Some((name, version))
            }
        })
        .collect()
}

/// Line parse of `[[package]]` records: a `name = "..."` line, then `version = "..."`.
fn package_versions(lock_text: &str) -> Vec<(String, String)> {
    let mut packages = Vec::new();
    let mut name = None;
    for line in lock_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[[") {
            name = None;
            continue;
        }
        if let Some(value) = quoted_after(trimmed, "name = \"") {
            name = Some(value.to_string());
            continue;
        }
        if let Some(value) = quoted_after(trimmed, "version = \"")
            && let Some(package) = name.take()
        {
            packages.push((package, value.to_string()));
        }
    }
    packages
}

/// Line parse of `path = "..."` and `reason = "..."` entries.
fn clippy_bans(text: &str) -> Vec<(String, String)> {
    let mut bans = Vec::new();
    let mut pending_path = None;
    for line in text.lines() {
        if let Some(path) = quoted_after(line, "path = \"") {
            if let Some(earlier) = pending_path.take() {
                bans.push((earlier, String::new()));
            }
            pending_path = Some(path.to_string());
        }
        if let Some(reason) = quoted_after(line, "reason = \"") {
            match pending_path.take() {
                Some(path) => bans.push((path, reason.to_string())),
                None => bans.push((String::new(), reason.to_string())),
            }
        }
    }
    if let Some(path) = pending_path {
        bans.push((path, String::new()));
    }
    bans
}

fn quoted_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn table_rows(section: &str) -> Vec<Vec<String>> {
    section
        .lines()
        .filter_map(table_cells)
        .filter(|cells| !is_separator(cells))
        .collect()
}

fn table_cells(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return None;
    }
    let inner = trimmed.trim_matches('|');
    Some(
        inner
            .split('|')
            .map(|cell| cell.trim().trim_matches('`').to_string())
            .collect(),
    )
}

fn is_separator(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let bare = cell.trim_matches(':');
            !bare.is_empty() && bare.chars().all(|character| character == '-')
        })
}

fn private_terms() -> Option<Vec<String>> {
    let path = match std::env::var("PATTERNS_PRIVATE_TERMS") {
        Ok(path) => path,
        Err(std::env::VarError::NotPresent) => {
            println!("private-term check not run: PATTERNS_PRIVATE_TERMS unset");
            return None;
        }
        Err(_) => panic!("PATTERNS_PRIVATE_TERMS is set but is not text"),
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("PATTERNS_PRIVATE_TERMS file is missing or unreadable ({error})");
    });
    let terms: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_lowercase)
        .collect();
    if terms.is_empty() {
        panic!("PATTERNS_PRIVATE_TERMS file holds no term");
    }
    Some(terms)
}

fn private_term_hits(guide: &str, terms: &[String]) -> Vec<String> {
    guide
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let lower = line.to_lowercase();
            terms
                .iter()
                .any(|term| lower.contains(term))
                .then(|| format!("{GUIDE}:{}: private term", index + 1))
        })
        .collect()
}

fn structural_findings(guide: &str) -> Vec<String> {
    let mut findings = Vec::new();
    for (index, line) in guide.lines().enumerate() {
        let number = index + 1;
        for kind in line_findings(line) {
            findings.push(format!("{GUIDE}:{number}: {kind}"));
        }
    }
    findings
}

fn line_findings(line: &str) -> Vec<&'static str> {
    let mut kinds = Vec::new();
    if has_absolute_path(line) {
        kinds.push("absolute path");
    }
    if has_home_shorthand(line) {
        kinds.push("home shorthand");
    }
    if has_ticket(line) {
        kinds.push("ticket");
    }
    if has_key(line) {
        kinds.push("key");
    }
    if has_address(line) {
        kinds.push("address");
    }
    if has_profile(line) {
        kinds.push("profile name");
    }
    kinds
}

fn has_absolute_path(line: &str) -> bool {
    const PREFIXES: &[&str] = &[
        concat!("/", "Users", "/"),
        concat!("/", "home", "/"),
        concat!("/", "private", "/"),
        concat!("/", "var", "/", "folders"),
        concat!("/", "tmp", "/"),
    ];
    if PREFIXES.iter().any(|prefix| line.contains(prefix)) {
        return true;
    }
    has_windows_users(line)
}

fn has_windows_users(line: &str) -> bool {
    const FORWARD: &str = concat!(":", "/", "Users");
    const BACKWARD: &str = concat!(":", "\\", "Users");
    [FORWARD, BACKWARD].into_iter().any(|needle| {
        let mut rest = line;
        while let Some(index) = rest.find(needle) {
            let preceded_by_drive = rest[..index]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_ascii_alphabetic());
            if preceded_by_drive {
                return true;
            }
            rest = &rest[index + needle.len()..];
        }
        false
    })
}

fn has_home_shorthand(line: &str) -> bool {
    const MARKER: &str = concat!("~", "/");
    let mut rest = line;
    while let Some(index) = rest.find(MARKER) {
        let after = &rest[index + MARKER.len()..];
        if !allowed_home_dir(after) {
            return true;
        }
        rest = &rest[index + MARKER.len()..];
    }
    false
}

fn allowed_home_dir(after: &str) -> bool {
    const ALLOWED: &[&str] = &[".hermes", ".talaria"];
    ALLOWED.iter().any(|name| {
        let Some(rest) = after.strip_prefix(name) else {
            return false;
        };
        match rest.chars().next() {
            None | Some('/' | ' ' | '"' | '`' | ')' | ',' | '.') => true,
            Some(_) => false,
        }
    })
}

fn has_ticket(line: &str) -> bool {
    const PREFIX: &str = concat!("hermes-gateway-ticket", ".");
    let mut rest = line;
    while let Some(index) = rest.find(PREFIX) {
        let after = &rest[index + PREFIX.len()..];
        if !after.starts_with('<') {
            return true;
        }
        rest = after;
    }
    false
}

fn has_key(line: &str) -> bool {
    const SECRET: &str = concat!("sk", "-");
    const GHP: &str = concat!("ghp", "_");
    const PAT: &str = concat!("github", "_pat_");
    const PEM: &str = concat!("-----", "BEGIN");
    if line.contains(GHP) || line.contains(PAT) || line.contains(PEM) {
        return true;
    }
    let mut rest = line;
    while let Some(index) = rest.find(SECRET) {
        let after = &rest[index + SECRET.len()..];
        let count = after
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .count();
        if count >= 16 {
            return true;
        }
        rest = &rest[index + SECRET.len()..];
    }
    false
}

fn has_address(line: &str) -> bool {
    ipv4_violation(line) || host_suffix_violation(line)
}

fn ipv4_violation(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if let Some(end) = ipv4_at(line, index) {
            let address = &line[index..end];
            if address != "127.0.0.1" && address != "0.0.0.0" {
                return true;
            }
            index = end;
            continue;
        }
        index += 1;
    }
    false
}

fn ipv4_at(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    if start > 0 {
        let previous = bytes[start - 1];
        if previous.is_ascii_digit() || previous == b'.' {
            return None;
        }
    }
    let mut cursor = start;
    for part in 0..4 {
        let (next, _) = octet_at(line, cursor)?;
        cursor = next;
        if part < 3 {
            if bytes.get(cursor) != Some(&b'.') {
                return None;
            }
            cursor += 1;
        }
    }
    if bytes
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
    {
        return None;
    }
    Some(cursor)
}

fn octet_at(line: &str, start: usize) -> Option<(usize, u16)> {
    let bytes = line.as_bytes();
    if start >= bytes.len() || !bytes[start].is_ascii_digit() {
        return None;
    }
    let mut value: u16 = 0;
    let mut cursor = start;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        if cursor - start == 3 {
            return None;
        }
        value = value * 10 + u16::from(bytes[cursor] - b'0');
        cursor += 1;
    }
    if value > 255 {
        return None;
    }
    Some((cursor, value))
}

fn host_suffix_violation(line: &str) -> bool {
    const SUFFIXES: &[&str] = &[".local", ".lan", ".internal"];
    SUFFIXES.iter().any(|suffix| {
        let mut rest = line;
        while let Some(index) = rest.find(suffix) {
            let after = &rest[index + suffix.len()..];
            let boundary = after
                .chars()
                .next()
                .is_none_or(|character| !character.is_ascii_alphanumeric());
            let word = rest[..index]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_ascii_alphanumeric() || character == '-');
            if boundary && word {
                return true;
            }
            rest = &rest[index + suffix.len()..];
        }
        false
    })
}

fn has_profile(line: &str) -> bool {
    const FLAG: &str = concat!("--", "profile ");
    const ASSIGN: &str = "profile = \"";
    const JSON: &str = concat!("\"", "profile\": \"");
    const ID: &str = "profile_id\": \"";
    const DIR: &str = concat!("profiles", "/");
    if !line.contains("cargo ") {
        for value in tokens_after(line, FLAG) {
            if !allowed_profile(&value) {
                return true;
            }
        }
    }
    for marker in [ASSIGN, JSON, ID] {
        for value in quoted_values(line, marker) {
            if !allowed_profile(&value) {
                return true;
            }
        }
    }
    for value in path_segments_after(line, DIR) {
        if !allowed_profile(&value) {
            return true;
        }
    }
    false
}

fn allowed_profile(value: &str) -> bool {
    value == "default" || value == "fixture"
}

fn tokens_after(line: &str, marker: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = line;
    while let Some(index) = rest.find(marker) {
        let after = rest[index + marker.len()..].trim_start();
        let token = after.split_whitespace().next().unwrap_or("");
        let token =
            token.trim_matches(|character: char| matches!(character, ',' | '.' | ')' | '"' | '\''));
        values.push(token.to_string());
        rest = &rest[index + marker.len()..];
    }
    values
}

fn quoted_values(line: &str, marker: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = line;
    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let value = after.split('"').next().unwrap_or("");
        values.push(value.to_string());
        rest = &rest[index + marker.len()..];
    }
    values
}

fn path_segments_after(line: &str, marker: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = line;
    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let segment = after
            .split(|character: char| {
                character == '/'
                    || character.is_whitespace()
                    || matches!(character, '"' | '\'' | '`' | ')')
            })
            .next()
            .unwrap_or("");
        values.push(segment.to_string());
        rest = &rest[index + marker.len()..];
    }
    values
}
