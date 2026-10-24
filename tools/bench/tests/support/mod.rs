//! Helpers shared by the harness tests.

#![allow(dead_code)]

use std::path::PathBuf;

use talaria_bench::Budgets;

/// The fake client binary built for these tests.
pub fn fake_tui_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake-tui"))
}

/// The harness binary built for these tests.
pub fn bench_bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_talaria-bench"))
}

/// The repository root, computed at run time.
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Loose budgets for tests against the fake client.
pub fn test_budgets() -> Budgets {
    Budgets::load(&data_dir().join("budgets-test.toml")).expect("test budgets load")
}

/// The shipped budgets file.
pub fn shipped_budgets_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("budgets.toml")
}

/// `tools/bench/tests/data`.
pub fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

/// The copy of the template frame logs the generator tests read.
pub fn templates_dir() -> PathBuf {
    data_dir().join("templates")
}
