use orbit_common::{ChatMessage, CheckpointNode};
use orbit_diff::apply_block;
use orbit_llm::{
    calculate_input_char_budget, calculate_reserved_output_tokens, select_compaction_boundary,
    CompactionBoundaryParams, CompactionMsgInfo, ContextBudget, SseEvent, SseParser, StreamEvent,
    ToolCallAccumulator, ToolCallDelta, DEFAULT_RESERVED_OUTPUT_TOKENS, MIN_INPUT_CHAR_BUDGET,
    MIN_RESERVED_OUTPUT_TOKENS,
};

#[test]
fn test_sse_infinite_line_buffer_overflow_protection() {
    // Adversarial test: stream sends 50KB without any newline
    // With max_buffer_size = 1024, the parser must clear and not grow unbounded
    let mut parser = SseParser::with_limits(1024, 100);

    let huge_chunk = "a".repeat(2048);
    let events = parser.push(&huge_chunk);
    assert!(events.is_empty(), "No events when newline is absent");

    // After clearing the oversized garbage, normal SSE stream must still work
    let valid_events = parser.push("data: recovered\n\n");
    assert_eq!(valid_events.len(), 1);
    assert_eq!(valid_events[0].data, "recovered");
}

#[test]
fn test_sse_unbounded_data_lines_growth_protection() {
    // Adversarial test: server sends endless `data: line\n` with NO blank dispatch line `\n\n`
    // With max_data_lines = 5, the parser must auto-dispatch after 5 lines to prevent OOM
    let mut parser = SseParser::with_limits(1024 * 1024, 5);

    let mut yielded = Vec::new();
    for i in 0..12 {
        let evs = parser.push(&format!("data: line_{i}\n"));
        yielded.extend(evs);
    }

    // Must have dispatched batches of 5 lines instead of accumulating forever
    assert!(
        !yielded.is_empty(),
        "Parser must auto-dispatch when max_data_lines is reached"
    );
    assert_eq!(yielded.len(), 2);
    assert!(yielded[0].data.contains("line_0"));
    assert!(yielded[0].data.contains("line_4"));

    let finished = parser.finish();
    assert_eq!(finished.len(), 1);
    assert!(finished[0].data.contains("line_10"));
    assert!(finished[0].data.contains("line_11"));
}

#[test]
fn test_sse_invalid_utf8_and_multibyte_split_chunks() {
    let mut parser = SseParser::new();

    // 1. Multibyte UTF-8 characters split across byte chunks:
    // Emoji 🚀 is 4 bytes: [0xF0, 0x9F, 0x9A, 0x80]
    // Chinese characters "你好" are [0xE4, 0xBD, 0xA0, 0xE5, 0xA5, 0xBD]
    let chunk1 = b"data: hello \xf0\x9f";
    let chunk2 = b"\x9a\x80 world \xe4\xbd";
    let chunk3 = b"\xa0\xe5\xa5\xbd!\n\n";

    assert!(parser.push_bytes(chunk1).is_empty());
    assert!(parser.push_bytes(chunk2).is_empty());
    let events = parser.push_bytes(chunk3);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, "hello 🚀 world 你好!");

    // 2. Completely invalid UTF-8 bytes must not panic
    let invalid_bytes = b"data: \xff\xfe invalid\n\n";
    let events = parser.push_bytes(invalid_bytes);
    assert_eq!(events.len(), 1);
    assert!(events[0].data.contains("invalid"));
}

#[test]
fn test_sse_malformed_and_edge_case_chunks() {
    let mut parser = SseParser::new();

    // Leading space strip spec: single space stripped, extra spaces preserved
    let raw = "data:  two spaces\ndata:no space\n\n";
    let events = parser.push(raw);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, " two spaces\nno space");

    // Case-insensitive [DONE]
    assert!(SseEvent {
        event: None,
        data: "  [done]  ".into(),
        id: None,
        retry: None,
    }
    .is_done());
    assert!(SseEvent {
        event: None,
        data: "[DONE]".into(),
        id: None,
        retry: None,
    }
    .is_done());

    // Empty data payload should not panic or error in to_stream_events
    let empty_event = SseEvent {
        event: Some("ping".into()),
        data: "".into(),
        id: None,
        retry: None,
    };
    let stream_events = empty_event
        .to_stream_events()
        .expect("empty data should not fail");
    assert!(stream_events.is_empty());

    // Comments ignored
    let comment_events = parser.push(": keepalive comment\n\n");
    assert!(comment_events.is_empty());
}

#[test]
fn test_tool_call_accumulator_adversarial_name_streaming() {
    let mut acc = ToolCallAccumulator::new();

    // 1. Function name with repeated substring ("test_test" / "banana" / "execute_exec")
    // Previous buggy code with `!entry.name.contains(name)` dropped "test"!
    acc.push(&ToolCallDelta::new(
        Some(0),
        Some("call_1".into()),
        Some("test_".into()),
        None,
    ));
    acc.push(&ToolCallDelta::new(
        Some(0),
        None,
        Some("test".into()),
        Some("{\"x\":1}".into()),
    ));
    let tools = acc.into_tool_calls();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].id, "call_1");
    assert_eq!(
        tools[0].name, "test_test",
        "Function name with repeating substrings must not drop parts"
    );

    // 2. Character-by-character streaming: ["c", "a", "l", "l", "_", "a", "l", "l"]
    let mut char_acc = ToolCallAccumulator::new();
    char_acc.push(&ToolCallDelta::new(
        Some(0),
        Some("call_char".into()),
        None,
        None,
    ));
    for ch in ["c", "a", "l", "l", "_", "a", "l", "l"] {
        char_acc.push(&ToolCallDelta::new(Some(0), None, Some(ch.into()), None));
    }
    let char_tools = char_acc.into_tool_calls();
    assert_eq!(char_tools[0].name, "call_all");

    // 3. Provider repeating full function name across every chunk
    let mut repeat_acc = ToolCallAccumulator::new();
    repeat_acc.push(&ToolCallDelta::new(
        Some(0),
        Some("call_rep".into()),
        Some("read_file".into()),
        Some("{\"p".into()),
    ));
    repeat_acc.push(&ToolCallDelta::new(
        Some(0),
        Some("call_rep".into()),  // repeated ID
        Some("read_file".into()), // repeated full name
        Some("ath\":\"a\"}".into()),
    ));
    let rep_tools = repeat_acc.into_tool_calls();
    assert_eq!(rep_tools[0].name, "read_file");
    assert_eq!(rep_tools[0].arguments, "{\"path\":\"a\"}");

    // 4. Parallel tool calls across multiple indices
    let mut parallel_acc = ToolCallAccumulator::new();
    parallel_acc.push(&ToolCallDelta::new(
        Some(0),
        Some("call_a".into()),
        Some("tool_a".into()),
        Some("{}".into()),
    ));
    parallel_acc.push(&ToolCallDelta::new(
        Some(1),
        Some("call_b".into()),
        Some("tool_b".into()),
        Some("{}".into()),
    ));
    let parallel_tools = parallel_acc.into_tool_calls();
    assert_eq!(parallel_tools.len(), 2);
    assert_eq!(parallel_tools[0].name, "tool_a");
    assert_eq!(parallel_tools[1].name, "tool_b");
}

#[test]
fn test_token_math_small_medium_and_massive_context_windows() {
    // Small: 1,000 tokens
    let small = ContextBudget::calculate(1_000, None);
    assert_eq!(small.reserved_output_tokens, MIN_RESERVED_OUTPUT_TOKENS);
    assert_eq!(small.available_input_tokens, 0);
    assert_eq!(small.input_char_budget, MIN_INPUT_CHAR_BUDGET);

    // Medium: 128,000 tokens
    let medium = ContextBudget::calculate(128_000, None);
    assert_eq!(
        medium.reserved_output_tokens,
        DEFAULT_RESERVED_OUTPUT_TOKENS
    );
    assert_eq!(medium.available_input_tokens, 123_904);
    assert_eq!(medium.input_char_budget, 123_904 * 4);

    // Massive: 1,000,000 tokens (1M tokens)
    let massive_1m = ContextBudget::calculate(1_000_000, None);
    assert_eq!(
        massive_1m.reserved_output_tokens,
        DEFAULT_RESERVED_OUTPUT_TOKENS
    );
    assert_eq!(massive_1m.available_input_tokens, 995_904);
    assert_eq!(massive_1m.input_char_budget, 995_904 * 4);

    // Massive: 1M tokens with custom 64,000 reserve
    let massive_custom = ContextBudget::calculate(1_000_000, Some(64_000));
    assert_eq!(massive_custom.reserved_output_tokens, 64_000);
    assert_eq!(massive_custom.available_input_tokens, 936_000);
    assert_eq!(massive_custom.input_char_budget, 936_000 * 4);

    // Massive: 1M tokens with excessive 500,000 reserve (capped at 1/4 = 250,000)
    let massive_capped = ContextBudget::calculate(1_000_000, Some(500_000));
    assert_eq!(massive_capped.reserved_output_tokens, 250_000);
    assert_eq!(massive_capped.available_input_tokens, 750_000);
    assert_eq!(massive_capped.input_char_budget, 750_000 * 4);

    // Extreme: 10M tokens
    let extreme_10m = ContextBudget::calculate(10_000_000, None);
    assert_eq!(extreme_10m.reserved_output_tokens, 4_096);
    assert_eq!(extreme_10m.available_input_tokens, 9_995_904);
    assert_eq!(extreme_10m.input_char_budget, 9_995_904 * 4);

    // Free functions consistency
    assert_eq!(
        calculate_reserved_output_tokens(1_000_000, Some(64_000)),
        64_000
    );
    assert_eq!(
        calculate_input_char_budget(1_000_000, Some(64_000)),
        936_000 * 4
    );
}

#[test]
fn test_cross_crate_end_to_end_pipeline() {
    // 1. Simulate SSE stream delivering an LLM function call to patch code
    let stream_payload = r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"patch_call_42","function":{"name":"apply_search_replace","arguments":"{\"original\":\"let x = 1;\",\"replacement\":\"let x = 42;\"}"}}]},"finish_reason":"tool_calls"}]}

data: [DONE]

"#;

    let sse_events = orbit_llm::parse_sse(stream_payload);
    assert_eq!(sse_events.len(), 2);

    let mut acc = ToolCallAccumulator::new();
    for event in sse_events {
        let stream_events = event.to_stream_events().expect("must parse OpenAI chunk");
        for se in stream_events {
            if let StreamEvent::ToolCallDelta(tc) = se {
                acc.push(&tc);
            }
        }
    }

    let tool_calls = acc.into_tool_calls();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "patch_call_42");
    assert_eq!(tool_calls[0].name, "apply_search_replace");

    // 2. Deserialize arguments and apply diff via orbit_diff
    #[derive(serde::Deserialize)]
    struct PatchArgs {
        original: String,
        replacement: String,
    }
    let args: PatchArgs =
        serde_json::from_str(&tool_calls[0].arguments).expect("arguments must be valid JSON");

    let original_source = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
    let block = orbit_diff::SearchReplaceBlock {
        original: args.original,
        replacement: args.replacement,
    };
    let modified_source = apply_block(original_source, &block).expect("apply_block must succeed");
    assert!(modified_source.contains("let x = 42;"));

    // 3. Snapshot with orbit_common CheckpointNode
    let mut checkpoint = CheckpointNode::new(Some("Apply LLM patch".into()));
    checkpoint.record_file("src/main.rs", original_source.to_string());
    let snap_before = checkpoint.files.get("src/main.rs").unwrap().clone();

    checkpoint.record_file("src/main.rs", modified_source.clone());
    let snap_after = checkpoint.files.get("src/main.rs").unwrap().clone();

    assert_ne!(snap_before.hash, snap_after.hash);
    assert_eq!(snap_before.path, "src/main.rs");

    // 4. Verify chat history across orbit_common and compaction in orbit_llm
    let history = [
        ChatMessage::user("Please update x to 42"),
        ChatMessage::assistant("Running patch tool"),
        ChatMessage::tool_result("patch_call_42", "Successfully replaced", false),
        ChatMessage::user("Great, now run tests"),
    ];

    let compaction_infos: Vec<CompactionMsgInfo> =
        history.iter().map(CompactionMsgInfo::from).collect();
    assert_eq!(compaction_infos.len(), 4);
    assert!(compaction_infos[0].is_user_boundary);
    assert!(!compaction_infos[1].is_user_boundary);
    assert!(!compaction_infos[2].is_user_boundary);
    assert!(compaction_infos[3].is_user_boundary);

    let boundary = select_compaction_boundary(&CompactionBoundaryParams {
        messages: &compaction_infos,
        start_idx: 0,
        target_tail_chars: 10,
        min_range: 2,
    });
    // Snaps cleanly to the recent user boundary (index 3)
    assert_eq!(boundary, Some(3));

    // 5. Context budget
    let budget = ContextBudget::calculate(128_000, None);
    let total_chars: usize = history.iter().map(|m| m.content.len()).sum();
    assert!(budget.is_within_budget(total_chars));
    assert!(budget.remaining_chars(total_chars) > 0);
}
