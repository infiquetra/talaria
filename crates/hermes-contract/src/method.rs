//! A method's wire name and generated request and response types.

pub trait Method {
    const NAME: &'static str;
    type Params: serde::Serialize;
    type Result: serde::de::DeserializeOwned;
}
