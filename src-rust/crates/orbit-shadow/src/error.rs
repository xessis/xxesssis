use orbit_common::OrbitError;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ShadowError {
    #[error("Buffer not found: {0}")]
    BufferNotFound(String),

    #[error("Fast apply diff failed for '{path}': {reason}")]
    DiffApplyFailed { path: String, reason: String },

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Bracket imbalance in '{path}': {message}")]
    BracketImbalance { path: String, message: String },

    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("Session is closed: {0}")]
    SessionClosed(String),

    #[error("Orbit error: {0}")]
    Orbit(String),
}

impl From<OrbitError> for ShadowError {
    fn from(err: OrbitError) -> Self {
        ShadowError::Orbit(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ShadowError>;
