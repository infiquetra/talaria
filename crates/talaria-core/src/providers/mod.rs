//! Providers overlay. Pure state: no client and no terminal.

mod accounts;
mod endpoints;
mod keys;
mod model;
mod text;

pub use accounts::{Account, AccountStatus, PollReply, PollStatus, SignInStart};
pub use endpoints::{Endpoint, EndpointCheck, EndpointDraft, EndpointList};
pub use keys::{EnvRow, KeyCheck};
pub use model::{
    ApiKeyChange, CardKind, CardView, Failure, OpenTarget, ProvidersAction, ProvidersEffect,
    ProvidersInput, ProvidersKey, ProvidersState, ProvidersTab, ProvidersView, next_deadline,
    parse_providers_args, update, view,
};
