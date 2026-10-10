use crate::error::{Result, RuntimeError};
use orbit_common::{ChatMessage, ToolCall};
use orbit_llm::ToolCallAccumulator;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ponytail: compile-time typestate structs mirror runtime ThreadStateKind; add fine-grained subagent state nesting when subagent branches split.

/// Typestate marker: thread is waiting for user or command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idle;

/// Typestate marker: LLM output stream is actively consuming deltas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Streaming;

/// Typestate marker: waiting for human approval of one or more tool calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AwaitingApproval;

/// Typestate marker: tool calls are currently executing in the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolExecuting;

/// Typestate marker: generation/turn finished with completion or cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Completed;

/// Runtime enum matching the typestate markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadStateKind {
    Idle,
    Streaming,
    AwaitingApproval,
    ToolExecuting,
    Completed,
}

impl ThreadStateKind {
    pub fn can_transition_to(&self, target: ThreadStateKind) -> bool {
        use ThreadStateKind::*;
        match (self, target) {
            // Idle starts a turn into Streaming
            (Idle, Streaming) => true,
            // Streaming can transition when tool calls arrive or finish
            (Streaming, AwaitingApproval) => true,
            (Streaming, ToolExecuting) => true,
            (Streaming, Completed) => true,
            (Streaming, Idle) => true, // direct reset / cancel fallback
            // Approval can proceed to execution or abort to completed/idle
            (AwaitingApproval, ToolExecuting) => true,
            (AwaitingApproval, Completed) => true,
            (AwaitingApproval, Idle) => true,
            // Tool execution can feed back into streaming or finish
            (ToolExecuting, Streaming) => true,
            (ToolExecuting, Completed) => true,
            (ToolExecuting, Idle) => true,
            // Completed can return to idle or immediately start new turn
            (Completed, Idle) => true,
            (Completed, Streaming) => true,
            _ => false,
        }
    }

    pub fn validate_transition(from: ThreadStateKind, to: ThreadStateKind) -> Result<()> {
        if from.can_transition_to(to) {
            Ok(())
        } else {
            Err(RuntimeError::InvalidStateTransition {
                from: format!("{from:?}"),
                to: format!("{to:?}"),
                reason: format!("State transition from {from:?} to {to:?} is not allowed"),
            })
        }
    }
}

/// Information snapshot of the thread state for diagnostics and UI listeners.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadStateInfo {
    pub thread_id: Uuid,
    pub state_kind: ThreadStateKind,
    pub turn_id: Option<Uuid>,
    pub pending_approval_count: usize,
    pub active_tools_count: usize,
    pub last_finish_reason: Option<String>,
}

/// Typestate container for thread turns.
#[derive(Debug)]
pub struct ThreadStateMachine<S> {
    pub thread_id: Uuid,
    pub turn_id: Option<Uuid>,
    pub state: S,
}

impl ThreadStateMachine<Idle> {
    pub fn new(thread_id: Uuid) -> Self {
        Self {
            thread_id,
            turn_id: None,
            state: Idle,
        }
    }

    pub fn start_turn(
        self,
        user_prompt: impl Into<String>,
    ) -> (ThreadStateMachine<Streaming>, ChatMessage) {
        let turn_id = Uuid::new_v4();
        let message = ChatMessage::user(user_prompt);
        (
            ThreadStateMachine {
                thread_id: self.thread_id,
                turn_id: Some(turn_id),
                state: Streaming,
            },
            message,
        )
    }
}

#[derive(Debug, Default)]
pub struct StreamingState {
    pub text_buffer: String,
    pub reasoning_buffer: String,
    pub tool_accumulator: ToolCallAccumulator,
}

impl ThreadStateMachine<Streaming> {
    pub fn new_with_turn(thread_id: Uuid, turn_id: Uuid) -> Self {
        Self {
            thread_id,
            turn_id: Some(turn_id),
            state: Streaming,
        }
    }

    pub fn require_approval(
        self,
        _pending_tools: Vec<ToolCall>,
    ) -> ThreadStateMachine<AwaitingApproval> {
        ThreadStateMachine {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            state: AwaitingApproval,
        }
    }

    pub fn execute_tools(self, tools: Vec<ToolCall>) -> ThreadStateMachine<ToolExecuting> {
        let _ = tools;
        ThreadStateMachine {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            state: ToolExecuting,
        }
    }

    pub fn complete(self) -> ThreadStateMachine<Completed> {
        ThreadStateMachine {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            state: Completed,
        }
    }

    pub fn cancel(self) -> ThreadStateMachine<Completed> {
        ThreadStateMachine {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            state: Completed,
        }
    }
}

impl ThreadStateMachine<AwaitingApproval> {
    pub fn approve(self) -> ThreadStateMachine<ToolExecuting> {
        ThreadStateMachine {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            state: ToolExecuting,
        }
    }

    pub fn reject(self) -> ThreadStateMachine<Completed> {
        ThreadStateMachine {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            state: Completed,
        }
    }

    pub fn cancel(self) -> ThreadStateMachine<Completed> {
        ThreadStateMachine {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            state: Completed,
        }
    }
}

impl ThreadStateMachine<ToolExecuting> {
    pub fn continue_stream(self) -> ThreadStateMachine<Streaming> {
        ThreadStateMachine {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            state: Streaming,
        }
    }

    pub fn complete(self) -> ThreadStateMachine<Completed> {
        ThreadStateMachine {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            state: Completed,
        }
    }

    pub fn cancel(self) -> ThreadStateMachine<Completed> {
        ThreadStateMachine {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            state: Completed,
        }
    }
}

impl ThreadStateMachine<Completed> {
    pub fn into_idle(self) -> ThreadStateMachine<Idle> {
        ThreadStateMachine {
            thread_id: self.thread_id,
            turn_id: None,
            state: Idle,
        }
    }

    pub fn start_next_turn(
        self,
        user_prompt: impl Into<String>,
    ) -> (ThreadStateMachine<Streaming>, ChatMessage) {
        self.into_idle().start_turn(user_prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_state_transitions() {
        assert!(ThreadStateKind::Idle.can_transition_to(ThreadStateKind::Streaming));
        assert!(ThreadStateKind::Streaming.can_transition_to(ThreadStateKind::AwaitingApproval));
        assert!(ThreadStateKind::Streaming.can_transition_to(ThreadStateKind::ToolExecuting));
        assert!(ThreadStateKind::Streaming.can_transition_to(ThreadStateKind::Completed));
        assert!(ThreadStateKind::AwaitingApproval.can_transition_to(ThreadStateKind::ToolExecuting));
        assert!(ThreadStateKind::AwaitingApproval.can_transition_to(ThreadStateKind::Completed));
        assert!(ThreadStateKind::ToolExecuting.can_transition_to(ThreadStateKind::Streaming));
        assert!(ThreadStateKind::ToolExecuting.can_transition_to(ThreadStateKind::Completed));
        assert!(ThreadStateKind::Completed.can_transition_to(ThreadStateKind::Idle));
        assert!(ThreadStateKind::Completed.can_transition_to(ThreadStateKind::Streaming));
    }

    #[test]
    fn test_invalid_state_transitions() {
        assert!(!ThreadStateKind::Idle.can_transition_to(ThreadStateKind::ToolExecuting));
        assert!(!ThreadStateKind::Idle.can_transition_to(ThreadStateKind::AwaitingApproval));
        assert!(!ThreadStateKind::Completed.can_transition_to(ThreadStateKind::ToolExecuting));

        assert!(ThreadStateKind::validate_transition(
            ThreadStateKind::Idle,
            ThreadStateKind::AwaitingApproval
        )
        .is_err());
    }

    #[test]
    fn test_typestate_full_turn_lifecycle() {
        let thread_id = Uuid::new_v4();
        let idle = ThreadStateMachine::new(thread_id);

        let (streaming, user_msg) = idle.start_turn("Edit README.md");
        assert_eq!(user_msg.content, "Edit README.md");
        assert_eq!(streaming.thread_id, thread_id);
        assert!(streaming.turn_id.is_some());

        let awaiting = streaming.require_approval(vec![ToolCall {
            id: "call_1".into(),
            name: "write_file".into(),
            arguments: "{}".into(),
        }]);

        let executing = awaiting.approve();
        let streaming_back = executing.continue_stream();
        let completed = streaming_back.complete();
        let back_to_idle = completed.into_idle();
        assert!(back_to_idle.turn_id.is_none());
    }

    #[test]
    fn test_typestate_rejection_path() {
        let thread_id = Uuid::new_v4();
        let idle = ThreadStateMachine::new(thread_id);
        let (streaming, _) = idle.start_turn("Delete DB");
        let awaiting = streaming.require_approval(vec![]);
        let completed = awaiting.reject();
        let idle_again = completed.into_idle();
        assert_eq!(idle_again.thread_id, thread_id);
    }
}
