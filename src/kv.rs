use std::collections::HashMap;
use crate::log::Command;

pub struct KvStore {
    map: HashMap<String, String>,
}

impl KvStore {
    pub fn new() -> Self {
        KvStore {
            map: HashMap::new(),
        }
    }

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

    pub fn get(&self, key: &str) -> Option<String> {
        self.map.get(key).cloned()
    }
}