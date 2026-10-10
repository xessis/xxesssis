use orbit_diff::{apply_block, compute_line_diff, parse_search_replace_blocks, SearchReplaceBlock};

#[test]
fn test_oob_panic_when_search_longer_than_source() {
    let source = "line 1\nline 2\n";
    let block = SearchReplaceBlock {
        original: "nonexistent 1\nnonexistent 2\nnonexistent 3\nnonexistent 4".to_string(),
        replacement: "replacement".to_string(),
    };

    let result = std::panic::catch_unwind(|| apply_block(source, &block));

    assert!(result.is_ok(), "apply_block must not panic");
    assert!(
        result.unwrap().is_err(),
        "must return Err for oversized search block"
    );
}

#[test]
fn test_oob_panic_on_empty_source() {
    let source = "";
    let block = SearchReplaceBlock {
        original: "line 1".to_string(),
        replacement: "replacement".to_string(),
    };

    let result = std::panic::catch_unwind(|| apply_block(source, &block));

    assert!(result.is_ok(), "apply_block must not panic on empty source");
    assert!(
        result.unwrap().is_err(),
        "must return Err for search block on empty source"
    );
}

#[test]
fn test_crlf_line_anchor_failure_and_newline_corruption() {
    let source = "line 1\r\nline 2\r\nline 3\r\n";
    let block = SearchReplaceBlock {
        original: "line 2".to_string(),
        replacement: "line 2 modified".to_string(),
    };

    let result = apply_block(source, &block).expect("apply_block should succeed");

    // CRLF line endings must be preserved
    assert!(result.contains("\r\n"), "CRLF endings must be preserved");
    assert_eq!(result, "line 1\r\nline 2 modified\r\nline 3\r\n");
}

#[test]
fn test_empty_search_block_inconsistency() {
    let block = SearchReplaceBlock {
        original: "".to_string(),
        replacement: "injected".to_string(),
    };

    let empty_res = apply_block("", &block);
    assert!(
        empty_res.is_err(),
        "empty search block must fail on empty source"
    );

    let nonempty_res = apply_block("hello", &block);
    assert!(
        nonempty_res.is_err(),
        "empty search block must fail on non-empty source"
    );
}

#[test]
fn test_unclosed_block_silent_data_loss() {
    let unclosed = r#"
<<<<<<< ORIGINAL
fn buggy() {}
=======
fn fixed() {}
"#;
    let blocks = parse_search_replace_blocks(unclosed);
    assert_eq!(
        blocks.len(),
        1,
        "unclosed block should be recovered cleanly"
    );
    assert_eq!(blocks[0].original, "fn buggy() {}");
    assert_eq!(blocks[0].replacement, "fn fixed() {}");
}

#[test]
fn test_myers_crlf_vs_lf_false_positive() {
    let old_text = "line 1\r\nline 2\r\n";
    let new_text = "line 1\nline 2\n";

    let diff = compute_line_diff(old_text, new_text);
    // Even though text content is identical, diff reports 2 deletions and 2 additions
    assert_eq!(diff.summary.deletions, 2);
    assert_eq!(diff.summary.additions, 2);
}
