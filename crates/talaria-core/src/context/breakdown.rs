//! The context breakdown: what fills the model's window (#234).

use hermes_contract::schemas::SessionContextBreakdownResult;

/// One category of context usage, in Hermes's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Category {
    pub id: String,
    pub label: String,
    pub tokens: u64,
}

/// How Hermes filed one context file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Loaded,
    Truncated,
    Flagged,
    Shadowed,
    Blocked,
    Empty,
    Unreadable,
    Suppressed,
    Other(String),
}

/// One context file, without its gateway-side path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextFile {
    pub label: String,
    pub est_tokens: u64,
    pub status: FileStatus,
}

/// Where the context figure came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextSource {
    ProviderUsage,
    LocalEstimate,
    ProviderUsagePlusEstimate,
    Other(String),
}

/// The parsed breakdown of one session's context window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Breakdown {
    pub model: String,
    pub categories: Vec<Category>,
    pub used: u64,
    pub max: u64,
    pub percent: u8,
    pub estimated_total: u64,
    pub estimated: bool,
    pub source: ContextSource,
    pub files: Vec<ContextFile>,
}

/// One cell of the hundred-cell usage grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridCell {
    Category(usize),
    Free,
}

/// One row of the category table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryRow {
    pub glyph: char,
    pub label: String,
    pub tokens: u64,
    pub tenths_of_percent: u32,
    pub category: Option<usize>,
}

/// The glyph of a free grid cell.
pub const FREE_GLYPH: char = '·';

/// The grid glyph for a category id, `▪` for any other id.
pub fn glyph_for(_id: &str) -> char {
    unimplemented!("#234")
}

impl Breakdown {
    /// Parse Hermes's breakdown result, clamping negatives and the percent.
    pub fn from_result(_result: &SessionContextBreakdownResult) -> Breakdown {
        unimplemented!("#234")
    }

    /// True before the agent exists: no categories and no maximum.
    pub fn is_prebuild(&self) -> bool {
        unimplemented!("#234")
    }

    /// Exactly 100 cells when `max > 0`, else empty.
    pub fn grid(&self) -> Vec<GridCell> {
        unimplemented!("#234")
    }

    /// One row per category in Hermes's order, then free space when `max > 0`.
    pub fn rows(&self) -> Vec<CategoryRow> {
        unimplemented!("#234")
    }
}
