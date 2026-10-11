use crate::bm25::{Bm25Index, Bm25SearchResult};
use crate::chunker::CodeChunk;
use crate::error::Result;
use crate::vector::{VectorIndex, VectorSearchResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Strategy used to combine BM25 and vector search results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FusionStrategy {
    /// Weighted linear combination of normalized scores:
    /// score = bm25_weight * norm(bm25) + vector_weight * norm(vector)
    Weighted {
        bm25_weight: f32,
        vector_weight: f32,
    },
    /// Reciprocal Rank Fusion (RRF):
    /// score = sum(1.0 / (rrf_k + rank)) for each rank list
    ReciprocalRankFusion { rrf_k: usize },
}

impl Default for FusionStrategy {
    fn default() -> Self {
        Self::Weighted {
            bm25_weight: 0.4,
            vector_weight: 0.6,
        }
    }
}

/// Configuration for the hybrid search engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridConfig {
    pub strategy: FusionStrategy,
    /// Multiplier on top_k to determine candidate pool size before fusion (default: 3)
    pub candidate_pool_multiplier: usize,
    /// Minimum candidates to fetch per modality (default: 20)
    pub min_candidates: usize,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            strategy: FusionStrategy::default(),
            candidate_pool_multiplier: 3,
            min_candidates: 20,
        }
    }
}

impl HybridConfig {
    pub fn weighted(bm25_weight: f32, vector_weight: f32) -> Self {
        Self {
            strategy: FusionStrategy::Weighted {
                bm25_weight,
                vector_weight,
            },
            candidate_pool_multiplier: 3,
            min_candidates: 20,
        }
    }

    pub fn rrf(rrf_k: usize) -> Self {
        Self {
            strategy: FusionStrategy::ReciprocalRankFusion { rrf_k },
            candidate_pool_multiplier: 3,
            min_candidates: 20,
        }
    }
}

/// A retrieved chunk with fused and modality-specific scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResult {
    pub chunk_id: String,
    pub score: f32,
    pub bm25_score: Option<f32>,
    pub vector_score: Option<f32>,
    pub bm25_rank: Option<usize>,
    pub vector_rank: Option<usize>,
    pub chunk: Option<CodeChunk>,
}

/// Hybrid searcher coordinating lexical BM25 and semantic Vector indices.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HybridSearcher {
    pub bm25: Bm25Index,
    pub vector: VectorIndex,
    pub config: HybridConfig,
}

impl HybridSearcher {
    pub fn new(bm25: Bm25Index, vector: VectorIndex, config: HybridConfig) -> Self {
        Self {
            bm25,
            vector,
            config,
        }
    }

    /// Indexes a code chunk with an optional embedding vector.
    pub fn index_chunk(&mut self, chunk: CodeChunk, embedding: Option<Vec<f32>>) -> Result<()> {
        let chunk_id = chunk.id.clone();
        self.bm25.add_chunk(chunk);
        if let Some(emb) = embedding {
            self.vector.add(chunk_id, emb)?;
        }
        Ok(())
    }

    /// Removes a chunk by ID from both indices.
    pub fn remove_chunk(&mut self, chunk_id: &str) -> bool {
        let bm25_removed = self.bm25.remove_chunk(chunk_id);
        let vec_removed = self.vector.remove(chunk_id);
        bm25_removed || vec_removed
    }

    /// Clears both indices.
    pub fn clear(&mut self) {
        self.bm25.clear();
        self.vector.clear();
    }

    /// Performs lexical search using BM25 only.
    pub fn search_bm25(&self, query_text: &str, top_k: usize) -> Vec<HybridSearchResult> {
        let bm25_results = self.bm25.search(query_text, top_k);
        let max_score = bm25_results.first().map_or(1.0, |r| r.score.max(1.0));

        bm25_results
            .into_iter()
            .enumerate()
            .map(|(idx, res)| {
                let norm_score = res.score / max_score;
                HybridSearchResult {
                    chunk_id: res.chunk_id,
                    score: norm_score,
                    bm25_score: Some(res.score),
                    vector_score: None,
                    bm25_rank: Some(idx + 1),
                    vector_rank: None,
                    chunk: Some(res.chunk),
                }
            })
            .collect()
    }

    /// Performs semantic vector search only.
    pub fn search_vector(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        let vec_results = self.vector.search(query_embedding, top_k)?;
        let results = vec_results
            .into_iter()
            .enumerate()
            .map(|(idx, res)| {
                let chunk = self.bm25.get_chunk(&res.id).cloned();
                HybridSearchResult {
                    chunk_id: res.id,
                    score: res.score,
                    bm25_score: None,
                    vector_score: Some(res.score),
                    bm25_rank: None,
                    vector_rank: Some(idx + 1),
                    chunk,
                }
            })
            .collect();
        Ok(results)
    }

    /// Executes hybrid search combining BM25 and Vector search according to configured strategy.
    /// If `query_embedding` is None, falls back to BM25 search.
    /// If `query_text` is empty, falls back to Vector search.
    pub fn search(
        &self,
        query_text: &str,
        query_embedding: Option<&[f32]>,
        top_k: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        if top_k == 0 {
            return Ok(Vec::new());
        }

        // If no embedding is provided, perform lexical BM25 search
        let query_emb = match query_embedding {
            Some(emb) if !emb.is_empty() => emb,
            _ => return Ok(self.search_bm25(query_text, top_k)),
        };

        // If text is blank, perform vector-only search
        if query_text.trim().is_empty() {
            return self.search_vector(query_emb, top_k);
        }

        // Candidate pool size
        let pool_size =
            (top_k * self.config.candidate_pool_multiplier).max(self.config.min_candidates);

        // Fetch candidate lists from both modalities
        let bm25_matches: Vec<Bm25SearchResult> = self.bm25.search(query_text, pool_size);
        let vec_matches: Vec<VectorSearchResult> = self.vector.search(query_emb, pool_size)?;

        // Map chunk_id to its BM25 info (score, 1-based rank, chunk)
        let mut bm25_map: HashMap<String, (f32, usize, CodeChunk)> = HashMap::new();
        let mut max_bm25_score = 0.0f32;
        for (i, m) in bm25_matches.into_iter().enumerate() {
            if m.score > max_bm25_score {
                max_bm25_score = m.score;
            }
            bm25_map.insert(m.chunk_id, (m.score, i + 1, m.chunk));
        }

        // Map chunk_id to its Vector info (score, 1-based rank)
        let mut vec_map: HashMap<String, (f32, usize)> = HashMap::new();
        let mut max_vec_score = 0.0f32;
        for (i, m) in vec_matches.into_iter().enumerate() {
            if m.score > max_vec_score {
                max_vec_score = m.score;
            }
            vec_map.insert(m.id, (m.score, i + 1));
        }

        // Collect all unique candidate IDs
        let all_ids: HashSet<String> = bm25_map
            .keys()
            .cloned()
            .chain(vec_map.keys().cloned())
            .collect();

        let mut fused_results: Vec<HybridSearchResult> = Vec::with_capacity(all_ids.len());

        match self.config.strategy {
            FusionStrategy::Weighted {
                bm25_weight,
                vector_weight,
            } => {
                let norm_bm25_div = if max_bm25_score > 0.0 {
                    max_bm25_score
                } else {
                    1.0
                };
                let norm_vec_div = if max_vec_score > 0.0 {
                    max_vec_score
                } else {
                    1.0
                };

                for id in all_ids {
                    let bm25_info = bm25_map.get(&id);
                    let vec_info = vec_map.get(&id);

                    let (bm25_score, bm25_rank, chunk) = match bm25_info {
                        Some((s, r, c)) => (Some(*s), Some(*r), Some(c.clone())),
                        None => (None, None, self.bm25.get_chunk(&id).cloned()),
                    };

                    let (vec_score, vec_rank) = match vec_info {
                        Some((s, r)) => (Some(*s), Some(*r)),
                        None => (None, None),
                    };

                    // Compute normalized scores in [0.0, 1.0]
                    let norm_bm25 = bm25_score.map_or(0.0, |s| s / norm_bm25_div);
                    let norm_vec = vec_score.map_or(0.0, |s| (s.max(0.0)) / norm_vec_div);

                    let combined_score = bm25_weight * norm_bm25 + vector_weight * norm_vec;

                    fused_results.push(HybridSearchResult {
                        chunk_id: id,
                        score: combined_score,
                        bm25_score,
                        vector_score: vec_score,
                        bm25_rank,
                        vector_rank: vec_rank,
                        chunk,
                    });
                }
            }

            FusionStrategy::ReciprocalRankFusion { rrf_k } => {
                let k_f32 = rrf_k as f32;

                for id in all_ids {
                    let bm25_info = bm25_map.get(&id);
                    let vec_info = vec_map.get(&id);

                    let (bm25_score, bm25_rank, chunk) = match bm25_info {
                        Some((s, r, c)) => (Some(*s), Some(*r), Some(c.clone())),
                        None => (None, None, self.bm25.get_chunk(&id).cloned()),
                    };

                    let (vec_score, vec_rank) = match vec_info {
                        Some((s, r)) => (Some(*s), Some(*r)),
                        None => (None, None),
                    };

                    let mut rrf_score = 0.0f32;
                    if let Some(r) = bm25_rank {
                        rrf_score += 1.0 / (k_f32 + r as f32);
                    }
                    if let Some(r) = vec_rank {
                        rrf_score += 1.0 / (k_f32 + r as f32);
                    }

                    fused_results.push(HybridSearchResult {
                        chunk_id: id,
                        score: rrf_score,
                        bm25_score,
                        vector_score: vec_score,
                        bm25_rank,
                        vector_rank: vec_rank,
                        chunk,
                    });
                }
            }
        }

        // Sort descending by combined score; tie-break on chunk_id
        fused_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.chunk_id.cmp(&b.chunk_id))
        });

        fused_results.truncate(top_k);
        Ok(fused_results)
    }
}
