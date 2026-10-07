# Reasoning controls and native history

Reasoning has three independent controls: the requested effort, the selected
model capability profile, and retention of native assistant reasoning. Output
budgets and application stop policies remain separate.

## Configure a known model

```rust
use mojentic::llm::{CompletionConfig, ReasoningProfile};
use mojentic::llm::gateway::ReasoningEffort;

let config = CompletionConfig {
    reasoning_profile: Some(ReasoningProfile::qwen3_8()),
    reasoning_effort: Some(ReasoningEffort::Medium),
    preserve_reasoning: true,
    ..Default::default()
};
config.validate_reasoning().expect("supported model controls");
```

Profiles are explicit capabilities, not model-name matching. The Qwen3.8 profile
accepts Disabled, Low, Medium, and XHigh. High is not an alias for XHigh and is
rejected before HTTP. Applications can construct another ReasoningProfile with
its exact supported controls and native-history capability.

An omitted profile delegates model validation to the provider. An omitted effort
leaves the provider/model default unspecified, even when a profile is selected.
For comparable benchmarks, specify effort explicitly. Profiles do not claim to
know server overrides or the final rendered prompt.

## Provider mappings

| Control | Ollama | oMLX |
| --- | --- | --- |
| Omitted | No think field | No effort or enable flag |
| Disabled | think=false | enable_thinking=false |
| Enabled | think=true | enable_thinking=true |
| Low, Medium, High, XHigh | Named string in think | Named string in reasoning_effort |

Enabled requests boolean enablement without choosing a depth. Use it for models
that accept boolean controls; the Qwen3.8 profile deliberately requires a named
level or omission instead. Named values are never collapsed to true. Provider
or model validation may reject an unsupported name when no profile is supplied.
Ordinary, streaming, and structured output share the same mapping.

The OpenAI gateway retains its existing model registry for named effort support.
It can send XHigh to reasoning models; individual models may reject it. Boolean
Enabled and automatic native-history preservation fail explicitly in that
gateway. Its legacy Disabled behavior is unchanged. The OpenAI adapter does not
send the oMLX-specific reasoning_content extension to OpenAI.

## Preserve completed reasoning

LlmMessage now carries optional reasoning_content separately from content. The
Ollama gateway sends it as assistant thinking; oMLX sends assistant
reasoning_content. Existing JSON without this field remains readable, and an
incoming thinking field is accepted as an alias. Constructors initialize it to
None; with_reasoning attaches it explicitly.

```rust
use mojentic::llm::LlmMessage;
let message = LlmMessage::assistant("I will inspect the file.")
    .with_reasoning("Inspect before editing.");
assert!(message.reasoning_content.is_some());
```

preserve_reasoning defaults to false. When enabled, the broker retains reasoning
from completed ordinary or streaming responses that request tools, paired with
the correct assistant message and tool results. It does not emit hidden
reasoning as visible response text. oMLX also requests
chat_template_kwargs.preserve_thinking=true; forced server settings can still
take precedence. Disabling retention does not disable reasoning generation.

Manually supplied message history belongs to the caller: the two local gateways
serialize native assistant reasoning already present even when automatic broker
retention is off. Callers that manage their own loop must retain responses
explicitly. ChatSession's convenience text-only history API is unchanged.

A failed or cancelled stream is not a completed assistant turn. This feature
does not fabricate a closing think block, continue a partial response, or convert
a partial reasoning tail to user instructions. Applications must define and
measure that separate policy. Retention can increase context size and preserve
wrong plans; compatibility tests do not establish better task performance.

## Prototype migration

This unreleased Rust prototype adds fields to LlmMessage and CompletionConfig,
and variants to ReasoningEffort. Rust struct literals and exhaustive matches
need updating; message constructors and CompletionConfig defaults ease migration.
Other language ports are unchanged. Consume the audited Git revision during
validation; a coordinated release and final version decision are separate work.
