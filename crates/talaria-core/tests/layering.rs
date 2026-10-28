use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;

const PLANNED_CRATES: [&str; 7] = [
    "hermes-contract",
    "hermes-client",
    "talaria-core",
    "talaria-app",
    "talaria-tui",
    "talaria-workflow",
    "talaria-cli",
];

const DOMAIN_CRATES: [&str; 5] = [
    "hermes-contract",
    "hermes-client",
    "talaria-core",
    "talaria-app",
    "talaria-workflow",
];

const TERMINAL_LIBRARIES: [&str; 2] = ["ratatui", "crossterm"];

const INHERITED_PACKAGE_LINES: [&str; 6] = [
    "version.workspace = true",
    "edition.workspace = true",
    "rust-version.workspace = true",
    "license.workspace = true",
    "repository.workspace = true",
    "publish.workspace = true",
];

fn workspace_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

struct Graph {
    edges: BTreeMap<String, BTreeSet<String>>,
}

impl Graph {
    /// A missing crate is not this graph's concern: tests 5 to 7 judge only crates that are present.
    fn path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        if !self.edges.contains_key(from) {
            return None;
        }
        let mut previous: BTreeMap<String, String> = BTreeMap::new();
        let mut queue = VecDeque::from([from.to_string()]);
        let mut seen = BTreeSet::from([from.to_string()]);
        let mut reached = false;
        while let Some(current) = queue.pop_front() {
            if current == to {
                reached = true;
                break;
            }
            let Some(dependencies) = self.edges.get(&current) else {
                continue;
            };
            for dependency in dependencies {
                if seen.insert(dependency.clone()) {
                    previous.insert(dependency.clone(), current.clone());
                    queue.push_back(dependency.clone());
                }
            }
        }
        if !reached || from == to {
            return None;
        }
        let mut names = vec![to.to_string()];
        let mut cursor = to;
        while cursor != from {
            let parent = previous.get(cursor)?;
            names.push(parent.clone());
            cursor = parent;
        }
        names.reverse();
        Some(names)
    }
}

struct Member {
    name: String,
    manifest: String,
}

/// `--all-features` is required. Without it, an optional dependency that only a feature turns on
/// (for example a future feature of talaria-core that enables ratatui) is absent from the graph
/// and the layering tests would pass.
///
/// The cargo binary is the `CARGO` variable when cargo set it, otherwise `cargo` on `PATH`.
fn cargo_metadata(root: &Path) -> serde_json::Value {
    let program = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(program)
        .current_dir(root)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--all-features",
            "--manifest-path",
            "Cargo.toml",
        ])
        .output()
        .unwrap_or_else(|_| panic!("cargo metadata could not run"));
    assert!(
        output.status.success(),
        "cargo metadata --locked failed with status {}",
        output.status
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("cargo metadata returned JSON this test could not read"))
}

fn package_names(metadata: &serde_json::Value) -> BTreeMap<String, String> {
    let packages = metadata
        .get("packages")
        .and_then(|value| value.as_array())
        .unwrap_or_else(|| panic!("cargo metadata has no packages array"));
    let mut names = BTreeMap::new();
    for package in packages {
        let id = package
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| panic!("cargo metadata package has no id"));
        let name = package
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| panic!("cargo metadata package has no name"));
        names.insert(id.to_string(), name.to_string());
    }
    names
}

fn counts_as_edge(kind: &serde_json::Value) -> bool {
    kind.is_null() || kind.as_str() == Some("build")
}

fn workspace_graph(metadata: &serde_json::Value) -> Graph {
    let names = package_names(metadata);
    let nodes = metadata
        .get("resolve")
        .and_then(|value| value.get("nodes"))
        .and_then(|value| value.as_array())
        .unwrap_or_else(|| panic!("cargo metadata has no resolve.nodes array"));
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for node in nodes {
        let id = node
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| panic!("cargo metadata node has no id"));
        let name = names
            .get(id)
            .unwrap_or_else(|| panic!("cargo metadata node is not in packages"))
            .clone();
        let mut dependencies = BTreeSet::new();
        let deps = node
            .get("deps")
            .and_then(|value| value.as_array())
            .unwrap_or_else(|| panic!("cargo metadata node has no deps array"));
        for dep in deps {
            let kinds = dep
                .get("dep_kinds")
                .and_then(|value| value.as_array())
                .unwrap_or_else(|| panic!("cargo metadata dependency has no dep_kinds array"));
            if !kinds.iter().any(|kind| counts_as_edge(&kind["kind"])) {
                continue;
            }
            let pkg = dep
                .get("pkg")
                .and_then(|value| value.as_str())
                .unwrap_or_else(|| panic!("cargo metadata dependency has no pkg"));
            let dependency_name = names
                .get(pkg)
                .unwrap_or_else(|| panic!("cargo metadata dependency is not in packages"));
            dependencies.insert(dependency_name.clone());
        }
        edges.insert(name, dependencies);
    }
    Graph { edges }
}

fn relative_manifest(manifest: &str, root: &Path) -> String {
    let manifest_path = std::fs::canonicalize(Path::new(manifest)).unwrap_or_else(|_| {
        panic!("a workspace member manifest could not be resolved inside the repository")
    });
    let root = std::fs::canonicalize(root)
        .unwrap_or_else(|_| panic!("the repository root could not be resolved"));
    match manifest_path.strip_prefix(&root) {
        Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
        Err(_) => panic!("a workspace member manifest is outside the repository"),
    }
}

fn workspace_members(metadata: &serde_json::Value, root: &Path) -> Vec<Member> {
    let member_ids = metadata
        .get("workspace_members")
        .and_then(|value| value.as_array())
        .unwrap_or_else(|| panic!("cargo metadata has no workspace_members array"));
    let packages = metadata
        .get("packages")
        .and_then(|value| value.as_array())
        .unwrap_or_else(|| panic!("cargo metadata has no packages array"));
    let mut members = Vec::new();
    for member_id in member_ids {
        let id = member_id
            .as_str()
            .unwrap_or_else(|| panic!("cargo metadata workspace member id is not a string"));
        let package = packages
            .iter()
            .find(|package| package.get("id").and_then(|value| value.as_str()) == Some(id))
            .unwrap_or_else(|| panic!("cargo metadata workspace member is not in packages"));
        let name = package
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| panic!("cargo metadata package has no name"))
            .to_string();
        let manifest = package
            .get("manifest_path")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| panic!("cargo metadata package has no manifest_path"));
        members.push(Member {
            name,
            manifest: relative_manifest(manifest, root),
        });
    }
    members
}

fn section_lines<'a>(text: &'a str, header: &str) -> Vec<&'a str> {
    let mut lines = Vec::new();
    let mut collecting = false;
    for line in text.lines() {
        if line.trim_start().starts_with('[') {
            if collecting {
                break;
            }
            collecting = line.trim() == header;
            continue;
        }
        if collecting {
            lines.push(line);
        }
    }
    lines
}

fn has_line(lines: &[&str], expected: &str) -> bool {
    lines.iter().any(|line| line.trim() == expected)
}

fn has_header(text: &str, header: &str) -> bool {
    text.lines().any(|line| line.trim() == header)
}

fn quoted_assignment<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.trim().strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn parse_exact_release(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn version_requirement(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if let Some(brace) = trimmed.find('{') {
        let inside = &trimmed[brace + 1..];
        let key_at = inside.find("version")?;
        let after_key = inside[key_at + "version".len()..].trim_start();
        let after_eq = after_key.strip_prefix('=')?.trim_start();
        let quoted = after_eq.strip_prefix('"')?;
        let end = quoted.find('"')?;
        return Some(&quoted[..end]);
    }
    let after_eq = trimmed.split_once('=')?.1.trim_start();
    let quoted = after_eq.strip_prefix('"')?;
    let end = quoted.find('"')?;
    Some(&quoted[..end])
}

fn assert_no_path(graph: &Graph, from: &str, to: &str) {
    let path = graph.path(from, to);
    assert!(
        path.is_none(),
        "forbidden dependency path {}",
        path.as_ref()
            .map(|steps| steps.join(" -> "))
            .unwrap_or_default()
    );
}

#[test]
fn workspace_holds_the_seven_planned_crates() {
    let root = workspace_root();
    let metadata = cargo_metadata(&root);
    let members = workspace_members(&metadata, &root);
    let mut problems = Vec::new();
    for name in PLANNED_CRATES {
        let expected = format!("crates/{name}/Cargo.toml");
        let found = members
            .iter()
            .any(|member| member.name == name && member.manifest == expected);
        if !found {
            problems.push(format!("missing member {expected}"));
        }
    }
    for member in &members {
        if member.manifest.starts_with("crates/") {
            if !PLANNED_CRATES.contains(&member.name.as_str()) {
                problems.push(format!(
                    "unexpected member under crates/: {}",
                    member.manifest
                ));
            }
        } else if !member.manifest.starts_with("tools/") {
            problems.push(format!("member outside tools/: {}", member.manifest));
        }
    }
    let present = members
        .iter()
        .map(|member| member.manifest.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    assert!(
        problems.is_empty(),
        "workspace members are not the seven planned crates:\n{}\nmembers present: {present}",
        problems.join("\n")
    );
}

#[test]
fn domain_crates_never_reach_a_terminal_library() {
    let graph = workspace_graph(&cargo_metadata(&workspace_root()));
    let mut violations = Vec::new();
    for domain in DOMAIN_CRATES {
        if !graph.edges.contains_key(domain) {
            continue;
        }
        for terminal in TERMINAL_LIBRARIES {
            if let Some(path) = graph.path(domain, terminal) {
                violations.push(path.join(" -> "));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "forbidden dependency path {}",
        violations.join("; ")
    );
}

#[test]
fn talaria_core_never_reaches_hermes_client() {
    let graph = workspace_graph(&cargo_metadata(&workspace_root()));
    assert_no_path(&graph, "talaria-core", "hermes-client");
}

#[test]
fn talaria_workflow_never_reaches_talaria_tui() {
    let graph = workspace_graph(&cargo_metadata(&workspace_root()));
    assert_no_path(&graph, "talaria-workflow", "talaria-tui");
}

#[test]
fn workflow_progress_edges_are_present() {
    let graph = workspace_graph(&cargo_metadata(&workspace_root()));
    for (from, to) in [
        ("talaria-workflow", "hermes-contract"),
        ("talaria-workflow", "talaria-core"),
        ("talaria-app", "talaria-workflow"),
        ("talaria-tui", "talaria-workflow"),
    ] {
        assert!(
            graph.path(from, to).is_some(),
            "missing dependency {from} -> {to}"
        );
    }
}

#[test]
fn every_crate_inherits_the_workspace_settings() {
    // Text, not a TOML parser: cargo metadata does not report lint settings or whether a key was
    // inherited, and a toml dependency would be a new pin outside this unit's dependency set.
    let root = workspace_root();
    let metadata = cargo_metadata(&root);
    let root_text = std::fs::read_to_string(root.join("Cargo.toml"))
        .unwrap_or_else(|_| panic!("Cargo.toml could not be read"));
    let mut missing = Vec::new();
    if !has_line(
        &section_lines(&root_text, "[workspace.lints.rust]"),
        "unsafe_code = \"forbid\"",
    ) {
        missing.push(
            "Cargo.toml [workspace.lints.rust] is missing `unsafe_code = \"forbid\"`".to_string(),
        );
    }
    for member in workspace_members(&metadata, &root) {
        let text = std::fs::read_to_string(root.join(&member.manifest))
            .unwrap_or_else(|_| panic!("{} could not be read", member.manifest));
        if !has_line(&section_lines(&text, "[lints]"), "workspace = true") {
            missing.push(format!("{} is missing `workspace = true`", member.manifest));
        }
        let package = section_lines(&text, "[package]");
        for line in INHERITED_PACKAGE_LINES {
            if !has_line(&package, line) {
                missing.push(format!("{} is missing `{line}`", member.manifest));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "workspace settings are not inherited:\n{}",
        missing.join("\n")
    );
}

#[test]
fn toolchain_and_external_versions_are_pinned_exactly() {
    let root = workspace_root();
    let toolchain_rel = "rust-toolchain.toml";
    let toolchain_path = root.join(toolchain_rel);
    assert!(toolchain_path.is_file(), "{toolchain_rel} is absent");
    let toolchain = std::fs::read_to_string(&toolchain_path)
        .unwrap_or_else(|_| panic!("{toolchain_rel} could not be read"));
    let channel = toolchain
        .lines()
        .find_map(|line| quoted_assignment(line, "channel"))
        .unwrap_or_else(|| panic!("{toolchain_rel} has no channel pin"));
    let parsed = parse_exact_release(channel)
        .unwrap_or_else(|| panic!("{toolchain_rel} channel is not an exact x.y.z"));
    assert!(
        parsed >= (1, 98, 1),
        "{toolchain_rel} channel is older than 1.98.1"
    );

    let root_rel = "Cargo.toml";
    let root_text = std::fs::read_to_string(root.join(root_rel))
        .unwrap_or_else(|_| panic!("{root_rel} could not be read"));
    assert!(
        has_header(&root_text, "[workspace.package]"),
        "{root_rel} has no [workspace.package] section"
    );
    let rust_version = section_lines(&root_text, "[workspace.package]")
        .into_iter()
        .find_map(|line| quoted_assignment(line, "rust-version"))
        .unwrap_or_else(|| panic!("{root_rel} [workspace.package] has no rust-version"));
    assert_eq!(
        channel, rust_version,
        "{toolchain_rel} channel does not equal {root_rel} rust-version"
    );

    assert!(
        has_header(&root_text, "[workspace.dependencies]"),
        "{root_rel} has no [workspace.dependencies] section"
    );
    let mut unpinned = Vec::new();
    for line in section_lines(&root_text, "[workspace.dependencies]") {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.contains("path =") {
            continue;
        }
        let name = trimmed.split('=').next().unwrap_or(trimmed).trim();
        match version_requirement(trimmed) {
            Some(requirement) if requirement.starts_with('=') => {}
            _ => unpinned.push(name.to_string()),
        }
    }
    assert!(
        unpinned.is_empty(),
        "workspace dependency is not pinned with '=': {}",
        unpinned.join(", ")
    );
}
