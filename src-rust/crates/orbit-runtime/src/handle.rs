use crate::actor::{ThreadActor, ThreadCommand, ThreadEvent};
use crate::checkpoint::RollbackPlan;
use crate::error::{Result, RuntimeError};
use crate::state::ThreadStateInfo;
use orbit_common::{ChatMessage, ChatMode, ToolCall};
use orbit_llm::events::StreamEvent;
use std::collections::HashMap;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// ponytail: in-memory actor thread handle using mpsc/broadcast channels; upgrade to distributed actor router if multi-process nodes communicate.

/// Handle to communicate with a ThreadActor task.
#[derive(Clone, Debug)]
pub struct ThreadHandle {
    thread_id: Uuid,
    sender: mpsc::Sender<ThreadCommand>,
    event_sender: broadcast::Sender<ThreadEvent>,
}

impl ThreadHandle {
    /// Spawn a new thread actor with a generated UUID.
    pub fn spawn(mode: ChatMode) -> (Self, JoinHandle<()>) {
        Self::spawn_with_id(Uuid::new_v4(), mode)
    }

    /// Spawn a new thread actor with a specific UUID.
    pub fn spawn_with_id(thread_id: Uuid, mode: ChatMode) -> (Self, JoinHandle<()>) {
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (evt_tx, _) = broadcast::channel(256);

        let actor = ThreadActor::new(thread_id, mode, cmd_rx, evt_tx.clone());
        let join_handle = tokio::spawn(actor.run());

        let handle = Self {
            thread_id,
            sender: cmd_tx,
            event_sender: evt_tx,
        };

        (handle, join_handle)
    }

    pub fn thread_id(&self) -> Uuid {
        self.thread_id
    }

    /// Subscribe to the actor's broadcast event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<ThreadEvent> {
        self.event_sender.subscribe()
    }

    /// Send a user message and initiate a streaming turn.
    pub async fn send_message(&self, content: impl Into<String>) -> Result<Uuid> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::SendMessage {
                content: content.into(),
                mode: None,
                response: resp_tx,
            })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Send a user message with a specific ChatMode.
    pub async fn send_message_with_mode(
        &self,
        content: impl Into<String>,
        mode: ChatMode,
    ) -> Result<Uuid> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::SendMessage {
                content: content.into(),
                mode: Some(mode),
                response: resp_tx,
            })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Feed LLM streaming events (TextDelta, ReasoningDelta, ToolCallDelta, Completed).
    pub async fn feed_stream_events(&self, events: Vec<StreamEvent>) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::FeedStreamEvents {
                events,
                response: resp_tx,
            })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Request approval for tool calls.
    pub async fn request_tool_approval(&self, tool_calls: Vec<ToolCall>) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::RequestToolApproval {
                tool_calls,
                response: resp_tx,
            })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Submit human approval or rejection for a pending tool call.
    pub async fn submit_approval(
        &self,
        call_id: impl Into<String>,
        approved: bool,
        reason: Option<String>,
    ) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::SubmitApproval {
                call_id: call_id.into(),
                approved,
                reason,
                response: resp_tx,
            })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Record a tool execution output and optionally continue the LLM stream.
    pub async fn execute_tool_result(
        &self,
        call_id: impl Into<String>,
        output: impl Into<String>,
        is_error: bool,
        continue_stream: bool,
    ) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::ExecuteToolResult {
                call_id: call_id.into(),
                output: output.into(),
                is_error,
                continue_stream,
                response: resp_tx,
            })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Cancel the active turn gracefully and instantly.
    pub async fn cancel(&self) -> Result<bool> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::CancelTurn { response: resp_tx })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Reset state machine back to Idle after a Completed turn.
    pub async fn reset_to_idle(&self) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::ResetToIdle { response: resp_tx })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Create a BLAKE3 CAS checkpoint of files.
    pub async fn create_checkpoint(
        &self,
        description: Option<String>,
        files: HashMap<String, String>,
    ) -> Result<Uuid> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::CreateCheckpoint {
                description,
                files,
                response: resp_tx,
            })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Undo to the previous checkpoint.
    pub async fn undo(&self) -> Result<RollbackPlan> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::UndoCheckpoint { response: resp_tx })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Redo to the next checkpoint.
    pub async fn redo(&self) -> Result<RollbackPlan> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::RedoCheckpoint { response: resp_tx })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Rollback to a specific checkpoint by ID.
    pub async fn rollback_to(&self, checkpoint_id: Uuid) -> Result<RollbackPlan> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::RollbackToCheckpoint {
                checkpoint_id,
                response: resp_tx,
            })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)?
    }

    /// Get current state diagnostic info.
    pub async fn get_state(&self) -> Result<ThreadStateInfo> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::GetState { response: resp_tx })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)
    }

    /// Get conversation history.
    pub async fn get_messages(&self) -> Result<Vec<ChatMessage>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::GetMessages { response: resp_tx })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)
    }

    /// Get the active turn's CancellationToken (if active).
    pub async fn get_cancellation_token(&self) -> Result<Option<CancellationToken>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::GetCancellationToken { response: resp_tx })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        resp_rx.await.map_err(|_| RuntimeError::ActorDisconnected)
    }

    /// Terminate the actor task cleanly.
    pub async fn shutdown(&self) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender
            .send(ThreadCommand::Shutdown { response: resp_tx })
            .await
            .map_err(|_| RuntimeError::ActorDisconnected)?;

        let _ = resp_rx.await;
        Ok(())
    }
}
