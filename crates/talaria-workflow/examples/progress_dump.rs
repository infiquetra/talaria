// Print an offline workflow journal with the same projection as the overlay.
use serde_json::json;
use std::{error::Error, fs, path::PathBuf};
use talaria_core::providers::text::sanitize;
use talaria_workflow::{
    journal::{JournalReader, NewManifest, OnUnknown, Record, RunDir, RunId},
    progress::{Tail, live::ProgressState},
};
fn main() -> Result<(), Box<dyn Error>> {
    let mut run = None;
    let mut journal = None;
    let mut out = None;
    let mut as_json = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--run-dir" => {
                run = Some(PathBuf::from(
                    args.next().ok_or("--run-dir needs a folder")?,
                ))
            }
            "--journal" => {
                journal = Some(PathBuf::from(args.next().ok_or("--journal needs a file")?))
            }
            "--out" => out = Some(PathBuf::from(args.next().ok_or("--out needs a folder")?)),
            "--json" => as_json = true,
            _ => return Err(format!("unknown option {arg}").into()),
        }
    }
    if run.is_some() == journal.is_some() {
        return Err("use exactly one of --run-dir or --journal".into());
    }
    let temporary = tempfile::tempdir()?;
    let dir = if let Some(path) = run {
        RunDir::open(&path)?
    } else {
        let path = journal.ok_or("journal missing")?;
        let (lines, _) = JournalReader::read_from(&path, 0)?;
        let id = lines
            .iter()
            .find_map(|l| {
                if let Record::RunStarted { run_id, .. } = &l.record {
                    Some(run_id.clone())
                } else {
                    None
                }
            })
            .ok_or("journal has no run_started")?;
        let name = lines
            .iter()
            .find_map(|l| {
                if let Record::Engine {
                    event: Some(talaria_workflow::engine::EngineEvent::Started { meta, .. }),
                    ..
                } = &l.record
                {
                    Some(meta.name.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "workflow".into());
        let parent = out.as_deref().unwrap_or(temporary.path());
        fs::create_dir_all(parent)?;
        let dir = RunDir::create_with_id(
            parent,
            RunId::parse(&id)?,
            NewManifest {
                workflow_name: name,
                args: None,
                started_at_ms: lines.first().map_or(0, |l| l.at_ms),
                seed: 0,
                profile: "fixture".into(),
                on_unknown: OnUnknown::Wait,
                talaria_version: env!("CARGO_PKG_VERSION").into(),
                hermes: json!({}),
                roster: json!({}),
                approval_policy: json!({"mode":"finite-only"}),
            },
            "",
        )?;
        fs::copy(&path, dir.path().join("journal.jsonl"))?;
        let source_blobs = path.parent().ok_or("journal has no parent")?.join("blobs");
        if source_blobs.is_dir() {
            for entry in fs::read_dir(source_blobs)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    fs::copy(
                        entry.path(),
                        dir.path().join("blobs").join(entry.file_name()),
                    )?;
                }
            }
        }
        dir
    };
    let (projection, _) = Tail::open(&dir)?;
    let state = ProgressState::new(projection);
    if as_json {
        let r = &state.progress.run;
        let phases:Vec<_>=state.progress.phases.iter().map(|p|json!({"title":p.title,"agents":p.agents.iter().filter_map(|k|state.progress.agent(k)).map(|a|json!({"key":a.key,"label":a.label,"state":a.state,"model":a.model,"provider":a.provider,"tokens":if a.tokens_unknown {None}else{Some(a.tokens)},"money":a.money})).collect::<Vec<_>>()})).collect();
        println!(
            "{}",
            serde_json::to_string(
                &json!({"run":{"name":r.name,"state":r.state,"counts":r.counts,"tokens":if r.tokens_unknown {None}else{Some(r.tokens)},"money":r.money},"phases":phases})
            )?
        );
    } else {
        println!(
            "{}",
            sanitize(&talaria_workflow::progress::text::run_line(&state))
        );
        for phase in &state.progress.phases {
            println!("  {}", sanitize(&phase.title));
            for a in phase.agents.iter().filter_map(|k| state.progress.agent(k)) {
                println!(
                    "    {} {} · {} · {} tokens · {}",
                    a.state.glyph(),
                    sanitize(&a.label),
                    a.state.as_str(),
                    talaria_workflow::progress::format_tokens(a.tokens),
                    talaria_workflow::progress::format_money(a.money)
                );
            }
        }
        if out.is_some() {
            println!("run folder: {}", dir.run_id().as_str());
        }
    }
    Ok(())
}
