use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileGitStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFileChange {
    pub path: String,
    pub status: FileGitStatus,
    pub is_staged: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitRepositoryState {
    pub branch: String,
    pub changes: Vec<GitFileChange>,
}

pub struct GitScmEngine;

impl GitScmEngine {
    /// Parses output from `git status --porcelain` into typed changes
    pub fn parse_porcelain_status(raw: &str) -> Vec<GitFileChange> {
        let mut changes = Vec::new();

        for line in raw.lines() {
            if line.len() < 4 {
                continue;
            }

            let staged_char = line.as_bytes()[0];
            let worktree_char = line.as_bytes()[1];
            let path = line[3..].trim().to_string();

            if staged_char == b'?' && worktree_char == b'?' {
                changes.push(GitFileChange {
                    path,
                    status: FileGitStatus::Untracked,
                    is_staged: false,
                });
                continue;
            }

            if staged_char != b' ' && staged_char != b'?' {
                let status = match staged_char {
                    b'M' => FileGitStatus::Modified,
                    b'A' => FileGitStatus::Added,
                    b'D' => FileGitStatus::Deleted,
                    b'R' => FileGitStatus::Renamed,
                    _ => FileGitStatus::Modified,
                };
                changes.push(GitFileChange {
                    path: path.clone(),
                    status,
                    is_staged: true,
                });
            }

            if worktree_char != b' ' && worktree_char != b'?' {
                let status = match worktree_char {
                    b'M' => FileGitStatus::Modified,
                    b'D' => FileGitStatus::Deleted,
                    _ => FileGitStatus::Modified,
                };
                changes.push(GitFileChange {
                    path,
                    status,
                    is_staged: false,
                });
            }
        }

        changes
    }

    /// Formats prompt context for LLM commit message generation
    pub fn generate_commit_prompt(changes: &[GitFileChange], diff_summary: &str) -> String {
        let mut prompt = String::new();
        prompt.push_str("Generate a concise, conventional commit message (e.g. feat:, fix:, chore:, refactor:)\n");
        prompt.push_str("Changed files:\n");

        for c in changes {
            let tag = if c.is_staged {
                "[staged]"
            } else {
                "[unstaged]"
            };
            let status_str = match c.status {
                FileGitStatus::Modified => "modified",
                FileGitStatus::Added => "added",
                FileGitStatus::Deleted => "deleted",
                FileGitStatus::Renamed => "renamed",
                FileGitStatus::Untracked => "untracked",
            };
            prompt.push_str(&format!("  {} {}: {}\n", tag, status_str, c.path));
        }

        if !diff_summary.trim().is_empty() {
            prompt.push_str("\nDiff excerpt:\n");
            prompt.push_str(diff_summary.trim());
            prompt.push('\n');
        }

        prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_porcelain_status() {
        let porcelain = "M  src/main.rs\n M src/lib.rs\n?? tests/new_test.rs\nA  README.md\n";
        let changes = GitScmEngine::parse_porcelain_status(porcelain);

        assert_eq!(changes.len(), 4);
        assert_eq!(changes[0].path, "src/main.rs");
        assert_eq!(changes[0].status, FileGitStatus::Modified);
        assert!(changes[0].is_staged);

        assert_eq!(changes[1].path, "src/lib.rs");
        assert_eq!(changes[1].status, FileGitStatus::Modified);
        assert!(!changes[1].is_staged);

        assert_eq!(changes[2].path, "tests/new_test.rs");
        assert_eq!(changes[2].status, FileGitStatus::Untracked);

        assert_eq!(changes[3].path, "README.md");
        assert_eq!(changes[3].status, FileGitStatus::Added);
        assert!(changes[3].is_staged);
    }

    #[test]
    fn test_generate_commit_prompt() {
        let changes = vec![GitFileChange {
            path: "src/main.rs".to_string(),
            status: FileGitStatus::Modified,
            is_staged: true,
        }];

        let prompt = GitScmEngine::generate_commit_prompt(&changes, "+ println!(\"hello\");");
        assert!(prompt.contains("Generate a concise, conventional commit message"));
        assert!(prompt.contains("[staged] modified: src/main.rs"));
        assert!(prompt.contains("Diff excerpt:"));
    }
}
