use std::fmt::Debug;

use crate::Value;

/// The doxsync document type.
#[derive(Clone)]
pub struct Document {
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
        Self { value }
    }

    pub fn value(&self) -> Value {
        self.value.clone()
    }
}
