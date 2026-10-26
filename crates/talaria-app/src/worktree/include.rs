//! `.worktreeinclude` copies and links.

use std::path::{Component, Path};

use crate::worktree::git::{GitRunner, safe_text};

/// Apply `.worktreeinclude` from `main_root` to `tree`, and return one warning per entry
/// that was skipped.
///
/// A file is copied, a folder is linked to the main checkout's copy, and an entry that is
/// absolute, climbs out with `..`, is a symbolic link, or resolves outside either root is
/// skipped. A destination that already exists is left alone. Nothing here fails a launch.
pub async fn apply_include(_git: &GitRunner, main_root: &Path, tree: &Path) -> Vec<String> {
    let main_root = main_root.to_path_buf();
    let tree = tree.to_path_buf();
    let applied = tokio::task::spawn_blocking(move || apply(&main_root, &tree)).await;
    applied.unwrap_or_else(|_| vec![".worktreeinclude could not be applied.".to_owned()])
}

fn skipped(entry: &str, reason: &str) -> String {
    format!(
        ".worktreeinclude entry \"{}\" was skipped ({reason}).",
        safe_text(entry)
    )
}

fn apply(main_root: &Path, tree: &Path) -> Vec<String> {
    let list = main_root.join(".worktreeinclude");
    let bytes = match std::fs::read(&list) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            return vec![format!(
                ".worktreeinclude could not be read ({}).",
                safe_text(&error.to_string())
            )];
        }
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return vec![".worktreeinclude is not valid UTF-8 and was ignored.".to_owned()];
    };
    let (Ok(canonical_root), Ok(canonical_tree)) = (
        std::fs::canonicalize(main_root),
        std::fs::canonicalize(tree),
    ) else {
        return vec![".worktreeinclude could not be applied.".to_owned()];
    };
    let mut warnings = Vec::new();
    for line in text.strip_prefix('\u{feff}').unwrap_or(&text).lines() {
        let entry = line.trim();
        if entry.is_empty() || entry.starts_with('#') {
            continue;
        }
        if let Err(reason) = place(entry, main_root, tree, &canonical_root, &canonical_tree) {
            warnings.push(skipped(entry, &reason));
        }
    }
    warnings
}

/// Copy or link one entry. `Err` is the reason it was skipped.
fn place(
    entry: &str,
    main_root: &Path,
    tree: &Path,
    canonical_root: &Path,
    canonical_tree: &Path,
) -> Result<(), String> {
    let relative = Path::new(entry);
    if relative.is_absolute() {
        return Err("absolute paths are not allowed".to_owned());
    }
    let parts: Vec<Component<'_>> = relative.components().collect();
    if parts.contains(&Component::ParentDir) {
        return Err("it climbs out of the repository".to_owned());
    }
    if parts.first() == Some(&Component::Normal(".git".as_ref())) {
        return Err("it is inside the repository's own data".to_owned());
    }
    // Trailing separators are dropped, because a link followed by `/` would be followed.
    let trimmed = entry.trim_end_matches('/');
    let source = main_root.join(trimmed);
    let meta = walk_source(main_root, Path::new(trimmed))?;
    let canonical_source =
        std::fs::canonicalize(&source).map_err(|error| safe_text(&error.to_string()))?;
    if !canonical_source.starts_with(canonical_root) {
        return Err("it resolves outside the repository".to_owned());
    }
    let destination = tree.join(trimmed);
    prepare_parents(tree, Path::new(trimmed))?;
    if let Some(parent) = destination.parent() {
        let canonical_parent =
            std::fs::canonicalize(parent).map_err(|error| safe_text(&error.to_string()))?;
        if !canonical_parent.starts_with(canonical_tree) {
            return Err("its destination leaves the worktree".to_owned());
        }
    }
    if std::fs::symlink_metadata(&destination).is_ok() {
        return Ok(());
    }
    if meta.is_file() {
        std::fs::copy(&source, &destination)
            .map(|_| ())
            .map_err(|error| format!("it could not be copied: {}", safe_text(&error.to_string())))
    } else if meta.is_dir() {
        std::os::unix::fs::symlink(&canonical_source, &destination)
            .map_err(|error| format!("it could not be linked: {}", safe_text(&error.to_string())))
    } else {
        Err("it is neither a file nor a folder".to_owned())
    }
}

/// Look at every component of the source, so a link anywhere on the way is refused.
/// Returns the metadata of the last component.
fn walk_source(main_root: &Path, relative: &Path) -> Result<std::fs::Metadata, String> {
    let mut current = main_root.to_path_buf();
    let mut last = None;
    for part in relative.components() {
        current.push(part);
        let meta = std::fs::symlink_metadata(&current).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => "it does not exist".to_owned(),
            _ => safe_text(&error.to_string()),
        })?;
        if meta.file_type().is_symlink() {
            return Err("it is, or passes through, a symbolic link".to_owned());
        }
        last = Some(meta);
    }
    last.ok_or_else(|| "it is empty".to_owned())
}

/// Create the folders above the destination one level at a time, refusing as soon as an
/// existing component is a symbolic link or not a folder. Nothing is created past a refusal,
/// so a link in the tree cannot send new folders outside it.
fn prepare_parents(tree: &Path, relative: &Path) -> Result<(), String> {
    let parts: Vec<Component<'_>> = relative.components().collect();
    let Some((_, parents)) = parts.split_last() else {
        return Ok(());
    };
    let mut current = tree.to_path_buf();
    for part in parents {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("its destination passes through a symbolic link".to_owned());
            }
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err("its destination is under something that is not a folder".to_owned());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&current).map_err(|error| safe_text(&error.to_string()))?;
            }
            Err(error) => return Err(safe_text(&error.to_string())),
        }
    }
    Ok(())
}
