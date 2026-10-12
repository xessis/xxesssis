use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FimFormat {
    DeepSeek,
    StarCoder,
    CodeLlama,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FimPrompt {
    pub prompt: String,
    pub stop_tokens: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextInterval {
    pub uri: String,
    pub start_line: usize,
    pub end_line: usize,
}

impl ContextInterval {
    pub fn new(uri: impl Into<String>, start_line: usize, end_line: usize) -> Self {
        Self {
            uri: uri.into(),
            start_line,
            end_line,
        }
    }

    pub fn overlaps_or_adjacent(&self, other: &Self) -> bool {
        if self.uri != other.uri {
            return false;
        }
        self.start_line <= other.end_line + 1 && other.start_line <= self.end_line + 1
    }

    pub fn merge(&self, other: &Self) -> Option<Self> {
        if !self.overlaps_or_adjacent(other) {
            return None;
        }
        Some(Self {
            uri: self.uri.clone(),
            start_line: self.start_line.min(other.start_line),
            end_line: self.end_line.max(other.end_line),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnippet {
    pub uri: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub score: f32,
}

#[derive(Debug, Default)]
pub struct ContextGatherer {
    intervals: Vec<ContextInterval>,
}

impl ContextGatherer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_interval(&mut self, interval: ContextInterval) {
        self.intervals.push(interval);
    }

    pub fn merged_intervals(&self) -> Vec<ContextInterval> {
        if self.intervals.is_empty() {
            return Vec::new();
        }

        let mut sorted = self.intervals.clone();
        sorted.sort_by(|a, b| a.uri.cmp(&b.uri).then(a.start_line.cmp(&b.start_line)));

        let mut merged: Vec<ContextInterval> = Vec::new();
        for item in sorted {
            if let Some(last) = merged.last_mut() {
                if let Some(combined) = last.merge(&item) {
                    *last = combined;
                    continue;
                }
            }
            merged.push(item);
        }
        merged
    }

    pub fn extract_snippets(
        &self,
        files: &HashMap<String, String>,
        max_total_chars: usize,
    ) -> Vec<ContextSnippet> {
        let mut results = Vec::new();
        let mut total_chars = 0;

        for interval in self.merged_intervals() {
            if let Some(content) = files.get(&interval.uri) {
                let lines: Vec<&str> = content.lines().collect();
                if interval.start_line == 0 || interval.start_line > lines.len() {
                    continue;
                }
                let end = interval.end_line.min(lines.len());
                let snippet_lines = &lines[interval.start_line - 1..end];
                let snippet_text = snippet_lines.join("\n");

                if total_chars + snippet_text.len() > max_total_chars {
                    break;
                }
                total_chars += snippet_text.len();

                results.push(ContextSnippet {
                    uri: interval.uri.clone(),
                    start_line: interval.start_line,
                    end_line: end,
                    content: snippet_text,
                    score: 1.0,
                });
            }
        }
        results
    }
}

#[derive(Debug, Clone)]
pub struct SpeculativeEntry {
    pub prefix: String,
    pub completion: String,
    pub created_at_turn: u64,
}

pub struct SpeculativeCache {
    entries: HashMap<String, SpeculativeEntry>,
    capacity: usize,
    current_turn: u64,
}

impl SpeculativeCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            capacity: capacity.max(1),
            current_turn: 0,
        }
    }

    pub fn put(&mut self, key: impl Into<String>, prefix: impl Into<String>, completion: impl Into<String>) {
        if self.entries.len() >= self.capacity {
            if let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.created_at_turn)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest_key);
            }
        }

        self.current_turn += 1;
        self.entries.insert(
            key.into(),
            SpeculativeEntry {
                prefix: prefix.into(),
                completion: completion.into(),
                created_at_turn: self.current_turn,
            },
        );
    }

    pub fn match_speculative(&self, key: &str, current_prefix: &str) -> Option<String> {
        let entry = self.entries.get(key)?;
        if current_prefix.starts_with(&entry.prefix) {
            let typed_extra = &current_prefix[entry.prefix.len()..];
            if entry.completion.starts_with(typed_extra) {
                return Some(entry.completion[typed_extra.len()..].to_string());
            }
        }
        None
    }
}

pub struct AutocompleteEngine {
    cache: HashMap<String, String>,
    speculative: SpeculativeCache,
    gatherer: ContextGatherer,
}

impl Default for AutocompleteEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AutocompleteEngine {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            speculative: SpeculativeCache::new(128),
            gatherer: ContextGatherer::new(),
        }
    }

    /// Prepares FIM prompt from document prefix and suffix around cursor
    pub fn build_fim_prompt(&self, format: FimFormat, prefix: &str, suffix: &str) -> FimPrompt {
        let (prompt, stop_tokens) = match format {
            FimFormat::DeepSeek => (
                format!(
                    "<｜fim begin｜>{}<｜fim hole｜>{}<｜fim end｜>",
                    prefix, suffix
                ),
                vec![
                    "<｜fim begin｜>".to_string(),
                    "<｜fim hole｜>".to_string(),
                    "<｜fim end｜>".to_string(),
                    "<|endoftext|>".to_string(),
                    "\n\n".to_string(),
                ],
            ),
            FimFormat::StarCoder => (
                format!("<fim_prefix>{}<fim_suffix>{}<fim_middle>", prefix, suffix),
                vec![
                    "<fim_prefix>".to_string(),
                    "<fim_suffix>".to_string(),
                    "<fim_middle>".to_string(),
                    "<|endoftext|>".to_string(),
                ],
            ),
            FimFormat::CodeLlama => (
                format!("<PRE> {} <SUF>{} <MID>", prefix, suffix),
                vec!["<EOT>".to_string()],
            ),
        };

        FimPrompt {
            prompt,
            stop_tokens,
        }
    }

    /// Stores completion in memory cache
    pub fn cache_put(&mut self, key: impl Into<String>, completion: impl Into<String>) {
        self.cache.insert(key.into(), completion.into());
    }

    /// Retrieves cached completion
    pub fn cache_get(&self, key: &str) -> Option<&str> {
        self.cache.get(key).map(|s| s.as_str())
    }

    pub fn speculative_mut(&mut self) -> &mut SpeculativeCache {
        &mut self.speculative
    }

    pub fn gatherer_mut(&mut self) -> &mut ContextGatherer {
        &mut self.gatherer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deepseek_fim_prompt() {
        let engine = AutocompleteEngine::new();
        let prompt =
            engine.build_fim_prompt(FimFormat::DeepSeek, "fn compute() -> u32 {\n    ", "\n}\n");

        assert!(prompt.prompt.contains("<｜fim begin｜>"));
        assert!(prompt.prompt.contains("<｜fim hole｜>"));
        assert!(prompt.prompt.contains("<｜fim end｜>"));
        assert!(prompt.stop_tokens.contains(&"\n\n".to_string()));
    }

    #[test]
    fn test_cache_roundtrip() {
        let mut engine = AutocompleteEngine::new();
        engine.cache_put("hash_123", "return 42;");
        assert_eq!(engine.cache_get("hash_123"), Some("return 42;"));
        assert_eq!(engine.cache_get("nonexistent"), None);
    }

    #[test]
    fn test_interval_merging() {
        let mut gatherer = ContextGatherer::new();
        gatherer.add_interval(ContextInterval::new("file://a.rs", 1, 5));
        gatherer.add_interval(ContextInterval::new("file://a.rs", 4, 10));
        gatherer.add_interval(ContextInterval::new("file://a.rs", 15, 20));
        gatherer.add_interval(ContextInterval::new("file://b.rs", 1, 3));

        let merged = gatherer.merged_intervals();
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].uri, "file://a.rs");
        assert_eq!(merged[0].start_line, 1);
        assert_eq!(merged[0].end_line, 10);
        assert_eq!(merged[1].start_line, 15);
        assert_eq!(merged[1].end_line, 20);
        assert_eq!(merged[2].uri, "file://b.rs");
    }

    #[test]
    fn test_speculative_cache_matching() {
        let mut spec = SpeculativeCache::new(10);
        spec.put("doc_1", "const value = ", "computeSomething(x);\nconsole.log(value);");

        let matched = spec.match_speculative("doc_1", "const value = compute");
        assert_eq!(
            matched,
            Some("Something(x);\nconsole.log(value);".to_string())
        );

        let mismatched = spec.match_speculative("doc_1", "const value = different");
        assert_eq!(mismatched, None);
    }
}
