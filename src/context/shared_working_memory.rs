//! Shared working memory for agent communication.
//!
//! This module provides [`SharedWorkingMemory`], a thread-safe shared context
//! that agents can read from and write to. It enables agents to maintain and
//! share state across interactions.
//!
//! # Examples
//!
//! ```
//! use mojentic::context::SharedWorkingMemory;
//! use serde_json::json;
//!
//! let memory = SharedWorkingMemory::new(json!({
//!     "user": {
//!         "name": "Alice",
//!         "age": 30
//!     }
//! }));
//!
//! let current = memory.get_working_memory();
//! assert_eq!(current["user"]["name"], "Alice");
//!
//! memory.merge_to_working_memory(json!({
//!     "user": {
//!         "age": 31,
//!         "city": "Boston"
//!     }
//! }));
//!
//! let updated = memory.get_working_memory();
//! assert_eq!(updated["user"]["age"], 31);
//! assert_eq!(updated["user"]["city"], "Boston");
//! assert_eq!(updated["user"]["name"], "Alice"); // Original value preserved
//! ```

use serde_json::Value;
use std::sync::{Arc, Mutex};

/// Thread-safe shared working memory for agents.
///
/// `SharedWorkingMemory` provides a shared context that multiple agents can
/// read from and write to. It uses deep merging to combine updates with
/// existing state, preserving values that aren't being updated.
///
/// The memory is thread-safe and can be safely cloned and shared across
/// multiple agents and async tasks.
#[derive(Debug, Clone)]
pub struct SharedWorkingMemory {
    memory: Arc<Mutex<Value>>,
}

impl SharedWorkingMemory {
    /// Create a new `SharedWorkingMemory` with initial state.
    ///
    /// # Arguments
    ///
    /// * `initial_memory` - The initial JSON value for the working memory
    ///
    /// # Examples
    ///
    /// ```
    /// use mojentic::context::SharedWorkingMemory;
    /// use serde_json::json;
    ///
    /// let memory = SharedWorkingMemory::new(json!({
    ///     "user": { "name": "Bob" }
    /// }));
    /// ```
    pub fn new(initial_memory: Value) -> Self {
        Self {
            memory: Arc::new(Mutex::new(initial_memory)),
        }
    }

    /// Get a clone of the current working memory.
    ///
    /// Returns a snapshot of the current state. Subsequent changes to the
    /// working memory will not affect this returned value.
    ///
    /// # Examples
    ///
    /// ```
    /// use mojentic::context::SharedWorkingMemory;
    /// use serde_json::json;
    ///
    /// let memory = SharedWorkingMemory::new(json!({"count": 1}));
    /// let snapshot = memory.get_working_memory();
    /// assert_eq!(snapshot["count"], 1);
    /// ```
    pub fn get_working_memory(&self) -> Value {
        self.memory.lock().unwrap().clone()
    }

    /// Merge new values into the working memory.
    ///
    /// Performs a deep merge of the provided value with the existing memory.
    /// For objects, this recursively merges nested fields. For arrays and
    /// primitives, the new value replaces the old value.
    ///
    /// # Arguments
    ///
    /// * `new_memory` - The JSON value to merge into the working memory
    ///
    /// # Examples
    ///
    /// ```
    /// use mojentic::context::SharedWorkingMemory;
    /// use serde_json::json;
    ///
    /// let memory = SharedWorkingMemory::new(json!({
    ///     "user": {
    ///         "name": "Charlie",
    ///         "age": 25
    ///     }
    /// }));
    ///
    /// memory.merge_to_working_memory(json!({
    ///     "user": {
    ///         "age": 26,
    ///         "city": "NYC"
    ///     }
    /// }));
    ///
    /// let result = memory.get_working_memory();
    /// assert_eq!(result["user"]["name"], "Charlie"); // Preserved
    /// assert_eq!(result["user"]["age"], 26);         // Updated
    /// assert_eq!(result["user"]["city"], "NYC");     // Added
    /// ```
    pub fn merge_to_working_memory(&self, new_memory: Value) {
        let mut memory = self.memory.lock().unwrap();
        deep_merge(&mut memory, new_memory);
    }
}

impl Default for SharedWorkingMemory {
    /// Create a new `SharedWorkingMemory` with an empty object as initial state.
    fn default() -> Self {
        Self::new(Value::Object(serde_json::Map::new()))
    }
}

/// Deep merge two JSON values.
///
/// If both values are objects, recursively merge their fields.
/// Otherwise, replace the destination with the source value.
///
/// # Arguments
///
/// * `dest` - The destination value to merge into (modified in place)
/// * `src` - The source value to merge from
fn deep_merge(dest: &mut Value, src: Value) {
    match (dest, src) {
        (Value::Object(dest_map), Value::Object(src_map)) => {
            for (key, value) in src_map {
                dest_map
                    .entry(key)
                    .and_modify(|dest_value| deep_merge(dest_value, value.clone()))
                    .or_insert(value);
            }
        }
        (dest_value, src_value) => {
            *dest_value = src_value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_new() {
        let memory = SharedWorkingMemory::new(json!({"key": "value"}));
        let result = memory.get_working_memory();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn test_default() {
        let memory = SharedWorkingMemory::default();
        let result = memory.get_working_memory();
        assert!(result.is_object());
        assert!(result.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_get_working_memory() {
        let memory = SharedWorkingMemory::new(json!({
            "user": {
                "name": "Alice",
                "age": 30
            }
        }));

        let result = memory.get_working_memory();
        assert_eq!(result["user"]["name"], "Alice");
        assert_eq!(result["user"]["age"], 30);
    }

    #[test]
    fn test_merge_to_working_memory_simple() {
        let memory = SharedWorkingMemory::new(json!({"key1": "value1"}));

        memory.merge_to_working_memory(json!({"key2": "value2"}));

        let result = memory.get_working_memory();
        assert_eq!(result["key1"], "value1");
        assert_eq!(result["key2"], "value2");
    }

    #[test]
    fn test_merge_to_working_memory_deep() {
        let memory = SharedWorkingMemory::new(json!({
            "user": {
                "name": "Bob",
                "age": 25
            }
        }));

        memory.merge_to_working_memory(json!({
            "user": {
                "age": 26,
                "city": "Boston"
            }
        }));

        let result = memory.get_working_memory();
        assert_eq!(result["user"]["name"], "Bob"); // Preserved
        assert_eq!(result["user"]["age"], 26); // Updated
        assert_eq!(result["user"]["city"], "Boston"); // Added
    }

    #[test]
    fn test_merge_to_working_memory_replace_primitive() {
        let memory = SharedWorkingMemory::new(json!({"count": 1}));

        memory.merge_to_working_memory(json!({"count": 2}));

        let result = memory.get_working_memory();
        assert_eq!(result["count"], 2);
    }

    #[test]
    fn test_merge_to_working_memory_replace_array() {
        let memory = SharedWorkingMemory::new(json!({"items": [1, 2, 3]}));

        memory.merge_to_working_memory(json!({"items": [4, 5]}));

        let result = memory.get_working_memory();
        assert_eq!(result["items"], json!([4, 5]));
    }

    #[test]
    fn test_merge_nested_objects() {
        let memory = SharedWorkingMemory::new(json!({
            "level1": {
                "level2": {
                    "level3": {
                        "value": "original"
                    }
                }
            }
        }));

        memory.merge_to_working_memory(json!({
            "level1": {
                "level2": {
                    "level3": {
                        "new_value": "added"
                    }
                }
            }
        }));

        let result = memory.get_working_memory();
        assert_eq!(result["level1"]["level2"]["level3"]["value"], "original");
        assert_eq!(result["level1"]["level2"]["level3"]["new_value"], "added");
    }

    #[test]
    fn test_thread_safety() {
        use std::thread;

        let memory = SharedWorkingMemory::new(json!({"count": 0}));

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let mem = memory.clone();
                thread::spawn(move || {
                    mem.merge_to_working_memory(json!({format!("key{}", i): i}));
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let result = memory.get_working_memory();
        assert_eq!(result["count"], 0);

        // Check that all keys were added
        for i in 0..10 {
            assert!(result.get(format!("key{}", i)).is_some());
        }
    }

    #[test]
    fn test_deep_merge_primitives() {
        let mut dest = json!(42);
        deep_merge(&mut dest, json!(100));
        assert_eq!(dest, json!(100));
    }

    #[test]
    fn test_deep_merge_arrays() {
        let mut dest = json!([1, 2, 3]);
        deep_merge(&mut dest, json!([4, 5]));
        assert_eq!(dest, json!([4, 5]));
    }

    #[test]
    fn test_deep_merge_mixed_types() {
        let mut dest = json!({"key": [1, 2, 3]});
        deep_merge(&mut dest, json!({"key": "string"}));
        assert_eq!(dest, json!({"key": "string"}));
    }

    #[test]
    fn test_deep_merge_empty_objects() {
        let mut dest = json!({});
        deep_merge(&mut dest, json!({"key": "value"}));
        assert_eq!(dest, json!({"key": "value"}));
    }

    #[test]
    fn test_multiple_merges() {
        let memory = SharedWorkingMemory::new(json!({}));

        memory.merge_to_working_memory(json!({"a": 1}));
        memory.merge_to_working_memory(json!({"b": 2}));
        memory.merge_to_working_memory(json!({"c": 3}));

        let result = memory.get_working_memory();
        assert_eq!(result["a"], 1);
        assert_eq!(result["b"], 2);
        assert_eq!(result["c"], 3);
    }

    #[test]
    fn test_clone_memory() {
        let memory1 = SharedWorkingMemory::new(json!({"key": "value"}));
        let memory2 = memory1.clone();

        memory2.merge_to_working_memory(json!({"key2": "value2"}));

        let result1 = memory1.get_working_memory();
        let result2 = memory2.get_working_memory();

        // Both should have the update since they share the same Arc
        assert_eq!(result1["key2"], "value2");
        assert_eq!(result2["key2"], "value2");
    }
}
