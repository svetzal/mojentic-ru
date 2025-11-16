//! Event storage with callbacks and filtering
//!
//! This module provides thread-safe event storage with support for callbacks,
//! filtering by type, time range, and custom predicates.

use super::tracer_events::TracerEvent;
use std::sync::{Arc, Mutex};

/// Type alias for event callback functions
pub type EventCallback = Arc<dyn Fn(&dyn TracerEvent) + Send + Sync>;

/// Store for capturing and querying tracer events
///
/// EventStore provides thread-safe storage for tracer events with support for:
/// - Callbacks triggered on each stored event
/// - Filtering by event type
/// - Filtering by time range
/// - Custom filter predicates
/// - Query for last N events
pub struct EventStore {
    events: Arc<Mutex<Vec<Box<dyn TracerEvent>>>>,
    on_store_callback: Option<EventCallback>,
}

impl EventStore {
    /// Create a new event store
    ///
    /// # Arguments
    ///
    /// * `on_store_callback` - Optional callback function called whenever an event is stored
    pub fn new(on_store_callback: Option<EventCallback>) -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            on_store_callback,
        }
    }

    /// Store an event in the event store
    ///
    /// If a callback is configured, it will be called with the stored event.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to store
    pub fn store(&self, event: Box<dyn TracerEvent>) {
        // Trigger callback before storing (if exists)
        if let Some(callback) = &self.on_store_callback {
            callback(event.as_ref());
        }

        // Store the event
        let mut events = self.events.lock().unwrap();
        events.push(event);
    }

    /// Count events matching filters
    ///
    /// # Arguments
    ///
    /// * `start_time` - Include events with timestamp >= start_time
    /// * `end_time` - Include events with timestamp <= end_time
    /// * `filter_func` - Custom filter function to apply to events
    ///
    /// # Returns
    ///
    /// Number of events matching the filter criteria
    #[allow(clippy::type_complexity)]
    pub fn count_events(
        &self,
        start_time: Option<f64>,
        end_time: Option<f64>,
        filter_func: Option<&dyn Fn(&dyn TracerEvent) -> bool>,
    ) -> usize {
        let events = self.events.lock().unwrap();
        let mut count = 0;

        for event in events.iter() {
            let event_ref = event.as_ref();

            // Filter by time range
            if let Some(start) = start_time {
                if event_ref.timestamp() < start {
                    continue;
                }
            }

            if let Some(end) = end_time {
                if event_ref.timestamp() > end {
                    continue;
                }
            }

            // Apply custom filter function
            if let Some(filter) = filter_func {
                if !filter(event_ref) {
                    continue;
                }
            }

            count += 1;
        }

        count
    }

    /// Get summaries of events matching filters
    ///
    /// Returns printable summaries instead of cloning events
    ///
    /// # Arguments
    ///
    /// * `start_time` - Include events with timestamp >= start_time
    /// * `end_time` - Include events with timestamp <= end_time
    /// * `filter_func` - Custom filter function to apply to events
    ///
    /// # Returns
    ///
    /// Vector of event summaries matching the filter criteria
    #[allow(clippy::type_complexity)]
    pub fn get_event_summaries(
        &self,
        start_time: Option<f64>,
        end_time: Option<f64>,
        filter_func: Option<&dyn Fn(&dyn TracerEvent) -> bool>,
    ) -> Vec<String> {
        let events = self.events.lock().unwrap();
        let mut result = Vec::new();

        for event in events.iter() {
            let event_ref = event.as_ref();

            // Filter by time range
            if let Some(start) = start_time {
                if event_ref.timestamp() < start {
                    continue;
                }
            }

            if let Some(end) = end_time {
                if event_ref.timestamp() > end {
                    continue;
                }
            }

            // Apply custom filter function
            if let Some(filter) = filter_func {
                if !filter(event_ref) {
                    continue;
                }
            }

            result.push(event_ref.printable_summary());
        }

        result
    }

    /// Get the last N event summaries, optionally filtered
    ///
    /// # Arguments
    ///
    /// * `n` - Number of events to return
    /// * `filter_func` - Optional custom filter function
    ///
    /// # Returns
    ///
    /// Vector of the last N event summaries matching the filter criteria
    #[allow(clippy::type_complexity)]
    pub fn get_last_n_summaries(
        &self,
        n: usize,
        filter_func: Option<&dyn Fn(&dyn TracerEvent) -> bool>,
    ) -> Vec<String> {
        let events = self.events.lock().unwrap();

        let filtered: Vec<_> = if let Some(filter) = filter_func {
            events.iter().filter(|e| filter(e.as_ref())).collect()
        } else {
            events.iter().collect()
        };

        let start_idx = if n < filtered.len() {
            filtered.len() - n
        } else {
            0
        };

        filtered[start_idx..].iter().map(|e| e.as_ref().printable_summary()).collect()
    }

    /// Clear all events from the store
    pub fn clear(&self) {
        let mut events = self.events.lock().unwrap();
        events.clear();
    }

    /// Get the total number of events in the store
    pub fn len(&self) -> usize {
        let events = self.events.lock().unwrap();
        events.len()
    }

    /// Check if the event store is empty
    pub fn is_empty(&self) -> bool {
        let events = self.events.lock().unwrap();
        events.is_empty()
    }
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracer::tracer_events::LlmCallTracerEvent;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn current_timestamp() -> f64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
    }

    #[test]
    fn test_store_event() {
        let store = EventStore::default();

        let event = Box::new(LlmCallTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: "test-123".to_string(),
            source: "test".to_string(),
            model: "llama3.2".to_string(),
            messages: vec![],
            temperature: 1.0,
            tools: None,
        });

        store.store(event);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_callback_triggered() {
        let callback_count = Arc::new(AtomicUsize::new(0));
        let callback_count_clone = Arc::clone(&callback_count);

        let callback: EventCallback = Arc::new(move |_event| {
            callback_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let store = EventStore::new(Some(callback));

        let event = Box::new(LlmCallTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: "test-123".to_string(),
            source: "test".to_string(),
            model: "llama3.2".to_string(),
            messages: vec![],
            temperature: 1.0,
            tools: None,
        });

        store.store(event);
        assert_eq!(callback_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_clear() {
        let store = EventStore::default();

        let event = Box::new(LlmCallTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: "test-123".to_string(),
            source: "test".to_string(),
            model: "llama3.2".to_string(),
            messages: vec![],
            temperature: 1.0,
            tools: None,
        });

        store.store(event);
        assert_eq!(store.len(), 1);

        store.clear();
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
    }

    #[test]
    fn test_multiple_events() {
        let store = EventStore::default();

        for i in 0..5 {
            let event = Box::new(LlmCallTracerEvent {
                timestamp: current_timestamp(),
                correlation_id: format!("test-{}", i),
                source: "test".to_string(),
                model: "llama3.2".to_string(),
                messages: vec![],
                temperature: 1.0,
                tools: None,
            });
            store.store(event);
        }

        assert_eq!(store.len(), 5);
    }

    #[test]
    fn test_len_and_is_empty() {
        let store = EventStore::default();
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());

        let event = Box::new(LlmCallTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: "test-123".to_string(),
            source: "test".to_string(),
            model: "llama3.2".to_string(),
            messages: vec![],
            temperature: 1.0,
            tools: None,
        });

        store.store(event);
        assert_eq!(store.len(), 1);
        assert!(!store.is_empty());
    }
}
