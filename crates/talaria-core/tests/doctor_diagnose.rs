use proptest::prelude::*;
use talaria_core::doctor::*;

fn pin() -> Pin {
    Pin {
        tag: hermes_contract::pinned::TAG.into(),
        commit: hermes_contract::pinned::COMMIT.into(),
    }
}
fn status() -> StatusPage {
    StatusPage {
        config_version: Some(23),
        latest_config_version: Some(23),
        heartbeat_stale_s: None,
        active_agents: Some(0),
        storage_status: Some("ok".into()),
        storage_reason: None,
        platforms_status: Some("ok".into()),
        platforms_configured: Some(0),
        platforms_connected: Some(0),
        dashboard_status: Some("ok".into()),
        dashboard_errors: Some(0),
        dashboard_selftest: Some("passing".into()),
        standalone_reason: None,
    }
}
fn ready() -> DoctorFacts {
    DoctorFacts {
        talaria_version: "0.7.0".into(),
        pin: pin(),
        profile: "fixture".into(),
        gateway: GatewayFacts::Ready {
            instance_id: "9f2c41aa0123456789abcdef01234567".into(),
            epoch: 3,
            launch: Some("manual".into()),
        },
        identity: Some(IdentityFacts {
            code_version: Some("0.22.4".into()),
            code_sha: Some(hermes_contract::pinned::COMMIT.into()),
            capabilities: vec![
                "session-authority-v1".into(),
                "durable-admission-v1".into(),
                "event-replay-v1".into(),
            ],
            served: vec!["default".into(), "fixture".into()],
            parked: vec![],
        }),
        attach: AttachFacts::Attached {
            describe_capabilities: vec!["durable-admission-v1".into(), "event-replay-v1".into()],
            identify_ms: 2,
            ticket_ms: 1,
            upgrade_ms: 18,
            ready_ms: 5,
            capabilities_ms: 3,
            describe_ms: 7,
        },
        status: StatusFacts::Read(status()),
    }
}
fn row(facts: &DoctorFacts, id: CheckId) -> Check {
    diagnose(facts)
        .checks
        .into_iter()
        .find(|c| c.id == id)
        .expect("required check")
}
fn assert_row(
    facts: &DoctorFacts,
    id: CheckId,
    severity: Severity,
    summary: &str,
    fix: Option<&str>,
) {
    let c = row(facts, id);
    assert_eq!(c.severity, severity);
    assert_eq!(c.summary, summary);
    assert_eq!(c.fix.as_deref(), fix);
}

#[test]
fn version_verdicts() {
    let p = pin();
    let upper = p.commit[..7].to_uppercase();
    let six = &p.commit[..6];
    for (version, sha, expected) in [
        (
            Some("0.22.4"),
            Some(p.commit.as_str()),
            VersionVerdict::Pinned,
        ),
        (Some("0.22.4"), Some(upper.as_str()), VersionVerdict::Pinned),
        (
            Some("0.22.4"),
            Some(six),
            VersionVerdict::SameReleaseOtherCommit,
        ),
        (
            Some("0.22.4"),
            Some("ffffffff"),
            VersionVerdict::SameReleaseOtherCommit,
        ),
        (
            Some("0.22.4"),
            None,
            VersionVerdict::SameReleaseUnknownCommit,
        ),
        (
            Some("0.22.3"),
            Some("ffffffff"),
            VersionVerdict::OlderThanPin,
        ),
        (Some("0.22.2"), None, VersionVerdict::TooOld),
        (Some("0.21.9"), None, VersionVerdict::TooOld),
        (Some("0.22.10"), None, VersionVerdict::NewerThanPin),
        (Some("1.0.0"), None, VersionVerdict::NewerThanPin),
        (Some("unknown"), None, VersionVerdict::Unknown),
        (None, None, VersionVerdict::Unknown),
        (Some("0.22"), None, VersionVerdict::Unknown),
        (Some("v0.22.4"), None, VersionVerdict::Unknown),
        (Some("0.22.4+3"), None, VersionVerdict::Unknown),
        (Some(""), None, VersionVerdict::Unknown),
        (Some("0.22.x"), None, VersionVerdict::Unknown),
        (
            Some("unknown"),
            Some(p.commit.as_str()),
            VersionVerdict::Pinned,
        ),
    ] {
        assert_eq!(
            compare_hermes_version(version, sha, &p),
            expected,
            "{version:?} {sha:?}"
        );
    }
    let r = row(&ready(), CheckId::HermesVersion);
    assert_eq!(
        r.details,
        vec![
            "running: 0.22.4 at 4d043038",
            "built for: v0.22.4 (4d043038)"
        ]
    );
}

#[test]
fn gateway_rows() {
    let cases = vec![
        (GatewayFacts::Ready { instance_id:"9f2c41aa0123456789abcdef01234567".into(), epoch:3, launch:Some("manual".into()) }, Severity::Ok,"ready · instance 9f2c41aa · epoch 3 · started by hand",None),
        (GatewayFacts::RestartWait { instance_id:"abc".into(), epoch:3 },Severity::Warn,"restarting: finishing the turns it already started",Some("Wait a minute and check again.")),
        (GatewayFacts::NotReady { state:"starting".into(), drain_reason:None },Severity::Warn,"starting",Some("Wait a few seconds and check again.")),
        (GatewayFacts::NotReady { state:"draining".into(), drain_reason:Some("idle_exit".into()) },Severity::Fail,"stopping because it was idle",Some("Run hermes gateway ensure --json to start it again.")),
        (GatewayFacts::NotReady { state:"draining".into(), drain_reason:None },Severity::Fail,"shutting down (no reason given)",Some("Wait a minute; if it does not come back, run hermes gateway ensure --json.")),
        (GatewayFacts::NotReady { state:"conflict".into(), drain_reason:None },Severity::Fail,"reports a conflict",Some("Run hermes gateway status to see why.")),
        (GatewayFacts::NotReady { state:"failed".into(), drain_reason:None },Severity::Fail,"failed: its session service stopped",Some("Run hermes gateway restart in a shell, then check again.")),
        (GatewayFacts::NotReady { state:"odd".into(), drain_reason:None },Severity::Fail,"reports an unknown state \"odd\"",Some("Update Talaria and Hermes to matching versions.")),
        (GatewayFacts::Incompatible,Severity::Fail,"speaks an older protocol than Talaria needs",Some("Run hermes update, then restart the gateway.")),
        (GatewayFacts::Unreachable { message:"no Hermes gateway is running for this home".into(), next_step:"Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.".into() },Severity::Fail,"no Hermes gateway is running for this home",Some("Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.")),
    ];
    for (gateway, severity, summary, fix) in cases {
        let mut f = ready();
        f.gateway = gateway;
        assert_row(&f, CheckId::Gateway, severity, summary, fix);
    }
}

#[test]
fn capability_rows() {
    let mut f = ready();
    assert_row(
        &f,
        CheckId::Capabilities,
        Severity::Ok,
        "all 5 capabilities Talaria needs are offered",
        None,
    );
    if let AttachFacts::Attached {
        describe_capabilities,
        ..
    } = &mut f.attach
    {
        describe_capabilities.retain(|s| s != "durable-admission-v1");
    }
    assert_row(
        &f,
        CheckId::Capabilities,
        Severity::Fail,
        "missing: durable-admission-v1",
        Some("Update Hermes to v0.22.4 or later and restart its gateway."),
    );
    let identity = f.identity.as_mut().unwrap();
    identity
        .capabilities
        .retain(|s| s != "event-replay-v1" && s != "session-authority-v1");
    if let AttachFacts::Attached {
        describe_capabilities,
        ..
    } = &mut f.attach
    {
        describe_capabilities.retain(|s| s != "event-replay-v1");
    }
    assert_eq!(
        row(&f, CheckId::Capabilities).summary,
        "missing: session-authority-v1, durable-admission-v1, event-replay-v1"
    );
    f = ready();
    f.attach = AttachFacts::Failed {
        message: "not connected".into(),
        next_step: "retry".into(),
    };
    let c = row(&f, CheckId::Capabilities);
    assert_eq!(c.summary, "all 3 capabilities Talaria needs are offered");
    assert!(
        c.details
            .iter()
            .any(|s| s.starts_with("session service: not checked ("))
    );
    f = ready();
    f.identity
        .as_mut()
        .unwrap()
        .capabilities
        .push("local-cli-create-v1".into());
    assert!(
        row(&f, CheckId::Capabilities)
            .details
            .iter()
            .any(|s| s.contains("also offered: local-cli-create-v1"))
    );
    f.gateway = GatewayFacts::NotReady {
        state: "starting".into(),
        drain_reason: None,
    };
    assert_row(
        &f,
        CheckId::Capabilities,
        Severity::Skip,
        "needs a ready gateway",
        None,
    );
}

#[test]
fn profile_rows() {
    let mut f = ready();
    assert_row(
        &f,
        CheckId::Profiles,
        Severity::Ok,
        "fixture is served; the gateway serves 2 profiles",
        None,
    );
    f.profile = " Fixture ".into();
    assert_eq!(row(&f, CheckId::Profiles).severity, Severity::Ok);
    f.profile = "fixture".into();
    f.identity
        .as_mut()
        .unwrap()
        .served
        .retain(|name| name != "default");
    f.identity.as_mut().unwrap().parked.push((
        "default".into(),
        "<path> did not open: database is locked".into(),
    ));
    assert_row(
        &f,
        CheckId::Profiles,
        Severity::Warn,
        "fixture is served; 1 other profile is parked",
        Some("Run hermes gateway status to see why."),
    );
    f.profile = "default".into();
    assert_row(
        &f,
        CheckId::Profiles,
        Severity::Fail,
        "default is parked: <path> did not open: database is locked",
        Some("Fix the cause the gateway reports, then run hermes gateway status."),
    );
    f.profile = "fixture".into();
    f.identity.as_mut().unwrap().served = vec!["default".into()];
    f.identity.as_mut().unwrap().parked.clear();
    assert_row(
        &f,
        CheckId::Profiles,
        Severity::Fail,
        "the gateway never discovered a profile named fixture",
        Some("Create the profile, or check the name with hermes profile list."),
    );
    f.identity = None;
    assert_row(
        &f,
        CheckId::Profiles,
        Severity::Skip,
        "needs the list of served profiles",
        None,
    );
    f = ready();
    f.identity.as_mut().unwrap().served.clear();
    assert_row(
        &f,
        CheckId::Profiles,
        Severity::Skip,
        "needs the list of served profiles",
        None,
    );
}

#[test]
fn attach_rows() {
    let mut f = ready();
    let c = row(&f, CheckId::Attach);
    assert_eq!(c.severity, Severity::Ok);
    assert_eq!(c.summary, "attached to fixture in 36 ms");
    assert_eq!(
        c.details,
        vec![
            "identify 2 ms · ticket 1 ms · upgrade 18 ms · ready 5 ms · capabilities 3 ms · describe 7 ms"
        ]
    );
    f.gateway = GatewayFacts::NotReady {
        state: "starting".into(),
        drain_reason: None,
    };
    assert_row(
        &f,
        CheckId::Attach,
        Severity::Skip,
        "needs a ready gateway",
        None,
    );
    f = ready();
    f.identity.as_mut().unwrap().served = vec!["default".into()];
    assert_row(
        &f,
        CheckId::Attach,
        Severity::Skip,
        "needs the profile to be served",
        None,
    );
    f = ready();
    f.attach = AttachFacts::Failed {
        message: "connection failed".into(),
        next_step: "Try again.".into(),
    };
    assert_row(
        &f,
        CheckId::Attach,
        Severity::Fail,
        "connection failed",
        Some("Try again."),
    );
}

#[test]
fn runtime_health_rows() {
    let mut f = ready();
    assert_row(
        &f,
        CheckId::RuntimeHealth,
        Severity::Ok,
        "healthy · 0 agents working",
        None,
    );
    let mut s = status();
    s.storage_reason = Some("corrupt".into());
    f.status = StatusFacts::Read(s);
    assert_row(
        &f,
        CheckId::RuntimeHealth,
        Severity::Fail,
        "session storage is corrupt",
        Some("Run hermes doctor."),
    );
    let mut s = status();
    s.storage_status = Some("degraded".into());
    f.status = StatusFacts::Read(s);
    assert_row(
        &f,
        CheckId::RuntimeHealth,
        Severity::Warn,
        "session storage is degraded",
        Some("Run hermes doctor."),
    );
    let mut s = status();
    s.platforms_status = Some("degraded".into());
    s.platforms_configured = Some(2);
    s.platforms_connected = Some(1);
    f.status = StatusFacts::Read(s);
    assert_row(
        &f,
        CheckId::RuntimeHealth,
        Severity::Warn,
        "1 of 2 messaging platforms are connected",
        Some("Run hermes gateway status to see which platform failed."),
    );
    let mut s = status();
    s.dashboard_status = Some("degraded".into());
    s.dashboard_errors = Some(3);
    f.status = StatusFacts::Read(s);
    assert_row(
        &f,
        CheckId::RuntimeHealth,
        Severity::Warn,
        "the gateway's web service reported 3 recent errors",
        Some("Read the gateway log with hermes logs."),
    );
    let mut s = status();
    s.dashboard_status = Some("degraded".into());
    s.dashboard_selftest = Some("failing".into());
    f.status = StatusFacts::Read(s);
    assert_row(
        &f,
        CheckId::RuntimeHealth,
        Severity::Warn,
        "the gateway's web service is failing its self-test",
        Some("Read the gateway log with hermes logs."),
    );
    let mut s = status();
    s.heartbeat_stale_s = Some(125.7);
    s.config_version = Some(22);
    f.status = StatusFacts::Read(s);
    let c = row(&f, CheckId::RuntimeHealth);
    assert_eq!(c.severity, Severity::Warn);
    assert_eq!(
        c.summary,
        "the gateway's housekeeping has not run for 125 s"
    );
    assert_eq!(
        c.fix.as_deref(),
        Some("Restart the gateway: press R here, or run talaria doctor --restart.")
    );
    assert!(c.details.contains(&"also: Hermes configuration is at version 22; this Hermes expects 23. Fix: Run hermes config migrate.".to_owned()));
    let mut s = status();
    s.config_version = Some(22);
    f.status = StatusFacts::Read(s);
    assert_row(
        &f,
        CheckId::RuntimeHealth,
        Severity::Warn,
        "Hermes configuration is at version 22; this Hermes expects 23",
        Some("Run hermes config migrate."),
    );
    for reason in [
        "HTTP 500",
        "timed out after 8 s",
        "not JSON",
        "connection failed",
    ] {
        f.status = StatusFacts::Unreadable {
            reason: reason.into(),
        };
        assert_row(
            &f,
            CheckId::RuntimeHealth,
            Severity::Warn,
            &format!("Talaria could not read the gateway's status page ({reason})"),
            Some("Run hermes gateway status."),
        );
    }
    f.status = StatusFacts::NotReady;
    assert_row(
        &f,
        CheckId::RuntimeHealth,
        Severity::Skip,
        "needs a ready gateway",
        None,
    );
}

fn report_from_severities(severities: &[Severity]) -> Report {
    let ids = [
        CheckId::Talaria,
        CheckId::Gateway,
        CheckId::HermesVersion,
        CheckId::Capabilities,
        CheckId::Profiles,
        CheckId::Attach,
        CheckId::RuntimeHealth,
    ];
    Report {
        profile: "fixture".into(),
        checks: ids
            .into_iter()
            .zip(severities.iter().copied())
            .map(|(id, severity)| Check {
                id,
                severity,
                summary: "summary".into(),
                details: vec![],
                fix: None,
            })
            .collect(),
    }
}
proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn overall_is_the_worst_row(values in prop::collection::vec(0u8..5, 7)) {
        let severities:Vec<_>=values.into_iter().map(|n| match n {0=>Severity::Skip,1=>Severity::Info,2=>Severity::Ok,3=>Severity::Warn,_=>Severity::Fail}).collect();
        let report=report_from_severities(&severities);
        let expected=if severities.contains(&Severity::Fail) { Severity::Fail } else if severities.contains(&Severity::Warn) { Severity::Warn } else { Severity::Ok };
        prop_assert_eq!(report.overall(),expected);
        prop_assert_eq!(report.counts(),(severities.iter().filter(|x| **x==Severity::Ok).count(),severities.iter().filter(|x| **x==Severity::Warn).count(),severities.iter().filter(|x| **x==Severity::Fail).count()));
    }
    #[test]
    fn scrub_text_never_leaks_paths(word in "[a-z]{1,20}", separator in prop::sample::select(vec!["", ":", "=", "(", "[", ",", "\"", "'"]), control in prop::sample::select(vec!['\u{0000}','\u{001f}','\u{007f}','\u{009f}','\u{061c}','\u{200e}','\u{202e}','\u{2069}'])) {
        let temp=tempfile::tempdir().unwrap();
        for name in ["hh", "t a b/hh"] {
            let home=temp.path().join(name); let home=home.to_string_lossy(); let path=format!("{home}/state.db"); let input=format!("{word} {separator}{path} {control} {}‏","a".repeat(240));
            let out=scrub_text(&input,Some(&home));
            prop_assert!(!out.contains(home.as_ref())); prop_assert!(!out.contains("state.db")); prop_assert!(!out.contains("a b/hh")); prop_assert!(!out.contains(control)); prop_assert!(!out.contains(char::from_u32(0x200f).unwrap())); prop_assert!(out.chars().count()<=201);
        }
        let path=["", "a", "b"].join("/"); let out=scrub_text(&format!("{word} {separator}{path}"),None); prop_assert!(!out.contains(&path));
        let tilde=["~", "c"].join("/"); let out=scrub_text(&format!("{word} {separator}{tilde}"),None); prop_assert!(!out.contains(&tilde));
    }
}

fn contains_long_token(text: &str) -> bool {
    let mut count = 0;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            count += 1;
            if count >= 40 {
                return true;
            }
        } else {
            count = 0;
        }
    }
    false
}

#[test]
fn report_never_contains_paths_or_tickets() {
    let below = "x".repeat(39);
    let boundary = "x".repeat(40);
    assert_eq!(scrub_text(&below, None), below);
    assert_eq!(scrub_text(&boundary, None), "<redacted>");
    let temp = tempfile::tempdir().unwrap();
    let home = temp
        .path()
        .join("private home")
        .to_string_lossy()
        .into_owned();
    let path = format!("{home}/state.db");
    let sentinel = "x".repeat(43);
    let poison = format!("{path} {sentinel}");
    let mut facts = ready();
    facts.talaria_version = poison.clone();
    facts.gateway = GatewayFacts::Unreachable {
        message: poison.clone(),
        next_step: poison.clone(),
    };
    facts.identity = Some(IdentityFacts {
        code_version: Some(poison.clone()),
        code_sha: Some(poison.clone()),
        capabilities: vec![poison.clone()],
        served: vec!["fixture".into()],
        parked: vec![("default".into(), poison.clone())],
    });
    facts.attach = AttachFacts::Failed {
        message: poison.clone(),
        next_step: poison.clone(),
    };
    let mut status = status();
    status.storage_status = Some(poison.clone());
    status.storage_reason = Some(poison.clone());
    status.platforms_status = Some(poison.clone());
    status.dashboard_status = Some(poison.clone());
    status.dashboard_selftest = Some(poison.clone());
    status.standalone_reason = Some(poison.clone());
    facts.status = StatusFacts::Read(status);
    let report = diagnose(&facts);
    let text = render_text(&report);
    let rows = text
        .lines()
        .filter(|line| {
            ["ok    ", "warn  ", "FAIL  ", "skip  ", "info  "]
                .iter()
                .any(|prefix| line.starts_with(prefix))
        })
        .count();
    assert_eq!(rows, 7);
    assert!(text.lines().any(|line| line.starts_with("Result: ")));
    assert!(!text.contains(&home));
    assert!(!text.contains(&path));
    assert!(!contains_long_token(&text));
    let mut value: serde_json::Value =
        serde_json::from_str(&render_json(&report, None, &pin(), "0.7.0")).unwrap();
    assert_eq!(value["checks"].as_array().unwrap().len(), 7);
    value["talaria"]
        .as_object_mut()
        .unwrap()
        .remove("hermes_commit");
    let rendered = value.to_string();
    assert!(!rendered.contains(&home));
    assert!(!rendered.contains(&path));
    assert!(!contains_long_token(&rendered));
}

#[test]
fn text_and_json_golden() {
    let mut f = ready();
    let mut s = status();
    s.heartbeat_stale_s = Some(125.7);
    s.config_version = Some(22);
    f.status = StatusFacts::Read(s);
    let report = diagnose(&f);
    let expected = concat!(
        "Talaria doctor (profile fixture)\n",
        "ok    Talaria         Talaria 0.7.0, built for Hermes v0.22.4 (4d043038)\n",
        "ok    Gateway         ready · instance 9f2c41aa · epoch 3 · started by hand\n",
        "ok    Hermes version  0.22.4 (4d043038), the release Talaria was built for\n",
        "ok    Capabilities    all 5 capabilities Talaria needs are offered\n",
        "ok    Profiles        fixture is served; the gateway serves 2 profiles\n",
        "ok    Attach          attached to fixture in 36 ms\n",
        "warn  Runtime health  the gateway's housekeeping has not run for 125 s\n",
        "      Fix: Restart the gateway: press R here, or run talaria doctor --restart.\n",
        "      also: Hermes configuration is at version 22; this Hermes expects 23. Fix: Run hermes config migrate.\n",
        "Result: 6 ok, 1 warning, 0 failures.\n",
        "For provider keys, tools and dependencies, run hermes doctor.\n"
    );
    assert_eq!(render_text(&report), expected);
    let parse = |outcome| -> serde_json::Value {
        serde_json::from_str(&render_json(&report, outcome, &pin(), "0.7.0")).unwrap()
    };
    let v = parse(None);
    assert_eq!(v["format"], "talaria-doctor-v1");
    assert_eq!(v["overall"], "warn");
    assert!(v["restart"].is_null());
    let ids = v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "talaria",
            "gateway",
            "hermes_version",
            "capabilities",
            "profiles",
            "attach",
            "runtime_health"
        ]
    );
    let outcome = RestartOutcome::Restarted {
        seconds: 3,
        old_instance: "old".into(),
        new_instance: "new".into(),
    };
    let v = parse(Some(&outcome));
    assert_eq!(v["restart"]["seconds"], 3);
    assert!(v["restart"]["http_status"].is_null());
    let outcome = RestartOutcome::Refused {
        http_status: Some(401),
        detail: "refused".into(),
    };
    let v = parse(Some(&outcome));
    assert!(v["restart"]["seconds"].is_null());
    assert_eq!(v["restart"]["http_status"], 401);
}
