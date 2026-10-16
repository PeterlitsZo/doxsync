use crate::Path;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub(crate) protocol: Option<u32>,
    pub(crate) strings: BTreeMap<u64, String>,
    pub(crate) paths: BTreeMap<u64, Path>,
    pub(crate) bytes: BTreeMap<u64, Vec<u8>>,
}
