//! Privacy-safe gateway diagnostics and pure restart and view reducers.

use std::{collections::BTreeSet, time::Duration};

use serde::Serialize;
use serde_json::json;

pub const MIN_HERMES: (u64, u64, u64) = (0, 22, 3);
pub const RESTART_DEADLINE: Duration = Duration::from_secs(90);
pub const PROBE_INTERVAL: Duration = Duration::from_secs(1);
pub const FOOTER: &str = "For provider keys, tools and dependencies, run hermes doctor.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Skip,
    Info,
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    Talaria,
    Gateway,
    HermesVersion,
    Capabilities,
    Profiles,
    Attach,
    RuntimeHealth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    pub id: CheckId,
    pub severity: Severity,
    pub summary: String,
    pub details: Vec<String>,
    pub fix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub profile: String,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn overall(&self) -> Severity {
        if self.checks.iter().any(|c| c.severity == Severity::Fail) {
            Severity::Fail
        } else if self.checks.iter().any(|c| c.severity == Severity::Warn) {
            Severity::Warn
        } else {
            Severity::Ok
        }
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        self.checks
            .iter()
            .fold((0, 0, 0), |(ok, warn, fail), check| match check.severity {
                Severity::Ok => (ok + 1, warn, fail),
                Severity::Warn => (ok, warn + 1, fail),
                Severity::Fail => (ok, warn, fail + 1),
                Severity::Skip | Severity::Info => (ok, warn, fail),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub tag: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayFacts {
    Ready {
        instance_id: String,
        epoch: u64,
        launch: Option<String>,
    },
    RestartWait {
        instance_id: String,
        epoch: u64,
    },
    NotReady {
        state: String,
        drain_reason: Option<String>,
    },
    Incompatible,
    Unreachable {
        message: String,
        next_step: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityFacts {
    pub code_version: Option<String>,
    pub code_sha: Option<String>,
    pub capabilities: Vec<String>,
    pub served: Vec<String>,
    pub parked: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachFacts {
    NotTried,
    Attached {
        describe_capabilities: Vec<String>,
        identify_ms: u64,
        ticket_ms: u64,
        upgrade_ms: u64,
        ready_ms: u64,
        capabilities_ms: u64,
        describe_ms: u64,
    },
    Failed {
        message: String,
        next_step: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusPage {
    pub config_version: Option<i64>,
    pub latest_config_version: Option<i64>,
    pub heartbeat_stale_s: Option<f64>,
    pub active_agents: Option<u64>,
    pub storage_status: Option<String>,
    pub storage_reason: Option<String>,
    pub platforms_status: Option<String>,
    pub platforms_configured: Option<u64>,
    pub platforms_connected: Option<u64>,
    pub dashboard_status: Option<String>,
    pub dashboard_errors: Option<u64>,
    pub dashboard_selftest: Option<String>,
    pub standalone_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
// Keep the public facts shape shared by the service and pure diagnostics unboxed.
#[allow(clippy::large_enum_variant)]
pub enum StatusFacts {
    NotTried,
    NotReady,
    Unreadable { reason: String },
    Read(StatusPage),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoctorFacts {
    pub talaria_version: String,
    pub pin: Pin,
    pub profile: String,
    pub gateway: GatewayFacts,
    pub identity: Option<IdentityFacts>,
    pub attach: AttachFacts,
    pub status: StatusFacts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionVerdict {
    Pinned,
    SameReleaseOtherCommit,
    SameReleaseUnknownCommit,
    OlderThanPin,
    TooOld,
    NewerThanPin,
    Unknown,
}

fn version(text: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<_> = text.split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

pub fn compare_hermes_version(
    code_version: Option<&str>,
    code_sha: Option<&str>,
    pin: &Pin,
) -> VersionVerdict {
    if code_sha.is_some_and(|sha| {
        sha.len() >= 7
            && pin
                .commit
                .to_ascii_lowercase()
                .starts_with(&sha.to_ascii_lowercase())
    }) {
        return VersionVerdict::Pinned;
    }
    let Some(running) = code_version.and_then(version) else {
        return VersionVerdict::Unknown;
    };
    let Some(built) = version(pin.tag.strip_prefix('v').unwrap_or(&pin.tag)) else {
        return VersionVerdict::Unknown;
    };
    if running < MIN_HERMES {
        VersionVerdict::TooOld
    } else if running < built {
        VersionVerdict::OlderThanPin
    } else if running > built {
        VersionVerdict::NewerThanPin
    } else if code_sha.is_some() {
        VersionVerdict::SameReleaseOtherCommit
    } else {
        VersionVerdict::SameReleaseUnknownCommit
    }
}

fn path_end(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | ')' | ']' | ',' | ';')
}
fn path_start(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '(' | '[' | '=' | ':' | ',' | '<')
}

pub fn scrub_text(text: &str, home: Option<&str>) -> String {
    let mut clean: String = text.chars().map(|c| if c.is_control() || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') { '?' } else { c }).collect();
    if let Some(home) = home.filter(|home| !home.is_empty()) {
        let mut replaced = String::new();
        let mut rest = clean.as_str();
        while let Some(at) = rest.find(home) {
            replaced.push_str(&rest[..at]);
            replaced.push_str("<path>");
            let after = &rest[at + home.len()..];
            let end = after.find(path_end).unwrap_or(after.len());
            rest = &after[end..];
        }
        replaced.push_str(rest);
        clean = replaced;
    }
    let mut replaced = String::new();
    let mut chars = clean.chars().peekable();
    let mut previous = None;
    while let Some(c) = chars.next() {
        let boundary = previous.is_none_or(path_start);
        if boundary && (c == '/' || (c == '~' && chars.peek() == Some(&'/'))) {
            replaced.push_str("<path>");
            while chars.peek().is_some_and(|c| !path_end(*c)) {
                chars.next();
            }
        } else {
            replaced.push(c);
        }
        previous = Some(c);
    }
    let replaced = redact_tokens(&replaced);
    let collapsed = replaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = collapsed.chars();
    let mut result: String = chars.by_ref().take(200).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

fn short(text: &str) -> String {
    text.chars().take(8).collect()
}
fn check(id: CheckId, severity: Severity, summary: impl Into<String>, fix: Option<&str>) -> Check {
    Check {
        id,
        severity,
        summary: summary.into(),
        details: Vec::new(),
        fix: fix.map(str::to_owned),
    }
}
fn attachable(facts: &DoctorFacts) -> bool {
    matches!(
        facts.gateway,
        GatewayFacts::Ready { .. } | GatewayFacts::RestartWait { .. }
    )
}
fn same_profile(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}
fn selected_served(facts: &DoctorFacts) -> bool {
    facts
        .identity
        .as_ref()
        .is_some_and(|i| i.served.iter().any(|p| same_profile(p, &facts.profile)))
}

fn gateway_check(facts: &GatewayFacts) -> Check {
    use Severity::{Fail, Ok, Warn};
    let (severity, summary, fix) = match facts {
        GatewayFacts::Ready {
            instance_id,
            epoch,
            launch,
        } => {
            let mut summary = format!("ready · instance {} · epoch {epoch}", short(instance_id));
            if let Some(launch) = launch {
                let words = match launch.as_str() {
                    "systemd" => "systemd service".into(),
                    "launchd" => "launchd service".into(),
                    "desktop" => "started by Hermes Desktop".into(),
                    "external" => "external supervisor".into(),
                    "manual" => "started by hand".into(),
                    other => scrub_text(other, None).chars().take(20).collect::<String>(),
                };
                summary.push_str(&format!(" · {words}"));
            }
            (Ok, summary, None)
        }
        GatewayFacts::RestartWait { .. } => (
            Warn,
            "restarting: finishing the turns it already started".into(),
            Some("Wait a minute and check again."),
        ),
        GatewayFacts::NotReady {
            state,
            drain_reason,
        } => match state.as_str() {
            "starting" => (
                Warn,
                "starting".into(),
                Some("Wait a few seconds and check again."),
            ),
            "draining" if drain_reason.as_deref() == Some("idle_exit") => (
                Fail,
                "stopping because it was idle".into(),
                Some("Run hermes gateway ensure --json to start it again."),
            ),
            "draining" => (
                Fail,
                format!(
                    "shutting down ({})",
                    drain_reason.as_deref().unwrap_or("no reason given")
                ),
                Some("Wait a minute; if it does not come back, run hermes gateway ensure --json."),
            ),
            "conflict" => (
                Fail,
                "reports a conflict".into(),
                Some("Run hermes gateway status to see why."),
            ),
            "failed" => (
                Fail,
                "failed: its session service stopped".into(),
                Some("Run hermes gateway restart in a shell, then check again."),
            ),
            _ => (
                Fail,
                format!("reports an unknown state \"{state}\""),
                Some("Update Talaria and Hermes to matching versions."),
            ),
        },
        GatewayFacts::Incompatible => (
            Fail,
            "speaks an older protocol than Talaria needs".into(),
            Some("Run hermes update, then restart the gateway."),
        ),
        GatewayFacts::Unreachable { message, next_step } => {
            (Fail, message.clone(), Some(next_step.as_str()))
        }
    };
    check(CheckId::Gateway, severity, summary, fix)
}

fn version_check(facts: &DoctorFacts) -> Check {
    let Some(identity) = &facts.identity else {
        return check(
            CheckId::HermesVersion,
            Severity::Skip,
            "needs an answer from the gateway",
            None,
        );
    };
    let v = identity.code_version.as_deref().unwrap_or("unknown");
    let sha = identity
        .code_sha
        .as_deref()
        .map(short)
        .unwrap_or_else(|| "no commit".into());
    let tag = &facts.pin.tag;
    let pin = short(&facts.pin.commit);
    let ordinary_fix = format!(
        "Talaria usually works with this build. If something misbehaves, install Hermes {tag} and restart the gateway."
    );
    let unknown_fix = format!("Run hermes --version and compare it with {tag}.");
    let (severity, summary, fix) = match compare_hermes_version(
        identity.code_version.as_deref(),
        identity.code_sha.as_deref(),
        &facts.pin,
    ) {
        VersionVerdict::Pinned => (
            Severity::Ok,
            format!("{v} ({sha}), the release Talaria was built for"),
            None,
        ),
        VersionVerdict::SameReleaseOtherCommit => (
            Severity::Warn,
            format!("{v} at commit {sha}; Talaria was built for {tag} ({pin})"),
            Some(ordinary_fix.as_str()),
        ),
        VersionVerdict::SameReleaseUnknownCommit => (
            Severity::Warn,
            format!("{v}; the gateway did not report its commit"),
            Some(ordinary_fix.as_str()),
        ),
        VersionVerdict::OlderThanPin => (
            Severity::Warn,
            format!("{v}, older than {tag} that Talaria was built for"),
            Some("Run hermes update, then restart the gateway."),
        ),
        VersionVerdict::TooOld => (
            Severity::Fail,
            format!("{v}; Talaria needs 0.22.3 or later"),
            Some("Run hermes update, then restart the gateway."),
        ),
        VersionVerdict::NewerThanPin => (
            Severity::Warn,
            format!("{v}, newer than {tag} that Talaria was built for"),
            Some(
                "Newer gateways usually work; the Capabilities row shows anything missing. Report problems with both versions named.",
            ),
        ),
        VersionVerdict::Unknown => (
            Severity::Warn,
            "the gateway did not report a version Talaria can read".into(),
            Some(unknown_fix.as_str()),
        ),
    };
    let mut row = check(CheckId::HermesVersion, severity, summary, fix);
    row.details = vec![
        format!("running: {v} at {sha}"),
        format!("built for: {tag} ({pin})"),
    ];
    row
}

fn capability_check(facts: &DoctorFacts) -> Check {
    if !attachable(facts) {
        return check(
            CheckId::Capabilities,
            Severity::Skip,
            "needs a ready gateway",
            None,
        );
    }
    let required = [
        "session-authority-v1",
        "durable-admission-v1",
        "event-replay-v1",
    ];
    let gateway = facts
        .identity
        .as_ref()
        .map(|i| i.capabilities.as_slice())
        .unwrap_or_default();
    let describe = match &facts.attach {
        AttachFacts::Attached {
            describe_capabilities,
            ..
        } => Some(describe_capabilities.as_slice()),
        _ => None,
    };
    let missing: Vec<_> = required
        .iter()
        .copied()
        .filter(|name| {
            !gateway.iter().any(|value| value == name)
                || (*name != required[0]
                    && describe.is_some_and(|list| !list.iter().any(|value| value == name)))
        })
        .collect();
    let fix = format!(
        "Update Hermes to {} or later and restart its gateway.",
        facts.pin.tag
    );
    let mut row = if missing.is_empty() {
        check(
            CheckId::Capabilities,
            Severity::Ok,
            format!(
                "all {} capabilities Talaria needs are offered",
                if describe.is_some() { 5 } else { 3 }
            ),
            None,
        )
    } else {
        check(
            CheckId::Capabilities,
            Severity::Fail,
            format!("missing: {}", missing.join(", ")),
            Some(&fix),
        )
    };
    let sorted = |list: &[String]| {
        list.iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ")
    };
    row.details.push(format!("gateway: {}", sorted(gateway)));
    row.details.push(match describe {
        Some(list) => format!("session service: {}", sorted(list)),
        None => format!(
            "session service: not checked ({})",
            match &facts.attach {
                AttachFacts::Failed { message, .. } => message.as_str(),
                _ if !selected_served(facts) => "needs the profile to be served",
                _ => "not tried",
            }
        ),
    });
    let extras: BTreeSet<_> = gateway
        .iter()
        .chain(describe.unwrap_or_default())
        .filter(|s| !required.contains(&s.as_str()))
        .cloned()
        .collect();
    if !extras.is_empty() {
        row.details.push(format!(
            "also offered: {}",
            extras.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    row
}

fn served_order(profiles: &[String]) -> Vec<String> {
    let mut names = profiles.to_vec();
    names.sort_by(|a, b| (!same_profile(a, "default"), a).cmp(&(!same_profile(b, "default"), b)));
    names
}

fn profile_check(facts: &DoctorFacts) -> Check {
    let Some(identity) = facts.identity.as_ref().filter(|i| !i.served.is_empty()) else {
        return check(
            CheckId::Profiles,
            Severity::Skip,
            "needs the list of served profiles",
            None,
        );
    };
    let profile = facts.profile.trim();
    let others = identity
        .parked
        .iter()
        .filter(|(name, _)| !same_profile(name, profile))
        .count();
    let mut row = if selected_served(facts) {
        if others == 0 {
            check(
                CheckId::Profiles,
                Severity::Ok,
                format!(
                    "{profile} is served; the gateway serves {} {}",
                    identity.served.len(),
                    plural(identity.served.len() as u64, "profile", "profiles")
                ),
                None,
            )
        } else {
            check(
                CheckId::Profiles,
                Severity::Warn,
                format!(
                    "{profile} is served; {others} other {}",
                    if others == 1 {
                        "profile is parked"
                    } else {
                        "profiles are parked"
                    }
                ),
                Some("Run hermes gateway status to see why."),
            )
        }
    } else if let Some((_, reason)) = identity
        .parked
        .iter()
        .find(|(name, _)| same_profile(name, profile))
    {
        check(
            CheckId::Profiles,
            Severity::Fail,
            format!("{profile} is parked: {reason}"),
            Some("Fix the cause the gateway reports, then run hermes gateway status."),
        )
    } else {
        check(
            CheckId::Profiles,
            Severity::Fail,
            format!("the gateway never discovered a profile named {profile}"),
            Some("Create the profile, or check the name with hermes profile list."),
        )
    };
    row.details.push(format!(
        "served: {}",
        served_order(&identity.served).join(", ")
    ));
    for (name, reason) in &identity.parked {
        row.details.push(format!("parked: {name}: {reason}"));
    }
    if let StatusFacts::Read(status) = &facts.status
        && let Some(reason) = status
            .standalone_reason
            .as_ref()
            .filter(|s| !s.trim().is_empty())
    {
        row.details
            .push(format!("the gateway serves only its own profile: {reason}"));
    }
    row
}

fn attach_check(facts: &DoctorFacts) -> Check {
    if !attachable(facts) {
        return check(
            CheckId::Attach,
            Severity::Skip,
            "needs a ready gateway",
            None,
        );
    }
    if !selected_served(facts) {
        return check(
            CheckId::Attach,
            Severity::Skip,
            "needs the profile to be served",
            None,
        );
    }
    match &facts.attach {
        AttachFacts::Attached {
            identify_ms: a,
            ticket_ms: b,
            upgrade_ms: c,
            ready_ms: d,
            capabilities_ms: e,
            describe_ms: f,
            ..
        } => {
            let total = [a, b, c, d, e, f]
                .into_iter()
                .fold(0_u64, |sum, n| sum.saturating_add(*n));
            let mut row = check(
                CheckId::Attach,
                Severity::Ok,
                format!("attached to {} in {total} ms", facts.profile.trim()),
                None,
            );
            row.details.push(format!("identify {a} ms · ticket {b} ms · upgrade {c} ms · ready {d} ms · capabilities {e} ms · describe {f} ms"));
            row
        }
        AttachFacts::Failed { message, next_step } => {
            check(CheckId::Attach, Severity::Fail, message, Some(next_step))
        }
        AttachFacts::NotTried => check(CheckId::Attach, Severity::Skip, "not checked", None),
    }
}

fn runtime_check(status: &StatusFacts) -> Check {
    let page = match status {
        StatusFacts::NotTried | StatusFacts::NotReady => {
            return check(
                CheckId::RuntimeHealth,
                Severity::Skip,
                "needs a ready gateway",
                None,
            );
        }
        StatusFacts::Unreadable { reason } => {
            return check(
                CheckId::RuntimeHealth,
                Severity::Warn,
                format!("Talaria could not read the gateway's status page ({reason})"),
                Some("Run hermes gateway status."),
            );
        }
        StatusFacts::Read(page) => page,
    };
    let mut problems = Vec::new();
    let mut add = |severity, text, fix| {
        problems.push(check(CheckId::RuntimeHealth, severity, text, Some(fix)))
    };
    if page.storage_reason.as_deref() == Some("corrupt") {
        add(
            Severity::Fail,
            "session storage is corrupt".into(),
            "Run hermes doctor.",
        );
    } else if page.storage_status.as_deref().is_some_and(|s| s != "ok") {
        add(
            Severity::Warn,
            "session storage is degraded".into(),
            "Run hermes doctor.",
        );
    }
    if let Some(n) = page.heartbeat_stale_s.filter(|n| n.is_finite()) {
        add(
            Severity::Warn,
            format!(
                "the gateway's housekeeping has not run for {:.0} s",
                n.floor()
            ),
            "Restart the gateway: press R here, or run talaria doctor --restart.",
        );
    }
    if page.platforms_status.as_deref().is_some_and(|s| s != "ok")
        && page.platforms_configured.is_some_and(|n| n > 0)
    {
        add(
            Severity::Warn,
            format!(
                "{} of {} messaging platforms are connected",
                page.platforms_connected.unwrap_or(0),
                page.platforms_configured.unwrap_or(0)
            ),
            "Run hermes gateway status to see which platform failed.",
        );
    }
    if page.dashboard_status.as_deref().is_some_and(|s| s != "ok") {
        let text = if let Some(n) = page.dashboard_errors.filter(|n| *n > 0) {
            format!("the gateway's web service reported {n} recent errors")
        } else if page.dashboard_selftest.as_deref() == Some("failing") {
            "the gateway's web service is failing its self-test".into()
        } else {
            "the gateway's web service reports a problem".into()
        };
        add(
            Severity::Warn,
            text,
            "Read the gateway log with hermes logs.",
        );
    }
    if let (Some(a), Some(b)) = (page.config_version, page.latest_config_version)
        && a < b
    {
        add(
            Severity::Warn,
            format!("Hermes configuration is at version {a}; this Hermes expects {b}"),
            "Run hermes config migrate.",
        );
    }
    if problems.is_empty() {
        let agents = page
            .active_agents
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".into());
        return check(
            CheckId::RuntimeHealth,
            Severity::Ok,
            format!(
                "healthy · {agents} {} working",
                if page.active_agents == Some(1) {
                    "agent"
                } else {
                    "agents"
                }
            ),
            None,
        );
    }
    let worst = problems
        .iter()
        .position(|p| p.severity == Severity::Fail)
        .unwrap_or(0);
    let mut row = problems[worst].clone();
    for (i, problem) in problems.into_iter().enumerate() {
        if i != worst {
            row.details.push(format!(
                "also: {}. Fix: {}",
                problem.summary,
                problem.fix.as_deref().unwrap_or_default()
            ));
        }
    }
    row
}

fn output_text(text: &str) -> String {
    scrub_text(text, None)
}

fn redact_tokens(text: &str) -> String {
    let mut result = String::new();
    let mut token = String::new();
    let flush = |token: &mut String, result: &mut String| {
        if token.len() >= 40 {
            result.push_str("<redacted>");
        } else {
            result.push_str(token);
        }
        token.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
            token.push(c);
        } else {
            flush(&mut token, &mut result);
            result.push(c);
        }
    }
    flush(&mut token, &mut result);
    result
}

#[cfg(test)]
mod scrub_tests {
    use super::scrub_text;

    #[test]
    fn quoted_paths_are_redacted_without_losing_the_quotes() {
        let separator = std::path::MAIN_SEPARATOR;
        let path = format!("{separator}diagnostic-private{separator}state");
        assert_eq!(scrub_text(&format!("open `{path}`"), None), "open `<path>`");
    }

    #[test]
    fn long_tokens_are_removed_before_the_display_cut() {
        let short = "x".repeat(39);
        assert_eq!(scrub_text(&short, None), short);
        assert_eq!(scrub_text(&"x".repeat(40), None), "<redacted>");
        let text = format!("{}{}", "word ".repeat(33), "T".repeat(43));
        let scrubbed = scrub_text(&text, None);
        assert!(scrubbed.ends_with("<redacted>"));
        assert!(!scrubbed.contains('T'));
    }
}

fn scrub_check(mut check: Check) -> Check {
    check.summary = output_text(&check.summary);
    check.details = check.details.iter().map(|s| output_text(s)).collect();
    check.fix = check.fix.map(|s| output_text(&s));
    check
}

pub fn diagnose(facts: &DoctorFacts) -> Report {
    let checks = vec![
        check(
            CheckId::Talaria,
            Severity::Ok,
            format!(
                "Talaria {}, built for Hermes {} ({})",
                facts.talaria_version,
                facts.pin.tag,
                short(&facts.pin.commit)
            ),
            None,
        ),
        gateway_check(&facts.gateway),
        version_check(facts),
        capability_check(facts),
        profile_check(facts),
        attach_check(facts),
        runtime_check(&facts.status),
    ];
    Report {
        profile: output_text(facts.profile.trim()),
        checks: checks.into_iter().map(scrub_check).collect(),
    }
}

pub fn check_name(id: CheckId) -> &'static str {
    match id {
        CheckId::Talaria => "Talaria",
        CheckId::Gateway => "Gateway",
        CheckId::HermesVersion => "Hermes version",
        CheckId::Capabilities => "Capabilities",
        CheckId::Profiles => "Profiles",
        CheckId::Attach => "Attach",
        CheckId::RuntimeHealth => "Runtime health",
    }
}
pub fn severity_text(severity: Severity) -> &'static str {
    match severity {
        Severity::Skip => "skip",
        Severity::Info => "info",
        Severity::Ok => "ok",
        Severity::Warn => "warn",
        Severity::Fail => "FAIL",
    }
}
fn plural<'a>(n: u64, singular: &'a str, plural: &'a str) -> &'a str {
    if n == 1 { singular } else { plural }
}

pub fn render_text(report: &Report) -> String {
    let mut lines = vec![format!(
        "Talaria doctor (profile {})",
        output_text(&report.profile)
    )];
    for row in &report.checks {
        let row = scrub_check(row.clone());
        lines.push(format!(
            "{:<6}{:<16}{}",
            severity_text(row.severity),
            check_name(row.id),
            row.summary
        ));
        if matches!(row.severity, Severity::Warn | Severity::Fail)
            && let Some(fix) = row.fix
        {
            lines.push(format!("      Fix: {fix}"));
        }
        lines.extend(
            row.details
                .iter()
                .filter(|s| s.starts_with("also:"))
                .map(|s| format!("      {s}")),
        );
    }
    let (ok, warnings, failures) = report.counts();
    lines.push(format!(
        "Result: {ok} ok, {warnings} {}, {failures} {}.",
        plural(warnings as u64, "warning", "warnings"),
        plural(failures as u64, "failure", "failures")
    ));
    lines.push(FOOTER.into());
    format!("{}\n", lines.join("\n"))
}

pub fn render_json(
    report: &Report,
    restart: Option<&RestartOutcome>,
    pin: &Pin,
    talaria_version: &str,
) -> String {
    let restart = restart.map(|outcome| {
        let (name, seconds, status) = match outcome {
            RestartOutcome::Restarted { seconds, .. } => ("restarted", Some(*seconds), None),
            RestartOutcome::Refused { http_status, .. } => ("refused", None, *http_status),
            RestartOutcome::Changed => ("changed", None, None), RestartOutcome::DidNotRestart => ("did_not_restart", None, None),
            RestartOutcome::NotBack => ("not_back", None, None), RestartOutcome::NotReady => ("not_ready", None, None),
            RestartOutcome::Declined => ("declined", None, None), RestartOutcome::NeedsConfirmation => ("needs_confirmation", None, None),
        };
        json!({"outcome": name, "seconds": seconds, "http_status": status, "text": outcome_text(outcome)})
    });
    let checks: Vec<_> = report.checks.iter().cloned().map(scrub_check).collect();
    format!(
        "{}\n",
        json!({"format": "talaria-doctor-v1", "overall": report.overall(), "profile": output_text(&report.profile), "talaria": {"version": output_text(talaria_version), "hermes_tag": output_text(&pin.tag), "hermes_commit": if pin.commit == hermes_contract::pinned::COMMIT { pin.commit.clone() } else { output_text(&pin.commit) }}, "checks": checks, "restart": restart})
    )
}

pub fn exit_code(report: &Report, restart: Option<&RestartOutcome>) -> i32 {
    match restart {
        Some(RestartOutcome::NeedsConfirmation) => 3,
        Some(RestartOutcome::Declined) => 4,
        Some(RestartOutcome::Restarted { .. }) | None => {
            i32::from(report.overall() == Severity::Fail)
        }
        Some(_) => 1,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkSummary {
    pub chat: u64,
    pub other: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RestartState {
    #[default]
    Idle,
    Asking {
        old_instance: String,
        profiles: Vec<String>,
    },
    Confirming {
        old_instance: String,
        profiles: Vec<String>,
        work: Option<WorkSummary>,
        restart_selected: bool,
    },
    Requesting {
        old_instance: String,
    },
    Waiting {
        old_instance: String,
        elapsed: Duration,
        unknown_request: bool,
    },
    Done(RestartOutcome),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartOutcome {
    Restarted {
        seconds: u64,
        old_instance: String,
        new_instance: String,
    },
    Refused {
        http_status: Option<u16>,
        detail: String,
    },
    Changed,
    DidNotRestart,
    NotBack,
    NotReady,
    Declined,
    NeedsConfirmation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartInput {
    Ask {
        gateway_ready: bool,
        instance_id: String,
        profiles: Vec<String>,
    },
    ActiveWork(Result<WorkSummary, ()>),
    Toggle,
    Confirm,
    Cancel,
    Posted(PostResult),
    Tick(Duration),
    Probe(ProbeResult),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostResult {
    Accepted,
    Refused {
        http_status: Option<u16>,
        detail: String,
    },
    Unknown,
    Changed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeResult {
    Ready { instance_id: String },
    NotAnswering,
    NotReady,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartEffect {
    QueryActiveWork,
    Post { old_instance: String },
    ProbeAfter(Duration),
    RerunChecks,
    Notice(String),
}

pub fn outcome_text(outcome: &RestartOutcome) -> String {
    match outcome {
        RestartOutcome::Restarted {
            seconds,
            old_instance,
            new_instance,
        } => format!(
            "Gateway restarted in {seconds} s (instance {} → {}).",
            scrub_text(&short(old_instance), None),
            scrub_text(&short(new_instance), None)
        ),
        RestartOutcome::Refused {
            http_status,
            detail,
        } => {
            let detail = output_text(detail);
            match http_status {
                Some(status) if !detail.is_empty() => {
                    format!("The gateway refused the restart (HTTP {status}: {detail}).")
                }
                Some(status) => format!("The gateway refused the restart (HTTP {status})."),
                None => format!("The restart request failed: {detail}."),
            }
        }
        RestartOutcome::Changed => {
            "The gateway changed while the confirmation was open, so nothing was sent.".into()
        }
        RestartOutcome::DidNotRestart => "The gateway did not restart within 90 s.".into(),
        RestartOutcome::NotBack => "The gateway did not come back within 90 s.".into(),
        RestartOutcome::NotReady => {
            "The gateway is not ready, so Talaria cannot ask it to restart.".into()
        }
        RestartOutcome::Declined => "Not restarted.".into(),
        RestartOutcome::NeedsConfirmation => {
            "Refusing to restart without confirmation: run in a terminal or add --yes.".into()
        }
    }
}

pub fn outcome_fix(outcome: &RestartOutcome) -> Option<&'static str> {
    match outcome {
        RestartOutcome::Refused { .. } | RestartOutcome::DidNotRestart => {
            Some("Run hermes gateway restart in a shell and read its output.")
        }
        RestartOutcome::Changed => Some("Check again, then restart if it is still needed."),
        RestartOutcome::NotBack => Some(
            "Run hermes gateway status; if the gateway is stopped, run hermes gateway ensure --json.",
        ),
        RestartOutcome::NotReady => Some("Run hermes gateway restart in a shell."),
        RestartOutcome::Restarted { .. }
        | RestartOutcome::Declined
        | RestartOutcome::NeedsConfirmation => None,
    }
}

pub fn confirmation_lines(profiles: &[String], work: Option<&WorkSummary>) -> Vec<String> {
    let profiles = served_order(profiles);
    let mut names = profiles
        .iter()
        .take(5)
        .map(|s| output_text(s))
        .collect::<Vec<_>>();
    if profiles.len() > 5 {
        names.push(format!("and {} more", profiles.len() - 5));
    }
    let serving = format!(
        "It serves {} {}: {}. Every session on {} reconnects.",
        profiles.len(),
        plural(profiles.len() as u64, "profile", "profiles"),
        names.join(", "),
        if profiles.len() == 1 { "it" } else { "them" }
    );
    let jobs = match work {
        None => "Talaria could not find out what is running; anything running will be interrupted."
            .into(),
        Some(work) => {
            let total = work.chat.saturating_add(work.other);
            if total == 0 {
                "Nothing is running right now.".into()
            } else {
                format!(
                    "{total} running {} will be interrupted: {} {} and {} {}.",
                    plural(total, "job", "jobs"),
                    work.chat,
                    plural(work.chat, "chat turn", "chat turns"),
                    work.other,
                    plural(work.other, "other job", "other jobs")
                )
            }
        }
    };
    vec!["Restart the Hermes gateway?".into(), serving, jobs]
}

fn finished(outcome: RestartOutcome, rerun: bool) -> (RestartState, Vec<RestartEffect>) {
    let mut effects = Vec::new();
    if rerun {
        effects.push(RestartEffect::RerunChecks);
    }
    effects.push(RestartEffect::Notice(outcome_text(&outcome)));
    (RestartState::Done(outcome), effects)
}

pub fn restart_update(
    state: RestartState,
    input: RestartInput,
) -> (RestartState, Vec<RestartEffect>) {
    match (state, input) {
        (
            RestartState::Idle | RestartState::Done(_),
            RestartInput::Ask {
                gateway_ready,
                instance_id,
                profiles,
            },
        ) => {
            if gateway_ready {
                (
                    RestartState::Asking {
                        old_instance: instance_id,
                        profiles,
                    },
                    vec![RestartEffect::QueryActiveWork],
                )
            } else {
                (RestartState::Done(RestartOutcome::NotReady), vec![])
            }
        }
        (
            RestartState::Asking {
                old_instance,
                profiles,
            },
            RestartInput::ActiveWork(work),
        ) => (
            RestartState::Confirming {
                old_instance,
                profiles,
                work: work.ok(),
                restart_selected: false,
            },
            vec![],
        ),
        (RestartState::Asking { .. } | RestartState::Confirming { .. }, RestartInput::Cancel) => {
            (RestartState::Done(RestartOutcome::Declined), vec![])
        }
        (
            RestartState::Confirming {
                old_instance,
                profiles,
                work,
                restart_selected,
            },
            RestartInput::Toggle,
        ) => (
            RestartState::Confirming {
                old_instance,
                profiles,
                work,
                restart_selected: !restart_selected,
            },
            vec![],
        ),
        (RestartState::Confirming { old_instance, .. }, RestartInput::Confirm) => (
            RestartState::Requesting {
                old_instance: old_instance.clone(),
            },
            vec![RestartEffect::Post { old_instance }],
        ),
        (
            RestartState::Requesting { old_instance },
            RestartInput::Posted(result @ (PostResult::Accepted | PostResult::Unknown)),
        ) => (
            RestartState::Waiting {
                old_instance,
                elapsed: Duration::ZERO,
                unknown_request: result == PostResult::Unknown,
            },
            vec![RestartEffect::ProbeAfter(PROBE_INTERVAL)],
        ),
        (
            RestartState::Requesting { .. },
            RestartInput::Posted(PostResult::Refused {
                http_status,
                detail,
            }),
        ) => finished(
            RestartOutcome::Refused {
                http_status,
                detail: scrub_text(&detail, None),
            },
            false,
        ),
        (RestartState::Requesting { .. }, RestartInput::Posted(PostResult::Changed)) => {
            finished(RestartOutcome::Changed, false)
        }
        (
            RestartState::Waiting {
                old_instance,
                elapsed: _,
                unknown_request,
            },
            RestartInput::Tick(elapsed),
        ) => (
            RestartState::Waiting {
                old_instance,
                elapsed,
                unknown_request,
            },
            vec![],
        ),
        (
            RestartState::Waiting {
                old_instance,
                elapsed,
                unknown_request,
            },
            RestartInput::Probe(probe),
        ) => {
            if let ProbeResult::Ready { instance_id } = &probe
                && instance_id != &old_instance
            {
                return finished(
                    RestartOutcome::Restarted {
                        seconds: elapsed
                            .as_secs()
                            .saturating_add(u64::from(elapsed.subsec_nanos() != 0)),
                        old_instance,
                        new_instance: instance_id.clone(),
                    },
                    true,
                );
            }
            if elapsed >= RESTART_DEADLINE {
                finished(
                    if matches!(probe, ProbeResult::Ready { .. }) {
                        RestartOutcome::DidNotRestart
                    } else {
                        RestartOutcome::NotBack
                    },
                    true,
                )
            } else {
                (
                    RestartState::Waiting {
                        old_instance,
                        elapsed,
                        unknown_request,
                    },
                    vec![RestartEffect::ProbeAfter(PROBE_INTERVAL)],
                )
            }
        }
        (state, _) => (state, vec![]),
    }
}

pub fn ask_input(facts: &DoctorFacts) -> RestartInput {
    let (gateway_ready, instance_id) = match &facts.gateway {
        GatewayFacts::Ready { instance_id, .. } => (true, instance_id.clone()),
        _ => (false, String::new()),
    };
    RestartInput::Ask {
        gateway_ready,
        instance_id,
        profiles: facts
            .identity
            .as_ref()
            .map(|i| i.served.clone())
            .unwrap_or_default(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoctorView {
    pub report: Option<Report>,
    pub facts: Option<DoctorFacts>,
    pub running: bool,
    pub selected: usize,
    pub expanded: BTreeSet<CheckId>,
}
impl DoctorView {
    pub fn new() -> Self {
        Self {
            report: None,
            facts: None,
            running: true,
            selected: 0,
            expanded: BTreeSet::new(),
        }
    }
}
impl Default for DoctorView {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorKey {
    Up,
    Down,
    Toggle,
    Rerun,
    Restart,
    Close,
    Yes,
    No,
    Tab,
    Left,
    Right,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorEffect {
    Gather,
    AskRestart,
    Restart(RestartInput),
    Close,
}

pub fn doctor_view_update(
    view: &mut DoctorView,
    restart: &RestartState,
    key: DoctorKey,
) -> Vec<DoctorEffect> {
    match restart {
        RestartState::Asking { .. } => {
            if matches!(key, DoctorKey::Close | DoctorKey::No) {
                vec![DoctorEffect::Restart(RestartInput::Cancel)]
            } else {
                vec![]
            }
        }
        RestartState::Confirming {
            restart_selected, ..
        } => match key {
            DoctorKey::Tab | DoctorKey::Left | DoctorKey::Right => {
                vec![DoctorEffect::Restart(RestartInput::Toggle)]
            }
            DoctorKey::Yes => vec![DoctorEffect::Restart(RestartInput::Confirm)],
            DoctorKey::No | DoctorKey::Close => vec![DoctorEffect::Restart(RestartInput::Cancel)],
            DoctorKey::Toggle => vec![DoctorEffect::Restart(if *restart_selected {
                RestartInput::Confirm
            } else {
                RestartInput::Cancel
            })],
            _ => vec![],
        },
        RestartState::Requesting { .. } | RestartState::Waiting { .. } => {
            if key == DoctorKey::Close {
                vec![DoctorEffect::Close]
            } else {
                vec![]
            }
        }
        RestartState::Idle | RestartState::Done(_) => {
            match key {
                DoctorKey::Up => view.selected = view.selected.saturating_sub(1),
                DoctorKey::Down => view.selected = view.selected.saturating_add(1).min(6),
                DoctorKey::Toggle => {
                    if let Some(row) = view
                        .report
                        .as_ref()
                        .and_then(|r| r.checks.get(view.selected))
                        && !view.expanded.remove(&row.id)
                    {
                        view.expanded.insert(row.id);
                    }
                }
                DoctorKey::Rerun if !view.running => {
                    return vec![DoctorEffect::Gather];
                }
                DoctorKey::Restart if !view.running => return vec![DoctorEffect::AskRestart],
                DoctorKey::Close => return vec![DoctorEffect::Close],
                _ => {}
            }
            vec![]
        }
    }
}
