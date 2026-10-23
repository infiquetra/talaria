pub const NAME: &str = "runtime.describe";
pub type Params = crate::schemas::RuntimeDescribeParams;
pub type Result = crate::schemas::RuntimeDescribeResult;

pub struct Marker;

impl crate::method::Method for Marker {
    const NAME: &'static str = NAME;
    type Params = crate::schemas::RuntimeDescribeParams;
    type Result = crate::schemas::RuntimeDescribeResult;
}
