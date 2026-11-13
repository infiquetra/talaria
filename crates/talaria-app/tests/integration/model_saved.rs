//! Saved model file persistence scenarios. Issue 230.

use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use talaria_app::models_file::SavedModelsFile;
use talaria_core::models::{Effort, SavedModels};

struct TempModels {
    path: PathBuf,
}

impl TempModels {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("talaria-models-{nanos}.json"));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempModels {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn assert_unreadable(error: &str) {
    let prefix = "~/.talaria/models.json could not be read (";
    let suffix = "); favourites and saved defaults are off until it is fixed or removed.";
    assert!(error.starts_with(prefix), "{error}");
    assert!(error.ends_with(suffix), "{error}");
    let reason = &error[prefix.len()..error.len() - suffix.len()];
    assert!(!reason.is_empty(), "{error}");
}

#[test]
fn file_round_trip_and_errors() {
    let file = TempModels::new();
    let body = json!({
        "version": 1,
        "favourites": ["ollama-cloud::glm-5"],
        "defaults": {
            "fixture": {"model": "glm-5", "provider": "ollama-cloud", "effort": "high"}
        }
    });
    fs::write(file.path(), body.to_string()).expect("write sample");
    let loaded = SavedModels::read(file.path()).expect("read sample");
    assert_eq!(loaded.favourites, vec!["ollama-cloud::glm-5".to_owned()]);
    let saved = loaded.defaults.get("fixture").expect("fixture default");
    assert_eq!(saved.model.as_deref(), Some("glm-5"));
    assert_eq!(saved.provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(saved.effort, Some(Effort::High));
    loaded.write(file.path()).expect("write back");
    let again = SavedModels::read(file.path()).expect("read back");
    assert_eq!(again, loaded);

    let mut extra = body.clone();
    extra["note"] = json!("keep me out");
    fs::write(file.path(), extra.to_string()).expect("write extra");
    let loaded = SavedModels::read(file.path()).expect("unknown key is ignored");
    loaded.write(file.path()).expect("rewrite");
    let text = fs::read_to_string(file.path()).expect("reread");
    assert!(!text.contains("note"));

    fs::remove_file(file.path()).expect("remove");
    let missing = SavedModels::read(file.path()).expect("missing file is empty");
    assert_eq!(missing, SavedModels::empty());

    let bad = [
        "{",
        r#"{"version":2,"favourites":[],"defaults":{}}"#,
        r#"{"version":1,"favourites":"nope","defaults":{}}"#,
        r#"{"version":1,"favourites":[],"defaults":{"fixture":{"effort":"hgih"}}}"#,
    ];
    for body in bad {
        fs::write(file.path(), body).expect("write bad file");
        let before = fs::read(file.path()).expect("read bytes");
        let error = SavedModels::read(file.path()).expect_err("bad file");
        assert_unreadable(&error);
        let after = fs::read(file.path()).expect("bytes after the refused read");
        assert_eq!(before, after);
    }
}
