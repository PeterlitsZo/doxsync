use std::{collections::HashMap, fmt::Debug, sync::Arc};

use blake3::Hash;

use crate::{
    Result, Value, ValueKind,
    message::{Path, PathSegment, value_own_cost},
};

pub(crate) struct IndexEntry {
    pub(crate) path: Path,
    pub(crate) value: Value,
    pub(crate) cost: usize,
}

/// The doxsync document type.
///
/// Very cheap to clone.
#[derive(Clone)]
pub struct Document {
    /// Nodes indexed by content hash, with one representative path per hash.
    index: Arc<HashMap<Hash, IndexEntry>>,

    /// The root value of the document.
    value: Value,
}

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Document({:?})", self.value)
    }
}

impl Document {
    pub fn new(value: Value) -> Self {
        Self {
            index: Self::build_index(&value, None),
            value,
        }
    }

    pub fn value(&self) -> Value {
        self.value.clone()
    }

    pub fn modify<F>(&self, f: F) -> Result<Self>
    where
        F: FnOnce(&mut Value) -> Result<()>,
    {
        let mut value = self.value.clone();
        f(&mut value)?;
        Ok(Self {
            index: Self::build_index(&value, Some(&self.index)),
            value,
        })
    }

    pub(crate) fn index(&self) -> Arc<HashMap<Hash, IndexEntry>> {
        self.index.clone()
    }

    fn build_index(
        value: &Value,
        prev_index: Option<&HashMap<Hash, IndexEntry>>,
    ) -> Arc<HashMap<Hash, IndexEntry>> {
        fn update_index(
            index: &mut HashMap<Hash, IndexEntry>,
            prev_index: Option<&HashMap<Hash, IndexEntry>>,
            path: &mut Path,
            value: &Value,
        ) -> usize {
            let hash = value.hash();
            let cached_cost = index
                .get(&hash)
                .or_else(|| prev_index.and_then(|previous| previous.get(&hash)))
                .map(|entry| entry.cost);
            let mut cost = cached_cost.unwrap_or_else(|| value_own_cost(value));

            // Even cached subtrees need fresh paths and entries for all
            // descendants.
            match value.kind() {
                ValueKind::Array => {
                    for (position, item) in value
                        .as_array()
                        .expect("value must be an array")
                        .iter()
                        .enumerate()
                    {
                        path.push_segment(PathSegment::Index(position));
                        let child_cost = update_index(index, prev_index, path, item);
                        if cached_cost.is_none() {
                            cost += child_cost;
                        }
                        path.pop_segment();
                    }
                }
                ValueKind::Map => {
                    for (key, item) in value.as_map().expect("value must be a map") {
                        path.push_segment(PathSegment::Key(key.clone()));
                        let child_cost = update_index(index, prev_index, path, item);
                        if cached_cost.is_none() {
                            cost += child_cost;
                        }
                        path.pop_segment();
                    }
                }
                _ => {}
            }
            index.insert(
                hash,
                IndexEntry {
                    path: path.clone(),
                    value: value.clone(),
                    cost,
                },
            );
            cost
        }

        let mut index = HashMap::new();
        update_index(&mut index, prev_index, &mut Path::empty(), value);
        Arc::new(index)
    }
}
