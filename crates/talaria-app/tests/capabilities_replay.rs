#[path = "support/capabilities_fixture.rs"]
mod fixture;

use fixture::Harness;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};
use talaria_core::capabilities::{CapEffect, CapInput, CapTab, CapabilitiesState};

#[tokio::test]
async fn fixture_reduces_to_expected_view() {
    let harness = Harness::new().await;
    let service = harness.service();
    let mut state = CapabilitiesState::new("fixture".into(), None, CapTab::Skills);
    for (tab, expected) in [
        (
            CapTab::Skills,
            [
                "fixture-skill-a",
                "fixture-skill-d",
                "fixture-skill-b",
                "fixture-skill-c",
            ]
            .map(str::to_owned)
            .to_vec(),
        ),
        (
            CapTab::Toolsets,
            ["fixture-platform", "fixture-toolset", "image_gen", "web"]
                .map(str::to_owned)
                .to_vec(),
        ),
        (
            CapTab::Plugins,
            [
                "fixture-git",
                "fixture-no-key",
                "fixture-package",
                "fixture-plugin",
            ]
            .map(str::to_owned)
            .to_vec(),
        ),
        (
            CapTab::Hub,
            (1..=20)
                .map(|n| format!("fixture-hub-{n}"))
                .collect::<Vec<_>>(),
        ),
    ] {
        let mut pending: VecDeque<_> = state.update(CapInput::Open(tab), 0).into();
        while let Some(effect) = pending.pop_front() {
            if matches!(
                effect,
                CapEffect::Notice(_) | CapEffect::Dispose | CapEffect::ScheduleTick { .. }
            ) {
                continue;
            }
            for input in service.execute(effect).await {
                pending.extend(state.update(input, 1));
            }
        }
        let view = state.view();
        assert_eq!(view.title, "Capabilities · profile fixture");
        assert_eq!(view.tab, tab);
        assert!(!view.loading);
        let names: Vec<_> = view
            .rows
            .iter()
            .filter(|row| !row.is_header)
            .map(|row| row.name.as_str())
            .collect();
        assert_eq!(names, expected);
        assert!(!format!("{view:?}").contains("/redacted"));
        if tab == CapTab::Skills {
            assert_eq!(view.tabs[0], "Skills 3/4 on");
            assert_eq!(
                view.rows
                    .iter()
                    .filter(|row| row.is_header)
                    .map(|row| row.name.as_str())
                    .collect::<Vec<_>>(),
                ["creative", "development", "(no category)"]
            );
        }
        if tab == CapTab::Hub {
            assert_eq!(view.footer, "Page 1 of 2 · 25 skills");
        }
    }
}

fn files_under(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("fixture directory") {
        let path = entry.expect("fixture entry").path();
        assert!(
            !path.is_symlink(),
            "fixtures must be ordinary files or directories"
        );
        if path.is_dir() {
            files_under(&path, files);
        } else {
            files.push(path);
        }
    }
}

#[test]
fn fixtures_hold_no_private_data() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/capabilities");
    let mut files = Vec::new();
    files_under(&root, &mut files);
    assert!(!files.is_empty(), "the recursive scan must read fixtures");
    assert!(
        files
            .iter()
            .any(|path| path.parent() == Some(root.join("rest").as_path()))
    );
    assert!(
        files
            .iter()
            .any(|path| path.parent() == Some(root.join("socket").as_path()))
    );
    let forbidden = hermes_client::redact::local_forbidden_literals();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("fixture text");
        assert!(
            hermes_client::redact::scan_for_secrets(&text).is_empty(),
            "fixture secret scan"
        );
        assert!(
            hermes_client::redact::scan_literals(&text, &forbidden).is_empty(),
            "fixture literal scan"
        );
        let json: serde_json::Value = serde_json::from_str(&text).expect("fixture JSON");
        let compact = serde_json::to_string(&json).expect("compact fixture");
        assert!(
            hermes_client::redact::scan_for_secrets(&compact).is_empty(),
            "nested fixture secret scan"
        );
    }
}
