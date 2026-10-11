use thiserror::Error;

#[derive(Error, Debug)]
pub enum WorktreeError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Git error: {0}")]
    Git(String),

    #[error("Invalid task ID '{task_id}': {reason}")]
    InvalidTaskId { task_id: String, reason: String },

    #[error("Invalid branch name '{branch}': {reason}")]
    InvalidBranch { branch: String, reason: String },

    #[error("Worktree for task '{0}' not found")]
    NotFound(String),

    #[error("Worktree for task '{0}' already exists")]
    AlreadyExists(String),

    #[error("Worktree for task '{task_id}' is locked: {reason:?}")]
    Locked {
        task_id: String,
        reason: Option<String>,
    },

    #[error("Repository error: {0}")]
    Repository(String),

    #[error("Common error: {0}")]
    Common(#[from] orbit_common::OrbitError),
}

pub type Result<T> = std::result::Result<T, WorktreeError>;
