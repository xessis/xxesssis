use crate::error::{Result, RuntimeError};
use orbit_common::{CheckpointNode, FileSnapshot};
use orbit_diff::{compute_line_diff, DiffOutput};
use orbit_tools::security::assert_no_traversal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// ponytail: in-memory BLAKE3 CAS store with linear undo/redo rollback stack; upgrade to disk-backed sqlite/git object store when persisting across restarts.

/// Content-Addressed Storage storing byte blobs keyed by their BLAKE3 hex hash.
#[derive(Debug, Clone, Default)]
pub struct CasStore {
    blobs: Arc<RwLock<HashMap<String, Arc<[u8]>>>>,
}

impl CasStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert arbitrary bytes into the CAS store.
    /// Returns (blake3_hash, was_inserted).
    pub fn insert_bytes(&self, bytes: &[u8]) -> (String, bool) {
        let hash = blake3::hash(bytes).to_hex().to_string();
        let mut store = self.blobs.write().expect("CAS lock poisoned");
        if store.contains_key(&hash) {
            (hash, false)
        } else {
            store.insert(hash.clone(), Arc::from(bytes));
            (hash, true)
        }
    }

    /// Insert string content into the CAS store.
    pub fn insert_str(&self, content: &str) -> (String, bool) {
        self.insert_bytes(content.as_bytes())
    }

    /// Get raw bytes by hash.
    pub fn get(&self, hash: &str) -> Option<Arc<[u8]>> {
        let store = self.blobs.read().expect("CAS lock poisoned");
        store.get(hash).cloned()
    }

    /// Get UTF-8 string content by hash.
    pub fn get_str(&self, hash: &str) -> Option<String> {
        self.get(hash)
            .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
    }

    /// Check if a hash exists in the CAS.
    pub fn contains(&self, hash: &str) -> bool {
        let store = self.blobs.read().expect("CAS lock poisoned");
        store.contains_key(hash)
    }

    /// Number of unique blobs stored.
    pub fn len(&self) -> usize {
        let store = self.blobs.read().expect("CAS lock poisoned");
        store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total stored byte size across all deduplicated blobs.
    pub fn total_bytes(&self) -> usize {
        let store = self.blobs.read().expect("CAS lock poisoned");
        store.values().map(|v| v.len()).sum()
    }
}

/// A checkpoint representation recording file hashes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub timestamp_ms: i64,
    pub description: Option<String>,
    /// Relative path -> BLAKE3 hash
    pub files: HashMap<String, String>,
}

impl Checkpoint {
    pub fn new(
        parent_id: Option<Uuid>,
        description: Option<String>,
        files: HashMap<String, String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            parent_id,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            description,
            files,
        }
    }
}

/// Atomic file rollback action required to move between checkpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FileRollbackAction {
    Create {
        path: String,
        content: String,
        hash: String,
    },
    Modify {
        path: String,
        old_content: String,
        new_content: String,
        old_hash: String,
        new_hash: String,
    },
    Delete {
        path: String,
        old_content: String,
        old_hash: String,
    },
}

impl FileRollbackAction {
    pub fn path(&self) -> &str {
        match self {
            FileRollbackAction::Create { path, .. }
            | FileRollbackAction::Modify { path, .. }
            | FileRollbackAction::Delete { path, .. } => path,
        }
    }
}

/// Summary counts of a rollback plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RollbackSummary {
    pub created: usize,
    pub modified: usize,
    pub deleted: usize,
    pub unchanged: usize,
}

/// A computed delta plan to rollback or fast-forward the file tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackPlan {
    pub from_checkpoint_id: Option<Uuid>,
    pub to_checkpoint_id: Uuid,
    pub actions: Vec<FileRollbackAction>,
    pub summary: RollbackSummary,
}

impl RollbackPlan {
    /// Compute a Myers line diff for a modified file in this plan.
    pub fn diff_for_file(&self, path: &str) -> Option<DiffOutput> {
        for action in &self.actions {
            if let FileRollbackAction::Modify {
                path: p,
                old_content,
                new_content,
                ..
            } = action
            {
                if p == path {
                    return Some(compute_line_diff(old_content, new_content));
                }
            }
        }
        None
    }
}

/// Manager maintaining checkpoints with BLAKE3 CAS and linear undo/redo history.
#[derive(Debug, Clone)]
pub struct CheckpointManager {
    cas: CasStore,
    history: Vec<Checkpoint>,
    cursor: Option<usize>,
}

impl Default for CheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointManager {
    pub fn new() -> Self {
        Self {
            cas: CasStore::new(),
            history: Vec::new(),
            cursor: None,
        }
    }

    pub fn cas(&self) -> &CasStore {
        &self.cas
    }

    pub fn history(&self) -> &[Checkpoint] {
        &self.history
    }

    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    pub fn current_checkpoint(&self) -> Option<&Checkpoint> {
        self.cursor.and_then(|idx| self.history.get(idx))
    }

    pub fn checkpoint_by_id(&self, id: Uuid) -> Option<&Checkpoint> {
        self.history.iter().find(|cp| cp.id == id)
    }

    pub fn can_undo(&self) -> bool {
        match self.cursor {
            Some(idx) => idx > 0,
            None => false,
        }
    }

    pub fn can_redo(&self) -> bool {
        match self.cursor {
            Some(idx) => idx + 1 < self.history.len(),
            None => false,
        }
    }

    /// Commit a new checkpoint.
    /// If cursor is behind the latest history (due to undos), the redo branch is truncated.
    pub fn commit(
        &mut self,
        description: Option<String>,
        files: HashMap<String, String>,
    ) -> Result<Uuid> {
        let parent_id = self.current_checkpoint().map(|c| c.id);

        let mut manifest = HashMap::new();
        for (raw_path, content) in files {
            let path = raw_path.replace('\\', "/");
            assert_no_traversal(&path)?;

            let (hash, _) = self.cas.insert_str(&content);
            manifest.insert(path, hash);
        }

        // Truncate redo stack if cursor is behind
        if let Some(idx) = self.cursor {
            self.history.truncate(idx + 1);
        }

        let checkpoint = Checkpoint::new(parent_id, description, manifest);
        let id = checkpoint.id;
        self.history.push(checkpoint);
        self.cursor = Some(self.history.len() - 1);

        Ok(id)
    }

    /// Undo to the previous checkpoint in history.
    pub fn undo(&mut self) -> Result<RollbackPlan> {
        let curr_idx = self.cursor.ok_or(RuntimeError::NoUndoAvailable)?;
        if curr_idx == 0 {
            return Err(RuntimeError::NoUndoAvailable);
        }

        let target_idx = curr_idx - 1;
        let from_cp = self.history[curr_idx].clone();
        let to_cp = self.history[target_idx].clone();

        let plan = self.compute_plan(Some(&from_cp), &to_cp);
        self.cursor = Some(target_idx);
        Ok(plan)
    }

    /// Redo to the next checkpoint in history.
    pub fn redo(&mut self) -> Result<RollbackPlan> {
        let curr_idx = self.cursor.ok_or(RuntimeError::NoRedoAvailable)?;
        if curr_idx + 1 >= self.history.len() {
            return Err(RuntimeError::NoRedoAvailable);
        }

        let target_idx = curr_idx + 1;
        let from_cp = self.history[curr_idx].clone();
        let to_cp = self.history[target_idx].clone();

        let plan = self.compute_plan(Some(&from_cp), &to_cp);
        self.cursor = Some(target_idx);
        Ok(plan)
    }

    /// Rollback to a specific checkpoint in history.
    pub fn rollback_to(&mut self, target_id: Uuid) -> Result<RollbackPlan> {
        let target_idx = self
            .history
            .iter()
            .position(|cp| cp.id == target_id)
            .ok_or(RuntimeError::CheckpointNotFound(target_id))?;

        let from_cp = self.current_checkpoint().cloned();
        let to_cp = self.history[target_idx].clone();

        let plan = self.compute_plan(from_cp.as_ref(), &to_cp);
        self.cursor = Some(target_idx);
        Ok(plan)
    }

    /// Compute the rollback delta plan between two checkpoints.
    pub fn compute_plan(&self, from: Option<&Checkpoint>, to: &Checkpoint) -> RollbackPlan {
        let mut actions = Vec::new();
        let mut created = 0;
        let mut modified = 0;
        let mut deleted = 0;
        let mut unchanged = 0;

        let empty_map = HashMap::new();
        let from_files = from.map(|c| &c.files).unwrap_or(&empty_map);
        let to_files = &to.files;

        // Files in target checkpoint
        for (path, new_hash) in to_files {
            let new_content = self.cas.get_str(new_hash).unwrap_or_default();
            if let Some(old_hash) = from_files.get(path) {
                if old_hash == new_hash {
                    unchanged += 1;
                } else {
                    let old_content = self.cas.get_str(old_hash).unwrap_or_default();
                    actions.push(FileRollbackAction::Modify {
                        path: path.clone(),
                        old_content,
                        new_content,
                        old_hash: old_hash.clone(),
                        new_hash: new_hash.clone(),
                    });
                    modified += 1;
                }
            } else {
                actions.push(FileRollbackAction::Create {
                    path: path.clone(),
                    content: new_content,
                    hash: new_hash.clone(),
                });
                created += 1;
            }
        }

        // Files in source checkpoint that were removed
        for (path, old_hash) in from_files {
            if !to_files.contains_key(path) {
                let old_content = self.cas.get_str(old_hash).unwrap_or_default();
                actions.push(FileRollbackAction::Delete {
                    path: path.clone(),
                    old_content,
                    old_hash: old_hash.clone(),
                });
                deleted += 1;
            }
        }

        RollbackPlan {
            from_checkpoint_id: from.map(|c| c.id),
            to_checkpoint_id: to.id,
            actions,
            summary: RollbackSummary {
                created,
                modified,
                deleted,
                unchanged,
            },
        }
    }

    /// Convert a checkpoint to orbit_common::CheckpointNode.
    pub fn to_checkpoint_node(&self, cp: &Checkpoint) -> CheckpointNode {
        let mut node = CheckpointNode {
            id: cp.id,
            timestamp_ms: cp.timestamp_ms,
            files: HashMap::new(),
            description: cp.description.clone(),
        };

        for (path, hash) in &cp.files {
            let content = self.cas.get_str(hash).unwrap_or_default();
            let snapshot = FileSnapshot {
                path: path.clone(),
                content,
                hash: hash.clone(),
            };
            node.files.insert(path.clone(), snapshot);
        }

        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cas_deduplication() {
        let cas = CasStore::new();
        let (hash1, new1) = cas.insert_str("identical file contents");
        let (hash2, new2) = cas.insert_str("identical file contents");

        assert_eq!(hash1, hash2);
        assert!(new1);
        assert!(!new2);
        assert_eq!(cas.len(), 1);
        assert_eq!(cas.get_str(&hash1).unwrap(), "identical file contents");
    }

    #[test]
    fn test_checkpoint_undo_redo_math() {
        let mut manager = CheckpointManager::new();

        // Checkpoint 0: initial state
        let mut files0 = HashMap::new();
        files0.insert("file_a.txt".to_string(), "version 1".to_string());
        files0.insert("file_b.txt".to_string(), "unrelated file".to_string());
        let cp0_id = manager
            .commit(Some("init".into()), files0)
            .expect("commit 0 failed");

        assert!(!manager.can_undo());
        assert!(!manager.can_redo());

        // Checkpoint 1: modify file_a, add file_c
        let mut files1 = HashMap::new();
        files1.insert("file_a.txt".to_string(), "version 2".to_string());
        files1.insert("file_b.txt".to_string(), "unrelated file".to_string());
        files1.insert("file_c.txt".to_string(), "new content".to_string());
        let cp1_id = manager
            .commit(Some("update".into()), files1)
            .expect("commit 1 failed");

        assert!(manager.can_undo());
        assert!(!manager.can_redo());

        // Checkpoint 2: delete file_b, modify file_c
        let mut files2 = HashMap::new();
        files2.insert("file_a.txt".to_string(), "version 2".to_string());
        files2.insert("file_c.txt".to_string(), "modified new content".to_string());
        let cp2_id = manager
            .commit(Some("delete & edit".into()), files2)
            .expect("commit 2 failed");

        assert_eq!(manager.history().len(), 3);
        assert_eq!(manager.cursor(), Some(2));

        // Undo from cp2 to cp1
        let plan_undo_2 = manager.undo().expect("undo failed");
        assert_eq!(plan_undo_2.from_checkpoint_id, Some(cp2_id));
        assert_eq!(plan_undo_2.to_checkpoint_id, cp1_id);
        assert_eq!(plan_undo_2.summary.modified, 1); // file_c modified back
        assert_eq!(plan_undo_2.summary.created, 1); // file_b recreated
        assert_eq!(plan_undo_2.summary.unchanged, 1); // file_a unchanged
        assert_eq!(manager.cursor(), Some(1));
        assert!(manager.can_redo());

        // Undo from cp1 to cp0
        let plan_undo_1 = manager.undo().expect("undo failed");
        assert_eq!(plan_undo_1.to_checkpoint_id, cp0_id);
        assert_eq!(plan_undo_1.summary.modified, 1); // file_a restored to v1
        assert_eq!(plan_undo_1.summary.deleted, 1); // file_c removed
        assert_eq!(plan_undo_1.summary.unchanged, 1); // file_b unchanged
        assert_eq!(manager.cursor(), Some(0));
        assert!(!manager.can_undo());

        // Redo back to cp1
        let plan_redo_1 = manager.redo().expect("redo failed");
        assert_eq!(plan_redo_1.to_checkpoint_id, cp1_id);
        assert_eq!(manager.cursor(), Some(1));

        // Test diff computation for modified file in rollback plan
        let diff = plan_redo_1
            .diff_for_file("file_a.txt")
            .expect("diff missing");
        assert_eq!(diff.summary.additions, 1);
        assert_eq!(diff.summary.deletions, 1);

        // Commit while at cp1 -> truncates cp2 from history!
        let mut files_alt = HashMap::new();
        files_alt.insert("file_fork.txt".to_string(), "forked".to_string());
        let cp_fork_id = manager
            .commit(Some("fork branch".into()), files_alt)
            .expect("fork commit failed");

        assert_eq!(manager.history().len(), 3); // [cp0, cp1, cp_fork]
        assert_eq!(manager.cursor(), Some(2));
        assert_eq!(manager.current_checkpoint().unwrap().id, cp_fork_id);
        assert!(!manager.can_redo()); // redo stack was truncated!
    }

    #[test]
    fn test_checkpoint_security_traversal_refused() {
        let mut manager = CheckpointManager::new();
        let mut malicious_files = HashMap::new();
        malicious_files.insert("../../etc/passwd".to_string(), "bad".to_string());

        assert!(manager.commit(None, malicious_files).is_err());
    }

    #[test]
    fn test_orbit_common_checkpoint_node_interop() {
        let mut manager = CheckpointManager::new();
        let mut files = HashMap::new();
        files.insert("main.rs".to_string(), "fn main() {}".to_string());
        manager.commit(Some("init node".into()), files).unwrap();

        let cp = manager.current_checkpoint().unwrap();
        let node = manager.to_checkpoint_node(cp);
        assert_eq!(node.id, cp.id);
        assert_eq!(node.description, Some("init node".into()));
        assert!(node.files.contains_key("main.rs"));
        assert_eq!(node.files["main.rs"].content, "fn main() {}");
    }
}
