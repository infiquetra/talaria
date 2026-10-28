use talaria_workflow::staffing::{PriceValue, parse_price};

fn hermes_format(value: f64) -> String {
    if value >= 0.01 {
        return format!("${value:.2}");
    }
    let decimal = format!("{value:.12}");
    format!("${}", decimal.trim_end_matches('0').trim_end_matches('.'))
}
#[test]
fn parses_hermes_price_strings() {
    for (input, expected) in [
        ("free", PriceValue::Free),
        ("$3.00", PriceValue::Usd(3.0)),
        ("$0.0005", PriceValue::Usd(0.0005)),
        ("$15", PriceValue::Usd(15.0)),
        ("", PriceValue::Unknown),
        ("?", PriceValue::Unknown),
        ("$", PriceValue::Unknown),
        ("$abc", PriceValue::Unknown),
        ("3.00", PriceValue::Unknown),
        ("$-1", PriceValue::Unknown),
        ("$NaN", PriceValue::Unknown),
        ("$inf", PriceValue::Unknown),
        (" $3.00", PriceValue::Unknown),
        ("$3.00 ", PriceValue::Unknown),
    ] {
        assert_eq!(parse_price(input), expected, "{input:?}");
    }
    assert_eq!(hermes_format(0.014), "$0.01");
    assert_eq!(parse_price(&hermes_format(0.014)), PriceValue::Usd(0.01));
    assert_eq!(parse_price(&hermes_format(1e-14)), PriceValue::Usd(0.0));
    for i in 1..=1_000 {
        let x = 1e-9 * (1e14_f64).powf(f64::from(i) / 1_000.0);
        let text = hermes_format(x);
        let printed = text[1..].parse::<f64>().unwrap();
        assert_eq!(parse_price(&text), PriceValue::Usd(printed));
        let digits = text.split_once('.').map_or(0, |(_, tail)| tail.len());
        let precision = if x >= 0.01 { 2 } else { digits.max(12) };
        assert!(
            (printed - x).abs()
                <= 0.5 * 10_f64.powi(-(precision as i32)) + f64::EPSILON * x.max(1.0)
        );
    }
}
