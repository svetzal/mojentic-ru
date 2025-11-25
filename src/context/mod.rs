//! Context management for agents.
//!
//! This module provides context management capabilities for agents, including
//! shared working memory for maintaining state across agent interactions.

pub mod shared_working_memory;

pub use shared_working_memory::SharedWorkingMemory;
