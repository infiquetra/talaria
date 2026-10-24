//! A terminal emulator over the child's output.

pub struct Emulator {
    _parser: vt100::Parser,
}

impl Emulator {
    pub fn new(rows: u16, cols: u16) -> Emulator {
        let _ = (rows, cols);
        todo!("bench")
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        let _ = bytes;
        todo!("bench")
    }

    /// 1-based (row, column).
    pub fn cursor(&self) -> (u16, u16) {
        todo!("bench")
    }

    pub fn rows_text(&self) -> Vec<String> {
        todo!("bench")
    }

    pub fn has_non_blank_cell(&self) -> bool {
        todo!("bench")
    }
}

/// True when some row, or joined adjacent rows, hold the first `k` characters of `probe`.
pub fn typed_prefix_visible(emulator: &Emulator, probe: &str, k: usize) -> bool {
    let _ = (emulator, probe, k);
    todo!("bench")
}
