//! Provider identity, setup methods and presentation facts for the shared picker.
use super::ProvidersState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowseKey {
    Provider(String),
    Endpoint(String),
    AddEndpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupMethod {
    Key,
    Account,
    Endpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderStatus {
    ListedReady,
    KeySaved,
    NeedsSetup,
    Checking,
    Unknown,
}

impl ProviderStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ListedReady => "listed ready",
            Self::KeySaved => "key saved",
            Self::NeedsSetup => "needs setup",
            Self::Checking => "checking",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowseRow {
    pub key: Option<BrowseKey>,
    pub label: String,
    pub status: Option<ProviderStatus>,
    pub methods: Vec<SetupMethod>,
    pub details: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrowseCounts {
    pub providers: usize,
    pub complete: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrowseView {
    pub rows: Vec<BrowseRow>,
    pub selected: Option<usize>,
    pub counts: BrowseCounts,
    pub details_open: bool,
    pub details_scroll: usize,
}

pub fn view(_state: &ProvidersState) -> BrowseView {
    BrowseView::default()
}
