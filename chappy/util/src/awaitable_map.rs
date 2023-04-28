use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use tokio::sync::watch;

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
    /// reset. Get then waits for a new value to be inserted for that key.
    ///
    /// TODO add timeout
    pub async fn get<F>(&self, key: K, reset_first: F) -> V
    where
        F: FnOnce(V) -> bool,
    {
        let mut rx = {
            let mut guard = self.inner.lock().unwrap();
            if let Some(value_tx) = guard.get(&key) {
                if let Some(value) = value_tx.subscribe().borrow().clone() {
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
        };

        let value_ref = rx.wait_for(|val| val.is_some()).await.unwrap();
        value_ref.clone().unwrap()
    }

    /// Insert the key/value pair, and returns the existing value if any
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let mut guard = self.inner.lock().unwrap();
        if let Some(target_tx) = guard.get(&key) {
            target_tx.send_replace(Some(value))
        } else {
            let (tx, _rx) = watch::channel(Some(value));
            guard.insert(key, tx);
            None
        }
    }
}
