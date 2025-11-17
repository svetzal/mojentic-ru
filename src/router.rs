//! Event routing system for agents.
//!
//! This module provides the `Router` type that maps event types to agents
//! for processing. The router is used by the dispatcher to determine which
//! agents should receive which events.

use crate::agents::BaseAsyncAgent;
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

/// Routes events to registered agents based on event type.
///
/// The Router maintains a mapping of event TypeIds to vectors of agents
/// that should process those events. Multiple agents can be registered
/// for the same event type.
///
/// # Examples
///
/// ```ignore
/// use mojentic::router::Router;
///
/// let mut router = Router::new();
/// router.add_route::<MyEvent>(my_agent);
/// ```
pub struct Router {
    routes: HashMap<TypeId, Vec<Arc<dyn BaseAsyncAgent>>>,
}

impl Router {
    /// Create a new empty router
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    /// Add a route mapping an event type to an agent
    ///
    /// # Arguments
    ///
    /// * `agent` - The agent to handle events of type T
    pub fn add_route<T: 'static>(&mut self, agent: Arc<dyn BaseAsyncAgent>) {
        let type_id = TypeId::of::<T>();
        self.routes.entry(type_id).or_default().push(agent);
    }

    /// Get all agents registered for a specific event type
    ///
    /// # Arguments
    ///
    /// * `type_id` - The TypeId of the event type
    pub fn get_agents(&self, type_id: TypeId) -> Vec<Arc<dyn BaseAsyncAgent>> {
        self.routes.get(&type_id).cloned().unwrap_or_default()
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::BaseAsyncAgent;
    use crate::event::Event;
    use crate::Result;
    use async_trait::async_trait;

    #[derive(Debug)]
    struct TestEvent1;
    #[derive(Debug)]
    struct TestEvent2;

    struct TestAgent;

    #[async_trait]
    impl BaseAsyncAgent for TestAgent {
        async fn receive_event_async(&self, _event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_router_new() {
        let router = Router::new();
        assert_eq!(router.routes.len(), 0);
    }

    #[test]
    fn test_router_default() {
        let router = Router::default();
        assert_eq!(router.routes.len(), 0);
    }

    #[test]
    fn test_add_route() {
        let mut router = Router::new();
        let agent = Arc::new(TestAgent);

        router.add_route::<TestEvent1>(agent.clone());

        let agents = router.get_agents(TypeId::of::<TestEvent1>());
        assert_eq!(agents.len(), 1);
    }

    #[test]
    fn test_add_multiple_routes_same_type() {
        let mut router = Router::new();
        let agent1 = Arc::new(TestAgent);
        let agent2 = Arc::new(TestAgent);

        router.add_route::<TestEvent1>(agent1);
        router.add_route::<TestEvent1>(agent2);

        let agents = router.get_agents(TypeId::of::<TestEvent1>());
        assert_eq!(agents.len(), 2);
    }

    #[test]
    fn test_add_routes_different_types() {
        let mut router = Router::new();
        let agent1 = Arc::new(TestAgent);
        let agent2 = Arc::new(TestAgent);

        router.add_route::<TestEvent1>(agent1);
        router.add_route::<TestEvent2>(agent2);

        let agents1 = router.get_agents(TypeId::of::<TestEvent1>());
        let agents2 = router.get_agents(TypeId::of::<TestEvent2>());

        assert_eq!(agents1.len(), 1);
        assert_eq!(agents2.len(), 1);
    }

    #[test]
    fn test_get_agents_no_routes() {
        let router = Router::new();
        let agents = router.get_agents(TypeId::of::<TestEvent1>());
        assert_eq!(agents.len(), 0);
    }

    #[test]
    fn test_get_agents_different_type() {
        let mut router = Router::new();
        let agent = Arc::new(TestAgent);

        router.add_route::<TestEvent1>(agent);

        // Try to get agents for a different type
        let agents = router.get_agents(TypeId::of::<TestEvent2>());
        assert_eq!(agents.len(), 0);
    }
}
