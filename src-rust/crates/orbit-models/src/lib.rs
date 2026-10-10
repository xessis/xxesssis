use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    OpenAI,
    Anthropic,
    Gemini,
    DeepSeek,
    XAi,
    Groq,
    Ollama,
    Vllm,
    LmStudio,
    OpenRouter,
    Mistral,
    LiteLLM,
    OpenAiCompatible,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub provider: ProviderId,
    pub model_id: String,
    pub display_name: String,
    pub max_context_tokens: usize,
    pub max_output_tokens: usize,
    pub supports_tools: bool,
    pub supports_reasoning: bool,
    pub supports_fim: bool,
    pub supports_vision: bool,
}

impl ModelCapabilities {
    pub fn new(
        provider: ProviderId,
        model_id: impl Into<String>,
        display_name: impl Into<String>,
        max_context: usize,
        max_output: usize,
    ) -> Self {
        Self {
            provider,
            model_id: model_id.into(),
            display_name: display_name.into(),
            max_context_tokens: max_context,
            max_output_tokens: max_output,
            supports_tools: true,
            supports_reasoning: false,
            supports_fim: false,
            supports_vision: false,
        }
    }

    pub fn with_reasoning(mut self) -> Self {
        self.supports_reasoning = true;
        self
    }

    pub fn with_fim(mut self) -> Self {
        self.supports_fim = true;
        self
    }

    pub fn with_vision(mut self) -> Self {
        self.supports_vision = true;
        self
    }
}

pub struct ModelRegistry {
    models: HashMap<(ProviderId, String), ModelCapabilities>,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRegistry {
    pub fn new() -> Self {
        let mut r = Self {
            models: HashMap::new(),
        };
        r.register_builtin_models();
        r
    }

    fn register_builtin_models(&mut self) {
        // Anthropic
        self.insert(
            ModelCapabilities::new(
                ProviderId::Anthropic,
                "claude-3-7-sonnet",
                "Claude 3.7 Sonnet",
                200_000,
                64_000,
            )
            .with_reasoning()
            .with_vision(),
        );
        self.insert(
            ModelCapabilities::new(
                ProviderId::Anthropic,
                "claude-3-5-sonnet",
                "Claude 3.5 Sonnet",
                200_000,
                8_192,
            )
            .with_vision(),
        );
        self.insert(ModelCapabilities::new(
            ProviderId::Anthropic,
            "claude-3-5-haiku",
            "Claude 3.5 Haiku",
            200_000,
            8_192,
        ));

        // OpenAI
        self.insert(
            ModelCapabilities::new(ProviderId::OpenAI, "gpt-4o", "GPT-4o", 128_000, 16_384)
                .with_vision(),
        );
        self.insert(
            ModelCapabilities::new(
                ProviderId::OpenAI,
                "gpt-4o-mini",
                "GPT-4o mini",
                128_000,
                16_384,
            )
            .with_vision(),
        );
        self.insert(
            ModelCapabilities::new(ProviderId::OpenAI, "o3-mini", "o3-mini", 200_000, 100_000)
                .with_reasoning(),
        );

        // DeepSeek
        self.insert(
            ModelCapabilities::new(
                ProviderId::DeepSeek,
                "deepseek-chat",
                "DeepSeek V3",
                64_000,
                8_192,
            )
            .with_fim(),
        );
        self.insert(
            ModelCapabilities::new(
                ProviderId::DeepSeek,
                "deepseek-reasoner",
                "DeepSeek R1",
                64_000,
                8_192,
            )
            .with_reasoning(),
        );

        // Google Gemini
        self.insert(
            ModelCapabilities::new(
                ProviderId::Gemini,
                "gemini-2.0-flash",
                "Gemini 2.0 Flash",
                1_048_576,
                8_192,
            )
            .with_vision(),
        );
        self.insert(
            ModelCapabilities::new(
                ProviderId::Gemini,
                "gemini-2.0-pro-exp-02-05",
                "Gemini 2.0 Pro",
                2_097_152,
                8_192,
            )
            .with_vision(),
        );

        // Groq
        self.insert(ModelCapabilities::new(
            ProviderId::Groq,
            "llama-3.3-70b-versatile",
            "Llama 3.3 70B",
            128_000,
            32_768,
        ));

        // Ollama local default
        self.insert(
            ModelCapabilities::new(
                ProviderId::Ollama,
                "qwen2.5-coder:7b",
                "Qwen 2.5 Coder 7B",
                32_768,
                4_096,
            )
            .with_fim(),
        );
    }

    pub fn insert(&mut self, caps: ModelCapabilities) {
        self.models
            .insert((caps.provider, caps.model_id.clone()), caps);
    }

    pub fn get(&self, provider: ProviderId, model_id: &str) -> Option<&ModelCapabilities> {
        self.models.get(&(provider, model_id.to_string()))
    }

    pub fn get_or_default(&self, provider: ProviderId, model_id: &str) -> ModelCapabilities {
        if let Some(caps) = self.get(provider, model_id) {
            caps.clone()
        } else {
            // Safe heuristic fallback for unlisted / self-hosted models
            ModelCapabilities::new(provider, model_id, model_id, 32_768, 4_096)
        }
    }
}

pub static GLOBAL_REGISTRY: LazyLock<ModelRegistry> = LazyLock::new(ModelRegistry::new);

pub fn get_capabilities(provider: ProviderId, model_id: &str) -> ModelCapabilities {
    GLOBAL_REGISTRY.get_or_default(provider, model_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_models_lookup() {
        let caps = get_capabilities(ProviderId::Anthropic, "claude-3-7-sonnet");
        assert_eq!(caps.max_context_tokens, 200_000);
        assert!(caps.supports_reasoning);
        assert!(caps.supports_tools);
        assert!(caps.supports_vision);

        let r1 = get_capabilities(ProviderId::DeepSeek, "deepseek-reasoner");
        assert!(r1.supports_reasoning);

        let unlisted = get_capabilities(ProviderId::Ollama, "custom-model:latest");
        assert_eq!(unlisted.max_context_tokens, 32_768);
    }
}
