pub mod actor;
pub mod checkpoint;
pub mod error;
pub mod handle;
pub mod state;

pub use actor::{PendingApproval, ThreadActor, ThreadCommand, ThreadEvent};
pub use checkpoint::{
    CasStore, Checkpoint, CheckpointManager, FileRollbackAction, RollbackPlan, RollbackSummary,
};
pub use error::{Result, RuntimeError};
pub use handle::ThreadHandle;
pub use state::{
    AwaitingApproval, Completed, Idle, Streaming, ThreadStateInfo, ThreadStateKind,
    ThreadStateMachine, ToolExecuting,
};
pub use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_common::{ChatMode, ChatRole};
    use orbit_llm::events::{Completed as LlmCompleted, StreamEvent, TextDelta};
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_handle_send_and_stream_completion() {
        let (handle, join_handle) = ThreadHandle::spawn(ChatMode::Normal);
        let mut rx = handle.subscribe();

        let turn_id = handle
            .send_message("Hello runtime!")
            .await
            .expect("send message failed");

        let state = handle.get_state().await.expect("get state failed");
        assert_eq!(state.state_kind, ThreadStateKind::Streaming);
        assert_eq!(state.turn_id, Some(turn_id));

        // Feed streaming text delta and completion
        handle
            .feed_stream_events(vec![
                StreamEvent::TextDelta(TextDelta::new("Hi there ")),
                StreamEvent::TextDelta(TextDelta::new("from assistant!")),
                StreamEvent::Completed(LlmCompleted::new(Some("stop".into()), None)),
            ])
            .await
            .expect("feed events failed");

        let state_after = handle.get_state().await.expect("get state failed");
        assert_eq!(state_after.state_kind, ThreadStateKind::Completed);

        let messages = handle.get_messages().await.expect("get messages failed");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[0].content, "Hello runtime!");
        assert_eq!(messages[1].role, ChatRole::Assistant);
        assert_eq!(messages[1].content, "Hi there from assistant!");

        // Verify broadcast events were received
        let mut event_count = 0;
        while let Ok(_event) = rx.try_recv() {
            event_count += 1;
        }
        assert!(event_count >= 4);

        handle.shutdown().await.expect("shutdown failed");
        let _ = join_handle.await;
    }

    #[tokio::test]
    async fn test_handle_checkpoint_integration() {
        let (handle, join_handle) = ThreadHandle::spawn(ChatMode::Agent);

        let mut files = HashMap::new();
        files.insert("config.json".to_string(), "{\"version\": 1}".to_string());

        let cp_id = handle
            .create_checkpoint(Some("initial config".into()), files)
            .await
            .expect("create checkpoint failed");

        assert!(!cp_id.is_nil());

        let mut files_v2 = HashMap::new();
        files_v2.insert("config.json".to_string(), "{\"version\": 2}".to_string());
        handle
            .create_checkpoint(Some("updated config".into()), files_v2)
            .await
            .expect("create checkpoint v2 failed");

        let undo_plan = handle.undo().await.expect("undo failed");
        assert_eq!(undo_plan.summary.modified, 1);

        let redo_plan = handle.redo().await.expect("redo failed");
        assert_eq!(redo_plan.summary.modified, 1);

        handle.shutdown().await.expect("shutdown failed");
        let _ = join_handle.await;
    }
}
