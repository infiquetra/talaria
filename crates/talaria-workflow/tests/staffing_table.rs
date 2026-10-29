use std::{path::Path, process::Command};
use talaria_workflow::{
    engine::Effort,
    staffing::{DEFAULT_MAX_IN_FLIGHT, StaffingError, StaffingTable, talaria_home},
};

const EXAMPLE: &str = r#"
version = 1
[defaults]
tier = "builder"
[tiers.survey]
model = "minimax-m3.1-flash"
provider = "ollama-cloud"
note = "free; good enough for listing files"
[tiers.builder]
model = "coding-model"
provider = "example-cloud"
effort = "high"
[tiers.judge]
model = "judge-model"
provider = "example-cloud"
effort = "high"
[limits]
max_in_flight = 10
[providers.ollama-cloud]
max_in_flight = 4
[budget]
money_usd = 5.0
tokens = 2000000
"#;

#[test]
fn parses_the_documented_example() {
    assert_eq!(DEFAULT_MAX_IN_FLIGHT, 1);
    let table = StaffingTable::parse(EXAMPLE).unwrap();
    assert_eq!(table.default_tier.as_deref(), Some("builder"));
    assert_eq!(table.tiers["survey"].model, "minimax-m3.1-flash");
    assert_eq!(table.tiers["survey"].provider, "ollama-cloud");
    assert_eq!(table.tiers["builder"].effort, Some(Effort::High));
    assert_eq!(table.limits.max_in_flight, 10);
    assert_eq!(table.providers["ollama-cloud"].max_in_flight, 4);
    assert_eq!(table.budget.tokens, Some(2_000_000));
    assert_eq!(table.budget.money_usd, Some(5.0));
}

#[test]
fn refuses_unknown_and_invalid_keys() {
    let cases = [
        ("unexpected = 1", "unknown key 'unexpected'"),
        ("version = 2", "version must be 1"),
        (
            "[tiers.BAD]\nmodel='a'\nprovider='b'",
            "tier name 'BAD' must be lower case letters, digits, '-' or '_', at most 32",
        ),
        ("[tiers.good]\nprovider='b'", "model is required"),
        (
            "[tiers.good]\nmodel='  '\nprovider='b'",
            "model is required",
        ),
        ("[tiers.good]\nmodel='a'", "provider is required"),
        (
            "[tiers.good]\nmodel='a'\nprovider='  '",
            "provider is required",
        ),
        (
            "[tiers.good]\nmodel='a'\nprovider='b'\neffort='extreme'",
            "effort must be one of none, minimal, low, medium, high, xhigh, max, ultra",
        ),
        (
            "[tiers.good]\nmodel='a'\nprovider='b'\nextra=1",
            "unknown key 'extra'",
        ),
        (
            "[defaults]\ntier='missing'",
            "defaults.tier names undefined tier 'missing'",
        ),
        (
            "[limits]\nmax_in_flight=0",
            "max_in_flight must be between 1 and 64",
        ),
        (
            "[limits]\nmax_in_flight=65",
            "max_in_flight must be between 1 and 64",
        ),
        (
            "[providers.alpha]\nmax_in_flight=0",
            "max_in_flight must be between 1 and 64",
        ),
        (
            "[budget]\nmoney_usd=0",
            "money_usd must be a positive number",
        ),
        ("[budget]\ntokens=0", "tokens must be a positive integer"),
    ];
    for (input, expected) in cases {
        match StaffingTable::parse(input) {
            Err(StaffingError::Invalid { key, problem }) => {
                assert!(!key.is_empty());
                assert_eq!(problem, expected, "{input}");
            }
            other => panic!("expected invalid input for {input}: {other:?}"),
        }
    }
    let long_model = format!("[tiers.good]\nmodel='{}'\nprovider='p'", "a".repeat(257));
    assert!(
        matches!(StaffingTable::parse(&long_model), Err(StaffingError::Invalid { problem, .. }) if problem == "model is required")
    );
    assert!(matches!(
        StaffingTable::parse("[limits\n"),
        Err(StaffingError::Toml {
            line: 1,
            column: 8,
            ..
        })
    ));
}

#[test]
fn absent_file_is_an_empty_table() {
    if let Ok(expected) = std::env::var("STAFFING_TEST_EXPECT_HOME") {
        assert_eq!(talaria_home().as_deref(), Some(Path::new(&expected)));
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let table = StaffingTable::load(&tmp.path().join("missing.toml")).unwrap();
    assert!(table.tiers.is_empty());
    assert_eq!(table.limits.max_in_flight, DEFAULT_MAX_IN_FLIGHT);
    assert!(matches!(
        StaffingTable::load(tmp.path()),
        Err(StaffingError::Io { .. })
    ));
    assert_eq!(
        StaffingTable::default_path(tmp.path()),
        tmp.path().join("staffing.toml")
    );
    let exe = std::env::current_exe().unwrap();
    for (config, expected) in [
        (tmp.path().join("config"), tmp.path().join("config")),
        (Path::new("").to_path_buf(), tmp.path().join(".talaria")),
    ] {
        let result = Command::new(&exe)
            .args(["--exact", "absent_file_is_an_empty_table"])
            .env("HOME", tmp.path())
            .env("TALARIA_CONFIG_DIR", config)
            .env("STAFFING_TEST_EXPECT_HOME", expected)
            .status()
            .unwrap();
        assert!(result.success());
    }
}

#[test]
fn template_parses() {
    let table = StaffingTable::parse(StaffingTable::TEMPLATE).unwrap();
    assert!(table.tiers.is_empty());
    assert_eq!(table.default_tier, None);
    assert_eq!(table.limits.max_in_flight, DEFAULT_MAX_IN_FLIGHT);
    assert!(table.providers.is_empty());
    assert_eq!(table.budget.tokens, None);
    assert_eq!(table.budget.money_usd, None);
}
