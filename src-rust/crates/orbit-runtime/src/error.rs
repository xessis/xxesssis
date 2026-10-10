use orbit_common::OrbitError;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Orbit error: {0}")]
    Orbit(#[from] OrbitError),

    #[error("Invalid state transition from {from:?} to {to:?}: {reason}")]
    InvalidStateTransition {
        from: String,
        to: String,
        reason: String,
    },

    #[error("Thread actor channel closed or terminated")]
    ActorDisconnected,

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Checkpoint not found: {0}")]
    CheckpointNotFound(Uuid),

    #[error("No checkpoint available to undo")]
    NoUndoAvailable,

    #[error("No checkpoint available to redo")]
    NoRedoAvailable,

    #[error("Pending approval not found for call ID: {0}")]
    ApprovalNotFound(String),

    #[error("Tool execution failed: {0}")]
    ToolExecution(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, RuntimeError>;

impl From<RuntimeError> for OrbitError {
    fn from(err: RuntimeError) -> Self {
        match err {
            RuntimeError::Orbit(e) => e,
            RuntimeError::InvalidStateTransition { from, to, reason } => {
                OrbitError::InvalidState(format!("{from} -> {to}: {reason}"))
            }
            RuntimeError::ActorDisconnected => {
                OrbitError::ToolExecution("Actor disconnected".into())
            }
            RuntimeError::Cancelled => OrbitError::ToolExecution("Operation cancelled".into()),
            RuntimeError::CheckpointNotFound(id) => {
                OrbitError::ToolExecution(format!("Checkpoint not found: {id}"))
            }
            RuntimeError::NoUndoAvailable => {
                OrbitError::InvalidState("No checkpoint available to undo".into())
            }
            RuntimeError::NoRedoAvailable => {
                OrbitError::InvalidState("No checkpoint available to redo".into())
            }
            RuntimeError::ApprovalNotFound(id) => {
                OrbitError::InvalidState(format!("Pending approval not found: {id}"))
            }
            RuntimeError::ToolExecution(msg) => OrbitError::ToolExecution(msg),
            RuntimeError::Io(e) => OrbitError::Io(e),
            RuntimeError::Serialization(e) => OrbitError::Serialization(e),
        }
    }
}
