//! Image type from the first bytes, and the unsupported-image rule.

use std::path::Path;

/// A type the gateway accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageType {
    /// Media type, such as `image/png`.
    pub mime: &'static str,
    /// Filename extension, without the dot. JPEG is `jpg`.
    pub ext: &'static str,
}

/// PNG, JPEG, GIF or WebP, when the bytes start with that signature.
#[must_use]
pub fn sniff_image(bytes: &[u8]) -> Option<ImageType> {
    let _ = bytes;
    unimplemented!()
}

/// No sniffed type, and the bytes or the extension are a known unsupported image.
#[must_use]
pub fn unsupported_image(bytes: &[u8], path: &Path) -> bool {
    let _ = (bytes, path);
    unimplemented!()
}
