use talaria_core::providers::CardView;

pub(super) fn lines(card: &CardView, secret_len: usize) -> Vec<String> {
    card.lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let mut text = super::safe(line);
            if index == 2 && secret_len > 0 {
                text.push_str(&format!(" {}", super::mask(secret_len)));
            }
            text
        })
        .collect()
}
