use std::{collections::HashMap, fmt::Debug, sync::Arc};

use blake3::Hash;

use crate::{
    Result, Value, ValueKind,
    patch::{Path, PathSegment},
};

pub(crate) struct IndexEntry {
    pub(crate) path: Path,
    pub(crate) value: Value,
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
            index: Self::build_index(&value),
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
            index: Self::build_index(&value),
            value,
        })
    }

    pub(crate) fn index(&self) -> Arc<HashMap<Hash, IndexEntry>> {
        self.index.clone()
    }

    fn build_index(value: &Value) -> Arc<HashMap<Hash, IndexEntry>> {
        fn update_index(index: &mut HashMap<Hash, IndexEntry>, path: &mut Path, value: &Value) {
            // Every occurrence needs a current path, even when values share a hash.
            match value.kind() {
                ValueKind::Array => {
                    for (position, item) in value
                        .as_array()
                        .expect("value must be an array")
                        .iter()
                        .enumerate()
                    {
                        path.push_segment(PathSegment::Index(position));
                        update_index(index, path, item);
                        path.pop_segment();
                    }
                }
                ValueKind::Map => {
                    for (key, item) in value.as_map().expect("value must be a map") {
                        path.push_segment(PathSegment::Key(key.clone()));
                        update_index(index, path, item);
                        path.pop_segment();
                    }
                }
                _ => {}
            }
            index.insert(
                value.hash(),
                IndexEntry {
                    path: path.clone(),
                    value: value.clone(),
                },
            );
        }

        let mut index = HashMap::new();
        update_index(&mut index, &mut Path::empty(), value);
        Arc::new(index)
    }
}
