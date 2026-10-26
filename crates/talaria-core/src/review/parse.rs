//! Pure parsers for git's machine output. Issue #245.

use std::collections::BTreeMap;

use crate::review::model::{BranchHead, ChangedFile, FileStatus};

/// A parse failure. The message is for the notice line, after sanitising.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError(pub String);

/// Numstat counts by path: added and removed lines, `None` for binary files.
pub type Numstat = BTreeMap<String, (Option<u32>, Option<u32>)>;

/// What a `git push --porcelain` report said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushReport {
    /// One `(flag, ref)` pair per pushed ref.
    pub refs: Vec<(char, String)>,
    /// Any ref was rejected.
    pub rejected: bool,
}

/// Parse `git status --porcelain=v2 -z --branch` output.
///
/// Paths are decoded lossily: a name that is not UTF-8 shows the replacement character.
pub fn parse_status_v2(raw: &[u8]) -> Result<(BranchHead, Vec<ChangedFile>), ParseError> {
    let mut head = BranchHead::default();
    let mut files = Vec::new();
    let mut records = raw
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        let text = String::from_utf8_lossy(record);
        if let Some(header) = text.strip_prefix("# ") {
            parse_header(header, &mut head)?;
            continue;
        }
        let Some((kind, rest)) = text.split_once(' ') else {
            return Err(malformed(&text));
        };
        match kind {
            "1" => {
                let fields: Vec<&str> = rest.splitn(8, ' ').collect();
                let [xy, _, _, _, _, _, _, path] = fields[..] else {
                    return Err(malformed(&text));
                };
                files.push(tracked(xy, path, None)?);
            }
            "2" => {
                let fields: Vec<&str> = rest.splitn(9, ' ').collect();
                let [xy, _, _, _, _, _, _, _, path] = fields[..] else {
                    return Err(malformed(&text));
                };
                let Some(orig) = records.next() else {
                    return Err(malformed(&text));
                };
                let orig = String::from_utf8_lossy(orig).into_owned();
                files.push(tracked(xy, path, Some(orig))?);
            }
            "u" => {
                let fields: Vec<&str> = rest.splitn(10, ' ').collect();
                let [_, _, _, _, _, _, _, _, _, path] = fields[..] else {
                    return Err(malformed(&text));
                };
                files.push(ChangedFile {
                    path: path.to_owned(),
                    orig_path: None,
                    status: FileStatus::Conflicted,
                    staged: false,
                    unstaged: true,
                    added: None,
                    removed: None,
                    is_dir: false,
                });
            }
            "?" => files.push(ChangedFile {
                path: rest.to_owned(),
                orig_path: None,
                status: FileStatus::Untracked,
                staged: false,
                unstaged: true,
                added: None,
                removed: None,
                is_dir: rest.ends_with('/'),
            }),
            "!" => {}
            _ => return Err(malformed(&text)),
        }
    }
    Ok((head, files))
}

fn malformed(record: &str) -> ParseError {
    let shown: String = record.chars().take(40).collect();
    ParseError(format!("git status gave an unexpected line: {shown}"))
}

fn parse_header(header: &str, head: &mut BranchHead) -> Result<(), ParseError> {
    let Some((key, value)) = header.split_once(' ') else {
        return Ok(());
    };
    match key {
        "branch.oid" => head.oid = (value != "(initial)").then(|| value.to_owned()),
        "branch.head" => head.head = (value != "(detached)").then(|| value.to_owned()),
        "branch.upstream" => head.upstream = Some(value.to_owned()),
        "branch.ab" => {
            let mut counts = value.split(' ');
            let ahead = counts.next().and_then(|n| n.strip_prefix('+'));
            let behind = counts.next().and_then(|n| n.strip_prefix('-'));
            let (Some(ahead), Some(behind)) = (ahead, behind) else {
                return Err(malformed(header));
            };
            head.ahead = ahead.parse().map_err(|_| malformed(header))?;
            head.behind = behind.parse().map_err(|_| malformed(header))?;
        }
        _ => {}
    }
    Ok(())
}

/// One ordinary or renamed row: the index code wins over the work tree code.
fn tracked(xy: &str, path: &str, orig_path: Option<String>) -> Result<ChangedFile, ParseError> {
    let mut codes = xy.chars();
    let (Some(index), Some(tree), None) = (codes.next(), codes.next(), codes.next()) else {
        return Err(malformed(xy));
    };
    let code = if index == '.' { tree } else { index };
    let status = match code {
        'M' => FileStatus::Modified,
        'A' => FileStatus::Added,
        'D' => FileStatus::Deleted,
        'R' => FileStatus::Renamed,
        'C' => FileStatus::Copied,
        'T' => FileStatus::TypeChanged,
        _ => return Err(malformed(xy)),
    };
    Ok(ChangedFile {
        path: path.to_owned(),
        orig_path,
        status,
        staged: index != '.',
        unstaged: tree != '.',
        added: None,
        removed: None,
        is_dir: false,
    })
}

/// Parse `git diff --numstat -z` output. Binary files give `None` counts.
///
/// A renamed or copied file is keyed by its new path.
pub fn parse_numstat_z(raw: &[u8]) -> Result<Numstat, ParseError> {
    let mut counts = Numstat::new();
    let mut records = raw
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        let text = String::from_utf8_lossy(record);
        let mut fields = text.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Err(ParseError(
                "git diff gave an unexpected numstat line".to_owned(),
            ));
        };
        let path = if path.is_empty() {
            // A rename: the old and new paths follow as their own records.
            let (Some(_old), Some(new)) = (records.next(), records.next()) else {
                return Err(ParseError("git diff cut a rename short".to_owned()));
            };
            String::from_utf8_lossy(new).into_owned()
        } else {
            path.to_owned()
        };
        let count = |text: &str| text.parse::<u32>().ok();
        let entry = match (count(added), count(removed)) {
            (Some(added), Some(removed)) => (Some(added), Some(removed)),
            _ if added == "-" && removed == "-" => (None, None),
            _ => return Err(ParseError("git diff gave an unexpected count".to_owned())),
        };
        counts.insert(path, entry);
    }
    Ok(counts)
}

/// Parse `git diff --name-status -z` output: status, new path, old path.
pub fn parse_name_status_z(
    raw: &[u8],
) -> Result<Vec<(FileStatus, String, Option<String>)>, ParseError> {
    let mut rows = Vec::new();
    let mut records = raw
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        let letter = String::from_utf8_lossy(record);
        let status = match letter.chars().next() {
            Some('M') => FileStatus::Modified,
            Some('A') => FileStatus::Added,
            Some('D') => FileStatus::Deleted,
            Some('T') => FileStatus::TypeChanged,
            Some('U') => FileStatus::Conflicted,
            Some('R') => FileStatus::Renamed,
            Some('C') => FileStatus::Copied,
            _ => return Err(ParseError("git diff gave an unexpected status".to_owned())),
        };
        let cut = || ParseError("git diff cut a name-status line short".to_owned());
        if matches!(status, FileStatus::Renamed | FileStatus::Copied) {
            let old = records.next().ok_or_else(cut)?;
            let new = records.next().ok_or_else(cut)?;
            rows.push((
                status,
                String::from_utf8_lossy(new).into_owned(),
                Some(String::from_utf8_lossy(old).into_owned()),
            ));
        } else {
            let path = records.next().ok_or_else(cut)?;
            rows.push((status, String::from_utf8_lossy(path).into_owned(), None));
        }
    }
    Ok(rows)
}

/// Merge staged and unstaged numstat counts into the rows, keyed by path.
///
/// A row in both maps gets the sum. A binary file in either map has no counts. A row in
/// neither keeps the counts it has.
pub fn merge_counts(files: &mut [ChangedFile], staged: &Numstat, unstaged: &Numstat) {
    for file in files {
        let parts: Vec<(Option<u32>, Option<u32>)> = [staged, unstaged]
            .iter()
            .filter_map(|counts| counts.get(&file.path).copied())
            .collect();
        if parts.is_empty() {
            continue;
        }
        let sum = |counts: Vec<Option<u32>>| {
            counts
                .into_iter()
                .try_fold(0_u32, |sum, n| n.map(|n| sum.saturating_add(n)))
        };
        file.added = sum(parts.iter().map(|entry| entry.0).collect());
        file.removed = sum(parts.iter().map(|entry| entry.1).collect());
    }
}

/// Count the lines of an untracked file. `None` when a NUL byte is present.
pub fn count_lines(bytes: &[u8]) -> Option<u32> {
    if bytes.contains(&0) {
        return None;
    }
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count();
    let unterminated = bytes.last().is_some_and(|byte| *byte != b'\n');
    Some(u32::try_from(newlines + usize::from(unterminated)).unwrap_or(u32::MAX))
}

/// Parse `git push --porcelain` standard output.
pub fn parse_push_porcelain(stdout: &str) -> PushReport {
    let mut report = PushReport {
        refs: Vec::new(),
        rejected: false,
    };
    for line in stdout.lines() {
        let mut fields = line.splitn(3, '\t');
        let (Some(flag), Some(reference)) = (fields.next(), fields.next()) else {
            continue;
        };
        let mut chars = flag.chars();
        let (Some(flag), None) = (chars.next(), chars.next()) else {
            continue;
        };
        if !matches!(flag, ' ' | '+' | '-' | '*' | '=' | '!') {
            continue;
        }
        report.rejected |= flag == '!';
        report.refs.push((flag, reference.to_owned()));
    }
    report
}
