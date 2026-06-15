use std::collections::HashSet;

use serde_json::Value;

pub struct TreeYielder {
    /// A function/closure that determines if a value should be yielded.
    /// Corresponds to `yieldValue` in the Python code.
    yield_value: Box<dyn Fn(&Value) -> bool>,
}

impl TreeYielder {
    /// Creates a new `TreeYielder`.
    ///
    /// `yield_value` should be a lambda/function that takes a reference to a value
    /// and returns `true`/`false`.
    pub fn new<F>(yield_value: F) -> Self
    where
        F: Fn(&Value) -> bool + 'static,
    {
        Self {
            yield_value: Box::new(yield_value),
        }
    }

    /// Traverses the JSON Value tree looking for subValues that meet the
    /// criteria defined by `yield_value`.
    ///
    /// `memo` is handled internally using pointers to the heap data
    /// to prevent cycles (though standard JSON is acyclic, this preserves
    /// the logic of the original Python class).
    #[must_use]
    pub fn find_all<'a>(&'a self, obj: &'a Value) -> Vec<&'a Value> {
        let mut results = Vec::new();
        // In Python, `id()` is used. Here we use the raw memory address of the Value.
        let mut memo: HashSet<*const Value> = HashSet::new();

        // Python's `stackVals` tracked the path for `currentLevel`.
        // In Rust, we can pass path context down if needed, but strictly
        // for yielding values, it isn't required.
        // We skip the explicit path stack to keep the Rust idiomatic.

        self.find_all_values(obj, &mut results, &mut memo);
        results
    }

    #[must_use]
    pub fn find_first<'a>(&'a self, obj: &'a Value) -> Option<&'a Value> {
        let mut memo: HashSet<*const Value> = HashSet::new();
        self.find_first_value(obj, &mut memo)
    }

    fn find_first_value<'a>(
        &'a self,
        node: &'a Value,
        memo: &mut HashSet<*const Value>,
    ) -> Option<&'a Value> {
        let node_id = std::ptr::from_ref::<Value>(node);

        if !memo.insert(node_id) {
            return None;
        }

        if (self.yield_value)(node) {
            return Some(node);
        }

        match node {
            Value::Object(map) => {
                for (_key, val) in map {
                    if let Some(x) = self.find_first_value(val, memo) {
                        return Some(x);
                    }
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    if let Some(x) = self.find_first_value(val, memo) {
                        return Some(x);
                    }
                }
            }
            _ => return None,
        }

        None
    }

    fn find_all_values<'a>(
        &'a self,
        node: &'a Value,
        results: &mut Vec<&'a Value>,
        memo: &mut HashSet<*const Value>,
    ) {
        let node_id = std::ptr::from_ref::<Value>(node);

        // Check memo to prevent infinite recursion in cyclic graphs
        if !memo.insert(node_id) {
            // Corresponds to: `if id(obj) in self.memo: return`
            return;
        }

        // Check if this node matches the yield condition
        if (self.yield_value)(node) {
            results.push(node);
        }

        match node {
            // Handles non-iterables: Null, Bool, Number, String
            // Corresponds to: `if isinstance(obj, self.nonIterables) or obj is None: pass`
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
                // Do nothing
            }

            // Handles dictionaries (Objects in JSON)
            // Corresponds to: `elif isinstance(obj, dict):`
            Value::Object(map) => {
                for (_key, val) in map {
                    // In Python, stackVals logic goes here.
                    self.find_all_values(val, results, memo);
                    // Python pops stackVals here.
                }
            }

            // Handles lists and tuples (Arrays in JSON)
            // Corresponds to: `elif isinstance(obj, (list, tuple)):`
            Value::Array(arr) => {
                for val in arr {
                    // In Python, stackVals logic goes here.
                    self.find_all_values(val, results, memo);
                    // Python pops stackVals here.
                }
            }
        }
    }
}

// Example Usage
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_tree_yielder_find_all() {
        // Create a nested JSON object
        let data = json!({
            "name": "Root",
            "value": 100,
            "child": {
                "name": "Child",
                "active": true,
                "items": [1, 2, 3]
            }
        });

        // Find all string values
        let yielder = TreeYielder::new(serde_json::Value::is_string);
        let strings = yielder.find_all(&data);

        assert_eq!(strings.len(), 2);
        assert!(strings.contains(&&json!("Root"))); // Comparing &Value to &str works via serde_json partial eq
        assert!(strings.contains(&&json!("Child")));

        // Find all integers (strictly checking is_number, though JSON numbers are floats)
        let yielder = TreeYielder::new(serde_json::Value::is_i64);
        let numbers = yielder.find_all(&data);

        assert_eq!(numbers.len(), 4); // 100, 1, 2, 3
    }

    #[test]
    fn test_tree_yielder_find_first() {
        let data = json!({
            "name": "Root",
            "value": 100,
            "child": {
                "name": "Child",
                "active": true,
                "items": [1, 2, 3]
            }
        });

        let yielder = TreeYielder::new(serde_json::Value::is_string);
        let string = yielder.find_first(&data);

        assert_eq!(string, Some(&json!("Root")));
    }
}
