//! Filter budget for the model-role picker.

use std::hint::black_box;
use std::time::Instant;

use talaria_core::model_roles::{
    PickerStep, RouteCatalog, RoutePickerView, RouteProvider, RouteRow, filter_routes,
};

const PROVIDERS: usize = 12;
const MODELS: usize = 300;
const WARMUP: usize = 20;
const RUNS: usize = 200;
const LIMIT_US: u128 = 2_000;

fn catalog() -> RouteCatalog {
    let mut providers = Vec::with_capacity(PROVIDERS);
    let mut remaining = MODELS;
    for index in 0..PROVIDERS {
        let count = if index + 1 == PROVIDERS {
            remaining
        } else {
            MODELS / PROVIDERS
        };
        remaining -= count;
        providers.push(RouteProvider {
            slug: format!("provider-{index}"),
            name: format!("Provider {index}"),
            models: (0..count)
                .map(|model| format!("model-{index}-{model}"))
                .collect(),
            authenticated: true,
            is_user_defined: false,
            api_url: String::new(),
        });
    }
    RouteCatalog { providers }
}

fn picker(rows: Vec<RouteRow>, filter: &str) -> RoutePickerView {
    RoutePickerView {
        title: "Choose a model".to_owned(),
        filter: filter.to_owned(),
        rows: rows
            .into_iter()
            .map(|row| {
                let text = match row {
                    RouteRow::Provider { name, .. } => name,
                    RouteRow::Model { model, .. } => model,
                };
                talaria_core::model_roles::RowView {
                    text,
                    selected: false,
                    dimmed: false,
                    marker: None,
                }
            })
            .collect(),
        status: None,
    }
}

fn main() {
    let catalog = catalog();
    let step = PickerStep::Models {
        provider: "provider-0".to_owned(),
    };
    let filter = "mod";
    for _ in 0..WARMUP {
        let rows = filter_routes(&catalog, &step, filter);
        black_box(picker(rows, filter));
    }
    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let started = Instant::now();
        let rows = filter_routes(&catalog, &step, filter);
        let view = picker(rows, filter);
        samples.push(started.elapsed().as_micros());
        black_box(view);
    }
    samples.sort_unstable();
    let median = (samples[RUNS / 2 - 1] + samples[RUNS / 2]) / 2;
    println!("median_us={median}");
    if median > LIMIT_US {
        std::process::exit(1);
    }
}
