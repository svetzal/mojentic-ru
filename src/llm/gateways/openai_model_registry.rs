//! OpenAI Model Registry for managing model-specific configurations and capabilities.
//!
//! This module provides infrastructure for categorizing OpenAI models and managing
//! their specific parameter requirements and capabilities.

use std::collections::HashMap;
use std::sync::LazyLock;
use tracing::warn;

/// Classification of OpenAI model types based on their capabilities and parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelType {
    /// Models like o1, o3 that use max_completion_tokens
    Reasoning,
    /// Standard chat models that use max_tokens
    Chat,
    /// Text embedding models
    Embedding,
    /// Content moderation models
    Moderation,
}

/// Defines the capabilities and parameter requirements for a model.
#[derive(Debug, Clone)]
pub struct ModelCapabilities {
    pub model_type: ModelType,
    pub supports_tools: bool,
    pub supports_streaming: bool,
    pub supports_vision: bool,
    pub max_context_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    /// None means all temperatures supported, empty vec means no temperature parameter allowed
    pub supported_temperatures: Option<Vec<f32>>,
}

impl ModelCapabilities {
    /// Get the correct parameter name for token limits based on model type.
    pub fn get_token_limit_param(&self) -> &'static str {
        if self.model_type == ModelType::Reasoning {
            "max_completion_tokens"
        } else {
            "max_tokens"
        }
    }

    /// Check if the model supports a specific temperature value.
    pub fn supports_temperature(&self, temperature: f32) -> bool {
        match &self.supported_temperatures {
            None => true, // All temperatures supported if not restricted
            Some(temps) if temps.is_empty() => false, // No temperature values supported
            Some(temps) => temps.iter().any(|t| (*t - temperature).abs() < 0.01),
        }
    }
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            model_type: ModelType::Chat,
            supports_tools: true,
            supports_streaming: true,
            supports_vision: false,
            max_context_tokens: None,
            max_output_tokens: None,
            supported_temperatures: None,
        }
    }
}

/// Registry for managing OpenAI model configurations and capabilities.
///
/// This struct provides a centralized way to manage model-specific configurations,
/// parameter mappings, and capabilities for OpenAI models.
pub struct OpenAIModelRegistry {
    models: HashMap<String, ModelCapabilities>,
    pattern_mappings: HashMap<String, ModelType>,
}

impl OpenAIModelRegistry {
    /// Create a new model registry with default models.
    pub fn new() -> Self {
        let mut registry = Self {
            models: HashMap::new(),
            pattern_mappings: HashMap::new(),
        };
        registry.initialize_default_models();
        registry
    }

    fn initialize_default_models(&mut self) {
        // Reasoning Models (o1, o3, o4, gpt-5 series)
        let reasoning_models = vec![
            "o1",
            "o1-2024-12-17",
            "o1-mini",
            "o1-mini-2024-09-12",
            "o1-pro",
            "o1-pro-2025-03-19",
            "o3",
            "o3-2025-04-16",
            "o3-deep-research",
            "o3-deep-research-2025-06-26",
            "o3-mini",
            "o3-mini-2025-01-31",
            "o3-pro",
            "o3-pro-2025-06-10",
            "o4-mini",
            "o4-mini-2025-04-16",
            "o4-mini-deep-research",
            "o4-mini-deep-research-2025-06-26",
            "gpt-5",
            "gpt-5-2025-08-07",
            "gpt-5-chat-latest",
            "gpt-5-codex",
            "gpt-5-mini",
            "gpt-5-mini-2025-08-07",
            "gpt-5-nano",
            "gpt-5-nano-2025-08-07",
        ];

        for model in reasoning_models {
            let is_deep_research = model.contains("deep-research");
            let is_gpt5 = model.contains("gpt-5");
            let is_o1_series = model.starts_with("o1");
            let is_o3_series = model.starts_with("o3");
            let is_o4_series = model.starts_with("o4");
            let is_mini_or_nano = model.contains("mini") || model.contains("nano");

            // GPT-5 models may support more features than o1/o3/o4
            let supports_tools = is_gpt5;
            let supports_streaming = is_gpt5;

            // Set context and output tokens based on model tier
            let (context_tokens, output_tokens) = if is_gpt5 {
                if is_mini_or_nano {
                    (200000, 32768)
                } else {
                    (300000, 50000)
                }
            } else if is_deep_research {
                (200000, 100000)
            } else {
                (128000, 32768)
            };

            // Temperature restrictions based on model series
            let supported_temps = if is_gpt5 || is_o1_series || is_o4_series {
                // GPT-5, o1, and o4 series only support temperature=1.0
                Some(vec![1.0])
            } else if is_o3_series {
                // o3 series doesn't support temperature parameter at all
                Some(vec![])
            } else {
                None
            };

            self.models.insert(
                model.to_string(),
                ModelCapabilities {
                    model_type: ModelType::Reasoning,
                    supports_tools,
                    supports_streaming,
                    supports_vision: false,
                    max_context_tokens: Some(context_tokens),
                    max_output_tokens: Some(output_tokens),
                    supported_temperatures: supported_temps,
                },
            );
        }

        // Chat Models (GPT-4 and GPT-4.1 series)
        let gpt4_and_newer_models = vec![
            "chatgpt-4o-latest",
            "gpt-4",
            "gpt-4-0125-preview",
            "gpt-4-0613",
            "gpt-4-1106-preview",
            "gpt-4-turbo",
            "gpt-4-turbo-2024-04-09",
            "gpt-4-turbo-preview",
            "gpt-4.1",
            "gpt-4.1-2025-04-14",
            "gpt-4.1-mini",
            "gpt-4.1-mini-2025-04-14",
            "gpt-4.1-nano",
            "gpt-4.1-nano-2025-04-14",
            "gpt-4o",
            "gpt-4o-2024-05-13",
            "gpt-4o-2024-08-06",
            "gpt-4o-2024-11-20",
            "gpt-4o-audio-preview",
            "gpt-4o-audio-preview-2024-10-01",
            "gpt-4o-audio-preview-2024-12-17",
            "gpt-4o-audio-preview-2025-06-03",
            "gpt-4o-mini",
            "gpt-4o-mini-2024-07-18",
            "gpt-4o-mini-audio-preview",
            "gpt-4o-mini-audio-preview-2024-12-17",
            "gpt-4o-mini-realtime-preview",
            "gpt-4o-mini-realtime-preview-2024-12-17",
            "gpt-4o-mini-search-preview",
            "gpt-4o-mini-search-preview-2025-03-11",
            "gpt-4o-mini-transcribe",
            "gpt-4o-mini-tts",
            "gpt-4o-realtime-preview",
            "gpt-4o-realtime-preview-2024-10-01",
            "gpt-4o-realtime-preview-2024-12-17",
            "gpt-4o-realtime-preview-2025-06-03",
            "gpt-4o-search-preview",
            "gpt-4o-search-preview-2025-03-11",
            "gpt-4o-transcribe",
        ];

        for model in gpt4_and_newer_models {
            let vision_support = model.contains("gpt-4o")
                || model.contains("audio-preview")
                || model.contains("realtime");
            let is_mini_or_nano = model.contains("mini") || model.contains("nano");
            let is_audio = model.contains("audio")
                || model.contains("realtime")
                || model.contains("transcribe");
            let is_gpt41 = model.contains("gpt-4.1");

            let (context_tokens, output_tokens) = if is_gpt41 {
                if is_mini_or_nano {
                    (128000, 16384)
                } else {
                    (200000, 32768)
                }
            } else if model.contains("gpt-4o") {
                (128000, 16384)
            } else {
                (32000, 8192)
            };

            self.models.insert(
                model.to_string(),
                ModelCapabilities {
                    model_type: ModelType::Chat,
                    supports_tools: true,
                    supports_streaming: !is_audio,
                    supports_vision: vision_support,
                    max_context_tokens: Some(context_tokens),
                    max_output_tokens: Some(output_tokens),
                    supported_temperatures: None,
                },
            );
        }

        // Chat Models (GPT-3.5 series)
        let gpt35_models = vec![
            "gpt-3.5-turbo",
            "gpt-3.5-turbo-0125",
            "gpt-3.5-turbo-1106",
            "gpt-3.5-turbo-16k",
            "gpt-3.5-turbo-instruct",
            "gpt-3.5-turbo-instruct-0914",
        ];

        for model in gpt35_models {
            let is_instruct = model.contains("instruct");

            self.models.insert(
                model.to_string(),
                ModelCapabilities {
                    model_type: ModelType::Chat,
                    supports_tools: !is_instruct,
                    supports_streaming: !is_instruct,
                    supports_vision: false,
                    max_context_tokens: Some(16385),
                    max_output_tokens: Some(4096),
                    supported_temperatures: None,
                },
            );
        }

        // Embedding Models
        let embedding_models = vec![
            "text-embedding-3-large",
            "text-embedding-3-small",
            "text-embedding-ada-002",
        ];

        for model in embedding_models {
            self.models.insert(
                model.to_string(),
                ModelCapabilities {
                    model_type: ModelType::Embedding,
                    supports_tools: false,
                    supports_streaming: false,
                    supports_vision: false,
                    max_context_tokens: None,
                    max_output_tokens: None,
                    supported_temperatures: None,
                },
            );
        }

        // Pattern mappings for unknown models
        self.pattern_mappings.insert("o1".to_string(), ModelType::Reasoning);
        self.pattern_mappings.insert("o3".to_string(), ModelType::Reasoning);
        self.pattern_mappings.insert("o4".to_string(), ModelType::Reasoning);
        self.pattern_mappings.insert("gpt-5".to_string(), ModelType::Reasoning);
        self.pattern_mappings.insert("gpt-4".to_string(), ModelType::Chat);
        self.pattern_mappings.insert("gpt-4.1".to_string(), ModelType::Chat);
        self.pattern_mappings.insert("gpt-3.5".to_string(), ModelType::Chat);
        self.pattern_mappings.insert("chatgpt".to_string(), ModelType::Chat);
        self.pattern_mappings.insert("text-embedding".to_string(), ModelType::Embedding);
        self.pattern_mappings
            .insert("text-moderation".to_string(), ModelType::Moderation);
    }

    /// Get the capabilities for a specific model.
    pub fn get_model_capabilities(&self, model_name: &str) -> ModelCapabilities {
        // Direct lookup first
        if let Some(caps) = self.models.get(model_name) {
            return caps.clone();
        }

        // Pattern matching for unknown models
        let model_lower = model_name.to_lowercase();
        for (pattern, model_type) in &self.pattern_mappings {
            if model_lower.contains(pattern) {
                warn!(
                    model = model_name,
                    pattern = pattern,
                    inferred_type = ?model_type,
                    "Using pattern matching for unknown model"
                );
                return self.get_default_capabilities_for_type(*model_type);
            }
        }

        // Default to chat model if no pattern matches
        warn!(model = model_name, "Unknown model, defaulting to chat model capabilities");
        self.get_default_capabilities_for_type(ModelType::Chat)
    }

    fn get_default_capabilities_for_type(&self, model_type: ModelType) -> ModelCapabilities {
        match model_type {
            ModelType::Reasoning => ModelCapabilities {
                model_type: ModelType::Reasoning,
                supports_tools: false,
                supports_streaming: false,
                supports_vision: false,
                max_context_tokens: None,
                max_output_tokens: None,
                supported_temperatures: None,
            },
            ModelType::Chat => ModelCapabilities {
                model_type: ModelType::Chat,
                supports_tools: true,
                supports_streaming: true,
                supports_vision: false,
                max_context_tokens: None,
                max_output_tokens: None,
                supported_temperatures: None,
            },
            ModelType::Embedding => ModelCapabilities {
                model_type: ModelType::Embedding,
                supports_tools: false,
                supports_streaming: false,
                supports_vision: false,
                max_context_tokens: None,
                max_output_tokens: None,
                supported_temperatures: None,
            },
            ModelType::Moderation => ModelCapabilities {
                model_type: ModelType::Moderation,
                supports_tools: false,
                supports_streaming: false,
                supports_vision: false,
                max_context_tokens: None,
                max_output_tokens: None,
                supported_temperatures: None,
            },
        }
    }

    /// Check if a model is a reasoning model.
    pub fn is_reasoning_model(&self, model_name: &str) -> bool {
        let capabilities = self.get_model_capabilities(model_name);
        capabilities.model_type == ModelType::Reasoning
    }

    /// Get a list of all explicitly registered models.
    pub fn get_registered_models(&self) -> Vec<String> {
        self.models.keys().cloned().collect()
    }

    /// Register a new model with its capabilities.
    pub fn register_model(&mut self, model_name: &str, capabilities: ModelCapabilities) {
        self.models.insert(model_name.to_string(), capabilities);
    }

    /// Register a pattern for inferring model types.
    pub fn register_pattern(&mut self, pattern: &str, model_type: ModelType) {
        self.pattern_mappings.insert(pattern.to_string(), model_type);
    }
}

impl Default for OpenAIModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global registry instance.
pub static MODEL_REGISTRY: LazyLock<OpenAIModelRegistry> = LazyLock::new(OpenAIModelRegistry::new);

/// Get the global OpenAI model registry instance.
pub fn get_model_registry() -> &'static OpenAIModelRegistry {
    &MODEL_REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_enum() {
        assert_ne!(ModelType::Reasoning, ModelType::Chat);
        assert_eq!(ModelType::Reasoning, ModelType::Reasoning);
    }

    #[test]
    fn test_model_capabilities_default() {
        let caps = ModelCapabilities::default();
        assert_eq!(caps.model_type, ModelType::Chat);
        assert!(caps.supports_tools);
        assert!(caps.supports_streaming);
        assert!(!caps.supports_vision);
    }

    #[test]
    fn test_get_token_limit_param_reasoning() {
        let caps = ModelCapabilities {
            model_type: ModelType::Reasoning,
            ..Default::default()
        };
        assert_eq!(caps.get_token_limit_param(), "max_completion_tokens");
    }

    #[test]
    fn test_get_token_limit_param_chat() {
        let caps = ModelCapabilities {
            model_type: ModelType::Chat,
            ..Default::default()
        };
        assert_eq!(caps.get_token_limit_param(), "max_tokens");
    }

    #[test]
    fn test_supports_temperature_unrestricted() {
        let caps = ModelCapabilities {
            supported_temperatures: None,
            ..Default::default()
        };
        assert!(caps.supports_temperature(0.5));
        assert!(caps.supports_temperature(1.0));
        assert!(caps.supports_temperature(0.0));
    }

    #[test]
    fn test_supports_temperature_restricted() {
        let caps = ModelCapabilities {
            supported_temperatures: Some(vec![1.0]),
            ..Default::default()
        };
        assert!(caps.supports_temperature(1.0));
        assert!(!caps.supports_temperature(0.5));
    }

    #[test]
    fn test_supports_temperature_none_allowed() {
        let caps = ModelCapabilities {
            supported_temperatures: Some(vec![]),
            ..Default::default()
        };
        assert!(!caps.supports_temperature(1.0));
        assert!(!caps.supports_temperature(0.5));
    }

    #[test]
    fn test_registry_new() {
        let registry = OpenAIModelRegistry::new();
        assert!(!registry.models.is_empty());
        assert!(!registry.pattern_mappings.is_empty());
    }

    #[test]
    fn test_get_known_model_capabilities() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-4");
        assert_eq!(caps.model_type, ModelType::Chat);
        assert!(caps.supports_tools);
    }

    #[test]
    fn test_get_reasoning_model_capabilities() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("o1");
        assert_eq!(caps.model_type, ModelType::Reasoning);
    }

    #[test]
    fn test_get_unknown_model_pattern_matching() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-4-unknown-version");
        assert_eq!(caps.model_type, ModelType::Chat);
    }

    #[test]
    fn test_get_unknown_model_default() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("completely-unknown-model");
        assert_eq!(caps.model_type, ModelType::Chat);
    }

    #[test]
    fn test_is_reasoning_model() {
        let registry = OpenAIModelRegistry::new();
        assert!(registry.is_reasoning_model("o1"));
        assert!(registry.is_reasoning_model("o3-mini"));
        assert!(!registry.is_reasoning_model("gpt-4"));
    }

    #[test]
    fn test_get_registered_models() {
        let registry = OpenAIModelRegistry::new();
        let models = registry.get_registered_models();
        assert!(models.contains(&"gpt-4".to_string()));
        assert!(models.contains(&"o1".to_string()));
    }

    #[test]
    fn test_register_model() {
        let mut registry = OpenAIModelRegistry::new();
        registry.register_model(
            "custom-model",
            ModelCapabilities {
                model_type: ModelType::Chat,
                supports_tools: false,
                ..Default::default()
            },
        );
        let caps = registry.get_model_capabilities("custom-model");
        assert!(!caps.supports_tools);
    }

    #[test]
    fn test_register_pattern() {
        let mut registry = OpenAIModelRegistry::new();
        registry.register_pattern("custom-pattern", ModelType::Embedding);
        let caps = registry.get_model_capabilities("my-custom-pattern-model");
        assert_eq!(caps.model_type, ModelType::Embedding);
    }

    #[test]
    fn test_global_registry() {
        let registry = get_model_registry();
        let caps = registry.get_model_capabilities("gpt-4");
        assert_eq!(caps.model_type, ModelType::Chat);
    }

    #[test]
    fn test_embedding_models() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("text-embedding-3-large");
        assert_eq!(caps.model_type, ModelType::Embedding);
        assert!(!caps.supports_tools);
        assert!(!caps.supports_streaming);
    }

    #[test]
    fn test_gpt35_instruct_no_tools() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-3.5-turbo-instruct");
        assert_eq!(caps.model_type, ModelType::Chat);
        assert!(!caps.supports_tools);
        assert!(!caps.supports_streaming);
    }
}
