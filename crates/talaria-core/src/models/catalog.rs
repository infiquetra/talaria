//! Provider and model rows in Hermes's order. Issue 230.

use hermes_contract::schemas::ModelOptionsResult;

/// Minutes east of UTC used to print a rate-limit clock. Tests pass a fixed offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayZone {
    pub minutes_east: i32,
}

/// One catalogue row a test or the picker can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogRow {
    pub kind: CatalogKind,
    pub text: String,
    pub dim: bool,
    pub provider_slug: String,
    pub provider_name: String,
    pub model: String,
    pub current: bool,
    pub favourite: bool,
    pub badges: Vec<String>,
    pub price: Option<String>,
    pub segments: Vec<String>,
}

/// Which group a row belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogKind {
    Header,
    Favourite,
    Model,
    NotSetUp,
}

/// A row the filter bench times. Headers are not included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterRow {
    pub provider_name: String,
    pub slug: String,
    pub model: String,
}

/// The catalogue Hermes returned, plus the favourite keys the caller supplies when listing rows.
#[derive(Clone, Debug)]
pub struct Catalog {
    result_model: String,
    result_provider: String,
}

impl Catalog {
    pub fn from_result(result: &ModelOptionsResult) -> Self {
        let _ = result;
        todo!()
    }

    pub fn display_name(&self, slug: &str) -> Option<&str> {
        let _ = (&self.result_model, &self.result_provider, slug);
        todo!()
    }

    /// Rows in picker order. `favourites` are `provider::model` keys in starring order.
    pub fn rows(&self, favourites: &[String], zone: DisplayZone) -> Vec<CatalogRow> {
        let _ = (self, favourites, zone);
        todo!()
    }
}

/// Indexes of the model rows whose provider name, slug and model contain every term.
pub fn filter_row_indexes(rows: &[FilterRow], filter: &str) -> Vec<usize> {
    let _ = (rows, filter);
    todo!()
}
