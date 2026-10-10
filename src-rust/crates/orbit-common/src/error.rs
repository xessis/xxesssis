use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrbitError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Security violation: {0}")]
    Security(String),

    #[error("Tool execution failed: {0}")]
    ToolExecution(String),

    #[error("Diff application failed: {0}")]
    DiffApply(String),

    #[error("Context limit exceeded: requested {requested}, available {max}")]
    ContextLimit { requested: usize, max: usize },

    #[error("Invalid state transition: {0}")]
    InvalidState(String),
}

pub type Result<T> = std::result::Result<T, OrbitError>;
