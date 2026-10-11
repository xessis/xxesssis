use thiserror::Error;

#[derive(Error, Debug)]
pub enum RetrievalError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("Empty embedding vector")]
    EmptyEmbedding,

    #[error("Empty query")]
    EmptyQuery,

    #[error("Chunk not found: {0}")]
    ChunkNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Orbit common error: {0}")]
    OrbitCommon(#[from] orbit_common::OrbitError),
}

impl From<RetrievalError> for orbit_common::OrbitError {
    fn from(err: RetrievalError) -> Self {
        match err {
            RetrievalError::Io(e) => orbit_common::OrbitError::Io(e),
            RetrievalError::Serialization(e) => orbit_common::OrbitError::Serialization(e),
            RetrievalError::OrbitCommon(e) => e,
            other => orbit_common::OrbitError::ToolExecution(other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, RetrievalError>;
