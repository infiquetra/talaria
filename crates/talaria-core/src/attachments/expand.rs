//! What a draft becomes when it is sent.

/// One staged image carried beside the prompt text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    /// Cache path returned by the gateway.
    pub path: String,
    /// Media type returned with that path.
    pub mime: String,
}

/// The draft after live labels have been expanded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expanded {
    /// Text sent to the gateway. Image labels stay in place.
    pub text: String,
    /// The draft as typed, shown in the user cell.
    pub display: String,
    /// Ready images, left to right.
    pub attachments: Vec<Attachment>,
    /// Built-in `@` references in `text`.
    pub reference_count: usize,
}

/// Why a draft cannot be sent yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpandError {
    /// An image label is still uploading.
    Pending { label: String },
    /// A paste label no longer has its text.
    MissingPaste { label: String },
    /// The draft holds more than ten live images.
    TooManyImages,
}
