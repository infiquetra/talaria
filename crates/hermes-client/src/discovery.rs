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

use std::path::{Path, PathBuf};
use std::time::Duration;

/// A gateway Talaria can attach to. Fields stay private so a path cannot leak
/// through a public field. [`Debug`] prints no path and only the first eight
/// characters of an id.
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
        let _ = (result, home);
        todo!("P1-01")
    }

    /// Return the profile id for a typed name, or say why it is not usable.
    pub fn resolve_profile(&self, name: &str) -> Result<ProfileId, ProfileLookupError> {
        let _ = name;
        todo!("P1-01")
    }

    /// Copy this endpoint with a different instance id. Live test 32 uses it.
    #[doc(hidden)]
    pub fn with_instance_id(&self, id: &str) -> Endpoint {
        let _ = id;
        todo!("P1-01")
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
    let _ = canonical_home;
    let _ = std::any::type_name::<sha2::Sha256>();
    todo!("P1-01")
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
                "Create the profile, or check the name with `hermes profile list`. (Talaria does not create profiles.)"
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

#[derive(Debug)]
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

impl std::error::Error for EnsureError {}

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

#[derive(Debug)]
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
    Ensure(EnsureError),
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

impl std::error::Error for DiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Ensure(err) => Some(err),
            _ => None,
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
/// The pure rules live in [`default_home_from`]. That function is still a
/// stub, and the live helper must reach `discover_with` before any skip
/// decision, so this wrapper uses the same rules through a private copy until
/// the stub is replaced.
pub fn default_home() -> PathBuf {
    let user_home = std::env::var_os("HOME").map(PathBuf::from);
    default_home_ready(|key| std::env::var(key).ok(), user_home.as_deref())
}

pub(crate) fn default_home_from(
    env: impl Fn(&str) -> Option<String>,
    user_home: Option<&Path>,
) -> PathBuf {
    let _ = (env, user_home);
    todo!("P1-01")
}

fn default_home_ready(env: impl Fn(&str) -> Option<String>, user_home: Option<&Path>) -> PathBuf {
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
    let _ = (home, options);
    todo!("P1-01")
}

/// Group write on a home is accepted. Only write permission for others is refused.
pub(crate) fn home_mode_ok(mode: u32) -> bool {
    let _ = mode;
    todo!("P1-01")
}

/// A socket is private when no group or other permission bit is set.
pub(crate) fn socket_mode_ok(mode: u32) -> bool {
    let _ = mode;
    todo!("P1-01")
}

/// True when the connected peer is this user. The only check that proves who is listening.
pub(crate) fn peer_is_self(peer_uid: u32, my_uid: u32) -> bool {
    let _ = (peer_uid, my_uid);
    todo!("P1-01")
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
    let _ = (text, home, limit);
    todo!("P1-01")
}

/// Build the ensure command. The caller runs it. Standard input is closed by the runner.
pub(crate) fn ensure_command(program: &Path, canonical_home: &Path) -> tokio::process::Command {
    let _ = (program, canonical_home);
    todo!("P1-01")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(pairs: &'a [(&str, &str)], key: &str) -> Option<String> {
        pairs
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| (*value).to_string())
    }

    #[test]
    fn default_home_from_rules() {
        let user = Path::new("/example-user");
        let rows: &[(&str, &[(&str, &str)], Option<&Path>, &str)] = &[
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
