use std::{collections::BTreeMap, sync::Arc};

use crate::patch::Path;

use super::ConsumerStateTxn;

#[derive(Default)]
pub(crate) struct ConsumerState {
    pub(super) path_pool: BTreeMap<u32, Arc<Path>>,
    pub(super) string_pool: BTreeMap<u32, Arc<String>>,
}

impl ConsumerState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Moves the pools into a transaction without cloning their contents.
    pub(crate) fn txn(self) -> ConsumerStateTxn {
        ConsumerStateTxn::new(self)
    }
}
