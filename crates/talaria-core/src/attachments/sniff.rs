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
    let (mime, ext) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        ("image/png", "png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        ("image/jpeg", "jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        ("image/gif", "gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        ("image/webp", "webp")
    } else {
        return None;
    };
    Some(ImageType { mime, ext })
}

/// No sniffed type, and the bytes or the extension are a known unsupported image.
#[must_use]
pub fn unsupported_image(bytes: &[u8], path: &Path) -> bool {
    sniff_image(bytes).is_none()
        && (bytes.starts_with(b"BM")
            || path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|ext| {
                    ["bmp", "svg", "tif", "tiff", "heic", "heif", "ico"]
                        .iter()
                        .any(|unsupported| ext.eq_ignore_ascii_case(unsupported))
                }))
}
