mod support;

#[path = "../../talaria-app/tests/support/capabilities_fixture.rs"]
mod capabilities_fixture;

use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use capabilities_fixture::{Harness, mock_gateway::StandaloneGateway};
use serde_json::{Value, json};
use talaria_cli::capabilities_cmd::{CapabilitiesArgs, collect, format_list, parse};
use talaria_core::capabilities::{CapData, CapTab};

fn args(list: &str, json: bool) -> CapabilitiesArgs {
    CapabilitiesArgs {
        list: list.into(),
        json,
        profile: "fixture".into(),
    }
}

fn assert_public_json(value: &Value, list: &str, count: usize) {
    assert_eq!(value["profile"], "fixture");
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["profile", list])
    );
    let expected = match list {
        "skills" => vec![
            "name",
            "enabled",
            "provenance",
            "category",
            "usage",
            "description",
        ],
        "toolsets" => vec![
            "name",
            "label",
            "enabled",
            "configured",
            "platform",
            "tools",
        ],
        "plugins" => vec!["name", "key", "version", "source", "status", "description"],
        _ => unreachable!(),
    };
    let rows = value[list].as_array().unwrap();
    assert_eq!(rows.len(), count);
    for row in rows {
        assert_eq!(
            row.as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            expected.iter().copied().collect()
        );
        for key in [
            "path",
            "install_dir",
            "after_install_path",
            "settings_schema",
        ] {
            assert!(row.get(key).is_none());
        }
    }
}

fn exact_columns_order_and_json_keys() {
    let cases = [
        (
            "skills",
            CapTab::Skills,
            json!([
                {"name":"zeta", "enabled":true, "provenance":"agent", "category":null, "usage":0, "description":null},
                {"name":"beta", "enabled":false, "provenance":"hub", "category":"build", "usage":3},
                {"name":"alpha", "enabled":true, "provenance":"bundled", "category":"build"}
            ]),
            "NAME   STATE  SOURCE   CATEGORY\nalpha  on     bundled  build\nbeta   off    hub      build\nzeta   on     local    -\n",
        ),
        (
            "toolsets",
            CapTab::Toolsets,
            json!([
                {"name":"web", "enabled":true, "configured":true, "tools":["search","fetch"]},
                {"name":"image_gen", "enabled":false, "configured":false, "tools":["draw"]}
            ]),
            "NAME       STATE  TOOLS  SETUP\nimage_gen  off    1      needed\nweb        on     2      ok\n",
        ),
        (
            "plugins",
            CapTab::Plugins,
            json!({"plugins":[
                {"name":"zeta", "source":"git", "status":"disabled", "version":"2"},
                {"name":"alpha", "source":"bundled", "status":"enabled"},
                {"name":"beta", "source":"entrypoint", "status":"not enabled", "version":"1"}
            ]}),
            "NAME   STATE        VERSION  SOURCE\nalpha  enabled      -        bundled\nbeta   not-enabled  1        package\nzeta   disabled     2        yours\n",
        ),
    ];
    for (list, tab, wire, expected) in cases {
        let data = CapData::from_json(tab, &wire).unwrap();
        assert_eq!(
            format_list(
                &args(list, false),
                &data,
                "/synthetic/hermes",
                "/synthetic/home"
            ),
            expected
        );
        let text = format_list(
            &args(list, true),
            &data,
            "/synthetic/hermes",
            "/synthetic/home",
        );
        assert_eq!(text.lines().count(), 1);
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_public_json(
            &value,
            list,
            match list {
                "skills" | "plugins" => 3,
                _ => 2,
            },
        );
        if list == "skills" {
            assert_eq!(value[list][2]["category"], Value::Null);
            assert_eq!(value[list][2]["provenance"], "local");
            assert_eq!(value[list][2]["description"], "");
        }
        if list == "plugins" {
            assert_eq!(value[list][2]["source"], "git");
            assert_eq!(value[list][1]["status"], "not enabled");
        }
    }
}

async fn mocked_transport_text_json_and_exit_codes() {
    for (list, header, count) in [
        ("skills", "NAME", 4),
        ("toolsets", "NAME", 4),
        ("plugins", "NAME", 6),
    ] {
        let harness = Harness::new().await;
        let service = harness.service();
        let text = collect(
            &args(list, false),
            &service,
            "/synthetic/hermes",
            "/synthetic/home",
        )
        .await;
        assert_eq!(text.status, 0);
        assert!(text.stdout.starts_with(header));
        assert!(text.stderr.is_empty());
        let result = collect(
            &args(list, true),
            &service,
            "/synthetic/hermes",
            "/synthetic/home",
        )
        .await;
        assert_eq!(result.status, 0);
        assert!(result.stderr.is_empty());
        assert_public_json(
            &serde_json::from_str::<Value>(&result.stdout).unwrap(),
            list,
            count,
        );
        for output in [&text.stdout, &result.stdout] {
            for forbidden in [
                "/synthetic/hermes",
                "/synthetic/home",
                "install_dir",
                "after_install_path",
                "fixture-secret-value",
            ] {
                assert!(!output.contains(forbidden), "leaked {forbidden}");
            }
        }
    }
    for (status, detail, expected) in [
        (
            403,
            "profile_scope_mismatch",
            "Profile fixture is not served by the gateway, so its capabilities cannot be read or changed.",
        ),
        (
            401,
            "Unauthorized",
            "Hermes refused Talaria's sign-in for this request. Press r to retry.",
        ),
        (
            404,
            "Not Found",
            "This gateway does not answer GET /api/skills on Talaria's connection.",
        ),
        (500, "internal", "Hermes refused this request: HTTP 500"),
    ] {
        let harness = Harness::new().await;
        harness
            .rest
            .fail_status("GET", "/api/skills", status, Some(detail))
            .await;
        let result = collect(
            &args("skills", false),
            &harness.service(),
            "/synthetic/hermes",
            "/synthetic/home",
        )
        .await;
        assert_eq!(result.status, 1);
        assert!(result.stdout.is_empty());
        assert_eq!(result.stderr, format!("{expected}\n"));
    }
}

fn fake_hermes(home: &std::path::Path) -> std::path::PathBuf {
    let bin = home.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let script = bin.join("hermes");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf called >> \"$HOME/hermes-called\"\nexit 91\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    bin
}

#[test]
fn text_json_and_exit_codes() {
    let _ = parse(&[
        "capabilities".into(),
        "skills".into(),
        "--profile".into(),
        "fixture".into(),
    ])
    .unwrap();
    exact_columns_order_and_json_keys();
    arguments_default_profile_and_reject_hub();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(mocked_transport_text_json_and_exit_codes());

    assert_eq!(
        parse(&[
            "capabilities".into(),
            "skills".into(),
            "--profile".into(),
            "fixture".into()
        ])
        .unwrap(),
        args("skills", false)
    );
    let home = tempfile::tempdir().unwrap();
    let bin = fake_hermes(home.path());
    let gateway = StandaloneGateway::start(home.path());
    gateway.with_capabilities("fixture");
    for (list, header, count) in [
        ("skills", "NAME             STATE  SOURCE", 4),
        ("toolsets", "NAME", 4),
        ("plugins", "NAME", 6),
    ] {
        for json_output in [false, true] {
            let mut command =
                support::talaria(&["capabilities", list, "--profile", "fixture"], home.path());
            command.env("PATH", &bin);
            if json_output {
                command.arg("--json");
            }
            let result = support::captured(command, Duration::from_secs(5));
            assert_eq!(
                result.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(result.stderr.is_empty());
            let text = String::from_utf8(result.stdout).unwrap();
            if json_output {
                assert_public_json(&serde_json::from_str::<Value>(&text).unwrap(), list, count);
            } else {
                assert!(text.starts_with(header), "{text}");
            }
            assert!(!text.contains(home.path().to_str().unwrap()));
        }
    }
    let mut command = support::talaria(
        &["capabilities", "plugins", "--profile", "unserved"],
        home.path(),
    );
    command.env("PATH", &bin);
    let unserved = support::captured(command, Duration::from_secs(5));
    assert_eq!(unserved.status.code(), Some(1));
    assert!(unserved.stdout.is_empty());
    assert_eq!(
        String::from_utf8(unserved.stderr).unwrap(),
        "Profile unserved is not served by the gateway, so its capabilities cannot be read or changed.\n"
    );
    assert!(!home.path().join("hermes-called").exists());
    gateway.fail_capability_http(500);
    let result = support::captured(
        support::talaria(
            &["capabilities", "skills", "--profile", "fixture"],
            home.path(),
        ),
        Duration::from_secs(5),
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        "Hermes refused this request: HTTP 500\n"
    );
    gateway.fail_capability(
        "plugins.manage",
        "list",
        -32601,
        "unknown method",
        Value::Null,
    );
    let result = support::captured(
        support::talaria(
            &["capabilities", "plugins", "--profile", "fixture"],
            home.path(),
        ),
        Duration::from_secs(5),
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        "This gateway does not answer plugins.manage on Talaria's connection.\n"
    );

    let absent = tempfile::tempdir().unwrap();
    let bin = fake_hermes(absent.path());
    for (list, expected) in [("skills", 1), ("unknown", 2)] {
        let mut command = support::talaria(&["capabilities", list], absent.path());
        command.env("PATH", &bin);
        let result = support::captured(command, Duration::from_secs(5));
        assert_eq!(result.status.code(), Some(expected));
        assert!(result.stdout.is_empty());
        if list == "unknown" {
            assert_eq!(
                String::from_utf8(result.stderr).unwrap(),
                "talaria capabilities: unknown list unknown; use skills, toolsets or plugins\n"
            );
        } else {
            assert!(!result.stderr.is_empty());
        }
    }
    assert!(!absent.path().join("hermes-called").exists());
}

fn arguments_default_profile_and_reject_hub() {
    assert_eq!(
        parse(&["capabilities".into(), "skills".into()]).unwrap(),
        CapabilitiesArgs {
            list: "skills".into(),
            profile: "default".into(),
            json: false
        }
    );
    assert_eq!(
        parse(&[
            "capabilities".into(),
            "toolsets".into(),
            "--json".into(),
            "--profile".into(),
            "fixture".into()
        ])
        .unwrap(),
        args("toolsets", true)
    );
    assert_eq!(
        parse(&["capabilities".into(), "hub".into()]).unwrap_err(),
        "talaria capabilities: unknown list hub; use skills, toolsets or plugins"
    );
}
