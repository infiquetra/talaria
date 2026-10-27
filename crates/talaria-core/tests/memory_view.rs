use talaria_core::memory::*;

#[test]
fn format_bytes_matches_desktop() {
    for (bytes, expected) in [
        (0, "empty"),
        (1, "1 B"),
        (1023, "1023 B"),
        (1024, "1.0 KB"),
        (1536, "1.5 KB"),
        (2304, "2.3 KB"),
        (1_048_575, "1024.0 KB"),
        (1_048_576, "1.0 MB"),
        (5_242_880, "5.0 MB"),
    ] {
        assert_eq!(format_bytes(bytes), expected);
    }
}

#[test]
fn provider_rows_and_labels() {
    provider_status_labels_are_clear();
    let now = std::time::Instant::now();
    let mut state = MemoryState::new();
    state.update(
        MemoryInput::Open {
            profile: "fixture".into(),
        },
        now,
    );
    let rows = ["ready", "needs_config", "unavailable", "missing", "future"]
        .map(|status| ProviderRowView {
            name: status.into(),
            description: String::new(),
            available: true,
            configured: true,
            status: status.into(),
            setup: ProviderSetup {
                pip_dependencies: vec![],
                python_dependencies_declared: false,
                external_dependencies: vec![],
                required_env: vec![],
                dependencies_installed: true,
            },
        })
        .to_vec();
    state.update(
        MemoryInput::StatusLoaded(Ok(StatusView {
            active: "ready".into(),
            providers: rows,
            builtin_files: BuiltinFiles { memory: 0, user: 0 },
        })),
        now,
    );
    let view = state.view(now);
    assert_eq!(view.active_text, "active: ready");
    let Section::Ready(rows) = view.providers else {
        panic!("provider rows not ready")
    };
    assert_eq!(
        rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec![
            "built-in only",
            "ready",
            "needs_config",
            "unavailable",
            "missing",
            "future"
        ]
    );
    assert_eq!(rows[0].label, "available");
    assert_eq!(rows[1].marker, '●');
    assert_eq!(rows[1].label, "in use");
    assert_eq!(rows[2].label, "needs settings");
    assert_eq!(rows[3].label, "not available here");
    assert_eq!(rows[4].label, "not installed");
    assert_eq!(rows[5].label, "future");
}

fn curator() -> CuratorView {
    CuratorView {
        enabled: true,
        paused: false,
        interval_hours: Some(168.0),
        last_run_at: None,
        min_idle_hours: None,
        stale_after_days: None,
        archive_after_days: None,
    }
}

#[test]
fn provider_status_labels_are_clear() {
    for (status, label) in [
        ("ready", "ready"),
        ("needs_config", "needs settings"),
        ("unavailable", "not available here"),
        ("missing", "not installed"),
        ("future", "future"),
    ] {
        assert_eq!(provider_label(status), label);
    }
}

#[test]
fn curator_lines() {
    let mut c = curator();
    for (interval, phrase) in [
        (Some(168.0), "every 7 days"),
        (Some(24.0), "every day"),
        (Some(30.0), "every 30 h"),
        (Some(12.5), "every 12.5 h"),
    ] {
        c.interval_hours = interval;
        assert!(talaria_core::memory::curator_lines(&c).1.contains(phrase));
    }
    assert_eq!(
        talaria_core::memory::curator_lines(&c).2,
        "Last run or first check: none yet"
    );
    c.last_run_at = Some("2026-10-20T16:03:11+02:00".into());
    assert_eq!(
        talaria_core::memory::curator_lines(&c).2,
        "Last run or first check: 2026-10-20 14:03 UTC"
    );
    c.last_run_at = Some("2026-10-20T14:03:11.5+00:00".into());
    assert_eq!(
        talaria_core::memory::curator_lines(&c).2,
        "Last run or first check: 2026-10-20 14:03 UTC"
    );
    c.last_run_at = Some("yesterday".into());
    assert_eq!(
        talaria_core::memory::curator_lines(&c).2,
        "Last run or first check: yesterday"
    );
    c.interval_hours = None;
    assert!(!talaria_core::memory::curator_lines(&c).1.contains("every"));
    c.paused = true;
    assert_eq!(talaria_core::memory::curator_lines(&c).0, "paused");
    c.enabled = false;
    assert_eq!(talaria_core::memory::curator_lines(&c).0, "off");
}

#[test]
fn action_tail_uses_only_the_latest_run() {
    let lines = [
        "=== curator-run started old",
        "old",
        "=== curator-run started new",
        "one",
        "two",
    ]
    .map(String::from);
    assert_eq!(tail_after_header(&lines, 8), vec!["one", "two"]);
    let lines: Vec<String> = (0..12).map(|n| n.to_string()).collect();
    assert_eq!(tail_after_header(&lines, 8), lines[4..]);
}
