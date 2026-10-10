use serde::{Deserialize, Serialize};

/// Minimal per-message projection the boundary selector needs.
/// Ported from `compactionHelpers.ts:CompactionMsgInfo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionMsgInfo {
    /// Number of characters this message contributes to the LLM payload.
    pub send_chars: usize,
    /// True for 'user' messages — the only safe points to start the kept tail (no orphaned tool_result).
    pub is_user_boundary: bool,
}

impl CompactionMsgInfo {
    pub fn new(send_chars: usize, is_user_boundary: bool) -> Self {
        Self {
            send_chars,
            is_user_boundary,
        }
    }

    pub fn user(send_chars: usize) -> Self {
        Self {
            send_chars,
            is_user_boundary: true,
        }
    }

    pub fn assistant(send_chars: usize) -> Self {
        Self {
            send_chars,
            is_user_boundary: false,
        }
    }
}

impl From<&orbit_common::ChatMessage> for CompactionMsgInfo {
    fn from(msg: &orbit_common::ChatMessage) -> Self {
        let is_user = msg.role == orbit_common::ChatRole::User;
        let mut chars = msg.content.len();
        if let Some(r) = &msg.reasoning {
            chars += r.len();
        }
        for tc in &msg.tool_calls {
            chars += tc.name.len() + tc.arguments.len();
        }
        if let Some(tr) = &msg.tool_result {
            chars += tr.call_id.len();
        }
        Self {
            send_chars: chars,
            is_user_boundary: is_user,
        }
    }
}

/// Parameters for choosing context compaction boundary.
/// Ported from `compactionHelpers.ts:CompactionBoundaryParams`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionBoundaryParams<'a> {
    pub messages: &'a [CompactionMsgInfo],
    /// Index to start summarizing from (exclusive of anything kept as the verbatim task head).
    pub start_idx: usize,
    /// Target size (in chars) of the recent transcript to keep verbatim.
    pub target_tail_chars: usize,
    /// Minimum number of messages that must be summarized for compaction to be worthwhile.
    pub min_range: usize,
}

/// Choose the index in `messages` where the kept (verbatim) tail begins.
/// Everything in `[start_idx, boundary)` gets summarized; `[boundary, end)` is kept as-is.
///
/// The boundary is always a user message (or `None` when none qualifies) so the kept tail never
/// begins with an orphaned tool_result whose tool_use was summarized away.
///
/// Returns `None` when compaction isn't worthwhile (no user boundary in the recent region, or the
/// range to summarize is smaller than `min_range`).
///
/// Ported from `compactionHelpers.ts:selectCompactionBoundary`.
pub fn select_compaction_boundary(params: &CompactionBoundaryParams<'_>) -> Option<usize> {
    let n = params.messages.len();
    if params.start_idx >= n {
        return None;
    }

    // Walk backward from the end accumulating chars until we've covered ~target_tail_chars.
    let mut acc = 0;
    let mut boundary = n;
    for i in (params.start_idx + 1..n).rev() {
        acc += params.messages[i].send_chars;
        if acc >= params.target_tail_chars {
            boundary = i;
            break;
        }
    }

    // Snap the boundary forward to the next user message so the kept tail starts cleanly.
    while boundary < n && !params.messages[boundary].is_user_boundary {
        boundary += 1;
    }

    if boundary >= n {
        return None; // no user boundary in the recent region
    }
    if boundary - params.start_idx < params.min_range {
        return None; // not enough to summarize
    }
    Some(boundary)
}

/// Convenience helper omitting explicit params struct.
pub fn select_compaction_boundary_simple(
    messages: &[CompactionMsgInfo],
    start_idx: usize,
    target_tail_chars: usize,
    min_range: usize,
) -> Option<usize> {
    select_compaction_boundary(&CompactionBoundaryParams {
        messages,
        start_idx,
        target_tail_chars,
        min_range,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(chars: usize) -> CompactionMsgInfo {
        CompactionMsgInfo::user(chars)
    }

    fn a(chars: usize) -> CompactionMsgInfo {
        CompactionMsgInfo::assistant(chars)
    }

    // Ported from: test/common/compactionHelpers.test.ts:
    // "keeps recent ~targetTailChars and snaps boundary to a user message"
    #[test]
    fn test_keeps_recent_target_tail_chars_and_snaps_boundary_to_user_message() {
        // idx:   0       1       2       3       4       5       6       7
        let messages = [
            u(100),
            a(500),
            a(500),
            u(100),
            a(500),
            a(500),
            u(100),
            a(500),
        ];
        // startIdx 1, want to keep ~600 chars of tail. Walking back: 7(500),6(100)=600 -> boundary=6 (already user).
        let b = select_compaction_boundary(&CompactionBoundaryParams {
            messages: &messages,
            start_idx: 1,
            target_tail_chars: 600,
            min_range: 2,
        });
        assert_eq!(b, Some(6));
        assert!(messages[b.unwrap()].is_user_boundary);
    }

    // Ported from: test/common/compactionHelpers.test.ts:
    // "snaps forward past assistant/tool messages to the next user boundary"
    #[test]
    fn test_snaps_forward_past_assistant_tool_messages_to_next_user_boundary() {
        // If the char threshold lands on a non-user message, boundary advances to the next user msg.
        let messages = [u(50), u(50), a(400), a(400), u(50), a(400), a(400)];
        // startIdx 1, target 500: back 6(400),5(400)=800>=500 -> boundary=5 (assistant) -> snap fwd -> no user
        // after 5 -> 6 assistant -> reaches end -> null (no clean boundary in recent region)
        let b = select_compaction_boundary(&CompactionBoundaryParams {
            messages: &messages,
            start_idx: 1,
            target_tail_chars: 500,
            min_range: 1,
        });
        assert_eq!(b, None);
    }

    // Ported from: test/common/compactionHelpers.test.ts:
    // "returns null when the range to summarize is smaller than minRange"
    #[test]
    fn test_returns_null_when_range_to_summarize_smaller_than_min_range() {
        let messages = [u(100), u(100), u(100), u(100)];
        // startIdx 2, boundary would be 3 (user), range = 3-2 = 1 < minRange 3 -> null
        let b = select_compaction_boundary(&CompactionBoundaryParams {
            messages: &messages,
            start_idx: 2,
            target_tail_chars: 50,
            min_range: 3,
        });
        assert_eq!(b, None);
    }

    // Ported from: test/common/compactionHelpers.test.ts:
    // "never returns a non-user boundary"
    #[test]
    fn test_never_returns_a_non_user_boundary() {
        let messages = [
            u(10),
            a(10),
            a(10),
            u(10),
            a(10),
            a(10),
            u(10),
            a(10),
            a(10),
        ];
        for target in (0..100).step_by(5) {
            let b = select_compaction_boundary(&CompactionBoundaryParams {
                messages: &messages,
                start_idx: 0,
                target_tail_chars: target,
                min_range: 1,
            });
            if let Some(idx) = b {
                assert!(
                    messages[idx].is_user_boundary,
                    "target={target} produced non-user boundary {idx}"
                );
            }
        }
    }

    // Ported from: test/common/compactionHelpers.test.ts:
    // "handles empty / out-of-range startIdx safely"
    #[test]
    fn test_handles_empty_and_out_of_range_start_idx_safely() {
        assert_eq!(
            select_compaction_boundary(&CompactionBoundaryParams {
                messages: &[],
                start_idx: 0,
                target_tail_chars: 10,
                min_range: 1,
            }),
            None
        );
        let single = [u(10)];
        assert_eq!(
            select_compaction_boundary(&CompactionBoundaryParams {
                messages: &single,
                start_idx: 5,
                target_tail_chars: 10,
                min_range: 1,
            }),
            None
        );
    }
}
