use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Detailed information about an Orbit agent git worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeInfo {
    /// Task identifier associated with the worktree
    pub task_id: String,
    /// Git branch checked out in this worktree
    pub branch: String,
    /// Filesystem path to the worktree directory
    pub path: PathBuf,
    /// Current HEAD commit hash, if available
    pub head_commit: Option<String>,
    /// Whether this worktree is currently locked
    pub is_locked: bool,
    /// Reason provided if locked
    pub lock_reason: Option<String>,
    /// Creation timestamp in epoch milliseconds, if tracked
    pub created_at_ms: Option<u64>,
}

/// Metadata stored on disk for tracking agent worktree state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeMetadata {
    pub task_id: String,
    pub branch: String,
    pub created_at_ms: u64,
    pub is_locked: bool,
    pub lock_reason: Option<String>,
}
