//! Async event aggregator agent implementation.
//!
//! This module provides an agent that aggregates events by correlation ID,
//! waiting for all required event types before processing them together.

use crate::agents::BaseAsyncAgent;
use crate::event::Event;
use crate::{MojenticError, Result};
use async_trait::async_trait;
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use tracing::debug;

type EventStore = Arc<Mutex<HashMap<String, Vec<Box<dyn Event>>>>>;
type WaiterStore = Arc<Mutex<HashMap<String, Vec<oneshot::Sender<Vec<Box<dyn Event>>>>>>>;

/// An agent that aggregates events by correlation ID.
///
/// This agent waits for all specified event types to arrive for a given
/// correlation ID before processing them together. This is useful for
/// workflows where multiple independent operations must complete before
/// a final action can be taken.
///
/// # Examples
///
/// ```ignore
/// use mojentic::agents::AsyncAggregatorAgent;
/// use std::any::TypeId;
///
/// let agent = AsyncAggregatorAgent::new(vec![
///     TypeId::of::<Event1>(),
///     TypeId::of::<Event2>(),
/// ]);
/// ```
pub struct AsyncAggregatorAgent {
    event_types_needed: Vec<TypeId>,
    results: EventStore,
    waiters: WaiterStore,
}

impl AsyncAggregatorAgent {
    /// Create a new AsyncAggregatorAgent.
    ///
    /// # Arguments
    ///
    /// * `event_types_needed` - Vector of TypeIds representing the event types
    ///   that must be collected before processing
    pub fn new(event_types_needed: Vec<TypeId>) -> Self {
        Self {
            event_types_needed,
            results: Arc::new(Mutex::new(HashMap::new())),
            waiters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Wait for all needed events for a specific correlation ID.
    ///
    /// This method blocks until all required event types have been received
    /// for the given correlation ID, or until the timeout expires.
    ///
    /// # Arguments
    ///
    /// * `correlation_id` - The correlation ID to wait for
    /// * `timeout` - Optional timeout duration
    ///
    /// # Returns
    ///
    /// Vector of all events collected for this correlation ID
    pub async fn wait_for_events(
        &self,
        correlation_id: &str,
        timeout: Option<Duration>,
    ) -> Result<Vec<Box<dyn Event>>> {
        // Check if we already have all needed events
        {
            let results = self.results.lock().await;
            if let Some(events) = results.get(correlation_id) {
                if self.has_all_needed_types(events) {
                    debug!(
                        "All needed events already available for correlation_id: {}",
                        correlation_id
                    );
                    return Ok(events.iter().map(|e| e.clone_box()).collect());
                }
            }
        }

        // Create a oneshot channel to wait for events
        let (tx, rx) = oneshot::channel();

        // Register the waiter
        {
            let mut waiters = self.waiters.lock().await;
            waiters.entry(correlation_id.to_string()).or_default().push(tx);
        }

        // Wait for the events with optional timeout
        if let Some(timeout_duration) = timeout {
            match tokio::time::timeout(timeout_duration, rx).await {
                Ok(Ok(events)) => Ok(events),
                Ok(Err(_)) => Err(MojenticError::EventError(
                    "Channel closed before events arrived".to_string(),
                )),
                Err(_) => {
                    debug!("Timeout waiting for events for correlation_id: {}", correlation_id);
                    // Return whatever we have collected so far
                    Err(MojenticError::TimeoutError(format!(
                        "Timeout waiting for events for correlation_id: {}",
                        correlation_id
                    )))
                }
            }
        } else {
            rx.await.map_err(|_| {
                MojenticError::EventError("Channel closed before events arrived".to_string())
            })
        }
    }

    /// Process collected events.
    ///
    /// This method is called when all needed event types have been collected.
    /// Override this in subclasses to implement custom processing logic.
    ///
    /// # Arguments
    ///
    /// * `events` - All collected events for a correlation ID
    ///
    /// # Returns
    ///
    /// Vector of new events to emit
    pub async fn process_events(
        &self,
        _events: Vec<Box<dyn Event>>,
    ) -> Result<Vec<Box<dyn Event>>> {
        // Default implementation returns empty
        // Subclasses should override this
        Ok(vec![])
    }

    /// Check if we have all needed event types.
    fn has_all_needed_types(&self, events: &[Box<dyn Event>]) -> bool {
        let event_types: Vec<TypeId> = events.iter().map(|e| e.as_any().type_id()).collect();

        self.event_types_needed
            .iter()
            .all(|needed_type| event_types.contains(needed_type))
    }

    /// Capture an event and check if we have all needed types.
    async fn capture_event(&self, event: Box<dyn Event>) -> Result<Option<Vec<Box<dyn Event>>>> {
        let correlation_id = event
            .correlation_id()
            .ok_or_else(|| MojenticError::EventError("Event missing correlation_id".to_string()))?
            .to_string();

        // Add event to results
        {
            let mut results = self.results.lock().await;
            results.entry(correlation_id.clone()).or_default().push(event);
        }

        // Check if we have all needed events
        let all_events: Option<Vec<Box<dyn Event>>> = {
            let results = self.results.lock().await;
            results
                .get(&correlation_id)
                .map(|events| events.iter().map(|e| e.clone_box()).collect())
        };

        if let Some(events) = all_events {
            if self.has_all_needed_types(&events) {
                debug!("All needed events collected for correlation_id: {}", correlation_id);

                // Notify all waiters
                {
                    let mut waiters = self.waiters.lock().await;
                    if let Some(senders) = waiters.remove(&correlation_id) {
                        for sender in senders {
                            let events_for_waiter: Vec<Box<dyn Event>> =
                                events.iter().map(|e| e.clone_box()).collect();
                            let _ = sender.send(events_for_waiter);
                        }
                    }
                }

                // Clear results for this correlation_id
                {
                    let mut results = self.results.lock().await;
                    results.remove(&correlation_id);
                }

                return Ok(Some(events));
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl BaseAsyncAgent for AsyncAggregatorAgent {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        debug!("AsyncAggregatorAgent received event");

        // Capture the event
        if let Some(events) = self.capture_event(event).await? {
            // We have all needed events, process them
            return self.process_events(events).await;
        }

        // Still waiting for more events
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::any::Any;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Event1 {
        source: String,
        correlation_id: Option<String>,
        data: String,
    }

    impl Event for Event1 {
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

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Event2 {
        source: String,
        correlation_id: Option<String>,
        value: i32,
    }

    impl Event for Event2 {
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

    #[tokio::test]
    async fn test_new_aggregator() {
        let agent = AsyncAggregatorAgent::new(vec![TypeId::of::<Event1>(), TypeId::of::<Event2>()]);
        assert_eq!(agent.event_types_needed.len(), 2);
    }

    #[tokio::test]
    async fn test_single_event_does_not_trigger() {
        let agent = AsyncAggregatorAgent::new(vec![TypeId::of::<Event1>(), TypeId::of::<Event2>()]);

        let event1 = Box::new(Event1 {
            source: "Test".to_string(),
            correlation_id: Some("test-123".to_string()),
            data: "data".to_string(),
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(event1).await.unwrap();
        assert_eq!(result.len(), 0); // Should not process yet
    }

    #[tokio::test]
    async fn test_both_events_trigger_processing() {
        let agent = Arc::new(AsyncAggregatorAgent::new(vec![
            TypeId::of::<Event1>(),
            TypeId::of::<Event2>(),
        ]));

        let event1 = Box::new(Event1 {
            source: "Test".to_string(),
            correlation_id: Some("test-123".to_string()),
            data: "data".to_string(),
        }) as Box<dyn Event>;

        let event2 = Box::new(Event2 {
            source: "Test".to_string(),
            correlation_id: Some("test-123".to_string()),
            value: 42,
        }) as Box<dyn Event>;

        // First event should not trigger
        let result1 = agent.receive_event_async(event1).await.unwrap();
        assert_eq!(result1.len(), 0);

        // Second event should trigger processing
        let result2 = agent.receive_event_async(event2).await.unwrap();
        // Default process_events returns empty, but it should have been called
        assert_eq!(result2.len(), 0);
    }

    #[tokio::test]
    async fn test_wait_for_events() {
        let agent = Arc::new(AsyncAggregatorAgent::new(vec![
            TypeId::of::<Event1>(),
            TypeId::of::<Event2>(),
        ]));

        let correlation_id = "wait-test-456";
        let agent_clone = agent.clone();

        // Spawn a task that will send events
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let event1 = Box::new(Event1 {
                source: "Test".to_string(),
                correlation_id: Some(correlation_id.to_string()),
                data: "data".to_string(),
            }) as Box<dyn Event>;

            agent_clone.receive_event_async(event1).await.unwrap();

            tokio::time::sleep(Duration::from_millis(100)).await;

            let event2 = Box::new(Event2 {
                source: "Test".to_string(),
                correlation_id: Some(correlation_id.to_string()),
                value: 42,
            }) as Box<dyn Event>;

            agent_clone.receive_event_async(event2).await.unwrap();
        });

        // Wait for all events
        let result = agent
            .wait_for_events(correlation_id, Some(Duration::from_secs(5)))
            .await
            .unwrap();

        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_wait_for_events_timeout() {
        let agent = AsyncAggregatorAgent::new(vec![TypeId::of::<Event1>(), TypeId::of::<Event2>()]);

        // Send only one event
        let event1 = Box::new(Event1 {
            source: "Test".to_string(),
            correlation_id: Some("timeout-test".to_string()),
            data: "data".to_string(),
        }) as Box<dyn Event>;

        agent.receive_event_async(event1).await.unwrap();

        // Wait should timeout
        let result = agent.wait_for_events("timeout-test", Some(Duration::from_millis(100))).await;

        assert!(result.is_err());
        match result {
            Err(MojenticError::TimeoutError(_)) => {}
            _ => panic!("Expected TimeoutError"),
        }
    }

    #[tokio::test]
    async fn test_different_correlation_ids() {
        let agent = Arc::new(AsyncAggregatorAgent::new(vec![
            TypeId::of::<Event1>(),
            TypeId::of::<Event2>(),
        ]));

        // Send events with different correlation IDs
        let event1_a = Box::new(Event1 {
            source: "Test".to_string(),
            correlation_id: Some("corr-a".to_string()),
            data: "data-a".to_string(),
        }) as Box<dyn Event>;

        let event1_b = Box::new(Event1 {
            source: "Test".to_string(),
            correlation_id: Some("corr-b".to_string()),
            data: "data-b".to_string(),
        }) as Box<dyn Event>;

        agent.receive_event_async(event1_a).await.unwrap();
        agent.receive_event_async(event1_b).await.unwrap();

        // Complete corr-a
        let event2_a = Box::new(Event2 {
            source: "Test".to_string(),
            correlation_id: Some("corr-a".to_string()),
            value: 1,
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(event2_a).await.unwrap();
        assert_eq!(result.len(), 0); // corr-a completes

        // Complete corr-b
        let event2_b = Box::new(Event2 {
            source: "Test".to_string(),
            correlation_id: Some("corr-b".to_string()),
            value: 2,
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(event2_b).await.unwrap();
        assert_eq!(result.len(), 0); // corr-b completes
    }

    #[tokio::test]
    async fn test_event_without_correlation_id_fails() {
        let agent = AsyncAggregatorAgent::new(vec![TypeId::of::<Event1>()]);

        let event = Box::new(Event1 {
            source: "Test".to_string(),
            correlation_id: None,
            data: "data".to_string(),
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(event).await;
        assert!(result.is_err());
        match result {
            Err(MojenticError::EventError(_)) => {}
            _ => panic!("Expected EventError"),
        }
    }

    #[tokio::test]
    async fn test_process_events_override() {
        struct CustomAggregator {
            inner: AsyncAggregatorAgent,
            processed_count: Arc<Mutex<usize>>,
        }

        impl CustomAggregator {
            fn new(event_types: Vec<TypeId>) -> Self {
                Self {
                    inner: AsyncAggregatorAgent::new(event_types),
                    processed_count: Arc::new(Mutex::new(0)),
                }
            }
        }

        #[async_trait]
        impl BaseAsyncAgent for CustomAggregator {
            async fn receive_event_async(
                &self,
                event: Box<dyn Event>,
            ) -> Result<Vec<Box<dyn Event>>> {
                if let Some(_events) = self.inner.capture_event(event).await? {
                    // Custom processing
                    let mut count = self.processed_count.lock().await;
                    *count += 1;

                    return Ok(vec![]);
                }
                Ok(vec![])
            }
        }

        let agent = CustomAggregator::new(vec![TypeId::of::<Event1>(), TypeId::of::<Event2>()]);
        let count_clone = agent.processed_count.clone();

        let event1 = Box::new(Event1 {
            source: "Test".to_string(),
            correlation_id: Some("custom-test".to_string()),
            data: "data".to_string(),
        }) as Box<dyn Event>;

        let event2 = Box::new(Event2 {
            source: "Test".to_string(),
            correlation_id: Some("custom-test".to_string()),
            value: 42,
        }) as Box<dyn Event>;

        agent.receive_event_async(event1).await.unwrap();
        agent.receive_event_async(event2).await.unwrap();

        let count = *count_clone.lock().await;
        assert_eq!(count, 1); // process_events should have been called once
    }
}
