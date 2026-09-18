use std::collections::HashSet;

use serde_json::Value;

pub struct TreeYielder {
    yield_value: Box<dyn Fn(&Value) -> bool>,
}

impl TreeYielder {
    pub fn new<F>(yield_value: F) -> Self
    where
        F: Fn(&Value) -> bool + 'static,
    {
        Self {
            yield_value: Box::new(yield_value),
        }
    }

    /// Returns every value in the tree for which `yield_value` returns `true`.
    ///
    /// Traversal tracks visited nodes by pointer address, so cyclic structures cannot
    /// cause infinite recursion.
    #[must_use]
    pub fn find_all<'a>(&'a self, obj: &'a Value) -> Vec<&'a Value> {
        let mut results = Vec::new();
        let mut memo: HashSet<*const Value> = HashSet::new();

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
            return;
        }

        if (self.yield_value)(node) {
            results.push(node);
        }

        match node {
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}

            Value::Object(map) => {
                for (_key, val) in map {
                    self.find_all_values(val, results, memo);
                }
            }

            Value::Array(arr) => {
                for val in arr {
                    self.find_all_values(val, results, memo);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_tree_yielder_find_all() {
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
        let strings = yielder.find_all(&data);

        assert_eq!(strings.len(), 2);
        assert!(strings.contains(&&json!("Root")));
        assert!(strings.contains(&&json!("Child")));

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
