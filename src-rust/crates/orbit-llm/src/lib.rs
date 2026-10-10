pub mod budget;
pub mod compaction;
pub mod events;
pub mod sse;

pub use budget::{
    calculate_input_char_budget, calculate_reserved_output_tokens, ContextBudget, CHARS_PER_TOKEN,
    DEFAULT_RESERVED_OUTPUT_TOKENS, MIN_INPUT_CHAR_BUDGET, MIN_RESERVED_OUTPUT_TOKENS,
};
pub use compaction::{
    select_compaction_boundary, select_compaction_boundary_simple, CompactionBoundaryParams,
    CompactionMsgInfo,
};
pub use events::{
    AccumulatedTool, Completed, ReasoningDelta, StreamEvent, TextDelta, ToolCallAccumulator,
    ToolCallDelta, UsageUpdate,
};
pub use sse::{
    parse_sse, parse_sse_bytes, SseEvent, SseParser, DEFAULT_MAX_BUFFER_SIZE,
    DEFAULT_MAX_DATA_LINES,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_e2e_reexports() {
        let budget = ContextBudget::calculate(128_000, None);
        assert_eq!(budget.reserved_output_tokens, 4_096);

        let msg = CompactionMsgInfo::user(100);
        assert!(msg.is_user_boundary);

        let event = StreamEvent::text("hello");
        assert!(matches!(event, StreamEvent::TextDelta(_)));

        let sse = parse_sse("data: test\n\n");
        assert_eq!(sse.len(), 1);
        assert_eq!(sse[0].data, "test");
    }
}
