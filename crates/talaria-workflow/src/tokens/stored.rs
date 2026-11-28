//! Read-only usage recovery for receipts that predate a cache split.
use super::{SESSION_CACHE_READ_KEY, matching_cache_read, turn_total};
use crate::{
    journal::{Record, RecordLine},
    skill::{hermes_root_of, normalise_profile},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt::Write,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Default)]
struct Attempt<'a> {
    profile: Option<&'a str>,
    session: Option<&'a str>,
    request: Option<&'a str>,
    settlement: Option<usize>,
    ambiguous: bool,
}

struct Candidate<'a> {
    index: usize,
    session: &'a str,
    request: &'a str,
    admission: &'a str,
    outcome: &'a str,
    usage: &'a Value,
}

/// Recover a split only for the journal's sole turn on a closed local workflow session.
/// The persisted data remains untouched, and a missing store or SQLite executable leaves the
/// conservative journal total in place. Normal workflow runners use local control-socket
/// discovery. An imported run can only be matched to an actual local session and admission.
pub(crate) fn recover_stored_cache(lines: &mut [RecordLine]) {
    if !lines
        .iter()
        .any(|line| missing_first_turn_cache(&line.record))
    {
        return;
    }
    let home = hermes_client::default_home();
    let native = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".hermes");
    let recovered = candidates(lines)
        .into_iter()
        .filter_map(|(profile, candidates)| {
            let profile_home = local_profile_home(&home, &native, profile)?;
            Some(read_cache(&profile_home.join("state.db"), &candidates))
        })
        .flatten()
        .collect::<Vec<_>>();
    for (index, cache_read) in recovered {
        if let Record::TurnSettled {
            usage: Some(usage), ..
        } = &mut lines[index].record
        {
            usage[SESSION_CACHE_READ_KEY] = cache_read.into();
        }
    }
}

fn local_profile_home(home: &Path, native: &Path, profile: &str) -> Option<PathBuf> {
    if normalise_profile(profile).ok()?.as_str() != profile {
        return None;
    }
    let root = hermes_root_of(home, native).canonicalize().ok()?;
    let configured = home.canonicalize().ok()?;
    if profile == "default" {
        // A legacy alias to a named profile is still a named profile after resolving it.
        return (configured == root
            && !root
                .ancestors()
                .any(|p| p.file_name().is_some_and(|name| name == "profiles")))
        .then_some(root);
    }
    let profiles = root.join("profiles");
    let expected = profiles.join(profile);
    let actual = expected.canonicalize().ok()?;
    // Hermes evolves a named home through profiles/.gen/<same profile>/<generation>.
    // An alias into a different profile or outside the root does not prove ownership.
    if actual != expected && actual.parent() != Some(profiles.join(".gen").join(profile).as_path())
    {
        return None;
    }
    (configured == root || configured == actual).then_some(actual)
}

fn missing_first_turn_cache(record: &Record) -> bool {
    matches!(record, Record::TurnSettled { turn: 0, usage: Some(usage), .. }
        if usage[SESSION_CACHE_READ_KEY].is_null() && turn_total(usage).is_some_and(|n| n > 0))
}

fn candidates(lines: &[RecordLine]) -> BTreeMap<&str, Vec<Candidate<'_>>> {
    let mut attempts = BTreeMap::<_, Attempt<'_>>::new();
    for (index, line) in lines.iter().enumerate() {
        let Some((key, attempt)) = line.record.identity() else {
            continue;
        };
        let state = attempts.entry((key, attempt)).or_default();
        state.ambiguous |= match &line.record {
            Record::CallIntent { launch, .. } => state.profile.replace(&launch.profile).is_some(),
            Record::SessionBound {
                session_id,
                stored_session_id,
                ..
            } => session_id != stored_session_id || state.session.replace(session_id).is_some(),
            Record::TurnIntent {
                turn,
                submission_id,
                ..
            } => *turn != 0 || state.request.replace(submission_id).is_some(),
            Record::TurnSettled { .. } => {
                !missing_first_turn_cache(&line.record) || state.settlement.replace(index).is_some()
            }
            _ => false,
        };
    }
    let mut by_profile = BTreeMap::<_, Vec<_>>::new();
    for state in attempts.values().filter(|s| !s.ambiguous) {
        let (Some(profile), Some(session), Some(request), Some(index)) = (
            state.profile,
            state.session,
            state.request,
            state.settlement,
        ) else {
            continue;
        };
        if let Record::TurnSettled {
            admission_id,
            outcome,
            usage: Some(usage),
            ..
        } = &lines[index].record
            && session.starts_with("local-")
            && !request.is_empty()
            && !admission_id.is_empty()
        {
            by_profile.entry(profile).or_default().push(Candidate {
                index,
                session,
                request,
                admission: admission_id,
                outcome,
                usage,
            });
        }
    }
    by_profile
}

fn sql_text(text: &str) -> String {
    // Hex literals preserve arbitrary IDs without letting journal text change the query.
    format!("CAST(X'{}' AS TEXT)", hex::encode(text))
}

fn read_cache(db: &Path, candidates: &[Candidate<'_>]) -> Vec<(usize, u64)> {
    let Ok(resolved) = db.canonicalize() else {
        return Vec::new();
    };
    if !resolved.is_file()
        || resolved.parent()
            != db
                .parent()
                .and_then(|dir| dir.canonicalize().ok())
                .as_deref()
    {
        return Vec::new();
    }
    candidates
        .chunks(100)
        .flat_map(|batch| read_batch(&resolved, batch))
        .collect()
}

fn read_batch(db: &Path, candidates: &[Candidate<'_>]) -> Vec<(usize, u64)> {
    let mut query =
        String::from("WITH candidate(i, session, request, admission, outcome) AS (VALUES");
    for (i, candidate) in candidates.iter().enumerate() {
        let separator = if i == 0 { " " } else { ", " };
        let _ = write!(
            query,
            "{separator}({i}, {}, {}, {}, {})",
            sql_text(candidate.session),
            sql_text(candidate.request),
            sql_text(candidate.admission),
            sql_text(candidate.outcome),
        );
    }
    query.push_str(
        ") SELECT c.i, s.input_tokens, s.output_tokens, s.cache_read_tokens, s.cache_write_tokens
          FROM candidate c
          JOIN session_admissions a ON a.admission_id = c.admission AND a.target_session_id = c.session
          JOIN sessions s ON s.id = c.session
          WHERE a.request_id = c.request AND a.outcome = c.outcome AND a.status = 'terminal'
            AND s.source = 'tool' AND s.end_reason = 'workflow_agent_done' AND s.ended_at IS NOT NULL
            AND s.parent_session_id IS NULL AND s.api_call_count > 0
            AND NOT EXISTS (SELECT 1 FROM sessions child WHERE child.parent_session_id = s.id)
            AND NOT EXISTS (SELECT 1 FROM session_admissions other
                            WHERE other.target_session_id = s.id AND other.admission_id <> a.admission_id)",
    );
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let output = Command::new("sqlite3")
        .args([
            "-batch",
            "-readonly",
            "-bail",
            "-init",
            null,
            "-tabs",
            "-noheader",
            "-cmd",
            ".timeout 50",
        ])
        .arg(db)
        .arg(query)
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let [index, input, output, read, write]: [u64; 5] = line
                .split('\t')
                .map(str::parse)
                .collect::<Result<Vec<_>, _>>()
                .ok()?
                .try_into()
                .ok()?;
            let candidate = candidates.get(usize::try_from(index).ok()?)?;
            let prompt = input.checked_add(read)?.checked_add(write)?;
            let read = matching_cache_read(candidate.usage, prompt, output, read)?;
            Some((candidate.index, read))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(unix)]
    #[test]
    fn local_profile_aliases_must_resolve_to_the_same_owner() {
        use std::{fs, os::unix::fs::symlink};

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("hermes");
        let profiles = root.join("profiles");
        let regular = profiles.join("regular");
        let generation = profiles.join(".gen/evolved/rev");
        let alias = profiles.join("evolved");
        let wrong_owner = profiles.join("wrong");
        let legacy = temp.path().join("legacy");
        fs::create_dir_all(&regular).unwrap();
        fs::create_dir_all(&generation).unwrap();
        symlink(&generation, &alias).unwrap();
        symlink(&regular, &wrong_owner).unwrap();
        symlink(&regular, &legacy).unwrap();

        assert_eq!(
            local_profile_home(&root, &root, "default"),
            root.canonicalize().ok()
        );
        assert_eq!(
            local_profile_home(&root, &root, "evolved"),
            generation.canonicalize().ok()
        );
        assert_eq!(
            local_profile_home(&alias, &root, "evolved"),
            generation.canonicalize().ok()
        );
        assert_eq!(local_profile_home(&root, &root, "wrong"), None);
        assert_eq!(local_profile_home(&alias, &root, "default"), None);
        assert_eq!(local_profile_home(&legacy, &root, "default"), None);
        assert_eq!(local_profile_home(&root, &root, "../regular"), None);
    }

    #[test]
    fn stored_cache_requires_an_exact_receipt_and_a_single_admission() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("state.db");
        let write = |sql: &str| Command::new("sqlite3").arg(&db).arg(sql).output();
        match write(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, source TEXT, end_reason TEXT, ended_at REAL,
              parent_session_id TEXT, api_call_count INTEGER, input_tokens INTEGER, output_tokens INTEGER,
              cache_read_tokens INTEGER, cache_write_tokens INTEGER);
             CREATE TABLE session_admissions (admission_id TEXT PRIMARY KEY, target_session_id TEXT,
              request_id TEXT, outcome TEXT, status TEXT);
             INSERT INTO sessions VALUES ('local-session', 'tool', 'workflow_agent_done', 1, NULL, 4, 3, 2, 4, 1);
             INSERT INTO session_admissions VALUES ('adm', 'local-session', 'wf:req', 'failed', 'terminal');",
        ) {
            Ok(result) => assert!(result.status.success(), "{result:?}"),
            // The report can also run on a machine that lacks SQLite.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("{error}"),
        }
        let matched = json!({"input_tokens": 8, "output_tokens": 2, "total_tokens": 10});
        let mismatched = json!({"input_tokens": 7, "output_tokens": 3, "total_tokens": 10});
        let mut candidate = Candidate {
            index: 7,
            session: "local-session",
            request: "wf:req",
            admission: "adm",
            outcome: "failed",
            usage: &mismatched,
        };
        assert!(read_cache(&db, std::slice::from_ref(&candidate)).is_empty());
        candidate.usage = &matched;
        assert_eq!(read_cache(&db, std::slice::from_ref(&candidate)), [(7, 4)]);
        let result = write(
            "INSERT INTO session_admissions VALUES ('other', 'local-session', 'other', NULL, 'started');",
        )
        .unwrap();
        assert!(result.status.success(), "{result:?}");
        assert!(read_cache(&db, &[candidate]).is_empty());
    }
}
