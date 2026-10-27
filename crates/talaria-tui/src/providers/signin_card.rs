use talaria_core::providers::{CardKind, CardView};

pub(super) fn lines(card: &CardView, secret_len: usize) -> Vec<String> {
    let mut lines: Vec<_> = card
        .lines
        .iter()
        .skip(1)
        .map(|line| super::safe(line))
        .collect();
    if card.kind == CardKind::Key {
        lines.insert(lines.len().min(1), super::mask(secret_len));
    }
    lines
}
