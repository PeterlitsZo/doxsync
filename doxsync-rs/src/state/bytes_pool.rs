//! Bounded binary payload storage. Message preparation owns admission and pins.

use std::{collections::BTreeMap, sync::Arc};

use super::{
    BYTES_POOL_CAPACITY, Bitmap, Lru,
    lru_txn::{LruPutResult, LruSavepoint, LruTxn},
};

pub(super) struct BytesPool {
    values: BTreeMap<u32, Arc<Vec<u8>>>,
    reverse: BTreeMap<Arc<Vec<u8>>, u32>,
    slots: Bitmap,
    lru: Lru<u32>,
}

impl Default for BytesPool {
    fn default() -> Self {
        Self {
            values: BTreeMap::new(),
            reverse: BTreeMap::new(),
            slots: Bitmap::new(BYTES_POOL_CAPACITY),
            lru: Lru::new(BYTES_POOL_CAPACITY),
        }
    }
}

impl BytesPool {
    pub(super) fn txn(self) -> BytesPoolTxn {
        BytesPoolTxn {
            values: self.values,
            reverse: self.reverse,
            slots: self.slots,
            lru: self.lru.txn(),
            undo: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BytesPoolSavepoint {
    undo_len: usize,
    lru: LruSavepoint,
}

pub(crate) struct BytesPoolTxn {
    values: BTreeMap<u32, Arc<Vec<u8>>>,
    reverse: BTreeMap<Arc<Vec<u8>>, u32>,
    slots: Bitmap,
    lru: LruTxn<u32>,
    undo: Vec<(u32, Option<Arc<Vec<u8>>>)>,
}

impl BytesPoolTxn {
    pub(crate) fn key(&self, value: &Arc<Vec<u8>>) -> Option<u32> {
        self.reverse.get(value).copied()
    }

    /// Read-only selection lets preparation check the exact definition budget
    /// before changing persistent state. Pinned slots cannot be replaced.
    pub(crate) fn candidate(&self, pinned: &[bool; BYTES_POOL_CAPACITY]) -> Option<u32> {
        if self.values.len() < BYTES_POOL_CAPACITY {
            return (0..BYTES_POOL_CAPACITY as u32).find(|key| !self.values.contains_key(key));
        }
        self.lru
            .least_recent_where(|key| !pinned[*key as usize])
            .copied()
    }

    pub(crate) fn touch(&mut self, key: u32) {
        assert!(matches!(self.lru.put(key), LruPutResult::Updated));
    }

    /// The caller has selected an unpinned candidate for a previously unseen value.
    pub(crate) fn insert(&mut self, key: u32, value: &Arc<Vec<u8>>) {
        assert!(!self.reverse.contains_key(value));
        let previous = self.values.insert(key, value.clone());
        if let Some(previous) = &previous {
            self.reverse.remove(previous);
            self.touch(key);
        } else {
            let allocated = self.slots.alloc().expect("free bytes pool slot");
            assert_eq!(allocated, key as usize);
            assert!(matches!(self.lru.put(key), LruPutResult::Inserted));
        }
        self.reverse.insert(value.clone(), key);
        self.undo.push((key, previous));
    }

    pub(super) fn savepoint(&self) -> BytesPoolSavepoint {
        BytesPoolSavepoint {
            undo_len: self.undo.len(),
            lru: self.lru.savepoint(),
        }
    }

    fn undo_to(&mut self, len: usize) {
        assert!(len <= self.undo.len());
        while self.undo.len() > len {
            let (key, previous) = self.undo.pop().expect("bytes pool undo entry");
            let value = self.values.remove(&key).expect("bytes pool slot");
            self.reverse.remove(&value);
            if let Some(value) = previous {
                self.values.insert(key, value.clone());
                self.reverse.insert(value, key);
            } else {
                self.slots.dealloc(key as usize);
            }
        }
    }

    pub(super) fn rollback_to(&mut self, point: BytesPoolSavepoint) {
        self.lru.assert_valid_savepoint(point.lru);
        self.undo_to(point.undo_len);
        self.lru.rollback_to(point.lru);
    }

    pub(super) fn commit(self) -> BytesPool {
        BytesPool {
            values: self.values,
            reverse: self.reverse,
            slots: self.slots,
            lru: self.lru.commit(),
        }
    }

    pub(super) fn rollback(mut self) -> BytesPool {
        self.undo_to(0);
        BytesPool {
            values: self.values,
            reverse: self.reverse,
            slots: self.slots,
            lru: self.lru.rollback(),
        }
    }
}
