use std::collections::BTreeMap;

use super::{bitmap::Bitmap, lru_txn::LruTxn};

pub(super) struct Lru<K> {
    pub(super) map: BTreeMap<K, usize>,
    pub(super) cap: usize,
    pub(super) arena: Vec<LruEntry<K>>,
    pub(super) bitmap: Bitmap,
    pub(super) head: usize,
    pub(super) tail: usize,
}

impl<K> Lru<K> {
    pub(super) fn new(cap: usize) -> Self {
        Self {
            map: BTreeMap::new(),
            cap,
            arena: vec![],
            bitmap: Bitmap::new(cap),
            head: usize::MAX,
            tail: usize::MAX,
        }
    }

    #[cfg(test)]
    pub(super) fn recent_keys(&self) -> Vec<&K> {
        let mut keys = Vec::with_capacity(self.map.len());
        let mut at = self.head;

        while at != usize::MAX {
            let entry = &self.arena[at];
            keys.push(&entry.key);
            at = entry.next;
        }

        keys
    }
}

impl<K> Lru<K>
where
    K: Clone + Default + Ord,
{
    pub(super) fn txn(self) -> LruTxn<K> {
        LruTxn::new(self)
    }
}

#[derive(Default)]
pub(super) struct LruEntry<K> {
    pub(super) key: K,
    pub(super) prev: usize,
    pub(super) next: usize,
}
