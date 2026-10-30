use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn workspace_version_has_one_source() {
    let root = root();
    let manifest = read(&root, "Cargo.toml");
    let mut in_table = false;
    let mut versions = 0usize;
    for (number, line) in manifest.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == "[workspace.package]" {
            in_table = true;
            continue;
        }
        if in_table && trimmed.starts_with('[') {
            break;
        }
        if in_table && !trimmed.starts_with('#') && trimmed.starts_with("version = \"") {
            versions += 1;
            assert_eq!(
                versions,
                1,
                "{}",
                blame("Cargo.toml", number + 1, "a second workspace version line")
            );
        }
    }
    assert_eq!(
        versions,
        1,
        "{}",
        blame("Cargo.toml", 1, "workspace package has no version line")
    );
    for manifest_path in member_manifests(&root) {
        let rel = rel_to(&root, &manifest_path);
        let text = fs::read_to_string(&manifest_path).unwrap_or_default();
        let mut in_package = false;
        let mut saw_package = false;
        let mut workspace_versions = 0usize;
        for (number, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed == "[package]" {
                in_package = true;
                saw_package = true;
                continue;
            }
            if in_package && trimmed.starts_with('[') {
                in_package = false;
            }
            if !in_package || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with("version = \"") {
                panic!("{}", blame(&rel, number + 1, "member sets its own version"));
            }
            if trimmed == "version.workspace = true" {
                workspace_versions += 1;
            }
        }
        assert!(
            saw_package,
            "{}",
            blame(&rel, 1, "package table is missing")
        );
        assert_eq!(
            workspace_versions,
            1,
            "{}",
            blame(
                &rel,
                1,
                "member does not inherit the workspace version once"
            )
        );
    }
}

#[test]
fn no_dev_version_literal_outside_allowed_files() {
    let root = root();
    let version = workspace_version(&root);
    let needle = format!("{}{}", "0.7.0-", "dev");
    for rel in listed_files(&root) {
        if rel
            .split('/')
            .any(|part| part == "corpora" || part == "node_modules")
        {
            continue;
        }
        let path = root.join(&rel);
        if path.is_symlink() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if allowed_version_file(&rel, &version) {
            continue;
        }
        for (number, line) in text.lines().enumerate() {
            assert!(
                !line.contains(&needle),
                "{}",
                blame(&rel, number + 1, "development version literal")
            );
        }
    }
}

#[test]
fn changelog_shape() {
    let root = root();
    let text = read(&root, "CHANGELOG.md");
    let mut headings: Vec<(usize, String)> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.starts_with("## [") {
            headings.push((number + 1, line.to_string()));
        }
    }
    assert!(
        !headings.is_empty() && headings[0].1 == "## [Unreleased]",
        "{}",
        blame(
            "CHANGELOG.md",
            headings.first().map(|item| item.0).unwrap_or(1),
            "first heading is not Unreleased"
        )
    );
    let mut dated = Vec::new();
    for (line_no, line) in headings.iter().skip(1) {
        let parsed = parse_dated(line).unwrap_or_else(|| {
            panic!(
                "{}",
                blame(
                    "CHANGELOG.md",
                    *line_no,
                    "dated heading is not a real version and date"
                )
            )
        });
        dated.push((*line_no, parsed));
    }
    for pair in dated.windows(2) {
        assert!(
            pair[1].1.version < pair[0].1.version,
            "{}",
            blame(
                "CHANGELOG.md",
                pair[1].0,
                "versions do not strictly descend"
            )
        );
        assert!(
            pair[1].1.date <= pair[0].1.date,
            "{}",
            blame("CHANGELOG.md", pair[1].0, "dates increase going down")
        );
    }
    let newest = dated
        .first()
        .map(|item| item.1.version_text.clone())
        .unwrap_or_default();
    let after = dated.last().map(|item| item.0).unwrap_or(1);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut urls: BTreeMap<String, String> = BTreeMap::new();
    for (number, line) in text.lines().enumerate() {
        if number < after {
            continue;
        }
        if let Some((name, url)) = reference_link(line) {
            *counts.entry(name.clone()).or_insert(0) += 1;
            urls.insert(name, url);
        }
    }
    let mut names = vec!["Unreleased".to_string()];
    names.extend(dated.iter().map(|item| item.1.version_text.clone()));
    for name in &names {
        assert_eq!(
            counts.get(name).copied().unwrap_or(0),
            1,
            "{}",
            blame(
                "CHANGELOG.md",
                after,
                &format!("reference link for {name} is missing or repeated")
            )
        );
    }
    let unreleased = urls.get("Unreleased").map(String::as_str).unwrap_or("");
    let suffix = format!("compare/v{newest}...HEAD");
    assert!(
        unreleased.ends_with(&suffix),
        "{}",
        blame(
            "CHANGELOG.md",
            after,
            "compare link does not name the newest version"
        )
    );
}

#[test]
fn changelog_relative_links_resolve() {
    let root = root();
    let mut files = vec!["CHANGELOG.md".to_string()];
    if let Ok(entries) = fs::read_dir(root.join("docs/release-notes")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                files.push(rel_to(&root, &path));
            }
        }
    }
    for file in files {
        let text = read(&root, &file);
        let base = root.join(&file).parent().unwrap().to_path_buf();
        let mut fenced = false;
        for (number, line) in text.lines().enumerate() {
            if line.trim().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }
            for target in markdown_targets(&strip_inline_code(line)) {
                check_relative(&file, number + 1, &base, &target);
            }
        }
    }
}

#[test]
fn final_version_requires_changelog_section() {
    let root = root();
    let version = workspace_version(&root);
    if !is_final(&version) {
        return;
    }
    let text = read(&root, "CHANGELOG.md");
    let sections = changelog_sections(&text);
    let unreleased = sections.iter().find(|section| section.name == "Unreleased");
    let unreleased = unreleased.unwrap_or_else(|| {
        panic!(
            "{}",
            blame("CHANGELOG.md", 1, "Unreleased section is missing")
        )
    });
    for line in &unreleased.body {
        assert!(
            line.text.trim().is_empty(),
            "{}",
            blame(
                "CHANGELOG.md",
                line.number,
                "Unreleased section is not empty"
            )
        );
    }
    let dated = sections.iter().find(|section| section.name != "Unreleased");
    let dated =
        dated.unwrap_or_else(|| panic!("{}", blame("CHANGELOG.md", 1, "dated section is missing")));
    assert!(
        dated.heading.starts_with(&format!("## [{version}] ")),
        "{}",
        blame(
            "CHANGELOG.md",
            dated.line,
            "first dated heading is not this version"
        )
    );
    for title in ["### Added", "### Changed", "### Removed"] {
        let hits: Vec<_> = dated
            .body
            .iter()
            .filter(|line| line.text.trim() == title)
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "{}",
            blame(
                "CHANGELOG.md",
                dated.line,
                &format!("{title} does not appear once")
            )
        );
        let at = hits[0].number;
        let mut bullet = false;
        for line in &dated.body {
            if line.number <= at {
                continue;
            }
            if line.text.starts_with("### ") || line.text.starts_with("## ") {
                break;
            }
            if line.text.starts_with("- ") {
                bullet = true;
            }
        }
        assert!(
            bullet,
            "{}",
            blame("CHANGELOG.md", at, &format!("{title} has no bullet"))
        );
    }
    let notes = format!(
        "https://github.com/infiquetra/talaria/blob/v{version}/docs/release-notes/v{version}.md"
    );
    let body = dated
        .body
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body.contains(&notes),
        "{}",
        blame("CHANGELOG.md", dated.line, "release notes link is missing")
    );
    let reference =
        format!("[{version}]: https://github.com/infiquetra/talaria/releases/tag/v{version}");
    assert!(
        text.lines().any(|line| line == reference),
        "{}",
        blame("CHANGELOG.md", 1, "version reference link is missing")
    );
}

#[test]
fn final_version_requires_release_notes() {
    let root = root();
    let version = workspace_version(&root);
    if !is_final(&version) {
        return;
    }
    let rel = notes_rel(&version);
    let path = root.join(&rel);
    assert!(
        path.is_file(),
        "{}",
        blame(&rel, 1, "release notes file is missing")
    );
    let text = read(&root, &rel);
    assert!(
        !text.starts_with('\u{feff}'),
        "{}",
        blame(&rel, 1, "title has a byte-order mark")
    );
    let first = text.lines().next().unwrap_or("");
    assert_eq!(
        first,
        format!("# Talaria v{version}"),
        "{}",
        blame(&rel, 1, "title is not the version heading")
    );
    let headings = outside_h2(&text);
    let expect = [
        "## Read this part first",
        "## What is new",
        "## What changed from 0.6.5",
        "## Install",
        "## Works with Hermes",
        "## Staying on 0.6.5",
        "## Known limits",
        "## What was checked",
    ];
    assert_eq!(
        headings,
        expect,
        "{}",
        blame(
            &rel,
            1,
            "level-two headings are not the eight release sections"
        )
    );
    let pinned = pinned_tag(&root);
    assert!(
        text.contains(&pinned),
        "{}",
        blame(&rel, 1, "pinned tag is missing")
    );
    for phrase in [
        "cargo build --release",
        "crates.io",
        "npm",
        "Python Package Index",
    ] {
        assert!(
            text.contains(phrase),
            "{}",
            blame(&rel, 1, &format!("{phrase} is missing"))
        );
    }
    let migration = format!(
        "https://github.com/infiquetra/talaria/blob/v{version}/docs/migrating-from-0.6.5.md"
    );
    assert!(
        text.contains(&migration),
        "{}",
        blame(&rel, 1, "migration link is missing")
    );
    assert!(
        text.lines().count() <= 400,
        "{}",
        blame(&rel, 1, "release notes are longer than 400 lines")
    );
}

#[test]
fn release_links_point_at_real_files() {
    let root = root();
    let version = workspace_version(&root);
    let rel = notes_rel(&version);
    if !is_final(&version) || !root.join(&rel).is_file() {
        return;
    }
    let prefix = format!("https://github.com/infiquetra/talaria/blob/v{version}/");
    let notes = numbered(&read(&root, &rel));
    let changelog = read(&root, "CHANGELOG.md");
    let section = dated_section_lines(&changelog);
    scan_blob_links(&root, &rel, &notes, &prefix);
    scan_blob_links(&root, "CHANGELOG.md", &section, &prefix);
}

#[test]
fn release_notes_flags_exist_in_help() {
    let root = root();
    let version = workspace_version(&root);
    let rel = notes_rel(&version);
    if !is_final(&version) || !root.join(&rel).is_file() {
        return;
    }
    let text = read(&root, &rel);
    let commands = command_lines(&text);
    assert!(
        !commands.is_empty(),
        "{}",
        blame(&rel, 1, "no command line in a shell block")
    );
    let mut flags = Vec::new();
    for (line_no, line) in commands {
        for token in line.split_whitespace() {
            if let Some(flag) = token.strip_prefix('-') {
                let flag = flag.split('=').next().unwrap_or("");
                if flag.is_empty() || flag == "-" {
                    continue;
                }
                flags.push((line_no, format!("-{flag}")));
            }
        }
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_talaria"));
    child
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .current_dir(&root)
        .arg("--help");
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        child.env("LLVM_PROFILE_FILE", profile);
    }
    let output = child.output().expect("help command failed");
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let first = err.lines().next().unwrap_or("");
        panic!(
            "{}",
            blame(&rel, 1, &format!("help exited {} {first}", output.status))
        );
    }
    let help = String::from_utf8_lossy(&output.stdout);
    for (line_no, flag) in flags {
        assert!(
            whole_token(&help, &flag),
            "{}",
            blame(
                &rel,
                line_no,
                &format!("flag {flag} is not a whole word in help")
            )
        );
    }
}

#[test]
fn release_text_is_public_safe() {
    let root = root();
    let changelog = read(&root, "CHANGELOG.md");
    let mut chunks = vec![
        (
            "CHANGELOG.md".to_string(),
            section_lines(&changelog, "Unreleased"),
        ),
        ("CHANGELOG.md".to_string(), dated_section_lines(&changelog)),
    ];
    let version = workspace_version(&root);
    let notes = notes_rel(&version);
    if root.join(&notes).is_file() {
        chunks.push((notes.clone(), numbered(&read(&root, &notes))));
    }
    let own = read(&root, "crates/talaria-cli/tests/release_metadata.rs");
    let comments = numbered(&own)
        .into_iter()
        .filter(|(_, line)| line.trim_start().starts_with("//"))
        .collect();
    chunks.push((
        "crates/talaria-cli/tests/release_metadata.rs".to_string(),
        comments,
    ));
    for (file, lines) in &chunks {
        scan_public(file, lines);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn read(root: &Path, file: &str) -> String {
    fs::read_to_string(root.join(file))
        .unwrap_or_else(|_| panic!("{}", blame(file, 1, "file is not readable")))
}

fn blame(file: &str, line: usize, property: &str) -> String {
    format!("{file}:{line}: {property}")
}

fn rel_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn member_manifests(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for parent in ["crates", "tools"] {
        let Ok(entries) = fs::read_dir(root.join(parent)) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let manifest = entry.path().join("Cargo.toml");
                if manifest.is_file() {
                    found.push(manifest);
                }
            }
        }
    }
    found.sort();
    found
}

fn workspace_version(root: &Path) -> String {
    let text = read(root, "Cargo.toml");
    let mut in_table = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "[workspace.package]" {
            in_table = true;
            continue;
        }
        if in_table && trimmed.starts_with('[') {
            break;
        }
        if in_table
            && !trimmed.starts_with('#')
            && let Some(value) = trimmed.strip_prefix("version = \"")
            && let Some((value, _)) = value.split_once('"')
        {
            return value.to_string();
        }
    }
    panic!("{}", blame("Cargo.toml", 1, "workspace version is missing"))
}

fn is_final(version: &str) -> bool {
    !version.contains('-') && !version.contains('+')
}

fn notes_rel(version: &str) -> String {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    format!("docs/release-notes/v{core}.md")
}

fn allowed_version_file(rel: &str, version: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    if name == "Cargo.toml" || name == "Cargo.lock" {
        return !is_final(version);
    }
    if rel == "CHANGELOG.md" || rel.starts_with("docs/") {
        return true;
    }
    !is_final(version) && (rel == "README.md" || rel == "llms.txt")
}

fn listed_files(root: &Path) -> Vec<String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output()
        .unwrap_or_else(|_| panic!("git ls-files failed"));
    if !output.status.success() {
        panic!("git ls-files failed");
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

struct Dated {
    version: (u64, u64, u64),
    version_text: String,
    date: (i32, u32, u32),
}

fn parse_dated(line: &str) -> Option<Dated> {
    let rest = line.strip_prefix("## [")?;
    let (version_text, after) = rest.split_once("] ")?;
    let version = parse_version(version_text)?;
    let after = after
        .strip_prefix('\u{2014}')
        .or_else(|| after.strip_prefix('-'))?;
    let date_text = after.strip_prefix(' ')?;
    if date_text.len() != 10 {
        return None;
    }
    let date = parse_date(date_text)?;
    Some(Dated {
        version,
        version_text: version_text.to_string(),
        date,
    })
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.split('.');
    let raw = [parts.next()?, parts.next()?, parts.next()?];
    if parts.next().is_some() {
        return None;
    }
    let mut numbers = [0u64; 3];
    for (index, part) in raw.iter().enumerate() {
        if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        if part.len() > 1 && part.starts_with('0') {
            return None;
        }
        numbers[index] = part.parse().ok()?;
    }
    Some((numbers[0], numbers[1], numbers[2]))
}

fn parse_date(text: &str) -> Option<(i32, u32, u32)> {
    let mut parts = text.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    let max = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap(year) => 29,
        2 => 28,
        _ => 0,
    };
    if day == 0 || day > max {
        return None;
    }
    Some((year, month, day))
}

fn leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn reference_link(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix('[')?;
    let (name, url) = rest.split_once("]: ")?;
    if name.is_empty() || name.contains(']') || url.is_empty() || url.contains(' ') {
        return None;
    }
    Some((name.to_string(), url.to_string()))
}

fn strip_inline_code(line: &str) -> String {
    let mut out = String::new();
    let mut code = false;
    for ch in line.chars() {
        if ch == '`' {
            code = !code;
            continue;
        }
        if !code {
            out.push(ch);
        }
    }
    out
}

fn markdown_targets(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut targets = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '!' && chars.get(index + 1) == Some(&'[') {
            index += 1;
        }
        if chars[index] == '['
            && let Some((start, end)) = link_target(&chars, index)
        {
            targets.push(chars[start..end].iter().collect());
            index = end;
            continue;
        }
        index += 1;
    }
    targets
}

fn link_target(chars: &[char], open: usize) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut index = open;
    while index < chars.len() {
        match chars[index] {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        index += 1;
    }
    if chars.get(index + 1) != Some(&'(') {
        return None;
    }
    let start = index + 2;
    let mut end = start;
    while end < chars.len() && chars[end] != ')' && !chars[end].is_whitespace() {
        end += 1;
    }
    if end == start {
        return None;
    }
    Some((start, end))
}

fn check_relative(file: &str, line: usize, base: &Path, target: &str) {
    if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with('#')
    {
        return;
    }
    assert!(
        !target.starts_with('/'),
        "{}",
        blame(file, line, "link target starts at the filesystem root")
    );
    if let Some(slash) = target.find('/') {
        assert!(
            !target[..slash].contains(':'),
            "{}",
            blame(file, line, "link target uses another scheme")
        );
    } else {
        assert!(
            !target.contains(':'),
            "{}",
            blame(file, line, "link target uses another scheme")
        );
    }
    let path = target.split(['#', '?']).next().unwrap_or(target);
    if path.is_empty() {
        return;
    }
    assert!(
        base.join(path).exists(),
        "{}",
        blame(file, line, &format!("link target {path} does not exist"))
    );
}

struct Line {
    number: usize,
    text: String,
}

struct Section {
    name: String,
    line: usize,
    heading: String,
    body: Vec<Line>,
}

fn changelog_sections(text: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut current: Option<Section> = None;
    for (number, line) in text.lines().enumerate() {
        if line.starts_with("## [") {
            if let Some(section) = current.take() {
                sections.push(section);
            }
            let name = line
                .trim_start_matches("## [")
                .split(']')
                .next()
                .unwrap_or("")
                .to_string();
            current = Some(Section {
                name,
                line: number + 1,
                heading: line.to_string(),
                body: Vec::new(),
            });
        } else if let Some(section) = current.as_mut() {
            section.body.push(Line {
                number: number + 1,
                text: line.to_string(),
            });
        }
    }
    if let Some(section) = current {
        sections.push(section);
    }
    sections
}

fn numbered(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.to_string()))
        .collect()
}

fn section_lines(text: &str, name: &str) -> Vec<(usize, String)> {
    changelog_sections(text)
        .into_iter()
        .find(|section| section.name == name)
        .map(|section| {
            let mut lines = vec![(section.line, section.heading)];
            lines.extend(
                section
                    .body
                    .into_iter()
                    .map(|line| (line.number, line.text)),
            );
            lines
        })
        .unwrap_or_default()
}

fn dated_section_lines(text: &str) -> Vec<(usize, String)> {
    changelog_sections(text)
        .into_iter()
        .find(|section| section.name != "Unreleased")
        .map(|section| {
            let mut lines = vec![(section.line, section.heading)];
            lines.extend(
                section
                    .body
                    .into_iter()
                    .map(|line| (line.number, line.text)),
            );
            lines
        })
        .unwrap_or_default()
}

fn outside_h2(text: &str) -> Vec<&str> {
    let mut fenced = false;
    let mut headings = Vec::new();
    for line in text.lines() {
        if line.trim().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced && line.starts_with("## ") {
            headings.push(line);
        }
    }
    headings
}

fn pinned_tag(root: &Path) -> String {
    let text = read(root, "crates/hermes-contract/PINNED");
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("tag=") {
            return value.trim().to_string();
        }
    }
    panic!(
        "{}",
        blame("crates/hermes-contract/PINNED", 1, "tag line is missing")
    )
}

fn scan_blob_links(root: &Path, file: &str, lines: &[(usize, String)], prefix: &str) {
    let mut fenced = false;
    for (number, line) in lines {
        if line.trim().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut rest = line.as_str();
        while let Some(at) = rest.find(prefix) {
            let after = &rest[at + prefix.len()..];
            let end = after
                .find(|ch: char| ch.is_whitespace() || matches!(ch, ')' | '>' | '"' | '\'' | '`'))
                .unwrap_or(after.len());
            let mut url = after[..end].to_string();
            while url.ends_with(['.', ',', ';', ':']) {
                url.pop();
            }
            let (path, anchor) = url.split_once('#').unwrap_or((url.as_str(), ""));
            assert!(
                !path.contains('%'),
                "{}",
                blame(file, *number, "link path is percent-encoded")
            );
            assert!(
                root.join(path).exists(),
                "{}",
                blame(file, *number, &format!("link path {path} does not exist"))
            );
            if !anchor.is_empty() {
                assert!(
                    path.ends_with(".md"),
                    "{}",
                    blame(file, *number, "anchor is not on a markdown file")
                );
                let target = fs::read_to_string(root.join(path)).unwrap_or_default();
                assert!(
                    heading_slugs(&target).contains(anchor),
                    "{}",
                    blame(file, *number, &format!("heading {anchor} does not exist"))
                );
            }
            rest = &after[end..];
        }
    }
}

fn heading_slugs(text: &str) -> BTreeSet<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut slugs = BTreeSet::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let Some(body) = line.trim_start_matches('#').strip_prefix(' ') else {
            continue;
        };
        if !line.starts_with('#') {
            continue;
        }
        let slug = slugify(body.trim());
        let seen = counts.entry(slug.clone()).or_insert(0);
        let unique = if *seen == 0 {
            slug
        } else {
            format!("{slug}-{seen}")
        };
        *seen += 1;
        slugs.insert(unique);
    }
    slugs
}

fn slugify(text: &str) -> String {
    let without_ticks = text.replace('`', "");
    let mut plain = String::new();
    let chars: Vec<char> = without_ticks.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '['
            && let Some((label, next)) = bracket_label(&chars, index)
        {
            plain.push_str(&label);
            index = next;
            continue;
        }
        plain.push(chars[index]);
        index += 1;
    }
    let mut out = String::new();
    for ch in plain.chars() {
        let lower = if ch.is_ascii_uppercase() {
            ch.to_ascii_lowercase()
        } else {
            ch
        };
        if lower.is_alphanumeric() || lower == '-' || lower == '_' {
            out.push(lower);
        } else if lower == ' ' {
            out.push('-');
        }
    }
    out
}

fn bracket_label(chars: &[char], open: usize) -> Option<(String, usize)> {
    let mut index = open + 1;
    let mut label = String::new();
    while index < chars.len() && chars[index] != ']' {
        label.push(chars[index]);
        index += 1;
    }
    if chars.get(index + 1) != Some(&'(') {
        return None;
    }
    index += 2;
    while index < chars.len() && chars[index] != ')' {
        index += 1;
    }
    Some((label, index + 1))
}

fn command_lines(text: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut fenced = false;
    let mut shell = false;
    for (number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if fenced {
                fenced = false;
                shell = false;
            } else {
                fenced = true;
                let info = trimmed.trim_start_matches('`');
                shell = info == "bash" || info == "sh";
            }
            continue;
        }
        if !shell {
            continue;
        }
        let mut body = line.trim_start();
        if let Some(rest) = body.strip_prefix("$ ") {
            body = rest;
        }
        if body.starts_with("talaria ")
            || body.starts_with("target/release/talaria ")
            || body == "talaria"
            || body == "target/release/talaria"
        {
            lines.push((number + 1, body.to_string()));
        }
    }
    lines
}

fn whole_token(help: &str, token: &str) -> bool {
    let mut rest = help;
    while let Some(at) = rest.find(token) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + token.len()..].chars().next();
        let edge =
            |ch: Option<char>| ch.is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '-'));
        if edge(before) && edge(after) {
            return true;
        }
        rest = &rest[at + token.len()..];
    }
    false
}

fn scan_public(file: &str, lines: &[(usize, String)]) {
    let prefixes = [
        format!("{}{}", "/Us", "ers/"),
        format!("{}{}", "/ho", "me/"),
        format!("{}{}", "/pri", "vate/"),
        format!("{}{}{}", "/va", "r/fol", "ders/"),
        format!("{}{}", "Co-auth", "ored-by"),
        format!("{}{}", "Generated", " with"),
    ];
    for (number, line) in lines {
        for prefix in &prefixes {
            assert!(
                !line.contains(prefix),
                "{}",
                blame(
                    file,
                    *number,
                    "public text holds a path or attribution fragment"
                )
            );
        }
        if let Some(column) = unit_code_at(line) {
            assert!(
                column > line.len(),
                "{}",
                blame(file, *number, "public text holds a unit code")
            );
        }
        let collapsed: String = line
            .chars()
            .filter(|ch| *ch != ' ' && *ch != '\t')
            .collect();
        let flag = format!("{}{}", "un", "attended");
        let forms = [
            format!("{flag}:true"),
            format!("{flag}:Some(true)"),
            format!("{flag}=true"),
            format!(".{flag}(true)"),
            format!(".{flag}(Some(true))"),
            format!("\"{flag}\":true"),
        ];
        for form in &forms {
            assert!(
                !collapsed.contains(form),
                "{}",
                blame(
                    file,
                    *number,
                    &format!("public text sets the {}{} flag true", "un", "attended"),
                )
            );
        }
    }
}

fn unit_code_at(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let previous_ok = index == 0 || !is_code_char(bytes[index - 1]);
        if previous_ok && let Some(len) = code_len(bytes, index) {
            let next = index + len;
            if next >= bytes.len() || !bytes[next].is_ascii_alphanumeric() {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

fn is_code_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-'
}

fn code_len(bytes: &[u8], index: usize) -> Option<usize> {
    let rest = &bytes[index..];
    if rest.len() >= 5
        && rest[0] == b'P'
        && rest[1].is_ascii_digit()
        && rest[2] == b'-'
        && rest[3].is_ascii_digit()
        && rest[4].is_ascii_digit()
    {
        return Some(5);
    }
    if rest.len() >= 3 && rest[0] == b'P' && rest[1] == b'H' && rest[2].is_ascii_digit() {
        let mut length = 3;
        if rest.get(length).is_some_and(u8::is_ascii_uppercase) {
            length += 1;
        }
        if rest.get(length..length + 4) == Some(b"-ACC") {
            length += 4;
        }
        return Some(length);
    }
    if rest.len() >= 6
        && rest[0] == b'R'
        && rest[1] == b'E'
        && rest[2] == b'L'
        && rest[3] == b'-'
        && rest[4].is_ascii_digit()
        && rest[5].is_ascii_digit()
    {
        return Some(6);
    }
    None
}
