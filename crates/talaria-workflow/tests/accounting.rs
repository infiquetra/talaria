use talaria_workflow::schedule::{MoneySource, Spend};
#[test]
fn money_text_reports_unknown_turns() {
    let mut value = Spend {
        tokens: 10,
        money_usd: 0.0,
        money_source: MoneySource::None,
        money_unknown_turns: 1,
        tokens_unknown_turns: 0,
    };
    assert_eq!(value.money_text(), "unknown (1 unpriced turns)");
    value.money_usd = 0.01;
    value.money_source = MoneySource::Hermes;
    assert_eq!(value.money_text(), "$0.01 plus 1 unpriced turns");
    value.money_unknown_turns = 0;
    assert_eq!(value.money_text(), "$0.01");
    value.money_usd = 0.0;
    assert_eq!(value.money_text(), "$0.00");
}
