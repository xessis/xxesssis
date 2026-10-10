use crate::events::{
    Completed, ReasoningDelta, StreamEvent, TextDelta, ToolCallDelta, UsageUpdate,
};
use orbit_common::LlmUsage;
use serde::{Deserialize, Serialize};

// ponytail: synchronous line-buffered SSE parser; upgrade to tokio / async-stream when async transport is wired.

/// A parsed Server-Sent Event (SSE).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SseEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u64>,
}

impl SseEvent {
    /// True if the payload matches standard stream termination sentinel `[DONE]`.
    pub fn is_done(&self) -> bool {
        self.data.trim().eq_ignore_ascii_case("[done]")
    }

    /// Try deserializing the data payload as JSON.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.data)
    }

    /// Convenience to parse standard OpenAI-compatible streaming chunks into `StreamEvent`s.
    pub fn to_stream_events(&self) -> Result<Vec<StreamEvent>, serde_json::Error> {
        if self.is_done() {
            return Ok(vec![StreamEvent::Completed(Completed::new(
                Some("stop".into()),
                None,
            ))]);
        }

        if self.data.trim().is_empty() {
            return Ok(Vec::new());
        }

        let chunk: OpenAiChatChunk = self.json()?;
        let mut events = Vec::new();

        if let Some(usage) = chunk.usage {
            events.push(StreamEvent::UsageUpdate(UsageUpdate::new(usage)));
        }

        for choice in chunk.choices {
            if let Some(delta) = choice.delta {
                if let Some(text) = delta.content {
                    if !text.is_empty() {
                        events.push(StreamEvent::TextDelta(TextDelta::new(text)));
                    }
                }
                let reasoning = delta.reasoning_content.or(delta.reasoning);
                if let Some(r) = reasoning {
                    if !r.is_empty() {
                        events.push(StreamEvent::ReasoningDelta(ReasoningDelta::new(r)));
                    }
                }
                if let Some(tool_calls) = delta.tool_calls {
                    for tc in tool_calls {
                        let (name, args) = if let Some(func) = tc.function {
                            (func.name, func.arguments)
                        } else {
                            (None, None)
                        };
                        events.push(StreamEvent::ToolCallDelta(ToolCallDelta::new(
                            tc.index, tc.id, name, args,
                        )));
                    }
                }
            }

            if let Some(finish_reason) = choice.finish_reason {
                events.push(StreamEvent::Completed(Completed::new(
                    Some(finish_reason),
                    None,
                )));
            }
        }

        Ok(events)
    }
}

/// Minimal internal representation for OpenAI streaming chunks.
#[derive(Debug, Deserialize)]
struct OpenAiChatChunk {
    #[serde(default)]
    choices: Vec<OpenAiChoiceDelta>,
    #[serde(default)]
    usage: Option<LlmUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoiceDelta {
    #[serde(default)]
    delta: Option<OpenAiDeltaPayload>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDeltaPayload {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCallChunk>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCallChunk {
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiFunctionChunk>,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunctionChunk {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

pub const DEFAULT_MAX_BUFFER_SIZE: usize = 16 * 1024 * 1024; // 16 MB
pub const DEFAULT_MAX_DATA_LINES: usize = 65_536; // 64K lines

/// Incremental SSE stream parser that buffers incoming string chunks and yields complete events.
#[derive(Debug)]
pub struct SseParser {
    buffer: String,
    raw_buffer: Vec<u8>,
    current_event: Option<String>,
    current_data_lines: Vec<String>,
    current_id: Option<String>,
    current_retry: Option<u64>,
    max_buffer_size: usize,
    max_data_lines: usize,
}

impl Default for SseParser {
    fn default() -> Self {
        Self {
            buffer: String::new(),
            raw_buffer: Vec::new(),
            current_event: None,
            current_data_lines: Vec::new(),
            current_id: None,
            current_retry: None,
            max_buffer_size: DEFAULT_MAX_BUFFER_SIZE,
            max_data_lines: DEFAULT_MAX_DATA_LINES,
        }
    }
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limits(max_buffer_size: usize, max_data_lines: usize) -> Self {
        Self {
            max_buffer_size,
            max_data_lines,
            ..Default::default()
        }
    }

    /// Push an arbitrary string slice into the parser and return any complete events.
    pub fn push(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.buffer.push_str(chunk);
        if self.buffer.len() > self.max_buffer_size {
            self.buffer.clear();
            return Vec::new();
        }

        let mut events = Vec::new();
        let mut start = 0;

        while let Some(rel_pos) = self.buffer[start..].find('\n') {
            let pos = start + rel_pos;
            let line = &self.buffer[start..pos];
            let line = line.strip_suffix('\r').unwrap_or(line);
            let line = line.to_string();
            start = pos + 1;

            if let Some(event) = self.process_line(&line) {
                events.push(event);
            }
        }

        if start > 0 {
            self.buffer.drain(..start);
        }

        events
    }

    /// Push raw byte chunk into the parser, handling split multi-byte UTF-8 sequences safely.
    pub fn push_bytes(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.raw_buffer.extend_from_slice(chunk);
        if self.raw_buffer.len() > self.max_buffer_size {
            self.raw_buffer.clear();
            return Vec::new();
        }

        let mut events = Vec::new();
        let mut start = 0;

        while let Some(rel_pos) = self.raw_buffer[start..].iter().position(|&b| b == b'\n') {
            let pos = start + rel_pos;
            let mut line_bytes = &self.raw_buffer[start..pos];
            if line_bytes.ends_with(b"\r") {
                line_bytes = &line_bytes[..line_bytes.len() - 1];
            }
            let line = String::from_utf8_lossy(line_bytes).into_owned();
            start = pos + 1;

            if let Some(event) = self.process_line(&line) {
                events.push(event);
            }
        }

        if start > 0 {
            self.raw_buffer.drain(..start);
        }

        events
    }

    /// Flush any remaining buffered event when the stream ends.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();
        if !self.raw_buffer.is_empty() {
            let mut line_bytes = self.raw_buffer.as_slice();
            if line_bytes.ends_with(b"\r") {
                line_bytes = &line_bytes[..line_bytes.len() - 1];
            }
            let line = String::from_utf8_lossy(line_bytes).into_owned();
            self.raw_buffer.clear();
            if let Some(event) = self.process_line(&line) {
                events.push(event);
            }
        }
        if !self.buffer.is_empty() {
            let line = self.buffer.strip_suffix('\r').unwrap_or(&self.buffer);
            let line = line.to_string();
            self.buffer.clear();
            if let Some(event) = self.process_line(&line) {
                events.push(event);
            }
        }
        if let Some(event) = self.dispatch_current() {
            events.push(event);
        }
        events
    }

    fn process_line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.dispatch_current();
        }

        // Lines starting with colon are comments (keepalive, etc.)
        if line.starts_with(':') {
            return None;
        }

        let (field, value) = if let Some((f, v)) = line.split_once(':') {
            // Strip single leading space per SSE specification
            let val = if let Some(stripped) = v.strip_prefix(' ') {
                stripped
            } else {
                v
            };
            (f, val)
        } else {
            (line, "")
        };

        match field {
            "event" => {
                self.current_event = Some(value.to_string());
            }
            "data" => {
                self.current_data_lines.push(value.to_string());
                if self.current_data_lines.len() >= self.max_data_lines {
                    return self.dispatch_current();
                }
            }
            "id" => {
                self.current_id = Some(value.to_string());
            }
            "retry" => {
                if let Ok(retry) = value.trim().parse::<u64>() {
                    self.current_retry = Some(retry);
                }
            }
            _ => {}
        }

        None
    }

    fn dispatch_current(&mut self) -> Option<SseEvent> {
        if self.current_data_lines.is_empty()
            && self.current_event.is_none()
            && self.current_id.is_none()
            && self.current_retry.is_none()
        {
            return None;
        }

        let data = self.current_data_lines.join("\n");
        self.current_data_lines.clear();

        Some(SseEvent {
            event: self.current_event.take(),
            data,
            id: self.current_id.take(),
            retry: self.current_retry.take(),
        })
    }
}

/// Convenience function to parse a complete SSE stream string into events.
pub fn parse_sse(input: &str) -> Vec<SseEvent> {
    let mut parser = SseParser::new();
    let mut events = parser.push(input);
    events.extend(parser.finish());
    events
}

/// Convenience function to parse a complete SSE byte stream into events.
pub fn parse_sse_bytes(input: &[u8]) -> Vec<SseEvent> {
    let mut parser = SseParser::new();
    let mut events = parser.push_bytes(input);
    events.extend(parser.finish());
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_sse() {
        let input = "data: {\"foo\":\"bar\"}\n\n";
        let events = parse_sse(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "{\"foo\":\"bar\"}");
        assert_eq!(events[0].event, None);
    }

    #[test]
    fn test_parse_multiline_data_and_event() {
        let input = "event: custom\nid: 42\ndata: first line\ndata: second line\n\n";
        let events = parse_sse(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, Some("custom".into()));
        assert_eq!(events[0].id, Some("42".into()));
        assert_eq!(events[0].data, "first line\nsecond line");
    }

    #[test]
    fn test_parse_comments_and_done() {
        let input = ": ping\n\ndata: [DONE]\n\n";
        let events = parse_sse(input);
        assert_eq!(events.len(), 1);
        assert!(events[0].is_done());
    }

    #[test]
    fn test_chunked_streaming() {
        let mut parser = SseParser::new();
        let ev1 = parser.push("data: hel");
        assert!(ev1.is_empty());
        let ev2 = parser.push("lo\n\ndata: world\n\n");
        assert_eq!(ev2.len(), 2);
        assert_eq!(ev2[0].data, "hello");
        assert_eq!(ev2[1].data, "world");
    }

    #[test]
    fn test_crlf_newlines() {
        let input = "data: test1\r\n\r\ndata: test2\r\n\r\n";
        let events = parse_sse(input);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "test1");
        assert_eq!(events[1].data, "test2");
    }

    #[test]
    fn test_to_stream_events_openai_chunk() {
        let chunk_json = r#"{
            "choices": [{
                "index": 0,
                "delta": {
                    "content": "Hello ",
                    "reasoning_content": "Thinking hard",
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "function": { "name": "test_fn", "arguments": "{}" }
                    }]
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let event = SseEvent {
            event: None,
            data: chunk_json.into(),
            id: None,
            retry: None,
        };

        let stream_events = event.to_stream_events().expect("should parse");
        assert_eq!(stream_events.len(), 5);
        assert!(matches!(&stream_events[0], StreamEvent::UsageUpdate(_)));
        assert!(matches!(&stream_events[1], StreamEvent::TextDelta(t) if t.text == "Hello "));
        assert!(
            matches!(&stream_events[2], StreamEvent::ReasoningDelta(r) if r.reasoning == "Thinking hard")
        );
        assert!(
            matches!(&stream_events[3], StreamEvent::ToolCallDelta(tc) if tc.id.as_deref() == Some("call_1"))
        );
        assert!(
            matches!(&stream_events[4], StreamEvent::Completed(c) if c.finish_reason.as_deref() == Some("stop"))
        );
    }
}
