//! The live region's layout: the height cap and how the streaming tail gives way.

mod support;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_tui::live::{LiveSection, LiveView, max_live_height};

/// A section that asks for a fixed number of rows at any width and draws nothing.
struct Fixed(u16);

impl LiveSection for Fixed {
    fn height(&self, _width: u16) -> u16 {
        self.0
    }

    fn render(&self, _area: Rect, _buf: &mut Buffer) {}
}

/// A section that asks for three rows at width 40 and a huge number anywhere else, so the layout
/// must pass its own width through.
struct OnlyAtWidthForty;

impl LiveSection for OnlyAtWidthForty {
    fn height(&self, width: u16) -> u16 {
        if width == 40 { 3 } else { 999 }
    }

    fn render(&self, _area: Rect, _buf: &mut Buffer) {}
}

/// Lay out sections of the given heights at width 80 and return each rectangle as
/// `(y, height)`; every rectangle must start at column 0 and be 80 wide.
fn laid_out(heights: &[u16], max: u16) -> Vec<(u16, u16)> {
    let sections: Vec<Fixed> = heights.iter().map(|&height| Fixed(height)).collect();
    let view = LiveView {
        sections: sections.iter().map(|s| s as &dyn LiveSection).collect(),
    };
    let rects = view.layout(80, max);
    assert_eq!(rects.len(), heights.len(), "one rectangle per section");
    for rect in &rects {
        assert_eq!((rect.x, rect.width), (0, 80), "full width from column 0");
    }
    rects.iter().map(|rect| (rect.y, rect.height)).collect()
}

#[test]
fn layout_caps_height_and_shrinks_the_tail() {
    // Rows 24: the cap is 22. A 500 row tail, a 3 row composer and a 1 row status line: the
    // later sections keep their height and the tail gets the rest.
    assert_eq!(max_live_height(24), 22);
    assert_eq!(
        laid_out(&[500, 3, 1], max_live_height(24)),
        vec![(0, 18), (18, 3), (21, 1)]
    );

    // Rows 8: the cap is 6. The later sections alone are 9 rows, so the tail gets nothing and
    // rows go to the sections from the last upward: status 1, composer 3, card 2.
    assert_eq!(max_live_height(8), 6);
    assert_eq!(
        laid_out(&[500, 5, 3, 1], max_live_height(8)),
        vec![(0, 0), (0, 2), (2, 3), (5, 1)]
    );

    // The cap itself.
    assert_eq!(max_live_height(2), 0);
    assert_eq!(max_live_height(0), 0);
    assert_eq!(max_live_height(1), 0);
    assert_eq!(max_live_height(3), 1);
    assert_eq!(max_live_height(4), 2);
    assert_eq!(max_live_height(100), 98);

    // Everything fits: sections stack at their full height, up to and including the cap.
    assert_eq!(laid_out(&[2, 3, 1], 22), vec![(0, 2), (2, 3), (5, 1)]);
    assert_eq!(laid_out(&[10, 8, 4], 22), vec![(0, 10), (10, 8), (18, 4)]);
    assert_eq!(laid_out(&[1], 22), vec![(0, 1)]);

    // One row over the cap, the later sections still fit: the tail loses exactly that row.
    assert_eq!(laid_out(&[10, 8, 5], 22), vec![(0, 9), (9, 8), (17, 5)]);

    // The later sections exactly fill the cap: the tail is left with nothing.
    assert_eq!(laid_out(&[500, 4, 2], 6), vec![(0, 0), (0, 4), (4, 2)]);

    // The later sections are one row over the cap: the first later section loses that row.
    assert_eq!(laid_out(&[500, 4, 3], 6), vec![(0, 0), (0, 3), (3, 3)]);

    // A single oversized section is cut to the cap.
    assert_eq!(laid_out(&[500], 22), vec![(0, 22)]);

    // Later sections far above the cap: the last ones keep their rows.
    assert_eq!(
        laid_out(&[9, 9, 9, 9], 10),
        vec![(0, 0), (0, 0), (0, 1), (1, 9)]
    );
}

#[test]
fn layout_asks_each_section_at_the_given_width() {
    let sections = [OnlyAtWidthForty, OnlyAtWidthForty];
    let view = LiveView {
        sections: sections.iter().map(|s| s as &dyn LiveSection).collect(),
    };
    let rects = view.layout(40, 22);
    assert_eq!(rects, vec![Rect::new(0, 0, 40, 3), Rect::new(0, 3, 40, 3)]);
}
