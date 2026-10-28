use serde::Serialize;
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceValue {
    Free,
    Usd(f64),
    Unknown,
}
impl PriceValue {
    pub(crate) fn dollars(&self) -> Option<f64> {
        match self {
            Self::Free => Some(0.0),
            Self::Usd(v) => Some(*v),
            Self::Unknown => None,
        }
    }
    pub(crate) fn text(&self) -> String {
        match self {
            Self::Free => "free".into(),
            Self::Usd(v) if *v >= 0.01 => format!("${v:.2}"),
            Self::Usd(v) => {
                let value = format!("{v:.12}");
                format!("${}", value.trim_end_matches('0').trim_end_matches('.'))
            }
            Self::Unknown => "unknown".into(),
        }
    }
    pub(crate) fn wire(&self) -> String {
        match self {
            Self::Free => "free".into(),
            Self::Usd(v) => format!("${v}"),
            Self::Unknown => String::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Price {
    pub input: PriceValue,
    pub output: PriceValue,
    pub cache: PriceValue,
}
impl Price {
    pub(crate) fn unknown() -> Self {
        Self {
            input: PriceValue::Unknown,
            output: PriceValue::Unknown,
            cache: PriceValue::Unknown,
        }
    }
    pub(crate) fn known(&self) -> bool {
        self.input.dollars().is_some() && self.output.dollars().is_some()
    }
    pub(crate) fn text(&self) -> String {
        if !self.known() {
            "unknown".into()
        } else if self.input == PriceValue::Free && self.output == PriceValue::Free {
            "free".into()
        } else {
            format!("{}/{}", self.input.text(), self.output.text())
        }
    }
}
pub fn parse_price(text: &str) -> PriceValue {
    if text == "free" {
        return PriceValue::Free;
    }
    let Some(s) = text.strip_prefix('$') else {
        return PriceValue::Unknown;
    };
    let mut pieces = s.split('.');
    let a = pieces.next().unwrap_or_default();
    let b = pieces.next();
    if a.is_empty()
        || !a.bytes().all(|c| c.is_ascii_digit())
        || b.is_some_and(|s| s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()))
        || pieces.next().is_some()
    {
        return PriceValue::Unknown;
    }
    s.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map_or(PriceValue::Unknown, PriceValue::Usd)
}
#[derive(Clone, Debug, PartialEq)]
pub struct PriceBook {
    profile: String,
    prices: BTreeMap<String, BTreeMap<String, Price>>,
}
impl PriceBook {
    pub fn from_snapshot(v: &serde_json::Value) -> Self {
        let mut prices = BTreeMap::new();
        if let Some(providers) = v["prices"].as_object() {
            for (p, models) in providers {
                let mut entries = BTreeMap::new();
                if let Some(models) = models.as_object() {
                    for (m, v) in models {
                        entries.insert(
                            m.clone(),
                            Price {
                                input: parse_price(v["input"].as_str().unwrap_or_default()),
                                output: parse_price(v["output"].as_str().unwrap_or_default()),
                                cache: parse_price(v["cache"].as_str().unwrap_or_default()),
                            },
                        );
                    }
                }
                prices.insert(p.clone(), entries);
            }
        }
        Self {
            profile: v["profile"].as_str().unwrap_or_default().into(),
            prices,
        }
    }
    pub fn price(&self, profile: &str, provider: &str, model: &str) -> Option<&Price> {
        if profile != self.profile {
            return None;
        }
        self.prices.get(provider)?.get(model)
    }
}
