use crate::buffer::ShadowBuffer;
use crate::error::{Result, ShadowError};
use orbit_diff::{DiffOutput, DiffSummary, SearchReplaceBlock};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Open,
    InTransaction,
    Committed,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCommitRecord {
    pub path: String,
    pub original_content: String,
    pub committed_content: String,
    pub diff_summary: DiffSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionSummary {
    pub session_id: String,
    pub files: Vec<FileCommitRecord>,
    pub total_summary: DiffSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditOperation {
    Rewrite { content: String },
    SearchReplace { blocks: Vec<SearchReplaceBlock> },
    SearchReplaceText { diff_text: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeculativeFileEdit {
    pub path: String,
    pub operation: EditOperation,
}

/// Tracks speculative edits across multiple files in an atomic transaction.
/// Supports commit (all succeed) or rollback (revert all in-memory buffers if any file fails).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeculativeEditSession {
    id: String,
    buffers: HashMap<String, ShadowBuffer>,
    state: SessionState,
    #[serde(skip)]
    tx_snapshots: Option<HashMap<String, String>>,
    #[serde(skip)]
    tx_new_buffers: Option<HashSet<String>>,
}

impl SpeculativeEditSession {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            buffers: HashMap::new(),
            state: SessionState::Open,
            tx_snapshots: None,
            tx_new_buffers: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn is_in_transaction(&self) -> bool {
        self.state == SessionState::InTransaction
    }

    pub fn open_buffer(
        &mut self,
        path: impl Into<String>,
        content: impl Into<String>,
    ) -> &mut ShadowBuffer {
        let path_str = path.into();
        if self.is_in_transaction() && !self.buffers.contains_key(&path_str) {
            if let Some(new_bufs) = &mut self.tx_new_buffers {
                new_bufs.insert(path_str.clone());
            }
        }
        let buffer = ShadowBuffer::new(path_str.clone(), content);
        self.buffers.insert(path_str.clone(), buffer);
        self.buffers
            .get_mut(&path_str)
            .expect("buffer just inserted")
    }

    pub fn add_buffer(&mut self, buffer: ShadowBuffer) -> &mut ShadowBuffer {
        let path = buffer.path().to_string();
        if self.is_in_transaction() && !self.buffers.contains_key(&path) {
            if let Some(new_bufs) = &mut self.tx_new_buffers {
                new_bufs.insert(path.clone());
            }
        }
        self.buffers.insert(path.clone(), buffer);
        self.buffers.get_mut(&path).expect("buffer just inserted")
    }

    pub fn get_buffer(&self, path: &str) -> Option<&ShadowBuffer> {
        self.buffers.get(path)
    }

    pub fn get_buffer_mut(&mut self, path: &str) -> Option<&mut ShadowBuffer> {
        self.buffers.get_mut(path)
    }

    pub fn remove_buffer(&mut self, path: &str) -> Option<ShadowBuffer> {
        self.buffers.remove(path)
    }

    pub fn has_buffer(&self, path: &str) -> bool {
        self.buffers.contains_key(path)
    }

    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    pub fn buffer_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.buffers.keys().cloned().collect();
        paths.sort();
        paths
    }

    pub fn is_dirty(&self) -> bool {
        self.buffers.values().any(|b| b.is_dirty())
    }

    pub fn dirty_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .buffers
            .iter()
            .filter(|(_, b)| b.is_dirty())
            .map(|(k, _)| k.clone())
            .collect();
        paths.sort();
        paths
    }

    /// Begins an atomic transaction snapshotting all current buffer states.
    pub fn begin_transaction(&mut self) -> Result<()> {
        if self.state == SessionState::InTransaction {
            return Err(ShadowError::Transaction(
                "Transaction already in progress".to_string(),
            ));
        }

        let snapshots: HashMap<String, String> = self
            .buffers
            .iter()
            .map(|(p, b)| (p.clone(), b.current_content().to_string()))
            .collect();

        self.tx_snapshots = Some(snapshots);
        self.tx_new_buffers = Some(HashSet::new());
        self.state = SessionState::InTransaction;
        Ok(())
    }

    /// Rewrites buffer content within transaction.
    pub fn apply_rewrite(&mut self, path: &str, content: impl Into<String>) -> Result<()> {
        let buffer = self
            .buffers
            .get_mut(path)
            .ok_or_else(|| ShadowError::BufferNotFound(path.to_string()))?;
        buffer.rewrite(content);
        Ok(())
    }

    /// Applies a single Fast Apply search/replace block to a buffer.
    pub fn apply_block(&mut self, path: &str, block: &SearchReplaceBlock) -> Result<()> {
        let buffer = self
            .buffers
            .get_mut(path)
            .ok_or_else(|| ShadowError::BufferNotFound(path.to_string()))?;
        buffer.apply_block(block)
    }

    /// Applies multiple Fast Apply search/replace blocks to a buffer.
    pub fn apply_blocks(&mut self, path: &str, blocks: &[SearchReplaceBlock]) -> Result<()> {
        let buffer = self
            .buffers
            .get_mut(path)
            .ok_or_else(|| ShadowError::BufferNotFound(path.to_string()))?;
        buffer.apply_blocks(blocks)
    }

    /// Convenience method to search and replace on a buffer.
    pub fn apply_search_replace(
        &mut self,
        path: &str,
        original: &str,
        replacement: &str,
    ) -> Result<()> {
        let buffer = self
            .buffers
            .get_mut(path)
            .ok_or_else(|| ShadowError::BufferNotFound(path.to_string()))?;
        buffer.apply_search_replace(original, replacement)
    }

    /// Parses search/replace blocks with conflict markers and applies them to a buffer.
    pub fn apply_search_replace_text(&mut self, path: &str, diff_text: &str) -> Result<usize> {
        let buffer = self
            .buffers
            .get_mut(path)
            .ok_or_else(|| ShadowError::BufferNotFound(path.to_string()))?;
        buffer.apply_search_replace_text(diff_text)
    }

    /// Validates all buffers in the session (bracket balancing, conflict markers).
    pub fn validate_all(&self) -> Result<()> {
        for buffer in self.buffers.values() {
            buffer.validate()?;
        }
        Ok(())
    }

    /// Commits the active transaction if all validations succeed.
    /// If validation fails, automatically rolls back all buffers to the pre-transaction state.
    pub fn commit_transaction(&mut self) -> Result<TransactionSummary> {
        if self.state != SessionState::InTransaction {
            return Err(ShadowError::Transaction(
                "No active transaction to commit".to_string(),
            ));
        }

        // Validate all buffers before finalizing commit
        if let Err(val_err) = self.validate_all() {
            let _ = self.rollback_transaction();
            return Err(val_err);
        }

        let mut committed_files = Vec::new();
        let mut total_summary = DiffSummary::default();

        let mut sorted_keys: Vec<String> = self.buffers.keys().cloned().collect();
        sorted_keys.sort();

        for key in sorted_keys {
            if let Some(buf) = self.buffers.get_mut(&key) {
                if buf.is_dirty() {
                    let diff = buf.compute_diff();
                    total_summary.additions += diff.summary.additions;
                    total_summary.deletions += diff.summary.deletions;
                    total_summary.modifications += diff.summary.modifications;

                    let record = FileCommitRecord {
                        path: buf.path().to_string(),
                        original_content: buf.original_content().to_string(),
                        committed_content: buf.current_content().to_string(),
                        diff_summary: diff.summary,
                    };
                    buf.commit();
                    committed_files.push(record);
                }
            }
        }

        self.tx_snapshots = None;
        self.tx_new_buffers = None;
        self.state = SessionState::Committed;

        Ok(TransactionSummary {
            session_id: self.id.clone(),
            files: committed_files,
            total_summary,
        })
    }

    /// Rolls back the active transaction, reverting all in-memory buffers to their pre-transaction state.
    pub fn rollback_transaction(&mut self) -> Result<()> {
        if self.state != SessionState::InTransaction {
            return Err(ShadowError::Transaction(
                "No active transaction to rollback".to_string(),
            ));
        }

        if let Some(snapshots) = self.tx_snapshots.take() {
            for (path, content) in snapshots {
                if let Some(buf) = self.buffers.get_mut(&path) {
                    buf.rewrite(content);
                }
            }
        }

        if let Some(new_paths) = self.tx_new_buffers.take() {
            for path in new_paths {
                self.buffers.remove(&path);
            }
        }

        self.state = SessionState::RolledBack;
        Ok(())
    }

    /// Executes an atomic transaction within a closure.
    /// If the closure or validation fails, all changes are automatically rolled back.
    pub fn run_transaction<F, R>(&mut self, f: F) -> Result<R>
    where
        F: FnOnce(&mut SpeculativeEditSession) -> Result<R>,
    {
        self.begin_transaction()?;
        match f(self) {
            Ok(res) => {
                self.commit_transaction()?;
                Ok(res)
            }
            Err(err) => {
                let _ = self.rollback_transaction();
                Err(err)
            }
        }
    }

    /// Applies a sequence of speculative file edits atomically.
    /// If any file edit fails (or syntax validation fails), rolls back everything.
    pub fn apply_edits(&mut self, edits: &[SpeculativeFileEdit]) -> Result<TransactionSummary> {
        self.begin_transaction()?;

        for edit in edits {
            let res = match &edit.operation {
                EditOperation::Rewrite { content } => self.apply_rewrite(&edit.path, content),
                EditOperation::SearchReplace { blocks } => self.apply_blocks(&edit.path, blocks),
                EditOperation::SearchReplaceText { diff_text } => self
                    .apply_search_replace_text(&edit.path, diff_text)
                    .map(|_| ()),
            };

            if let Err(e) = res {
                let _ = self.rollback_transaction();
                return Err(e);
            }
        }

        self.commit_transaction()
    }

    /// Computes Myers line diffs for all buffers in the session against their original content.
    pub fn compute_all_diffs(&self) -> HashMap<String, DiffOutput> {
        self.buffers
            .iter()
            .map(|(path, buf)| (path.clone(), buf.compute_diff()))
            .collect()
    }

    /// Computes aggregated diff summary across all dirty buffers in the session.
    pub fn total_diff_summary(&self) -> DiffSummary {
        let mut total = DiffSummary::default();
        for buf in self.buffers.values() {
            let s = buf.diff_summary();
            total.additions += s.additions;
            total.deletions += s.deletions;
            total.modifications += s.modifications;
        }
        total
    }

    /// Discards all speculative changes in all buffers, restoring them to original content.
    pub fn rollback_all(&mut self) {
        for buf in self.buffers.values_mut() {
            buf.revert();
        }
        self.tx_snapshots = None;
        self.tx_new_buffers = None;
        self.state = SessionState::RolledBack;
    }

    /// Commits all current buffer states as baseline original content after validation.
    pub fn commit_all(&mut self) -> Result<TransactionSummary> {
        self.validate_all()?;

        let mut committed_files = Vec::new();
        let mut total_summary = DiffSummary::default();

        let mut sorted_keys: Vec<String> = self.buffers.keys().cloned().collect();
        sorted_keys.sort();

        for key in sorted_keys {
            if let Some(buf) = self.buffers.get_mut(&key) {
                if buf.is_dirty() {
                    let diff = buf.compute_diff();
                    total_summary.additions += diff.summary.additions;
                    total_summary.deletions += diff.summary.deletions;
                    total_summary.modifications += diff.summary.modifications;

                    let record = FileCommitRecord {
                        path: buf.path().to_string(),
                        original_content: buf.original_content().to_string(),
                        committed_content: buf.current_content().to_string(),
                        diff_summary: diff.summary,
                    };
                    buf.commit();
                    committed_files.push(record);
                }
            }
        }

        self.state = SessionState::Committed;
        Ok(TransactionSummary {
            session_id: self.id.clone(),
            files: committed_files,
            total_summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_lifecycle_and_buffers() {
        let mut session = SpeculativeEditSession::new("session-1");
        assert_eq!(session.id(), "session-1");
        assert_eq!(session.state(), SessionState::Open);

        session.open_buffer("src/a.rs", "fn a() {}\n");
        session.open_buffer("src/b.rs", "fn b() {}\n");

        assert_eq!(session.buffer_count(), 2);
        assert!(session.has_buffer("src/a.rs"));
        assert!(session.has_buffer("src/b.rs"));
        assert!(!session.is_dirty());
    }

    #[test]
    fn test_multi_file_successful_transaction() {
        let mut session = SpeculativeEditSession::new("tx-success");
        session.open_buffer("file1.rs", "fn one() {\n    let a = 1;\n}\n");
        session.open_buffer("file2.rs", "fn two() {\n    let b = 2;\n}\n");

        session.begin_transaction().expect("begin tx");
        session
            .apply_search_replace("file1.rs", "let a = 1;", "let a = 100;")
            .expect("edit file1");
        session
            .apply_search_replace("file2.rs", "let b = 2;", "let b = 200;")
            .expect("edit file2");

        assert!(session.is_dirty());
        let summary = session.commit_transaction().expect("commit tx");

        assert_eq!(summary.files.len(), 2);
        assert_eq!(session.state(), SessionState::Committed);

        let buf1 = session.get_buffer("file1.rs").unwrap();
        assert!(buf1.current_content().contains("let a = 100;"));
        assert!(!buf1.is_dirty()); // committed to baseline

        let buf2 = session.get_buffer("file2.rs").unwrap();
        assert!(buf2.current_content().contains("let b = 200;"));
        assert!(!buf2.is_dirty());
    }

    #[test]
    fn test_multi_file_rollback_on_failed_edit() {
        let mut session = SpeculativeEditSession::new("tx-rollback-edit");
        session.open_buffer("file1.rs", "fn one() {\n    let a = 1;\n}\n");
        session.open_buffer("file2.rs", "fn two() {\n    let b = 2;\n}\n");

        let res = session.run_transaction(|sess| {
            sess.apply_search_replace("file1.rs", "let a = 1;", "let a = 100;")?;
            // file2 search block does NOT exist: will fail!
            sess.apply_search_replace("file2.rs", "nonexistent target", "replacement")?;
            Ok(())
        });

        assert!(res.is_err());
        assert_eq!(session.state(), SessionState::RolledBack);

        // Verify file1 was rolled back completely
        let buf1 = session.get_buffer("file1.rs").unwrap();
        assert_eq!(buf1.current_content(), "fn one() {\n    let a = 1;\n}\n");
        assert!(!buf1.is_dirty());

        let buf2 = session.get_buffer("file2.rs").unwrap();
        assert_eq!(buf2.current_content(), "fn two() {\n    let b = 2;\n}\n");
        assert!(!buf2.is_dirty());
    }

    #[test]
    fn test_multi_file_rollback_on_bracket_validation_failure() {
        let mut session = SpeculativeEditSession::new("tx-rollback-syntax");
        session.open_buffer("file1.rs", "fn valid() {\n    let a = 1;\n}\n");
        session.open_buffer("file2.rs", "fn broken() {\n    let b = 2;\n}\n");

        session.begin_transaction().expect("begin tx");
        // file1 has valid change
        session
            .apply_search_replace("file1.rs", "let a = 1;", "let a = 10;")
            .expect("file1 ok");
        // file2 introduces catastrophic unclosed bracket break
        session
            .apply_rewrite("file2.rs", "fn broken() {\n    let b = (2 + 3;\n}\n")
            .expect("apply rewrite");

        // commit_transaction must run syntax validation and fail, rolling back all files
        let commit_res = session.commit_transaction();
        assert!(commit_res.is_err());

        // file1 must be restored to baseline!
        let buf1 = session.get_buffer("file1.rs").unwrap();
        assert_eq!(buf1.current_content(), "fn valid() {\n    let a = 1;\n}\n");

        // file2 must be restored to baseline!
        let buf2 = session.get_buffer("file2.rs").unwrap();
        assert_eq!(buf2.current_content(), "fn broken() {\n    let b = 2;\n}\n");
    }

    #[test]
    fn test_apply_edits_batch_atomic() {
        let mut session = SpeculativeEditSession::new("batch-edits");
        session.open_buffer("auth.rs", "pub fn auth() -> bool { true }\n");
        session.open_buffer("user.rs", "pub fn user() -> String { \"user\".into() }\n");

        let edits = vec![
            SpeculativeFileEdit {
                path: "auth.rs".to_string(),
                operation: EditOperation::Rewrite {
                    content: "pub fn auth() -> bool { false }\n".to_string(),
                },
            },
            SpeculativeFileEdit {
                path: "user.rs".to_string(),
                operation: EditOperation::Rewrite {
                    content: "pub fn user() -> String { \"admin\".into() }\n".to_string(),
                },
            },
        ];

        let summary = session.apply_edits(&edits).expect("apply edits");
        assert_eq!(summary.files.len(), 2);

        assert_eq!(
            session.get_buffer("auth.rs").unwrap().current_content(),
            "pub fn auth() -> bool { false }\n"
        );
        assert_eq!(
            session.get_buffer("user.rs").unwrap().current_content(),
            "pub fn user() -> String { \"admin\".into() }\n"
        );
    }
}
