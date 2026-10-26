use super::{OUTPUT_CAP_CHARS, ProcessOutput};
use crate::side::clean_output;

impl ProcessOutput {
    pub(super) fn append(&mut self, chunk: &str, now_ms: u64) {
        if chunk.is_empty() {
            return;
        }
        self.raw.push_str(chunk);
        self.chars += chunk.chars().count();
        self.last_write_ms = now_ms;
        if self.chars <= OUTPUT_CAP_CHARS {
            return;
        }
        let remove = self.chars - OUTPUT_CAP_CHARS;
        let offset = self
            .raw()
            .char_indices()
            .nth(remove)
            .map_or(self.raw().len(), |(at, _)| at);
        self.start += offset;
        self.chars -= remove;
        let newline = self
            .raw()
            .char_indices()
            .take(4_096)
            .enumerate()
            .find(|(_, (_, ch))| *ch == '\n');
        if let Some((count, (offset, _))) = newline {
            self.start += offset + 1;
            self.chars -= count + 1;
        }
        self.trimmed_front = true;
        // Retain a dead prefix so ordinary appends do not shift the full live buffer.
        // Compact only after enough front bytes have accumulated to amortize that copy.
        if self.start >= self.raw.len() / 2 {
            self.raw.drain(..self.start);
            self.start = 0;
        }
    }

    pub(super) fn merge(&mut self, tail: &str, now_ms: u64) {
        if tail.is_empty() {
            return;
        }
        if tail == self.fence || tail == self.raw() || self.raw().ends_with(tail) {
            self.fence = tail.to_owned();
            return;
        }
        if let Some(extra) = tail.strip_prefix(self.raw()) {
            let was_empty = self.chars == 0;
            self.append(extra, now_ms);
            if was_empty && tail.chars().count() == 32_768 {
                self.trimmed_front = true;
            }
        } else if !self.fence.is_empty() && tail.starts_with(&self.fence) {
            self.append(&tail[self.fence.len()..], now_ms);
        } else {
            self.raw.clear();
            self.start = 0;
            self.chars = 0;
            self.trimmed_front = tail.chars().count() == 32_768;
            self.append(tail, now_ms);
        }
        self.fence = tail.to_owned();
    }

    pub(super) fn tail(&self, max_chars: usize) -> String {
        let raw = self.raw();
        let start = raw
            .char_indices()
            .rev()
            .nth(max_chars.saturating_sub(1))
            .map_or(0, |(at, _)| at);
        let slice = if max_chars == 0 { "" } else { &raw[start..] };
        let slice = slice.find('\n').map_or(slice, |at| &slice[at + 1..]);
        clean_output(slice)
    }

    pub(super) fn last_clean_line(&self) -> String {
        let raw = self.raw();
        for line in raw.rsplit('\n') {
            let clean = clean_output(line);
            if !clean.trim().is_empty() {
                return clean.chars().take(200).collect();
            }
        }
        String::new()
    }
}
