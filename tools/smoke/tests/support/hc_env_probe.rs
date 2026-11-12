//! Test-only binary that invokes `HcCli::check` on the executable path given as its first
//! argument.

use std::path::Path;

fn main() {
    let arg = std::env::args_os()
        .nth(1)
        .expect("hc-env-probe requires an executable path argument");
    smoke::HcCli::new(Path::new(&arg)).check().unwrap();
}
