use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

use blake3::Hash;

use crate::{Error, ErrorKind, Result};

const TAG_POSINT: u8 = 0x00;
const TAG_NEGINT: u8 = 0x01;
const TAG_MAP: u8 = 0x05;
const TAG_FLOAT: u8 = 0x07;

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
    PosInt { inner: u64 },
    /// A negative integer value.
    NegInt { inner: u64 },
    /// A floating-point value.
    Float { inner: f64 },
    /// A map value.
    Map { inner: BTreeMap<Arc<String>, Value> },
}

impl Value {
    /// Creates an integer value.
    ///
    /// The supported range is `-2^64..=2^64 - 1`.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::InvalidData`] when `value` is outside the supported
    /// range.
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

    /// Creates a 64-bit floating-point value.
    ///
    /// All [`f64`] bit patterns are accepted, including infinities and NaNs.
    pub fn float(value: f64) -> Result<Self> {
        Ok(Value::inner_float(value))
    }

    /// Creates a map value with UTF-8 string keys.
    ///
    /// Entries retain the deterministic key ordering provided by [`BTreeMap`].
    pub fn map(value: BTreeMap<Arc<String>, Value>) -> Result<Self> {
        Ok(Value::inner_map(value))
    }
}

impl Value {
    pub fn kind(&self) -> ValueKind {
        match &*self.inner {
            ValueInner::PosInt { .. } | ValueInner::NegInt { .. } => ValueKind::Int,
            ValueInner::Float { .. } => ValueKind::Float,
            ValueInner::Map { .. } => ValueKind::Map,
        }
    }
}

impl Value {
    pub fn as_int(&self) -> Option<i128> {
        match &*self.inner {
            ValueInner::PosInt { inner } => Some(*inner as i128),
            ValueInner::NegInt { inner } => Some(-(*inner as i128) - 1),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match &*self.inner {
            ValueInner::Float { inner } => Some(*inner),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&BTreeMap<Arc<String>, Value>> {
        match &*self.inner {
            ValueInner::Map { inner } => Some(inner),
            _ => None,
        }
    }

    pub fn as_int_and_modify<F>(&self, f: F) -> Result<Self>
    where
        F: FnOnce(&mut i128) -> Result<()>,
    {
        if let ValueInner::PosInt { inner } = &*self.inner {
            let mut inner = *inner as i128;
            f(&mut inner)?;
            Value::int(inner)
        } else if let ValueInner::NegInt { inner } = &*self.inner {
            let mut inner = -(*inner as i128) - 1;
            f(&mut inner)?;
            Value::int(inner)
        } else {
            Err(Error::new(
                ErrorKind::UnexpectedType,
                "expected an int value",
            ))
        }
    }

    pub fn as_float_and_modify<F>(&self, f: F) -> Result<Self>
    where
        F: FnOnce(&mut f64) -> Result<()>,
    {
        if let ValueInner::Float { inner } = &*self.inner {
            let mut inner = *inner as f64;
            f(&mut inner)?;
            Value::float(inner)
        } else {
            Err(Error::new(
                ErrorKind::UnexpectedType,
                "expected a float value",
            ))
        }
    }

    pub fn as_map_and_modify<F>(&self, f: F) -> Result<Self>
    where
        F: FnOnce(&mut BTreeMap<Arc<String>, Value>) -> Result<()>,
    {
        if let ValueInner::Map { inner } = &*self.inner {
            // Note: because the key and value types are both `Arc<T>`, so
            // `clone()` is not too expensive...
            let mut inner = inner.clone();
            f(&mut inner)?;
            Ok(Value::inner_map(inner))
        } else {
            Err(Error::new(
                ErrorKind::UnexpectedType,
                "expected a map value",
            ))
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

    pub(crate) fn inner_float(inner: f64) -> Self {
        let mut hash = blake3::Hasher::new();
        hash.update(&[TAG_FLOAT]);
        hash.update(&inner.to_le_bytes());
        let hash = hash.finalize();
        Value {
            hash,
            inner: Arc::new(ValueInner::Float { inner }),
        }
    }

    pub(crate) fn inner_map(inner: BTreeMap<Arc<String>, Value>) -> Self {
        let mut hash = blake3::Hasher::new();
        hash.update(&[TAG_MAP]);
        for (key, value) in &inner {
            let key_bytes = key.as_bytes();
            hash.update(&(key_bytes.len() as u64).to_le_bytes());
            hash.update(key_bytes);
            hash.update(value.hash.as_bytes());
        }
        let hash = hash.finalize();
        Value {
            hash,
            inner: Arc::new(ValueInner::Map { inner }),
        }
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &*self.inner {
            ValueInner::PosInt { inner } => write!(f, "Int({})", inner),
            ValueInner::NegInt { inner } => write!(f, "Int({})", -(*inner as i128) - 1),
            ValueInner::Float { inner } => write!(f, "Float({})", inner),
            ValueInner::Map { inner } => write!(f, "Map({:?})", inner),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Int,
    Float,
    Map,
}
