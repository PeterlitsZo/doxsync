use std::{collections::BTreeMap, sync::Arc};

#[derive(Default)]
pub(crate) struct ConsumerState {
    string_pool: BTreeMap<u32, Arc<String>>,
}

impl ConsumerState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn apply_string_pool_patch(&mut self, patch: Vec<(u32, Arc<String>)>) {
        for (key, value) in patch {
            self.string_pool.insert(key, value);
        }
    }

    pub(crate) fn get_string(&self, key: u32) -> Option<&Arc<String>> {
        self.string_pool.get(&key)
    }
}
