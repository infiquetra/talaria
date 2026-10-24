//! Attach to the local Hermes gateway and print one line per step.
//!
//! Agents and scripts pass `--no-ensure`. The default policy can start a
//! gateway, which is what a person at a terminal wants.

use std::path::PathBuf;
use std::time::Instant;

use hermes_client::{
    DiscoverOptions, DiscoveryError, EnsurePolicy, HandshakeError, Purpose, Scope, ServedProfile,
    connect, default_home, discover_with, format_failure, handshake, mint_ticket,
};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let mut home = None;
    let mut profile = "default".to_string();
    let mut no_ensure = false;
    let mut program = PathBuf::from("hermes");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => home = Some(PathBuf::from(require(&mut args, "--home"))),
            "--profile" => profile = require(&mut args, "--profile"),
            "--no-ensure" => no_ensure = true,
            "--hermes" => program = PathBuf::from(require(&mut args, "--hermes")),
            "--help" => {
                println!(
                    "usage: attach [--home DIR] [--profile NAME] [--no-ensure] [--hermes PROGRAM]"
                );
                return;
            }
            _ => {
                println!("unknown argument");
                std::process::exit(2);
            }
        }
    }

    let home = home.unwrap_or_else(default_home);
    let options = DiscoverOptions {
        control_timeout: std::time::Duration::from_secs(5),
        ensure: if no_ensure {
            EnsurePolicy::Never
        } else {
            EnsurePolicy::Run {
                program,
                timeout: std::time::Duration::from_secs(90),
            }
        },
    };

    let started = Instant::now();
    let endpoint = match discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(err) => fail_discovery(err),
    };
    println!("step identify ok {} ms", started.elapsed().as_millis());

    let profile_id = match endpoint.resolve_profile(&profile) {
        Ok(profile_id) => profile_id,
        Err(err) => fail("profile lookup", &err.to_string(), err.next_step()),
    };

    let started = Instant::now();
    let ticket = match mint_ticket(
        &home,
        &endpoint,
        &profile_id,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    {
        Ok(ticket) => ticket,
        Err(err) => fail("ticket", &err.to_string(), err.next_step()),
    };
    println!("step ticket ok {} ms", started.elapsed().as_millis());

    let started = Instant::now();
    let mut connection = match connect(&endpoint, ticket).await {
        Ok(connection) => connection,
        Err(err) => fail("upgrade", &err.to_string(), err.next_step()),
    };
    println!("step upgrade ok {} ms", started.elapsed().as_millis());

    let done = match handshake(&mut connection).await {
        Ok(done) => done,
        Err(err) => fail_handshake(err, connection.into_parts().next_request_id),
    };
    println!(
        "step gateway.ready ok {} ms",
        done.timings.ready.as_millis()
    );
    if done.server_requests.is_some() {
        println!(
            "step client.capabilities ok {} ms",
            done.timings.capabilities.as_millis()
        );
    } else {
        println!(
            "step client.capabilities failed {} ms",
            done.timings.capabilities.as_millis()
        );
    }
    println!(
        "step runtime.describe ok {} ms",
        done.timings.describe.as_millis()
    );
    println!("{}", served_line(endpoint.profiles(), &profile));
    println!(
        "gateway code_version={} code_sha={}",
        shown(endpoint.code_version()),
        sha_text(endpoint.code_sha())
    );
    println!("ATTACH OK");
}

fn require(args: &mut impl Iterator<Item = String>, flag: &str) -> String {
    match args.next() {
        Some(value) => value,
        None => {
            println!("missing value for {flag}");
            std::process::exit(2);
        }
    }
}

fn shown(value: Option<&str>) -> &str {
    match value {
        Some(text) if !text.is_empty() => text,
        _ => "unknown",
    }
}

fn sha_text(value: Option<&str>) -> String {
    match value {
        Some(text) if !text.is_empty() => text.chars().take(12).collect(),
        _ => "unknown".to_string(),
    }
}

fn served_line(profiles: &[ServedProfile], typed: &str) -> String {
    let mut listed = Vec::new();
    let mut used = vec![false; profiles.len()];
    for wanted in ["default", "fixture"] {
        if let Some(index) = profiles.iter().position(|profile| profile.name == wanted) {
            used[index] = true;
            listed.push(wanted);
        }
    }
    if typed != "default" && typed != "fixture" {
        let typed_norm = typed.trim().to_ascii_lowercase();
        if let Some(index) = profiles.iter().enumerate().position(|(index, profile)| {
            !used[index] && profile.name.trim().to_ascii_lowercase() == typed_norm
        }) {
            used[index] = true;
            listed.push(typed);
        }
    }
    let others = profiles.len() - listed.len();
    if listed.is_empty() {
        format!("served profiles: and {others} others")
    } else {
        format!("served profiles: {} and {others} others", listed.join(", "))
    }
}

fn fail_discovery(err: DiscoveryError) -> ! {
    let step = match &err {
        DiscoveryError::Ensure(_) | DiscoveryError::StillUnreachableAfterEnsure => "ensure",
        _ => "discovery",
    };
    fail(step, &err.to_string(), err.next_step())
}

fn fail_handshake(err: HandshakeError, next_request_id: u64) -> ! {
    let step = match &err {
        HandshakeError::ReadyTimeout
        | HandshakeError::BadReady
        | HandshakeError::NotSessionAuthority => "gateway.ready",
        HandshakeError::BadFrame | HandshakeError::Closed(_) | HandshakeError::Io(_) => {
            match next_request_id {
                1 => "gateway.ready",
                2 => "client.capabilities",
                _ => "runtime.describe",
            }
        }
        HandshakeError::CallTimeout { method } if *method == "client.capabilities" => {
            "client.capabilities"
        }
        HandshakeError::CallTimeout { .. }
        | HandshakeError::IdentityChanged
        | HandshakeError::BadDescribe
        | HandshakeError::DescribeFailed { .. } => "runtime.describe",
    };
    fail(step, &err.to_string(), err.next_step())
}

fn fail(step: &str, message: &str, next_step: &str) -> ! {
    println!("{}", format_failure(step, message, next_step));
    std::process::exit(1)
}
