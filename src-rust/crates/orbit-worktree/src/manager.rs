use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Result, WorktreeError};
use crate::types::{WorktreeInfo, WorktreeMetadata};

const ORBIT_WORKTREES_DIR: &str = ".orbit/worktrees";
const CURSOR_WORKTREES_DIR: &str = ".cursor/worktrees";

/// WorktreeManager coordinates isolated git worktrees for background agent operations.
// ponytail: uses system git cli subprocesses instead of libgit2 c-bindings; upgrade to libgit2 if process fork latency becomes a bottleneck (>500 ops/sec).
pub struct WorktreeManager {
    repo_root: PathBuf,
}

impl WorktreeManager {
    /// Creates a new WorktreeManager bound to a specific repository root.
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    /// Returns the configured repository root path.
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    // --- Instance async convenience methods ---

    pub async fn create(&self, task_id: &str, branch_name: &str) -> Result<WorktreeInfo> {
        Self::create_worktree(&self.repo_root, task_id, branch_name).await
    }

    pub async fn list(&self) -> Result<Vec<WorktreeInfo>> {
        Self::list_worktrees(&self.repo_root).await
    }

    pub async fn remove(&self, task_id: &str, force: bool) -> Result<()> {
        Self::remove_worktree(&self.repo_root, task_id, force).await
    }

    pub async fn diff(&self, worktree_path: &Path, base_branch: &str) -> Result<String> {
        Self::diff_worktree(worktree_path, base_branch).await
    }

    pub async fn lock(&self, task_id: &str, reason: Option<&str>) -> Result<()> {
        Self::lock_worktree(&self.repo_root, task_id, reason).await
    }

    pub async fn unlock(&self, task_id: &str) -> Result<()> {
        Self::unlock_worktree(&self.repo_root, task_id).await
    }

    pub async fn prune_stale(&self, max_age: std::time::Duration) -> Result<Vec<String>> {
        Self::prune_stale_worktrees(&self.repo_root, max_age).await
    }

    pub async fn repair(&self) -> Result<()> {
        Self::repair_worktrees(&self.repo_root).await
    }

    // --- Static Core Async API ---

    /// Creates an isolated git worktree for a task under `.orbit/worktrees/<task_id>`.
    pub async fn create_worktree(
        repo_root: &Path,
        task_id: &str,
        branch_name: &str,
    ) -> Result<WorktreeInfo> {
        validate_task_id(task_id)?;
        validate_branch_name(branch_name)?;

        if !repo_root.exists() {
            return Err(WorktreeError::Repository(format!(
                "Repository root path does not exist: {}",
                repo_root.display()
            )));
        }

        let worktree_dir = worktree_dir_for_task(repo_root, task_id);
        if worktree_dir.exists() {
            return Err(WorktreeError::AlreadyExists(task_id.to_string()));
        }

        let parent_dir = repo_root.join(ORBIT_WORKTREES_DIR);
        tokio::fs::create_dir_all(&parent_dir).await?;

        let worktree_path_str = to_git_path(&worktree_dir);

        // Check if the branch already exists in the repository
        let branch_ref = format!("refs/heads/{branch_name}");
        let branch_exists =
            run_git_status_check(repo_root, &["show-ref", "--verify", "--quiet", &branch_ref])
                .await;

        if branch_exists {
            run_git_async(
                repo_root,
                &["worktree", "add", &worktree_path_str, branch_name],
            )
            .await?;
        } else {
            run_git_async(
                repo_root,
                &["worktree", "add", "-b", branch_name, &worktree_path_str],
            )
            .await?;
        }

        let head = run_git_async(&worktree_dir, &["rev-parse", "HEAD"])
            .await
            .map(|h| h.trim().to_string())
            .ok();

        let now_ms = current_epoch_ms();

        // Write tracking metadata file
        let meta = WorktreeMetadata {
            task_id: task_id.to_string(),
            branch: branch_name.to_string(),
            created_at_ms: now_ms,
            is_locked: false,
            lock_reason: None,
        };
        let meta_path = metadata_path_for_task(repo_root, task_id);
        if let Ok(json) = serde_json::to_string_pretty(&meta) {
            let _ = tokio::fs::write(&meta_path, json).await;
        }

        Ok(WorktreeInfo {
            task_id: task_id.to_string(),
            branch: branch_name.to_string(),
            path: worktree_dir,
            head_commit: head,
            is_locked: false,
            lock_reason: None,
            created_at_ms: Some(now_ms),
        })
    }

    /// Lists all managed agent worktrees associated with the repository.
    pub async fn list_worktrees(repo_root: &Path) -> Result<Vec<WorktreeInfo>> {
        let raw = run_git_async(repo_root, &["worktree", "list", "--porcelain"]).await?;
        let items = parse_worktree_list_porcelain(&raw, repo_root);
        Ok(items)
    }

    /// Safely removes a worktree. Refuses removal if locked, unless force is true.
    pub async fn remove_worktree(repo_root: &Path, task_id: &str, force: bool) -> Result<()> {
        validate_task_id(task_id)?;

        let worktrees = Self::list_worktrees(repo_root).await?;
        let target = worktrees.iter().find(|w| w.task_id == task_id);

        let worktree_dir = if let Some(w) = target {
            w.path.clone()
        } else {
            let candidate = worktree_dir_for_task(repo_root, task_id);
            if candidate.exists() {
                candidate
            } else {
                return Err(WorktreeError::NotFound(task_id.to_string()));
            }
        };

        if let Some(w) = target {
            if w.is_locked && !force {
                return Err(WorktreeError::Locked {
                    task_id: task_id.to_string(),
                    reason: w.lock_reason.clone(),
                });
            }
        }

        let path_str = to_git_path(&worktree_dir);

        if force {
            let _ = run_git_async(repo_root, &["worktree", "unlock", &path_str]).await;
        }

        let remove_result = if force {
            run_git_async(repo_root, &["worktree", "remove", "--force", &path_str]).await
        } else {
            run_git_async(repo_root, &["worktree", "remove", &path_str]).await
        };

        if remove_result.is_err() && force {
            if worktree_dir.exists() {
                let _ = tokio::fs::remove_dir_all(&worktree_dir).await;
            }
        } else {
            remove_result?;
        }

        // Clean metadata file
        let meta_path = metadata_path_for_task(repo_root, task_id);
        if meta_path.exists() {
            let _ = tokio::fs::remove_file(&meta_path).await;
        }

        // Prune dangling git metadata to maintain parent repository integrity
        let _ = run_git_async(repo_root, &["worktree", "prune"]).await;

        Ok(())
    }

    /// Computes the git diff of the worktree against a base branch.
    pub async fn diff_worktree(worktree_path: &Path, base_branch: &str) -> Result<String> {
        validate_branch_name(base_branch)?;

        if !worktree_path.exists() {
            return Err(WorktreeError::NotFound(
                worktree_path.to_string_lossy().to_string(),
            ));
        }

        let diff_output = run_git_async(worktree_path, &["diff", base_branch]).await?;
        Ok(diff_output)
    }

    /// Locks a worktree to prevent accidental pruning or concurrent modification.
    pub async fn lock_worktree(
        repo_root: &Path,
        task_id: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        validate_task_id(task_id)?;

        let worktree_dir = find_worktree_path_or_default(repo_root, task_id);
        if !worktree_dir.exists() {
            return Err(WorktreeError::NotFound(task_id.to_string()));
        }

        let path_str = to_git_path(&worktree_dir);
        if let Some(r) = reason {
            run_git_async(repo_root, &["worktree", "lock", "--reason", r, &path_str]).await?;
        } else {
            run_git_async(repo_root, &["worktree", "lock", &path_str]).await?;
        }

        update_metadata_lock_async(repo_root, task_id, true, reason.map(|s| s.to_string())).await?;

        Ok(())
    }

    /// Unlocks a locked worktree.
    pub async fn unlock_worktree(repo_root: &Path, task_id: &str) -> Result<()> {
        validate_task_id(task_id)?;

        let worktree_dir = find_worktree_path_or_default(repo_root, task_id);
        if !worktree_dir.exists() {
            return Err(WorktreeError::NotFound(task_id.to_string()));
        }

        let path_str = to_git_path(&worktree_dir);
        run_git_async(repo_root, &["worktree", "unlock", &path_str]).await?;

        update_metadata_lock_async(repo_root, task_id, false, None).await?;

        Ok(())
    }

    /// Prunes stale, unlocked worktrees older than max_age without corrupting the parent repo.
    pub async fn prune_stale_worktrees(
        repo_root: &Path,
        max_age: std::time::Duration,
    ) -> Result<Vec<String>> {
        let worktrees = Self::list_worktrees(repo_root).await?;
        let now_ms = current_epoch_ms();
        let max_age_ms = max_age.as_millis() as u64;

        let mut pruned = Vec::new();

        for wt in worktrees {
            if wt.is_locked {
                continue;
            }

            let is_stale = if let Some(created_ms) = wt.created_at_ms {
                now_ms.saturating_sub(created_ms) >= max_age_ms
            } else if let Ok(metadata) = std::fs::metadata(&wt.path) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                        elapsed >= max_age
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                true
            };

            if is_stale
                && (Self::remove_worktree(repo_root, &wt.task_id, false)
                    .await
                    .is_ok()
                    || Self::remove_worktree(repo_root, &wt.task_id, true)
                        .await
                        .is_ok())
            {
                pruned.push(wt.task_id);
            }
        }

        let _ = run_git_async(repo_root, &["worktree", "prune"]).await;

        Ok(pruned)
    }

    /// Repairs administrative links between the parent repo and worktrees.
    pub async fn repair_worktrees(repo_root: &Path) -> Result<()> {
        let _ = run_git_async(repo_root, &["worktree", "repair"]).await;
        let _ = run_git_async(repo_root, &["worktree", "prune"]).await;
        Ok(())
    }

    // --- Synchronous / Blocking Equivalents ---

    pub fn create_worktree_sync(
        repo_root: &Path,
        task_id: &str,
        branch_name: &str,
    ) -> Result<WorktreeInfo> {
        validate_task_id(task_id)?;
        validate_branch_name(branch_name)?;

        if !repo_root.exists() {
            return Err(WorktreeError::Repository(format!(
                "Repository root path does not exist: {}",
                repo_root.display()
            )));
        }

        let worktree_dir = worktree_dir_for_task(repo_root, task_id);
        if worktree_dir.exists() {
            return Err(WorktreeError::AlreadyExists(task_id.to_string()));
        }

        let parent_dir = repo_root.join(ORBIT_WORKTREES_DIR);
        std::fs::create_dir_all(&parent_dir)?;

        let worktree_path_str = to_git_path(&worktree_dir);

        let branch_ref = format!("refs/heads/{branch_name}");
        let branch_exists =
            run_git_status_check_sync(repo_root, &["show-ref", "--verify", "--quiet", &branch_ref]);

        if branch_exists {
            run_git_sync(
                repo_root,
                &["worktree", "add", &worktree_path_str, branch_name],
            )?;
        } else {
            run_git_sync(
                repo_root,
                &["worktree", "add", "-b", branch_name, &worktree_path_str],
            )?;
        }

        let head = run_git_sync(&worktree_dir, &["rev-parse", "HEAD"])
            .map(|h| h.trim().to_string())
            .ok();

        let now_ms = current_epoch_ms();

        let meta = WorktreeMetadata {
            task_id: task_id.to_string(),
            branch: branch_name.to_string(),
            created_at_ms: now_ms,
            is_locked: false,
            lock_reason: None,
        };
        let meta_path = metadata_path_for_task(repo_root, task_id);
        if let Ok(json) = serde_json::to_string_pretty(&meta) {
            let _ = std::fs::write(&meta_path, json);
        }

        Ok(WorktreeInfo {
            task_id: task_id.to_string(),
            branch: branch_name.to_string(),
            path: worktree_dir,
            head_commit: head,
            is_locked: false,
            lock_reason: None,
            created_at_ms: Some(now_ms),
        })
    }

    pub fn list_worktrees_sync(repo_root: &Path) -> Result<Vec<WorktreeInfo>> {
        let raw = run_git_sync(repo_root, &["worktree", "list", "--porcelain"])?;
        let items = parse_worktree_list_porcelain(&raw, repo_root);
        Ok(items)
    }

    pub fn remove_worktree_sync(repo_root: &Path, task_id: &str, force: bool) -> Result<()> {
        validate_task_id(task_id)?;

        let worktrees = Self::list_worktrees_sync(repo_root)?;
        let target = worktrees.iter().find(|w| w.task_id == task_id);

        let worktree_dir = if let Some(w) = target {
            w.path.clone()
        } else {
            let candidate = worktree_dir_for_task(repo_root, task_id);
            if candidate.exists() {
                candidate
            } else {
                return Err(WorktreeError::NotFound(task_id.to_string()));
            }
        };

        if let Some(w) = target {
            if w.is_locked && !force {
                return Err(WorktreeError::Locked {
                    task_id: task_id.to_string(),
                    reason: w.lock_reason.clone(),
                });
            }
        }

        let path_str = to_git_path(&worktree_dir);

        if force {
            let _ = run_git_sync(repo_root, &["worktree", "unlock", &path_str]);
        }

        let remove_result = if force {
            run_git_sync(repo_root, &["worktree", "remove", "--force", &path_str])
        } else {
            run_git_sync(repo_root, &["worktree", "remove", &path_str])
        };

        if remove_result.is_err() && force {
            if worktree_dir.exists() {
                let _ = std::fs::remove_dir_all(&worktree_dir);
            }
        } else {
            remove_result?;
        }

        let meta_path = metadata_path_for_task(repo_root, task_id);
        if meta_path.exists() {
            let _ = std::fs::remove_file(&meta_path);
        }

        let _ = run_git_sync(repo_root, &["worktree", "prune"]);

        Ok(())
    }

    pub fn diff_worktree_sync(worktree_path: &Path, base_branch: &str) -> Result<String> {
        validate_branch_name(base_branch)?;

        if !worktree_path.exists() {
            return Err(WorktreeError::NotFound(
                worktree_path.to_string_lossy().to_string(),
            ));
        }

        let diff_output = run_git_sync(worktree_path, &["diff", base_branch])?;
        Ok(diff_output)
    }

    pub fn lock_worktree_sync(repo_root: &Path, task_id: &str, reason: Option<&str>) -> Result<()> {
        validate_task_id(task_id)?;

        let worktree_dir = find_worktree_path_or_default(repo_root, task_id);
        if !worktree_dir.exists() {
            return Err(WorktreeError::NotFound(task_id.to_string()));
        }

        let path_str = to_git_path(&worktree_dir);
        if let Some(r) = reason {
            run_git_sync(repo_root, &["worktree", "lock", "--reason", r, &path_str])?;
        } else {
            run_git_sync(repo_root, &["worktree", "lock", &path_str])?;
        }

        update_metadata_lock_sync(repo_root, task_id, true, reason.map(|s| s.to_string()))?;

        Ok(())
    }

    pub fn unlock_worktree_sync(repo_root: &Path, task_id: &str) -> Result<()> {
        validate_task_id(task_id)?;

        let worktree_dir = find_worktree_path_or_default(repo_root, task_id);
        if !worktree_dir.exists() {
            return Err(WorktreeError::NotFound(task_id.to_string()));
        }

        let path_str = to_git_path(&worktree_dir);
        run_git_sync(repo_root, &["worktree", "unlock", &path_str])?;

        update_metadata_lock_sync(repo_root, task_id, false, None)?;

        Ok(())
    }

    pub fn prune_stale_worktrees_sync(
        repo_root: &Path,
        max_age: std::time::Duration,
    ) -> Result<Vec<String>> {
        let worktrees = Self::list_worktrees_sync(repo_root)?;
        let now_ms = current_epoch_ms();
        let max_age_ms = max_age.as_millis() as u64;

        let mut pruned = Vec::new();

        for wt in worktrees {
            if wt.is_locked {
                continue;
            }

            let is_stale = if let Some(created_ms) = wt.created_at_ms {
                now_ms.saturating_sub(created_ms) >= max_age_ms
            } else if let Ok(metadata) = std::fs::metadata(&wt.path) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                        elapsed >= max_age
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                true
            };

            if is_stale
                && (Self::remove_worktree_sync(repo_root, &wt.task_id, false).is_ok()
                    || Self::remove_worktree_sync(repo_root, &wt.task_id, true).is_ok())
            {
                pruned.push(wt.task_id);
            }
        }

        let _ = run_git_sync(repo_root, &["worktree", "prune"]);

        Ok(pruned)
    }

    pub fn repair_worktrees_sync(repo_root: &Path) -> Result<()> {
        let _ = run_git_sync(repo_root, &["worktree", "repair"]);
        let _ = run_git_sync(repo_root, &["worktree", "prune"]);
        Ok(())
    }
}

// --- Helper Functions ---

fn validate_task_id(task_id: &str) -> Result<()> {
    let trimmed = task_id.trim();
    if trimmed.is_empty() {
        return Err(WorktreeError::InvalidTaskId {
            task_id: task_id.to_string(),
            reason: "task ID cannot be empty".to_string(),
        });
    }
    if trimmed == "." || trimmed == ".." {
        return Err(WorktreeError::InvalidTaskId {
            task_id: task_id.to_string(),
            reason: "task ID cannot be '.' or '..'".to_string(),
        });
    }
    if trimmed.chars().any(|c| {
        c == '/'
            || c == '\\'
            || c == ':'
            || c == '*'
            || c == '?'
            || c == '"'
            || c == '<'
            || c == '>'
            || c == '|'
    }) {
        return Err(WorktreeError::InvalidTaskId {
            task_id: task_id.to_string(),
            reason: "task ID contains invalid path characters".to_string(),
        });
    }
    Ok(())
}

fn validate_branch_name(branch_name: &str) -> Result<()> {
    let trimmed = branch_name.trim();
    if trimmed.is_empty() {
        return Err(WorktreeError::InvalidBranch {
            branch: branch_name.to_string(),
            reason: "branch name cannot be empty".to_string(),
        });
    }
    if trimmed.starts_with('-') {
        return Err(WorktreeError::InvalidBranch {
            branch: branch_name.to_string(),
            reason: "branch name cannot start with '-'".to_string(),
        });
    }
    if trimmed.contains("..")
        || trimmed.chars().any(|c| {
            c.is_whitespace()
                || c == '~'
                || c == '^'
                || c == ':'
                || c == '?'
                || c == '*'
                || c == '['
                || c == '\\'
        })
    {
        return Err(WorktreeError::InvalidBranch {
            branch: branch_name.to_string(),
            reason: "branch name contains invalid git ref characters".to_string(),
        });
    }
    Ok(())
}

fn worktree_dir_for_task(repo_root: &Path, task_id: &str) -> PathBuf {
    repo_root.join(ORBIT_WORKTREES_DIR).join(task_id)
}

fn metadata_path_for_task(repo_root: &Path, task_id: &str) -> PathBuf {
    repo_root
        .join(ORBIT_WORKTREES_DIR)
        .join(format!(".{}.meta.json", task_id))
}

fn find_worktree_path_or_default(repo_root: &Path, task_id: &str) -> PathBuf {
    let orbit_path = repo_root.join(ORBIT_WORKTREES_DIR).join(task_id);
    if orbit_path.exists() {
        return orbit_path;
    }
    let cursor_path = repo_root.join(CURSOR_WORKTREES_DIR).join(task_id);
    if cursor_path.exists() {
        return cursor_path;
    }
    orbit_path
}

fn to_git_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn normalize_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s);
    let s = s.strip_prefix(r"\\?\").unwrap_or(s);
    let s = s.trim_end_matches('/');
    PathBuf::from(s)
}

fn paths_are_equivalent(p1: &Path, p2: &Path) -> bool {
    let s1 = normalize_path(p1).to_string_lossy().to_lowercase();
    let s2 = normalize_path(p2).to_string_lossy().to_lowercase();
    s1 == s2
}

fn extract_task_id(worktree_path: &Path) -> String {
    let path_str = worktree_path.to_string_lossy().replace('\\', "/");
    for marker in &["/.orbit/worktrees/", "/.cursor/worktrees/"] {
        if let Some(pos) = path_str.find(marker) {
            let sub = &path_str[pos + marker.len()..];
            let segment = sub.split('/').next().unwrap_or(sub);
            if !segment.is_empty() {
                return segment.to_string();
            }
        }
    }

    worktree_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn read_metadata_file(repo_root: &Path, task_id: &str) -> (Option<u64>, bool, Option<String>) {
    let meta_path = metadata_path_for_task(repo_root, task_id);
    if let Ok(contents) = std::fs::read_to_string(&meta_path) {
        if let Ok(meta) = serde_json::from_str::<WorktreeMetadata>(&contents) {
            return (Some(meta.created_at_ms), meta.is_locked, meta.lock_reason);
        }
    }
    (None, false, None)
}

async fn update_metadata_lock_async(
    repo_root: &Path,
    task_id: &str,
    locked: bool,
    reason: Option<String>,
) -> Result<()> {
    let meta_path = metadata_path_for_task(repo_root, task_id);
    let mut meta = if meta_path.exists() {
        let contents = tokio::fs::read_to_string(&meta_path).await?;
        serde_json::from_str::<WorktreeMetadata>(&contents).unwrap_or_else(|_| WorktreeMetadata {
            task_id: task_id.to_string(),
            branch: "unknown".to_string(),
            created_at_ms: current_epoch_ms(),
            is_locked: locked,
            lock_reason: reason.clone(),
        })
    } else {
        WorktreeMetadata {
            task_id: task_id.to_string(),
            branch: "unknown".to_string(),
            created_at_ms: current_epoch_ms(),
            is_locked: locked,
            lock_reason: reason.clone(),
        }
    };

    meta.is_locked = locked;
    meta.lock_reason = reason;

    if let Ok(json) = serde_json::to_string_pretty(&meta) {
        let _ = tokio::fs::write(&meta_path, json).await;
    }
    Ok(())
}

fn update_metadata_lock_sync(
    repo_root: &Path,
    task_id: &str,
    locked: bool,
    reason: Option<String>,
) -> Result<()> {
    let meta_path = metadata_path_for_task(repo_root, task_id);
    let mut meta = if meta_path.exists() {
        let contents = std::fs::read_to_string(&meta_path)?;
        serde_json::from_str::<WorktreeMetadata>(&contents).unwrap_or_else(|_| WorktreeMetadata {
            task_id: task_id.to_string(),
            branch: "unknown".to_string(),
            created_at_ms: current_epoch_ms(),
            is_locked: locked,
            lock_reason: reason.clone(),
        })
    } else {
        WorktreeMetadata {
            task_id: task_id.to_string(),
            branch: "unknown".to_string(),
            created_at_ms: current_epoch_ms(),
            is_locked: locked,
            lock_reason: reason.clone(),
        }
    };

    meta.is_locked = locked;
    meta.lock_reason = reason;

    if let Ok(json) = serde_json::to_string_pretty(&meta) {
        let _ = std::fs::write(&meta_path, json);
    }
    Ok(())
}

/// Parses the output of `git worktree list --porcelain`.
pub fn parse_worktree_list_porcelain(raw: &str, repo_root: &Path) -> Vec<WorktreeInfo> {
    let mut results = Vec::new();
    let normalized_root = normalize_path(repo_root);

    let blocks = raw.split("\n\n");
    for block in blocks {
        let trimmed = block.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut path_opt: Option<PathBuf> = None;
        let mut head_opt: Option<String> = None;
        let mut branch_opt: Option<String> = None;
        let mut is_locked = false;
        let mut lock_reason: Option<String> = None;

        for line in trimmed.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("worktree ") {
                path_opt = Some(PathBuf::from(rest.trim()));
            } else if let Some(rest) = line.strip_prefix("HEAD ") {
                head_opt = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("branch ") {
                let ref_str = rest.trim();
                let clean_branch = ref_str.strip_prefix("refs/heads/").unwrap_or(ref_str);
                branch_opt = Some(clean_branch.to_string());
            } else if line == "detached" {
                branch_opt = Some("detached".to_string());
            } else if let Some(rest) = line.strip_prefix("locked ") {
                is_locked = true;
                let reason = rest.trim();
                if !reason.is_empty() {
                    lock_reason = Some(reason.to_string());
                }
            } else if line == "locked" {
                is_locked = true;
            }
        }

        if let Some(path) = path_opt {
            let normalized_path = normalize_path(&path);

            if paths_are_equivalent(&normalized_path, &normalized_root) {
                continue;
            }

            let task_id = extract_task_id(&normalized_path);
            let (meta_time, meta_locked, meta_reason) = read_metadata_file(repo_root, &task_id);

            let final_locked = is_locked || meta_locked;
            let final_reason = lock_reason.or(meta_reason);

            results.push(WorktreeInfo {
                task_id,
                branch: branch_opt.unwrap_or_else(|| "unknown".to_string()),
                path,
                head_commit: head_opt,
                is_locked: final_locked,
                lock_reason: final_reason,
                created_at_ms: meta_time,
            });
        }
    }

    results
}

// --- Subprocess Execution Helpers ---

async fn run_git_async(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| WorktreeError::Git(format!("Failed to execute git process: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let err_msg = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        return Err(WorktreeError::Git(format!(
            "git {} failed (exit code {:?}): {}",
            args.join(" "),
            output.status.code(),
            err_msg
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn run_git_status_check(cwd: &Path, args: &[&str]) -> bool {
    tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_git_sync(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| WorktreeError::Git(format!("Failed to execute git process: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let err_msg = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        return Err(WorktreeError::Git(format!(
            "git {} failed (exit code {:?}): {}",
            args.join(" "),
            output.status.code(),
            err_msg
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_git_status_check_sync(cwd: &Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_task_id() {
        assert!(validate_task_id("task-123").is_ok());
        assert!(validate_task_id("task_abc-456").is_ok());
        assert!(validate_task_id("task.1").is_ok());

        assert!(validate_task_id("").is_err());
        assert!(validate_task_id("   ").is_err());
        assert!(validate_task_id(".").is_err());
        assert!(validate_task_id("..").is_err());
        assert!(validate_task_id("../escape").is_err());
        assert!(validate_task_id("sub/dir").is_err());
        assert!(validate_task_id("sub\\dir").is_err());
        assert!(validate_task_id("task:bad").is_err());
        assert!(validate_task_id("task*bad").is_err());
        assert!(validate_task_id("task?bad").is_err());
    }

    #[test]
    fn test_validate_branch_name() {
        assert!(validate_branch_name("main").is_ok());
        assert!(validate_branch_name("feature/worktree").is_ok());
        assert!(validate_branch_name("orbit/task-100").is_ok());

        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("   ").is_err());
        assert!(validate_branch_name("-flag").is_err());
        assert!(validate_branch_name("--option").is_err());
        assert!(validate_branch_name("branch with space").is_err());
        assert!(validate_branch_name("branch..double").is_err());
        assert!(validate_branch_name("branch~1").is_err());
        assert!(validate_branch_name("branch^2").is_err());
        assert!(validate_branch_name("branch:colon").is_err());
    }

    #[test]
    fn test_parse_worktree_list_porcelain() {
        let repo_root = Path::new("C:/repos/myrepo");
        let sample_output = r#"worktree C:/repos/myrepo
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree C:/repos/myrepo/.orbit/worktrees/task-1
HEAD 2222222222222222222222222222222222222222
branch refs/heads/orbit/task-1

worktree C:/repos/myrepo/.orbit/worktrees/task-2
HEAD 3333333333333333333333333333333333333333
branch refs/heads/orbit/task-2
locked active agent computation

worktree C:/repos/myrepo/.cursor/worktrees/task-3
HEAD 4444444444444444444444444444444444444444
detached
locked
"#;

        let worktrees = parse_worktree_list_porcelain(sample_output, repo_root);

        assert_eq!(worktrees.len(), 3);

        assert_eq!(worktrees[0].task_id, "task-1");
        assert_eq!(worktrees[0].branch, "orbit/task-1");
        assert_eq!(
            worktrees[0].head_commit.as_deref(),
            Some("2222222222222222222222222222222222222222")
        );
        assert!(!worktrees[0].is_locked);
        assert_eq!(worktrees[0].lock_reason, None);

        assert_eq!(worktrees[1].task_id, "task-2");
        assert_eq!(worktrees[1].branch, "orbit/task-2");
        assert!(worktrees[1].is_locked);
        assert_eq!(
            worktrees[1].lock_reason.as_deref(),
            Some("active agent computation")
        );

        assert_eq!(worktrees[2].task_id, "task-3");
        assert_eq!(worktrees[2].branch, "detached");
        assert!(worktrees[2].is_locked);
    }

    #[test]
    fn test_extract_task_id() {
        assert_eq!(
            extract_task_id(Path::new("C:/repo/.orbit/worktrees/my-task")),
            "my-task"
        );
        assert_eq!(
            extract_task_id(Path::new("C:\\repo\\.orbit\\worktrees\\my-task")),
            "my-task"
        );
        assert_eq!(
            extract_task_id(Path::new(
                "/home/user/repo/.cursor/worktrees/cursor-task-99"
            )),
            "cursor-task-99"
        );
        assert_eq!(
            extract_task_id(Path::new("/tmp/isolated-task")),
            "isolated-task"
        );
    }

    #[test]
    fn test_paths_are_equivalent() {
        assert!(paths_are_equivalent(
            Path::new("C:/repo/myrepo"),
            Path::new("C:\\repo\\myrepo")
        ));
        assert!(paths_are_equivalent(
            Path::new(r"\\?\C:\repo\myrepo"),
            Path::new("C:/repo/myrepo")
        ));
        assert!(paths_are_equivalent(
            Path::new("C:/repo/myrepo/"),
            Path::new("C:/repo/myrepo")
        ));
    }
}
