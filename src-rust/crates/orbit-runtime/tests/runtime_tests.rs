use orbit_common::{ChatMode, ChatRole, ToolCall};
use orbit_llm::events::{
    Completed as LlmCompleted, ReasoningDelta, StreamEvent, TextDelta, ToolCallDelta,
};
use orbit_runtime::{FileRollbackAction, ThreadEvent, ThreadHandle, ThreadStateKind};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_full_thread_turn_with_tool_approval_and_execution() {
    let (handle, join_handle) = ThreadHandle::spawn(ChatMode::Agent);
    let mut rx = handle.subscribe();

    // 1. Initial State: Idle
    let state = handle.get_state().await.expect("state failed");
    assert_eq!(state.state_kind, ThreadStateKind::Idle);

    // 2. Send user message: Idle -> Streaming
    let turn_id = handle
        .send_message("Please refactor parser")
        .await
        .expect("send failed");

    let state = handle.get_state().await.expect("state failed");
    assert_eq!(state.state_kind, ThreadStateKind::Streaming);
    assert_eq!(state.turn_id, Some(turn_id));

    // 3. Stream text and tool call deltas
    handle
        .feed_stream_events(vec![
            StreamEvent::ReasoningDelta(ReasoningDelta::new("Analyzing files...")),
            StreamEvent::TextDelta(TextDelta::new("I need to run edit tool.")),
            StreamEvent::ToolCallDelta(ToolCallDelta::new(
                Some(0),
                Some("call_edit_1".into()),
                Some("edit_file".into()),
                Some("{\"path\":\"parser.rs\"}".into()),
            )),
            StreamEvent::Completed(LlmCompleted::new(Some("tool_calls".into()), None)),
        ])
        .await
        .expect("feed deltas failed");

    // 4. Stream completion with pending tool calls -> AwaitingApproval
    let state = handle.get_state().await.expect("state failed");
    assert_eq!(state.state_kind, ThreadStateKind::AwaitingApproval);
    assert_eq!(state.pending_approval_count, 1);

    // 5. Submit approval -> ToolExecuting
    handle
        .submit_approval("call_edit_1", true, None)
        .await
        .expect("approval failed");

    let state = handle.get_state().await.expect("state failed");
    assert_eq!(state.state_kind, ThreadStateKind::ToolExecuting);
    assert_eq!(state.active_tools_count, 1);

    // 6. Execute tool result with continue_stream = true -> Streaming
    handle
        .execute_tool_result("call_edit_1", "Edit succeeded", false, true)
        .await
        .expect("tool result failed");

    let state = handle.get_state().await.expect("state failed");
    assert_eq!(state.state_kind, ThreadStateKind::Streaming);

    // 7. Stream final answer and complete -> Completed
    handle
        .feed_stream_events(vec![
            StreamEvent::TextDelta(TextDelta::new("Refactoring complete.")),
            StreamEvent::Completed(LlmCompleted::new(Some("stop".into()), None)),
        ])
        .await
        .expect("final feed failed");

    let state = handle.get_state().await.expect("state failed");
    assert_eq!(state.state_kind, ThreadStateKind::Completed);

    // 8. Completed -> Idle
    handle.reset_to_idle().await.expect("reset failed");
    let state = handle.get_state().await.expect("state failed");
    assert_eq!(state.state_kind, ThreadStateKind::Idle);
    assert!(state.turn_id.is_none());

    // Verify messages record the complete history: user, assistant(with tools), tool_result, assistant(final)
    let messages = handle.get_messages().await.expect("get messages failed");
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].role, ChatRole::User);
    assert_eq!(messages[1].role, ChatRole::Assistant);
    assert_eq!(messages[1].tool_calls.len(), 1);
    assert_eq!(messages[2].role, ChatRole::Tool);
    assert_eq!(messages[3].role, ChatRole::Assistant);
    assert_eq!(messages[3].content, "Refactoring complete.");

    // Drain and verify event stream
    let mut event_types = Vec::new();
    while let Ok(event) = rx.try_recv() {
        match event {
            ThreadEvent::StateChanged { from, to, .. } => {
                event_types.push(format!("state:{:?}->{:?}", from, to));
            }
            ThreadEvent::ToolApprovalResolved { approved, .. } => {
                event_types.push(format!("approved:{approved}"));
            }
            ThreadEvent::ToolExecutionFinished { is_error, .. } => {
                event_types.push(format!("tool_finished:err={is_error}"));
            }
            _ => {}
        }
    }

    assert!(event_types.contains(&"state:Idle->Streaming".into()));
    assert!(event_types.contains(&"state:Streaming->AwaitingApproval".into()));
    assert!(event_types.contains(&"state:AwaitingApproval->ToolExecuting".into()));
    assert!(event_types.contains(&"state:ToolExecuting->Streaming".into()));
    assert!(event_types.contains(&"state:Streaming->Completed".into()));
    assert!(event_types.contains(&"state:Completed->Idle".into()));
    assert!(event_types.contains(&"approved:true".into()));
    assert!(event_types.contains(&"tool_finished:err=false".into()));

    handle.shutdown().await.expect("shutdown failed");
    let _ = join_handle.await;
}

#[tokio::test]
async fn test_tool_rejection_path() {
    let (handle, join_handle) = ThreadHandle::spawn(ChatMode::Agent);

    handle.send_message("Format hard drive").await.unwrap();

    handle
        .request_tool_approval(vec![ToolCall {
            id: "format_call".into(),
            name: "format_disk".into(),
            arguments: "{\"force\": true}".into(),
        }])
        .await
        .unwrap();

    let state = handle.get_state().await.unwrap();
    assert_eq!(state.state_kind, ThreadStateKind::AwaitingApproval);

    // User rejects the tool call
    handle
        .submit_approval("format_call", false, Some("Forbidden operation".into()))
        .await
        .unwrap();

    let state = handle.get_state().await.unwrap();
    assert_eq!(state.state_kind, ThreadStateKind::Completed);
    assert_eq!(state.last_finish_reason.as_deref(), Some("tool_rejected"));

    let messages = handle.get_messages().await.unwrap();
    assert_eq!(messages.last().unwrap().role, ChatRole::Tool);
    assert!(messages
        .last()
        .unwrap()
        .content
        .contains("Forbidden operation"));

    handle.shutdown().await.unwrap();
    let _ = join_handle.await;
}

#[tokio::test]
async fn test_graceful_and_instant_cancellation() {
    let (handle, join_handle) = ThreadHandle::spawn(ChatMode::Normal);
    let mut rx = handle.subscribe();

    handle.send_message("Write a very long poem").await.unwrap();

    let token = handle
        .get_cancellation_token()
        .await
        .unwrap()
        .expect("token should be present");

    assert!(!token.is_cancelled());

    // Emit partial text
    handle
        .feed_stream_events(vec![StreamEvent::TextDelta(TextDelta::new(
            "In the quiet deep...",
        ))])
        .await
        .unwrap();

    let state = handle.get_state().await.unwrap();
    assert_eq!(state.state_kind, ThreadStateKind::Streaming);

    // Cancel turn via handle
    let cancelled = handle.cancel().await.unwrap();
    assert!(cancelled);
    assert!(token.is_cancelled());

    let state_after = handle.get_state().await.unwrap();
    assert_eq!(state_after.state_kind, ThreadStateKind::Completed);

    // Further feeds are rejected with Cancelled
    let feed_err = handle
        .feed_stream_events(vec![StreamEvent::TextDelta(TextDelta::new("more text"))])
        .await;
    assert!(feed_err.is_err());

    // Verify TurnCancelled was broadcasted
    let mut saw_cancelled_event = false;
    while let Ok(event) = rx.try_recv() {
        if let ThreadEvent::TurnCancelled { .. } = event {
            saw_cancelled_event = true;
        }
    }
    assert!(saw_cancelled_event);

    // Can immediately start a new turn
    let next_turn = handle.send_message("New fresh prompt").await.unwrap();
    assert!(!next_turn.is_nil());

    let state_next = handle.get_state().await.unwrap();
    assert_eq!(state_next.state_kind, ThreadStateKind::Streaming);

    handle.shutdown().await.unwrap();
    let _ = join_handle.await;
}

#[tokio::test]
async fn test_cancellation_during_approval_state() {
    let (handle, join_handle) = ThreadHandle::spawn(ChatMode::Agent);

    handle.send_message("Run tests").await.unwrap();
    handle
        .request_tool_approval(vec![ToolCall {
            id: "test_call".into(),
            name: "cargo_test".into(),
            arguments: "{}".into(),
        }])
        .await
        .unwrap();

    let state = handle.get_state().await.unwrap();
    assert_eq!(state.state_kind, ThreadStateKind::AwaitingApproval);

    let cancelled = handle.cancel().await.unwrap();
    assert!(cancelled);

    let state_after = handle.get_state().await.unwrap();
    assert_eq!(state_after.state_kind, ThreadStateKind::Completed);
    assert_eq!(state_after.pending_approval_count, 0);

    handle.shutdown().await.unwrap();
    let _ = join_handle.await;
}

#[tokio::test]
async fn test_content_addressed_checkpoint_multi_turn_rollback() {
    let (handle, join_handle) = ThreadHandle::spawn(ChatMode::Agent);

    // Initial project state
    let mut files_c0 = HashMap::new();
    files_c0.insert(
        "src/lib.rs".to_string(),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
    );
    files_c0.insert("src/main.rs".to_string(), "fn main() {}".to_string());
    files_c0.insert("README.md".to_string(), "# Project".to_string());

    let c0 = handle
        .create_checkpoint(Some("initial commit".into()), files_c0)
        .await
        .unwrap();

    // Turn 1 modifies src/lib.rs and adds src/utils.rs
    let mut files_c1 = HashMap::new();
    files_c1.insert(
        "src/lib.rs".to_string(),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub fn sub(a: i32, b: i32) -> i32 { a - b }"
            .to_string(),
    );
    files_c1.insert("src/main.rs".to_string(), "fn main() {}".to_string());
    files_c1.insert("src/utils.rs".to_string(), "pub fn helper() {}".to_string());
    files_c1.insert("README.md".to_string(), "# Project".to_string());

    let c1 = handle
        .create_checkpoint(Some("add sub & helper".into()), files_c1)
        .await
        .unwrap();

    // Turn 2 removes README.md and updates src/main.rs
    let mut files_c2 = HashMap::new();
    files_c2.insert(
        "src/lib.rs".to_string(),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub fn sub(a: i32, b: i32) -> i32 { a - b }"
            .to_string(),
    );
    files_c2.insert(
        "src/main.rs".to_string(),
        "fn main() { println!(\"ready\"); }".to_string(),
    );
    files_c2.insert("src/utils.rs".to_string(), "pub fn helper() {}".to_string());

    let _c2 = handle
        .create_checkpoint(Some("remove readme & edit main".into()), files_c2)
        .await
        .unwrap();

    // Rollback from C2 to C1 (undo 1)
    let undo1 = handle.undo().await.unwrap();
    assert_eq!(undo1.to_checkpoint_id, c1);
    assert_eq!(undo1.summary.created, 1); // README.md restored
    assert_eq!(undo1.summary.modified, 1); // src/main.rs restored
    assert_eq!(undo1.summary.deleted, 0);
    assert_eq!(undo1.summary.unchanged, 2); // src/lib.rs and src/utils.rs unchanged

    // Check line diff for restored src/main.rs
    let main_diff = undo1.diff_for_file("src/main.rs").expect("diff missing");
    assert_eq!(main_diff.summary.deletions, 1);
    assert_eq!(main_diff.summary.additions, 1);

    // Rollback from C1 to C0 (undo 2)
    let undo2 = handle.undo().await.unwrap();
    assert_eq!(undo2.to_checkpoint_id, c0);
    assert_eq!(undo2.summary.deleted, 1); // src/utils.rs removed
    assert_eq!(undo2.summary.modified, 1); // src/lib.rs restored
    assert_eq!(undo2.summary.unchanged, 2); // src/main.rs and README.md unchanged

    // Redo back from C0 to C1
    let redo1 = handle.redo().await.unwrap();
    assert_eq!(redo1.to_checkpoint_id, c1);
    assert_eq!(redo1.summary.created, 1); // src/utils.rs created
    assert_eq!(redo1.summary.modified, 1); // src/lib.rs modified

    // Direct rollback to C0
    let rollback_to_c0 = handle.rollback_to(c0).await.unwrap();
    assert_eq!(rollback_to_c0.to_checkpoint_id, c0);

    // While at C0, create a new branch checkpoint C3
    let mut files_c3 = HashMap::new();
    files_c3.insert("hotfix.rs".to_string(), "// hotfix".to_string());
    let c3 = handle
        .create_checkpoint(Some("hotfix branch".into()), files_c3)
        .await
        .unwrap();

    // Now redo is impossible because forward history was truncated
    let redo_err = handle.redo().await;
    assert!(redo_err.is_err());

    // Undo moves from C3 to C0
    let undo_c3 = handle.undo().await.unwrap();
    assert_eq!(undo_c3.to_checkpoint_id, c0);
    assert_eq!(undo_c3.from_checkpoint_id, Some(c3));

    // Verify rollback actions detail
    for action in &undo_c3.actions {
        match action {
            FileRollbackAction::Delete { path, .. } => {
                assert_eq!(path, "hotfix.rs");
            }
            FileRollbackAction::Create { path, .. } => {
                assert!(path.starts_with("src/") || path == "README.md");
            }
            _ => {}
        }
    }

    handle.shutdown().await.unwrap();
    let _ = join_handle.await;
}

#[tokio::test]
async fn test_multiple_broadcast_subscribers_order_integrity() {
    let (handle, join_handle) = ThreadHandle::spawn(ChatMode::Normal);
    let sub1 = handle.subscribe();
    let sub2 = handle.subscribe();
    let sub3 = handle.subscribe();

    handle
        .send_message("Testing multi-subscriber")
        .await
        .unwrap();
    handle
        .feed_stream_events(vec![
            StreamEvent::TextDelta(TextDelta::new("Token 1 ")),
            StreamEvent::TextDelta(TextDelta::new("Token 2 ")),
            StreamEvent::Completed(LlmCompleted::new(Some("stop".into()), None)),
        ])
        .await
        .unwrap();

    let collect_events = |mut rx: tokio::sync::broadcast::Receiver<ThreadEvent>| async move {
        let mut list = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            list.push(ev);
        }
        list
    };

    let events1 = collect_events(sub1).await;
    let events2 = collect_events(sub2).await;
    let events3 = collect_events(sub3).await;

    assert!(!events1.is_empty());
    assert_eq!(events1.len(), events2.len());
    assert_eq!(events2.len(), events3.len());

    for i in 0..events1.len() {
        assert_eq!(events1[i], events2[i]);
        assert_eq!(events2[i], events3[i]);
    }

    handle.shutdown().await.unwrap();
    let _ = join_handle.await;
}

#[tokio::test]
async fn test_actor_disconnect_clean_handling() {
    let (handle, join_handle) = ThreadHandle::spawn(ChatMode::Normal);
    handle.shutdown().await.unwrap();
    let _ = join_handle.await;

    // Subsequent commands should return ActorDisconnected
    let res = timeout(Duration::from_millis(500), handle.send_message("Hello")).await;
    assert!(res.is_ok());
    assert!(res.unwrap().is_err());
}
