//! The hand-written method trait every generated marker implements.

pub trait Method {
    const NAME: &'static str;
    type Params: serde::Serialize;
    type Result: serde::de::DeserializeOwned;
}
