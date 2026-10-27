use proptest::prelude::*;
use ratatui::{buffer::Buffer, layout::Rect};
use talaria_core::mcp::{CardView, FieldView, McpTab, McpView, RowView};
use talaria_tui::mcp::draw_mcp;

fn forbidden(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, .. ProptestConfig::default() })]
    #[test]
    fn hostile_text_is_sanitised(text in prop::collection::vec(prop_oneof![any::<char>(), prop::sample::select(vec!['\u{0000}', '\u{001b}', '\u{007f}', '\u{0085}', '\u{202e}', '\u{2066}'])], 0..100)) {
        let text: String = text.into_iter().collect();
        let mut view = McpView {
            title: text.clone(), tab: McpTab::Servers, banner: Some(text.clone()), filter: text.clone(),
            placeholder: None, rows: vec![RowView { name: text.clone(), kind: text.clone(), label: text.clone(), dim: false }],
            selected: Some(0), status_line: Some(text.clone()), card: None, pending_line: Some(text.clone()),
            notice: Some(text.clone()), footer: text.clone(), secret_focus: None,
        };
        let area = Rect::new(0, 0, 120, 40);
        let mut buffer = Buffer::empty(area);
        draw_mcp(&view, &|_| 0, area, &mut buffer);
        for cell in buffer.content() {
            prop_assert!(!cell.symbol().chars().any(forbidden));
        }
        view.card = Some(CardView { title: text.clone(), lines: vec![text.clone()], fields: vec![FieldView { label: text.clone(), text: text.clone(), secret: None, error: Some(text.clone()), warning: Some(text.clone()), focused: true }], footer: text });
        buffer = Buffer::empty(area);
        draw_mcp(&view, &|_| 0, area, &mut buffer);
        for cell in buffer.content() {
            prop_assert!(!cell.symbol().chars().any(forbidden));
        }
    }
}
