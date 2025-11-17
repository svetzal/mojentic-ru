//! Event system for agent communication.
//!
//! This module provides the core event types that agents use to communicate.
//! Events are the fundamental unit of information exchange in the agent system.
//!
//! # Examples
//!
//! ```
//! use mojentic::event::Event;
//! use serde::{Deserialize, Serialize};
//! use std::any::Any;
//!
//! #[derive(Debug, Clone, Serialize, Deserialize)]
//! struct MyCustomEvent {
//!     source: String,
//!     correlation_id: Option<String>,
//!     data: String,
//! }
//!
//! impl Event for MyCustomEvent {
//!     fn source(&self) -> &str {
//!         &self.source
//!     }
//!
//!     fn correlation_id(&self) -> Option<&str> {
//!         self.correlation_id.as_deref()
//!     }
//!
//!     fn set_correlation_id(&mut self, id: String) {
//!         self.correlation_id = Some(id);
//!     }
//!
//!     fn as_any(&self) -> &dyn Any {
//!         self
//!     }
//!
//!     fn clone_box(&self) -> Box<dyn Event> {
//!         Box::new(self.clone())
//!     }
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::any::Any;

/// Base trait for all events in the agent system.
///
/// Events are used to communicate between agents in an asynchronous manner.
/// Each event has a source (the agent that created it) and optionally a
/// correlation_id to track related events through a workflow.
pub trait Event: Send + Sync + std::fmt::Debug {
    /// Returns the source agent identifier
    fn source(&self) -> &str;

    /// Returns the correlation ID if set
    fn correlation_id(&self) -> Option<&str>;

    /// Sets the correlation ID
    fn set_correlation_id(&mut self, id: String);

    /// Cast to Any for downcasting
    fn as_any(&self) -> &dyn Any;

    /// Clone the event into a Box
    fn clone_box(&self) -> Box<dyn Event>;
}

/// A special event type that signals the dispatcher to terminate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateEvent {
    pub source: String,
    pub correlation_id: Option<String>,
}

impl Event for TerminateEvent {
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

impl TerminateEvent {
    /// Create a new TerminateEvent
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            correlation_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_event_source() {
        let event = TestEvent {
            source: "TestAgent".to_string(),
            correlation_id: None,
            data: "test".to_string(),
        };

        assert_eq!(event.source(), "TestAgent");
    }

    #[test]
    fn test_event_correlation_id() {
        let mut event = TestEvent {
            source: "TestAgent".to_string(),
            correlation_id: None,
            data: "test".to_string(),
        };

        assert_eq!(event.correlation_id(), None);

        event.set_correlation_id("test-123".to_string());
        assert_eq!(event.correlation_id(), Some("test-123"));
    }

    #[test]
    fn test_terminate_event() {
        let mut event = TerminateEvent::new("System");
        assert_eq!(event.source(), "System");
        assert_eq!(event.correlation_id(), None);

        event.set_correlation_id("stop-123".to_string());
        assert_eq!(event.correlation_id(), Some("stop-123"));
    }

    #[test]
    fn test_event_clone_box() {
        let event = TestEvent {
            source: "TestAgent".to_string(),
            correlation_id: Some("test-456".to_string()),
            data: "test data".to_string(),
        };

        let cloned = event.clone_box();
        assert_eq!(cloned.source(), "TestAgent");
        assert_eq!(cloned.correlation_id(), Some("test-456"));
    }

    #[test]
    fn test_event_as_any() {
        let event = TestEvent {
            source: "TestAgent".to_string(),
            correlation_id: None,
            data: "test".to_string(),
        };

        let any = event.as_any();
        assert!(any.is::<TestEvent>());
        assert!(any.downcast_ref::<TestEvent>().is_some());
    }
}
