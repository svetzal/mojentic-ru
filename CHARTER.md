# Mojentic Rust — Project Charter

## Purpose

Mojentic-ru is the Rust implementation of the Mojentic LLM integration framework. It provides a type-safe, async-first abstraction over multiple LLM providers (Ollama, OpenAI, Anthropic) with tool calling, structured output, streaming, and an event-driven agent system. It exists to give Rust developers a production-ready library for building LLM-powered applications without provider lock-in.

## Goals

- Provide a unified async API for interacting with multiple LLM providers through a single `LlmBroker` interface
- Leverage Rust's type system for safe structured output generation and tool calling
- Support an event-driven multi-agent architecture with shared working memory and ReAct-pattern reasoning
- Maintain full feature parity with the Python, Elixir, and TypeScript implementations of Mojentic
- Include comprehensive examples and documentation so the library is learnable without external guidance
- Publish to crates.io with automated CI/CD for quality gates and releases

## Non-Goals

- Being a standalone AI application or end-user product — this is a library
- Implementing LLM model training, fine-tuning, or hosting
- Replacing provider-specific SDKs for advanced provider-only features outside the common abstraction
- Supporting synchronous/blocking APIs — the framework is async-only by design

## Target Users

Rust developers building applications that integrate with LLMs, particularly those who need multi-provider support, structured output, tool use, or multi-agent orchestration.
