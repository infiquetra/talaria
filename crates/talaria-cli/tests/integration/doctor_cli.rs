use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};
use talaria_core::doctor::{Check, CheckId, Report, RestartOutcome, Severity, exit_code};

fn report(severity: Severity) -> Report {
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
            .enumerate()
            .map(|(i, id)| Check {
                id,
                severity: if i == 1 { severity } else { Severity::Ok },
                summary: String::new(),
                details: vec![],
                fix: None,
            })
            .collect(),
    }
}

fn invoke(home: &Path, fake_bin: &Path, marker: &Path, args: &[&str]) -> Output {
    let mut paths = vec![fake_bin.to_path_buf()];
    if let Some(original) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&original));
    }
    Command::new(env!("CARGO_BIN_EXE_talaria"))
        .arg("doctor")
        .args(args)
        .env("HERMES_HOME", home)
        .env("PATH", std::env::join_paths(paths).expect("path list"))
        .env("DOCTOR_MARK", marker)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("doctor process")
}
fn text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

#[test]
fn exit_codes_and_outputs() {
    let ok = report(Severity::Warn);
    let fail = report(Severity::Fail);
    assert_eq!(exit_code(&ok, None), 0);
    assert_eq!(exit_code(&fail, None), 1);
    let restarted = RestartOutcome::Restarted {
        seconds: 4,
        old_instance: "old-id".into(),
        new_instance: "new-id".into(),
    };
    assert_eq!(exit_code(&ok, Some(&restarted)), 0);
    for outcome in [
        RestartOutcome::Refused {
            http_status: Some(401),
            detail: "refused".into(),
        },
        RestartOutcome::Changed,
        RestartOutcome::DidNotRestart,
        RestartOutcome::NotBack,
        RestartOutcome::NotReady,
    ] {
        assert_eq!(exit_code(&ok, Some(&outcome)), 1, "{outcome:?}");
    }
    assert_eq!(exit_code(&ok, Some(&RestartOutcome::Declined)), 4);
    assert_eq!(
        exit_code(&fail, Some(&RestartOutcome::NeedsConfirmation)),
        3
    );

    let temp = tempfile::tempdir().expect("temporary directory");
    let home = temp.path().join("home");
    let fake_bin = temp.path().join("bin");
    let marker = temp.path().join("invoked");
    fs::create_dir(&home).expect("home");
    fs::create_dir(&fake_bin).expect("bin");
    let fake = fake_bin.join("hermes");
    let script = format!(
        "#!{}bin{}sh\nprintf invoked > \"$DOCTOR_MARK\"\nexit 1\n",
        '/', '/'
    );
    fs::write(&fake, script).expect("fake command");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).expect("executable");
    }

    let plain = invoke(&home, &fake_bin, &marker, &[]);
    let output = text(&plain);
    assert_eq!(plain.status.code(), Some(1));
    assert!(
        output
            .lines()
            .any(|line| line.contains("FAIL") && line.contains("Gateway"))
    );
    assert_eq!(
        output
            .lines()
            .filter(|line| line.trim_start().starts_with("skip"))
            .count(),
        5
    );
    let json = invoke(&home, &fake_bin, &marker, &["--json"]);
    assert_eq!(json.status.code(), Some(1));
    let parsed: serde_json::Value = serde_json::from_slice(&json.stdout).expect("JSON output");
    assert_eq!(parsed["overall"], "fail");
    for args in [&["--yes"][..], &["--profile", ""][..], &["--unknown"][..]] {
        assert_eq!(
            invoke(&home, &fake_bin, &marker, args).status.code(),
            Some(2)
        );
    }
    let refusing = invoke(&home, &fake_bin, &marker, &["--restart"]);
    assert_eq!(refusing.status.code(), Some(3));
    assert!(text(&refusing).contains(
        "Restart: Refusing to restart without confirmation: run in a terminal or add --yes."
    ));
    let confirmed = invoke(&home, &fake_bin, &marker, &["--restart", "--yes"]);
    assert_eq!(confirmed.status.code(), Some(1));
    let confirmed_text = text(&confirmed);
    assert!(
        confirmed_text
            .contains("Restart: The gateway is not ready, so Talaria cannot ask it to restart.")
    );
    assert!(!confirmed_text.contains("Refusing"));
    assert!(!confirmed_text.contains("Type yes"));
    let trimmed = invoke(
        &home,
        &fake_bin,
        &marker,
        &["--json", "--profile", " fixture "],
    );
    assert_eq!(trimmed.status.code(), Some(1));
    let parsed: serde_json::Value = serde_json::from_slice(&trimmed.stdout).expect("JSON output");
    assert_eq!(parsed["profile"], "fixture");
    assert!(
        !marker.exists(),
        "doctor must not invoke the gateway command"
    );
}
