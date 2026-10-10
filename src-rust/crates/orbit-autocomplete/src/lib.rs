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

pub struct AutocompleteEngine {
    cache: HashMap<String, String>,
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
}
