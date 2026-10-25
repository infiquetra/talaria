//! Issue 230, catalogue scenarios.

#[path = "support/catalogue.rs"]
mod catalogue;

use talaria_core::models::{Catalog, CatalogKind, DisplayZone};

const ZONE: DisplayZone = DisplayZone { minutes_east: 0 };

fn row_text(row: &talaria_core::models::CatalogRow) -> String {
    let mut text = row.text.clone();
    if !row.badges.is_empty() {
        text.push(' ');
        text.push_str(&row.badges.join(" "));
    }
    if let Some(price) = &row.price {
        text.push(' ');
        text.push_str(price);
    }
    text.push_str(&row.segments.join(" "));
    text
}

#[test]
#[allow(clippy::manual_contains)]
fn groups_and_order() {
    let catalog = Catalog::from_result(&catalogue::section_five_plus_absent());
    let rows = catalog.rows(&["ollama-cloud::minimax-m3.1-flash".to_string()], ZONE);
    let headers: Vec<_> = rows
        .iter()
        .filter(|row| row.kind == CatalogKind::Header)
        .map(|row| row.text.as_str())
        .collect();
    assert_eq!(headers[0], "Favourites");
    assert!(headers.iter().any(|text| text.starts_with("Ollama Cloud")));
    assert!(headers.iter().any(|text| *text == "Not set up (2)"));
    assert!(
        headers.iter().position(|text| *text == "Favourites")
            < headers
                .iter()
                .position(|text| text.starts_with("Ollama Cloud"))
    );
    assert!(
        headers
            .iter()
            .position(|text| text.starts_with("Ollama Cloud"))
            < headers.iter().position(|text| *text == "Not set up (2)")
    );

    let slugs: Vec<_> = rows
        .iter()
        .filter(|row| row.kind == CatalogKind::NotSetUp)
        .map(|row| row.provider_slug.as_str())
        .collect();
    assert_eq!(slugs, ["anthropic", "openrouter"]);
    assert_eq!(
        rows.iter()
            .find(|row| row.kind == CatalogKind::Favourite)
            .map(|row| row.model.as_str()),
        Some("minimax-m3.1-flash")
    );
}

#[test]
fn header_segments() {
    let catalog = Catalog::from_result(&catalogue::header_segments());
    let rows = catalog.rows(&[], ZONE);
    let rich = rows
        .iter()
        .find(|row| row.kind == CatalogKind::Header && row.provider_slug == "rich")
        .expect("rich header");
    assert_eq!(
        rich.segments,
        [
            "current".to_string(),
            "session 43%".to_string(),
            "weekly 11%".to_string(),
            "rate-limited until 18:00".to_string(),
            "prices loading".to_string(),
            "free tier".to_string(),
            "showing 2 of 30".to_string(),
        ]
    );
    let pool = rows
        .iter()
        .find(|row| row.provider_slug == "pool" && row.kind == CatalogKind::Header)
        .expect("pool header");
    assert!(
        pool.segments
            .iter()
            .any(|segment| segment == "3 accounts: 2 ready, 1 limited")
    );
}

#[test]
fn model_row_badges_and_prices() {
    let catalog = Catalog::from_result(&catalogue::badges_and_prices());
    let rows = catalog.rows(&[], ZONE);
    let row = |model: &str| {
        rows.iter()
            .find(|row| row.kind == CatalogKind::Model && row.model == model)
            .unwrap_or_else(|| panic!("missing {model}"))
    };
    assert_eq!(row("speedy").badges, ["fast".to_string()]);
    assert_eq!(row("speedy").price.as_deref(), Some("free"));
    assert_eq!(row("plain").badges, ["no effort".to_string()]);
    assert_eq!(row("plain").price.as_deref(), Some("$0.60 in · $2.20 out"));
    assert!(row("gone").dim);
    assert!(
        row("gone")
            .badges
            .iter()
            .any(|badge| badge == "unavailable")
    );
    assert!(
        row("limited")
            .badges
            .iter()
            .any(|badge| badge == "rate-limited until 18:00")
    );
    assert_eq!(row("mystery").price, None);
}

#[test]
fn current_marker_rules() {
    let catalog = Catalog::from_result(&catalogue::empty_provider_current());
    let rows = catalog.rows(&[], ZONE);
    let current: Vec<_> = rows
        .iter()
        .filter(|row| row.current)
        .map(|row| (row.provider_slug.as_str(), row.model.as_str()))
        .collect();
    assert_eq!(current, [("ollama-cloud", "glm-5")]);

    let missing = Catalog::from_result(&catalogue::missing_current_model());
    assert!(missing.rows(&[], ZONE).iter().all(|row| !row.current));
}

#[test]
fn account_labels_never_shown() {
    let catalog = Catalog::from_result(&catalogue::labelled_accounts());
    let rendered = catalog
        .rows(&[], ZONE)
        .iter()
        .map(row_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!rendered.contains("Private Label One"));
    assert!(!rendered.contains("acct-1"));
    assert!(rendered.contains("1 accounts") || rendered.contains("accounts"));
}
