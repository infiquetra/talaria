use super::{
    capture,
    plan::{self, CallLine},
    *,
};
use crate::{
    engine,
    staffing::{Availability, RouteSource},
};
use hermes_client::kanban::{KanbanClient, KanbanTask};
use std::{collections::BTreeSet, path::Path, process::Command};

pub const WARNINGS: [&str; 4] = [
    "static check: two captures agreed and the subset guard passed; a script that hides result-dependent control flow from both can still differ at run time - those scripts belong to talaria workflow run.",
    "kanban does not check card results against your schema; the schema travels in each card body and the next card is the checker.",
    "workspaces are temporary: results must travel as summaries, metadata or declared completion artifacts, not as files left in the workspace.",
    "this board is shared by every profile on this machine; card bodies must contain nothing secret.",
];
struct Draft {
    call: CapturedCall,
    payload: CreateCard,
    parents: Vec<usize>,
    limited: Option<String>,
}
fn optional(v: Option<&str>) -> String {
    v.unwrap_or("-").into()
}
fn anchor(repo: Option<&Path>) -> Option<String> {
    let path = repo?.canonicalize().ok()?;
    let git = |arg| {
        Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(["rev-parse", arg])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
    };
    if git("--is-bare-repository")?.trim() != "false" {
        return None;
    }
    let top = PathBuf::from(git("--show-toplevel")?.trim())
        .canonicalize()
        .ok()?;
    path.starts_with(top)
        .then(|| path.to_string_lossy().into_owned())
}
fn resolved(
    call: &CapturedCall,
    req: &CompileRequest,
    previous: &[CapturedCall],
) -> Result<(CreateCard, Option<String>), CompileFailure> {
    let label = &call.label;
    for (present, option) in [
        (call.toolsets.is_some(), "toolsets"),
        (call.max_turns.is_some(), "maxTurns"),
    ] {
        if present {
            return Err(CompileFailure::new(
                2,
                format!(
                    "not compilable: call {label} sets {option}, which kanban cards cannot enforce; remove it or use talaria workflow run"
                ),
            ));
        }
    }
    let assignee = call
        .profile
        .as_ref()
        .or(req.profile.as_ref())
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| {
            CompileFailure::new(
                2,
                format!(
                    "no assignee for call {label}: set a profile on the call or pass --profile"
                ),
            )
        })?
        .trim()
        .to_lowercase();
    if assignee != req.resolver.roster().profile().trim().to_lowercase() {
        return Err(CompileFailure::new(
            2,
            format!(
                "no usable route for call {label}: profile '{assignee}' was not read at compile start"
            ),
        ));
    }
    let mut q = call.request().map_err(|e| CompileFailure::new(2, e))?;
    q.profile = Some(req.resolver.roster().profile().into());
    let inherited = call.tier.is_none() && call.model.is_none() && call.provider.is_none();
    let offline_inherited =
        inherited && req.dry_run && req.resolver.roster().providers().is_empty();
    if inherited && !offline_inherited {
        let (model, provider) = req.resolver.roster().current();
        if model.is_empty() || provider.is_empty() {
            return Err(CompileFailure::new(
                2,
                format!("no usable route for call {label}: the profile has no default model"),
            ));
        }
        q.model = Some(model.into());
        q.provider = Some(provider.into());
    }
    let route = if offline_inherited {
        Ok(crate::staffing::Route {
            model: String::new(),
            provider: String::new(),
            effort: q.effort,
            source: RouteSource::ProfileDefault,
            ignored_tier: None,
        })
    } else {
        req.resolver.route(&q)
    }
    .map_err(|e| CompileFailure::new(2, format!("no usable route for call {label}: {e}")))?;
    let limited = match req.resolver.roster().availability(
        &route.provider,
        &route.model,
        req.resolver.roster().taken_at(),
    ) {
        Availability::Limited { .. } => Some("limited".into()),
        Availability::NearLimit { .. } => Some("near_limit".into()),
        _ => None,
    };
    let overrides = call.tier.is_some() || call.model.is_some();
    let (model, provider) = if overrides && !matches!(route.source, RouteSource::ProfileDefault) {
        (Some(route.model), Some(route.provider))
    } else {
        (None, None)
    };
    let effort = route
        .effort
        .and_then(|e| serde_json::to_value(e).ok())
        .and_then(|v| v.as_str().map(str::to_owned));
    let workspace_kind = if call.isolation.as_deref() == Some("worktree") {
        "worktree"
    } else {
        "scratch"
    };
    let workspace_path = if workspace_kind == "worktree" {
        Some(anchor(req.repo.as_deref()).ok_or_else(|| CompileFailure::new(2,format!("call {label} uses a worktree workspace but --repo is missing or not a git repository")))?)
    } else {
        None
    };
    Ok((
        CreateCard {
            title: plan::title(call, previous),
            body: String::new(),
            assignee: Some(assignee),
            parents: vec![],
            idempotency_key: String::new(),
            skills: call.skills.clone().unwrap_or_default(),
            workspace_kind: workspace_kind.into(),
            workspace_path,
            tenant: req.tenant.clone(),
            model_override: model,
            provider_override: provider,
            reasoning_effort: effort,
        },
        limited,
    ))
}
fn body(
    draft: &Draft,
    calls: &[CapturedCall],
    ids: &[String],
    script: &str,
    identity: &str,
) -> String {
    let mut prompt = draft.call.prompt.clone();
    // Replace in a single pass over original text so the inserted instructions cannot recurse.
    let mut replacements = Vec::new();
    for &j in &draft.parents {
        for marker in plan::markers(&calls[j]) {
            replacements.push((marker.clone(),format!("Use the result of card `{}` (in the parent handoffs above) where this text says `{marker}`.",ids[j])));
        }
    }
    for (n, (marker, _)) in replacements.iter().enumerate() {
        prompt = plan::replace_marker(&prompt, marker, &format!("\u{e000}{n}\u{e001}"));
    }
    for (n, (_, value)) in replacements.iter().enumerate() {
        prompt = prompt.replace(&format!("\u{e000}{n}\u{e001}"), value);
    }
    let result = plan::card_body(
        script,
        identity,
        &draft.call.label,
        draft.call.phase.as_deref().unwrap_or("-"),
        &prompt,
        draft.call.schema.as_ref(),
    );
    if draft.call.workflow_path.is_empty() {
        result
    } else {
        result.replacen(
            "\n\n## Prompt",
            &format!(
                "\nPath: {}\n\n## Prompt",
                draft.call.workflow_path.join("/")
            ),
            1,
        )
    }
}
fn same(task: &KanbanTask, wanted: &CreateCard) -> bool {
    task.title == wanted.title
        && task.body == wanted.body
        && task.assignee.as_ref().map(|s| s.trim().to_lowercase()) == wanted.assignee
        && task.tenant == wanted.tenant
        && task.model_override == wanted.model_override
        && task.provider_override == wanted.provider_override
        && task.reasoning_effort == wanted.reasoning_effort
        && task.skills == wanted.skills
        && task.workspace_kind.as_deref().unwrap_or("scratch") == wanted.workspace_kind
        && task.workspace_path == wanted.workspace_path
}
fn conflict(key: &str) -> CompileFailure {
    CompileFailure::new(
        2,
        format!(
            "board already has a different card for {key}; the compile inputs changed - refusing to overwrite"
        ),
    )
}
fn checked_body(label: &str, body: &str) -> Result<(), CompileFailure> {
    if body.len() > 8192 {
        return Err(CompileFailure::new(
            2,
            format!(
                "card body for call {label} is {} bytes (limit 8192); shorten the prompt or use talaria workflow run",
                body.len()
            ),
        ));
    }
    Ok(())
}
fn view(
    d: &Draft,
    id: &str,
    status: &str,
    ids: &[String],
    root: &str,
    calls: &[CapturedCall],
) -> PlannedCard {
    PlannedCard {
        label: d.call.label.clone(),
        task_id: id.into(),
        status: status.into(),
        parent_ids: if d.parents.is_empty() {
            vec![root.into()]
        } else {
            d.parents.iter().map(|&j| ids[j].clone()).collect()
        },
        parent_labels: if d.parents.is_empty() {
            vec!["root".into()]
        } else {
            d.parents.iter().map(|&j| calls[j].label.clone()).collect()
        },
        title: d.payload.title.clone(),
        body: d.payload.body.clone(),
        assignee: d.payload.assignee.clone(),
        idempotency_key: d.payload.idempotency_key.clone(),
        model_override: d.payload.model_override.clone(),
        provider_override: d.payload.provider_override.clone(),
        reasoning_effort: d.payload.reasoning_effort.clone(),
        workspace_kind: d.payload.workspace_kind.clone(),
        workspace_path: d.payload.workspace_path.clone(),
    }
}
pub(super) async fn compile(
    req: CompileRequest,
    sink: &dyn CardSink,
) -> Result<CompileReport, CompileFailure> {
    if !req.args.is_object() {
        return Err(CompileFailure::new(2, "--args must be a JSON object"));
    }
    let parsed =
        engine::parse_script(&req.source).map_err(|e| CompileFailure::new(2, e.to_string()))?;
    let base = Path::new(&req.script_name)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let captured = capture::capture(&req.source, &req.args, base.into())
        .await
        .map_err(|e| CompileFailure::new(2, e))?;
    let calls = captured.calls;
    if calls.is_empty() {
        return Ok(CompileReport::nothing_to_compile());
    }
    if !req.dry_run {
        let default = sink.orchestration().await.map_err(|e| {
            CompileFailure::new(
                1,
                format!("cannot read orchestration settings: {e}; nothing was created"),
            )
        })?;
        if !default.trim().is_empty() {
            return Err(CompileFailure::new(
                1,
                format!(
                    "kanban.default_assignee is set to {default}; unset it for the compile, then re-run"
                ),
            ));
        }
    }
    let edges = plan::parents(&calls);
    let mut drafts = Vec::new();
    let mut lines = Vec::new();
    for (i, call) in calls.iter().enumerate() {
        let (payload, limited) = resolved(call, &req, &calls[..i])?;
        lines.push(CallLine {
            digest: call.call_id.digest.clone(),
            occurrence: call.call_id.occurrence,
            assignee: payload.assignee.clone().unwrap_or_default(),
            tenant: optional(payload.tenant.as_deref()),
            model: optional(payload.model_override.as_deref()),
            provider: optional(payload.provider_override.as_deref()),
            effort: optional(payload.reasoning_effort.as_deref()),
            workspace_kind: payload.workspace_kind.clone(),
            workspace_path: optional(payload.workspace_path.as_deref()),
            workflow_path: call.workflow_path.join("/"),
        });
        drafts.push(Draft {
            call: call.clone(),
            payload,
            parents: edges[i].clone(),
            limited,
        });
    }
    let children: Vec<_> = captured.children.iter().map(String::as_str).collect();
    let identity = plan::compile_id(&req.source, &req.args, &lines, &children);
    let script = Path::new(&req.script_name)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    for draft in &mut drafts {
        draft.payload.idempotency_key = format!(
            "wf:{identity}:{}:{}",
            draft.call.call_id.digest, draft.call.call_id.occurrence
        );
    }
    let mut stdout = String::new();
    let mut root = CreateCard {
        title: format!("Workflow: {}", parsed.meta.name),
        body: format!(
            "Compiled by Talaria from {script} (compile {identity}).\nArguments: {}\nCards:\n{}",
            plan::canonical(&req.args),
            drafts
                .iter()
                .map(|d| format!(
                    "  {} ({}:{})",
                    d.call.label, d.call.call_id.digest, d.call.call_id.occurrence
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        assignee: None,
        parents: vec![],
        idempotency_key: format!("wf:{identity}:root"),
        skills: vec![],
        workspace_kind: "scratch".into(),
        workspace_path: None,
        tenant: req.tenant.clone(),
        model_override: None,
        provider_override: None,
        reasoning_effort: None,
    };
    checked_body("root", &root.body)?;
    // Pinned kanban_db.py::_new_task_id returns t_ plus eight hexadecimal characters.
    let placeholders: Vec<_> = calls.iter().map(|_| "t_00000000".into()).collect();
    for d in &drafts {
        checked_body(
            &d.call.label,
            &body(d, &calls, &placeholders, &script, &identity),
        )?;
    }
    if req.dry_run {
        let mut cards = Vec::new();
        for d in &mut drafts {
            d.payload.body = body(d, &calls, &placeholders, &script, &identity);
            let parents = if d.parents.is_empty() {
                "root".into()
            } else {
                d.parents
                    .iter()
                    .map(|&j| calls[j].label.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            stdout.push_str(&format!(
                "{} parents=[{parents}] model={} assignee={}\n",
                d.call.label,
                d.payload.model_override.as_deref().unwrap_or("-"),
                d.payload.assignee.as_deref().unwrap_or("-")
            ));
            cards.push(view(d, "", "planned", &placeholders, "root", &calls));
        }
        stdout.push_str(&format!(
            "dry run: {} calls, nothing written\n",
            drafts.len()
        ));
        warnings(&mut stdout);
        return Ok(CompileReport {
            stdout,
            stderr: String::new(),
            exit_code: 0,
            cards,
        });
    }
    let existing = sink.existing().await.map_err(|e| {
        CompileFailure::new(
            1,
            format!("cannot read board {}: {e}; nothing was created", req.board),
        )
    })?;
    let find = |key: &str| {
        existing
            .iter()
            .find(|e| e.idempotency_key == key && !e.archived)
    };
    let old_root = find(&root.idempotency_key);
    let mut ids: Vec<_> = drafts
        .iter()
        .map(|d| {
            find(&d.payload.idempotency_key)
                .map(|e| e.id.clone())
                .unwrap_or_else(|| "t_00000000".into())
        })
        .collect();
    for d in &drafts {
        checked_body(&d.call.label, &body(d, &calls, &ids, &script, &identity))?;
    }
    let root_id = old_root
        .map(|e| e.id.clone())
        .unwrap_or_else(|| "pending-root".into());
    let reader = KanbanClient::new(&req.origin, &req.board, req.minter.clone())
        .map_err(|e| CompileFailure::new(1, e.to_string()))?;
    // The legacy sink metadata seam omits task payloads; read them through the same native client.
    for (payload, parent_ids) in std::iter::once((&root, vec![])).chain(drafts.iter().map(|d| {
        (
            &d.payload,
            if d.parents.is_empty() {
                vec![root_id.clone()]
            } else {
                d.parents.iter().map(|&j| ids[j].clone()).collect()
            },
        )
    })) {
        if let Some(found) = find(&payload.idempotency_key) {
            let detail = sink.read_task(&found.id, &reader).await.map_err(|e| {
                CompileFailure::new(1, format!("cannot read board {}: {e}", req.board))
            })?;
            let mut wanted = payload.clone();
            if let Some(d) = drafts
                .iter()
                .find(|d| d.payload.idempotency_key == payload.idempotency_key)
            {
                wanted.body = body(d, &calls, &ids, &script, &identity);
            }
            let actual: BTreeSet<_> = detail.parent_ids.iter().collect();
            let planned: BTreeSet<_> = parent_ids.iter().collect();
            if !same(&detail.task, &wanted) || actual != planned {
                return Err(conflict(&payload.idempotency_key));
            }
        }
    }
    let keys: BTreeSet<_> = std::iter::once(root.idempotency_key.as_str())
        .chain(drafts.iter().map(|d| d.payload.idempotency_key.as_str()))
        .collect();
    let archived = existing
        .iter()
        .filter(|e| e.archived && keys.contains(e.idempotency_key.as_str()))
        .count();
    if archived > 0 {
        stdout.push_str(&format!("recreating {archived} archived card(s)\n"));
    }
    let mut changed = false;
    let mut root_status = old_root.map(|e| e.status.clone()).unwrap_or_default();
    let root_id = if let Some(found) = old_root {
        found.id.clone()
    } else {
        root.parents.clear();
        let made = sink.create(&root).await.map_err(|e| {
            CompileFailure::new(
                1,
                format!("root card creation failed: {e}; re-run the same command to finish"),
            )
        })?;
        changed = true;
        root_status = made.status.clone();
        stdout.push_str(&format!("root -> {} ({})\n", made.id, made.status));
        made.id
    };
    let mut cards = Vec::new();
    let mut created = 0;
    for (i, d) in drafts.iter_mut().enumerate() {
        d.payload.parents = if d.parents.is_empty() {
            vec![root_id.clone()]
        } else {
            d.parents.iter().map(|&j| ids[j].clone()).collect()
        };
        d.payload.body = body(d, &calls, &ids, &script, &identity);
        let status;
        if let Some(found) = find(&d.payload.idempotency_key) {
            status = found.status.clone();
            for parent in &d.payload.parents {
                if !found.parents.contains(parent) {
                    sink.link(parent, &found.id).await.map_err(|e| {
                        CompileFailure::new(
                            1,
                            format!(
                                "cannot finish card {}: {e}; re-run the same command to finish",
                                d.call.label
                            ),
                        )
                    })?;
                    changed = true;
                }
            }
        } else {
            let made = sink.create(&d.payload).await.map_err(|e| CompileFailure::new(1,format!("created {created} of {} cards; card {} ({}) failed: {e}; re-run the same command to finish",calls.len(),i+1,d.call.label)))?;
            ids[i] = made.id;
            status = made.status;
            changed = true;
            created += 1;
        }
        stdout.push_str(&format!(
            "{} -> {} ({status}){}\n",
            d.call.label,
            ids[i],
            d.limited
                .as_ref()
                .map(|s| format!(" (roster: {s})"))
                .unwrap_or_default()
        ));
        cards.push(view(d, &ids[i], &status, &ids, &root_id, &calls));
    }
    if old_root.is_none_or(|e| e.status != "done") {
        sink.complete_root(&root_id, &format!("compiled {} cards", calls.len()))
            .await
            .map_err(|e| {
                CompileFailure::new(
                    1,
                    format!("cards created; root left in `{root_status}`: {e}; re-run the same command to finish"),
                )
            })?;
        changed = true;
    }
    if !changed {
        stdout = "already compiled\n".into();
    } else {
        if !req.no_dispatch {
            sink.dispatch().await.map_err(|e| {
                CompileFailure::new(
                    1,
                    format!(
                        "compiled {} cards; dispatcher nudge failed: {e}",
                        calls.len()
                    ),
                )
            })?;
        }
        stdout.push_str(&format!(
            "compiled {} cards to board {}; dispatcher {}nudged\n",
            calls.len(),
            req.board,
            if req.no_dispatch { "not " } else { "" }
        ));
        warnings(&mut stdout);
        for warning in sink.warnings() {
            stdout.push_str(&warning);
            stdout.push('\n');
        }
    }
    Ok(CompileReport {
        stdout,
        stderr: String::new(),
        exit_code: 0,
        cards,
    })
}
fn warnings(out: &mut String) {
    for warning in WARNINGS {
        out.push_str(warning);
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Never;
    impl hermes_client::TicketMinter for Never {
        fn mint(&self) -> BoxFuture<'_, Result<hermes_client::Ticket, hermes_client::TicketError>> {
            Box::pin(async { hermes_client::Ticket::parse(String::new()) })
        }
    }
    #[test]
    fn inherited_routes_are_checked_without_becoming_overrides() {
        let call = CapturedCall {
            call_id: engine::CallId {
                digest: "a".repeat(64),
                occurrence: 0,
            },
            label: "inherit".into(),
            phase: None,
            prompt: "Work".into(),
            schema: None,
            tier: None,
            model: None,
            provider: None,
            effort: None,
            profile: Some("fixture".into()),
            toolsets: None,
            skills: None,
            isolation: None,
            max_turns: None,
            workflow_path: vec![],
            outstanding: false,
        };
        for authenticated in [false, true] {
            let roster=crate::staffing::Roster::from_model_options(&serde_json::json!({"model":"m-small","provider":"alpha-cloud","providers":[{"slug":"alpha-cloud","name":"Alpha","models":["m-small"],"authenticated":authenticated}]}),"fixture",chrono::DateTime::parse_from_rfc3339("2026-10-22T16:00:00Z").unwrap().with_timezone(&chrono::Utc)).unwrap();
            let request = CompileRequest {
                source: String::new(),
                script_name: "inherit".into(),
                board: "mock".into(),
                tenant: None,
                args: serde_json::json!({}),
                profile: Some("fixture".into()),
                repo: None,
                dry_run: false,
                no_dispatch: false,
                resolver: crate::staffing::StaffingResolver::new(
                    Arc::new(crate::staffing::StaffingTable::parse("").unwrap()),
                    Arc::new(roster),
                ),
                minter: Arc::new(Never),
                origin: "http://127.0.0.1:1".into(),
            };
            let result = resolved(&call, &request, &[]);
            if authenticated {
                let (card, _) = result.unwrap();
                assert!(card.model_override.is_none());
                assert!(card.provider_override.is_none());
            } else {
                assert_eq!(result.err().unwrap().exit_code, 2);
            }
        }
    }
}
