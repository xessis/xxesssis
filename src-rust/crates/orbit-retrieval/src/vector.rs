use crate::error::{Result, RetrievalError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Computes the cosine similarity between two float vectors.
/// Returns 0.0 if either vector has zero magnitude.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a_sq = 0.0f32;
    let mut norm_b_sq = 0.0f32;

    for (&ai, &bi) in a.iter().zip(b.iter()) {
        dot += ai * bi;
        norm_a_sq += ai * ai;
        norm_b_sq += bi * bi;
    }

    let denom = norm_a_sq.sqrt() * norm_b_sq.sqrt();
    if denom <= f32::EPSILON {
        0.0
    } else {
        (dot / denom).clamp(-1.0, 1.0)
    }
}

/// An entry in the vector index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEntry {
    pub id: String,
    pub embedding: Vec<f32>,
    pub norm: f32,
}

impl VectorEntry {
    pub fn new(id: impl Into<String>, embedding: Vec<f32>) -> Result<Self> {
        if embedding.is_empty() {
            return Err(RetrievalError::EmptyEmbedding);
        }

        let norm_sq: f32 = embedding.iter().map(|&x| x * x).sum();
        let norm = norm_sq.sqrt();

        Ok(Self {
            id: id.into(),
            embedding,
            norm,
        })
    }
}

/// Represents a scored vector search match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub id: String,
    pub score: f32,
}

/// In-memory vector index for storing chunk embeddings and performing cosine similarity search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorIndex {
    dimension: Option<usize>,
    entries: HashMap<String, VectorEntry>,
}

impl Default for VectorIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorIndex {
    /// Creates a new empty vector index.
    pub fn new() -> Self {
        Self {
            dimension: None,
            entries: HashMap::new(),
        }
    }

    /// Creates a vector index with a fixed dimension expectation.
    pub fn with_dimension(dimension: usize) -> Self {
        Self {
            dimension: Some(dimension),
            entries: HashMap::new(),
        }
    }

    pub fn dimension(&self) -> Option<usize> {
        self.dimension
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    pub fn get(&self, id: &str) -> Option<&[f32]> {
        self.entries.get(id).map(|e| e.embedding.as_slice())
    }

    /// Adds a single embedding vector to the index.
    pub fn add(&mut self, id: impl Into<String>, embedding: Vec<f32>) -> Result<()> {
        let id_str = id.into();
        if embedding.is_empty() {
            return Err(RetrievalError::EmptyEmbedding);
        }

        if let Some(expected) = self.dimension {
            if embedding.len() != expected {
                return Err(RetrievalError::DimensionMismatch {
                    expected,
                    actual: embedding.len(),
                });
            }
        } else {
            self.dimension = Some(embedding.len());
        }

        let entry = VectorEntry::new(id_str.clone(), embedding)?;
        self.entries.insert(id_str, entry);
        Ok(())
    }

    /// Adds multiple embeddings in batch.
    pub fn add_batch(&mut self, items: impl IntoIterator<Item = (String, Vec<f32>)>) -> Result<()> {
        for (id, embedding) in items {
            self.add(id, embedding)?;
        }
        Ok(())
    }

    /// Removes an embedding by ID. Returns true if the entry was present.
    pub fn remove(&mut self, id: &str) -> bool {
        self.entries.remove(id).is_some()
    }

    /// Clears the index.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Searches the vector index using cosine similarity against the query embedding.
    pub fn search(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<VectorSearchResult>> {
        if query_embedding.is_empty() {
            return Err(RetrievalError::EmptyEmbedding);
        }

        if let Some(expected) = self.dimension {
            if query_embedding.len() != expected {
                return Err(RetrievalError::DimensionMismatch {
                    expected,
                    actual: query_embedding.len(),
                });
            }
        }

        if top_k == 0 || self.entries.is_empty() {
            return Ok(Vec::new());
        }

        let query_norm_sq: f32 = query_embedding.iter().map(|&x| x * x).sum();
        let query_norm = query_norm_sq.sqrt();
        if query_norm <= f32::EPSILON {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(self.entries.len());

        for entry in self.entries.values() {
            if entry.norm <= f32::EPSILON {
                continue;
            }

            let mut dot = 0.0f32;
            for (&qi, &ei) in query_embedding.iter().zip(entry.embedding.iter()) {
                dot += qi * ei;
            }

            let denom = query_norm * entry.norm;
            let sim = if denom > f32::EPSILON {
                (dot / denom).clamp(-1.0, 1.0)
            } else {
                0.0
            };

            results.push(VectorSearchResult {
                id: entry.id.clone(),
                score: sim,
            });
        }

        // Sort descending by similarity; tie-break on ID for determinism
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });

        results.truncate(top_k);
        Ok(results)
    }
}
