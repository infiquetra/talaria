//! Methods the native session socket serves.

pub trait NativeMethod: crate::method::Method {}

impl NativeMethod for crate::methods::canonical::runtime_describe::Marker {}
impl NativeMethod for crate::methods::legacy::client_capabilities::Marker {}
