use serde_json::json;
use talaria_core::config_check::{ConfigFacts, Finding, findings};
use talaria_core::doctor::*;

fn pin() -> Pin {
    Pin {
        tag: hermes_contract::pinned::TAG.into(),
        commit: hermes_contract::pinned::COMMIT.into(),
    }
}
fn facts(config: ConfigFacts) -> DoctorFacts {
    DoctorFacts {
        talaria_version: "0.0.0".into(),
        pin: pin(),
        profile: "fixture".into(),
        gateway: GatewayFacts::Incompatible,
        identity: None,
        attach: AttachFacts::NotTried,
        status: StatusFacts::NotTried,
        config,
    }
}

#[test]
fn compression_is_reported_only_when_it_is_false() {
    assert_eq!(
        findings(&json!({"compression":{"in_place":false}})),
        [Finding::CompressionNotInPlace]
    );
    assert!(findings(&json!({"compression":{"in_place":true}})).is_empty());
    assert!(findings(&json!({"compression":{}})).is_empty());
    assert!(findings(&json!({})).is_empty());
    assert!(findings(&json!({"compression":{"in_place":"false"}})).is_empty());
}

#[test]
fn a_finite_step_cap_is_reported_and_every_no_cap_spelling_is_not() {
    for (raw, turns) in [
        (json!(60), 60),
        (json!(60.0), 60),
        (json!("60"), 60),
        (json!(" 12 "), 12),
        (json!("7.9"), 7),
    ] {
        assert_eq!(
            findings(&json!({"agent":{"max_turns":raw.clone()}})),
            [Finding::StepCap { turns }],
            "{raw}"
        );
    }
    for raw in [
        json!(null),
        json!(0),
        json!(-1),
        json!("0"),
        json!("-1"),
        json!("none"),
        json!("None"),
        json!("unlimited"),
        json!("inf"),
        json!(""),
        json!("sixty"),
        json!(true),
        json!([60]),
    ] {
        assert!(
            findings(&json!({"agent":{"max_turns":raw.clone()}})).is_empty(),
            "{raw}"
        );
    }
    assert!(findings(&json!({"agent":{}})).is_empty());
}

#[test]
fn a_root_step_cap_counts_only_when_agent_has_none() {
    assert_eq!(
        findings(&json!({"max_turns":25})),
        [Finding::StepCap { turns: 25 }]
    );
    assert!(findings(&json!({"max_turns":25,"agent":{"max_turns":null}})).is_empty());
    assert_eq!(
        findings(&json!({"max_turns":25,"agent":{"max_turns":9}})),
        [Finding::StepCap { turns: 9 }]
    );
}

#[test]
fn both_findings_come_in_a_fixed_order_and_name_their_keys() {
    let both = findings(&json!({"compression":{"in_place":false},"agent":{"max_turns":60}}));
    assert_eq!(
        both,
        [
            Finding::CompressionNotInPlace,
            Finding::StepCap { turns: 60 }
        ]
    );
    assert_eq!(both[0].key(), "compression.in_place");
    assert_eq!(both[1].key(), "agent.max_turns");
    assert!(
        both[0]
            .notice("work")
            .contains("hermes -p work config set compression.in_place true")
    );
    assert!(both[1].notice("work").contains("60 steps"));
    assert!(
        both[1]
            .notice("work")
            .contains("config set agent.max_turns 0")
    );
    assert!(both[0].workflow_warning("work").is_some());
    assert_eq!(both[1].workflow_warning("work"), None);
}

#[test]
fn doctor_adds_one_warning_row_per_finding() {
    let report = diagnose(&facts(ConfigFacts::Read(vec![
        Finding::CompressionNotInPlace,
        Finding::StepCap { turns: 60 },
    ])));
    assert_eq!(report.checks.len(), 9);
    let rows = &report.checks[7..];
    assert_eq!(rows[0].id, CheckId::Compression);
    assert_eq!(rows[1].id, CheckId::StepCap);
    for row in rows {
        assert_eq!(row.severity, Severity::Warn);
        let fix = row.fix.as_deref().unwrap();
        assert!(fix.contains("hermes -p fixture config set"), "{fix}");
    }
    assert!(
        rows[0]
            .fix
            .as_deref()
            .unwrap()
            .ends_with("compression.in_place true")
    );
    assert!(
        rows[1]
            .fix
            .as_deref()
            .unwrap()
            .ends_with("agent.max_turns 0")
    );
    assert!(rows[0].summary.starts_with("compression.in_place is false"));
    assert!(rows[1].summary.starts_with("agent.max_turns is 60"));
    let baseline = diagnose(&facts(ConfigFacts::NotTried)).counts();
    assert_eq!(report.counts(), (baseline.0, baseline.1 + 2, baseline.2));
    let text = render_text(&report);
    assert!(text.contains("warn  Compression"));
    assert!(text.contains("warn  Step cap"));
    let json = render_json(&report, None, &pin(), "0.0.0");
    let value: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    let ids: Vec<_> = value["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(&ids[7..], ["compression", "step_cap"]);
    assert_eq!(value["checks"][7]["severity"], "warn");
}

#[test]
fn doctor_adds_nothing_when_the_settings_are_fine_or_were_not_checked() {
    for config in [ConfigFacts::NotTried, ConfigFacts::Read(Vec::new())] {
        assert_eq!(diagnose(&facts(config)).checks.len(), 7);
    }
}

#[test]
fn an_unreadable_configuration_is_an_unknown_row_that_does_not_warn() {
    let report = diagnose(&facts(ConfigFacts::Unknown {
        reason: "the gateway refused the request".into(),
    }));
    assert_eq!(report.checks.len(), 8);
    let row = &report.checks[7];
    assert_eq!(row.id, CheckId::Settings);
    assert_eq!(row.severity, Severity::Info);
    assert!(row.summary.starts_with("unknown: "));
    assert_eq!(
        report.counts(),
        diagnose(&facts(ConfigFacts::NotTried)).counts()
    );
    assert!(render_text(&report).contains("info  Settings"));
}

#[test]
fn the_selection_can_reach_the_added_rows() {
    let mut view = DoctorView::new();
    view.report = Some(diagnose(&facts(ConfigFacts::Read(vec![
        Finding::CompressionNotInPlace,
        Finding::StepCap { turns: 3 },
    ]))));
    view.running = false;
    for _ in 0..20 {
        doctor_view_update(&mut view, &RestartState::Idle, DoctorKey::Down);
    }
    assert_eq!(view.selected, 8);
}
