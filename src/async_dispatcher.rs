//! Asynchronous event dispatcher.
//!
//! This module provides the `AsyncDispatcher` that manages event processing
//! in a background task, routing events to registered agents via a router.

use crate::event::{Event, TerminateEvent};
use crate::router::Router;
use crate::{MojenticError, Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, info};
use uuid::Uuid;

/// Asynchronous event dispatcher for agent systems.
///
/// The dispatcher manages a queue of events and routes them to registered
/// agents via a router. It runs in a background task and can be stopped
/// gracefully.
///
/// # Examples
///
/// ```ignore
/// use mojentic::async_dispatcher::AsyncDispatcher;
/// use mojentic::router::Router;
///
/// let router = Arc::new(Router::new());
/// let mut dispatcher = AsyncDispatcher::new(router);
///
/// dispatcher.start().await.unwrap();
/// dispatcher.dispatch(my_event);
/// dispatcher.wait_for_empty_queue(Some(Duration::from_secs(10))).await.unwrap();
/// dispatcher.stop().await.unwrap();
/// ```
pub struct AsyncDispatcher {
    router: Arc<Router>,
    event_queue: Arc<Mutex<VecDeque<Box<dyn Event>>>>,
    stop_flag: Arc<AtomicBool>,
    task_handle: Option<JoinHandle<()>>,
    batch_size: usize,
}

impl AsyncDispatcher {
    /// Create a new AsyncDispatcher.
    ///
    /// # Arguments
    ///
    /// * `router` - The router to use for routing events to agents
    pub fn new(router: Arc<Router>) -> Self {
        Self {
            router,
            event_queue: Arc::new(Mutex::new(VecDeque::new())),
            stop_flag: Arc::new(AtomicBool::new(false)),
            task_handle: None,
            batch_size: 5,
        }
    }

    /// Set the batch size (number of events to process per iteration).
    ///
    /// # Arguments
    ///
    /// * `size` - The batch size
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Start the event dispatch task.
    ///
    /// This spawns a background task that processes events from the queue.
    pub async fn start(&mut self) -> Result<()> {
        if self.task_handle.is_some() {
            return Err(MojenticError::DispatcherError("Dispatcher already started".to_string()));
        }

        debug!("Starting async dispatcher");
        self.stop_flag.store(false, Ordering::Relaxed);

        let router = self.router.clone();
        let queue = self.event_queue.clone();
        let stop_flag = self.stop_flag.clone();
        let batch_size = self.batch_size;

        let handle = tokio::spawn(async move {
            Self::dispatch_loop(router, queue, stop_flag, batch_size).await;
        });

        self.task_handle = Some(handle);
        info!("Async dispatcher started");

        Ok(())
    }

    /// Stop the event dispatch task.
    ///
    /// This signals the background task to stop and waits for it to complete.
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(handle) = self.task_handle.take() {
            debug!("Stopping async dispatcher");
            self.stop_flag.store(true, Ordering::Relaxed);
            handle.await.map_err(|e| {
                MojenticError::DispatcherError(format!("Failed to stop dispatcher: {}", e))
            })?;
            info!("Async dispatcher stopped");
        }

        Ok(())
    }

    /// Dispatch an event to the queue.
    ///
    /// The event will be processed asynchronously by the background task.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to dispatch
    pub fn dispatch(&self, mut event: Box<dyn Event>) {
        // Assign correlation_id if not set
        if event.correlation_id().is_none() {
            event.set_correlation_id(Uuid::new_v4().to_string());
        }

        let queue = self.event_queue.clone();
        tokio::spawn(async move {
            let mut q = queue.lock().await;
            debug!("Dispatching event: {:?}", event);
            q.push_back(event);
        });
    }

    /// Wait for the event queue to become empty.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Optional timeout duration
    ///
    /// # Returns
    ///
    /// `true` if the queue is empty, `false` if timeout was reached
    pub async fn wait_for_empty_queue(&self, timeout: Option<Duration>) -> Result<bool> {
        let start = tokio::time::Instant::now();

        loop {
            let len = {
                let queue = self.event_queue.lock().await;
                queue.len()
            };

            if len == 0 {
                return Ok(true);
            }

            if let Some(timeout_duration) = timeout {
                if start.elapsed() > timeout_duration {
                    return Ok(false);
                }
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Get the current queue length.
    pub async fn queue_len(&self) -> usize {
        let queue = self.event_queue.lock().await;
        queue.len()
    }

    /// Background dispatch loop.
    async fn dispatch_loop(
        router: Arc<Router>,
        queue: Arc<Mutex<VecDeque<Box<dyn Event>>>>,
        stop_flag: Arc<AtomicBool>,
        batch_size: usize,
    ) {
        while !stop_flag.load(Ordering::Relaxed) {
            for _ in 0..batch_size {
                let event = {
                    let mut q = queue.lock().await;
                    q.pop_front()
                };

                if let Some(event) = event {
                    debug!("Processing event: {:?}", event);

                    // Check for terminate event
                    if event.as_any().is::<TerminateEvent>() {
                        info!("Received TerminateEvent, stopping dispatcher");
                        stop_flag.store(true, Ordering::Relaxed);
                        break;
                    }

                    // Get the event type
                    let type_id = event.as_any().type_id();

                    // Get agents for this event type
                    let agents = router.get_agents(type_id);
                    debug!("Found {} agents for event type", agents.len());

                    // Process event through each agent
                    for agent in agents {
                        debug!("Sending event to agent");
                        match agent.receive_event_async(event.clone_box()).await {
                            Ok(new_events) => {
                                debug!("Agent returned {} events", new_events.len());
                                // Add new events to queue
                                let mut q = queue.lock().await;
                                for new_event in new_events {
                                    q.push_back(new_event);
                                }
                            }
                            Err(e) => {
                                tracing::error!("Agent error processing event: {}", e);
                            }
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        debug!("Dispatch loop exiting");
    }
}

impl Drop for AsyncDispatcher {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::BaseAsyncAgent;
    use async_trait::async_trait;
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

    struct CountingAgent {
        count: Arc<Mutex<usize>>,
    }

    #[async_trait]
    impl BaseAsyncAgent for CountingAgent {
        async fn receive_event_async(&self, _event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
            let mut count = self.count.lock().await;
            *count += 1;
            Ok(vec![])
        }
    }

    #[allow(dead_code)]
    struct EchoAgent;

    #[async_trait]
    impl BaseAsyncAgent for EchoAgent {
        async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
            // Echo back a new event
            let new_event = Box::new(TestEvent {
                source: "EchoAgent".to_string(),
                correlation_id: event.correlation_id().map(|s| s.to_string()),
                data: "echoed".to_string(),
            }) as Box<dyn Event>;
            Ok(vec![new_event])
        }
    }

    #[tokio::test]
    async fn test_dispatcher_new() {
        let router = Arc::new(Router::new());
        let dispatcher = AsyncDispatcher::new(router);
        assert!(dispatcher.task_handle.is_none());
        assert_eq!(dispatcher.batch_size, 5);
    }

    #[tokio::test]
    async fn test_dispatcher_with_batch_size() {
        let router = Arc::new(Router::new());
        let dispatcher = AsyncDispatcher::new(router).with_batch_size(10);
        assert_eq!(dispatcher.batch_size, 10);
    }

    #[tokio::test]
    async fn test_start_and_stop() {
        let router = Arc::new(Router::new());
        let mut dispatcher = AsyncDispatcher::new(router);

        dispatcher.start().await.unwrap();
        assert!(dispatcher.task_handle.is_some());

        dispatcher.stop().await.unwrap();
        assert!(dispatcher.task_handle.is_none());
    }

    #[tokio::test]
    async fn test_start_twice_fails() {
        let router = Arc::new(Router::new());
        let mut dispatcher = AsyncDispatcher::new(router);

        dispatcher.start().await.unwrap();
        let result = dispatcher.start().await;
        assert!(result.is_err());

        dispatcher.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_dispatch_event() {
        let mut router = Router::new();
        let count = Arc::new(Mutex::new(0));
        let agent = Arc::new(CountingAgent {
            count: count.clone(),
        });

        router.add_route::<TestEvent>(agent);

        let mut dispatcher = AsyncDispatcher::new(Arc::new(router));
        dispatcher.start().await.unwrap();

        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: Some("test-123".to_string()),
            data: "test".to_string(),
        }) as Box<dyn Event>;

        dispatcher.dispatch(event);

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(500)).await;

        let final_count = *count.lock().await;
        assert_eq!(final_count, 1);

        dispatcher.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_dispatch_assigns_correlation_id() {
        let router = Arc::new(Router::new());
        let mut dispatcher = AsyncDispatcher::new(router);
        dispatcher.start().await.unwrap();

        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: None,
            data: "test".to_string(),
        }) as Box<dyn Event>;

        dispatcher.dispatch(event);

        // Give time for the event to be queued
        tokio::time::sleep(Duration::from_millis(100)).await;

        dispatcher.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_wait_for_empty_queue() {
        let mut router = Router::new();
        let count = Arc::new(Mutex::new(0));
        let agent = Arc::new(CountingAgent {
            count: count.clone(),
        });

        router.add_route::<TestEvent>(agent);

        let mut dispatcher = AsyncDispatcher::new(Arc::new(router));
        dispatcher.start().await.unwrap();

        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: Some("test-456".to_string()),
            data: "test".to_string(),
        }) as Box<dyn Event>;

        dispatcher.dispatch(event);

        let result = dispatcher.wait_for_empty_queue(Some(Duration::from_secs(2))).await.unwrap();

        assert!(result);
        dispatcher.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_wait_for_empty_queue_timeout() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct SlowAgent {
            processing_count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl BaseAsyncAgent for SlowAgent {
            async fn receive_event_async(
                &self,
                _event: Box<dyn Event>,
            ) -> Result<Vec<Box<dyn Event>>> {
                // Simulate slow processing
                tokio::time::sleep(Duration::from_millis(200)).await;
                self.processing_count.fetch_add(1, Ordering::Relaxed);
                Ok(vec![])
            }
        }

        let mut router = Router::new();
        let processing_count = Arc::new(AtomicUsize::new(0));
        let agent = Arc::new(SlowAgent {
            processing_count: processing_count.clone(),
        });
        router.add_route::<TestEvent>(agent);

        let mut dispatcher = AsyncDispatcher::new(Arc::new(router));
        dispatcher.start().await.unwrap();

        // Dispatch multiple events that take time to process
        for i in 0..10 {
            let event = Box::new(TestEvent {
                source: "Test".to_string(),
                correlation_id: Some(format!("slow-{}", i)),
                data: "test".to_string(),
            }) as Box<dyn Event>;
            dispatcher.dispatch(event);
        }

        // Give time for events to be queued
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Queue should not be empty within short timeout due to slow processing
        let result =
            dispatcher.wait_for_empty_queue(Some(Duration::from_millis(300))).await.unwrap();

        assert!(!result); // Should timeout before all events are processed

        dispatcher.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_queue_len() {
        let router = Arc::new(Router::new());
        let mut dispatcher = AsyncDispatcher::new(router);

        assert_eq!(dispatcher.queue_len().await, 0);

        dispatcher.start().await.unwrap();

        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: Some("len-test".to_string()),
            data: "test".to_string(),
        }) as Box<dyn Event>;

        dispatcher.dispatch(event);

        // Give time for the event to be queued
        tokio::time::sleep(Duration::from_millis(100)).await;

        dispatcher.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_terminate_event_stops_dispatcher() {
        let mut router = Router::new();
        let count = Arc::new(Mutex::new(0));
        let agent = Arc::new(CountingAgent {
            count: count.clone(),
        });

        router.add_route::<TestEvent>(agent.clone());
        router.add_route::<TerminateEvent>(agent);

        let mut dispatcher = AsyncDispatcher::new(Arc::new(router));
        dispatcher.start().await.unwrap();

        // Send normal event
        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: Some("before-stop".to_string()),
            data: "test".to_string(),
        }) as Box<dyn Event>;

        dispatcher.dispatch(event);

        // Send terminate event
        let terminate = Box::new(TerminateEvent::new("System")) as Box<dyn Event>;
        dispatcher.dispatch(terminate);

        // Wait for dispatcher to stop itself
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Dispatcher should have stopped
        assert!(dispatcher.stop_flag.load(Ordering::Relaxed));

        dispatcher.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_agents_receive_event() {
        let mut router = Router::new();
        let count1 = Arc::new(Mutex::new(0));
        let count2 = Arc::new(Mutex::new(0));

        let agent1 = Arc::new(CountingAgent {
            count: count1.clone(),
        });
        let agent2 = Arc::new(CountingAgent {
            count: count2.clone(),
        });

        router.add_route::<TestEvent>(agent1);
        router.add_route::<TestEvent>(agent2);

        let mut dispatcher = AsyncDispatcher::new(Arc::new(router));
        dispatcher.start().await.unwrap();

        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: Some("multi-agent".to_string()),
            data: "test".to_string(),
        }) as Box<dyn Event>;

        dispatcher.dispatch(event);

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_eq!(*count1.lock().await, 1);
        assert_eq!(*count2.lock().await, 1);

        dispatcher.stop().await.unwrap();
    }
}
