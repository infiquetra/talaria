pub const NAME: &str = "client.capabilities";
pub type Params = crate::schemas::ClientCapabilitiesParams;
pub type Result = crate::schemas::ClientCapabilitiesResult;

pub struct Marker;

impl crate::method::Method for Marker {
    const NAME: &'static str = NAME;
    type Params = crate::schemas::ClientCapabilitiesParams;
    type Result = crate::schemas::ClientCapabilitiesResult;
}
