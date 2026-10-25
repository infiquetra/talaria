// Checkpoint, diff and rewind answers for issue #233. Included by `mock_gateway.rs`.
//
// Opt-in: checkpoints are off and diffs are empty until a test sets them. Every reply follows
// the pinned contract's shapes and the gateway's behaviour as the unit specification restates
// them; the absolute folder of a restore is the recorder's `<tmp>` placeholder.

/// The rewind side of the mock.
#[derive(Default)]
struct RewindSide {
    /// `rollback.list`'s `enabled`.
    enabled: bool,
    /// `rollback.list`'s rows, newest first: `{hash, timestamp, message}`.
    checkpoints: Vec<Value>,
    /// The `rollback.diff` result. Absent: `{stat: "", diff: ""}`.
    diff: Option<Value>,
    /// The `rollback.session_diff` result. Absent: no checkpoint yet.
    session_diff: Option<Value>,
    /// Events written right after the reply of the request that produced them.
    after_reply: Vec<(String, Value)>,
}

impl MockGateway {
    /// Checkpoints on or off, and the list rows, newest first.
    pub fn set_checkpoints(&self, enabled: bool, rows: Vec<Value>) {
        let mut state = lock(&self.shared);
        state.rewind.enabled = enabled;
        state.rewind.checkpoints = rows;
    }

    /// The `rollback.diff` result, for example `{stat, diff, rendered}`.
    pub fn set_checkpoint_diff(&self, result: Value) {
        lock(&self.shared).rewind.diff = Some(result);
    }

    /// The `rollback.session_diff` result.
    pub fn set_session_diff(&self, result: Value) {
        lock(&self.shared).rewind.session_diff = Some(result);
    }

    /// The resume snapshot's `running` flag, which a rewind and a whole-folder restore check.
    pub fn set_resume_running(&self, running: bool) {
        lock(&self.shared).sess.resume_snapshot["running"] = json!(running);
    }
}

/// The snapshot a resume of `session_id` reads, so a rewind changes what the next read sees.
fn rewind_snapshot<'a>(state: &'a mut State, session_id: &str) -> Option<&'a mut Value> {
    if state.sess.sessions.contains_key(session_id) {
        return state.sess.sessions.get_mut(session_id);
    }
    (state.sess.resume_snapshot["session_id"] == session_id)
        .then_some(&mut state.sess.resume_snapshot)
}

fn active_rows(snapshot: &Value) -> Vec<Value> {
    snapshot["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row.get("archived") != Some(&Value::Bool(true)))
        .cloned()
        .collect()
}

/// A queued, started or unknown admission, or a running turn, keeps the session busy.
fn rewind_busy(snapshot: &Value) -> bool {
    snapshot["running"] == true
        || snapshot["pending"].as_array().into_iter().flatten().any(|entry| {
            matches!(
                entry["status"].as_str(),
                Some("queued" | "started" | "unknown")
            )
        })
}

/// Deactivate `target` and every active row with a larger id, raise both counters by one, and
/// return the mutation result. The `session.updated` frame is written after the reply.
fn apply_rewind(state: &mut State, session_id: &str, target: i64) -> Option<Value> {
    let snapshot = rewind_snapshot(state, session_id)?;
    let mut target_row = None;
    let mut count = 0_u64;
    for row in snapshot["messages"].as_array_mut().into_iter().flatten() {
        let Some(row_id) = row["row_id"].as_i64() else {
            continue;
        };
        if row_id < target || row.get("archived") == Some(&Value::Bool(true)) {
            continue;
        }
        if row_id == target {
            target_row = Some(row.clone());
        }
        row["archived"] = json!(true);
        count += 1;
    }
    let revision = snapshot["revision"].as_i64().unwrap_or(0) + 1;
    let generation = snapshot["execution_generation"].as_i64().unwrap_or(0) + 1;
    snapshot["revision"] = json!(revision);
    snapshot["execution_generation"] = json!(generation);
    snapshot["display_messages"] = json!(active_rows(snapshot));
    snapshot["message_count"] = json!(active_rows(snapshot).len());
    // A resume reports the generation the mock tracks per session, so raise that one too.
    state.sess.gens.insert(session_id.to_owned(), generation);
    state.sess.mutate_revision = revision;
    state.sess.mutate_generation = generation;
    let result = json!({
        "session_id": session_id,
        "operation": "rewind",
        "revision": revision,
        "execution_generation": generation,
        "rewound_count": count,
        "target_message": target_row.unwrap_or(Value::Null),
    });
    state
        .rewind
        .after_reply
        .push(("session.updated".to_owned(), result.clone()));
    Some(result)
}

/// `session.mutate` with operation `rewind`: closed payload, fences, idle check, active target.
fn handle_rewind_mutate(state: &mut State, params: &Value) -> Result<Value, String> {
    let session_id = params["session_id"].as_str().unwrap_or_default().to_owned();
    let payload = params["payload"].as_object();
    let target = payload
        .filter(|payload| payload.len() == 1)
        .and_then(|payload| payload.get("target_message_id"))
        .and_then(Value::as_i64)
        .filter(|target| *target > 0)
        .ok_or_else(|| "invalid_params".to_owned())?;
    let Some(expected_generation) = params["expected_generation"].as_i64() else {
        return Err("invalid_params".to_owned());
    };
    let snapshot = rewind_snapshot(state, &session_id)
        .ok_or_else(|| "not_found".to_owned())?
        .clone();
    state.sess.mutate_revision = snapshot["revision"].as_i64().unwrap_or(0);
    state.sess.mutate_generation = snapshot["execution_generation"].as_i64().unwrap_or(0);
    if params["expected_revision"].as_i64() != Some(state.sess.mutate_revision) {
        return Err("fence:revision_conflict".to_owned());
    }
    if expected_generation != state.sess.mutate_generation {
        return Err("fence:stale_generation".to_owned());
    }
    if rewind_busy(&snapshot) {
        return Err("session_busy".to_owned());
    }
    let active_user = active_rows(&snapshot)
        .iter()
        .any(|row| row["row_id"].as_i64() == Some(target) && row["role"] == "user");
    if !active_user {
        return Err("field:target_message_id".to_owned());
    }
    apply_rewind(state, &session_id, target).ok_or_else(|| "not_found".to_owned())
}

fn handle_rollback_list(state: &State) -> Value {
    if !state.rewind.enabled {
        return json!({"enabled": false, "checkpoints": []});
    }
    json!({"enabled": true, "checkpoints": state.rewind.checkpoints})
}

fn handle_rollback_diff(state: &State) -> Value {
    state
        .rewind
        .diff
        .clone()
        .unwrap_or_else(|| json!({"stat": "", "diff": ""}))
}

fn handle_rollback_session_diff(state: &State) -> Value {
    if !state.rewind.enabled {
        return json!({"enabled": false});
    }
    state.rewind.session_diff.clone().unwrap_or_else(|| {
        json!({
            "enabled": true,
            "success": true,
            "stat": "",
            "diff": "",
            "diff_lines": 0,
            "empty": true,
        })
    })
}

/// `rollback.restore`. A whole-folder restore needs an idle turn, then rewinds to before the
/// newest user row and reports the removed rows as `history_removed`; queued prompts make that
/// follow-up rewind fail after the files were restored.
fn handle_rollback_restore(state: &mut State, params: &Value) -> Result<Value, String> {
    let session_id = params["session_id"].as_str().unwrap_or_default().to_owned();
    let hash = params["hash"].as_str().unwrap_or_default();
    let rows = &state.rewind.checkpoints;
    let row = match hash.parse::<usize>() {
        Ok(number) if (1..=rows.len()).contains(&number) => Some(rows[number - 1].clone()),
        Ok(_) => {
            return Ok(json!({
                "success": false,
                "error": format!("Invalid checkpoint number. Use 1-{}.", rows.len()),
            }));
        }
        Err(_) => rows
            .iter()
            .find(|row| row["hash"].as_str().is_some_and(|full| full.starts_with(hash)))
            .cloned(),
    };
    let Some(row) = row.filter(|_| state.rewind.enabled) else {
        return Ok(json!({
            "success": false,
            "error": format!("Checkpoint '{hash}' not found"),
        }));
    };
    let mut result = json!({
        "success": true,
        "restored_to": row["hash"].as_str().unwrap_or_default().chars().take(8).collect::<String>(),
        "reason": row["message"],
        "directory": "<tmp>",
    });
    if let Some(file) = params["file_path"].as_str() {
        result["file"] = json!(file);
        return Ok(result);
    }
    let snapshot = rewind_snapshot(state, &session_id)
        .ok_or_else(|| "not_found".to_owned())?
        .clone();
    if snapshot["running"] == true {
        return Err("session_busy".to_owned());
    }
    result["restored_files"] = json!(["notes.txt"]);
    result["skipped_user_edits"] = json!([]);
    result["skipped_oversize"] = json!([]);
    if rewind_busy(&snapshot) {
        return Err("session_busy".to_owned());
    }
    let newest = active_rows(&snapshot)
        .iter()
        .rev()
        .find(|row| {
            row["role"] == "user"
                && row["row_id"].as_i64().is_some()
                && matches!(row["display_kind"].as_str(), None | Some("steer"))
        })
        .and_then(|row| row["row_id"].as_i64());
    let removed = match newest {
        Some(target) => apply_rewind(state, &session_id, target)
            .and_then(|rewound| rewound["rewound_count"].as_i64())
            .unwrap_or(0),
        None => 0,
    };
    result["history_removed"] = json!(removed);
    Ok(result)
}

/// Write the events a handler queued, right after its reply.
fn flush_after_reply(
    state: &mut State,
    delay: Option<Duration>,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
) {
    for (type_name, payload) in std::mem::take(&mut state.rewind.after_reply) {
        let text = event_text(state, &type_name, payload);
        let sequence = state.next_seq.saturating_sub(1);
        if let Some(snapshot) = rewind_snapshot(state, &state.session_id.clone()) {
            snapshot["last_sequence"] = json!(sequence);
        }
        outgoing.push((delay, text, event_meta(&type_name)));
    }
}
