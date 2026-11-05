//! Pure grouping and presentation facts above the unchanged gateway catalogue.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListScope {
    #[default]
    All,
    Starred,
    Configured,
    SetUp,
}

impl ListScope {
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Starred => "Starred",
            Self::Configured => "Configured",
            Self::SetUp => "Set up",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrowseCounts {
    pub models: usize,
    pub providers: usize,
    pub starred: usize,
    pub configured: usize,
    pub setup: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrowseView<'a> {
    pub scope: ListScope,
    pub counts: BrowseCounts,
    pub details: &'a [String],
    pub details_open: bool,
    pub details_scroll: usize,
}
