//! Resolve an isolated call through the real Hermes port and an offline scripted wire.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{env, path::Path, path::PathBuf, process::ExitCode, sync::Arc};
use talaria_workflow::{
    engine::{AgentRequest, CallId, Isolation},
    hermes::{ExplicitLaunch, HermesPort, LaunchDefaults, PortConfig, mock_wire::ScriptedWire},
    journal::{AdmissionState, CreateSession, GatewayPort, LaunchResolver, SubmitTurn},
    worktree::GitWorktrees,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match demonstrate().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn demonstrate() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = env::current_dir()?;
    let mut run_dir = None;
    let mut profile = "fixture".to_owned();
    let mut args = env::args().skip(1);
    while let Some(flag) = args.next() {
        if matches!(flag.as_str(), "--help" | "-h") {
            println!("worktree_agent [--source <dir>] [--run-dir <dir>] [--profile <name>]");
            println!(
                "Runs an offline scripted Hermes call and prints only its session.create cwd."
            );
            println!(
                "Defaults: current source directory, a retained temporary run folder, profile fixture."
            );
            println!(
                "Worktrees retain committed HEAD only; local edits, untracked and ignored files are not copied."
            );
            println!(
                "Reuse the same source and run folder to retain the same call's edits and branch."
            );
            return Ok(());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--source" => source = value.into(),
            "--run-dir" => run_dir = Some(PathBuf::from(value)),
            "--profile" if !value.trim().is_empty() => profile = value,
            _ => return Err(format!("unknown or invalid option: {flag}").into()),
        }
    }
    let run_dir = match run_dir {
        Some(path) => path,
        None => tempfile::Builder::new()
            .prefix("talaria-worktree-agent-")
            .tempdir()?
            .keep(),
    };
    std::fs::create_dir_all(&run_dir)?;
    let run_dir = run_dir.canonicalize()?;
    let run_id = hex::encode(Sha256::digest(run_dir.as_os_str().as_encoded_bytes()));
    let request = AgentRequest {
        call_id: CallId {
            digest: "0".repeat(64),
            occurrence: 0,
        },
        label: "isolated example".into(),
        phase: None,
        prompt: "Reply with one short word.".into(),
        schema: None,
        tier: None,
        model: Some("scripted-model".into()),
        provider: Some("scripted".into()),
        effort: None,
        profile: Some(profile.clone()),
        toolsets: Some(vec!["file".into()]),
        skills: None,
        isolation: Some(Isolation::Worktree),
        max_turns: Some(1),
        workflow_path: vec![],
    };
    let defaults = LaunchDefaults {
        profile: profile.clone(),
        cwd: source.clone(),
        toolsets: Some(vec!["file".into()]),
        worktrees: Some(Arc::new(GitWorktrees::new(run_dir, source, run_id))),
    };
    let launch = ExplicitLaunch::new(defaults).resolve(&request)?;
    let mut describe: Value =
        serde_json::from_str(include_str!("../tests/data/hermes/wire/describe_ok.json"))?;
    let mut created: Value =
        serde_json::from_str(include_str!("../tests/data/hermes/wire/create_ok.json"))?;
    let mut admitted: Value =
        serde_json::from_str(include_str!("../tests/data/hermes/wire/submit_ok.json"))?;
    let mut settled: Value = serde_json::from_str(include_str!(
        "../tests/data/hermes/wire/receipt_terminal.json"
    ))?;
    describe["profile_id"] = profile.clone().into();
    created["info"]["cwd"] = json!(launch.cwd);
    admitted["ref"]["profile_id"] = profile.clone().into();
    settled["ref"]["profile_id"] = profile.clone().into();
    let steps = json!([
        {"expect":"runtime.describe","reply":describe},
        {"expect":"session.create","reply":created},
        {"expect":"prompt.submit","reply":admitted},
        {"expect":"prompt.receipt","reply":settled.clone()},
        {"expect":"prompt.receipt","match":{"include_result":true},"reply":settled}
    ]);
    let wire = Arc::new(ScriptedWire::from_value(
        Path::new("."),
        json!({"profiles":{(profile.clone()):steps}}),
    )?);
    let (port, _feed) =
        HermesPort::with_wires(PortConfig::default(), wire.clone(), &profile).await?;
    let session = port
        .create_session(CreateSession {
            request_id: "worktree-agent-example".into(),
            launch,
            label: request.label,
        })
        .await?;
    let receipt = port
        .submit(SubmitTurn {
            profile: profile.clone(),
            session_id: session.session_id.clone(),
            submission_id: "worktree-agent-example:0".into(),
            text: request.prompt,
        })
        .await?;
    let receipt = port
        .wait_settled(profile, session.session_id, receipt.admission_id)
        .await?;
    if receipt.status != AdmissionState::Terminal || receipt.outcome.as_deref() != Some("completed")
    {
        return Err("scripted call did not complete".into());
    }
    let frames = wire.outbound();
    let frame = frames
        .iter()
        .find(|f| f.method == "session.create")
        .ok_or("session.create frame missing")?;
    println!(
        "{}",
        frame.params["cwd"]
            .as_str()
            .ok_or("session.create cwd missing")?
    );
    Ok(())
}
