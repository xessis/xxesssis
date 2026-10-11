use crate::bracket::{validate_brackets, BracketValidationReport};
use crate::error::{Result, ShadowError};
use orbit_diff::{
    apply_all_blocks, apply_block, compute_line_diff, parse_search_replace_blocks, DiffOutput,
    DiffSummary, SearchReplaceBlock,
};
use serde::{Deserialize, Serialize};

/// Represents an in-memory speculative file state.
/// Holds the original baseline and current speculative content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowBuffer {
    path: String,
    original_content: String,
    current_content: String,
    version: u64,
}

impl ShadowBuffer {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Self {
        let content_str = content.into();
        Self {
            path: path.into(),
            original_content: content_str.clone(),
            current_content: content_str,
            version: 0,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn original_content(&self) -> &str {
        &self.original_content
    }

    pub fn current_content(&self) -> &str {
        &self.current_content
    }

    pub fn is_dirty(&self) -> bool {
        self.original_content != self.current_content
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn len(&self) -> usize {
        self.current_content.len()
    }

    pub fn is_empty(&self) -> bool {
        self.current_content.is_empty()
    }

    pub fn line_count(&self) -> usize {
        self.current_content.lines().count()
    }

    /// Replaces the current content entirely (instant rewrite).
    pub fn rewrite(&mut self, new_content: impl Into<String>) -> &mut Self {
        self.current_content = new_content.into();
        self.version += 1;
        self
    }

    /// Applies a single Fast Apply search/replace block.
    pub fn apply_block(&mut self, block: &SearchReplaceBlock) -> Result<()> {
        let updated = apply_block(&self.current_content, block).map_err(|e| {
            ShadowError::DiffApplyFailed {
                path: self.path.clone(),
                reason: e.to_string(),
            }
        })?;
        self.current_content = updated;
        self.version += 1;
        Ok(())
    }

    /// Applies multiple Fast Apply search/replace blocks sequentially.
    pub fn apply_blocks(&mut self, blocks: &[SearchReplaceBlock]) -> Result<()> {
        let updated = apply_all_blocks(&self.current_content, blocks).map_err(|e| {
            ShadowError::DiffApplyFailed {
                path: self.path.clone(),
                reason: e.to_string(),
            }
        })?;
        self.current_content = updated;
        self.version += 1;
        Ok(())
    }

    /// Convenience method to search and replace a single block.
    pub fn apply_search_replace(&mut self, original: &str, replacement: &str) -> Result<()> {
        let block = SearchReplaceBlock {
            original: original.to_string(),
            replacement: replacement.to_string(),
        };
        self.apply_block(&block)
    }

    /// Parses search/replace blocks formatted with conflict markers and applies them.
    pub fn apply_search_replace_text(&mut self, text: &str) -> Result<usize> {
        let blocks = parse_search_replace_blocks(text);
        if blocks.is_empty() {
            return Err(ShadowError::DiffApplyFailed {
                path: self.path.clone(),
                reason: "No valid search/replace blocks found in input text".to_string(),
            });
        }
        let count = blocks.len();
        self.apply_blocks(&blocks)?;
        Ok(count)
    }

    /// Validates bracket balancing (parentheses, brackets, curly braces) for current content.
    pub fn validate_brackets(&self) -> BracketValidationReport {
        validate_brackets(&self.current_content, Some(&self.path))
    }

    /// Checks bracket balancing, returning an error on imbalance.
    pub fn ensure_valid_brackets(&self) -> Result<()> {
        self.validate_brackets().ensure_balanced(&self.path)
    }

    /// Validates the buffer state, verifying bracket balance and checking for unresolved conflict markers.
    pub fn validate(&self) -> Result<()> {
        self.ensure_valid_brackets()?;

        // Detect unmerged conflict markers left behind
        for line in self.current_content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("<<<<<<<")
                || trimmed.starts_with("=======")
                || trimmed.starts_with(">>>>>>>")
            {
                return Err(ShadowError::ValidationFailed(format!(
                    "Unresolved conflict/diff marker found in '{}': {}",
                    self.path, trimmed
                )));
            }
        }

        Ok(())
    }

    /// Computes Myers line diff against original baseline content.
    pub fn compute_diff(&self) -> DiffOutput {
        compute_line_diff(&self.original_content, &self.current_content)
    }

    /// Computes summary statistics (additions, deletions, modifications) against baseline.
    pub fn diff_summary(&self) -> DiffSummary {
        self.compute_diff().summary
    }

    /// Reverts the speculative content back to original content.
    pub fn revert(&mut self) {
        self.current_content = self.original_content.clone();
        self.version += 1;
    }

    /// Commits the current speculative content as the new baseline original content.
    pub fn commit(&mut self) {
        self.original_content = self.current_content.clone();
    }

    /// Overwrites the original baseline content without modifying current speculative content.
    pub fn set_baseline(&mut self, content: impl Into<String>) {
        self.original_content = content.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_basic_ops() {
        let mut buf = ShadowBuffer::new("test.txt", "line 1\nline 2\nline 3\n");
        assert_eq!(buf.path(), "test.txt");
        assert_eq!(buf.original_content(), "line 1\nline 2\nline 3\n");
        assert_eq!(buf.current_content(), "line 1\nline 2\nline 3\n");
        assert!(!buf.is_dirty());
        assert_eq!(buf.version(), 0);

        buf.rewrite("line 1\nline 2 modified\nline 3\n");
        assert!(buf.is_dirty());
        assert_eq!(buf.version(), 1);

        let diff = buf.compute_diff();
        assert_eq!(diff.summary.additions, 1);
        assert_eq!(diff.summary.deletions, 1);

        buf.revert();
        assert!(!buf.is_dirty());
        assert_eq!(buf.current_content(), "line 1\nline 2\nline 3\n");

        buf.rewrite("new baseline");
        buf.commit();
        assert!(!buf.is_dirty());
        assert_eq!(buf.original_content(), "new baseline");
    }

    #[test]
    fn test_buffer_apply_search_replace() {
        let mut buf = ShadowBuffer::new(
            "app.rs",
            "fn main() {\n    let count = 0;\n    println!(\"{}\", count);\n}\n",
        );

        buf.apply_search_replace("let count = 0;", "let count = 42;")
            .expect("should apply block");

        assert!(buf.current_content().contains("let count = 42;"));
        assert!(buf.is_dirty());

        let summary = buf.diff_summary();
        assert_eq!(summary.additions, 1);
        assert_eq!(summary.deletions, 1);
    }

    #[test]
    fn test_buffer_apply_conflict_marker_text() {
        let mut buf = ShadowBuffer::new("index.js", "const a = 1;\nconst b = 2;\n");
        let edit_block = r#"
<<<<<<< ORIGINAL
const b = 2;
=======
const b = 20;
const c = 30;
>>>>>>> UPDATED
"#;
        let applied_count = buf
            .apply_search_replace_text(edit_block)
            .expect("apply text blocks");
        assert_eq!(applied_count, 1);
        assert_eq!(
            buf.current_content(),
            "const a = 1;\nconst b = 20;\nconst c = 30;\n"
        );
    }

    #[test]
    fn test_buffer_validation_catches_errors() {
        let mut buf = ShadowBuffer::new("main.rs", "fn run() {\n    let x = 1;\n}\n");
        assert!(buf.validate().is_ok());

        // Introduce bracket imbalance
        buf.rewrite("fn run() {\n    let x = (1 + 2;\n}\n");
        assert!(buf.validate().is_err());

        // Introduce conflict marker left in code
        buf.rewrite("fn run() {\n<<<<<<< ORIGINAL\n    let x = 1;\n=======\n    let x = 2;\n>>>>>>> UPDATED\n}\n");
        assert!(buf.validate().is_err());
    }
}
