//! Filter time for the slash palette. Issue #223.
//!
//! Prints `entries=<n> p95_us=<n>` for 2,000 and 5,000 entries. Exits non-zero when the
//! 2,000-entry 95th percentile is over 2,000 microseconds. The 5,000 line is informational.

use std::collections::HashMap;
use std::time::Instant;

use hermes_contract::schemas::{
    ArgumentMode, CommandCatalogMeta, CommandCategory, CommandsCatalogResult, SkillCatalogEntry,
};
use talaria_core::commands::{
    CatalogStatus, CommandCatalog, CommandsState, LocalCommandTable, PaletteContext, PaletteInput,
    PaletteState, palette_update as update,
};

fn main() {
    let mut failed = false;
    for entries in [2_000_usize, 5_000] {
        let p95 = measure(entries);
        println!("entries={entries} p95_us={p95}");
        if entries == 2_000 && p95 > 2_000 {
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}

fn measure(entries: usize) -> u128 {
    let catalog = CommandCatalog::from_result(&catalogue(entries));
    let local = LocalCommandTable::builtin();
    let status = CatalogStatus::Ready;
    let refused = CommandsState::default().refused;
    let ctx = PaletteContext {
        catalog: Some(&catalog),
        catalog_status: &status,
        local: &local,
        refused: &refused,
    };
    let mut open = PaletteState::closed();
    open.open = true;
    let _ = update(
        &mut open,
        PaletteInput::DraftChanged {
            text: "/".to_string(),
            caret: 1,
        },
        &ctx,
    );
    for _ in 0..100 {
        let mut state = open.clone();
        let _ = update(&mut state, draft(), &ctx);
    }
    let mut samples = Vec::with_capacity(1_000);
    for _ in 0..1_000 {
        let mut state = open.clone();
        let started = Instant::now();
        let _ = update(&mut state, draft(), &ctx);
        samples.push(started.elapsed().as_micros());
    }
    samples.sort_unstable();
    samples[949]
}

fn draft() -> PaletteInput {
    PaletteInput::DraftChanged {
        text: "/re".to_string(),
        caret: 3,
    }
}

fn catalogue(entries: usize) -> CommandsCatalogResult {
    let commands_n = 40.min(entries);
    let categories = ["Session", "Configuration", "Info", "Tools & Skills"];
    let mut pairs = Vec::new();
    let mut commands = HashMap::new();
    let mut grouped: Vec<Vec<Vec<String>>> = vec![Vec::new(); categories.len()];
    for index in 0..commands_n {
        let name = format!("/fixture-cmd-{index}");
        let description = format!("Command {index} (usage: {name} [arg])");
        let pair = vec![name.clone(), description];
        grouped[index % categories.len()].push(pair.clone());
        pairs.push(pair);
        commands.insert(
            name,
            CommandCatalogMeta {
                argument_mode: Some(ArgumentMode::Text),
                desktop: None,
                desktop_subcommands: None,
            },
        );
    }
    let mut category_rows = Vec::new();
    for (index, name) in categories.iter().enumerate() {
        category_rows.push(CommandCategory {
            name: (*name).to_string(),
            pairs: grouped[index].clone(),
        });
    }
    let origins = ["hub", "bundled", "local"];
    let mut skills = HashMap::new();
    for index in 0..(entries - commands_n) {
        let name = format!("/fixture-skill-{index}");
        pairs.push(vec![name.clone(), format!("Skill number {index}")]);
        skills.insert(
            name,
            SkillCatalogEntry {
                origin: origins[index % origins.len()].to_string(),
                usage: (index % 7) as i64,
            },
        );
    }
    CommandsCatalogResult {
        canon: HashMap::new(),
        categories: category_rows,
        commands,
        pairs,
        plugin_commands: HashMap::new(),
        plugin_discovery_error: String::new(),
        skill_count: skills.len() as i64,
        skills,
        sub: HashMap::new(),
        warning: String::new(),
    }
}
