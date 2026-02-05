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
    pub supports_chat_api: bool,
    pub supports_completions_api: bool,
    pub supports_responses_api: bool,
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
            supports_chat_api: true,
            supports_completions_api: false,
            supports_responses_api: false,
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
        // Updated 2026-02-04 based on OpenAI API audit
        // Reasoning Models (o1, o3, o4, gpt-5 series)
        let reasoning_models = vec![
            "o1",
            "o1-2024-12-17",
            "o3",
            "o3-2025-04-16",
            "o3-mini",
            "o3-mini-2025-01-31",
            "o4-mini",
            "o4-mini-2025-04-16",
            "gpt-5",
            "gpt-5-2025-08-07",
            "gpt-5-mini",
            "gpt-5-mini-2025-08-07",
            "gpt-5-nano",
            "gpt-5-nano-2025-08-07",
            "gpt-5-pro",
            "gpt-5-pro-2025-10-06",
            "gpt-5.1",
            "gpt-5.1-2025-11-13",
            "gpt-5.1-chat-latest",
            "gpt-5.2",
            "gpt-5.2-2025-12-11",
            "gpt-5.2-chat-latest",
        ];

        for model in reasoning_models {
            let is_gpt5_mini = model == "gpt-5-mini";
            let is_o4_mini = model == "o4-mini";
            let is_o_series =
                model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4");
            let is_gpt5_series = model.starts_with("gpt-5");
            let is_mini_or_nano = model.contains("mini") || model.contains("nano");

            // All reasoning models now support tools and streaming (audit 2026-02-04)
            // Exceptions: gpt-5-mini (base) and o4-mini (base) do not support tools
            let supports_tools = !is_gpt5_mini && !is_o4_mini;
            let supports_streaming = true;

            // Set context and output tokens based on model tier
            let (context_tokens, output_tokens) = if is_gpt5_series {
                if is_mini_or_nano {
                    (200000, 32768)
                } else {
                    (300000, 50000)
                }
            } else {
                (128000, 32768)
            };

            // Temperature restrictions based on model series (audit 2026-02-04)
            // o1 series: temperature=1.0 only
            // o3 series: temperature=1.0 only (was: no temperature)
            // o4 series: temperature=1.0 only
            // gpt-5 base/mini/nano/pro: temperature=1.0 only
            // gpt-5.1*: all temperatures
            // gpt-5.2*: all temperatures
            let supported_temps = if model.starts_with("gpt-5.1") || model.starts_with("gpt-5.2") {
                None // All temperatures supported
            } else if is_o_series || is_gpt5_series {
                Some(vec![1.0]) // Only temperature=1.0
            } else {
                None
            };

            // Endpoint support flags
            let is_responses_only =
                model.contains("pro") || model.contains("deep-research") || model == "gpt-5-codex";
            let is_both_endpoint = model == "gpt-5.1" || model == "gpt-5.1-2025-11-13";

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
                    supports_chat_api: !is_responses_only,
                    supports_completions_api: is_both_endpoint,
                    supports_responses_api: is_responses_only,
                },
            );
        }

        // Chat Models (GPT-4, GPT-4.1, and GPT-5 chat series)
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
            "gpt-4o-audio-preview-2024-12-17",
            "gpt-4o-audio-preview-2025-06-03",
            "gpt-4o-mini",
            "gpt-4o-mini-2024-07-18",
            "gpt-4o-mini-audio-preview",
            "gpt-4o-mini-audio-preview-2024-12-17",
            "gpt-4o-mini-search-preview",
            "gpt-4o-mini-search-preview-2025-03-11",
            "gpt-4o-search-preview",
            "gpt-4o-search-preview-2025-03-11",
            "gpt-5-chat-latest",
            "gpt-5-search-api",
            "gpt-5-search-api-2025-10-14",
        ];

        for model in gpt4_and_newer_models {
            // Audit 2026-02-04: Keep vision=true for gpt-4o (probe limitation, not real capability change)
            let vision_support = model.contains("gpt-4o");
            let is_mini_or_nano = model.contains("mini") || model.contains("nano");
            let is_audio = model.contains("audio-preview");
            let is_search = model.contains("search");
            let is_gpt41 = model.contains("gpt-4.1");
            let is_gpt41_nano_base = model == "gpt-4.1-nano";

            // Audit 2026-02-04: chatgpt-4o-latest, gpt-4.1-nano (base only), audio models, and search models don't support tools
            let supports_tools =
                model != "chatgpt-4o-latest" && !is_gpt41_nano_base && !is_audio && !is_search;

            // Audio models don't support streaming (require audio modality)
            let supports_streaming = !is_audio;

            let (context_tokens, output_tokens) = if is_gpt41 {
                if is_mini_or_nano {
                    (128000, 16384)
                } else {
                    (200000, 32768)
                }
            } else if model.contains("gpt-4o") {
                (128000, 16384)
            } else if model.starts_with("gpt-5") {
                (300000, 50000)
            } else {
                (32000, 8192)
            };

            // Search models don't allow temperature parameter
            let supported_temps = if is_search { Some(vec![]) } else { None };

            // Endpoint support flags
            let is_both_endpoint = model == "gpt-4.1-nano"
                || model == "gpt-4.1-nano-2025-04-14"
                || model == "gpt-4o-mini"
                || model == "gpt-4o-mini-2024-07-18";

            self.models.insert(
                model.to_string(),
                ModelCapabilities {
                    model_type: ModelType::Chat,
                    supports_tools,
                    supports_streaming,
                    supports_vision: vision_support,
                    max_context_tokens: Some(context_tokens),
                    max_output_tokens: Some(output_tokens),
                    supported_temperatures: supported_temps,
                    supports_chat_api: true,
                    supports_completions_api: is_both_endpoint,
                    supports_responses_api: false,
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
                    supports_chat_api: !is_instruct,
                    supports_completions_api: is_instruct,
                    supports_responses_api: false,
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
                    supports_chat_api: false,
                    supports_completions_api: false,
                    supports_responses_api: false,
                },
            );
        }

        // Legacy & Codex Models - completions-only and responses-only
        self.models.insert(
            "babbage-002".to_string(),
            ModelCapabilities {
                model_type: ModelType::Chat,
                supports_tools: false,
                supports_streaming: false,
                supports_vision: false,
                max_context_tokens: Some(16384),
                max_output_tokens: Some(4096),
                supported_temperatures: None,
                supports_chat_api: false,
                supports_completions_api: true,
                supports_responses_api: false,
            },
        );
        self.models.insert(
            "davinci-002".to_string(),
            ModelCapabilities {
                model_type: ModelType::Chat,
                supports_tools: false,
                supports_streaming: false,
                supports_vision: false,
                max_context_tokens: Some(16384),
                max_output_tokens: Some(4096),
                supported_temperatures: None,
                supports_chat_api: false,
                supports_completions_api: true,
                supports_responses_api: false,
            },
        );
        self.models.insert(
            "gpt-5.1-codex-mini".to_string(),
            ModelCapabilities {
                model_type: ModelType::Reasoning,
                supports_tools: false,
                supports_streaming: false,
                supports_vision: false,
                max_context_tokens: Some(200000),
                max_output_tokens: Some(32768),
                supported_temperatures: None,
                supports_chat_api: false,
                supports_completions_api: true,
                supports_responses_api: false,
            },
        );
        self.models.insert(
            "codex-mini-latest".to_string(),
            ModelCapabilities {
                model_type: ModelType::Reasoning,
                supports_tools: false,
                supports_streaming: false,
                supports_vision: false,
                max_context_tokens: Some(200000),
                max_output_tokens: Some(32768),
                supported_temperatures: None,
                supports_chat_api: false,
                supports_completions_api: false,
                supports_responses_api: true,
            },
        );

        // Pattern mappings for unknown models
        self.pattern_mappings.insert("o1".to_string(), ModelType::Reasoning);
        self.pattern_mappings.insert("o3".to_string(), ModelType::Reasoning);
        self.pattern_mappings.insert("o4".to_string(), ModelType::Reasoning);
        self.pattern_mappings.insert("gpt-5.2".to_string(), ModelType::Reasoning);
        self.pattern_mappings.insert("gpt-5.1".to_string(), ModelType::Reasoning);
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
                supports_chat_api: true,
                supports_completions_api: false,
                supports_responses_api: false,
            },
            ModelType::Chat => ModelCapabilities {
                model_type: ModelType::Chat,
                supports_tools: true,
                supports_streaming: true,
                supports_vision: false,
                max_context_tokens: None,
                max_output_tokens: None,
                supported_temperatures: None,
                supports_chat_api: true,
                supports_completions_api: false,
                supports_responses_api: false,
            },
            ModelType::Embedding => ModelCapabilities {
                model_type: ModelType::Embedding,
                supports_tools: false,
                supports_streaming: false,
                supports_vision: false,
                max_context_tokens: None,
                max_output_tokens: None,
                supported_temperatures: None,
                supports_chat_api: false,
                supports_completions_api: false,
                supports_responses_api: false,
            },
            ModelType::Moderation => ModelCapabilities {
                model_type: ModelType::Moderation,
                supports_tools: false,
                supports_streaming: false,
                supports_vision: false,
                max_context_tokens: None,
                max_output_tokens: None,
                supported_temperatures: None,
                supports_chat_api: false,
                supports_completions_api: false,
                supports_responses_api: false,
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

    // Tests for audit 2026-02-04 changes

    #[test]
    fn test_o1_supports_tools_and_streaming() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("o1");
        assert!(caps.supports_tools);
        assert!(caps.supports_streaming);
        assert_eq!(caps.supported_temperatures, Some(vec![1.0]));
    }

    #[test]
    fn test_o3_supports_tools_and_streaming_and_temperature() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("o3");
        assert!(caps.supports_tools);
        assert!(caps.supports_streaming);
        assert_eq!(caps.supported_temperatures, Some(vec![1.0]));
    }

    #[test]
    fn test_o3_mini_supports_tools_and_streaming() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("o3-mini");
        assert!(caps.supports_tools);
        assert!(caps.supports_streaming);
        assert_eq!(caps.supported_temperatures, Some(vec![1.0]));
    }

    #[test]
    fn test_o4_mini_no_tools_but_supports_streaming() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("o4-mini");
        assert!(!caps.supports_tools);
        assert!(caps.supports_streaming);
    }

    #[test]
    fn test_o4_mini_dated_supports_tools() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("o4-mini-2025-04-16");
        assert!(caps.supports_tools);
        assert!(caps.supports_streaming);
    }

    #[test]
    fn test_chatgpt_4o_latest_no_tools() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("chatgpt-4o-latest");
        assert!(!caps.supports_tools);
        assert!(caps.supports_streaming);
    }

    #[test]
    fn test_gpt41_nano_base_no_tools() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-4.1-nano");
        assert!(!caps.supports_tools);
    }

    #[test]
    fn test_gpt41_nano_dated_has_tools() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-4.1-nano-2025-04-14");
        assert!(caps.supports_tools);
    }

    #[test]
    fn test_audio_preview_no_tools_no_streaming() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-4o-audio-preview");
        assert!(!caps.supports_tools);
        assert!(!caps.supports_streaming);
    }

    #[test]
    fn test_search_preview_no_tools_no_temperature() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-4o-search-preview");
        assert!(!caps.supports_tools);
        assert!(caps.supports_streaming);
        assert_eq!(caps.supported_temperatures, Some(vec![]));
    }

    #[test]
    fn test_gpt5_chat_latest_is_chat_type() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-5-chat-latest");
        assert_eq!(caps.model_type, ModelType::Chat);
        assert!(caps.supports_tools);
        assert_eq!(caps.supported_temperatures, None);
    }

    #[test]
    fn test_gpt5_mini_base_no_tools() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-5-mini");
        assert!(!caps.supports_tools);
    }

    #[test]
    fn test_gpt5_mini_dated_has_tools() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-5-mini-2025-08-07");
        assert!(caps.supports_tools);
    }

    #[test]
    fn test_gpt5_pro_exists() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-5-pro");
        assert_eq!(caps.model_type, ModelType::Reasoning);
        assert!(caps.supports_tools);
    }

    #[test]
    fn test_gpt5_search_api_no_tools_no_temperature() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-5-search-api");
        assert_eq!(caps.model_type, ModelType::Chat);
        assert!(!caps.supports_tools);
        assert_eq!(caps.supported_temperatures, Some(vec![]));
    }

    #[test]
    fn test_gpt51_all_temperatures() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-5.1");
        assert_eq!(caps.model_type, ModelType::Reasoning);
        assert_eq!(caps.supported_temperatures, None);
    }

    #[test]
    fn test_gpt52_all_temperatures() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-5.2");
        assert_eq!(caps.model_type, ModelType::Reasoning);
        assert_eq!(caps.supported_temperatures, None);
    }

    #[test]
    fn test_gpt51_pattern_matching() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-5.1-unknown");
        assert_eq!(caps.model_type, ModelType::Reasoning);
    }

    #[test]
    fn test_gpt52_pattern_matching() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-5.2-unknown");
        assert_eq!(caps.model_type, ModelType::Reasoning);
    }

    #[test]
    fn test_deprecated_models_removed() {
        let registry = OpenAIModelRegistry::new();
        let models = registry.get_registered_models();

        // Verify removed models are not in registry
        assert!(!models.contains(&"o1-mini".to_string()));
        assert!(!models.contains(&"o1-mini-2024-09-12".to_string()));
        assert!(!models.contains(&"o1-pro".to_string()));
        assert!(!models.contains(&"o3-pro".to_string()));
        assert!(!models.contains(&"o3-deep-research".to_string()));
        assert!(!models.contains(&"o4-mini-deep-research".to_string()));
        assert!(!models.contains(&"gpt-4o-audio-preview-2024-10-01".to_string()));
        assert!(!models.contains(&"gpt-5-codex".to_string()));
    }

    #[test]
    fn test_chat_only_model_endpoint_flags() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-4");
        assert!(caps.supports_chat_api);
        assert!(!caps.supports_completions_api);
        assert!(!caps.supports_responses_api);
    }

    #[test]
    fn test_both_endpoint_model_flags() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-4o-mini");
        assert!(caps.supports_chat_api);
        assert!(caps.supports_completions_api);
        assert!(!caps.supports_responses_api);
    }

    #[test]
    fn test_completions_only_model_flags() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-3.5-turbo-instruct");
        assert!(!caps.supports_chat_api);
        assert!(caps.supports_completions_api);
        assert!(!caps.supports_responses_api);
    }

    #[test]
    fn test_responses_only_model_flags() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-5-pro");
        assert!(!caps.supports_chat_api);
        assert!(!caps.supports_completions_api);
        assert!(caps.supports_responses_api);
    }

    #[test]
    fn test_legacy_completions_model_flags() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("babbage-002");
        assert!(!caps.supports_chat_api);
        assert!(caps.supports_completions_api);
        assert!(!caps.supports_responses_api);
    }

    #[test]
    fn test_embedding_model_endpoint_flags() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("text-embedding-3-large");
        assert!(!caps.supports_chat_api);
        assert!(!caps.supports_completions_api);
        assert!(!caps.supports_responses_api);
    }

    #[test]
    fn test_codex_mini_latest_responses_only() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("codex-mini-latest");
        assert!(!caps.supports_chat_api);
        assert!(!caps.supports_completions_api);
        assert!(caps.supports_responses_api);
    }

    #[test]
    fn test_gpt51_both_chat_and_completions() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("gpt-5.1");
        assert!(caps.supports_chat_api);
        assert!(caps.supports_completions_api);
        assert!(!caps.supports_responses_api);
    }

    #[test]
    fn test_default_capabilities_include_endpoint_flags() {
        let registry = OpenAIModelRegistry::new();
        let caps = registry.get_model_capabilities("completely-unknown-model-xyz");
        assert!(caps.supports_chat_api);
        assert!(!caps.supports_completions_api);
        assert!(!caps.supports_responses_api);
    }
}
