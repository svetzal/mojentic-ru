# OpenAI Model Registry

## Overview

The OpenAI Model Registry is a centralized system for managing model-specific configurations, capabilities, and parameter requirements for OpenAI models. It provides a compile-time type-safe registry that informs the gateway about which parameters and features each model supports.

This registry is essential for:
- Determining which models support tools, streaming, or vision capabilities
- Selecting the correct token limit parameter (`max_tokens` vs `max_completion_tokens`)
- Understanding temperature support for reasoning models
- Tracking which API endpoints each model supports
- Providing sensible defaults when working with unknown models

## Model Types

The registry categorizes models into four distinct types:

### Reasoning Models

Models that use internal reasoning steps before generating responses. These models use `max_completion_tokens` instead of `max_tokens`.

```rust
ModelType::Reasoning
```

Examples: `o1`, `o1-mini`, `o3`, `o3-mini`, `gpt-5`, `gpt-5.1`

### Chat Models

Standard chat completion models that use `max_tokens` for output control.

```rust
ModelType::Chat
```

Examples: `gpt-4o`, `gpt-4-turbo`, `gpt-3.5-turbo`

### Embedding Models

Models designed for generating text embeddings.

```rust
ModelType::Embedding
```

Examples: `text-embedding-3-small`, `text-embedding-3-large`

### Moderation Models

Models for content moderation and safety classification.

```rust
ModelType::Moderation
```

Examples: `omni-moderation-latest`, `text-moderation-latest`

## Model Capabilities

The `ModelCapabilities` struct defines what each model can do. Here are all the fields:

### Core Capabilities

- `model_type: ModelType` - The type of model (Reasoning, Chat, Embedding, or Moderation)
- `supports_tools: bool` - Whether the model supports function/tool calling
- `supports_streaming: bool` - Whether the model supports streaming responses
- `supports_vision: bool` - Whether the model can process image inputs

### Token Limits

- `max_context_tokens: Option<u32>` - Maximum input context window size in tokens
- `max_output_tokens: Option<u32>` - Maximum number of output tokens the model can generate

### Temperature Support

- `supported_temperatures: Option<Vec<f32>>` - Temperature constraints:
  - `None` - Accepts any temperature value (most chat models)
  - `Some(vec![])` - Does not accept temperature parameter (not exposed to users)
  - `Some(vec![1.0])` - Only accepts temperature 1.0 (reasoning models like o1, o3)

### API Endpoint Support

- `supports_chat_api: bool` - Supports `/v1/chat/completions` endpoint
- `supports_completions_api: bool` - Supports `/v1/completions` endpoint
- `supports_responses_api: bool` - Supports `/v1/responses` endpoint

Note: The OpenAI gateway currently only calls the Chat API. These flags are informational and available for future gateway enhancements.

## API Endpoint Categories

OpenAI models support different API endpoints based on their design and capabilities:

### Chat API (Most Common)

The `/v1/chat/completions` endpoint is supported by most modern models:
- All GPT-4 variants (except legacy completions models)
- Reasoning models (o1, o3, o4-mini)
- Base GPT-5 models

### Completions API (Legacy)

The `/v1/completions` endpoint is supported by older models:
- `babbage-002`
- `davinci-002`
- `gpt-3.5-turbo-instruct`

These models do not support the chat API format.

### Both APIs

Some models support both chat and completions:
- `gpt-4o-mini`
- `gpt-4.1-nano`
- `gpt-5.1`

### Responses API (Newer Models)

The `/v1/responses` endpoint is supported by specialized models:
- `gpt-5-pro`
- `codex-mini-latest`

## Using the Global Registry

The registry provides a global instance for convenience:

```rust
use mojentic::llm::gateways::openai_model_registry::{
    get_model_registry, ModelType
};

// Access the global registry
let registry = get_model_registry();

// Look up model capabilities
let caps = registry.get_model_capabilities("gpt-4o");
assert_eq!(caps.model_type, ModelType::Chat);
assert!(caps.supports_tools);
assert!(caps.supports_streaming);
assert!(caps.supports_vision);
```

## Token Limit Parameters

Different model types use different parameter names for controlling output length:

```rust
use mojentic::llm::gateways::openai_model_registry::get_model_registry;

let registry = get_model_registry();

// Chat models use "max_tokens"
let gpt4_caps = registry.get_model_capabilities("gpt-4o");
assert_eq!(gpt4_caps.get_token_limit_param(), "max_tokens");

// Reasoning models use "max_completion_tokens"
let o1_caps = registry.get_model_capabilities("o1");
assert_eq!(o1_caps.get_token_limit_param(), "max_completion_tokens");
```

## Temperature Support

Reasoning models have restricted temperature support:

```rust
use mojentic::llm::gateways::openai_model_registry::get_model_registry;

let registry = get_model_registry();

// Chat models support arbitrary temperatures
let gpt4_caps = registry.get_model_capabilities("gpt-4o");
assert!(gpt4_caps.supports_temperature(0.7));
assert!(gpt4_caps.supports_temperature(1.5));

// Reasoning models only support temperature 1.0
let o1_caps = registry.get_model_capabilities("o1");
assert!(o1_caps.supports_temperature(1.0));
assert!(!o1_caps.supports_temperature(0.7));
```

## Checking Endpoint Support

You can check which API endpoints a model supports:

```rust
use mojentic::llm::gateways::openai_model_registry::get_model_registry;

let registry = get_model_registry();

// Standard chat model - chat API only
let caps = registry.get_model_capabilities("gpt-4o");
assert!(caps.supports_chat_api);
assert!(!caps.supports_completions_api);
assert!(!caps.supports_responses_api);

// Dual-endpoint model
let mini_caps = registry.get_model_capabilities("gpt-4o-mini");
assert!(mini_caps.supports_chat_api);
assert!(mini_caps.supports_completions_api);
assert!(!mini_caps.supports_responses_api);

// Completions-only model
let instruct_caps = registry.get_model_capabilities("gpt-3.5-turbo-instruct");
assert!(!instruct_caps.supports_chat_api);
assert!(instruct_caps.supports_completions_api);
assert!(!instruct_caps.supports_responses_api);

// Responses-only model
let pro_caps = registry.get_model_capabilities("gpt-5-pro");
assert!(!pro_caps.supports_chat_api);
assert!(!pro_caps.supports_completions_api);
assert!(pro_caps.supports_responses_api);
```

## Creating Custom Registries

While the global registry is convenient, you can create custom registries for testing or specialized configurations:

```rust
use mojentic::llm::gateways::openai_model_registry::{
    OpenAIModelRegistry, ModelCapabilities, ModelType
};

// Create a new empty registry
let mut custom_registry = OpenAIModelRegistry::new();

// Register a custom model
custom_registry.register_model("custom-model", ModelCapabilities {
    model_type: ModelType::Chat,
    supports_tools: true,
    supports_streaming: true,
    supports_vision: false,
    max_context_tokens: Some(8192),
    max_output_tokens: Some(4096),
    supported_temperatures: None, // Accepts any temperature
    supports_chat_api: true,
    supports_completions_api: false,
    supports_responses_api: false,
});

// Use the custom registry
let caps = custom_registry.get_model_capabilities("custom-model");
assert_eq!(caps.model_type, ModelType::Chat);
assert!(caps.supports_tools);
```

## Pattern Matching for Unknown Models

When a model is not explicitly registered, the registry infers capabilities from the model name:

```rust
use mojentic::llm::gateways::openai_model_registry::get_model_registry;

let registry = get_model_registry();

// Unknown model with "gpt-4" prefix inherits GPT-4 capabilities
let unknown_caps = registry.get_model_capabilities("gpt-4-future-model");
assert!(unknown_caps.supports_tools);
assert!(unknown_caps.supports_streaming);

// Unknown model with "o" prefix is treated as a reasoning model
let unknown_o_caps = registry.get_model_capabilities("o5-experimental");
assert_eq!(unknown_o_caps.get_token_limit_param(), "max_completion_tokens");
assert!(unknown_o_caps.supports_temperature(1.0));
assert!(!unknown_o_caps.supports_temperature(0.7));
```

## Common Model Categories

### Chat-Only Models

Most GPT-4, reasoning, and base GPT-5 models:
- `gpt-4o`, `gpt-4-turbo`, `gpt-4`
- `o1`, `o1-mini`, `o3`, `o3-mini`, `o4-mini`
- `gpt-5`, `gpt-5.1`

### Completions-Only Models

Legacy models that don't support chat format:
- `babbage-002`
- `davinci-002`
- `gpt-3.5-turbo-instruct`

### Dual-Endpoint Models

Models supporting both chat and completions:
- `gpt-4o-mini`
- `gpt-4.1-nano`
- `gpt-5.1`

### Responses-Only Models

Specialized newer endpoint models:
- `gpt-5-pro`
- `codex-mini-latest`

## Summary

The OpenAI Model Registry provides:
- Type-safe model capability definitions
- Automatic parameter selection based on model type
- Temperature validation for reasoning models
- API endpoint support tracking
- Pattern matching for unknown models
- A global registry for convenience
- Custom registries for specialized use cases

By consulting the registry, the OpenAI gateway ensures it uses the correct parameters and features for each model, providing a robust and maintainable integration layer.
