pub mod agents;
pub mod async_dispatcher;
pub mod context;
pub mod error;
pub mod event;
pub mod llm;
pub mod router;
pub mod tracer;

// Example implementations (for documentation and reference)
pub mod examples;

pub use error::{MojenticError, Result};

/// Prelude module for common imports
pub mod prelude {
    pub use crate::agents::{
        AsyncAggregatorAgent, AsyncLlmAgent, BaseAsyncAgent, IterativeProblemSolver,
    };
    pub use crate::async_dispatcher::AsyncDispatcher;
    pub use crate::context::SharedWorkingMemory;
    pub use crate::error::{MojenticError, Result};
    pub use crate::event::{Event, TerminateEvent};
    pub use crate::llm::gateways::OllamaGateway;
    pub use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
    pub use crate::llm::{CompletionConfig, LlmBroker, LlmGateway, LlmMessage, MessageRole};
    pub use crate::router::Router;
}
