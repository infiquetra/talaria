//! Direct colours belong in the theme module. Like the boundary scans, this is
//! a text heuristic: mentions in comments and strings count too.

use std::path::{Path, PathBuf};

const COLOURS: &[&str] = &[
    "Rgb",
    "Indexed",
    "Red",
    "Green",
    "Yellow",
    "Blue",
    "Magenta",
    "Cyan",
    "Gray",
    "DarkGray",
    "LightRed",
    "LightGreen",
    "LightYellow",
    "LightBlue",
    "LightMagenta",
    "LightCyan",
    "White",
    "Black",
    "from",
];

#[derive(Clone)]
struct Token {
    text: String,
    line: usize,
}

fn tokens(source: &str) -> Vec<Token> {
    let mut result = Vec::new();
    for (index, line) in source.lines().enumerate() {
        if line.trim().starts_with("#[cfg(test)]") {
            break;
        }
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch.is_whitespace() {
                continue;
            }
            let mut text = ch.to_string();
            if ch.is_alphanumeric() || ch == '_' {
                while let Some(next) = chars.peek() {
                    if !next.is_alphanumeric() && *next != '_' {
                        break;
                    }
                    text.push(chars.next().unwrap());
                }
            } else if ch == ':' && chars.peek() == Some(&':') {
                chars.next();
                text.push(':');
            }
            result.push(Token {
                text,
                line: index + 1,
            });
        }
    }
    result
}

struct Import {
    path: Vec<String>,
    alias: Option<String>,
    line: usize,
}

// Expand use trees so `style::{Color::{Red as Warning}, palette}` has the
// same meaning for the guard as separate fully qualified imports.
fn imports(tokens: &[Token], cursor: &mut usize, prefix: &[String], found: &mut Vec<Import>) {
    let mut path = prefix.to_vec();
    let line = tokens.get(*cursor).map_or(1, |token| token.line);
    while let Some(token) = tokens.get(*cursor) {
        match token.text.as_str() {
            "::" => *cursor += 1,
            "{" => {
                *cursor += 1;
                while *cursor < tokens.len() && tokens[*cursor].text != "}" {
                    imports(tokens, cursor, &path, found);
                    if tokens.get(*cursor).is_some_and(|token| token.text == ",") {
                        *cursor += 1;
                    }
                }
                *cursor += usize::from(*cursor < tokens.len());
                return;
            }
            "," | "}" | ";" => break,
            "as" => {
                *cursor += 1;
                let alias = tokens.get(*cursor).map(|token| token.text.clone());
                *cursor += usize::from(*cursor < tokens.len());
                found.push(Import { path, alias, line });
                return;
            }
            _ => {
                path.push(token.text.clone());
                *cursor += 1;
            }
        }
    }
    found.push(Import {
        path,
        alias: None,
        line,
    });
}

fn source_hits(source: &str) -> Vec<(usize, String)> {
    let tokens = tokens(source);
    let mut imported = Vec::new();
    let mut in_use = vec![false; tokens.len()];
    let mut cursor = 0;
    while cursor < tokens.len() {
        if tokens[cursor].text == "use" {
            let start = cursor;
            let end = tokens[cursor..]
                .iter()
                .position(|token| token.text == ";")
                .map_or(tokens.len(), |end| cursor + end);
            cursor += 1;
            imports(&tokens[..end], &mut cursor, &[], &mut imported);
            in_use[start..end].fill(true);
            cursor = end;
        }
        cursor += 1;
    }
    let mut names = vec!["Color".to_string()];
    for import in &imported {
        if import.path.last().is_some_and(|name| name == "Color")
            && let Some(alias) = &import.alias
        {
            names.push(alias.clone());
        }
    }
    let mut hits = Vec::new();
    for import in imported {
        if import.path.windows(2).any(|pair| {
            names.contains(&pair[0]) && (COLOURS.contains(&pair[1].as_str()) || pair[1] == "*")
                || pair[0] == "style" && pair[1] == "palette"
        }) {
            hits.push((import.line, format!("use {}", import.path.join("::"))));
        }
    }
    for (index, path) in tokens.windows(3).enumerate() {
        if !in_use[index] && path[1].text == "::" {
            if names.contains(&path[0].text) && COLOURS.contains(&path[2].text.as_str()) {
                hits.push((path[0].line, format!("{}::{}", path[0].text, path[2].text)));
            } else if path[0].text == "style" && path[2].text == "palette" {
                hits.push((path[0].line, "style::palette".to_string()));
            }
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

fn colour_violations(root: &Path) -> Vec<String> {
    let crates = root.join("crates");
    let mut pending = Vec::new();
    if crates.exists() {
        for entry in std::fs::read_dir(&crates).expect("read crates") {
            let src = entry.expect("read crate entry").path().join("src");
            if src.is_dir() {
                pending.push(src);
            }
        }
    }
    let mut found = Vec::new();
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(folder).expect("read source directory") {
            let path = entry.expect("read source entry").path();
            let relative = path.strip_prefix(root).expect("source is under root");
            let relative = relative.to_string_lossy().replace('\\', "/");
            if path.is_dir() {
                if !matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some("tests" | "examples")
                ) && relative != "crates/talaria-tui/src/theme"
                {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path).expect("read Rust source");
                for (line, token) in source_hits(&source) {
                    found.push(format!("{relative}:{line}: {token}"));
                }
            }
        }
    }
    found.sort();
    found
}

fn scratch(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("clear planted directory");
    }
    std::fs::create_dir_all(&root).expect("create planted directory");
    root
}

fn plant(root: &Path, relative: &str, source: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).expect("create planted parent");
    std::fs::write(path, source).expect("write planted source");
}

#[test]
fn guard_reports_direct_colours_and_imports_with_line_evidence() {
    let root = scratch("colour_guard_syntax");
    let cases = [
        ("Color::Red", "Color::Red"),
        ("Color :: Rgb(1, 2, 3)", "Color::Rgb"),
        ("Color::Indexed(1)", "Color::Indexed"),
        ("Color::from(1)", "Color::from"),
        ("ratatui::style::palette::tailwind::RED", "style::palette"),
        (
            "use ratatui::style::Color::Red as Warning;",
            "use ratatui::style::Color::Red",
        ),
        (
            "use ratatui::style::{Color::{LightBlue as Accent}};",
            "use ratatui::style::Color::LightBlue",
        ),
        (
            "use ratatui::style::{palette as colours};",
            "use ratatui::style::palette",
        ),
        (
            "use ratatui::style::Color::*;",
            "use ratatui::style::Color::*",
        ),
        ("use ratatui::style::{Color as C};\nC::Yellow", "C::Yellow"),
        ("Color\n::\nGreen", "Color::Green"),
    ];
    for (index, (source, expected)) in cases.into_iter().enumerate() {
        let relative = format!("crates/x/src/nested/case_{index}.rs");
        plant(&root, &relative, source);
        let line = if source.contains("C::Yellow") { 2 } else { 1 };
        let expected = format!("{relative}:{line}: {expected}");
        assert!(
            colour_violations(&root).contains(&expected),
            "missing {expected}"
        );
    }
    for colour in COLOURS {
        assert_eq!(
            source_hits(&format!("Color::{colour}")),
            [(1, format!("Color::{colour}"))]
        );
    }
}

#[test]
fn guard_limits_its_scan_to_production_sources_outside_theme() {
    let root = scratch("colour_guard_scope");
    for relative in [
        "crates/x/tests/test.rs",
        "crates/x/examples/demo.rs",
        "crates/x/src/tests/test.rs",
        "crates/x/src/examples/demo.rs",
        "crates/talaria-tui/src/theme/legacy.rs",
        "crates/talaria-tui/src/theme/nested/x.rs",
        "tools/x/src/lib.rs",
        "crates/x/lib.rs",
        "crates/x/src/notes.txt",
    ] {
        plant(&root, relative, "Color::Red\n");
    }
    plant(
        &root,
        "crates/x/src/lib.rs",
        "Color::Reset\nuse ratatui::style::Color::Reset;\n#[cfg(test)]\nColor::Red\n",
    );
    // A similarly named module is not the permitted theme directory.
    plant(&root, "crates/x/src/theme/mod.rs", "Color::Red\n");
    assert_eq!(
        colour_violations(&root),
        ["crates/x/src/theme/mod.rs:1: Color::Red"]
    );
}

#[test]
fn production_colours_live_only_in_the_theme_module() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let found = colour_violations(&root);
    assert!(
        found.is_empty(),
        "direct production colours outside theme:\n{}",
        found.join("\n")
    );
}
