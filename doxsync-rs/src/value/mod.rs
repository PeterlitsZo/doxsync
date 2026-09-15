use std::{fmt::Debug, sync::Arc};

use blake3::Hash;

use crate::{Error, ErrorKind, Result};

const TAG_POSINT: u8 = 0;
const TAG_NEGINT: u8 = 1;

/// The doxsync value type.
///
/// Fast to compare, and cheap to clone.
#[derive(Clone)]
pub struct Value {
    hash: Hash,
    inner: Arc<ValueInner>,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

#[derive(Clone)]
pub(crate) enum ValueInner {
    /// A positive integer value.
    PosInt{ inner: u64 },
    /// A negative integer value.
    NegInt{ inner: u64 },
}

impl Value {
    pub fn int(value: i128) -> Result<Self> {
        if value >= 0 {
            if value >= (1 << 64) {
                return Err(Error::new(ErrorKind::InvalidData, "value too large"));
            }
            Ok(Value::inner_posint(value as u64))
        } else {
            if value < -(1 << 64) {
                return Err(Error::new(ErrorKind::InvalidData, "value too small"));
            }
            Ok(Value::inner_negint((-value - 1) as u64))
        }
    }
}

impl Value {
    pub(crate) fn inner(&self) -> &ValueInner {
        &self.inner
    }

    pub(crate) fn inner_posint(inner: u64) -> Self {
        let mut hash = blake3::Hasher::new();
        hash.update(&[TAG_POSINT]);
        hash.update(&inner.to_le_bytes());
        let hash = hash.finalize();
        Value {
            hash,
            inner: Arc::new(ValueInner::PosInt { inner }),
        }
    }

    pub(crate) fn inner_negint(inner: u64) -> Self {
        let mut hash = blake3::Hasher::new();
        hash.update(&[TAG_NEGINT]);
        hash.update(&inner.to_le_bytes());
        let hash = hash.finalize();
        Value {
            hash,
            inner: Arc::new(ValueInner::NegInt { inner }),
        }
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &*self.inner {
            ValueInner::PosInt { inner } => write!(f, "PosInt({})", inner),
            ValueInner::NegInt { inner } => write!(f, "NegInt({})", -(*inner as i128) - 1),
        }
    }
}
