pub mod hunk;
pub mod myers;
pub mod search_replace;

pub use hunk::{DiffChange, DiffHunk, DiffOp, DiffOutput, DiffSummary};
pub use myers::compute_line_diff;
pub use search_replace::{
    apply_all_blocks, apply_block, parse_search_replace_blocks, SearchReplaceBlock,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_myers_diff_simple() {
        let old_text = "fn main() {\n    println!(\"hello\");\n}\n";
        let new_text = "fn main() {\n    println!(\"hello world\");\n}\n";

        let diff = compute_line_diff(old_text, new_text);
        assert!(!diff.hunks.is_empty());
        assert_eq!(diff.summary.additions, 1);
        assert_eq!(diff.summary.deletions, 1);
    }

    #[test]
    fn test_search_replace_exact() {
        let source = "line 1\nline 2\nline 3\n";
        let block = SearchReplaceBlock {
            original: "line 2".to_string(),
            replacement: "line 2 modified".to_string(),
        };

        let result = apply_block(source, &block).expect("apply failed");
        assert_eq!(result, "line 1\nline 2 modified\nline 3\n");
    }

    #[test]
    fn test_search_replace_trimmed_fallback() {
        let source = "    let x = 10;\n    let y = 20;\n";
        let block = SearchReplaceBlock {
            original: "let x = 10;".to_string(),
            replacement: "    let x = 99;".to_string(),
        };

        let result = apply_block(source, &block).expect("trimmed fallback failed");
        assert_eq!(result, "    let x = 99;\n    let y = 20;\n");
    }

    #[test]
    fn test_parse_blocks() {
        let text = r#"
Here is the code change:
<<<<<<< ORIGINAL
const a = 1;
=======
const a = 2;
>>>>>>> UPDATED
Done!
"#;
        let blocks = parse_search_replace_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].original, "const a = 1;");
        assert_eq!(blocks[0].replacement, "const a = 2;");
    }
}
