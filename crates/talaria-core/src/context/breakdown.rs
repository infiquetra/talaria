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
pub fn glyph_for(id: &str) -> char {
    match id {
        "system_prompt" => '■',
        "tool_definitions" => '▣',
        "rules" => '▩',
        "skills" => '▤',
        "mcp" => '▥',
        "subagent_definitions" => '▦',
        "memory" => '▧',
        "conversation" => '▨',
        _ => '▪',
    }
}

/// The colour index of a category id: its place in the fixed order, eight for any other id.
pub fn category_index(id: &str) -> usize {
    match id {
        "system_prompt" => 0,
        "tool_definitions" => 1,
        "rules" => 2,
        "skills" => 3,
        "mcp" => 4,
        "subagent_definitions" => 5,
        "memory" => 6,
        "conversation" => 7,
        _ => 8,
    }
}

/// How Hermes filed one context file.
fn file_status(raw: &str) -> FileStatus {
    match raw {
        "loaded" => FileStatus::Loaded,
        "truncated" => FileStatus::Truncated,
        "flagged" => FileStatus::Flagged,
        "shadowed" => FileStatus::Shadowed,
        "blocked" => FileStatus::Blocked,
        "empty" => FileStatus::Empty,
        "unreadable" => FileStatus::Unreadable,
        "suppressed" => FileStatus::Suppressed,
        other => FileStatus::Other(other.to_owned()),
    }
}

/// Where the context figure came from.
fn context_source(raw: &str) -> ContextSource {
    match raw {
        "provider_usage" => ContextSource::ProviderUsage,
        "local_estimate" => ContextSource::LocalEstimate,
        "provider_usage_plus_estimate" => ContextSource::ProviderUsagePlusEstimate,
        other => ContextSource::Other(other.to_owned()),
    }
}

/// A negative count is zero.
fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

impl Breakdown {
    /// Parse Hermes's breakdown result, clamping negatives and the percent.
    pub fn from_result(result: &SessionContextBreakdownResult) -> Breakdown {
        let max = count(result.context_max);
        let used = count(result.context_used);
        let used = if max > 0 { used.min(max) } else { used };
        let percent = u8::try_from(result.context_percent.clamp(0, 100)).unwrap_or(0);
        Breakdown {
            model: result.model.clone(),
            categories: result
                .categories
                .iter()
                .map(|category| Category {
                    id: category.id.clone(),
                    label: category.label.clone(),
                    tokens: count(category.tokens),
                })
                .collect(),
            used,
            max,
            percent,
            estimated_total: count(result.estimated_total),
            estimated: result.context_estimated,
            source: context_source(&result.context_source),
            files: result
                .context_files
                .iter()
                .map(|file| ContextFile {
                    label: file.label.clone(),
                    est_tokens: count(file.est_tokens),
                    status: file_status(&file.status),
                })
                .collect(),
        }
    }

    /// True before the agent exists: no categories and no maximum.
    pub fn is_prebuild(&self) -> bool {
        self.categories.is_empty() && self.max == 0
    }

    /// Exactly 100 cells when `max > 0`, else empty.
    pub fn grid(&self) -> Vec<GridCell> {
        if self.max == 0 {
            return Vec::new();
        }
        let mut cells: Vec<GridCell> = Vec::with_capacity(100);
        for (index, category) in self.categories.iter().enumerate() {
            let mut share = (category.tokens * 100 + self.max / 2) / self.max;
            if share == 0 && category.tokens > 0 {
                share = 1;
            }
            for _ in 0..share {
                if cells.len() == 100 {
                    return cells;
                }
                cells.push(GridCell::Category(index));
            }
        }
        while cells.len() < 100 {
            cells.push(GridCell::Free);
        }
        cells
    }

    /// One row per category in Hermes's order, then free space when `max > 0`.
    pub fn rows(&self) -> Vec<CategoryRow> {
        let denominator = if self.max > 0 {
            self.max
        } else {
            self.estimated_total
        };
        let mut rows: Vec<CategoryRow> = self
            .categories
            .iter()
            .enumerate()
            .map(|(index, category)| CategoryRow {
                glyph: glyph_for(&category.id),
                label: category.label.clone(),
                tokens: category.tokens,
                tenths_of_percent: tenths(category.tokens, denominator),
                category: Some(index),
            })
            .collect();
        if self.max > 0 {
            let free = self.max.saturating_sub(self.estimated_total);
            rows.push(CategoryRow {
                glyph: FREE_GLYPH,
                label: "Free space".to_owned(),
                tokens: free,
                tenths_of_percent: tenths(free, self.max),
                category: None,
            });
        }
        rows
    }
}

/// The share of a total in tenths of a percent, rounded half up.
fn tenths(tokens: u64, denominator: u64) -> u32 {
    if denominator == 0 {
        return 0;
    }
    u32::try_from((tokens * 1000 + denominator / 2) / denominator).unwrap_or(u32::MAX)
}
