use crate::message::Path;
use std::{collections::BTreeMap, sync::Arc};

use super::{
    ProducerState,
    bitmap::Bitmap,
    lru_txn::{LruPutResult, LruSavepoint, LruTxn},
};

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum InsertStringResult {
    Existing { key: u32 },
    Inserted { key: u32 },
    Replaced { key: u32 },
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum InsertPathResult {
    Existing { key: u32 },
    Inserted { key: u32 },
    Replaced { key: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[must_use]
pub(crate) struct ProducerStateSavepoint {
    rollback_log_len: usize,
    string_pool_lru: LruSavepoint,
    path_pool_lru: LruSavepoint,
}

pub(crate) struct ProducerStateTxn {
    pub(super) path_pool: BTreeMap<u32, Arc<Path>>,
    pub(super) path_pool_bitmap: Bitmap,
    pub(super) path_pool_reverse: BTreeMap<Arc<Path>, u32>,
    pub(super) path_pool_lru: LruTxn<Arc<Path>>,

    pub(super) string_pool: BTreeMap<u32, Arc<String>>,
    pub(super) string_pool_bitmap: Bitmap,
    pub(super) string_pool_reverse: BTreeMap<Arc<String>, u32>,
    pub(super) string_pool_lru: LruTxn<Arc<String>>,
    pub(super) rollback_log: Vec<StateRollbackEntry>,
}

impl ProducerStateTxn {
    pub(super) fn new(state: ProducerState) -> Self {
        let ProducerState {
            path_pool,
            path_pool_bitmap,
            path_pool_reverse,
            string_pool,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru,
            path_pool_lru,
        } = state;

        Self {
            path_pool,
            path_pool_bitmap,
            path_pool_reverse,
            string_pool,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru: string_pool_lru.txn(),
            path_pool_lru: path_pool_lru.txn(),
            rollback_log: Vec::new(),
        }
    }

    pub(crate) fn commit(self) -> ProducerState {
        let Self {
            path_pool,
            path_pool_bitmap,
            path_pool_reverse,
            string_pool,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru,
            path_pool_lru,
            rollback_log: _,
        } = self;

        ProducerState {
            path_pool,
            path_pool_bitmap,
            path_pool_reverse,
            string_pool,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru: string_pool_lru.commit(),
            path_pool_lru: path_pool_lru.commit(),
        }
    }

    pub(crate) fn rollback(mut self) -> ProducerState {
        self.rollback_all();

        let Self {
            path_pool,
            path_pool_bitmap,
            path_pool_reverse,
            string_pool,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru,
            path_pool_lru,
            rollback_log: _,
        } = self;

        ProducerState {
            path_pool,
            path_pool_bitmap,
            path_pool_reverse,
            string_pool,
            string_pool_bitmap,
            string_pool_reverse,
            string_pool_lru: string_pool_lru.rollback(),
            path_pool_lru: path_pool_lru.rollback(),
        }
    }

    pub(crate) fn get_path_key(&self, value: &Path) -> Option<u32> {
        self.path_pool_reverse.get(value).copied()
    }

    pub(crate) fn get_string_key(&self, value: &Arc<String>) -> Option<u32> {
        self.string_pool_reverse.get(value).copied()
    }

    pub(crate) fn savepoint(&self) -> ProducerStateSavepoint {
        ProducerStateSavepoint {
            rollback_log_len: self.rollback_log.len(),
            string_pool_lru: self.string_pool_lru.savepoint(),
            path_pool_lru: self.path_pool_lru.savepoint(),
        }
    }

    pub(crate) fn rollback_to(&mut self, savepoint: ProducerStateSavepoint) {
        assert!(
            savepoint.rollback_log_len <= self.rollback_log.len(),
            "savepoint must not be ahead of the producer state transaction"
        );
        self.string_pool_lru
            .assert_valid_savepoint(savepoint.string_pool_lru);

        self.path_pool_lru
            .assert_valid_savepoint(savepoint.path_pool_lru);
        while self.rollback_log.len() > savepoint.rollback_log_len {
            let entry = self
                .rollback_log
                .pop()
                .expect("rollback log must contain an entry after the savepoint");
            entry.undo(self);
        }
        self.string_pool_lru.rollback_to(savepoint.string_pool_lru);
        self.path_pool_lru.rollback_to(savepoint.path_pool_lru);
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

    pub(crate) fn insert_path(&mut self, value: Arc<Path>) -> InsertPathResult {
        // Check if the path already exists in the pool.
        if let Some(key) = self.path_pool_reverse.get(&value).copied() {
            assert!(
                matches!(self.path_pool_lru.put(value), LruPutResult::Updated),
                "an existing path pool entry must also exist in the LRU"
            );
            return InsertPathResult::Existing { key };
        }

        // Check if we need to evict an existing path from the pool.
        let evicted = match self.path_pool_lru.put(value.clone()) {
            LruPutResult::Inserted => false,
            LruPutResult::Updated => {
                unreachable!("a new path pool entry must not already exist in the LRU")
            }
            LruPutResult::Evicted { key: evicted } => {
                let key = self
                    .path_pool_reverse
                    .remove(&evicted)
                    .expect("an evicted LRU path must exist in the path pool");
                let value = self
                    .path_pool
                    .remove(&key)
                    .expect("an evicted path pool key must exist");
                self.path_pool_bitmap.dealloc(key as usize);
                self.rollback_log
                    .push(StateRollbackEntry::PathPoolInsert { key, value });
                true
            }
        };

        // Do insert the path into the pool.
        let key = self
            .path_pool_bitmap
            .alloc()
            .expect("the path pool LRU must leave a free bitmap slot") as u32;
        self.path_pool.insert(key, value.clone());
        self.path_pool_reverse.insert(value, key);
        self.rollback_log
            .push(StateRollbackEntry::PathPoolRemove { key });
        if evicted {
            InsertPathResult::Replaced { key }
        } else {
            InsertPathResult::Inserted { key }
        }
    }

    fn rollback_all(&mut self) {
        while let Some(entry) = self.rollback_log.pop() {
            entry.undo(self);
        }
    }
}

pub(super) enum StateRollbackEntry {
    PathPoolInsert { key: u32, value: Arc<Path> },
    PathPoolRemove { key: u32 },
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
            Self::PathPoolInsert { key, value } => {
                txn.path_pool_bitmap.alloc_at(key as usize);
                txn.path_pool.insert(key, value.clone());
                txn.path_pool_reverse.insert(value, key);
            }
            Self::PathPoolRemove { key } => {
                txn.path_pool_bitmap.dealloc(key as usize);
                let value = txn.path_pool.remove(&key).expect("path pool key not found");
                txn.path_pool_reverse.remove(&value);
            }
        }
    }
}
