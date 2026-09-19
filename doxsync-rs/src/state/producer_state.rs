use std::{collections::BTreeMap, sync::Arc};

use crate::message::Path;
use crate::state::{Bitmap, Lru, ProducerStateTxn};

pub(crate) struct ProducerState {
    pub(super) path_pool: BTreeMap<u32, Arc<Path>>,
    pub(super) path_pool_bitmap: Bitmap,
    pub(super) path_pool_reverse: BTreeMap<Arc<Path>, u32>,
    pub(super) path_pool_lru: Lru<Arc<Path>>,

    pub(super) string_pool: BTreeMap<u32, Arc<String>>,
    pub(super) string_pool_bitmap: Bitmap,
    pub(super) string_pool_reverse: BTreeMap<Arc<String>, u32>,
    pub(super) string_pool_lru: Lru<Arc<String>>,
}

impl Default for ProducerState {
    fn default() -> Self {
        let default_string_pool_size = super::STRING_POOL_CAPACITY;
        let default_path_pool_size = super::PATH_POOL_CAPACITY;
        Self {
            path_pool: BTreeMap::new(),
            path_pool_bitmap: Bitmap::new(default_path_pool_size),
            path_pool_reverse: BTreeMap::new(),
            path_pool_lru: Lru::new(default_path_pool_size),
            string_pool: BTreeMap::new(),
            string_pool_bitmap: Bitmap::new(default_string_pool_size),
            string_pool_reverse: BTreeMap::new(),
            string_pool_lru: Lru::new(default_string_pool_size),
        }
    }
}

impl ProducerState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Begin a transaction.
    pub(crate) fn txn(self) -> ProducerStateTxn {
        ProducerStateTxn::new(self)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::state::{InsertStringResult, ProducerState};

    #[test]
    fn state_txn_commit_keeps_changes() {
        let state = ProducerState::new();

        let mut txn = state.txn();
        assert_eq!(
            txn.insert_string(Arc::new("tmp-0000".to_string())),
            InsertStringResult::Inserted { key: 0 }
        );
        assert_eq!(
            txn.insert_string(Arc::new("tmp-0001".to_string())),
            InsertStringResult::Inserted { key: 1 }
        );
        let state = txn.commit();

        let mut txn = state.txn();
        assert_eq!(
            txn.insert_string(Arc::new("tmp-0002".to_string())),
            InsertStringResult::Inserted { key: 2 }
        );
        assert_eq!(
            txn.insert_string(Arc::new("tmp-0000".to_string())),
            InsertStringResult::Existing { key: 0 }
        );
        let state = txn.commit();

        let mut txn = state.txn();
        assert_eq!(
            txn.insert_string(Arc::new("tmp-0002".to_string())),
            InsertStringResult::Existing { key: 2 }
        );
        assert_eq!(
            txn.insert_string(Arc::new("tmp-0003".to_string())),
            InsertStringResult::Inserted { key: 3 }
        );
        let state = txn.rollback();

        let mut txn = state.txn();
        assert_eq!(
            txn.insert_string(Arc::new("tmp-0004".to_string())),
            InsertStringResult::Inserted { key: 3 }
        );
        let state = txn.commit();

        assert_eq!(
            state.string_pool_lru.recent_keys(),
            &[
                &Arc::new("tmp-0004".to_string()),
                &Arc::new("tmp-0000".to_string()),
                &Arc::new("tmp-0002".to_string()),
                &Arc::new("tmp-0001".to_string()),
            ]
        );

        let mut txn = state.txn();
        assert_eq!(
            txn.insert_string(Arc::new("tmp-0003".to_string())),
            InsertStringResult::Inserted { key: 4 }
        );
        for i in 5..4096 {
            let key = Arc::new(format!("tmp-{:04}", i));
            assert_eq!(
                txn.insert_string(key),
                InsertStringResult::Inserted { key: i }
            );
        }
        let state = txn.commit();

        let mut txn = state.txn();
        // If we try to insert existing string, it should reuse the existing
        // key.
        assert_eq!(
            txn.insert_string(Arc::new("tmp-0004".to_string())),
            InsertStringResult::Existing { key: 3 }
        );
        // But if we insert a new string, it should evict the least recently
        // used key.
        assert_eq!(
            txn.insert_string(Arc::new("tmp-4096".to_string())),
            InsertStringResult::Replaced { key: 1 }
        );
        let state = txn.commit();

        let recent_keys = state.string_pool_lru.recent_keys();
        assert_eq!(
            &recent_keys[..4],
            &[
                &Arc::new("tmp-4096".to_string()),
                &Arc::new("tmp-0004".to_string()),
                &Arc::new("tmp-4095".to_string()),
                &Arc::new("tmp-4094".to_string()),
            ]
        );
        assert_eq!(
            &recent_keys[recent_keys.len() - 4..],
            &[
                &Arc::new("tmp-0005".to_string()),
                &Arc::new("tmp-0003".to_string()),
                &Arc::new("tmp-0000".to_string()),
                &Arc::new("tmp-0002".to_string()),
            ]
        );

        let mut txn = state.txn();
        assert_eq!(
            txn.insert_string(Arc::new("tmp-4097".to_string())),
            InsertStringResult::Replaced { key: 2 }
        );
        assert_eq!(
            txn.insert_string(Arc::new("tmp-4098".to_string())),
            InsertStringResult::Replaced { key: 0 }
        );
        assert_eq!(
            txn.insert_string(Arc::new("tmp-4099".to_string())),
            InsertStringResult::Replaced { key: 4 }
        );
        let _state = txn.commit();
    }
}
