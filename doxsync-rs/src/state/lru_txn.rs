use super::lru::{Lru, LruEntry};

pub(super) enum LruPutResult<K> {
    Inserted,
    Updated,
    Evicted { key: K },
}

pub(super) struct LruTxn<K>
where
    K: Clone + Default + Ord,
{
    lru: Lru<K>,
    rollback_log: Vec<LruRollbackEntry<K>>,
}

impl<K> LruTxn<K>
where
    K: Clone + Default + Ord,
{
    pub(super) fn new(lru: Lru<K>) -> Self {
        Self {
            lru,
            rollback_log: Vec::new(),
        }
    }

    pub(super) fn put(&mut self, key: K) -> LruPutResult<K> {
        let result = if let Some(at) = self.lru.map.get(&key).copied() {
            let entry = self.remove_at(at);
            self.rollback_log.push(LruRollbackEntry::Put {
                key: entry.key,
                prev: entry.prev,
                next: entry.next,
                at,
            });
            LruPutResult::Updated
        } else if self.lru.map.len() == self.lru.cap {
            assert_ne!(self.lru.tail, usize::MAX, "a full LRU must have a tail");

            let at = self.lru.tail;
            let entry = self.remove_at(at);
            let evicted_key = entry.key.clone();
            self.rollback_log.push(LruRollbackEntry::Put {
                key: entry.key,
                prev: entry.prev,
                next: entry.next,
                at,
            });
            LruPutResult::Evicted { key: evicted_key }
        } else {
            LruPutResult::Inserted
        };

        let rollback_key = key.clone();
        self.insert_at_head(key);
        self.rollback_log
            .push(LruRollbackEntry::Remove { key: rollback_key });

        result
    }

    pub(super) fn commit(mut self) -> Lru<K> {
        self.rollback_log.clear();
        self.lru
    }

    pub(super) fn rollback(mut self) -> Lru<K> {
        self.rollback_all();
        self.lru
    }
}

impl<K> LruTxn<K>
where
    K: Clone + Default + Ord,
{
    fn rollback_all(&mut self) {
        while let Some(entry) = self.rollback_log.pop() {
            entry.undo(self);
        }
    }

    fn remove_at(&mut self, at: usize) -> LruEntry<K> {
        let entry = std::mem::take(
            self.lru
                .arena
                .get_mut(at)
                .expect("LRU entry index must be within the arena"),
        );

        let mapped_at = self
            .lru
            .map
            .remove(&entry.key)
            .expect("LRU entry must be present in the map");
        assert_eq!(mapped_at, at, "LRU map and arena index must agree");
        self.lru.bitmap.dealloc(at);

        if entry.prev == usize::MAX {
            self.lru.head = entry.next;
        } else {
            self.lru.arena[entry.prev].next = entry.next;
        }

        if entry.next == usize::MAX {
            self.lru.tail = entry.prev;
        } else {
            self.lru.arena[entry.next].prev = entry.prev;
        }

        entry
    }

    fn insert_at_head(&mut self, key: K) {
        let at = self
            .lru
            .bitmap
            .alloc()
            .expect("LRU must have a free slot before inserting an entry");
        let entry = LruEntry {
            key,
            prev: usize::MAX,
            next: self.lru.head,
        };

        self.insert_allocated_at(at, entry);
    }

    fn restore_at(&mut self, at: usize, entry: LruEntry<K>) {
        self.lru.bitmap.alloc_at(at);
        self.insert_allocated_at(at, entry);
    }

    fn insert_allocated_at(&mut self, at: usize, entry: LruEntry<K>) {
        assert!(
            at <= self.lru.arena.len(),
            "a new LRU arena slot must immediately follow the arena"
        );
        assert!(
            !self.lru.map.contains_key(&entry.key),
            "LRU key must not already be present"
        );

        let key = entry.key.clone();
        let prev = entry.prev;
        let next = entry.next;

        if at == self.lru.arena.len() {
            self.lru.arena.push(entry);
        } else {
            self.lru.arena[at] = entry;
        }

        let previous = self.lru.map.insert(key, at);
        debug_assert!(previous.is_none());

        if prev == usize::MAX {
            self.lru.head = at;
        } else {
            self.lru.arena[prev].next = at;
        }

        if next == usize::MAX {
            self.lru.tail = at;
        } else {
            self.lru.arena[next].prev = at;
        }
    }
}

enum LruRollbackEntry<K> {
    Remove {
        key: K,
    },
    Put {
        key: K,
        prev: usize,
        next: usize,
        at: usize,
    },
}

impl<K> LruRollbackEntry<K>
where
    K: Clone + Default + Ord,
{
    fn undo(self, txn: &mut LruTxn<K>) {
        match self {
            Self::Remove { key } => {
                let at = txn
                    .lru
                    .map
                    .get(&key)
                    .copied()
                    .expect("rollback key must be present in the LRU");
                txn.remove_at(at);
            }
            Self::Put {
                key,
                prev,
                next,
                at,
            } => {
                txn.restore_at(at, LruEntry { key, prev, next });
            }
        }
    }
}
