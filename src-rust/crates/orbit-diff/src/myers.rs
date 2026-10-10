use crate::hunk::{DiffChange, DiffHunk, DiffOp, DiffOutput, DiffSummary};
use similar::{ChangeTag, TextDiff};

pub fn compute_line_diff(old_text: &str, new_text: &str) -> DiffOutput {
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut hunks = Vec::new();
    let mut summary = DiffSummary::default();

    for group in diff.grouped_ops(3) {
        let mut changes = Vec::new();
        let mut old_start = 0;
        let mut old_lines = 0;
        let mut new_start = 0;
        let mut new_lines = 0;

        for op in &group {
            for change in diff.iter_changes(op) {
                let (diff_op, old_idx, new_idx) = match change.tag() {
                    ChangeTag::Equal => (DiffOp::Equal, change.old_index(), change.new_index()),
                    ChangeTag::Delete => {
                        summary.deletions += 1;
                        (DiffOp::Delete, change.old_index(), None)
                    }
                    ChangeTag::Insert => {
                        summary.additions += 1;
                        (DiffOp::Insert, None, change.new_index())
                    }
                };

                if old_start == 0 {
                    if let Some(idx) = old_idx {
                        old_start = idx + 1;
                    }
                }
                if new_start == 0 {
                    if let Some(idx) = new_idx {
                        new_start = idx + 1;
                    }
                }

                if diff_op == DiffOp::Equal || diff_op == DiffOp::Delete {
                    old_lines += 1;
                }
                if diff_op == DiffOp::Equal || diff_op == DiffOp::Insert {
                    new_lines += 1;
                }

                changes.push(DiffChange {
                    op: diff_op,
                    text: change.value().to_string(),
                    old_line: old_idx.map(|i| i + 1),
                    new_line: new_idx.map(|i| i + 1),
                });
            }
        }

        hunks.push(DiffHunk {
            old_start: if old_start == 0 { 1 } else { old_start },
            old_lines,
            new_start: if new_start == 0 { 1 } else { new_start },
            new_lines,
            changes,
        });
    }

    summary.modifications = summary.additions.min(summary.deletions);

    DiffOutput { hunks, summary }
}
