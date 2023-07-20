use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use tokio::sync::watch;
use tracing::trace_span;

/// A map where values can be asynchronously be awaited.
pub struct AwaitableMap<K, V> {
    inner: Mutex<HashMap<K, watch::Sender<Option<V>>>>,
}

impl<K, V> AwaitableMap<K, V>
where
    K: Eq + Hash,
    V: Clone,
{
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Get the value for the key, waiting for it to be defined if necessary
    ///
    /// If `reset_first` returns true, the value in place for the key is first
    /// reset. Get then waits for a new value to be inserted for that key. It
    /// should not attempt to call `insert`, otherwise it would result in a
    /// deadlock.
    pub async fn get<F>(&self, key: K, reset_first: F) -> V
    where
        F: FnOnce(V) -> bool,
    {
        let mut rx = trace_span!("lock", src = "AwaitableMap.get").in_scope(|| {
            let mut guard = self.inner.lock().unwrap();
            if let Some(value_tx) = guard.get(&key) {
                let current_val = value_tx.subscribe().borrow().clone();
                if let Some(value) = current_val {
                    if reset_first(value) {
                        value_tx.send_replace(None);
                    }
                }
                value_tx.subscribe()
            } else {
                let (tx, rx) = watch::channel(None);
                guard.insert(key, tx);
                rx
            }
        });

        let value_ref = rx.wait_for(|val| val.is_some()).await.unwrap();
        value_ref.clone().unwrap()
    }

    /// Insert the key/value pair, and returns the existing value if any
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        trace_span!("lock", src = "AwaitableMap.insert").in_scope(|| {
            let mut guard = self.inner.lock().unwrap();
            if let Some(target_tx) = guard.get(&key) {
                target_tx.send_replace(Some(value))
            } else {
                let (tx, _rx) = watch::channel(Some(value));
                guard.insert(key, tx);
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AwaitableMap;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_awaitable_map_existing() {
        let map = AwaitableMap::new();

        // Test inserting a new key-value pair
        assert_eq!(map.insert(1, "first"), None);

        // Test inserting a new value for an existing key
        assert_eq!(map.insert(1, "second"), Some("first"));

        // Test getting a value that already exists in the map
        assert_eq!(map.get(1, |_| false).await, "second");
    }

    #[tokio::test]
    async fn test_awaitable_map_awaited() {
        let map = Arc::new(AwaitableMap::new());
        let map_ref = Arc::clone(&map);
        // Test setting a value and waiting for a new value to be inserted
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(map_ref.insert(1, "value"), None);
        });
        // `get` should wait for the new value to be set
        let get_fut = map.get(1, |_| {
            panic!("Should not be called because value uninitialized")
        });
        assert_eq!(get_fut.await, "value");
    }

    #[tokio::test]
    async fn test_awaitable_map_reset() {
        let map = Arc::new(AwaitableMap::new());
        // Insert the value that will be reset
        assert_eq!(map.insert(1, "first"), None);
        let map_ref = Arc::clone(&map);
        // Test resetting a value and waiting for a new value to be inserted
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            // This value is Expected to have been reset already
            assert_eq!(map_ref.insert(1, "second"), None);
        });
        let mut callback_called = false;
        let get_fut = map.get(1, |v| {
            assert_eq!(v, "first");
            callback_called = true;
            true
        });
        let timed_get_fut = tokio::time::timeout(Duration::from_millis(100), get_fut);
        assert_eq!(timed_get_fut.await.unwrap(), "second");
        assert!(callback_called, "callback wasn't called");
        task.await.unwrap();
    }

    #[tokio::test]
    async fn test_awaitable_map_multiple() {
        let map = Arc::new(AwaitableMap::new());
        // Insert the value that will be reset
        assert_eq!(map.insert(1, "first"), None);
        let mut callback_called = false;
        for _ in [0..5] {
            let get_fut = map.get(1, |v| {
                assert_eq!(v, "first");
                callback_called = true;
                false
            });
            let timed_get_fut = tokio::time::timeout(Duration::from_millis(100), get_fut);
            assert_eq!(timed_get_fut.await.unwrap(), "first");
            assert!(callback_called, "callback wasn't called");
        }
    }
}

impl<K, V> Default for AwaitableMap<K, V>
where
    K: Eq + Hash,
    V: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}
