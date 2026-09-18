use std::{collections::HashMap, fmt::Debug, sync::Arc};

use blake3::Hash;

use crate::{
    Result, Value, ValueKind,
    message::{Path, PathSegment},
};

/// The doxsync document type.
///
/// Very cheap to clone.
#[derive(Clone)]
pub struct Document {
    /// The hash map of values in the document.
    hash_map: Arc<HashMap<Hash, (Path, Value)>>,

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
            hash_map: Self::build_hash_map(&value),
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
            hash_map: Self::build_hash_map(&value),
            value,
        })
    }

    pub(crate) fn hash_map(&self) -> Arc<HashMap<Hash, (Path, Value)>> {
        self.hash_map.clone()
    }

    fn build_hash_map(value: &Value) -> Arc<HashMap<Hash, (Path, Value)>> {
        let mut hash_map = HashMap::new();

        fn update_hash_map(
            hash_map: &mut HashMap<Hash, (Path, Value)>,
            path: &mut Path,
            value: &Value,
        ) {
            hash_map.insert(value.hash(), (path.clone(), value.clone()));

            match value.kind() {
                ValueKind::Array => {
                    let array = value.as_array().expect("value must be an array");
                    for (inedx, item) in array.iter().enumerate() {
                        path.push_segment(PathSegment::Index(inedx));
                        update_hash_map(hash_map, path, item);
                        path.pop_segment();
                    }
                }
                ValueKind::Map => {
                    let map = value.as_map().expect("value must be a map");
                    for (key, item) in map {
                        path.push_segment(PathSegment::Key(key.clone()));
                        update_hash_map(hash_map, path, item);
                        path.pop_segment();
                    }
                }
                _ => {}
            }
        }
        let mut tmp = Path::empty();
        update_hash_map(&mut hash_map, &mut tmp, &value);

        Arc::new(hash_map)
    }
}
