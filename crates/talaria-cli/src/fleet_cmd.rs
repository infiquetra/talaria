//! `talaria fleet list`. The stub prints nothing and exits 0.

/// Pad `text` with spaces until its terminal-cell width is `width`.
/// Wide characters count two cells. Combining characters count zero.
pub fn pad_cells(text: &str, width: usize) -> String {
    let _ = (text, width);
    todo!()
}

/// Run `talaria fleet ...`. The stub writes nothing.
pub fn run(args: &[String]) -> u8 {
    let _ = args;
    0
}
