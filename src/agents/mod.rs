//! Agent system for event processing.
//!
//! This module provides the agent framework for building reactive, event-driven
//! systems. Agents process events and can emit new events in response.
//!
//! # Agent Types
//!
//! - [`BaseAgent`] - Synchronous agent trait for simple, non-blocking event processing
//! - [`BaseAsyncAgent`] - Asynchronous agent trait for I/O-bound operations
//!
//! # Implementations
//!
//! - [`AsyncLlmAgent`] - LLM-powered async agent
//! - [`AsyncAggregatorAgent`] - Aggregates events from multiple sources
//! - [`IterativeProblemSolver`] - Iterative approach to problem solving
//! - [`SimpleRecursiveAgent`] - Basic recursive event processing

pub mod async_aggregator_agent;
pub mod async_llm_agent;
pub mod base_agent;
pub mod base_async_agent;
pub mod iterative_problem_solver;
pub mod simple_recursive_agent;

pub use async_aggregator_agent::AsyncAggregatorAgent;
pub use async_llm_agent::AsyncLlmAgent;
pub use base_agent::BaseAgent;
pub use base_async_agent::BaseAsyncAgent;
pub use iterative_problem_solver::IterativeProblemSolver;
pub use simple_recursive_agent::SimpleRecursiveAgent;
