//! Orbit retrieval engine: codebase indexing & hybrid retrieval (BM25 + vector cosine).

pub mod bm25;
pub mod chunker;
pub mod engine;
pub mod error;
pub mod hybrid;
pub mod vector;

pub use bm25::{tokenize, Bm25Index, Bm25Params, Bm25SearchResult};
pub use chunker::{ChunkerConfig, CodeChunk, CodeChunker};
pub use engine::{EngineStats, IndexingStats, RetrievalEngine};
pub use error::{Result, RetrievalError};
pub use hybrid::{FusionStrategy, HybridConfig, HybridSearchResult, HybridSearcher};
pub use vector::{cosine_similarity, VectorIndex, VectorSearchResult};
