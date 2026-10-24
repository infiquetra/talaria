//! Two-line launch failures. Notices that stay on screen are separate strings.

use hermes_client::connect::{Closed, ConnectError, HandshakeError};
use hermes_client::discovery::{DiscoveryError, EnsureError, ProfileLookupError};
use hermes_client::liveness::DialFailure;
use hermes_client::rpc::CallError;
use hermes_client::ticket::TicketError;

use crate::launch::UsageError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchFailure {
    pub what: String,
    pub next_step: String,
    pub exit_code: u8,
}

impl LaunchFailure {
    #[must_use]
    pub fn render(&self) -> String {
        format!("talaria: {}\nNext step: {}\n", self.what, self.next_step)
    }
}

impl From<UsageError> for LaunchFailure {
    fn from(value: UsageError) -> Self {
        let _ = value;
        todo!("binary")
    }
}

impl From<DiscoveryError> for LaunchFailure {
    fn from(value: DiscoveryError) -> Self {
        let _ = value;
        todo!("binary")
    }
}

impl From<EnsureError> for LaunchFailure {
    fn from(value: EnsureError) -> Self {
        let _ = value;
        todo!("binary")
    }
}

impl From<ProfileLookupError> for LaunchFailure {
    fn from(value: ProfileLookupError) -> Self {
        let _ = value;
        todo!("binary")
    }
}

impl From<TicketError> for LaunchFailure {
    fn from(value: TicketError) -> Self {
        let _ = value;
        todo!("binary")
    }
}

impl From<ConnectError> for LaunchFailure {
    fn from(value: ConnectError) -> Self {
        let _ = value;
        todo!("binary")
    }
}

impl From<HandshakeError> for LaunchFailure {
    fn from(value: HandshakeError) -> Self {
        let _ = value;
        todo!("binary")
    }
}

impl From<Closed> for LaunchFailure {
    fn from(value: Closed) -> Self {
        let _ = value;
        todo!("binary")
    }
}

impl From<CallError> for LaunchFailure {
    fn from(value: CallError) -> Self {
        let _ = value;
        todo!("binary")
    }
}

impl From<DialFailure> for LaunchFailure {
    fn from(value: DialFailure) -> Self {
        let _ = value;
        todo!("binary")
    }
}

#[must_use]
pub fn no_terminal() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn nothing_to_continue() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn resume_not_found(session_id: &str, profile: &str) -> LaunchFailure {
    let _ = (session_id, profile);
    todo!("binary")
}

#[must_use]
pub fn permission_denied() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn profile_mismatch() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn runtime_draining() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn option_not_offered(option: &str) -> LaunchFailure {
    let _ = option;
    todo!("binary")
}

#[must_use]
pub fn cwd_not_offered() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn no_session_source() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn folder_unknown() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn create_lost_twice() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn folder_unreadable() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn record_unwritable() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn record_exists() -> LaunchFailure {
    todo!("binary")
}

#[must_use]
pub fn other_refusal(reason: &str) -> LaunchFailure {
    let _ = reason;
    todo!("binary")
}

#[must_use]
pub fn voice_stop_notice() -> String {
    todo!("binary")
}

#[must_use]
pub fn interrupt_refused_notice(reason: &str) -> String {
    let _ = reason;
    todo!("binary")
}

#[must_use]
pub fn link_failure(what: &str) -> LaunchFailure {
    let _ = what;
    todo!("binary")
}

#[must_use]
pub fn refusal(reason: &str) -> LaunchFailure {
    let _ = reason;
    todo!("binary")
}

/// A refusal reason as it may be shown: the reason when it is a plain `[a-z_]` word.
#[must_use]
pub fn reason_text(reason: &str) -> &str {
    let _ = reason;
    todo!("binary")
}

/// `prompt.submit` was refused with an RPC error.
#[must_use]
pub fn prompt_refused_notice(reason: &str) -> String {
    let _ = reason;
    todo!("binary")
}

/// The connection dropped while a prompt was being sent.
#[must_use]
pub fn prompt_unknown_notice() -> String {
    todo!("binary")
}

#[cfg(test)]
mod tests {
    use super::{
        LaunchFailure, create_lost_twice, cwd_not_offered, folder_unknown, folder_unreadable,
        interrupt_refused_notice, no_session_source, nothing_to_continue, other_refusal,
        permission_denied, profile_mismatch, record_exists, resume_not_found, runtime_draining,
        voice_stop_notice,
    };
    use crate::launch::UsageError;
    use hermes_client::connect::{Closed, ConnectError, HandshakeError};
    use hermes_client::discovery::{
        DiscoveryError, EnsureError, ExitKind, NodeKind, NodeProblem, PointerProblem,
        ProfileLookupError, ReportedState,
    };
    use hermes_client::liveness::DialFailure;
    use hermes_client::rpc::{CallError, RpcError};
    use hermes_client::ticket::TicketError;
    use std::time::Duration;

    fn assert_clean(text: &str) {
        let mut token = 0usize;
        let chars: Vec<char> = text.chars().collect();
        for (index, ch) in chars.iter().copied().enumerate() {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                token += 1;
                assert!(token < 40, "long token in {text}");
            } else {
                token = 0;
            }
            if ch == '/'
                && chars
                    .get(index + 1)
                    .is_some_and(|next| next.is_ascii_alphabetic())
            {
                let rest: String = chars[index..].iter().collect();
                assert!(
                    rest.starts_with("/127.0.0.1") || text.contains("http://127.0.0.1"),
                    "path-like slash in {text}"
                );
            }
        }
        assert_eq!(
            text.bytes()
                .filter(|byte| *byte < 0x20 && *byte != b'\n')
                .count(),
            0,
            "{text}"
        );
    }

    fn check(failure: LaunchFailure) {
        let rendered = failure.render();
        assert!(rendered.starts_with("talaria: "), "{rendered}");
        assert!(rendered.contains("\nNext step: "), "{rendered}");
        assert!(rendered.ends_with('\n'), "{rendered}");
        assert_eq!(rendered.matches("talaria: ").count(), 1, "{rendered}");
        assert_clean(&rendered);
        assert!(failure.exit_code == 1 || failure.exit_code == 2);
    }

    #[test]
    fn every_failure_has_two_clean_lines() {
        for error in discovery_values() {
            check(LaunchFailure::from(error));
        }
        for error in ensure_values() {
            check(LaunchFailure::from(error));
        }
        for error in profile_values() {
            check(LaunchFailure::from(error));
        }
        for error in ticket_values() {
            check(LaunchFailure::from(error));
        }
        for error in connect_values() {
            check(LaunchFailure::from(error));
        }
        for error in handshake_values() {
            check(LaunchFailure::from(error));
        }
        for error in closed_values() {
            check(LaunchFailure::from(error));
        }
        for error in call_values() {
            check(LaunchFailure::from(error));
        }
        for error in dial_values() {
            check(LaunchFailure::from(error));
        }
        check(LaunchFailure::from(UsageError::UnknownEffort(
            "hgih".to_string(),
        )));
        check(LaunchFailure::from(UsageError::CwdNotOffered));
        check(LaunchFailure::from(UsageError::NoSessionSource));
        check(LaunchFailure::from(UsageError::OptionNotOffered("--model")));
        check(LaunchFailure::from(
            UsageError::NewSessionOptionsWithResume {
                given: vec!["--model"],
            },
        ));
    }

    fn discovery_values() -> Vec<DiscoveryError> {
        vec![
            DiscoveryError::NoGateway,
            DiscoveryError::ControlTimeout,
            DiscoveryError::UnsafeNode {
                node: NodeKind::Socket,
                problem: NodeProblem::WrongType,
            },
            DiscoveryError::BadPointer(PointerProblem::NotAbsolute),
            DiscoveryError::ReplyTooLarge,
            DiscoveryError::InvalidReply,
            DiscoveryError::ControlRefused {
                code: None,
                text: "refused".to_string(),
            },
            DiscoveryError::Incompatible,
            DiscoveryError::NotReady {
                state: ReportedState::Starting,
                idle_exit: false,
            },
            DiscoveryError::InvalidState,
            DiscoveryError::InvalidApiOrigin,
            DiscoveryError::InvalidSupervisor,
            DiscoveryError::InvalidIdentity,
            DiscoveryError::HomeNotServed,
            DiscoveryError::NamedProfileHome,
            DiscoveryError::Ensure(EnsureError::NotInstalled),
            DiscoveryError::StillUnreachableAfterEnsure,
            DiscoveryError::Io(std::io::ErrorKind::NotFound),
        ]
    }

    fn ensure_values() -> Vec<EnsureError> {
        vec![
            EnsureError::NotInstalled,
            EnsureError::TimedOut,
            EnsureError::UnreadableOutput,
            EnsureError::Exit {
                code: 2,
                kind: ExitKind::InvalidInvocation,
                state: None,
                reason_code: None,
                detail: None,
            },
        ]
    }

    fn profile_values() -> Vec<ProfileLookupError> {
        vec![
            ProfileLookupError::Parked {
                name: "fixture".to_string(),
                reason: "parked".to_string(),
            },
            ProfileLookupError::NeverDiscovered {
                name: "fixture".to_string(),
            },
        ]
    }

    fn ticket_values() -> Vec<TicketError> {
        vec![
            TicketError::StaleInstance,
            TicketError::BindingRejected,
            TicketError::NotAuthenticated,
            TicketError::CapacityExhausted,
            TicketError::InvalidRequest,
            TicketError::IdentityChanged,
            TicketError::Malformed,
            TicketError::InvalidReply,
            TicketError::ScopeMismatch,
            TicketError::Refused("no".to_string()),
            TicketError::ReplyTooLarge,
            TicketError::Timeout,
            TicketError::Io(std::io::ErrorKind::ConnectionReset),
            TicketError::UnsafeNode {
                node: NodeKind::Home,
                problem: NodeProblem::WrongOwner,
            },
        ]
    }

    fn connect_values() -> Vec<ConnectError> {
        vec![
            ConnectError::UpgradeRejected { status: Some(401) },
            ConnectError::UpgradeRejected { status: None },
            ConnectError::SubprotocolMismatch,
            ConnectError::WrongPurpose,
            ConnectError::Timeout,
            ConnectError::TlsNotSupported,
            ConnectError::Io(std::io::ErrorKind::ConnectionRefused),
        ]
    }

    fn handshake_values() -> Vec<HandshakeError> {
        vec![
            HandshakeError::ReadyTimeout,
            HandshakeError::CallTimeout {
                method: "runtime.describe",
            },
            HandshakeError::BadReady,
            HandshakeError::BadFrame,
            HandshakeError::NotSessionAuthority,
            HandshakeError::IdentityChanged,
            HandshakeError::BadDescribe,
            HandshakeError::DescribeFailed {
                code: 4001,
                reason: Some("not_found".to_string()),
            },
            HandshakeError::Closed(Closed::Dropped),
            HandshakeError::Io(std::io::ErrorKind::BrokenPipe),
        ]
    }

    fn closed_values() -> Vec<Closed> {
        vec![Closed::Overloaded, Closed::Dropped]
    }

    fn call_values() -> Vec<CallError> {
        vec![
            CallError::NotConnected,
            CallError::Encode,
            CallError::InvalidTimeout,
            CallError::Rpc(RpcError {
                code: 4001,
                message: "not_found".to_string(),
                reason: Some("not_found".to_string()),
                data: None,
            }),
            CallError::BadResult {
                method: "session.create".to_string(),
                reason: "result did not decode",
            },
            CallError::TimedOut {
                method: "session.create".to_string(),
                after: Duration::from_secs(120),
            },
            CallError::Unknown {
                method: "session.create".to_string(),
            },
        ]
    }

    fn dial_values() -> Vec<DialFailure> {
        vec![
            DialFailure::Retry {
                reason: "the gateway did not answer".to_string(),
                next_step: "Try again.".to_string(),
            },
            DialFailure::GiveUp {
                reason: "the profile is gone".to_string(),
                next_step: "Check the profile name.".to_string(),
            },
        ]
    }

    #[test]
    fn refusal_texts_by_reason() {
        let continue_missing = nothing_to_continue();
        assert_eq!(continue_missing.exit_code, 1);
        assert_eq!(
            continue_missing.render(),
            "talaria: no earlier session found for this folder.\n\
             Next step: Start a new one with talaria, or choose one with talaria --resume.\n"
        );
        let unknown_id = resume_not_found("local-session-1", "fixture");
        assert_eq!(
            unknown_id.render(),
            "talaria: no session with the id local-session-1 in the profile fixture.\n\
             Next step: Session ids belong to one profile. Check --profile, or list sessions with talaria --resume.\n"
        );
        assert_eq!(
            permission_denied().render(),
            "talaria: Hermes refused to open that session (it belongs to another user or profile).\n\
             Next step: Check --profile and the id.\n"
        );
        assert_eq!(
            profile_mismatch().render(),
            "talaria: that session belongs to a different profile.\n\
             Next step: Pass the profile it was created with using --profile.\n"
        );
        assert_eq!(
            runtime_draining().render(),
            "talaria: Hermes is restarting and cannot open sessions right now.\n\
             Next step: Wait a few seconds and run the command again.\n"
        );
        assert_eq!(
            other_refusal("admission_conflict").render(),
            "talaria: Hermes refused the request (admission_conflict).\n\
             Next step: Run the command again; if it keeps failing, report the reason.\n"
        );
        assert_eq!(
            no_session_source().render(),
            "talaria: this Hermes gateway does not offer the session source Talaria needs (tui).\n\
             Next step: Update Hermes to a release that offers it.\n"
        );
        assert_eq!(
            cwd_not_offered().render(),
            "talaria: this Hermes gateway cannot keep the working folder of a new session.\n\
             Next step: Update Hermes to a release that supports it.\n"
        );
        assert_eq!(
            folder_unknown().render(),
            "talaria: the most recent session cannot be checked against this folder.\n\
             Next step: Choose a session with talaria --resume, or start a new one with talaria.\n"
        );
        assert_eq!(
            create_lost_twice().render(),
            "talaria: the connection dropped while the session was being created; Hermes may have created an empty session.\n\
             Next step: Run the command again.\n"
        );
        assert_eq!(
            record_exists().render(),
            "talaria: the recording file already exists.\nNext step: Choose a new path for --record.\n"
        );
        assert_eq!(
            folder_unreadable().render(),
            "talaria: the current folder cannot be read.\n\
             Next step: Run talaria from a folder that exists and that you can open.\n"
        );
        assert_eq!(
            voice_stop_notice(),
            "Hermes took that text as the stop phrase of a voice chat on this session, so it was not sent as a prompt."
        );
        assert_eq!(
            interrupt_refused_notice("stale_generation"),
            "Hermes did not stop the turn (stale_generation). If it is still running, press Esc again."
        );
    }
}
