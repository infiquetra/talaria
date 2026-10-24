//! The resume list. Rows, the prompt, and the out-of-range hint go to `output`.

use std::io::{BufRead, Write};

use talaria_app::launch::ListRow;

pub fn pick(
    rows: &[ListRow],
    input: &mut impl BufRead,
    output: &mut impl Write,
    profile: &str,
) -> std::io::Result<Option<String>> {
    let _ = (rows, input, output, profile);
    todo!("binary")
}
