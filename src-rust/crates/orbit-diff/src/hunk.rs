use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffOp {
    Equal,
    Delete,
    Insert,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffChange {
    pub op: DiffOp,
    pub text: String,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub changes: Vec<DiffChange>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub additions: usize,
    pub deletions: usize,
    pub modifications: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffOutput {
    pub hunks: Vec<DiffHunk>,
    pub summary: DiffSummary,
}
