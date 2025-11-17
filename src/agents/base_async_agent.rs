//! Base trait for asynchronous agents.
//!
//! This module defines the core `BaseAsyncAgent` trait that all async agents
//! must implement. Agents receive events asynchronously and can produce new
//! events in response.

use crate::event::Event;
use crate::Result;
use async_trait::async_trait;

/// Base trait for all asynchronous agents in the system.
///
/// Agents process events and optionally emit new events. This trait defines
/// the core interface that all agents must implement.
///
/// # Examples
///
/// ```
/// use mojentic::agents::BaseAsyncAgent;
/// use mojentic::event::Event;
/// use mojentic::Result;
/// use async_trait::async_trait;
///
/// struct MyAgent;
///
/// #[async_trait]
/// impl BaseAsyncAgent for MyAgent {
///     async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
///         // Process the event and return new events
///         Ok(vec![])
///     }
/// }
/// ```
#[async_trait]
pub trait BaseAsyncAgent: Send + Sync {
    /// Process an event asynchronously and return resulting events.
    ///
    /// This method is called when an event is routed to this agent. The agent
    /// can perform any async work needed and return zero or more new events
    /// to be processed.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to process
    ///
    /// # Returns
    ///
    /// A vector of new events to be dispatched, or an error if processing failed.
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use serde::{Deserialize, Serialize};
    use std::any::Any;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestEvent {
        source: String,
        correlation_id: Option<String>,
        data: String,
    }

    impl Event for TestEvent {
        fn source(&self) -> &str {
            &self.source
        }

        fn correlation_id(&self) -> Option<&str> {
            self.correlation_id.as_deref()
        }

        fn set_correlation_id(&mut self, id: String) {
            self.correlation_id = Some(id);
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn clone_box(&self) -> Box<dyn Event> {
            Box::new(self.clone())
        }
    }

    struct SimpleAgent;

    #[async_trait]
    impl BaseAsyncAgent for SimpleAgent {
        async fn receive_event_async(&self, _event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
            Ok(vec![])
        }
    }

    struct EchoAgent;

    #[async_trait]
    impl BaseAsyncAgent for EchoAgent {
        async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
            // Echo back a new event
            let new_event = TestEvent {
                source: "EchoAgent".to_string(),
                correlation_id: event.correlation_id().map(|s| s.to_string()),
                data: "echoed".to_string(),
            };
            Ok(vec![Box::new(new_event)])
        }
    }

    #[tokio::test]
    async fn test_simple_agent_returns_empty() {
        let agent = SimpleAgent;
        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: None,
            data: "test".to_string(),
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(event).await.unwrap();
        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    async fn test_echo_agent_returns_event() {
        let agent = EchoAgent;
        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: Some("test-123".to_string()),
            data: "original".to_string(),
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(event).await.unwrap();
        assert_eq!(result.len(), 1);

        let returned_event = result[0].as_any().downcast_ref::<TestEvent>().unwrap();
        assert_eq!(returned_event.source(), "EchoAgent");
        assert_eq!(returned_event.correlation_id(), Some("test-123"));
        assert_eq!(returned_event.data, "echoed");
    }

    #[tokio::test]
    async fn test_agent_preserves_correlation_id() {
        let agent = EchoAgent;
        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: Some("preserve-456".to_string()),
            data: "test".to_string(),
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(event).await.unwrap();
        assert_eq!(result.len(), 1);

        let correlation = result[0].correlation_id();
        assert_eq!(correlation, Some("preserve-456"));
    }
}
