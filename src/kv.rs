use std::collections::HashMap;
use crate::log::Command;

pub struct KvStore {
    map: HashMap<String, String>,
}

impl KvStore {
    /// Create an empty in-memory key-value store.
    pub fn new() -> Self {
        KvStore {
            map: HashMap::new(),
        }
    }

    /// Apply a replicated command to the store.
    pub fn apply(&mut self, cmd: &Command){
        match cmd {
            Command::Put(k, v) => {
                self.map.insert(k.clone(), v.clone());
            }
            Command::Delete(k) => {
                self.map.remove(k);
            }
        }
    }

    /// Read a value by key, cloning the stored string if present.
    pub fn get(&self, key: &str) -> Option<String> {
        self.map.get(key).cloned()
    }
}
