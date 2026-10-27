//! Screen readers against synthetic examples.
use smoke::screen4a::*;
use smoke::wording4a::*;
fn screen(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/screens/phase4a")
            .join(name),
    )
    .expect("screen")
}
#[test]
fn settings_title_found_with_profile() {
    assert!(settings_title_found(
        &screen("config-browse-r1.txt"),
        "fixture"
    ));
    assert!(!settings_title_found(
        &screen("config-browse-r1.txt"),
        "other"
    ));
}
#[test]
fn settings_category_row_found() {
    assert_eq!(
        settings_categories(&screen("config-browse-r1.txt")),
        SETTINGS_CATEGORIES
    );
}
#[test]
fn setting_row_parsed_with_value_and_flags() {
    let row = setting_row(&screen("config-browse-r2.txt"), "display.compact").expect("row");
    assert_eq!(row.value, "false");
    assert!(row.flags.contains(&"set".to_owned()));
    assert!(row.flags.contains(&"may need a restart".to_owned()));
    for value in ["(not set)", "••••…", "plaintext"] {
        let row = setting_row(
            &format!("delegation.api_key  {value}"),
            "delegation.api_key",
        )
        .expect("row");
        assert_eq!(row.value, value);
    }
}
#[test]
fn providers_row_parsed_with_status_and_key_tail() {
    let text = screen("providers-tabs-r1.txt");
    let row = providers_row(&text, "Ollama Cloud").expect("row");
    assert!(row.status.starts_with("ready ·"));
    assert_eq!(row.key_tail.as_deref(), Some("abcd"));
    assert_eq!(
        providers_row(&text, "OpenRouter").expect("row").status,
        "no key"
    );
}
#[test]
fn providers_tabs_found() {
    assert_eq!(
        providers_tabs(&screen("providers-tabs-r1.txt")),
        ["Keys", "Accounts", "Endpoints"]
    );
}
#[test]
fn models_sections_found_in_order() {
    let text = (1..=5)
        .map(|i| screen(&format!("models-page-r{i}.txt")))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        models_sections(&text),
        [
            "Main",
            "Fallbacks",
            "Helpers",
            "Subagents",
            "Mixture of Agents"
        ]
    );
}
#[test]
fn models_failed_section_detected() {
    assert!(models_failed("Helpers\nFailed"));
    assert!(!models_failed("Helpers\nVision"));
}
#[test]
fn profiles_header_and_rows_parsed() {
    let text = screen("profiles-list-r1.txt");
    assert!(text.contains(PROFILES_HEADER));
    let rows = profiles_rows(&text);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains("fixture") && rows[0].contains("served"));
}
#[test]
fn profiles_refusals_found_in_order() {
    for (i, refusal) in PROFILES_NAME_REFUSALS.iter().enumerate() {
        let text = screen(&format!("profiles-bad-names-r{}.txt", i + 1));
        assert!(refusal_found(&text, refusal));
    }
}
#[test]
fn write_banner_detected() {
    let hint = "Saved to profile fixture's key file as OPENROUTER_API_KEY.";
    assert!(!write_banner_found(hint));
    assert!(write_banner_found(&format!(
        "{hint}\nSaved the key for OpenRouter."
    )));
    for text in [
        "Saved the key for OpenRouter.",
        "Not restarted",
        "Gateway restarted",
    ] {
        assert!(write_banner_found(text));
    }
    assert!(!write_banner_found("Nothing changed"));
}
#[test]
fn refusal_text_matched_exactly() {
    assert!(refusal_found(
        &screen("config-secret-refused-r1.txt"),
        SETTINGS_SECRET_REFUSAL
    ));
    assert!(!refusal_found("Enter a number.", SETTINGS_NUMBER_REFUSAL));
}
#[test]
fn doctor_rows_counted() {
    assert_eq!(doctor_rows(&screen("doctor-overlay-r1.txt")).len(), 7);
}
#[test]
fn doctor_json_format_and_count_parsed() {
    assert_eq!(
        doctor_json(r#"{"format":"talaria-doctor-v1","checks":[1,2,3,4,5,6,7]}"#),
        Some(("talaria-doctor-v1".to_owned(), 7))
    );
    assert_eq!(doctor_json("bad"), None);
}
#[test]
fn doctor_summary_counts_parsed() {
    assert_eq!(
        doctor_summary(&screen("doctor-overlay-r1.txt")),
        Some((7, 0, 0))
    );
}
