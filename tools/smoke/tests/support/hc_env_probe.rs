//! Test-only binary that invokes `HcCli::check` on the executable path given as its first
//! argument.

use std::path::Path;

fn main() {
    let arg = std::env::args_os()
        .nth(1)
        .expect("hc-env-probe requires an executable path argument");
    let client = smoke::HcCli::new(Path::new(&arg));
    if let Some(text) = std::env::args().nth(2) {
        use smoke::Terminal;
        client
            .send_text(&smoke::PaneId("term_x".to_owned()), &text)
            .unwrap();
    } else {
        client.check().unwrap();
    }
}
