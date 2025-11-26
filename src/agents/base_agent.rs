//! Base trait for synchronous agents.
//!
//! This module defines the core `BaseAgent` trait that synchronous agents
//! implement. Agents receive events and can produce new events in response.
//!
//! For agents that need to perform async operations (I/O, LLM calls, etc.),
//! use the `BaseAsyncAgent` trait instead.

use crate::event::Event;

/// Base trait for synchronous agents in the system.
///
/// Agents process events and optionally emit new events. This trait defines
/// the simplest agent interface for synchronous event processing.
///
/// # When to Use
///
/// Use `BaseAgent` when:
/// - Event processing doesn't require I/O operations
/// - Processing is fast and won't block
/// - You have a simple transformation pipeline
///
/// Use `BaseAsyncAgent` when:
/// - You need to call LLMs or external APIs
/// - Processing involves database queries
/// - Operations may take significant time
///
/// # Examples
///
/// ```
/// use mojentic::agents::BaseAgent;
/// use mojentic::event::Event;
///
/// struct TransformAgent;
///
/// impl BaseAgent for TransformAgent {
///     fn receive_event(&self, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
///         // Process the event synchronously and return new events
///         vec![]
///     }
/// }
/// ```
pub trait BaseAgent: Send + Sync {
    /// Process an event synchronously and return resulting events.
    ///
    /// This method is called when an event is routed to this agent. The agent
    /// can process the event and return zero or more new events to be processed.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to process
    ///
    /// # Returns
    ///
    /// A vector of new events to be dispatched (can be empty).
    fn receive_event(&self, event: Box<dyn Event>) -> Vec<Box<dyn Event>>;
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

    impl BaseAgent for SimpleAgent {
        fn receive_event(&self, _event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
            vec![]
        }
    }

    struct EchoAgent;

    impl BaseAgent for EchoAgent {
        fn receive_event(&self, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
            // Echo back a new event
            let new_event = TestEvent {
                source: "EchoAgent".to_string(),
                correlation_id: event.correlation_id().map(|s| s.to_string()),
                data: "echoed".to_string(),
            };
            vec![Box::new(new_event)]
        }
    }

    struct MultiEventAgent;

    impl BaseAgent for MultiEventAgent {
        fn receive_event(&self, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
            vec![
                Box::new(TestEvent {
                    source: "MultiEventAgent".to_string(),
                    correlation_id: event.correlation_id().map(|s| s.to_string()),
                    data: "event1".to_string(),
                }),
                Box::new(TestEvent {
                    source: "MultiEventAgent".to_string(),
                    correlation_id: event.correlation_id().map(|s| s.to_string()),
                    data: "event2".to_string(),
                }),
            ]
        }
    }

    #[test]
    fn test_simple_agent_returns_empty() {
        let agent = SimpleAgent;
        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: None,
            data: "test".to_string(),
        }) as Box<dyn Event>;

        let result = agent.receive_event(event);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_echo_agent_returns_event() {
        let agent = EchoAgent;
        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: Some("test-123".to_string()),
            data: "original".to_string(),
        }) as Box<dyn Event>;

        let result = agent.receive_event(event);
        assert_eq!(result.len(), 1);

        let returned_event = result[0].as_any().downcast_ref::<TestEvent>().unwrap();
        assert_eq!(returned_event.source(), "EchoAgent");
        assert_eq!(returned_event.correlation_id(), Some("test-123"));
        assert_eq!(returned_event.data, "echoed");
    }

    #[test]
    fn test_agent_preserves_correlation_id() {
        let agent = EchoAgent;
        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: Some("preserve-456".to_string()),
            data: "test".to_string(),
        }) as Box<dyn Event>;

        let result = agent.receive_event(event);
        assert_eq!(result.len(), 1);

        let correlation = result[0].correlation_id();
        assert_eq!(correlation, Some("preserve-456"));
    }

    #[test]
    fn test_agent_can_return_multiple_events() {
        let agent = MultiEventAgent;
        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: Some("multi-789".to_string()),
            data: "test".to_string(),
        }) as Box<dyn Event>;

        let result = agent.receive_event(event);
        assert_eq!(result.len(), 2);

        let event1 = result[0].as_any().downcast_ref::<TestEvent>().unwrap();
        assert_eq!(event1.data, "event1");
        assert_eq!(event1.correlation_id(), Some("multi-789"));

        let event2 = result[1].as_any().downcast_ref::<TestEvent>().unwrap();
        assert_eq!(event2.data, "event2");
        assert_eq!(event2.correlation_id(), Some("multi-789"));
    }

    #[test]
    fn test_agent_without_correlation_id() {
        let agent = EchoAgent;
        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: None,
            data: "test".to_string(),
        }) as Box<dyn Event>;

        let result = agent.receive_event(event);
        assert_eq!(result.len(), 1);

        let returned_event = result[0].as_any().downcast_ref::<TestEvent>().unwrap();
        assert_eq!(returned_event.correlation_id(), None);
    }
}
