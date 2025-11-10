pub mod error;
pub mod llm;

pub use error::{MojenticError, Result};

/// Prelude module for common imports
pub mod prelude {
    pub use crate::error::{MojenticError, Result};
    pub use crate::llm::gateways::OllamaGateway;
    pub use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
    pub use crate::llm::{CompletionConfig, LlmBroker, LlmGateway, LlmMessage, MessageRole};
}
