pub mod error;
pub mod manager;
pub mod types;

pub use error::{Result, WorktreeError};
pub use manager::{parse_worktree_list_porcelain, WorktreeManager};
pub use types::{WorktreeInfo, WorktreeMetadata};
