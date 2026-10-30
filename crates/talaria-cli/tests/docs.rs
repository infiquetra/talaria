//! User documentation checks. Missing pages fail with a named message.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use talaria_tui::keys::{Action, KeyContext, KeyMap};

const GUIDE: &str = "docs/guide.md";
const KEYS: &str = "docs/keybindings.md";
const CONFIG: &str = "docs/configuration.md";
const MIGRATION: &str = "docs/migrating-from-0.6.5.md";

const GUIDE_HEADINGS: [&str; 11] = [
    "## What Talaria is",
    "## Install",
    "## Before you start",
    "## Your first session",
    "## Working in a session",
    "## Many sessions at once",
    "## Settings",
    "## Workflows",
    "## Scripts and automation",
    "## When something goes wrong",
    "## Where things live",
];

const MIGRATION_HEADINGS: [&str; 8] = [
    "## Who this is for",
    "## Staying on 0.6.5 for now",
    "## Installing 0.7.0 next to 0.6.5",
    "## Your config file",
    "## Your status-line command",
    "## Your theme",
    "## What no longer exists",
    "## Going back to 0.6.5",
];

#[test]
fn required_pages_exist_with_headings() {
    let root = root();
    for (file, why) in [
        (GUIDE, "the three new pages do not exist"),
        (KEYS, "the three new pages do not exist"),
        (MIGRATION, "the three new pages do not exist"),
    ] {
        assert!(root.join(file).is_file(), "{file} does not exist: {why}");
    }
    assert!(root.join(CONFIG).is_file(), "{CONFIG} does not exist");
    let guide = read(&root, GUIDE);
    assert_eq!(
        heading_sequence(&guide, &GUIDE_HEADINGS),
        GUIDE_HEADINGS.to_vec(),
        "{GUIDE}:1: guide headings are missing or out of order"
    );
    let migration = read(&root, MIGRATION);
    assert_eq!(
        heading_sequence(&migration, &MIGRATION_HEADINGS),
        MIGRATION_HEADINGS.to_vec(),
        "{MIGRATION}:1: migration headings are missing or out of order"
    );
    let keys = read(&root, KEYS);
    assert!(
        keys.lines().any(|line| line.trim() == "## Composer keys"),
        "{KEYS}:1: ## Composer keys is missing"
    );
}

#[test]
fn relative_links_and_anchors_resolve() {
    let root = root();
    for file in [
        GUIDE,
        KEYS,
        CONFIG,
        MIGRATION,
        "README.md",
        "docs/00-index.md",
        "llms.txt",
    ] {
        assert!(
            root.join(file).is_file(),
            "{file} does not exist: the pages and new links do not exist"
        );
    }
    for file in [
        GUIDE,
        KEYS,
        CONFIG,
        MIGRATION,
        "README.md",
        "docs/00-index.md",
        "llms.txt",
    ] {
        let text = read(&root, file);
        let base = root.join(file).parent().unwrap().to_path_buf();
        let mut fenced = false;
        for (number, line) in text.lines().enumerate() {
            let line_no = number + 1;
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }
            for target in link_targets(&strip_inline_code(line)) {
                check_link(&root, file, line_no, &base, &target);
            }
        }
    }
}

#[test]
fn pages_and_fixtures_are_public_safe() {
    let root = root();
    for file in [GUIDE, KEYS, CONFIG, MIGRATION] {
        assert!(
            root.join(file).is_file(),
            "{file} does not exist: the scan reports the missing pages"
        );
    }
    let files = [
        GUIDE,
        KEYS,
        CONFIG,
        MIGRATION,
        "fixtures/config/config-0.6.5-defaults.toml",
        "fixtures/config/config-0.6.5-custom.toml",
        "llms.txt",
    ];
    for file in files {
        assert!(root.join(file).is_file(), "{file} does not exist");
    }
    let short_flag = help_has_short_profile();
    for file in files {
        let fixture = file.starts_with("fixtures/");
        for (number, line) in read(&root, file).lines().enumerate() {
            scan_line(file, number + 1, line, fixture, short_flag);
        }
    }
}

#[test]
fn commands_in_docs_exist_in_help() {
    let root = root();
    assert!(
        root.join(GUIDE).is_file(),
        "{GUIDE} does not exist: the guide does not exist"
    );
    for file in [KEYS, CONFIG, MIGRATION] {
        assert!(root.join(file).is_file(), "{file} does not exist");
    }
    for file in [GUIDE, KEYS, CONFIG, MIGRATION] {
        check_bash_commands(&root, file);
    }
}

#[test]
fn keybinding_tables_equal_default_key_map() {
    let root = root();
    assert!(
        root.join(KEYS).is_file(),
        "{KEYS} does not exist: the tables do not exist"
    );
    let text = read(&root, KEYS);
    let groups = action_groups();
    let defaults = KeyMap::defaults();
    let mut expected_headings = BTreeSet::new();
    for (context, actions) in &groups {
        let heading = context_heading(*context);
        expected_headings.insert(heading.clone());
        let table = table_after_heading(&text, &heading).unwrap_or_else(|| {
            panic!("{KEYS}:1: missing table under {heading}: the tables do not exist")
        });
        let mut seen = BTreeSet::new();
        for row in table {
            let config = row.config.trim_matches('`').trim();
            if config == "fixed" {
                continue;
            }
            let action = actions
                .iter()
                .copied()
                .find(|action| action.config_name() == config)
                .unwrap_or_else(|| {
                    panic!(
                        "{KEYS}:{}: config name {config} is not an action of {heading}",
                        row.line
                    )
                });
            assert!(
                seen.insert(config.to_string()),
                "{KEYS}:{}: config name {config} is repeated",
                row.line
            );
            let got = chord_cell(&row.default_keys);
            let want: BTreeSet<String> = defaults
                .chords(action)
                .iter()
                .map(|chord| chord.label().to_lowercase())
                .collect();
            assert_eq!(
                got, want,
                "{KEYS}:{}: default keys for {config} do not match the key map",
                row.line
            );
            let lower = row.default_keys.to_lowercase();
            for banned in ["f1", "f2", "ctrl+b", "ctrl+s"] {
                if banned == "ctrl+s"
                    && matches!(config, "commit_submit" | "providers_save" | "mcp_save")
                {
                    continue;
                }
                assert!(
                    !split_chords(&lower).iter().any(|chord| chord == banned),
                    "{KEYS}:{}: default keys hold {banned}",
                    row.line
                );
            }
        }
        let want_names: BTreeSet<String> = actions
            .iter()
            .map(|action| action.config_name().to_string())
            .collect();
        assert_eq!(
            seen, want_names,
            "{KEYS}:1: config names under {heading} do not match the actions"
        );
    }
    for (number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") && trimmed.ends_with(" keys") {
            assert!(
                expected_headings.contains(trimmed),
                "{KEYS}:{}: unexpected key heading {trimmed}",
                number + 1
            );
        }
    }
}

#[test]
fn migration_table_has_a_row_for_every_old_setting() {
    let root = root();
    assert!(
        root.join(MIGRATION).is_file(),
        "{MIGRATION} does not exist: the page does not exist"
    );
    let universe = fixture_settings(&root);
    let rows = migration_rows(&read(&root, MIGRATION));
    let mut covered = BTreeMap::<String, String>::new();
    for row in &rows {
        let paths = expand_setting(&row.setting, &universe);
        assert!(
            !paths.is_empty(),
            "{MIGRATION}:{}: {} matches no fixture setting",
            row.line,
            row.setting
        );
        for path in paths {
            assert!(
                covered.insert(path.clone(), row.outcome.clone()).is_none(),
                "{MIGRATION}:{}: {path} is listed twice",
                row.line
            );
        }
    }
    assert_eq!(
        covered.keys().cloned().collect::<BTreeSet<_>>(),
        universe.keys().cloned().collect::<BTreeSet<_>>(),
        "{MIGRATION}:1: the table does not list each old setting once"
    );
}

#[test]
fn migration_table_outcomes_match_the_code() {
    let root = root();
    assert!(
        root.join(MIGRATION).is_file(),
        "{MIGRATION} does not exist: the table does not exist"
    );
    let universe = fixture_settings(&root);
    let rows = migration_rows(&read(&root, MIGRATION));
    let dir = std::env::temp_dir().join(format!("migration-outcomes-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for row in &rows {
        for path in expand_setting(&row.setting, &universe) {
            let value = &universe[&path];
            match row.outcome.as_str() {
                "kept" => {
                    let notices = pipeline_for(&dir, &path, value);
                    assert!(
                        notices.is_empty(),
                        "{MIGRATION}:{}: {path} produced notices: {notices:?}",
                        row.line
                    );
                }
                "kept with a warning" => {
                    assert!(
                        path.starts_with("keys."),
                        "{MIGRATION}:{}: a warning row is outside keys",
                        row.line
                    );
                    let name = path.rsplit('.').next().unwrap();
                    let action = Action::from_config_name(name).unwrap_or_else(|| {
                        panic!("{MIGRATION}:{}: {name} is not an action", row.line)
                    });
                    let notices = pipeline_for(&dir, &path, value);
                    let hits: Vec<_> = notices
                        .iter()
                        .filter(|notice| notice.contains(name))
                        .cloned()
                        .collect();
                    assert_eq!(
                        hits.len(),
                        1,
                        "{MIGRATION}:{}: {path} notices: {notices:?}",
                        row.line
                    );
                    let (_loaded_notices, labels) = chords_for(&dir, &path, value, action);
                    let want = value_chords(value);
                    assert_eq!(
                        labels, want,
                        "{MIGRATION}:{}: chords for {path} did not take effect",
                        row.line
                    );
                }
                "moved" => {
                    assert_ne!(
                        row.new_path, "-",
                        "{MIGRATION}:{}: moved row has no new path",
                        row.line
                    );
                    let old = pipeline_for(&dir, &path, value);
                    let segment = path.rsplit('.').next().unwrap();
                    assert_eq!(
                        old.len(),
                        1,
                        "{MIGRATION}:{}: old path notices: {old:?}",
                        row.line
                    );
                    assert!(
                        old[0].contains(segment),
                        "{MIGRATION}:{}: old notice does not name {segment}: {old:?}",
                        row.line
                    );
                    let new = pipeline_for(&dir, &row.new_path, value);
                    assert!(
                        new.is_empty(),
                        "{MIGRATION}:{}: new path notices: {new:?}",
                        row.line
                    );
                }
                "notice" => {
                    let notices = pipeline_for(&dir, &path, value);
                    assert_eq!(
                        notices.len(),
                        1,
                        "{MIGRATION}:{}: {path} notices: {notices:?}",
                        row.line
                    );
                    let first = path.split('.').next().unwrap();
                    let last = path.rsplit('.').next().unwrap();
                    assert!(
                        notices[0].contains(first) || notices[0].contains(last),
                        "{MIGRATION}:{}: notice does not name {path}: {}",
                        row.line,
                        notices[0]
                    );
                }
                other => panic!("{MIGRATION}:{}: unknown outcome {other}", row.line),
            }
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migration_notices_block_is_what_a_reader_sees() {
    let root = root();
    assert!(
        root.join(MIGRATION).is_file(),
        "{MIGRATION} does not exist: the block does not exist"
    );
    let text = read(&root, MIGRATION);
    let block = text_block_after_table(&text)
        .unwrap_or_else(|| panic!("{MIGRATION}:1: the block does not exist"));
    let seen = pipeline_file(&root.join("fixtures/config/config-0.6.5-custom.toml"));
    assert_eq!(
        block,
        seen,
        "{MIGRATION}:1: notice block differs from the pipeline\nreal list:\n{}",
        seen.join("\n")
    );
}

#[test]
fn migration_default_notice_count_matches_the_page() {
    let root = root();
    assert!(
        root.join(MIGRATION).is_file(),
        "{MIGRATION} does not exist: the sentence does not exist"
    );
    let text = read(&root, MIGRATION);
    let mut found = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if let Some(count) = notice_count_sentence(line.trim()) {
            found.push((number + 1, count));
        }
    }
    assert_eq!(found.len(), 1, "{MIGRATION}:1: the sentence does not exist");
    let seen = pipeline_file(&root.join("fixtures/config/config-0.6.5-defaults.toml"));
    assert_eq!(
        found[0].1,
        seen.len(),
        "{MIGRATION}:{}: sentence says {} but the pipeline produced {}\n{}",
        found[0].0,
        found[0].1,
        seen.len(),
        seen.join("\n")
    );
}

#[test]
fn required_statements_are_present() {
    let root = root();
    for file in [
        GUIDE,
        KEYS,
        MIGRATION,
        "README.md",
        "docs/00-index.md",
        "llms.txt",
    ] {
        assert!(
            root.join(file).is_file(),
            "{file} does not exist: the pages do not exist"
        );
    }
    let guide = read(&root, GUIDE);
    let do_not: Vec<_> = guide
        .lines()
        .filter(|line| {
            line.starts_with("Do not")
                && line.contains("crates.io")
                && line.contains("npm")
                && line.contains("Python Package Index")
        })
        .collect();
    assert_eq!(
        do_not.len(),
        1,
        "{GUIDE}:1: the install warning line is missing"
    );
    let keys = read(&root, KEYS);
    let section = section_until_next_h2(&keys, "## Keys that do not reach Talaria");
    assert!(
        section.is_some(),
        "{KEYS}:1: ## Keys that do not reach Talaria is missing"
    );
    let section = section.unwrap();
    for word in [
        "F1",
        "F2",
        "Ctrl+B",
        "Ctrl+S",
        "Shift+Enter",
        "Alt+Enter",
        "Ctrl+J",
    ] {
        assert!(
            section.contains(word),
            "{KEYS}:1: key section is missing {word}"
        );
    }
    for file in ["README.md", "docs/00-index.md", "llms.txt"] {
        let text = read(&root, file);
        for page in [
            "guide.md",
            "keybindings.md",
            "configuration.md",
            "migrating-from-0.6.5.md",
        ] {
            assert!(
                link_targets_in(&text).iter().any(|target| target
                    .split(['#', '?'])
                    .next()
                    .unwrap()
                    .ends_with(page)),
                "{file}:1: missing a link to {page}"
            );
        }
    }
    let migration = read(&root, MIGRATION);
    let status =
        section_until_next_h2(&migration, "## Your status-line command").unwrap_or_default();
    for phrase in [
        "git show v0.6.5:docs/examples/status_bar_starter.py",
        "TALARIA_STATUS_INTERVAL_SECONDS",
        "formats/status-line.md",
        "version 1",
        "2 second",
    ] {
        assert!(
            status.contains(phrase),
            "{MIGRATION}:1: status section is missing {phrase}"
        );
    }
    assert!(
        migration.lines().any(|line| line.contains("approval")
            && (line.contains("reported") || line.contains("observed"))),
        "{MIGRATION}:1: the approval statement is missing"
    );
    for file in [GUIDE, KEYS, CONFIG, MIGRATION] {
        if !root.join(file).is_file() {
            continue;
        }
        for (number, line) in read(&root, file).lines().enumerate() {
            if line.contains("PYTHONDONTWRITEBYTECODE") {
                assert!(
                    line.contains("not measured"),
                    "{file}:{}: bytecode claim is bare",
                    number + 1
                );
            }
        }
    }
    let theme = section_until_next_h2(&migration, "## Your theme").unwrap_or_default();
    assert!(
        theme.contains("display.skin"),
        "{MIGRATION}:1: theme section is missing display.skin"
    );
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn read(root: &Path, file: &str) -> String {
    fs::read_to_string(root.join(file)).unwrap_or_else(|error| panic!("{file}:1: {error}"))
}

fn heading_sequence(text: &str, wanted: &[&str]) -> Vec<String> {
    let wanted: BTreeSet<&str> = wanted.iter().copied().collect();
    text.lines()
        .map(str::trim)
        .filter(|line| wanted.contains(line))
        .map(str::to_string)
        .collect()
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

fn link_targets(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut targets = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '['
            || (chars[index] == '!' && index + 1 < chars.len() && chars[index + 1] == '[')
        {
            if chars[index] == '!' {
                index += 1;
            }
            if let Some(end) = find_link(&chars, index) {
                targets.push(chars[end.0..end.1].iter().collect());
                index = end.1 + 1;
                continue;
            }
        }
        index += 1;
    }
    targets
}

fn find_link(chars: &[char], open: usize) -> Option<(usize, usize)> {
    let mut depth = 0;
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
    if index >= chars.len() || chars.get(index + 1) != Some(&'(') {
        return None;
    }
    let start = index + 2;
    let mut end = start;
    while end < chars.len() && chars[end] != ')' && chars[end] != ' ' {
        end += 1;
    }
    if end == start {
        return None;
    }
    Some((start, end))
}

fn link_targets_in(text: &str) -> Vec<String> {
    let mut fenced = false;
    let mut all = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced {
            all.extend(link_targets(&strip_inline_code(line)));
        }
    }
    all
}

fn check_link(_root: &Path, file: &str, line: usize, base: &Path, target: &str) {
    if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
    {
        return;
    }
    assert!(
        !target.starts_with('/'),
        "{file}:{line}: link target starts at the filesystem root"
    );
    if let Some(slash) = target.find('/') {
        assert!(
            !target[..slash].contains(':'),
            "{file}:{line}: link target uses another scheme"
        );
    } else {
        assert!(
            !target.contains(':'),
            "{file}:{line}: link target uses another scheme"
        );
    }
    let (path, anchor) = target.split_once('#').unwrap_or((target, ""));
    let path = path.split('?').next().unwrap_or(path);
    if path.is_empty() {
        let text = fs::read_to_string(base.join(file_name(file))).unwrap_or_default();
        assert!(
            heading_slugs(&text).contains(anchor),
            "{file}:{line}: missing anchor {anchor}"
        );
        return;
    }
    let full = base.join(path);
    assert!(
        full.exists(),
        "{file}:{line}: link target {path} does not exist"
    );
    if anchor.is_empty() {
        return;
    }
    assert!(
        path.ends_with(".md"),
        "{file}:{line}: anchor is on a file that is not markdown"
    );
    let text = fs::read_to_string(&full).unwrap_or_default();
    assert!(
        heading_slugs(&text).contains(anchor),
        "{file}:{line}: missing anchor {anchor} in {path}"
    );
}

fn file_name(file: &str) -> &str {
    file.rsplit('/').next().unwrap_or(file)
}

fn heading_slugs(text: &str) -> BTreeSet<String> {
    let mut slugs = BTreeSet::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced || !line.starts_with('#') {
            continue;
        }
        let Some(text) = line.trim_start_matches('#').strip_prefix(' ') else {
            continue;
        };
        let slug = slug(text);
        let n = counts.entry(slug.clone()).or_insert(0);
        let unique = if *n == 0 { slug } else { format!("{slug}-{n}") };
        *n += 1;
        slugs.insert(unique);
    }
    slugs
}

fn slug(text: &str) -> String {
    let mut plain = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '`' {
            index += 1;
            while index < chars.len() && chars[index] != '`' {
                plain.push(chars[index]);
                index += 1;
            }
            index += 1;
            continue;
        }
        if chars[index] == '['
            && let Some((label, next)) = link_label(&chars, index)
        {
            plain.push_str(&label);
            index = next;
            continue;
        }
        plain.push(chars[index]);
        index += 1;
    }
    let mut out = String::new();
    for ch in plain.trim().chars() {
        let lower = if ch.is_ascii_uppercase() {
            ch.to_ascii_lowercase()
        } else {
            ch
        };
        if lower.is_alphanumeric() || lower == ' ' || lower == '-' || lower == '_' {
            if lower == ' ' {
                out.push('-');
            } else {
                out.push(lower);
            }
        }
    }
    out
}

fn link_label(chars: &[char], open: usize) -> Option<(String, usize)> {
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

fn scan_line(file: &str, line: usize, text: &str, fixture: bool, short_flag: bool) {
    for prefix in [
        format!("/{}", "Users"),
        format!("/{}", "home"),
        format!("/{}", "private"),
        format!("/{}", "tmp"),
        format!("/{}/{}", "var", "folders"),
    ] {
        assert!(!text.contains(&prefix), "{file}:{line}: local path prefix");
    }
    let authored = format!("{}{}", "Co-authored-b", "y");
    let generated = format!("{}{}", "Generated ", "with");
    assert!(!text.contains(&authored), "{file}:{line}: attribution line");
    assert!(
        !text.contains(&generated),
        "{file}:{line}: attribution line"
    );
    let flag = format!("{}{}", "un", "attended");
    let collapsed: String = text
        .chars()
        .filter(|ch| *ch != ' ' && *ch != '\t')
        .collect();
    for form in [
        format!("{flag}:true"),
        format!("\"{flag}\":true"),
        format!("{flag}=true"),
    ] {
        assert!(!collapsed.contains(&form), "{file}:{line}: flag set true");
    }
    assert!(!has_email(text), "{file}:{line}: email address");
    for name in profile_names(text, short_flag) {
        assert!(
            name == "default" || name == "fixture" || name == "<name>",
            "{file}:{line}: profile name {name}"
        );
    }
    let delete_command = text
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '`' | ';' | '&' | '|'))
        .any(|word| word == "rm");
    if delete_command && text.contains(".talaria") {
        panic!("{file}:{line}: advice to delete the configuration directory");
    }
    if fixture {
        assert!(!text.contains("://"), "{file}:{line}: address in a fixture");
    }
    if !text.contains("Do not") && install_then_name(text) {
        panic!("{file}:{line}: registry install of the command");
    }
}

fn has_email(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'@' {
            let mut left = index;
            while left > 0 && is_email_local(bytes[left - 1]) {
                left -= 1;
            }
            if left < index {
                let mut right = index + 1;
                let mut dots = 0;
                let mut label = 0;
                let mut tld = 0;
                while right < bytes.len() && (is_email_domain(bytes[right]) || bytes[right] == b'.')
                {
                    if bytes[right] == b'.' {
                        if label == 0 {
                            break;
                        }
                        dots += 1;
                        tld = 0;
                        label = 0;
                    } else if bytes[right].is_ascii_alphabetic() && dots > 0 {
                        tld += 1;
                        label += 1;
                    } else {
                        label += 1;
                        tld = 0;
                    }
                    right += 1;
                }
                if dots > 0 && tld >= 2 {
                    return true;
                }
            }
        }
        index += 1;
    }
    false
}

fn is_email_local(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'%' | b'+' | b'-')
}

fn is_email_domain(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-'
}

fn profile_names(text: &str, short_flag: bool) -> Vec<String> {
    let mut names = Vec::new();
    let words: Vec<&str> = text.split_whitespace().collect();
    for (index, word) in words.iter().enumerate() {
        let flag = *word == "--profile" || (short_flag && *word == "-P");
        if flag && let Some(next) = words.get(index + 1) {
            names.push(
                (*next)
                    .trim_matches(|ch| ch == '"' || ch == '\'' || ch == ',')
                    .to_string(),
            );
        }
        if *word == "profile"
            && words.get(index + 1) == Some(&"named")
            && let Some(next) = words.get(index + 2)
        {
            names.push(
                (*next)
                    .trim_matches(|ch| matches!(ch, '"' | '\'' | ',' | '.'))
                    .to_string(),
            );
        }
    }
    names
}

fn install_then_name(text: &str) -> bool {
    for phrase in [
        "npm install",
        "cargo install",
        "pip install",
        "uv tool install",
    ] {
        if let Some(at) = text.find(phrase) {
            let after = &text[at + phrase.len()..];
            if after.contains("git+") {
                continue;
            }
            for word in after.split_whitespace() {
                let bare = word.trim_matches(|ch| matches!(ch, '`' | '"' | '\'' | ',' | '.'));
                if bare.starts_with("talaria@") {
                    return false;
                }
                if bare == "talaria" {
                    return true;
                }
            }
        }
    }
    false
}

fn help_has_short_profile() -> bool {
    help(&[]).contains("-P")
}

fn help(args: &[&str]) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_talaria"));
    command.args(args).arg("--help").current_dir(root());
    let output = command.output().expect("help");
    assert!(output.status.success(), "help exited {}", output.status);
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn check_bash_commands(root: &Path, file: &str) {
    let text = read(root, file);
    let mut fenced = false;
    let mut bash = false;
    for (number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if fenced {
                fenced = false;
                bash = false;
            } else {
                fenced = true;
                bash = trimmed == "```bash";
            }
            continue;
        }
        if !bash {
            continue;
        }
        let mut words = trimmed.split_whitespace();
        if words.next() != Some("talaria") {
            continue;
        }
        let mut path: Vec<String> = Vec::new();
        let rest: Vec<&str> = words.collect();
        let mut cursor = help(&path.iter().map(String::as_str).collect::<Vec<_>>());
        for word in &rest {
            if word.starts_with('-') {
                break;
            }
            if subcommands(&cursor).contains(*word) {
                path.push((*word).to_string());
                cursor = help(&path.iter().map(String::as_str).collect::<Vec<_>>());
            } else {
                break;
            }
        }
        let final_help = help(&path.iter().map(String::as_str).collect::<Vec<_>>());
        for word in &rest {
            if let Some(flag) = word.strip_prefix("--") {
                let flag = flag.split('=').next().unwrap();
                if flag.is_empty() {
                    continue;
                }
                let needle = format!("--{flag}");
                assert!(
                    final_help.contains(&needle),
                    "{file}:{}: {needle} is not in help for talaria {}",
                    number + 1,
                    path.join(" ")
                );
            }
        }
    }
}

fn subcommands(help_text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut inside = false;
    for line in help_text.lines() {
        if !inside {
            if line.trim() == "Commands:" {
                inside = true;
            }
            continue;
        }
        if line.trim().is_empty() || (!line.starts_with(' ') && line.contains(':')) {
            break;
        }
        if let Some(name) = line.split_whitespace().next()
            && name.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-')
        {
            names.insert(name.to_string());
        }
    }
    names
}

fn action_groups() -> Vec<(KeyContext, Vec<Action>)> {
    let mut groups: Vec<(KeyContext, Vec<Action>)> = Vec::new();
    for action in Action::ALL {
        let context = action.context();
        if let Some((_, list)) = groups.iter_mut().find(|(found, _)| *found == context) {
            list.push(action);
        } else {
            groups.push((context, vec![action]));
        }
    }
    groups
}

fn context_heading(context: KeyContext) -> String {
    let debug = format!("{context:?}");
    let mut words = Vec::new();
    let mut current = String::new();
    for ch in debug.chars() {
        if ch.is_uppercase() && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() {
        words.push(current);
    }
    let mut titled = Vec::new();
    for (index, word) in words.iter().enumerate() {
        if index == 0 {
            let mut chars = word.chars();
            let first = chars.next().unwrap().to_uppercase().to_string();
            titled.push(format!("{first}{}", chars.as_str().to_lowercase()));
        } else {
            titled.push(word.to_lowercase());
        }
    }
    format!("## {} keys", titled.join(" "))
}

struct KeyRow {
    line: usize,
    default_keys: String,
    config: String,
}

fn table_after_heading(text: &str, heading: &str) -> Option<Vec<KeyRow>> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|line| line.trim() == heading)?;
    let mut index = start + 1;
    while index < lines.len() && !lines[index].trim().starts_with('|') {
        if lines[index].trim().starts_with("## ") {
            return None;
        }
        index += 1;
    }
    if index >= lines.len() {
        return None;
    }
    let mut rows = Vec::new();
    while index < lines.len() && lines[index].trim().starts_with('|') {
        let cells = split_row(lines[index]);
        if cells.len() >= 4 && !cells[0].contains("---") && cells[0] != "Action" {
            rows.push(KeyRow {
                line: index + 1,
                default_keys: cells[1].clone(),
                config: cells[3].clone(),
            });
        }
        index += 1;
    }
    Some(rows)
}

fn split_row(line: &str) -> Vec<String> {
    let mut cells: Vec<String> = line
        .trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().trim_matches('`').trim().to_string())
        .collect();
    if cells.first().is_some_and(String::is_empty) {
        cells.remove(0);
    }
    if cells.last().is_some_and(String::is_empty) {
        cells.pop();
    }
    cells
}

fn chord_cell(cell: &str) -> BTreeSet<String> {
    split_chords(&cell.to_lowercase()).into_iter().collect()
}

fn split_chords(cell: &str) -> Vec<String> {
    if cell.trim() == "none" {
        return Vec::new();
    }
    cell.split(',')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty() && part != "none")
        .collect()
}

struct SettingRow {
    line: usize,
    setting: String,
    outcome: String,
    new_path: String,
}

fn migration_rows(text: &str) -> Vec<SettingRow> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.trim() == "## Your config file")
        .unwrap_or_else(|| panic!("{MIGRATION}:1: the table does not exist"));
    let mut index = start + 1;
    while index < lines.len() && !lines[index].trim().starts_with('|') {
        index += 1;
    }
    assert!(
        index < lines.len(),
        "{MIGRATION}:1: the table does not exist"
    );
    let header = split_row(lines[index]);
    assert_eq!(
        header,
        vec!["0.6.5 setting", "Outcome", "0.7.0 setting", "What to do"],
        "{MIGRATION}:{}: table columns differ",
        index + 1
    );
    index += 1;
    let mut rows = Vec::new();
    while index < lines.len() && lines[index].trim().starts_with('|') {
        let cells = split_row(lines[index]);
        if cells.len() >= 4 && !cells[0].contains('-') {
            let outcome = cells[1].clone();
            assert!(
                matches!(
                    outcome.as_str(),
                    "kept" | "kept with a warning" | "moved" | "notice"
                ),
                "{MIGRATION}:{}: outcome {outcome} is not one of the four",
                index + 1
            );
            rows.push(SettingRow {
                line: index + 1,
                setting: cells[0].clone(),
                outcome,
                new_path: cells[2].clone(),
            });
        }
        index += 1;
    }
    rows
}

fn fixture_settings(root: &Path) -> BTreeMap<String, toml::Value> {
    let mut all = BTreeMap::new();
    for file in [
        "fixtures/config/config-0.6.5-defaults.toml",
        "fixtures/config/config-0.6.5-custom.toml",
    ] {
        let text = read(root, file);
        let table: toml::Table =
            toml::from_str(&text).unwrap_or_else(|error| panic!("{file}:1: {error}"));
        let mut found = Vec::new();
        collect_settings(&table, "", &mut found);
        for (path, value) in found {
            all.entry(path).or_insert(value);
        }
    }
    all
}

fn collect_settings(table: &toml::Table, prefix: &str, out: &mut Vec<(String, toml::Value)>) {
    if table.is_empty() && !prefix.is_empty() {
        out.push((prefix.to_string(), toml::Value::Table(table.clone())));
        return;
    }
    for (key, value) in table {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::Table(child) if child.is_empty() => {
                out.push((path, value.clone()));
            }
            toml::Value::Table(child) => collect_settings(child, &path, out),
            other => out.push((path, other.clone())),
        }
    }
}

fn expand_setting(pattern: &str, universe: &BTreeMap<String, toml::Value>) -> Vec<String> {
    if let Some(table) = pattern.strip_suffix(".*") {
        universe
            .keys()
            .filter(|path| path.as_str() == table || path.starts_with(&format!("{table}.")))
            .cloned()
            .collect()
    } else if universe.contains_key(pattern) {
        vec![pattern.to_string()]
    } else {
        Vec::new()
    }
}

fn insert_path(root: &mut toml::Table, path: &str, value: toml::Value) {
    let mut parts: Vec<&str> = path.split('.').collect();
    let last = parts.pop().expect("path");
    let mut cursor = root;
    for part in parts {
        let entry = cursor
            .entry(part)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        cursor = entry.as_table_mut().expect("table");
    }
    cursor.insert(last.to_string(), value);
}

fn one_setting_text(path: &str, value: &toml::Value) -> String {
    let mut table = toml::Table::new();
    insert_path(&mut table, path, value.clone());
    toml::to_string(&toml::Value::Table(table)).expect("setting")
}

fn pipeline_file(path: &Path) -> Vec<String> {
    let loaded = talaria_app::config::load_file(path);
    let (_map, key_notices) = KeyMap::from_entries(&loaded.config.keys);
    loaded.notices.into_iter().chain(key_notices).collect()
}

fn pipeline_for(dir: &Path, path: &str, value: &toml::Value) -> Vec<String> {
    let file = dir.join(path.replace('.', "-") + ".toml");
    fs::write(&file, one_setting_text(path, value)).expect("write setting");
    pipeline_file(&file)
}

fn chords_for(
    dir: &Path,
    path: &str,
    value: &toml::Value,
    action: Action,
) -> (Vec<String>, BTreeSet<String>) {
    let file = dir.join(format!("chords-{}.toml", path.replace('.', "-")));
    fs::write(&file, one_setting_text(path, value)).expect("write setting");
    let loaded = talaria_app::config::load_file(&file);
    let (map, notices) = KeyMap::from_entries(&loaded.config.keys);
    let labels = map
        .chords(action)
        .iter()
        .map(|chord| chord.label().to_lowercase())
        .collect();
    (notices, labels)
}

fn value_chords(value: &toml::Value) -> BTreeSet<String> {
    match value {
        toml::Value::String(text) => chord_cell(text),
        toml::Value::Array(items) => items
            .iter()
            .filter_map(toml::Value::as_str)
            .flat_map(chord_cell)
            .collect(),
        _ => BTreeSet::new(),
    }
}

fn text_block_after_table(text: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.trim() == "## Your config file")?;
    let mut index = start + 1;
    while index < lines.len() && !lines[index].trim().starts_with('|') {
        index += 1;
    }
    while index < lines.len() && lines[index].trim().starts_with('|') {
        index += 1;
    }
    while index < lines.len() && lines[index].trim() != "```text" {
        if lines[index].trim().starts_with("## ") {
            return None;
        }
        index += 1;
    }
    if index >= lines.len() {
        return None;
    }
    index += 1;
    let mut block = Vec::new();
    while index < lines.len() && lines[index].trim() != "```" {
        block.push(lines[index].to_string());
        index += 1;
    }
    Some(block)
}

fn notice_count_sentence(line: &str) -> Option<usize> {
    let prefix = "A file holding every 0.6.5 default shows ";
    let suffix = " notices.";
    let number = line.strip_prefix(prefix)?.strip_suffix(suffix)?;
    if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}

fn section_until_next_h2(text: &str, heading: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|line| line.trim() == heading)?;
    let mut body = Vec::new();
    for line in lines.iter().skip(start + 1) {
        if line.starts_with("## ") {
            break;
        }
        body.push(*line);
    }
    Some(body.join("\n"))
}
