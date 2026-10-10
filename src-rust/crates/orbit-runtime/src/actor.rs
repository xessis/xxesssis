use crate::checkpoint::{CheckpointManager, RollbackPlan};
use crate::error::{Result, RuntimeError};
use crate::state::{ThreadStateInfo, ThreadStateKind};
use orbit_common::{ChatMessage, ChatMode, LlmUsage, ToolCall};
use orbit_llm::{events::StreamEvent, ToolCallAccumulator};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// ponytail: single active turn per thread actor; upgrade to queued turn pipeline when background subagent turns run concurrently.

/// Events broadcasted to UI and external observers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThreadEvent {
    StateChanged {
        thread_id: Uuid,
        from: ThreadStateKind,
        to: ThreadStateKind,
    },
    MessageAppended {
        thread_id: Uuid,
        message: ChatMessage,
    },
    TextDeltaReceived {
        thread_id: Uuid,
        delta: String,
    },
    ReasoningDeltaReceived {
        thread_id: Uuid,
        delta: String,
    },
    ApprovalRequested {
        thread_id: Uuid,
        call_id: String,
        tool_name: String,
        arguments: String,
    },
    ToolApprovalResolved {
        thread_id: Uuid,
        call_id: String,
        approved: bool,
    },
    ToolExecutionStarted {
        thread_id: Uuid,
        call_id: String,
        tool_name: String,
    },
    ToolExecutionFinished {
        thread_id: Uuid,
        call_id: String,
        is_error: bool,
        output: String,
    },
    CheckpointCreated {
        thread_id: Uuid,
        checkpoint_id: Uuid,
        description: Option<String>,
    },
    CheckpointRolledBack {
        thread_id: Uuid,
        target_checkpoint_id: Uuid,
        plan: RollbackPlan,
    },
    TurnCancelled {
        thread_id: Uuid,
    },
    TurnCompleted {
        thread_id: Uuid,
        finish_reason: Option<String>,
        usage: Option<LlmUsage>,
    },
}

#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub call: ToolCall,
}

/// Commands sent to the ThreadActor via MPSC channel.
pub enum ThreadCommand {
    SendMessage {
        content: String,
        mode: Option<ChatMode>,
        response: oneshot::Sender<Result<Uuid>>,
    },
    FeedStreamEvents {
        events: Vec<StreamEvent>,
        response: oneshot::Sender<Result<()>>,
    },
    RequestToolApproval {
        tool_calls: Vec<ToolCall>,
        response: oneshot::Sender<Result<()>>,
    },
    SubmitApproval {
        call_id: String,
        approved: bool,
        reason: Option<String>,
        response: oneshot::Sender<Result<()>>,
    },
    ExecuteToolResult {
        call_id: String,
        output: String,
        is_error: bool,
        continue_stream: bool,
        response: oneshot::Sender<Result<()>>,
    },
    CancelTurn {
        response: oneshot::Sender<Result<bool>>,
    },
    ResetToIdle {
        response: oneshot::Sender<Result<()>>,
    },
    CreateCheckpoint {
        description: Option<String>,
        files: HashMap<String, String>,
        response: oneshot::Sender<Result<Uuid>>,
    },
    UndoCheckpoint {
        response: oneshot::Sender<Result<RollbackPlan>>,
    },
    RedoCheckpoint {
        response: oneshot::Sender<Result<RollbackPlan>>,
    },
    RollbackToCheckpoint {
        checkpoint_id: Uuid,
        response: oneshot::Sender<Result<RollbackPlan>>,
    },
    GetState {
        response: oneshot::Sender<ThreadStateInfo>,
    },
    GetMessages {
        response: oneshot::Sender<Vec<ChatMessage>>,
    },
    GetCancellationToken {
        response: oneshot::Sender<Option<CancellationToken>>,
    },
    Shutdown {
        response: oneshot::Sender<()>,
    },
}

/// The Actor managing a Chat Thread's state, turns, tools, and CAS checkpoints.
pub struct ThreadActor {
    pub thread_id: Uuid,
    pub mode: ChatMode,
    pub state_kind: ThreadStateKind,
    pub messages: Vec<ChatMessage>,
    pub checkpoint_manager: CheckpointManager,

    // Active turn state
    pub turn_id: Option<Uuid>,
    pub turn_cancellation_token: Option<CancellationToken>,
    pub text_buffer: String,
    pub reasoning_buffer: String,
    pub tool_accumulator: ToolCallAccumulator,
    pub pending_approvals: Vec<PendingApproval>,
    pub active_tool_calls: Vec<ToolCall>,
    pub last_finish_reason: Option<String>,

    // Channels
    receiver: mpsc::Receiver<ThreadCommand>,
    event_sender: broadcast::Sender<ThreadEvent>,
}

impl ThreadActor {
    pub fn new(
        thread_id: Uuid,
        mode: ChatMode,
        receiver: mpsc::Receiver<ThreadCommand>,
        event_sender: broadcast::Sender<ThreadEvent>,
    ) -> Self {
        Self {
            thread_id,
            mode,
            state_kind: ThreadStateKind::Idle,
            messages: Vec::new(),
            checkpoint_manager: CheckpointManager::new(),
            turn_id: None,
            turn_cancellation_token: None,
            text_buffer: String::new(),
            reasoning_buffer: String::new(),
            tool_accumulator: ToolCallAccumulator::new(),
            pending_approvals: Vec::new(),
            active_tool_calls: Vec::new(),
            last_finish_reason: None,
            receiver,
            event_sender,
        }
    }

    /// Run the actor event loop until shutdown or all senders drop.
    pub async fn run(mut self) {
        while let Some(command) = self.receiver.recv().await {
            match command {
                ThreadCommand::SendMessage {
                    content,
                    mode,
                    response,
                } => {
                    let res = self.handle_send_message(content, mode);
                    let _ = response.send(res);
                }
                ThreadCommand::FeedStreamEvents { events, response } => {
                    let res = self.handle_feed_stream_events(events);
                    let _ = response.send(res);
                }
                ThreadCommand::RequestToolApproval {
                    tool_calls,
                    response,
                } => {
                    let res = self.handle_request_tool_approval(tool_calls);
                    let _ = response.send(res);
                }
                ThreadCommand::SubmitApproval {
                    call_id,
                    approved,
                    reason,
                    response,
                } => {
                    let res = self.handle_submit_approval(&call_id, approved, reason);
                    let _ = response.send(res);
                }
                ThreadCommand::ExecuteToolResult {
                    call_id,
                    output,
                    is_error,
                    continue_stream,
                    response,
                } => {
                    let res = self.handle_execute_tool_result(
                        &call_id,
                        output,
                        is_error,
                        continue_stream,
                    );
                    let _ = response.send(res);
                }
                ThreadCommand::CancelTurn { response } => {
                    let res = self.handle_cancel_turn();
                    let _ = response.send(Ok(res));
                }
                ThreadCommand::ResetToIdle { response } => {
                    let res = self.handle_reset_to_idle();
                    let _ = response.send(res);
                }
                ThreadCommand::CreateCheckpoint {
                    description,
                    files,
                    response,
                } => {
                    let res = self.handle_create_checkpoint(description, files);
                    let _ = response.send(res);
                }
                ThreadCommand::UndoCheckpoint { response } => {
                    let res = self.handle_undo_checkpoint();
                    let _ = response.send(res);
                }
                ThreadCommand::RedoCheckpoint { response } => {
                    let res = self.handle_redo_checkpoint();
                    let _ = response.send(res);
                }
                ThreadCommand::RollbackToCheckpoint {
                    checkpoint_id,
                    response,
                } => {
                    let res = self.handle_rollback_to_checkpoint(checkpoint_id);
                    let _ = response.send(res);
                }
                ThreadCommand::GetState { response } => {
                    let _ = response.send(self.current_state_info());
                }
                ThreadCommand::GetMessages { response } => {
                    let _ = response.send(self.messages.clone());
                }
                ThreadCommand::GetCancellationToken { response } => {
                    let _ = response.send(self.turn_cancellation_token.clone());
                }
                ThreadCommand::Shutdown { response } => {
                    let _ = response.send(());
                    break;
                }
            }
        }
    }

    fn transition_state(&mut self, target: ThreadStateKind) -> Result<()> {
        ThreadStateKind::validate_transition(self.state_kind, target)?;
        let from = self.state_kind;
        self.state_kind = target;
        let _ = self.event_sender.send(ThreadEvent::StateChanged {
            thread_id: self.thread_id,
            from,
            to: target,
        });
        Ok(())
    }

    fn handle_send_message(&mut self, content: String, mode: Option<ChatMode>) -> Result<Uuid> {
        // Must be in Idle or Completed to start a new turn
        match self.state_kind {
            ThreadStateKind::Idle | ThreadStateKind::Completed => {}
            other => {
                return Err(RuntimeError::InvalidStateTransition {
                    from: format!("{other:?}"),
                    to: format!("{:?}", ThreadStateKind::Streaming),
                    reason: "Cannot start a new turn while a previous turn is active".into(),
                });
            }
        }

        if let Some(m) = mode {
            self.mode = m;
        }

        let user_msg = ChatMessage::user(content);
        self.messages.push(user_msg.clone());
        let _ = self.event_sender.send(ThreadEvent::MessageAppended {
            thread_id: self.thread_id,
            message: user_msg,
        });

        // Initialize turn buffers and cancellation token
        let turn_id = Uuid::new_v4();
        let token = CancellationToken::new();
        self.turn_id = Some(turn_id);
        self.turn_cancellation_token = Some(token);
        self.text_buffer.clear();
        self.reasoning_buffer.clear();
        self.tool_accumulator = ToolCallAccumulator::new();
        self.pending_approvals.clear();
        self.active_tool_calls.clear();
        self.last_finish_reason = None;

        self.transition_state(ThreadStateKind::Streaming)?;
        Ok(turn_id)
    }

    fn handle_feed_stream_events(&mut self, events: Vec<StreamEvent>) -> Result<()> {
        if self.state_kind != ThreadStateKind::Streaming {
            return Err(RuntimeError::InvalidStateTransition {
                from: format!("{:?}", self.state_kind),
                to: format!("{:?}", ThreadStateKind::Streaming),
                reason: "FeedStreamEvents only allowed when in Streaming state".into(),
            });
        }

        let is_cancelled = self
            .turn_cancellation_token
            .as_ref()
            .map(|t| t.is_cancelled())
            .unwrap_or(false);

        if is_cancelled {
            let _ = self.handle_cancel_turn();
            return Err(RuntimeError::Cancelled);
        }

        for ev in events {
            if let Some(token) = &self.turn_cancellation_token {
                if token.is_cancelled() {
                    let _ = self.handle_cancel_turn();
                    return Err(RuntimeError::Cancelled);
                }
            }

            match ev {
                StreamEvent::TextDelta(t) => {
                    self.text_buffer.push_str(&t.text);
                    let _ = self.event_sender.send(ThreadEvent::TextDeltaReceived {
                        thread_id: self.thread_id,
                        delta: t.text,
                    });
                }
                StreamEvent::ReasoningDelta(r) => {
                    self.reasoning_buffer.push_str(&r.reasoning);
                    let _ = self.event_sender.send(ThreadEvent::ReasoningDeltaReceived {
                        thread_id: self.thread_id,
                        delta: r.reasoning,
                    });
                }
                StreamEvent::ToolCallDelta(d) => {
                    self.tool_accumulator.push(&d);
                }
                StreamEvent::UsageUpdate(_) => {
                    // Usage update is captured in Completed event or tracked
                }
                StreamEvent::Completed(c) => {
                    self.last_finish_reason = c.finish_reason.clone();
                    let accumulated_tools = self.tool_accumulator.tools();

                    if !accumulated_tools.is_empty() {
                        let tool_calls = self.tool_accumulator.clone().into_tool_calls();

                        // Emit assistant message with tool calls if we have text or tools
                        let mut assistant_msg = ChatMessage::assistant(&self.text_buffer);
                        if !self.reasoning_buffer.is_empty() {
                            assistant_msg.reasoning = Some(self.reasoning_buffer.clone());
                        }
                        assistant_msg.tool_calls = tool_calls.clone();
                        self.messages.push(assistant_msg.clone());
                        let _ = self.event_sender.send(ThreadEvent::MessageAppended {
                            thread_id: self.thread_id,
                            message: assistant_msg,
                        });

                        // Clear buffers for subsequent continuation turns
                        self.tool_accumulator = ToolCallAccumulator::new();
                        self.text_buffer.clear();
                        self.reasoning_buffer.clear();

                        // Automatically request approval for accumulated tool calls
                        self.handle_request_tool_approval(tool_calls)?;
                    } else {
                        // Regular completion without tool calls
                        let mut assistant_msg = ChatMessage::assistant(&self.text_buffer);
                        if !self.reasoning_buffer.is_empty() {
                            assistant_msg.reasoning = Some(self.reasoning_buffer.clone());
                        }
                        self.messages.push(assistant_msg.clone());
                        let _ = self.event_sender.send(ThreadEvent::MessageAppended {
                            thread_id: self.thread_id,
                            message: assistant_msg,
                        });

                        self.tool_accumulator = ToolCallAccumulator::new();
                        self.text_buffer.clear();
                        self.reasoning_buffer.clear();

                        self.transition_state(ThreadStateKind::Completed)?;
                        let _ = self.event_sender.send(ThreadEvent::TurnCompleted {
                            thread_id: self.thread_id,
                            finish_reason: c.finish_reason,
                            usage: c.usage,
                        });
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_request_tool_approval(&mut self, tool_calls: Vec<ToolCall>) -> Result<()> {
        if self.state_kind != ThreadStateKind::Streaming {
            return Err(RuntimeError::InvalidStateTransition {
                from: format!("{:?}", self.state_kind),
                to: format!("{:?}", ThreadStateKind::AwaitingApproval),
                reason: "Can only request tool approval from Streaming state".into(),
            });
        }

        self.pending_approvals.clear();
        for call in &tool_calls {
            self.pending_approvals
                .push(PendingApproval { call: call.clone() });
            let _ = self.event_sender.send(ThreadEvent::ApprovalRequested {
                thread_id: self.thread_id,
                call_id: call.id.clone(),
                tool_name: call.name.clone(),
                arguments: call.arguments.clone(),
            });
        }

        self.transition_state(ThreadStateKind::AwaitingApproval)?;
        Ok(())
    }

    fn handle_submit_approval(
        &mut self,
        call_id: &str,
        approved: bool,
        reason: Option<String>,
    ) -> Result<()> {
        if self.state_kind != ThreadStateKind::AwaitingApproval {
            return Err(RuntimeError::InvalidStateTransition {
                from: format!("{:?}", self.state_kind),
                to: format!("{:?}", ThreadStateKind::ToolExecuting),
                reason: "Approval submission only allowed when AwaitingApproval".into(),
            });
        }

        let idx = self
            .pending_approvals
            .iter()
            .position(|p| p.call.id == call_id)
            .ok_or_else(|| RuntimeError::ApprovalNotFound(call_id.to_string()))?;

        let pending = self.pending_approvals.remove(idx);
        let _ = self.event_sender.send(ThreadEvent::ToolApprovalResolved {
            thread_id: self.thread_id,
            call_id: call_id.to_string(),
            approved,
        });

        if approved {
            self.active_tool_calls.push(pending.call.clone());
            let _ = self.event_sender.send(ThreadEvent::ToolExecutionStarted {
                thread_id: self.thread_id,
                call_id: pending.call.id,
                tool_name: pending.call.name,
            });

            // Transition to ToolExecuting if not already there
            if self.state_kind != ThreadStateKind::ToolExecuting {
                self.transition_state(ThreadStateKind::ToolExecuting)?;
            }
        } else {
            // Tool call rejected by user: add error message and complete turn
            let rejection_message = reason.unwrap_or_else(|| "Tool call rejected by user".into());
            let tool_msg = ChatMessage::tool_result(call_id.to_string(), rejection_message, true);
            self.messages.push(tool_msg.clone());
            let _ = self.event_sender.send(ThreadEvent::MessageAppended {
                thread_id: self.thread_id,
                message: tool_msg,
            });

            self.last_finish_reason = Some("tool_rejected".into());
            self.transition_state(ThreadStateKind::Completed)?;
            let _ = self.event_sender.send(ThreadEvent::TurnCompleted {
                thread_id: self.thread_id,
                finish_reason: Some("tool_rejected".into()),
                usage: None,
            });
        }

        Ok(())
    }

    fn handle_execute_tool_result(
        &mut self,
        call_id: &str,
        output: String,
        is_error: bool,
        continue_stream: bool,
    ) -> Result<()> {
        if self.state_kind != ThreadStateKind::ToolExecuting {
            return Err(RuntimeError::InvalidStateTransition {
                from: format!("{:?}", self.state_kind),
                to: format!("{:?}", ThreadStateKind::ToolExecuting),
                reason: "ExecuteToolResult only allowed in ToolExecuting state".into(),
            });
        }

        self.active_tool_calls.retain(|t| t.id != call_id);

        let tool_msg = ChatMessage::tool_result(call_id.to_string(), output.clone(), is_error);
        self.messages.push(tool_msg.clone());
        let _ = self.event_sender.send(ThreadEvent::MessageAppended {
            thread_id: self.thread_id,
            message: tool_msg,
        });

        let _ = self.event_sender.send(ThreadEvent::ToolExecutionFinished {
            thread_id: self.thread_id,
            call_id: call_id.to_string(),
            is_error,
            output,
        });

        if continue_stream {
            self.transition_state(ThreadStateKind::Streaming)?;
        } else {
            self.last_finish_reason = Some("tool_completed".into());
            self.transition_state(ThreadStateKind::Completed)?;
            let _ = self.event_sender.send(ThreadEvent::TurnCompleted {
                thread_id: self.thread_id,
                finish_reason: Some("tool_completed".into()),
                usage: None,
            });
        }

        Ok(())
    }

    fn handle_cancel_turn(&mut self) -> bool {
        match self.state_kind {
            ThreadStateKind::Streaming
            | ThreadStateKind::AwaitingApproval
            | ThreadStateKind::ToolExecuting => {
                if let Some(token) = &self.turn_cancellation_token {
                    token.cancel();
                }

                self.pending_approvals.clear();
                self.active_tool_calls.clear();
                self.last_finish_reason = Some("cancelled".into());

                let _ = self.transition_state(ThreadStateKind::Completed);
                let _ = self.event_sender.send(ThreadEvent::TurnCancelled {
                    thread_id: self.thread_id,
                });
                true
            }
            ThreadStateKind::Idle | ThreadStateKind::Completed => false,
        }
    }

    fn handle_reset_to_idle(&mut self) -> Result<()> {
        self.transition_state(ThreadStateKind::Idle)?;
        self.turn_id = None;
        self.turn_cancellation_token = None;
        Ok(())
    }

    fn handle_create_checkpoint(
        &mut self,
        description: Option<String>,
        files: HashMap<String, String>,
    ) -> Result<Uuid> {
        let id = self.checkpoint_manager.commit(description.clone(), files)?;
        let _ = self.event_sender.send(ThreadEvent::CheckpointCreated {
            thread_id: self.thread_id,
            checkpoint_id: id,
            description,
        });
        Ok(id)
    }

    fn handle_undo_checkpoint(&mut self) -> Result<RollbackPlan> {
        let plan = self.checkpoint_manager.undo()?;
        let _ = self.event_sender.send(ThreadEvent::CheckpointRolledBack {
            thread_id: self.thread_id,
            target_checkpoint_id: plan.to_checkpoint_id,
            plan: plan.clone(),
        });
        Ok(plan)
    }

    fn handle_redo_checkpoint(&mut self) -> Result<RollbackPlan> {
        let plan = self.checkpoint_manager.redo()?;
        let _ = self.event_sender.send(ThreadEvent::CheckpointRolledBack {
            thread_id: self.thread_id,
            target_checkpoint_id: plan.to_checkpoint_id,
            plan: plan.clone(),
        });
        Ok(plan)
    }

    fn handle_rollback_to_checkpoint(&mut self, checkpoint_id: Uuid) -> Result<RollbackPlan> {
        let plan = self.checkpoint_manager.rollback_to(checkpoint_id)?;
        let _ = self.event_sender.send(ThreadEvent::CheckpointRolledBack {
            thread_id: self.thread_id,
            target_checkpoint_id: checkpoint_id,
            plan: plan.clone(),
        });
        Ok(plan)
    }

    fn current_state_info(&self) -> ThreadStateInfo {
        ThreadStateInfo {
            thread_id: self.thread_id,
            state_kind: self.state_kind,
            turn_id: self.turn_id,
            pending_approval_count: self.pending_approvals.len(),
            active_tools_count: self.active_tool_calls.len(),
            last_finish_reason: self.last_finish_reason.clone(),
        }
    }
}
