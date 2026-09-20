mod bitmap;
mod consumer_state;
mod consumer_state_txn;
mod lru;
mod lru_txn;
mod producer_state;
mod producer_state_txn;

pub(crate) use consumer_state::ConsumerState;
pub(crate) use consumer_state_txn::ConsumerStateTxn;
pub(crate) use producer_state::ProducerState;
pub(crate) use producer_state_txn::{InsertPathResult, InsertStringPoolResult, ProducerStateTxn};

use bitmap::Bitmap;
use lru::Lru;

pub(crate) const STRING_POOL_CAPACITY: usize = 4096;
pub(crate) const PATH_POOL_CAPACITY: usize = 4096;
pub(crate) const PATH_SEGMENTS_LIMIT: usize = 256;
pub(crate) const PATH_KEY_BYTES_LIMIT: usize = 64 * 1024;
pub(crate) const PATH_PATCH_BYTES_LIMIT: usize = 1024 * 1024;
