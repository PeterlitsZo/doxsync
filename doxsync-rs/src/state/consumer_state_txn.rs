use std::{collections::BTreeMap, sync::Arc};

use crate::patch::Path;

use super::ConsumerState;

/// Owns consumer pools until explicitly committed or rolled back.
#[must_use]
pub(crate) struct ConsumerStateTxn {
    string_pool: BTreeMap<u32, Arc<String>>,
    path_pool: BTreeMap<u32, Arc<Path>>,
    rollback_log: Vec<ConsumerStateRollbackEntry>,
}

/// Valid only in its originating transaction, until rolled back past this point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[must_use]
pub(crate) struct ConsumerStateSavepoint {
    rollback_log_len: usize,
}

impl ConsumerStateTxn {
    pub(super) fn new(state: ConsumerState) -> Self {
        let ConsumerState {
            string_pool,
            path_pool,
        } = state;
        Self {
            string_pool,
            path_pool,
            rollback_log: Vec::new(),
        }
    }

    pub(crate) fn get_string(&self, key: u32) -> Option<&Arc<String>> {
        self.string_pool.get(&key)
    }

    pub(crate) fn get_path(&self, key: u32) -> Option<&Arc<Path>> {
        self.path_pool.get(&key)
    }

    /// Applies a patch already validated by the decoder.
    pub(crate) fn apply_string_pool_patch(&mut self, patch: Vec<(u32, Arc<String>)>) {
        for (key, value) in patch {
            let previous = self.string_pool.insert(key, value);
            self.rollback_log
                .push(ConsumerStateRollbackEntry::StringPoolSet { key, previous });
        }
    }

    /// Applies a patch already validated by the decoder.
    pub(crate) fn apply_path_pool_patch(&mut self, patch: Vec<(u32, Arc<Path>)>) {
        for (key, value) in patch {
            let previous = self.path_pool.insert(key, value);
            self.rollback_log
                .push(ConsumerStateRollbackEntry::PathPoolSet { key, previous });
        }
    }

    pub(crate) fn savepoint(&self) -> ConsumerStateSavepoint {
        ConsumerStateSavepoint {
            rollback_log_len: self.rollback_log.len(),
        }
    }

    pub(crate) fn rollback_to(&mut self, savepoint: ConsumerStateSavepoint) {
        assert!(
            savepoint.rollback_log_len <= self.rollback_log.len(),
            "savepoint must not be ahead of the consumer state transaction"
        );
        while self.rollback_log.len() > savepoint.rollback_log_len {
            self.rollback_log
                .pop()
                .expect("rollback entry after savepoint")
                .undo(self);
        }
    }

    pub(crate) fn commit(self) -> ConsumerState {
        let Self {
            string_pool,
            path_pool,
            rollback_log: _,
        } = self;
        ConsumerState {
            string_pool,
            path_pool,
        }
    }

    pub(crate) fn rollback(mut self) -> ConsumerState {
        self.rollback_to(ConsumerStateSavepoint {
            rollback_log_len: 0,
        });
        self.commit()
    }
}

enum ConsumerStateRollbackEntry {
    StringPoolSet {
        key: u32,
        previous: Option<Arc<String>>,
    },
    PathPoolSet {
        key: u32,
        previous: Option<Arc<Path>>,
    },
}

impl ConsumerStateRollbackEntry {
    fn undo(self, txn: &mut ConsumerStateTxn) {
        match self {
            Self::StringPoolSet { key, previous } => match previous {
                Some(value) => {
                    txn.string_pool.insert(key, value);
                }
                None => {
                    txn.string_pool.remove(&key);
                }
            },
            Self::PathPoolSet { key, previous } => match previous {
                Some(value) => {
                    txn.path_pool.insert(key, value);
                }
                None => {
                    txn.path_pool.remove(&key);
                }
            },
        }
    }
}
