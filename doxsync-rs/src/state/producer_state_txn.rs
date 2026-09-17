use std::{collections::BTreeMap, sync::Arc};

use super::{
    ProducerState,
    bitmap::Bitmap,
    lru_txn::{LruPutResult, LruTxn},
};

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum InsertStringResult {
    Existing { key: u32 },
    Inserted { key: u32 },
    Replaced { key: u32 },
}

pub(crate) struct ProducerStateTxn {
    pub(super) string_pool: BTreeMap<u32, Arc<String>>,
    pub(super) string_pool_size: usize,
    pub(super) string_pool_bitmap: Bitmap,
    pub(super) string_pool_reverse: BTreeMap<Arc<String>, u32>,
    pub(super) string_pool_lru: LruTxn<Arc<String>>,
    pub(super) rollback_log: Vec<StateRollbackEntry>,
}

impl ProducerStateTxn {
    pub(super) fn new(state: ProducerState) -> Self {
        let ProducerState {
            string_pool,
            string_pool_size,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru,
        } = state;

        Self {
            string_pool,
            string_pool_size,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru: string_pool_lru.txn(),
            rollback_log: Vec::new(),
        }
    }

    pub(crate) fn commit(self) -> ProducerState {
        let Self {
            string_pool,
            string_pool_size,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru,
            rollback_log: _,
        } = self;

        ProducerState {
            string_pool,
            string_pool_size,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru: string_pool_lru.commit(),
        }
    }

    pub(crate) fn rollback(mut self) -> ProducerState {
        self.rollback_all();

        let Self {
            string_pool,
            string_pool_size,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru,
            rollback_log: _,
        } = self;

        ProducerState {
            string_pool,
            string_pool_size,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru: string_pool_lru.rollback(),
        }
    }

    pub(crate) fn get_string_key(&self, value: &Arc<String>) -> Option<u32> {
        self.string_pool_reverse.get(value).copied()
    }

    pub(crate) fn insert_string(&mut self, value: Arc<String>) -> InsertStringResult {
        // Check if the string already exists in the pool.
        if let Some(key) = self.string_pool_reverse.get(&value).copied() {
            assert!(
                matches!(self.string_pool_lru.put(value), LruPutResult::Updated),
                "an existing string pool entry must also exist in the LRU"
            );
            return InsertStringResult::Existing { key };
        }

        // Check if we need to evict an existing string from the pool.
        let evicted = match self.string_pool_lru.put(value.clone()) {
            LruPutResult::Inserted => false,
            LruPutResult::Updated => {
                unreachable!("a new string pool entry must not already exist in the LRU")
            }
            LruPutResult::Evicted { key: evicted } => {
                let key = self
                    .string_pool_reverse
                    .remove(&evicted)
                    .expect("an evicted LRU string must exist in the string pool");
                let value = self
                    .string_pool
                    .remove(&key)
                    .expect("an evicted string pool key must exist");
                self.string_pool_bitmap.dealloc(key as usize);
                self.rollback_log
                    .push(StateRollbackEntry::StringPoolInsert { key, value });
                true
            }
        };

        // Do insert the string into the pool.
        let key = self
            .string_pool_bitmap
            .alloc()
            .expect("the string pool LRU must leave a free bitmap slot") as u32;
        self.string_pool.insert(key, value.clone());
        self.string_pool_reverse.insert(value, key);
        self.rollback_log
            .push(StateRollbackEntry::StringPoolRemove { key });
        if evicted {
            InsertStringResult::Replaced { key }
        } else {
            InsertStringResult::Inserted { key }
        }
    }

    fn rollback_all(&mut self) {
        while let Some(entry) = self.rollback_log.pop() {
            entry.undo(self);
        }
    }
}

pub(super) enum StateRollbackEntry {
    StringPoolInsert { key: u32, value: Arc<String> },
    StringPoolRemove { key: u32 },
}

impl StateRollbackEntry {
    fn undo(self, txn: &mut ProducerStateTxn) {
        match self {
            Self::StringPoolInsert { key, value } => {
                txn.string_pool_bitmap.alloc_at(key as usize);
                txn.string_pool.insert(key, value.clone());
                txn.string_pool_reverse.insert(value, key);
            }
            Self::StringPoolRemove { key } => {
                txn.string_pool_bitmap.dealloc(key as usize);
                let value = txn
                    .string_pool
                    .remove(&key)
                    .expect("string pool key not found");
                txn.string_pool_reverse.remove(&value);
            }
        }
    }
}
