use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use uuid::Uuid;

use orbit_worktree::{WorktreeError, WorktreeManager};

struct TestRepo {
    path: PathBuf,
}

impl TestRepo {
    fn new() -> Self {
        let temp_dir = std::env::temp_dir().join(format!("orbit-wt-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("failed to create temp test dir");

        // Initialize git repository
        let run_cmd = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(&temp_dir)
                .output()
                .expect("failed to execute git command");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };

        run_cmd(&["init"]);
        run_cmd(&["config", "user.name", "Orbit Tester"]);
        run_cmd(&["config", "user.email", "tester@orbit.dev"]);
        run_cmd(&["config", "commit.gpgsign", "false"]);

        // Create initial commit on branch 'main'
        let file_path = temp_dir.join("README.md");
        std::fs::write(&file_path, "# Initial Repo\n").expect("write file");
        run_cmd(&["add", "README.md"]);
        run_cmd(&["commit", "-m", "initial commit"]);
        run_cmd(&["branch", "-M", "main"]);

        Self { path: temp_dir }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        // Safe best-effort cleanup of temporary test repo
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&self.path)
            .output();
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[tokio::test]
async fn test_create_and_list_worktree() {
    let repo = TestRepo::new();
    let task_id = "task-alpha";
    let branch = "orbit/task-alpha";

    let wt = WorktreeManager::create_worktree(repo.path(), task_id, branch)
        .await
        .expect("create worktree should succeed");

    assert_eq!(wt.task_id, task_id);
    assert_eq!(wt.branch, branch);
    assert!(wt.path.exists());
    assert!(wt.head_commit.is_some());
    assert!(!wt.is_locked);
    assert_eq!(wt.lock_reason, None);
    assert!(wt.created_at_ms.is_some());

    let list = WorktreeManager::list_worktrees(repo.path())
        .await
        .expect("list worktrees should succeed");

    assert_eq!(list.len(), 1);
    assert_eq!(list[0].task_id, task_id);
    assert_eq!(list[0].branch, branch);
}

#[tokio::test]
async fn test_create_with_existing_branch() {
    let repo = TestRepo::new();

    // Create a branch in the repository beforehand
    let output = Command::new("git")
        .args(["branch", "feat-existing"])
        .current_dir(repo.path())
        .output()
        .expect("git branch");
    assert!(output.status.success());

    let task_id = "task-beta";
    let wt = WorktreeManager::create_worktree(repo.path(), task_id, "feat-existing")
        .await
        .expect("create worktree on existing branch");

    assert_eq!(wt.branch, "feat-existing");
    assert!(wt.path.exists());
}

#[tokio::test]
async fn test_duplicate_task_id_fails() {
    let repo = TestRepo::new();
    let task_id = "task-dup";

    WorktreeManager::create_worktree(repo.path(), task_id, "orbit/branch-1")
        .await
        .expect("first creation succeeds");

    let err = WorktreeManager::create_worktree(repo.path(), task_id, "orbit/branch-2")
        .await
        .expect_err("second creation with same task_id should fail");

    match err {
        WorktreeError::AlreadyExists(id) => assert_eq!(id, task_id),
        other => panic!("expected AlreadyExists, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_worktree_diff() {
    let repo = TestRepo::new();
    let task_id = "task-diff";
    let branch = "orbit/task-diff";

    let wt = WorktreeManager::create_worktree(repo.path(), task_id, branch)
        .await
        .expect("create worktree");

    // Make committed edit
    let file1 = wt.path.join("file1.txt");
    std::fs::write(&file1, "line 1\nline 2\n").expect("write file1");

    let output = Command::new("git")
        .args(["add", "file1.txt"])
        .current_dir(&wt.path)
        .output()
        .expect("git add");
    assert!(output.status.success());

    let output = Command::new("git")
        .args(["commit", "-m", "add file1"])
        .current_dir(&wt.path)
        .output()
        .expect("git commit");
    assert!(output.status.success());

    // Make uncommitted edit
    let file2 = wt.path.join("README.md");
    std::fs::write(&file2, "# Initial Repo\nModified by agent!\n").expect("modify README");

    let diff = WorktreeManager::diff_worktree(&wt.path, "main")
        .await
        .expect("diff worktree should succeed");

    assert!(diff.contains("file1.txt"));
    assert!(diff.contains("line 1"));
    assert!(diff.contains("Modified by agent!"));
}

#[tokio::test]
async fn test_safe_lock_and_unlock_lifecycle() {
    let repo = TestRepo::new();
    let task_id = "task-lock-test";
    let branch = "orbit/task-lock";

    let wt = WorktreeManager::create_worktree(repo.path(), task_id, branch)
        .await
        .expect("create worktree");

    // Lock the worktree with a reason
    WorktreeManager::lock_worktree(repo.path(), task_id, Some("in flight subagent run"))
        .await
        .expect("lock worktree");

    let list = WorktreeManager::list_worktrees(repo.path())
        .await
        .expect("list worktrees");
    assert_eq!(list.len(), 1);
    assert!(list[0].is_locked);
    assert_eq!(
        list[0].lock_reason.as_deref(),
        Some("in flight subagent run")
    );

    // Attempt removal without force — MUST fail with Locked
    let remove_err = WorktreeManager::remove_worktree(repo.path(), task_id, false)
        .await
        .expect_err("removing locked worktree without force must fail");

    match remove_err {
        WorktreeError::Locked { task_id: id, .. } => assert_eq!(id, task_id),
        other => panic!("expected Locked error, got: {other:?}"),
    }

    // Worktree directory must still exist safely
    assert!(wt.path.exists());

    // Unlock worktree
    WorktreeManager::unlock_worktree(repo.path(), task_id)
        .await
        .expect("unlock worktree");

    let list_after_unlock = WorktreeManager::list_worktrees(repo.path())
        .await
        .expect("list worktrees");
    assert!(!list_after_unlock[0].is_locked);

    // Remove without force now succeeds
    WorktreeManager::remove_worktree(repo.path(), task_id, false)
        .await
        .expect("remove after unlock should succeed");

    assert!(!wt.path.exists());

    let final_list = WorktreeManager::list_worktrees(repo.path())
        .await
        .expect("list worktrees");
    assert!(final_list.is_empty());
}

#[tokio::test]
async fn test_force_remove_locked_and_dirty() {
    let repo = TestRepo::new();
    let task_id = "task-dirty";
    let branch = "orbit/task-dirty";

    let wt = WorktreeManager::create_worktree(repo.path(), task_id, branch)
        .await
        .expect("create worktree");

    WorktreeManager::lock_worktree(repo.path(), task_id, Some("lock test"))
        .await
        .expect("lock");

    // Create uncommitted dirty file
    std::fs::write(wt.path.join("dirty.txt"), "dirty uncommitted").expect("write dirty");

    // Force removal must succeed despite lock and dirty working tree
    WorktreeManager::remove_worktree(repo.path(), task_id, true)
        .await
        .expect("force remove should succeed");

    assert!(!wt.path.exists());

    let list = WorktreeManager::list_worktrees(repo.path())
        .await
        .expect("list");
    assert!(list.is_empty());
}

#[tokio::test]
async fn test_prune_stale_worktrees() {
    let repo = TestRepo::new();

    let _stale_wt = WorktreeManager::create_worktree(repo.path(), "task-stale", "orbit/stale")
        .await
        .expect("create stale");

    let _active_wt = WorktreeManager::create_worktree(repo.path(), "task-active", "orbit/active")
        .await
        .expect("create active");

    // Lock the active one
    WorktreeManager::lock_worktree(repo.path(), "task-active", Some("running"))
        .await
        .expect("lock active");

    // Prune with 0 duration: stale is pruned, active is preserved!
    let pruned = WorktreeManager::prune_stale_worktrees(repo.path(), Duration::from_millis(0))
        .await
        .expect("prune stale worktrees");

    assert!(pruned.contains(&"task-stale".to_string()));
    assert!(!pruned.contains(&"task-active".to_string()));

    let remaining = WorktreeManager::list_worktrees(repo.path())
        .await
        .expect("list remaining");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].task_id, "task-active");
}

#[tokio::test]
async fn test_repair_worktrees() {
    let repo = TestRepo::new();
    let res = WorktreeManager::repair_worktrees(repo.path()).await;
    assert!(res.is_ok());
}

#[test]
fn test_sync_api_lifecycle() {
    let repo = TestRepo::new();
    let task_id = "task-sync";
    let branch = "orbit/task-sync";

    let wt =
        WorktreeManager::create_worktree_sync(repo.path(), task_id, branch).expect("create sync");
    assert_eq!(wt.task_id, task_id);
    assert!(wt.path.exists());

    let list = WorktreeManager::list_worktrees_sync(repo.path()).expect("list sync");
    assert_eq!(list.len(), 1);

    WorktreeManager::lock_worktree_sync(repo.path(), task_id, Some("sync lock"))
        .expect("lock sync");
    let lock_err = WorktreeManager::remove_worktree_sync(repo.path(), task_id, false);
    assert!(lock_err.is_err());

    WorktreeManager::unlock_worktree_sync(repo.path(), task_id).expect("unlock sync");
    WorktreeManager::remove_worktree_sync(repo.path(), task_id, false).expect("remove sync");
    assert!(!wt.path.exists());
}

#[tokio::test]
async fn test_instance_api() {
    let repo = TestRepo::new();
    let mgr = WorktreeManager::new(repo.path());

    let wt = mgr.create("task-inst", "orbit/inst").await.expect("create");
    assert_eq!(wt.task_id, "task-inst");

    let list = mgr.list().await.expect("list");
    assert_eq!(list.len(), 1);

    mgr.lock("task-inst", Some("busy")).await.expect("lock");
    assert!(mgr.remove("task-inst", false).await.is_err());

    mgr.unlock("task-inst").await.expect("unlock");
    mgr.remove("task-inst", false).await.expect("remove");
}
