use orbit_common::{ChatMessage, ChatRole};
use serde_json::{json, Value};

pub struct AnthropicWireFormatter;

impl AnthropicWireFormatter {
    /// Formats messages into Anthropic messages API schema, injecting prompt cache markers
    pub fn format(
        system_prompt: &str,
        messages: &[ChatMessage],
        max_tokens: usize,
        model_id: &str,
    ) -> Value {
        let mut anthropic_messages = Vec::new();

        for (i, msg) in messages.iter().enumerate() {
            let is_penultimate_or_last = i + 2 >= messages.len();

            let role = match msg.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::Tool => "user",
                ChatRole::System => "user",
            };

            let mut content_blocks = Vec::new();

            if let Some(r) = &msg.reasoning {
                content_blocks.push(json!({
                    "type": "thinking",
                    "thinking": r
                }));
            }

            if !msg.content.is_empty() {
                let mut block = json!({
                    "type": "text",
                    "text": msg.content
                });

                if is_penultimate_or_last && msg.role == ChatRole::User {
                    block["cache_control"] = json!({ "type": "ephemeral" });
                }

                content_blocks.push(block);
            }

            for tool in &msg.tool_calls {
                let args_parsed: Value = serde_json::from_str(&tool.arguments)
                    .unwrap_or_else(|_| json!({ "raw": tool.arguments }));
                content_blocks.push(json!({
                    "type": "tool_use",
                    "id": tool.id,
                    "name": tool.name,
                    "input": args_parsed
                }));
            }

            if let Some(tr) = &msg.tool_result {
                content_blocks.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tr.call_id,
                    "content": msg.content,
                    "is_error": tr.is_error
                }));
            }

            anthropic_messages.push(json!({
                "role": role,
                "content": content_blocks
            }));
        }

        json!({
            "model": model_id,
            "max_tokens": max_tokens,
            "system": [{
                "type": "text",
                "text": system_prompt,
                "cache_control": { "type": "ephemeral" }
            }],
            "messages": anthropic_messages
        })
    }
}

pub struct OpenAiWireFormatter;

impl OpenAiWireFormatter {
    /// Formats messages into standard OpenAI / OpenAI-compatible chat completion payload
    pub fn format(
        system_prompt: &str,
        messages: &[ChatMessage],
        max_tokens: usize,
        model_id: &str,
    ) -> Value {
        let mut openai_messages = Vec::new();

        if !system_prompt.is_empty() {
            openai_messages.push(json!({
                "role": "system",
                "content": system_prompt
            }));
        }

        for msg in messages {
            match msg.role {
                ChatRole::User => {
                    openai_messages.push(json!({
                        "role": "user",
                        "content": msg.content
                    }));
                }
                ChatRole::Assistant => {
                    let mut item = json!({
                        "role": "assistant",
                        "content": msg.content
                    });
                    if !msg.tool_calls.is_empty() {
                        let calls: Vec<_> = msg
                            .tool_calls
                            .iter()
                            .map(|tc| {
                                json!({
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.name,
                                        "arguments": tc.arguments
                                    }
                                })
                            })
                            .collect();
                        item["tool_calls"] = json!(calls);
                    }
                    openai_messages.push(item);
                }
                ChatRole::Tool => {
                    if let Some(tr) = &msg.tool_result {
                        openai_messages.push(json!({
                            "role": "tool",
                            "tool_call_id": tr.call_id,
                            "content": msg.content
                        }));
                    }
                }
                ChatRole::System => {
                    openai_messages.push(json!({
                        "role": "system",
                        "content": msg.content
                    }));
                }
            }
        }

        json!({
            "model": model_id,
            "max_tokens": max_tokens,
            "stream": true,
            "stream_options": { "include_usage": true },
            "messages": openai_messages
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_common::ToolCall;

    #[test]
    fn test_anthropic_wire_format() {
        let mut msg = ChatMessage::user("Hello");
        msg.tool_calls.push(ToolCall {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: r#"{"path":"test.txt"}"#.to_string(),
        });

        let payload =
            AnthropicWireFormatter::format("You are Orbit", &[msg], 4096, "claude-3-7-sonnet");
        assert_eq!(payload["model"], "claude-3-7-sonnet");
        assert_eq!(payload["system"][0]["cache_control"]["type"], "ephemeral");

        let messages = payload["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["content"][1]["type"], "tool_use");
    }

    #[test]
    fn test_openai_wire_format() {
        let msg1 = ChatMessage::user("List files");
        let payload = OpenAiWireFormatter::format("System prompt", &[msg1], 2048, "gpt-4o");
        assert_eq!(payload["stream"], true);
        assert_eq!(payload["stream_options"]["include_usage"], true);
        assert_eq!(payload["messages"].as_array().unwrap().len(), 2);
    }
}
