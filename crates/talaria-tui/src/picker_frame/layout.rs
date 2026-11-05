//! Pure picker geometry shared with the application viewport calculation.

use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickerMode {
    #[default]
    SizeMessage,
    Compact,
    Below,
    Side,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PickerLayout {
    pub content: Rect,
    pub filter: Rect,
    pub chips: Rect,
    pub count: Rect,
    pub list: Rect,
    pub details: Rect,
    pub footer: Rect,
    pub mode: PickerMode,
}

impl PickerLayout {
    pub fn new(area: Rect) -> Self {
        let mode = if area.width < 40 || area.height < 10 {
            PickerMode::SizeMessage
        } else if area.width < 60 || area.height < 16 {
            PickerMode::Compact
        } else if area.width >= 112 && area.height >= 28 {
            PickerMode::Side
        } else {
            PickerMode::Below
        };
        if mode == PickerMode::SizeMessage {
            return Self {
                content: area,
                mode,
                ..Self::default()
            };
        }
        if mode == PickerMode::Compact {
            return Self {
                content: area,
                filter: Rect::new(area.x, area.y + 1, area.width, 1),
                list: Rect::new(area.x, area.y + 2, area.width, area.height - 4),
                footer: Rect::new(area.x, area.bottom() - 2, area.width, 2),
                mode,
                ..Self::default()
            };
        }
        let content = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2);
        let body_y = content.y + 3;
        let body_height = content.height - 5;
        let (list, details) = if mode == PickerMode::Side {
            let list_width = u16::try_from(u32::from(content.width) * 60 / 100).unwrap_or(0);
            (
                Rect::new(content.x, body_y, list_width, body_height),
                Rect::new(
                    content.x + list_width,
                    body_y,
                    content.width - list_width,
                    body_height,
                ),
            )
        } else {
            (
                Rect::new(content.x, body_y, content.width, body_height - 7),
                Rect::new(content.x, body_y + body_height - 7, content.width, 7),
            )
        };
        Self {
            content,
            filter: Rect::new(content.x, content.y, content.width, 1),
            chips: Rect::new(content.x, content.y + 1, content.width, 1),
            count: Rect::new(content.x, content.y + 2, content.width, 1),
            list,
            details,
            footer: Rect::new(content.x, content.bottom() - 2, content.width, 2),
            mode,
        }
    }
}

/// Number of list rows available after reserving the scroll indicator.
pub fn list_height(area: Rect) -> usize {
    usize::from(PickerLayout::new(area).list.height.saturating_sub(1))
}
