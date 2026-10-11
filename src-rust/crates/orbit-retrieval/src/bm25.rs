use crate::chunker::CodeChunk;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Configuration parameters for BM25 ranking.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Bm25Params {
    /// Term frequency saturation parameter (default: 1.2)
    pub k1: f32,
    /// Document length normalization parameter (default: 0.75)
    pub b: f32,
}

impl Default for Bm25Params {
    fn default() -> Self {
        Self { k1: 1.2, b: 0.75 }
    }
}

/// Tokenizes text into normalized keywords, splitting camelCase, snake_case,
/// and non-alphanumeric separators.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();

    for raw_word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if raw_word.is_empty() {
            continue;
        }

        for sub_word in raw_word.split('_') {
            if sub_word.is_empty() {
                continue;
            }

            let mut current = String::new();
            let chars: Vec<char> = sub_word.chars().collect();
            for i in 0..chars.len() {
                let c = chars[i];
                if c.is_uppercase() && !current.is_empty() {
                    let next_is_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();
                    let prev_is_upper = i > 0 && chars[i - 1].is_uppercase();

                    if next_is_lower || !prev_is_upper {
                        let token = current.to_lowercase();
                        if !token.is_empty() {
                            tokens.push(token);
                        }
                        current.clear();
                    }
                }
                current.push(c);
            }

            if !current.is_empty() {
                let token = current.to_lowercase();
                if !token.is_empty() {
                    tokens.push(token);
                }
            }

            // Also keep the composite identifier when it was split into multiple sub-tokens
            let lower_sub = sub_word.to_lowercase();
            if lower_sub.len() > 1 && tokens.last() != Some(&lower_sub) {
                tokens.push(lower_sub);
            }
        }
    }

    tokens
}

/// Represents a scored BM25 match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25SearchResult {
    pub chunk_id: String,
    pub score: f32,
    pub chunk: CodeChunk,
}

/// Lexical BM25 inverted index for code chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25Index {
    params: Bm25Params,
    chunks: HashMap<String, CodeChunk>,
    doc_lengths: HashMap<String, usize>,
    /// Inverted index: term -> { chunk_id -> term_frequency }
    inverted_index: HashMap<String, HashMap<String, u32>>,
    total_tokens: usize,
}

impl Default for Bm25Index {
    fn default() -> Self {
        Self::new(Bm25Params::default())
    }
}

impl Bm25Index {
    /// Creates a new empty BM25 index with the specified parameters.
    pub fn new(params: Bm25Params) -> Self {
        Self {
            params,
            chunks: HashMap::new(),
            doc_lengths: HashMap::new(),
            inverted_index: HashMap::new(),
            total_tokens: 0,
        }
    }

    pub fn params(&self) -> &Bm25Params {
        &self.params
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn total_tokens(&self) -> usize {
        self.total_tokens
    }

    pub fn get_chunk(&self, chunk_id: &str) -> Option<&CodeChunk> {
        self.chunks.get(chunk_id)
    }

    /// Computes the average document length across all indexed chunks.
    pub fn avgdl(&self) -> f32 {
        if self.chunks.is_empty() {
            0.0
        } else {
            self.total_tokens as f32 / self.chunks.len() as f32
        }
    }

    /// Computes Inverse Document Frequency (IDF) for a term using the standard
    /// Lucene/Robertson smooth formulation: ln(1 + (N - n + 0.5) / (n + 0.5)).
    pub fn idf(&self, term: &str) -> f32 {
        let n = self
            .inverted_index
            .get(term)
            .map_or(0, |postings| postings.len());

        let total_docs = self.chunks.len();
        if total_docs == 0 {
            return 0.0;
        }

        let numerator = (total_docs as f32 - n as f32 + 0.5).max(0.0);
        let denominator = n as f32 + 0.5;
        (1.0 + (numerator / denominator)).ln()
    }

    /// Adds a single chunk to the index. If a chunk with the same ID already exists,
    /// it is replaced.
    pub fn add_chunk(&mut self, chunk: CodeChunk) {
        if self.chunks.contains_key(&chunk.id) {
            self.remove_chunk(&chunk.id);
        }

        let tokens = tokenize(&chunk.content);
        let doc_len = tokens.len();

        let mut term_counts: HashMap<String, u32> = HashMap::new();
        for token in tokens {
            *term_counts.entry(token).or_insert(0) += 1;
        }

        for (term, tf) in &term_counts {
            self.inverted_index
                .entry(term.clone())
                .or_default()
                .insert(chunk.id.clone(), *tf);
        }

        self.doc_lengths.insert(chunk.id.clone(), doc_len);
        self.total_tokens += doc_len;
        self.chunks.insert(chunk.id.clone(), chunk);
    }

    /// Adds multiple chunks to the index.
    pub fn add_chunks(&mut self, chunks: impl IntoIterator<Item = CodeChunk>) {
        for chunk in chunks {
            self.add_chunk(chunk);
        }
    }

    /// Removes a chunk by ID from the index. Returns true if the chunk existed.
    pub fn remove_chunk(&mut self, chunk_id: &str) -> bool {
        let chunk = match self.chunks.remove(chunk_id) {
            Some(c) => c,
            None => return false,
        };

        if let Some(doc_len) = self.doc_lengths.remove(chunk_id) {
            self.total_tokens = self.total_tokens.saturating_sub(doc_len);
        }

        // Clean up inverted index postings
        let tokens = tokenize(&chunk.content);
        let unique_terms: HashSet<String> = tokens.into_iter().collect();

        for term in unique_terms {
            if let Some(postings) = self.inverted_index.get_mut(&term) {
                postings.remove(chunk_id);
                if postings.is_empty() {
                    self.inverted_index.remove(&term);
                }
            }
        }

        true
    }

    /// Clears the entire index.
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.doc_lengths.clear();
        self.inverted_index.clear();
        self.total_tokens = 0;
    }

    /// Computes the BM25 score for a specific chunk against tokenized query terms.
    pub fn score_chunk(&self, query_terms: &[String], chunk_id: &str) -> f32 {
        let avgdl = self.avgdl();
        if avgdl <= 0.0 {
            return 0.0;
        }

        let doc_len = match self.doc_lengths.get(chunk_id) {
            Some(&len) => len as f32,
            None => return 0.0,
        };

        let k1 = self.params.k1;
        let b = self.params.b;
        let length_norm = 1.0 - b + b * (doc_len / avgdl);

        let mut total_score = 0.0;

        for term in query_terms {
            if let Some(postings) = self.inverted_index.get(term) {
                if let Some(&tf) = postings.get(chunk_id) {
                    let tf_f32 = tf as f32;
                    let idf = self.idf(term);
                    let tf_component = (tf_f32 * (k1 + 1.0)) / (tf_f32 + k1 * length_norm);
                    total_score += idf * tf_component;
                }
            }
        }

        total_score
    }

    /// Searches the index with a natural language or code query and returns top_k results.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<Bm25SearchResult> {
        if top_k == 0 || self.chunks.is_empty() {
            return Vec::new();
        }

        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        // Candidate chunk ids: chunks containing at least one query term
        let mut candidates = HashSet::new();
        for term in &query_tokens {
            if let Some(postings) = self.inverted_index.get(term) {
                for chunk_id in postings.keys() {
                    candidates.insert(chunk_id);
                }
            }
        }

        let mut results = Vec::with_capacity(candidates.len());
        for chunk_id in candidates {
            let score = self.score_chunk(&query_tokens, chunk_id);
            if score > 0.0 {
                if let Some(chunk) = self.chunks.get(chunk_id) {
                    results.push(Bm25SearchResult {
                        chunk_id: chunk_id.clone(),
                        score,
                        chunk: chunk.clone(),
                    });
                }
            }
        }

        // Sort descending by score; tie-break on chunk_id for determinism
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.chunk_id.cmp(&b.chunk_id))
        });

        results.truncate(top_k);
        results
    }
}
