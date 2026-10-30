//! Scriptable local stand-in for the terminal manager command.

use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(path) = std::env::var_os("TALARIA_MOCK_HERDR_LOG") {
        let mut log = OpenOptions::new().create(true).append(true).open(path)?;
        let record = format!("{}\n", json!({"argv":args}));
        log.write_all(record.as_bytes())?;
    }
    if let Ok(ms) = std::env::var("TALARIA_MOCK_HERDR_DELAY_MS") {
        std::thread::sleep(std::time::Duration::from_millis(ms.parse().unwrap_or(0)));
    }
    let path = PathBuf::from(
        std::env::var_os("TALARIA_MOCK_HERDR_STATE")
            .ok_or_else(|| io::Error::other("mock state is required"))?,
    );
    let mut state: Value =
        serde_json::from_str(&fs::read_to_string(&path)?).map_err(io::Error::other)?;
    let parts: Vec<&str> = args.iter().map(String::as_str).collect();
    match parts.as_slice() {
        ["agent", "list"] => println!(
            "{}",
            json!({"id":"cli:agent:list","result":{"type":"agent_list","agents":state["agents"]}})
        ),
        ["agent", "get", target] => {
            let agent = state["agents"]
                .as_array()
                .and_then(|a| a.iter().find(|a| a["pane_id"] == *target))
                .ok_or_else(|| io::Error::other("unknown pane"))?;
            println!(
                "{}",
                json!({"id":"cli:agent:get","result":{"type":"agent_info","agent":agent}})
            );
        }
        ["pane", "read", target, "--source", "visible"] => {
            print!("{}", state["visible"][*target].as_str().unwrap_or(""));
        }
        ["agent", "prompt", target, _text] => {
            let agent = state["agents"]
                .as_array_mut()
                .and_then(|a| a.iter_mut().find(|a| a["pane_id"] == *target))
                .ok_or_else(|| io::Error::other("unknown pane"))?;
            if agent["agent_status"] == "blocked" {
                eprintln!("agent_blocked: prompt was not submitted");
                std::process::exit(1);
            }
            agent["agent_status"] = json!("working");
            fs::write(&path, state.to_string())?;
            println!(
                "{}",
                json!({"id":"cli:agent:prompt","result":{"type":"agent_prompted"}})
            );
        }
        ["agent", "send-keys", target, keys @ ..] if !keys.is_empty() => {
            let agent = state["agents"]
                .as_array_mut()
                .and_then(|a| a.iter_mut().find(|a| a["pane_id"] == *target))
                .ok_or_else(|| io::Error::other("unknown pane"))?;
            if keys.contains(&"C-c") || keys.contains(&"esc") {
                agent["agent_status"] = json!("idle");
            } else if agent["agent_status"] == "blocked" && keys.last() == Some(&"enter") {
                agent["agent_status"] = json!("working");
            }
            fs::write(&path, state.to_string())?;
            println!(
                "{}",
                json!({"id":"cli:agent:send-keys","result":{"type":"agent_keys_sent"}})
            );
        }
        ["agent", "focus", target] => {
            if !state["agents"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["pane_id"] == *target))
            {
                return Err(io::Error::other("unknown pane"));
            }
            println!(
                "{}",
                json!({"id":"cli:agent:focus","result":{"type":"agent_focused"}})
            );
        }
        _ => return Err(io::Error::other("unsupported mock command")),
    }
    Ok(())
}
