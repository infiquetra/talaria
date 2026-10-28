#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MoneySource {
    Hermes,
    Pricing,
    Mixed,
    None,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Spend {
    pub tokens: u64,
    pub money_usd: f64,
    pub money_source: MoneySource,
    pub money_unknown_turns: u32,
    pub tokens_unknown_turns: u32,
}
impl Spend {
    pub fn money_text(&self) -> String {
        todo!()
    }
}
