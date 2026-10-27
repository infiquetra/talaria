use talaria_core::providers::ProvidersKey;
use zeroize::{Zeroize, Zeroizing};

const CAPACITY: usize = 65_536;

/// An editable secret whose storage is erased when cleared or dropped.
pub struct SecretField(Zeroizing<String>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretOutcome {
    Edited { empty: bool },
    Submit,
    Clear,
    NotHandled,
}

impl Default for SecretField {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretField {
    #[must_use]
    pub fn new() -> Self {
        Self(Zeroizing::new(String::with_capacity(CAPACITY)))
    }

    pub fn handle_key(&mut self, key: &ProvidersKey) -> SecretOutcome {
        match key {
            ProvidersKey::Char(ch) if !ch.is_control() => {
                if self.0.len() + ch.len_utf8() <= CAPACITY {
                    self.0.push(*ch);
                    SecretOutcome::Edited { empty: false }
                } else {
                    SecretOutcome::NotHandled
                }
            }
            ProvidersKey::Backspace if !self.0.is_empty() => {
                let start = self.0.char_indices().next_back().map_or(0, |(i, _)| i);
                self.0[start..].zeroize();
                self.0.truncate(start);
                SecretOutcome::Edited {
                    empty: self.0.is_empty(),
                }
            }
            ProvidersKey::Enter if !self.0.is_empty() => SecretOutcome::Submit,
            ProvidersKey::Clear | ProvidersKey::CtrlU => {
                self.clear();
                SecretOutcome::Clear
            }
            _ => SecretOutcome::NotHandled,
        }
    }

    pub fn handle_paste(&mut self, text: &str) -> SecretOutcome {
        let text = text.trim();
        let chars = text
            .chars()
            .filter(|ch| *ch != '\r' && *ch != '\n' && !ch.is_control());
        let bytes: usize = chars.clone().map(char::len_utf8).sum();
        if bytes == 0 || self.0.len().saturating_add(bytes) > CAPACITY {
            return SecretOutcome::NotHandled;
        }
        self.0.extend(chars);
        SecretOutcome::Edited { empty: false }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.chars().count()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn value(&self) -> &Zeroizing<String> {
        &self.0
    }

    pub fn clear(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::{SecretField, SecretOutcome};
    use talaria_core::providers::ProvidersKey;

    #[test]
    fn edits_and_erases_without_reallocating() {
        let mut field = SecretField::new();
        assert_eq!(
            field.handle_paste("  sk-a b\r\n"),
            SecretOutcome::Edited { empty: false }
        );
        assert_eq!(field.value().as_str(), "sk-a b");
        assert_eq!(
            field.handle_key(&ProvidersKey::Backspace),
            SecretOutcome::Edited { empty: false }
        );
        assert_eq!(field.len(), 5);
        assert_eq!(field.handle_key(&ProvidersKey::CtrlU), SecretOutcome::Clear);
        assert_eq!(
            field.handle_key(&ProvidersKey::Enter),
            SecretOutcome::NotHandled
        );
        assert_eq!(field.value().capacity(), 65_536);
        assert_eq!(
            field.handle_paste(&"x".repeat(65_537)),
            SecretOutcome::NotHandled
        );
        assert_eq!(field.value().capacity(), 65_536);
    }
}
