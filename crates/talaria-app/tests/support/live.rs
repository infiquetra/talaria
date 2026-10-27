#![allow(dead_code)]
use hermes_client::{
    DiscoverOptions, DiscoveryError, Endpoint, EnsurePolicy, ProfileId, ProfileLookupError,
    default_home, discover_with,
};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
pub struct LiveEnv {
    pub home: PathBuf,
    pub endpoint: Endpoint,
    pub fixture: ProfileId,
}

// The pre-build gateway may predate the pinned identity contract. Only an
// authenticated, well-formed reply proving a different revision may skip it.
// This probe never relaxes discovery, starts a gateway, or accepts an endpoint.
// Pointer-based homes retain the original discovery failure.
async fn incompatible_live_revision(home: &Path) -> Option<String> {
    let home = home.canonicalize().ok()?;
    let uid = rustix::process::getuid().as_raw();
    let metadata = std::fs::symlink_metadata(&home).ok()?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o002 != 0 {
        return None;
    }
    let socket = home.join("gateway.sock");
    let metadata = std::fs::symlink_metadata(&socket).ok()?;
    if !metadata.file_type().is_socket() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return None;
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = UnixStream::connect(socket).await.ok()?;
        if stream.peer_cred().ok()?.uid() != uid {
            return None;
        }
        stream
            .write_all(b"{\"protocol\":1,\"verb\":\"identify\",\"id\":1}\n")
            .await
            .ok()?;
        let mut line = Vec::new();
        BufReader::new(stream.take(524_290))
            .read_until(b'\n', &mut line)
            .await
            .ok()?;
        if line.last() != Some(&b'\n') || line.len() > 524_289 {
            return None;
        }
        let reply: serde_json::Value = serde_json::from_slice(&line).ok()?;
        if reply["ok"] != true || reply["protocol"] != 1 || reply["id"] != 1 {
            return None;
        }
        let result = reply.get("result")?.as_object()?;
        let sha = result.get("code_sha")?.as_str()?;
        // Restrict the only peer text included in a skip reason to a revision.
        if !(7..=40).contains(&sha.len()) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        hermes_contract::pinned::live_skip_reason(Some(sha))
    })
    .await
    .ok()
    .flatten()
}

pub async fn live_preconditions() -> Result<LiveEnv, String> {
    let home = default_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(DiscoveryError::NoGateway) => {
            return Err("no Hermes gateway is running for this home".to_string());
        }
        Err(DiscoveryError::InvalidIdentity) => {
            if let Some(reason) = incompatible_live_revision(&home).await {
                return Err(reason);
            }
            let err = DiscoveryError::InvalidIdentity;
            panic!("discovery failed: {err} | {}", err.next_step());
        }
        Err(err) => panic!("discovery failed: {err} | {}", err.next_step()),
    };
    let fixture = match endpoint.resolve_profile("fixture") {
        Ok(profile) => profile,
        Err(ProfileLookupError::NeverDiscovered { name }) => {
            return Err(format!(
                "the gateway never discovered a profile named {name}"
            ));
        }
        Err(err) => panic!("profile lookup failed: {err} | {}", err.next_step()),
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        return Err(reason);
    }
    Ok(LiveEnv {
        home,
        endpoint,
        fixture,
    })
}

pub fn report_skip(prefix: &str, reason: &str) {
    println!("{prefix}: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live gateway required");
    }
}
