#[path = "../tests/support/configure_ctx.rs"]
mod configure_ctx;

use std::{hint::black_box, time::Instant};
use talaria_core::configure::build_brief;

fn build_brief_825_keys() {
    let context = configure_ctx::generated_context();
    let request = "𐀀".repeat(4_000);
    let available =
        std::panic::catch_unwind(|| build_brief(&context, &request, configure_ctx::detect));
    assert!(available.is_ok(), "brief construction must be available");

    let mut samples = Vec::with_capacity(200);
    for index in 0..220 {
        let start = Instant::now();
        black_box(build_brief(&context, &request, configure_ctx::detect));
        if index >= 20 {
            samples.push(start.elapsed().as_micros());
        }
    }
    samples.sort_unstable();
    let median = (samples[99] + samples[100]) / 2;
    println!("median_us={median}");
    assert!(median <= 10_000, "brief construction exceeded its budget");
}

fn main() {
    build_brief_825_keys();
}
