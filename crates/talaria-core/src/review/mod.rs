//! The review: diff, commit, push and draft pull request. Step 0 stubs. Issue #245.

pub mod draft;
pub mod model;
pub mod parse;
pub mod pr;
pub mod ship;
pub mod state;

pub use draft::{DRAFT_DIFF_MAX_CHARS, DraftError};
pub use model::{
    BranchHead, ChangedFile, DIFF_MAX_FILE_BYTES, DIFF_MAX_LINES, FileStatus, REVIEW_FILE_CAP,
    UNTRACKED_COUNT_MAX_BYTES,
};
pub use parse::{
    Numstat, ParseError, PushReport, count_lines, merge_counts, parse_name_status_z,
    parse_numstat_z, parse_push_porcelain, parse_status_v2,
};
pub use pr::{
    Checks, LabelTone, PR_QUERY_CAP, PR_QUERY_CHUNK, PrLabel, PrState, PrStatus, ReviewDecision,
    parse_pr_reply, pr_chunks, pr_label, pr_query,
};
pub use ship::{GhReady, ShipRefusal, can_commit, can_open_pr, can_push, refusal_text};
pub use state::{
    GitAction, Listing, RecordView, ReviewEffect, ReviewFocus, ReviewInput, ReviewKey, ReviewState,
    ReviewTarget, ReviewView, Scope,
};
