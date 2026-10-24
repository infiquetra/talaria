//! A terminal emulator over the child's output.

pub struct Emulator {
    _parser: vt100::Parser,
}

impl Emulator {
    pub fn new(rows: u16, cols: u16) -> Emulator {
        Self {
            _parser: vt100::Parser::new(rows, cols, 0),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self._parser.process(bytes);
    }

    /// 1-based (row, column).
    pub fn cursor(&self) -> (u16, u16) {
        let (row, col) = self._parser.screen().cursor_position();
        (row + 1, col + 1)
    }

    pub fn rows_text(&self) -> Vec<String> {
        self._parser
            .screen()
            .rows(0, self._parser.screen().size().1)
            .collect()
    }

    pub fn has_non_blank_cell(&self) -> bool {
        self.rows_text().iter().any(|row| !row.trim().is_empty())
    }
}

/// True when some row, or joined adjacent rows, hold the first `k` characters of `probe`.
pub fn typed_prefix_visible(emulator: &Emulator, probe: &str, k: usize) -> bool {
    if k == 0 || k > probe.chars().count() {
        return false;
    }
    let prefix: String = probe.chars().take(k).collect();
    emulator.rows_text().join("").contains(&prefix)
}
