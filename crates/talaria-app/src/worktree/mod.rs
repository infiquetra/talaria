//! Git worktrees created for a background session.

pub mod git;
pub mod include;
pub mod prepare;
pub mod record;
pub mod rollback;

pub use git::{GitError, GitOutput, GitRunner, git_env, git_prefix_args};
pub use prepare::{main_root_of, prepare_worktree};
pub use record::{WorktreeRecord, read_record, record_session, records_in};
pub use rollback::rollback;
