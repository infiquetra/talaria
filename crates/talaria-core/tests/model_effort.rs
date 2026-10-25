//! Issue 230, scenario "parse and ladder".

use talaria_core::models::Effort;

#[test]
fn parse_and_ladder() {
    assert_eq!(Effort::parse("High"), Some(Effort::High));
    assert_eq!(Effort::parse(" xhigh "), Some(Effort::Xhigh));
    assert_eq!(Effort::parse("ULTRA"), Some(Effort::Ultra));
    assert_eq!(Effort::parse("none"), Some(Effort::None));
    assert_eq!(Effort::parse("NONE"), Some(Effort::None));
    assert_eq!(Effort::parse("false"), None);
    assert_eq!(Effort::parse("disabled"), None);
    assert_eq!(Effort::parse("hgih"), None);
    assert_eq!(Effort::parse(""), None);
    assert_eq!(Effort::parse("  "), None);

    assert_eq!(Effort::Medium.up(), Effort::High);
    assert_eq!(Effort::None.down(), Effort::None);
    assert_eq!(Effort::Ultra.up(), Effort::Ultra);
    assert_eq!(Effort::High.as_wire(), "high");
    assert_eq!(Effort::None.as_wire(), "none");
}
