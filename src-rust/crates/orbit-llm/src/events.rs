use orbit_common::{LlmUsage, ToolCall};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ponytail: OpenAI & Anthropic delta unification without vendor-specific extensions; add custom vendor extensions when needed.

/// Stream event representing an incremental text token or slice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDelta {
    #[serde(alias = "delta", alias = "content")]
    pub text: String,
}

impl TextDelta {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Stream event representing an incremental reasoning / thinking chunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningDelta {
    #[serde(alias = "delta", alias = "thinking", alias = "reasoning_content")]
    pub reasoning: String,
}

impl ReasoningDelta {
    pub fn new(reasoning: impl Into<String>) -> Self {
        Self {
            reasoning: reasoning.into(),
        }
    }
}

/// Stream event representing an incremental tool call delta.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ToolCallDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

impl ToolCallDelta {
    pub fn new(
        index: Option<usize>,
        id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    ) -> Self {
        Self {
            index,
            id,
            name,
            arguments,
        }
    }
}

/// Stream event representing a token usage update.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UsageUpdate {
    pub usage: LlmUsage,
}

impl UsageUpdate {
    pub fn new(usage: LlmUsage) -> Self {
        Self { usage }
    }
}

/// Stream event representing completion of the generation.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Completed {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<LlmUsage>,
}

impl Completed {
    pub fn new(finish_reason: Option<String>, usage: Option<LlmUsage>) -> Self {
        Self {
            finish_reason,
            usage,
        }
    }
}

/// Unified streaming events emitted by LLM stream parsers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    TextDelta(TextDelta),
    ReasoningDelta(ReasoningDelta),
    ToolCallDelta(ToolCallDelta),
    UsageUpdate(UsageUpdate),
    Completed(Completed),
}

impl StreamEvent {
    pub fn text(text: impl Into<String>) -> Self {
        Self::TextDelta(TextDelta::new(text))
    }

    pub fn reasoning(reasoning: impl Into<String>) -> Self {
        Self::ReasoningDelta(ReasoningDelta::new(reasoning))
    }

    pub fn tool_call(
        index: Option<usize>,
        id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    ) -> Self {
        Self::ToolCallDelta(ToolCallDelta::new(index, id, name, arguments))
    }

    pub fn usage(usage: LlmUsage) -> Self {
        Self::UsageUpdate(UsageUpdate::new(usage))
    }

    pub fn completed(finish_reason: Option<String>, usage: Option<LlmUsage>) -> Self {
        Self::Completed(Completed::new(finish_reason, usage))
    }
}

impl From<TextDelta> for StreamEvent {
    fn from(d: TextDelta) -> Self {
        Self::TextDelta(d)
    }
}

impl From<ReasoningDelta> for StreamEvent {
    fn from(d: ReasoningDelta) -> Self {
        Self::ReasoningDelta(d)
    }
}

impl From<ToolCallDelta> for StreamEvent {
    fn from(d: ToolCallDelta) -> Self {
        Self::ToolCallDelta(d)
    }
}

impl From<UsageUpdate> for StreamEvent {
    fn from(d: UsageUpdate) -> Self {
        Self::UsageUpdate(d)
    }
}

impl From<Completed> for StreamEvent {
    fn from(d: Completed) -> Self {
        Self::Completed(d)
    }
}

/// An accumulated tool call from multiple stream chunks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccumulatedTool {
    pub index: usize,
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl AccumulatedTool {
    pub fn to_tool_call(&self) -> ToolCall {
        ToolCall {
            id: self.id.clone(),
            name: self.name.clone(),
            arguments: self.arguments.clone(),
        }
    }
}

/// Accumulator for reconstructing tool calls from streamed `ToolCallDelta`s.
#[derive(Debug, Default, Clone)]
pub struct ToolCallAccumulator {
    tools_by_index: BTreeMap<usize, AccumulatedTool>,
}

impl ToolCallAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, delta: &ToolCallDelta) {
        let index = delta.index.unwrap_or(0);
        let entry = self
            .tools_by_index
            .entry(index)
            .or_insert_with(|| AccumulatedTool {
                index,
                id: String::new(),
                name: String::new(),
                arguments: String::new(),
            });

        let entry = if let Some(new_id) = &delta.id {
            if !entry.id.is_empty() && &entry.id != new_id {
                let next_idx = self.tools_by_index.keys().max().map_or(0, |k| k + 1);
                self.tools_by_index
                    .entry(next_idx)
                    .or_insert_with(|| AccumulatedTool {
                        index: next_idx,
                        id: new_id.clone(),
                        name: String::new(),
                        arguments: String::new(),
                    })
            } else {
                entry
            }
        } else {
            entry
        };

        if let Some(id) = &delta.id {
            if entry.id.is_empty() {
                entry.id = id.clone();
            }
        }
        if let Some(name) = &delta.name {
            if entry.name.is_empty() {
                entry.name = name.clone();
            } else if entry.name == *name {
                // Provider repeats full tool name across delta chunks
            } else if name.starts_with(&entry.name) {
                // Cumulative name update
                entry.name = name.clone();
            } else {
                // Incremental name chunk delta
                entry.name.push_str(name);
            }
        }
        if let Some(args) = &delta.arguments {
            entry.arguments.push_str(args);
        }
    }

    pub fn tools(&self) -> Vec<AccumulatedTool> {
        self.tools_by_index.values().cloned().collect()
    }

    pub fn into_tool_calls(self) -> Vec<ToolCall> {
        self.tools_by_index
            .into_values()
            .map(|t| t.to_tool_call())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serde_roundtrip() {
        let events = vec![
            StreamEvent::text("hello world"),
            StreamEvent::reasoning("thinking step 1"),
            StreamEvent::tool_call(
                Some(0),
                Some("call_123".into()),
                Some("read_file".into()),
                Some("{\"path\":\"test.txt\"}".into()),
            ),
            StreamEvent::usage(LlmUsage {
                prompt_tokens: Some(15),
                completion_tokens: Some(30),
                total_tokens: Some(45),
                ..Default::default()
            }),
            StreamEvent::completed(Some("stop".into()), None),
        ];

        for ev in &events {
            let json = serde_json::to_string(ev).expect("serialization failed");
            let parsed: StreamEvent = serde_json::from_str(&json).expect("deserialization failed");
            assert_eq!(ev, &parsed);
        }
    }

    #[test]
    fn test_tool_call_accumulator() {
        let mut acc = ToolCallAccumulator::new();
        acc.push(&ToolCallDelta {
            index: Some(0),
            id: Some("call_abc".into()),
            name: Some("search_".into()),
            arguments: None,
        });
        acc.push(&ToolCallDelta {
            index: Some(0),
            id: None,
            name: Some("files".into()),
            arguments: Some("{\"pattern\":".into()),
        });
        acc.push(&ToolCallDelta {
            index: Some(0),
            id: None,
            name: None,
            arguments: Some("\"*.rs\"}".into()),
        });

        let tools = acc.into_tool_calls();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "call_abc");
        assert_eq!(tools[0].name, "search_files");
        assert_eq!(tools[0].arguments, "{\"pattern\":\"*.rs\"}");
    }
}
