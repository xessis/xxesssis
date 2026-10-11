use orbit_retrieval::{
    cosine_similarity, tokenize, Bm25Index, Bm25Params, ChunkerConfig, CodeChunk, CodeChunker,
    HybridConfig, HybridSearcher, RetrievalEngine, RetrievalError, VectorIndex,
};

// ---------------------------------------------------------------------------
// 1. CHUNKING TESTS
// ---------------------------------------------------------------------------

#[test]
fn test_chunker_empty_input() {
    let chunker = CodeChunker::default();
    let chunks = chunker.chunk_text("test.rs", "");
    assert!(chunks.is_empty());

    let chunks_ws = chunker.chunk_text("test.rs", "   \n\n\t  ");
    assert!(chunks_ws.is_empty());
}

#[test]
fn test_chunker_small_file_single_chunk() {
    let chunker = CodeChunker::default();
    let code = "fn main() {\n    println!(\"hello\");\n}\n";
    let chunks = chunker.chunk_text("src/main.rs", code);

    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(chunk.file_path, "src/main.rs");
    assert_eq!(chunk.start_line, 1);
    assert_eq!(chunk.end_line, 3);
    assert_eq!(chunk.line_count(), 3);
    assert_eq!(chunk.id, "src/main.rs:1-3");

    // Blake3 hash validation
    let expected_hash = blake3::hash(chunk.content.as_bytes()).to_hex().to_string();
    assert_eq!(chunk.hash, expected_hash);
}

#[test]
fn test_chunker_multiline_with_overlap() {
    // Config: min 10, max 20, overlap 5
    let config = ChunkerConfig::new(10, 20, 5).expect("valid config");
    let chunker = CodeChunker::new(config);

    // Generate 45 lines of code
    let mut lines = Vec::new();
    for i in 1..=45 {
        lines.push(format!("let var_{} = {};", i, i));
    }
    let code = lines.join("\n");

    let chunks = chunker.chunk_text("test.rs", &code);
    assert!(
        chunks.len() >= 3,
        "Should split 45 lines into at least 3 chunks"
    );

    // Verify 1-based indexing and monotonicity
    for chunk in &chunks {
        assert!(chunk.start_line >= 1);
        assert!(chunk.end_line >= chunk.start_line);
        assert_eq!(chunk.line_count(), chunk.end_line - chunk.start_line + 1);
        assert_eq!(
            chunk.hash,
            blake3::hash(chunk.content.as_bytes()).to_hex().to_string()
        );
    }

    // Check overlap between consecutive chunks
    for w in chunks.windows(2) {
        let prev = &w[0];
        let next = &w[1];
        assert!(next.start_line <= prev.end_line, "Chunks must overlap");
        assert!(
            next.start_line > prev.start_line,
            "Next chunk must make forward progress"
        );
    }
}

#[test]
fn test_chunker_semantic_boundary_cut() {
    let config = ChunkerConfig::new(10, 20, 3).expect("valid config");
    let chunker = CodeChunker::new(config);

    let mut lines = Vec::new();
    for i in 1..=12 {
        lines.push(format!("let x{} = {};", i, i));
    }
    // Blank line at line 13
    lines.push("".to_string());
    for i in 14..=35 {
        lines.push(format!("let y{} = {};", i, i));
    }
    let code = lines.join("\n");

    let chunks = chunker.chunk_text("semantic.rs", &code);
    assert!(!chunks.is_empty());
    // The first chunk should have preferred breaking around line 13 (the blank line)
    assert!(chunks[0].end_line >= 12 && chunks[0].end_line <= 15);
}

#[test]
fn test_chunker_config_validation() {
    assert!(ChunkerConfig::new(0, 10, 2).is_err());
    assert!(ChunkerConfig::new(20, 10, 2).is_err());
    assert!(ChunkerConfig::new(10, 20, 15).is_err());
    assert!(ChunkerConfig::new(10, 20, 10).is_err());
    assert!(ChunkerConfig::new(10, 20, 9).is_ok());
}

// ---------------------------------------------------------------------------
// 2. BM25 TOKENIZER & RANKING TESTS
// ---------------------------------------------------------------------------

#[test]
fn test_bm25_tokenizer() {
    let tokens = tokenize("fn calculateUserScore_v2() -> Option<f32>");
    assert!(tokens.contains(&"fn".to_string()));
    assert!(tokens.contains(&"calculate".to_string()));
    assert!(tokens.contains(&"user".to_string()));
    assert!(tokens.contains(&"score".to_string()));
    assert!(tokens.contains(&"v2".to_string()));
    assert!(tokens.contains(&"option".to_string()));
    assert!(tokens.contains(&"f32".to_string()));
}

#[test]
fn test_bm25_idf_ranking() {
    let mut index = Bm25Index::default();

    // Doc 1: common term "vector" and rare term "quantum"
    let c1 = CodeChunk::new(
        "doc1.rs",
        1,
        10,
        "pub fn vector_quantum() { let vector = 1; }",
    );
    // Doc 2: common term "vector"
    let c2 = CodeChunk::new("doc2.rs", 1, 10, "pub fn vector_only() { let vector = 2; }");
    // Doc 3: common term "vector"
    let c3 = CodeChunk::new("doc3.rs", 1, 10, "pub fn vector_more() { let vector = 3; }");

    index.add_chunk(c1);
    index.add_chunk(c2);
    index.add_chunk(c3);

    // Rare term "quantum" should have strictly higher IDF than common term "vector"
    let idf_quantum = index.idf("quantum");
    let idf_vector = index.idf("vector");
    assert!(
        idf_quantum > idf_vector,
        "Rare term quantum ({}) must have higher IDF than common term vector ({})",
        idf_quantum,
        idf_vector
    );

    // Searching for "quantum" should rank doc1 #1
    let results = index.search("quantum", 5);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].chunk.file_path, "doc1.rs");
}

#[test]
fn test_bm25_term_frequency_and_length_normalization() {
    let mut index = Bm25Index::default();

    // Short document containing "alpha" once
    let c1 = CodeChunk::new("short.rs", 1, 2, "alpha");
    // Long document containing "alpha" once, padded with other tokens
    let c2 = CodeChunk::new(
        "long.rs",
        1,
        20,
        "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda",
    );

    index.add_chunk(c1);
    index.add_chunk(c2);

    let results = index.search("alpha", 5);
    assert_eq!(results.len(), 2);
    // Shorter document should have higher score due to length penalty (b = 0.75)
    assert_eq!(results[0].chunk_id, "short.rs:1-2");
    assert!(results[0].score > results[1].score);
}

#[test]
fn test_bm25_add_remove_chunk() {
    let mut index = Bm25Index::default();
    let c1 = CodeChunk::new("temp.rs", 1, 5, "fn temporary_code() {}");
    index.add_chunk(c1);
    assert_eq!(index.len(), 1);

    let res = index.search("temporary", 5);
    assert_eq!(res.len(), 1);

    // Remove
    assert!(index.remove_chunk("temp.rs:1-5"));
    assert_eq!(index.len(), 0);
    assert_eq!(index.total_tokens(), 0);

    let res_after = index.search("temporary", 5);
    assert!(res_after.is_empty());
}

// ---------------------------------------------------------------------------
// 3. VECTOR INDEX & COSINE SIMILARITY TESTS
// ---------------------------------------------------------------------------

#[test]
fn test_cosine_similarity_properties() {
    // Parallel vectors -> 1.0
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![2.0, 4.0, 6.0];
    let sim_parallel = cosine_similarity(&a, &b);
    assert!((sim_parallel - 1.0).abs() < 1e-6);

    // Identical vector with self -> 1.0
    let sim_self = cosine_similarity(&a, &a);
    assert!((sim_self - 1.0).abs() < 1e-6);

    // Orthogonal vectors -> 0.0
    let c = vec![1.0, 0.0, 0.0];
    let d = vec![0.0, 1.0, 0.0];
    let sim_ortho = cosine_similarity(&c, &d);
    assert!(sim_ortho.abs() < 1e-6);

    // Opposite vectors -> -1.0
    let e = vec![1.0, 0.0];
    let f = vec![-1.0, 0.0];
    let sim_opp = cosine_similarity(&e, &f);
    assert!((sim_opp - (-1.0)).abs() < 1e-6);

    // Zero vector -> 0.0 (no NaN/panic)
    let zero = vec![0.0, 0.0, 0.0];
    assert_eq!(cosine_similarity(&a, &zero), 0.0);
    assert_eq!(cosine_similarity(&zero, &zero), 0.0);
}

#[test]
fn test_vector_index_ranking() {
    let mut vindex = VectorIndex::with_dimension(3);

    vindex.add("target", vec![1.0, 0.0, 0.0]).unwrap();
    vindex.add("similar", vec![0.9, 0.1, 0.0]).unwrap();
    vindex.add("different", vec![0.0, 1.0, 0.0]).unwrap();

    let query = vec![1.0, 0.0, 0.0];
    let results = vindex.search(&query, 3).unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].id, "target");
    assert!((results[0].score - 1.0).abs() < 1e-5);
    assert_eq!(results[1].id, "similar");
    assert_eq!(results[2].id, "different");
}

#[test]
fn test_vector_index_dimension_mismatch() {
    let mut vindex = VectorIndex::with_dimension(4);
    let err = vindex.add("invalid", vec![1.0, 2.0, 3.0]).unwrap_err();
    match err {
        RetrievalError::DimensionMismatch { expected, actual } => {
            assert_eq!(expected, 4);
            assert_eq!(actual, 3);
        }
        _ => panic!("Expected DimensionMismatch error"),
    }

    let search_err = vindex.search(&[1.0, 2.0], 5).unwrap_err();
    assert!(matches!(
        search_err,
        RetrievalError::DimensionMismatch { .. }
    ));
}

// ---------------------------------------------------------------------------
// 4. HYBRID SEARCHER (WEIGHTED + RRF) TESTS
// ---------------------------------------------------------------------------

#[test]
fn test_hybrid_searcher_weighted_ranking() {
    let mut searcher = HybridSearcher::new(
        Bm25Index::new(Bm25Params::default()),
        VectorIndex::with_dimension(2),
        HybridConfig::weighted(0.4, 0.6),
    );

    // Item 1: High BM25 match ("login auth"), low semantic match [0.0, 1.0]
    let chunk1 = CodeChunk::new("auth.rs", 1, 10, "pub fn login_auth() { check_token(); }");
    searcher.index_chunk(chunk1, Some(vec![0.0, 1.0])).unwrap();

    // Item 2: Medium BM25 match ("auth"), high semantic match [1.0, 0.0]
    let chunk2 = CodeChunk::new("user.rs", 1, 10, "pub fn user_auth() {}");
    searcher.index_chunk(chunk2, Some(vec![1.0, 0.0])).unwrap();

    // Item 3: Low BM25 match ("metrics"), unrelated vector [0.0, -1.0]
    let chunk3 = CodeChunk::new("metrics.rs", 1, 10, "pub fn record_metrics() {}");
    searcher.index_chunk(chunk3, Some(vec![0.0, -1.0])).unwrap();

    // Query with text "login auth" and query vector [1.0, 0.0]
    let results = searcher.search("login auth", Some(&[1.0, 0.0]), 3).unwrap();

    assert_eq!(results.len(), 3);
    // user.rs has strong vector match (1.0 * 0.6 = 0.6) + partial BM25 match
    // auth.rs has strong BM25 match (1.0 * 0.4 = 0.4) + 0 vector match
    // Therefore user.rs should be ranked top
    assert_eq!(results[0].chunk_id, "user.rs:1-10");
    assert!(results[0].score > results[1].score);
    assert_eq!(results[1].chunk_id, "auth.rs:1-10");
}

#[test]
fn test_hybrid_searcher_reciprocal_rank_fusion() {
    let mut searcher = HybridSearcher::new(
        Bm25Index::new(Bm25Params::default()),
        VectorIndex::with_dimension(2),
        HybridConfig::rrf(60),
    );

    // Chunk A: Ranked #1 in BM25, #1 in Vector
    let chunk_a = CodeChunk::new("a.rs", 1, 5, "database postgres query execution");
    searcher.index_chunk(chunk_a, Some(vec![1.0, 0.0])).unwrap();

    // Chunk B: Ranked #2 in BM25, #2 in Vector
    let chunk_b = CodeChunk::new("b.rs", 1, 5, "database query runner");
    searcher.index_chunk(chunk_b, Some(vec![0.8, 0.2])).unwrap();

    // Chunk C: Ranked #3 in BM25, orthogonal in Vector
    let chunk_c = CodeChunk::new("c.rs", 1, 5, "database schema");
    searcher.index_chunk(chunk_c, Some(vec![0.0, 1.0])).unwrap();

    let results = searcher
        .search("database postgres query", Some(&[1.0, 0.0]), 3)
        .unwrap();

    assert_eq!(results.len(), 3);
    // a.rs is #1 in both modalities: score = 1/(60+1) + 1/(60+1) = 2/61 ~ 0.03278
    assert_eq!(results[0].chunk_id, "a.rs:1-5");
    assert_eq!(results[1].chunk_id, "b.rs:1-5");
    assert_eq!(results[2].chunk_id, "c.rs:1-5");

    let expected_a_score = 1.0 / 61.0 + 1.0 / 61.0;
    assert!((results[0].score - expected_a_score).abs() < 1e-4);
}

#[test]
fn test_hybrid_searcher_text_only_fallback() {
    let mut searcher = HybridSearcher::new(
        Bm25Index::default(),
        VectorIndex::default(),
        HybridConfig::default(),
    );

    let chunk = CodeChunk::new("sample.rs", 1, 5, "fn execute_command() {}");
    searcher.index_chunk(chunk, None).unwrap();

    // When query_embedding is None, should execute BM25 search cleanly
    let results = searcher.search("execute", None, 5).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].chunk_id, "sample.rs:1-5");
    assert!(results[0].bm25_score.is_some());
    assert!(results[0].vector_score.is_none());
}

// ---------------------------------------------------------------------------
// 5. ENGINE INTEGRATION TESTS
// ---------------------------------------------------------------------------

#[test]
fn test_retrieval_engine_index_and_search() {
    let mut engine = RetrievalEngine::default();

    // Create chunks manually via chunk_text
    let code_a = "pub fn add_numbers(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
    let code_b = "pub fn multiply_numbers(a: i32, b: i32) -> i32 {\n    a * b\n}\n";

    let chunks_a = engine.chunker().chunk_text("math/add.rs", code_a);
    let chunks_b = engine.chunker().chunk_text("math/mul.rs", code_b);

    for chunk in chunks_a {
        engine.searcher_mut().index_chunk(chunk, None).unwrap();
    }
    for chunk in chunks_b {
        engine.searcher_mut().index_chunk(chunk, None).unwrap();
    }

    let results = engine.search_bm25("add", 5);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].chunk_id, "math/add.rs:1-3");

    let mul_results = engine.search_bm25("multiply", 5);
    assert_eq!(mul_results.len(), 1);
    assert_eq!(mul_results[0].chunk_id, "math/mul.rs:1-3");
}

#[test]
fn test_serde_roundtrips() {
    let chunk = CodeChunk::new("test.rs", 1, 10, "let x = 42;");
    let serialized = serde_json::to_string(&chunk).unwrap();
    let deserialized: CodeChunk = serde_json::from_str(&serialized).unwrap();
    assert_eq!(chunk, deserialized);

    let config = HybridConfig::weighted(0.3, 0.7);
    let serialized_config = serde_json::to_string(&config).unwrap();
    let deserialized_config: HybridConfig = serde_json::from_str(&serialized_config).unwrap();
    assert_eq!(
        config.candidate_pool_multiplier,
        deserialized_config.candidate_pool_multiplier
    );

    let rrf_config = HybridConfig::rrf(50);
    let serialized_rrf = serde_json::to_string(&rrf_config).unwrap();
    let deserialized_rrf: HybridConfig = serde_json::from_str(&serialized_rrf).unwrap();
    assert_eq!(rrf_config.strategy, deserialized_rrf.strategy);
}

#[test]
fn test_bm25_empty_and_unknown_queries() {
    let mut index = Bm25Index::default();
    index.add_chunk(CodeChunk::new(
        "sample.rs",
        1,
        5,
        "pub fn known_symbol() {}",
    ));

    assert!(index.search("", 5).is_empty());
    assert!(index.search("   ", 5).is_empty());
    assert!(index.search("completely_unknown_token_xyz", 5).is_empty());
}

#[test]
fn test_engine_file_crud_lifecycle() {
    let mut engine = RetrievalEngine::default();
    let temp_dir = std::env::temp_dir().join(format!(
        "orbit_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let file_path = temp_dir.join("worker.rs");
    std::fs::write(
        &file_path,
        "pub fn process_task() {\n    let status = 1;\n}\n",
    )
    .unwrap();

    let chunks_created = engine.index_file(&file_path).unwrap();
    assert!(chunks_created > 0);

    let search_res = engine.search_bm25("task", 5);
    assert_eq!(search_res.len(), 1);

    // Update file content
    std::fs::write(
        &file_path,
        "pub fn process_event() {\n    let status = 2;\n}\n",
    )
    .unwrap();
    let updated_count = engine.index_file(&file_path).unwrap();
    assert!(updated_count > 0);

    // Old symbol should be gone, new symbol should be found
    let old_res = engine.search_bm25("task", 5);
    assert!(old_res.is_empty());

    let new_res = engine.search_bm25("event", 5);
    assert_eq!(new_res.len(), 1);

    // Remove file
    let path_str = file_path.to_string_lossy().replace('\\', "/");
    let removed = engine.remove_file(&path_str);
    assert!(removed > 0);

    let final_res = engine.search_bm25("event", 5);
    assert!(final_res.is_empty());

    let _ = std::fs::remove_dir_all(&temp_dir);
}
