//! ReAct pattern implementation module.
//!
//! This module provides a complete implementation of the Reasoning and Acting (ReAct)
//! pattern, where agents iteratively plan, decide, act, and summarize to answer
//! user queries.
//!
//! # Components
//!
//! - **Models**: Data structures for context, plans, and observations
//! - **Events**: Event types for coordinating the ReAct loop
//! - **Agents**: Specialized agents for thinking, deciding, acting, and summarizing
//! - **Formatters**: Helper functions for formatting prompts
//!
//! # Example
//!
//! See `examples/react.rs` for a complete working example.

pub mod decisioning_agent;
pub mod events;
pub mod formatters;
pub mod models;
pub mod summarization_agent;
pub mod thinking_agent;
pub mod tool_call_agent;

pub use decisioning_agent::DecisioningAgent;
pub use events::{
    FailureOccurred, FinishAndSummarize, InvokeDecisioning, InvokeThinking, InvokeToolCall,
};
pub use formatters::{format_available_tools, format_current_context};
pub use models::{CurrentContext, NextAction, Plan, ThoughtActionObservation};
pub use summarization_agent::SummarizationAgent;
pub use thinking_agent::ThinkingAgent;
pub use tool_call_agent::ToolCallAgent;
