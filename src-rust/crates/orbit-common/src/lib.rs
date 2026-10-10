pub mod chat;
pub mod checkpoint;
pub mod error;

pub use chat::{ChatMessage, ChatMode, ChatRole, LlmUsage, ToolCall, ToolResult};
pub use checkpoint::{CheckpointNode, FileSnapshot};
pub use error::{OrbitError, Result};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_creation() {
        let msg = ChatMessage::user("hello world");
        assert_eq!(msg.role, ChatRole::User);
        assert_eq!(msg.content, "hello world");

        let serialized = serde_json::to_string(&msg).expect("serialization failed");
        let deserialized: ChatMessage =
            serde_json::from_str(&serialized).expect("deserialization failed");
        assert_eq!(deserialized.role, ChatRole::User);
        assert_eq!(deserialized.content, "hello world");
    }

    #[test]
    fn test_file_snapshot_hash() {
        let snap1 = FileSnapshot::new("test.txt", "content A".to_string());
        let snap2 = FileSnapshot::new("test.txt", "content A".to_string());
        let snap3 = FileSnapshot::new("test.txt", "content B".to_string());
        assert_eq!(snap1.hash, snap2.hash);
        assert_ne!(snap1.hash, snap3.hash);
    }
}
