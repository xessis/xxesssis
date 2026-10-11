use crate::chunker::{ChunkerConfig, CodeChunk, CodeChunker};
use crate::error::Result;
use crate::hybrid::{HybridConfig, HybridSearchResult, HybridSearcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Summary statistics of an indexing operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexingStats {
    pub files_scanned: usize,
    pub files_indexed: usize,
    pub chunks_created: usize,
}

/// Runtime statistics of the retrieval engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
    pub total_files: usize,
    pub total_chunks: usize,
    pub total_tokens: usize,
    pub vector_count: usize,
    pub avg_chunk_lines: f32,
}

/// High-level workspace indexing and retrieval engine.
/// Coordinates code chunking, BM25 indexing, and vector similarity search.
#[derive(Debug, Clone)]
pub struct RetrievalEngine {
    chunker: CodeChunker,
    searcher: HybridSearcher,
    /// Maps normalized file_path -> list of chunk_ids
    file_to_chunks: HashMap<String, Vec<String>>,
    /// Maps file_path -> file content blake3 hash
    file_hashes: HashMap<String, String>,
}

impl Default for RetrievalEngine {
    fn default() -> Self {
        Self::new(ChunkerConfig::default(), HybridConfig::default())
    }
}

impl RetrievalEngine {
    pub fn new(chunker_config: ChunkerConfig, hybrid_config: HybridConfig) -> Self {
        Self {
            chunker: CodeChunker::new(chunker_config),
            searcher: HybridSearcher::new(Default::default(), Default::default(), hybrid_config),
            file_to_chunks: HashMap::new(),
            file_hashes: HashMap::new(),
        }
    }

    pub fn chunker(&self) -> &CodeChunker {
        &self.chunker
    }

    pub fn searcher(&self) -> &HybridSearcher {
        &self.searcher
    }

    pub fn searcher_mut(&mut self) -> &mut HybridSearcher {
        &mut self.searcher
    }

    /// Indexes an entire workspace directory.
    pub fn index_workspace(&mut self, root: impl AsRef<Path>) -> Result<IndexingStats> {
        let chunks = self.chunker.chunk_workspace(root)?;
        let mut stats = IndexingStats {
            files_scanned: 0,
            files_indexed: 0,
            chunks_created: chunks.len(),
        };

        // Group chunks by file path
        let mut file_groups: HashMap<String, Vec<CodeChunk>> = HashMap::new();
        for chunk in chunks {
            file_groups
                .entry(chunk.file_path.clone())
                .or_default()
                .push(chunk);
        }

        stats.files_indexed = file_groups.len();
        stats.files_scanned = file_groups.len();

        for (file_path, file_chunks) in file_groups {
            // Remove existing chunks for this file if re-indexing
            self.remove_file(&file_path);

            let mut chunk_ids = Vec::with_capacity(file_chunks.len());
            for chunk in file_chunks {
                chunk_ids.push(chunk.id.clone());
                self.searcher.index_chunk(chunk, None)?;
            }
            self.file_to_chunks.insert(file_path, chunk_ids);
        }

        Ok(stats)
    }

    /// Indexes a single file into the engine. Returns the number of chunks created.
    pub fn index_file(&mut self, path: impl AsRef<Path>) -> Result<usize> {
        let path_ref = path.as_ref();
        let content = std::fs::read_to_string(path_ref)?;
        let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
        let file_path = path_ref.to_string_lossy().replace('\\', "/");

        // If file content has not changed, skip re-indexing
        if let Some(existing_hash) = self.file_hashes.get(&file_path) {
            if existing_hash == &hash {
                return Ok(self.file_to_chunks.get(&file_path).map_or(0, |c| c.len()));
            }
        }

        // Clean up prior chunks
        self.remove_file(&file_path);

        let chunks = self.chunker.chunk_text(&file_path, &content);
        let count = chunks.len();
        let mut chunk_ids = Vec::with_capacity(count);

        for chunk in chunks {
            chunk_ids.push(chunk.id.clone());
            self.searcher.index_chunk(chunk, None)?;
        }

        self.file_to_chunks.insert(file_path.clone(), chunk_ids);
        self.file_hashes.insert(file_path, hash);

        Ok(count)
    }

    /// Updates embeddings for a specific chunk.
    pub fn update_chunk_embedding(&mut self, chunk_id: &str, embedding: Vec<f32>) -> Result<()> {
        self.searcher.vector.add(chunk_id.to_string(), embedding)
    }

    /// Removes all indexed chunks for a file. Returns the number of chunks removed.
    pub fn remove_file(&mut self, file_path: &str) -> usize {
        let normalized = file_path.replace('\\', "/");
        self.file_hashes.remove(&normalized);
        let chunk_ids = match self.file_to_chunks.remove(&normalized) {
            Some(ids) => ids,
            None => return 0,
        };

        let count = chunk_ids.len();
        for id in &chunk_ids {
            self.searcher.remove_chunk(id);
        }
        count
    }

    /// Performs hybrid search across the index.
    pub fn search(
        &self,
        query_text: &str,
        query_embedding: Option<&[f32]>,
        top_k: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        self.searcher.search(query_text, query_embedding, top_k)
    }

    /// Performs lexical search using BM25 only.
    pub fn search_bm25(&self, query_text: &str, top_k: usize) -> Vec<HybridSearchResult> {
        self.searcher.search_bm25(query_text, top_k)
    }

    /// Performs semantic vector search only.
    pub fn search_vector(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        self.searcher.search_vector(query_embedding, top_k)
    }

    /// Returns aggregate statistics of the indexed codebase.
    pub fn stats(&self) -> EngineStats {
        let total_chunks = self.searcher.bm25.len();

        EngineStats {
            total_files: self.file_to_chunks.len(),
            total_chunks,
            total_tokens: self.searcher.bm25.total_tokens(),
            vector_count: self.searcher.vector.len(),
            avg_chunk_lines: 0.0, // ponytail: compute line count lazily when needed
        }
    }
}
