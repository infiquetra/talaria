//! Background launch: a new session, optionally in a fresh git worktree.
//!
//! Step 0 leaves the pure functions unimplemented and the service sending nothing.

#![allow(dead_code)]

pub mod conn;
pub mod link;
pub mod machine;
pub mod outcome;
pub mod request;
pub mod service;
pub mod wire;

pub use conn::LaunchConn;
pub use link::{LaunchLink, LinkScope};
pub use machine::{
    CallFailure, CreatedSession, CwdCheck, DescribeSummary, END_REASON_ABANDONED, LaunchEffect,
    LaunchIds, LaunchInput, LaunchMachine, NewSessionSpec, Step, SubmitReply,
};
pub use outcome::{
    LaunchError, LaunchOutcome, Launched, exit_code, notice_text, render_json, render_text,
};
pub use request::{
    LaunchOrigin, LaunchRequest, MAX_PROMPT_BYTES, NameError, WorktreeNames, WorktreeWanted,
    explicit_slug, pick_names, slugify,
};
pub use service::{Busy, CancelToken, LaunchNotice, LaunchService, Stage, compare_cwd, run_launch};
