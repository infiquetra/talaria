//! Fleet registry: rows, the needs-you queue, and the pure reducer.
//!
//! No clock, no input or output, and no terminal library.

mod queue;
mod registry;
mod row;
mod subscribed;

pub use crate::attention::{AlertKind, NeedSummary};
pub use crate::directory::rows::RowKey;
pub use queue::{NeedDetail, NeedId, NeedItem, NeedSlot};
pub use registry::{
    CallError, FleetCounts, FleetEffect, FleetInput, FleetRegistry, PolledApproval, ProfileStatus,
    update,
};
pub use row::{FleetRow, LiveItem, LiveStatus, Outcome, RosterRow, RowState};
pub use subscribed::{SubscribedNeed, SubscribedTurn, SubscribedView};
