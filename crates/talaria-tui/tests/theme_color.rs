//! Colour parsing, contrast and palette mapping. Behavior contract: issue 227.

use talaria_tui::theme::color::{Rgb, ensure_contrast, parse_color, to_ansi256};

fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb(r, g, b)
}

#[test]
fn parse_color_accepts_hermes_forms_only() {
    let pale = rgb(0xaa, 0xbb, 0xcc);
    assert_eq!(parse_color("#abc"), Some(pale));
    assert_eq!(parse_color("abc"), Some(pale));
    assert_eq!(parse_color("#A1B2C3"), Some(rgb(0xa1, 0xb2, 0xc3)));
    assert_eq!(parse_color("FFD700"), Some(rgb(0xff, 0xd7, 0x00)));
    assert_eq!(parse_color(" #abcdef "), Some(rgb(0xab, 0xcd, 0xef)));

    let dark = rgb(1, 2, 3);
    assert_eq!(parse_color("rgb(1, 2, 3)"), Some(dark));
    assert_eq!(parse_color("RGB(1,2,3)"), Some(dark));
    assert_eq!(parse_color("rgb( 1 ,2 , 3 )"), Some(dark));
    assert_eq!(parse_color("rgba(1, 2, 3, 0.5)"), Some(dark));
    assert_eq!(parse_color("rgb(1,2,3) trailing"), Some(dark));
    assert_eq!(parse_color("rgb(1, 2, 3"), Some(dark));
    assert_eq!(parse_color("rgb(300, 0, 0)"), Some(rgb(255, 0, 0)));

    for text in [
        "red",
        "ansi256(3)",
        "#abcd",
        "",
        "rgb(1,2)",
        "#ggg",
        "rgb (1,2,3)",
    ] {
        assert_eq!(parse_color(text), None, "{text:?} is not a colour");
    }
}

#[test]
fn ensure_contrast_lifts_only_failing_colours() {
    assert_eq!(
        ensure_contrast(rgb(0xff, 0xf8, 0xdc), rgb(0xff, 0xff, 0xff), 1.18),
        rgb(0xe5, 0xdf, 0xc6)
    );
    assert_eq!(
        ensure_contrast(rgb(0x20, 0x20, 0x20), rgb(0x10, 0x10, 0x14), 1.45),
        rgb(0x37, 0x37, 0x37)
    );
    let kept = rgb(0xc0, 0xc0, 0xc0);
    assert_eq!(
        ensure_contrast(kept, rgb(0x1e, 0x1e, 0x1e), 1.45),
        kept,
        "above the floor stays byte-identical"
    );
}

#[test]
fn ansi256_picks_nearest_index() {
    assert_eq!(to_ansi256(rgb(0, 0, 0)), 16);
    assert_eq!(to_ansi256(rgb(255, 255, 255)), 231);
    assert_eq!(to_ansi256(rgb(0x5f, 0x87, 0xaf)), 67);
    assert_eq!(to_ansi256(rgb(0x80, 0x80, 0x80)), 244);
}
