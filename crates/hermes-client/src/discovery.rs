//! Find the Hermes home and the gateway that serves it.
//!
//! Departures from Hermes:
//!
//! State is checked before the served profile list, so a gateway that is still
//! starting is "not ready" rather than a profile mismatch.
//!
//! A home that its group can write is accepted. The peer user id authenticates
//! the listener. Hermes refuses that home earlier; minting a ticket then fails
//! with an authentication error whose next step says to remove group write.
//!
//! Ensure is not run for an incompatible gateway. Doing so would stop that
//! gateway and start a replacement, which can cut off another client's turn.
//!
//! Ensure is not run for a named-profile home (the parent directory is named
//! `profiles`). Doing so could start or replace a gateway for a profile the
//! caller did not name.
//!
//! Discovery is repeated once after ensure, and only then.

use serde_json::{Value, json};
use sha2::Digest;
use std::io::Read;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadVerb {
    Identify,
    ActiveWork,
}

impl ReadVerb {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Identify => "identify",
            Self::ActiveWork => "active-work",
        }
    }
}

pub async fn control_query(
    _home: &Path,
    _verb: ReadVerb,
    _options: &DiscoverOptions,
) -> Result<Value, DiscoveryError> {
    Err(DiscoveryError::InvalidReply)
}

/// A gateway Talaria can attach to. Fields stay private so a path cannot leak
/// through a public field. [`Debug`] prints no path and only the first eight
/// characters of an id.
#[derive(Clone)]
pub struct Endpoint {
    api_origin: String,
    instance_id: String,
    authority_epoch: u64,
    runtime_protocol: u8,
    state: GatewayState,
    capabilities: Vec<String>,
    code_sha: Option<String>,
    code_version: Option<String>,
    profiles: Vec<ServedProfile>,
    parked: Vec<(String, String)>,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix: String = self.instance_id.chars().take(8).collect();
        f.debug_struct("Endpoint")
            .field("state", &self.state)
            .field("instance_id_prefix", &prefix)
            .field("authority_epoch", &self.authority_epoch)
            .field("profiles", &self.profiles.len())
            .field("parked", &self.parked.len())
            .finish()
    }
}

impl Endpoint {
    pub fn api_origin(&self) -> &str {
        &self.api_origin
    }
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub fn authority_epoch(&self) -> u64 {
        self.authority_epoch
    }
    pub fn runtime_protocol(&self) -> u8 {
        self.runtime_protocol
    }
    pub fn state(&self) -> GatewayState {
        self.state
    }
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }
    pub fn code_sha(&self) -> Option<&str> {
        self.code_sha.as_deref()
    }
    pub fn code_version(&self) -> Option<&str> {
        self.code_version.as_deref()
    }
    pub fn profiles(&self) -> &[ServedProfile] {
        &self.profiles
    }
    pub fn parked(&self) -> &[(String, String)] {
        &self.parked
    }

    /// Build an endpoint from an `identify` result object.
    ///
    /// State is checked before served profiles.
    pub fn from_identify(
        result: &serde_json::Value,
        home: &Path,
    ) -> Result<Endpoint, DiscoveryError> {
        if result["runtime_protocol"].as_u64() != Some(1) {
            return Err(DiscoveryError::Incompatible);
        }
        let state = match result["state"].as_str() {
            Some("ready") => GatewayState::Ready,
            Some("draining") if result["drain_reason"] == "restart_wait" => {
                GatewayState::DrainingForRestart
            }
            Some("starting") => {
                return Err(DiscoveryError::NotReady {
                    state: ReportedState::Starting,
                    idle_exit: false,
                });
            }
            Some("draining") => {
                return Err(DiscoveryError::NotReady {
                    state: ReportedState::Draining,
                    idle_exit: result["drain_reason"] == "idle_exit",
                });
            }
            Some("conflict") => {
                return Err(DiscoveryError::NotReady {
                    state: ReportedState::Conflict,
                    idle_exit: false,
                });
            }
            _ => return Err(DiscoveryError::InvalidState),
        };
        let capabilities: Vec<String> = serde_json::from_value(result["capabilities"].clone())
            .map_err(|_| DiscoveryError::Incompatible)?;
        if !capabilities.iter().any(|cap| cap == "session-authority-v1") {
            return Err(DiscoveryError::Incompatible);
        }
        let authority_epoch = result["authority_epoch"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or(DiscoveryError::InvalidIdentity)?;
        let instance_id = result["instance_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or(DiscoveryError::InvalidIdentity)?
            .to_owned();
        let mut parked = Vec::new();
        if let Some(value) = result.get("parked_profiles") {
            for (name, reason) in value.as_object().ok_or(DiscoveryError::InvalidIdentity)? {
                parked.push((
                    scrub_peer_text(name, home, 200),
                    scrub_peer_text(
                        reason.as_str().ok_or(DiscoveryError::InvalidIdentity)?,
                        home,
                        200,
                    ),
                ));
            }
        }
        let api_origin = result["api_origin"]
            .as_str()
            .filter(|origin| valid_api_origin(origin))
            .ok_or(DiscoveryError::InvalidApiOrigin)?
            .to_owned();
        if !matches!(
            result["supervisor"].as_str(),
            Some("none" | "systemd" | "launchd" | "windows" | "external")
        ) {
            return Err(DiscoveryError::InvalidSupervisor);
        }
        let entries = result["served_profiles"]
            .as_array()
            .ok_or(DiscoveryError::HomeNotServed)?;
        let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let wanted = canonical(home);
        let matches = entries
            .iter()
            .filter(|entry| {
                entry["home"]
                    .as_str()
                    .is_some_and(|s| canonical(Path::new(s)) == wanted)
                    && entry["profile_id"].as_str().is_some_and(|s| !s.is_empty())
            })
            .count();
        if matches != 1 {
            return Err(DiscoveryError::HomeNotServed);
        }
        let mut profiles = Vec::<ServedProfile>::new();
        for entry in entries {
            let path = Path::new(
                entry["home"]
                    .as_str()
                    .ok_or(DiscoveryError::InvalidIdentity)?,
            );
            let id = entry["profile_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or(DiscoveryError::InvalidIdentity)?;
            let name = if named_home(path) {
                path.file_name()
                    .and_then(|s| s.to_str())
                    .ok_or(DiscoveryError::InvalidIdentity)?
                    .to_lowercase()
            } else {
                "default".to_string()
            };
            if profiles.iter().any(|p| p.name == name) {
                return Err(DiscoveryError::InvalidIdentity);
            }
            profiles.push(ServedProfile {
                name,
                id: ProfileId(id.to_owned()),
            });
        }
        Ok(Self {
            api_origin,
            instance_id,
            authority_epoch,
            runtime_protocol: 1,
            state,
            capabilities,
            code_sha: result["code_sha"]
                .as_str()
                .map(|s| scrub_peer_text(s, home, 100)),
            code_version: result["code_version"]
                .as_str()
                .map(|s| scrub_peer_text(s, home, 100)),
            profiles,
            parked,
        })
    }

    /// Return the profile id for a typed name, or say why it is not usable.
    pub fn resolve_profile(&self, name: &str) -> Result<ProfileId, ProfileLookupError> {
        let name = name.trim().to_lowercase();
        if let Some(profile) = self.profiles.iter().find(|profile| profile.name == name) {
            return Ok(profile.id.clone());
        }
        // Names supplied by a caller are untrusted diagnostic text too.
        let shown = scrub_peer_text(&name, Path::new(""), 200);
        if let Some((_, reason)) = self.parked.iter().find(|(parked, _)| parked == &name) {
            return Err(ProfileLookupError::Parked {
                name: shown,
                reason: reason.clone(),
            });
        }
        Err(ProfileLookupError::NeverDiscovered { name: shown })
    }

    /// Copy this endpoint with a different instance id. Live test 32 uses it.
    #[doc(hidden)]
    pub fn with_instance_id(&self, id: &str) -> Endpoint {
        Self {
            instance_id: id.to_owned(),
            ..self.clone()
        }
    }

    /// Copy this endpoint with a validated loopback relay origin, keeping its identity.
    #[doc(hidden)]
    pub fn with_api_origin(&self, origin: &str) -> Result<Endpoint, DiscoveryError> {
        if !valid_api_origin(origin) {
            return Err(DiscoveryError::InvalidApiOrigin);
        }
        Ok(Self {
            api_origin: origin.to_owned(),
            ..self.clone()
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayState {
    Ready,
    DrainingForRestart,
}

#[derive(Debug, Clone)]
pub struct DiscoverOptions {
    pub control_timeout: Duration,
    pub ensure: EnsurePolicy,
}

impl Default for DiscoverOptions {
    fn default() -> Self {
        Self {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Run {
                program: PathBuf::from("hermes"),
                timeout: Duration::from_secs(90),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum EnsurePolicy {
    Run { program: PathBuf, timeout: Duration },
    Never,
}

/// First 16 lower-case hex digits of the SHA-256 of the canonical home.
pub fn home_hash(canonical_home: &str) -> String {
    sha2::Sha256::digest(canonical_home.as_bytes())[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone)]
pub struct ServedProfile {
    pub name: String,
    pub id: ProfileId,
}

/// The exact `profile_id` string the gateway sent. Never normalised.
#[derive(Clone)]
pub struct ProfileId(String);

impl std::fmt::Debug for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = &self.0;
        f.write_str("ProfileId(<path>)")
    }
}

impl AsRef<str> for ProfileId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for ProfileId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileLookupError {
    #[error("profile {name} is parked by the gateway: {reason}")]
    Parked { name: String, reason: String },
    #[error("the gateway never discovered a profile named {name}")]
    NeverDiscovered { name: String },
}

impl ProfileLookupError {
    pub fn next_step(&self) -> &'static str {
        match self {
            Self::Parked { .. } => {
                "Fix the cause the gateway reports, then check `hermes gateway status`."
            }
            Self::NeverDiscovered { .. } => {
                "Create the profile, or check the name with `hermes profile list`. Talaria does not create profiles."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Home,
    Socket,
    PointerFile,
    PointerDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeProblem {
    WrongType,
    WrongOwner,
    OpenToOthers,
    WritableByOthers,
    PeerIsAnotherUser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerProblem {
    TooLarge,
    NotAbsolute,
    WrongLeaf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportedState {
    Starting,
    Draining,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitKind {
    InvalidInvocation,
    Incompatible,
    Authorization,
    Deadline,
    Draining,
    Inaccessible,
    Other,
}

#[derive(Debug, thiserror::Error)]
pub enum EnsureError {
    NotInstalled,
    TimedOut,
    UnreadableOutput,
    Exit {
        code: i32,
        kind: ExitKind,
        state: Option<String>,
        reason_code: Option<String>,
        detail: Option<String>,
    },
}

impl std::fmt::Display for EnsureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("the `hermes` program was not found"),
            Self::TimedOut => f.write_str("`ensure` did not finish in time"),
            Self::UnreadableOutput => f.write_str("`ensure` printed output Talaria could not read"),
            Self::Exit { code, kind, .. } => {
                write!(f, "hermes gateway ensure failed with exit {code}")?;
                if *kind == ExitKind::Incompatible {
                    write!(f, " (incompatible)")?;
                }
                Ok(())
            }
        }
    }
}

impl EnsureError {
    pub fn next_step(&self) -> &'static str {
        match self {
            Self::NotInstalled => "Install Hermes, or give the path to the `hermes` program.",
            Self::TimedOut => {
                "Run `hermes gateway ensure --json` yourself and read the gateway log."
            }
            Self::UnreadableOutput => {
                "Run `hermes gateway ensure --json` yourself and read its output."
            }
            Self::Exit {
                code, reason_code, ..
            } => exit_next_step(*code, reason_code.as_deref()),
        }
    }
}

fn exit_next_step(code: i32, reason: Option<&str>) -> &'static str {
    match code {
        2 => "Talaria called Hermes incorrectly. Report a bug.",
        3 => "Update Hermes to v0.22.3 or later and restart its gateway.",
        4 => match reason {
            Some("profile_mismatch") => {
                "The gateway does not serve this Hermes home; check HERMES_HOME and run `hermes gateway status`."
            }
            Some("authorization") | None => "Run Talaria as the user who runs the gateway.",
            Some(_) => "Run Talaria as the user who runs the gateway, then restart the gateway.",
        },
        5 => "Run `hermes gateway ensure --json` yourself and read the gateway log.",
        6 => "Wait a minute and try again.",
        7 => "Check `hermes gateway status`.",
        _ => "Run `hermes gateway ensure --json` yourself and read its output.",
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    NoGateway,
    ControlTimeout,
    UnsafeNode {
        node: NodeKind,
        problem: NodeProblem,
    },
    BadPointer(PointerProblem),
    ReplyTooLarge,
    InvalidReply,
    ControlRefused {
        code: Option<String>,
        text: String,
    },
    Incompatible,
    NotReady {
        state: ReportedState,
        idle_exit: bool,
    },
    InvalidState,
    InvalidApiOrigin,
    InvalidSupervisor,
    InvalidIdentity,
    HomeNotServed,
    NamedProfileHome,
    Ensure(#[source] EnsureError),
    StillUnreachableAfterEnsure,
    Io(std::io::ErrorKind),
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoGateway => f.write_str("no Hermes gateway is running for this home"),
            Self::ControlTimeout => f.write_str("the gateway did not answer in time"),
            Self::UnsafeNode { node, problem } => f.write_str(unsafe_node_message(*node, *problem)),
            Self::BadPointer(problem) => f.write_str(pointer_message(*problem)),
            Self::ReplyTooLarge => {
                f.write_str("the gateway sent a control reply that is too large.")
            }
            Self::InvalidReply => {
                f.write_str("the gateway sent a control reply Talaria could not read.")
            }
            Self::ControlRefused { .. } => f.write_str("the gateway refused the control request."),
            Self::Incompatible => f.write_str("incompatible"),
            Self::NotReady { state, .. } => {
                write!(f, "the gateway is {}", reported_state_word(*state))
            }
            Self::InvalidState => {
                f.write_str("the gateway reported a state Talaria does not understand.")
            }
            Self::InvalidApiOrigin => {
                f.write_str("the gateway reported an API address Talaria cannot use.")
            }
            Self::InvalidSupervisor => {
                f.write_str("the gateway reported a supervisor Talaria does not understand.")
            }
            Self::InvalidIdentity => {
                f.write_str("the gateway reported an identity Talaria cannot use.")
            }
            Self::HomeNotServed => f.write_str("the gateway does not serve this Hermes home."),
            Self::NamedProfileHome => {
                f.write_str("this home belongs to a named profile that the default gateway serves")
            }
            Self::Ensure(err) => write!(f, "{err}"),
            Self::StillUnreachableAfterEnsure => {
                f.write_str("the gateway is still not reachable after `ensure`")
            }
            Self::Io(_) => f.write_str("the gateway connection failed."),
        }
    }
}

impl DiscoveryError {
    pub fn next_step(&self) -> &'static str {
        match self {
            Self::NoGateway => {
                "Run `hermes gateway ensure --json` to start the Hermes gateway, then try again."
            }
            Self::ControlTimeout => "Check `hermes gateway status`.",
            Self::UnsafeNode { node, problem } => unsafe_node_step(*node, *problem),
            Self::BadPointer(_) => "Restart the gateway so it writes the pointer again.",
            Self::ReplyTooLarge
            | Self::InvalidReply
            | Self::InvalidState
            | Self::InvalidSupervisor => "Update Hermes and try again.",
            Self::ControlRefused { .. } | Self::InvalidApiOrigin => {
                "Check `hermes gateway status`."
            }
            Self::Incompatible => "Update Hermes to v0.22.3 or later and restart its gateway.",
            Self::NotReady { state, .. } => match state {
                ReportedState::Starting => "Wait a few seconds and try again.",
                ReportedState::Draining => {
                    "The gateway is shutting down; wait a minute and try again."
                }
                ReportedState::Conflict => "Check `hermes gateway status`.",
            },
            Self::InvalidIdentity => "Restart the gateway and try again.",
            Self::HomeNotServed => "Check HERMES_HOME and run `hermes gateway status`.",
            Self::NamedProfileHome => {
                "Set HERMES_HOME to the Hermes root (the folder that holds `profiles/`) and choose the profile with `--profile`."
            }
            Self::Ensure(err) => err.next_step(),
            Self::StillUnreachableAfterEnsure => {
                "Read the gateway log, or run `hermes gateway ensure --json` yourself and read its output."
            }
            Self::Io(_) => "Try again.",
        }
    }
}

pub(crate) fn unsafe_node_message(node: NodeKind, problem: NodeProblem) -> &'static str {
    match (node, problem) {
        (NodeKind::Socket, NodeProblem::OpenToOthers) => {
            "the control socket has unsafe permissions"
        }
        (NodeKind::Socket, NodeProblem::WrongType) => "the gateway socket path is not a socket",
        (NodeKind::Socket, NodeProblem::WrongOwner) => "the control socket belongs to another user",
        (NodeKind::Socket, NodeProblem::PeerIsAnotherUser) => "the gateway runs as another user",
        (NodeKind::Home, NodeProblem::WritableByOthers) => {
            "the Hermes home is writable by other users"
        }
        (NodeKind::Home, NodeProblem::WrongType | NodeProblem::WrongOwner) => {
            "the Hermes home is not a directory that you own"
        }
        (NodeKind::PointerFile | NodeKind::PointerDir, _) => {
            "the gateway's fallback socket pointer has unsafe permissions or the wrong owner or type"
        }
        _ => "a gateway file has unsafe permissions or the wrong owner or type.",
    }
}

pub(crate) fn unsafe_node_step(node: NodeKind, problem: NodeProblem) -> &'static str {
    match (node, problem) {
        (NodeKind::Socket, NodeProblem::OpenToOthers) => {
            "Fix the permissions (`chmod go-rwx` on the socket), or run Talaria as the user who runs the gateway."
        }
        (NodeKind::Socket, NodeProblem::WrongType) => {
            "Remove the leftover file named gateway.sock from the Hermes home, then run `hermes gateway ensure --json`."
        }
        (NodeKind::Socket, NodeProblem::WrongOwner | NodeProblem::PeerIsAnotherUser) => {
            "Run Talaria as the user who runs the gateway."
        }
        (NodeKind::Home, NodeProblem::WritableByOthers) => {
            "Remove write access for other users (`chmod o-w` on the Hermes home), or run Talaria as the user who runs the gateway."
        }
        (NodeKind::Home, NodeProblem::WrongType | NodeProblem::WrongOwner) => {
            "Check HERMES_HOME, or run Talaria as the user who owns the Hermes home."
        }
        (NodeKind::PointerFile | NodeKind::PointerDir, _) => {
            "Restart the gateway so it writes the pointer again, or run Talaria as the user who runs the gateway."
        }
        _ => "Run Talaria as the user who runs the gateway.",
    }
}

fn pointer_message(problem: PointerProblem) -> &'static str {
    match problem {
        PointerProblem::TooLarge => "the gateway pointer file is too large.",
        PointerProblem::NotAbsolute => {
            "the gateway pointer file does not hold an absolute socket path."
        }
        PointerProblem::WrongLeaf => {
            "the gateway pointer file does not name the socket for this home."
        }
    }
}

fn reported_state_word(state: ReportedState) -> &'static str {
    match state {
        ReportedState::Starting => "starting",
        ReportedState::Draining => "draining",
        ReportedState::Conflict => "in conflict",
    }
}

/// The Hermes home Talaria would use for this process.
///
/// The pure rules live in the private `default_home_from` helper.
pub fn default_home() -> PathBuf {
    let user_home = std::env::var_os("HOME").map(PathBuf::from);
    default_home_from(|key| std::env::var(key).ok(), user_home.as_deref())
}

pub(crate) fn default_home_from(
    env: impl Fn(&str) -> Option<String>,
    user_home: Option<&Path>,
) -> PathBuf {
    let raw = env("HERMES_HOME").unwrap_or_default();
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
        let expanded = expand_vars(trimmed, &env);
        return expand_tilde(&expanded, user_home);
    }
    let suffix = env("HERMES_DATA_DIR_SUFFIX").unwrap_or_default();
    let folder = format!(".hermes{suffix}");
    match user_home {
        Some(home) => home.join(folder),
        None => PathBuf::from(folder),
    }
}

fn expand_vars(text: &str, env: &impl Fn(&str) -> Option<String>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            if chars[i + 1] == '{' {
                if let Some(rel_end) = chars[i + 2..].iter().position(|c| *c == '}') {
                    let name: String = chars[i + 2..i + 2 + rel_end].iter().collect();
                    let consumed = i + 3 + rel_end;
                    if is_env_name(&name) {
                        match env(&name) {
                            Some(value) => out.push_str(&value),
                            None => push_chars(&mut out, &chars[i..consumed]),
                        }
                        i = consumed;
                        continue;
                    }
                }
            } else if is_env_start(chars[i + 1]) {
                let mut j = i + 1;
                while j < chars.len() && is_env_cont(chars[j]) {
                    j += 1;
                }
                let name: String = chars[i + 1..j].iter().collect();
                match env(&name) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('$');
                        out.push_str(&name);
                    }
                }
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn push_chars(out: &mut String, chars: &[char]) {
    for ch in chars {
        out.push(*ch);
    }
}

fn is_env_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_env_cont(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if is_env_start(first) => chars.all(is_env_cont),
        _ => false,
    }
}

fn expand_tilde(text: &str, user_home: Option<&Path>) -> PathBuf {
    if (text == "~" || text.starts_with("~/"))
        && let Some(home) = user_home
    {
        if text == "~" {
            return home.to_path_buf();
        }
        return home.join(&text[2..]);
    }
    PathBuf::from(text)
}

/// Discover the gateway for `home`, running ensure when the default policy says so.
pub async fn discover(home: &Path) -> Result<Endpoint, DiscoveryError> {
    discover_with(home, &DiscoverOptions::default()).await
}

/// Discover the gateway for `home` under an explicit policy.
pub async fn discover_with(
    home: &Path,
    options: &DiscoverOptions,
) -> Result<Endpoint, DiscoveryError> {
    let supplied_home = home;
    if !home.is_absolute() {
        return Err(DiscoveryError::NoGateway);
    }
    let home = match home.canonicalize() {
        Ok(home) => home,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(DiscoveryError::NoGateway);
        }
        Err(err) => return Err(DiscoveryError::Io(err.kind())),
    };
    let first = identify(&home, supplied_home, options.control_timeout).await;
    let eligible = matches!(
        &first,
        Err(DiscoveryError::NoGateway
            | DiscoveryError::NotReady {
                state: ReportedState::Starting,
                ..
            }
            | DiscoveryError::NotReady {
                state: ReportedState::Draining,
                idle_exit: true
            })
    );
    if eligible
        && !named_home(&home)
        && let EnsurePolicy::Run { program, timeout } = &options.ensure
    {
        run_ensure(program, &home, supplied_home, *timeout).await?;
        return match identify(&home, supplied_home, options.control_timeout).await {
            Err(DiscoveryError::NoGateway) => Err(DiscoveryError::StillUnreachableAfterEnsure),
            other => other,
        };
    }
    first
}

/// Group write on a home is accepted. Only write permission for others is refused.
pub(crate) fn home_mode_ok(mode: u32) -> bool {
    mode & 0o002 == 0
}

/// A socket is private when no group or other permission bit is set.
pub(crate) fn socket_mode_ok(mode: u32) -> bool {
    mode & 0o077 == 0
}

/// True when the connected peer is this user. The only check that proves who is listening.
pub(crate) fn peer_is_self(peer_uid: u32, my_uid: u32) -> bool {
    peer_uid == my_uid
}

/// User id of this process, for the peer check in discovery.
#[allow(dead_code)]
pub(crate) fn current_uid() -> u32 {
    rustix::process::getuid().as_raw()
}

/// Replace a home path, other absolute paths, and control characters.
///
/// This is a heuristic filter. It reduces accidental leaks. It is not a
/// security boundary. The boundary is that no error variant stores raw peer text.
pub(crate) fn scrub_peer_text(text: &str, home: &Path, limit: usize) -> String {
    let mut homes = vec![home.to_string_lossy().into_owned()];
    if let Ok(canonical) = home.canonicalize() {
        homes.push(canonical.to_string_lossy().into_owned());
    }
    homes.sort_by_key(|s| std::cmp::Reverse(s.len()));
    homes.dedup();
    let mut text = text.to_owned();
    for home in homes.into_iter().filter(|s| !s.is_empty()) {
        text = text.replace(&home, "<home>");
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let starts = i == 0 || chars[i - 1].is_whitespace() || "'\"([=,:".contains(chars[i - 1]);
        if chars[i] == '/' && starts && chars.get(i + 1) != Some(&'/') {
            out.push_str("<path>");
            i += 1;
            while i < chars.len() && !chars[i].is_whitespace() && !"\"',;:)]}<>".contains(chars[i])
            {
                i += 1;
            }
        } else {
            out.push(if chars[i].is_control() { ' ' } else { chars[i] });
            i += 1;
        }
    }
    out.chars().take(limit).collect()
}

/// Build the ensure command. The caller runs it. Standard input is closed by the runner.
pub(crate) fn ensure_command(program: &Path, canonical_home: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    command
        .args(["gateway", "ensure", "--json"])
        .env("HERMES_HOME", canonical_home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    for name in [
        "HERMES_MODEL",
        "HERMES_INFERENCE_MODEL",
        "HERMES_TUI_PROVIDER",
        "HERMES_INFERENCE_PROVIDER",
        "HERMES_TUI_TOOLSETS",
        "HERMES_TUI_SKILLS",
        "HERMES_CWD",
        "TERMINAL_CWD",
        "HERMES_YOLO",
        "HERMES_ACCEPT_HOOKS",
        "HERMES_TUI_CHECKPOINTS",
        "HERMES_TUI_PASS_SESSION_ID",
        "HERMES_KANBAN_BOARD",
        "HERMES_TUI_CLIENT_ENV",
    ] {
        command.env_remove(name);
    }
    command
}

fn named_home(home: &Path) -> bool {
    home.parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "profiles")
}

fn valid_api_origin(origin: &str) -> bool {
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let Some((host, port)) = authority.rsplit_once(':') else {
        return false;
    };
    if port.starts_with('0')
        || !port.bytes().all(|b| b.is_ascii_digit())
        || !port.parse::<u16>().is_ok_and(|n| n > 0)
    {
        return false;
    }
    if host == "[::1]" {
        return true;
    }
    let groups: Vec<&str> = host.split('.').collect();
    groups.len() == 4
        && groups[0] == "127"
        && groups.iter().all(|group| {
            !group.is_empty()
                && (group.len() == 1 || !group.starts_with('0'))
                && group.bytes().all(|b| b.is_ascii_digit())
                && group.parse::<u8>().is_ok()
        })
}

fn absent(home: &Path) -> DiscoveryError {
    if named_home(home) {
        DiscoveryError::NamedProfileHome
    } else {
        DiscoveryError::NoGateway
    }
}

fn io_error(home: &Path, err: std::io::Error) -> DiscoveryError {
    if matches!(
        err.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
    ) {
        absent(home)
    } else {
        DiscoveryError::Io(err.kind())
    }
}

fn check_node(meta: &std::fs::Metadata, node: NodeKind) -> Result<(), DiscoveryError> {
    let correct_type = match node {
        NodeKind::Home | NodeKind::PointerDir => meta.is_dir(),
        NodeKind::PointerFile => meta.is_file(),
        NodeKind::Socket => meta.file_type().is_socket(),
    };
    let problem = if !correct_type {
        Some(NodeProblem::WrongType)
    } else if meta.uid() != current_uid() {
        Some(NodeProblem::WrongOwner)
    } else if node == NodeKind::Home && !home_mode_ok(meta.mode()) {
        Some(NodeProblem::WritableByOthers)
    } else if node != NodeKind::Home && !socket_mode_ok(meta.mode()) {
        Some(NodeProblem::OpenToOthers)
    } else {
        None
    };
    match problem {
        Some(problem) => Err(DiscoveryError::UnsafeNode { node, problem }),
        None => Ok(()),
    }
}

fn socket_path(home: &Path) -> Result<PathBuf, DiscoveryError> {
    check_node(
        &std::fs::symlink_metadata(home).map_err(|e| io_error(home, e))?,
        NodeKind::Home,
    )?;
    let direct = home.join("gateway.sock");
    match std::fs::symlink_metadata(&direct) {
        Ok(meta) => {
            check_node(&meta, NodeKind::Socket)?;
            return Ok(direct);
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(io_error(home, err)),
    }
    let pointer = home.join("gateway.sock.path");
    let meta = std::fs::symlink_metadata(&pointer).map_err(|e| io_error(home, e))?;
    check_node(&meta, NodeKind::PointerFile)?;
    let fd = rustix::fs::open(
        &pointer,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(|err| {
        if err == rustix::io::Errno::LOOP {
            DiscoveryError::UnsafeNode {
                node: NodeKind::PointerFile,
                problem: NodeProblem::WrongType,
            }
        } else {
            io_error(home, err.into())
        }
    })?;
    let file = std::fs::File::from(fd);
    check_node(
        &file.metadata().map_err(|e| io_error(home, e))?,
        NodeKind::PointerFile,
    )?;
    let mut bytes = Vec::new();
    file.take(4097)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(home, e))?;
    if bytes.len() > 4096 {
        return Err(DiscoveryError::BadPointer(PointerProblem::TooLarge));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| DiscoveryError::BadPointer(PointerProblem::NotAbsolute))?;
    let path = Path::new(text.trim());
    if !path.is_absolute() {
        return Err(DiscoveryError::BadPointer(PointerProblem::NotAbsolute));
    }
    let parent = path
        .parent()
        .ok_or(DiscoveryError::BadPointer(PointerProblem::WrongLeaf))?;
    let expected = format!("hermes-gw-{}", home_hash(&home.to_string_lossy()));
    if path.file_name().is_none_or(|s| s != "control.sock")
        || parent.file_name().is_none_or(|s| s != expected.as_str())
    {
        return Err(DiscoveryError::BadPointer(PointerProblem::WrongLeaf));
    }
    check_node(
        &std::fs::symlink_metadata(parent).map_err(|e| io_error(home, e))?,
        NodeKind::PointerDir,
    )?;
    check_node(
        &std::fs::symlink_metadata(path).map_err(|e| io_error(home, e))?,
        NodeKind::Socket,
    )?;
    Ok(path.to_path_buf())
}

/// A fresh authenticated control connection, bounded from connect through newline.
pub(crate) async fn control_request(
    home: &Path,
    request: Value,
    timeout: Duration,
) -> Result<Value, DiscoveryError> {
    tokio::time::timeout(timeout, async {
        let path = socket_path(home)?;
        let mut socket = tokio::net::UnixStream::connect(path)
            .await
            .map_err(|e| io_error(home, e))?;
        let peer = socket
            .peer_cred()
            .map_err(|e| DiscoveryError::Io(e.kind()))?;
        if !peer_is_self(peer.uid(), current_uid()) {
            return Err(DiscoveryError::UnsafeNode {
                node: NodeKind::Socket,
                problem: NodeProblem::PeerIsAnotherUser,
            });
        }
        let mut bytes = if request["verb"] == "identify" {
            br#"{"protocol":1,"verb":"identify","id":1}"#.to_vec()
        } else {
            serde_json::to_vec(&request).map_err(|_| DiscoveryError::InvalidReply)?
        };
        bytes.push(b'\n');
        socket
            .write_all(&bytes)
            .await
            .map_err(|e| DiscoveryError::Io(e.kind()))?;
        let mut reader = tokio::io::BufReader::new(socket);
        let mut line = Vec::new();
        loop {
            let chunk = reader
                .fill_buf()
                .await
                .map_err(|e| DiscoveryError::Io(e.kind()))?;
            if chunk.is_empty() {
                return Err(DiscoveryError::InvalidReply);
            }
            let end = chunk.iter().position(|b| *b == b'\n');
            let count = end.unwrap_or(chunk.len());
            if line.len() + count > 524_288 {
                return Err(DiscoveryError::ReplyTooLarge);
            }
            line.extend_from_slice(&chunk[..count]);
            reader.consume(count + usize::from(end.is_some()));
            if end.is_some() {
                break;
            }
        }
        let reply: Value =
            serde_json::from_slice(&line).map_err(|_| DiscoveryError::InvalidReply)?;
        // Discovery can receive a refusal before the gateway has an envelope.
        if request["verb"] == "identify" && reply["ok"] == false {
            return Ok(reply);
        }
        if reply["protocol"].as_u64() != Some(1)
            || reply["id"] != request["id"]
            || !reply["id"].is_u64()
            || !reply["ok"].is_boolean()
        {
            return Err(DiscoveryError::InvalidReply);
        }
        Ok(reply)
    })
    .await
    .map_err(|_| DiscoveryError::ControlTimeout)?
}

async fn identify(
    home: &Path,
    supplied_home: &Path,
    timeout: Duration,
) -> Result<Endpoint, DiscoveryError> {
    let reply = control_request(
        home,
        json!({"protocol":1,"verb":"identify","id":1}),
        timeout,
    )
    .await?;
    if reply["ok"] == false {
        let error = reply["error"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or(DiscoveryError::InvalidReply)?;
        let code = reply["code"]
            .as_str()
            .map(|s| scrub_peer_text(s, supplied_home, 80));
        return Err(DiscoveryError::ControlRefused {
            code,
            text: scrub_peer_text(error, supplied_home, 200),
        });
    }
    if !reply["result"].is_object() {
        return Err(DiscoveryError::InvalidReply);
    }
    Endpoint::from_identify(&reply["result"], supplied_home)
}

async fn run_ensure(
    program: &Path,
    home: &Path,
    supplied_home: &Path,
    timeout: Duration,
) -> Result<(), DiscoveryError> {
    use tokio::io::AsyncReadExt;
    let deadline = tokio::time::Instant::now() + timeout;
    let mut child = ensure_command(program, home).spawn().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            DiscoveryError::Ensure(EnsureError::NotInstalled)
        } else {
            DiscoveryError::Io(err.kind())
        }
    })?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or(DiscoveryError::Ensure(EnsureError::UnreadableOutput))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or(DiscoveryError::Ensure(EnsureError::UnreadableOutput))?;
    let mut output = Vec::new();
    let mut discard = tokio::io::sink();
    let status = {
        let pipes = async {
            tokio::join!(
                stdout.read_to_end(&mut output),
                tokio::io::copy(&mut stderr, &mut discard)
            )
        };
        tokio::pin!(pipes);
        let mut drained = false;
        let status = loop {
            tokio::select! {
                status = child.wait() => break status.map_err(|e| DiscoveryError::Io(e.kind()))?,
                _ = &mut pipes, if !drained => { drained = true; },
                _ = tokio::time::sleep_until(deadline) => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Err(DiscoveryError::Ensure(EnsureError::TimedOut));
                }
            }
        };
        if !drained {
            let drain_deadline = deadline.min(tokio::time::Instant::now() + Duration::from_secs(2));
            if tokio::time::timeout_at(drain_deadline, &mut pipes)
                .await
                .is_err()
                && drain_deadline == deadline
            {
                return Err(DiscoveryError::Ensure(EnsureError::TimedOut));
            }
        }
        status
    };
    let code = status.code().unwrap_or(-1);
    let value = String::from_utf8_lossy(&output)
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| serde_json::from_str::<Value>(line).ok())
        .filter(Value::is_object);
    if code == 0 {
        return if value.as_ref().is_some_and(|v| v["state"] == "ready") {
            Ok(())
        } else {
            Err(DiscoveryError::Ensure(EnsureError::UnreadableOutput))
        };
    }
    let field = |name: &str, limit| {
        value
            .as_ref()
            .and_then(|v| v[name].as_str())
            .map(|s| scrub_peer_text(s, supplied_home, limit))
    };
    let kind = match code {
        2 => ExitKind::InvalidInvocation,
        3 => ExitKind::Incompatible,
        4 => ExitKind::Authorization,
        5 => ExitKind::Deadline,
        6 => ExitKind::Draining,
        7 => ExitKind::Inaccessible,
        _ => ExitKind::Other,
    };
    let reason_code = field("reason_code", 65).filter(|s| {
        !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    });
    Err(DiscoveryError::Ensure(EnsureError::Exit {
        code,
        kind,
        state: field("state", 200),
        reason_code,
        detail: field("detail", 500),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &[(&str, &str)], key: &str) -> Option<String> {
        pairs
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| (*value).to_string())
    }

    #[test]
    fn default_home_from_rules() {
        let user = Path::new("/example-user");
        type Row<'a> = (&'a str, &'a [(&'a str, &'a str)], Option<&'a Path>, &'a str);
        let rows: &[Row<'_>] = &[
            ("unset", &[], Some(user), "/example-user/.hermes"),
            (
                "empty",
                &[("HERMES_HOME", "")],
                Some(user),
                "/example-user/.hermes",
            ),
            (
                "spaces",
                &[("HERMES_HOME", "   ")],
                Some(user),
                "/example-user/.hermes",
            ),
            (
                "absolute",
                &[("HERMES_HOME", "/example-home")],
                Some(user),
                "/example-home",
            ),
            (
                "trimmed",
                &[("HERMES_HOME", " /example-home ")],
                Some(user),
                "/example-home",
            ),
            (
                "tilde-slash",
                &[("HERMES_HOME", "~/hh")],
                Some(user),
                "/example-user/hh",
            ),
            (
                "dollar",
                &[("HERMES_HOME", "$ROOT/hh"), ("ROOT", "/example-root")],
                Some(user),
                "/example-root/hh",
            ),
            (
                "braces",
                &[("HERMES_HOME", "${ROOT}/hh"), ("ROOT", "/example-root")],
                Some(user),
                "/example-root/hh",
            ),
            (
                "suffix",
                &[("HERMES_DATA_DIR_SUFFIX", "-alt")],
                Some(user),
                "/example-user/.hermes-alt",
            ),
            (
                "suffix-ignored",
                &[
                    ("HERMES_HOME", "/example-home"),
                    ("HERMES_DATA_DIR_SUFFIX", "-alt"),
                ],
                Some(user),
                "/example-home",
            ),
            (
                "bare-tilde",
                &[("HERMES_HOME", "~")],
                Some(user),
                "/example-user",
            ),
            (
                "tilde-only-slash",
                &[("HERMES_HOME", "~/")],
                Some(user),
                "/example-user",
            ),
            (
                "other-user",
                &[("HERMES_HOME", "~other/hh")],
                Some(user),
                "~other/hh",
            ),
            ("no-user-home", &[], None, ".hermes"),
            (
                "var-in-the-middle",
                &[("HERMES_HOME", "/x/$ROOT/y"), ("ROOT", "example-root")],
                Some(user),
                "/x/example-root/y",
            ),
            (
                "empty-suffix",
                &[("HERMES_DATA_DIR_SUFFIX", "")],
                Some(user),
                "/example-user/.hermes",
            ),
            (
                "unknown-dollar",
                &[("HERMES_HOME", "$NOPE/z")],
                Some(user),
                "$NOPE/z",
            ),
            (
                "unknown-braces",
                &[("HERMES_HOME", "${NOPE}/z")],
                Some(user),
                "${NOPE}/z",
            ),
        ];
        for (name, env, home, expect) in rows {
            let got = default_home_from(|key| lookup(env, key), *home);
            assert_eq!(got, Path::new(expect), "{name}");
        }
    }

    #[test]
    fn file_mode_and_owner_rules() {
        for mode in [0o600, 0o700] {
            assert!(socket_mode_ok(mode), "socket {mode:o} is private");
        }
        for mode in [0o755, 0o660, 0o606, 0o666] {
            assert!(!socket_mode_ok(mode), "socket {mode:o} is open");
        }
        for mode in [0o700, 0o750, 0o775] {
            assert!(home_mode_ok(mode), "home {mode:o} is accepted");
        }
        for mode in [0o777, 0o757, 0o702] {
            assert!(!home_mode_ok(mode), "home {mode:o} is writable by others");
        }
        assert!(peer_is_self(10, 10));
        assert!(!peer_is_self(11, 10));
    }

    #[test]
    fn scrub_removes_paths_and_control_characters() {
        let home = Path::new("/example-home");
        let cases = [
            ("see /sample/dir/file now", "see <path> now"),
            ("\"/sample/x\"", "\"<path>\""),
            ("path=/sample/x,", "path=<path>,"),
            ("(/sample/x)", "(<path>)"),
            ("failed: /sample/x: no", "failed: <path>: no"),
            ("http://127.0.0.1:9", "http://127.0.0.1:9"),
            ("/example-home/run.log", "<home>/run.log"),
            ("a\nb\tc", "a b c"),
        ];
        for (input, expect) in cases {
            assert_eq!(scrub_peer_text(input, home, 500), expect, "{input:?}");
        }
        let long = "a".repeat(600);
        assert_eq!(scrub_peer_text(&long, home, 500).chars().count(), 500);
    }

    #[test]
    fn ensure_command_removes_launch_policy_variables() {
        let removed = [
            "HERMES_MODEL",
            "HERMES_INFERENCE_MODEL",
            "HERMES_TUI_PROVIDER",
            "HERMES_INFERENCE_PROVIDER",
            "HERMES_TUI_TOOLSETS",
            "HERMES_TUI_SKILLS",
            "HERMES_CWD",
            "TERMINAL_CWD",
            "HERMES_YOLO",
            "HERMES_ACCEPT_HOOKS",
            "HERMES_TUI_CHECKPOINTS",
            "HERMES_TUI_PASS_SESSION_ID",
            "HERMES_KANBAN_BOARD",
            "HERMES_TUI_CLIENT_ENV",
        ];
        let program = Path::new("hermes");
        let home = Path::new("/example-home");
        let cmd = ensure_command(program, home);
        // Tokio 1.54 keeps the standard-library command behind `as_std`.
        let view = cmd.as_std();
        let envs: Vec<(String, Option<String>)> = view
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|item| item.to_string_lossy().into_owned()),
                )
            })
            .collect();
        for name in removed {
            let found = envs.iter().find(|(key, _)| key == name);
            assert_eq!(found.map(|(_, value)| value.clone()), Some(None), "{name}");
        }
        let home_env = envs
            .iter()
            .find(|(key, _)| key == "HERMES_HOME")
            .map(|(_, value)| value.clone());
        assert_eq!(home_env, Some(Some("/example-home".to_string())));
        let args: Vec<String> = view
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["gateway", "ensure", "--json"]);
        assert_eq!(view.get_program(), program);
    }
}
