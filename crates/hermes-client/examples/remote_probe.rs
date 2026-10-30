//! Run the operator's native sign-in and remote listener probe, saving only sanitized evidence.
use hermes_client::remote::{RemoteConfig, RemoteName, TokenStore};
use std::path::PathBuf;

async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let name = args
        .windows(2)
        .find(|pair| pair[0] == "--remote")
        .map(|pair| pair[1].as_str())
        .ok_or("usage: remote_probe --remote <name> [--no-browser]")?;
    let name = RemoteName::parse(name).map_err(|e| e.to_string())?;
    let home = std::env::var_os("TALARIA_CONFIG_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|s| PathBuf::from(s).join(".talaria")))
        .ok_or("configuration folder unavailable")?;
    let text = std::fs::read_to_string(home.join("remotes.toml"))
        .map_err(|_| "could not read remotes.toml")?;
    let remotes: toml::Value = toml::from_str(&text).map_err(|_| "could not parse remotes.toml")?;
    let row = remotes
        .get("remotes")
        .and_then(|r| r.get(name.as_str()))
        .ok_or("remote not configured")?;
    let base_url = row
        .get("url")
        .and_then(toml::Value::as_str)
        .and_then(|s| url::Url::parse(s).ok())
        .ok_or("invalid remote URL")?;
    let cfg = RemoteConfig {
        name,
        base_url,
        provider: row
            .get("provider")
            .and_then(toml::Value::as_str)
            .map(str::to_owned),
    };
    let store = TokenStore::new(&home, cfg.name.clone());
    let report = hermes_client::remote::capture::probe(
        &cfg,
        &store,
        args.iter().any(|s| s == "--no-browser"),
    )
    .await
    .map_err(|e| e.to_string())?;
    let text =
        serde_json::to_string_pretty(&report).map_err(|_| "could not encode sanitized probe")?;
    if !hermes_client::scan_for_secrets(&text).is_empty() {
        return Err("probe sanitizer refused its output".into());
    }
    let output = PathBuf::from("fixtures/remote/probe.json");
    std::fs::create_dir_all("fixtures/remote").map_err(|_| "could not create probe folder")?;
    std::fs::write(output, text).map_err(|_| "could not save sanitized probe")?;
    if report["verified"] != true {
        return Err("probe saved; the remote deployment checks did not all pass".into());
    }
    println!(
        "Remote deployment probe passed; sanitized evidence saved to fixtures/remote/probe.json"
    );
    Ok(())
}
#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("remote probe: {error}");
            std::process::ExitCode::from(1)
        }
    }
}
