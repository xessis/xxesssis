use crate::modes::mode_system_reminder;
use orbit_common::{ChatMessage, ChatMode, ChatRole};

/// Formats a harness timestamp for the fresh user turn.
pub fn format_harness_timestamp() -> String {
    // ponytail: simple UTC/local ISO-like timestamp; upgrade to chrono format if exact localized timezone strings are needed.
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Calculate approximate date components
    let days = secs / 86400;
    let day_secs = secs % 86400;
    let hours = day_secs / 3600;
    let mins = (day_secs % 3600) / 60;

    // Days since Jan 1, 1970
    format!("Epoch Day {days}, {hours:02}:{mins:02} UTC")
}

/// Injects Cursor/Orbit harness context into the chat messages at LLM prepare time.
/// - Wraps the latest user turn in `<user_query>...</user_query>`.
/// - Prepends `<system_reminder>` if the active mode defines a per-turn reminder.
/// - Injects `<timestamp>` only on a fresh user turn (when user turn is the last message),
///   preserving prompt cache stability across mid-agent re-prepares.
pub fn augment_chat_messages_with_harness_context(
    messages: &[ChatMessage],
    chat_mode: ChatMode,
) -> Vec<ChatMessage> {
    let mut out = messages.to_vec();

    let mut last_user_idx = None;
    for (i, msg) in out.iter().enumerate().rev() {
        if msg.role == ChatRole::User {
            last_user_idx = Some(i);
            break;
        }
    }

    let Some(idx) = last_user_idx else {
        return out;
    };

    let content = &out[idx].content;
    if content.contains("<user_query>") {
        return out;
    }

    let reminder_opt = mode_system_reminder(chat_mode);
    let is_fresh_user_turn = idx == out.len() - 1;
    let timestamp_part = if is_fresh_user_turn {
        format!("<timestamp>{}</timestamp>\n", format_harness_timestamp())
    } else {
        String::new()
    };

    let wrapped_content = match reminder_opt {
        Some(reminder) => format!(
            "<system_reminder>\n{reminder}\n</system_reminder>\n\n{timestamp_part}<user_query>\n{content}\n</user_query>"
        ),
        None => format!("{timestamp_part}<user_query>\n{content}\n</user_query>"),
    };

    out[idx].content = wrapped_content;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_augment_plan_mode_injects_reminder_and_user_query() {
        let msgs = vec![
            ChatMessage::system("You are Orbit assistant."),
            ChatMessage::user("Refactor database schema."),
        ];

        let augmented = augment_chat_messages_with_harness_context(&msgs, ChatMode::Plan);
        let user_content = &augmented[1].content;

        assert!(user_content.contains("<system_reminder>"));
        assert!(user_content.contains("Plan mode is active"));
        assert!(user_content.contains("<timestamp>"));
        assert!(user_content.contains("<user_query>\nRefactor database schema.\n</user_query>"));
    }

    #[test]
    fn test_augment_agent_mode_has_no_reminder() {
        let msgs = vec![ChatMessage::user("Add a new route.")];

        let augmented = augment_chat_messages_with_harness_context(&msgs, ChatMode::Agent);
        let user_content = &augmented[0].content;

        assert!(!user_content.contains("<system_reminder>"));
        assert!(user_content.contains("<timestamp>"));
        assert!(user_content.contains("<user_query>\nAdd a new route.\n</user_query>"));
    }

    #[test]
    fn test_idempotent_if_already_augmented() {
        let msgs = vec![ChatMessage::user(
            "<user_query>\nAlready wrapped query\n</user_query>",
        )];

        let augmented = augment_chat_messages_with_harness_context(&msgs, ChatMode::Plan);
        assert_eq!(augmented[0].content, msgs[0].content);
    }
}
