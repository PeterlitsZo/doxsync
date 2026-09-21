use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

use blake3::Hash;

use crate::{Error, ErrorKind, Result};

mod macros;

const TAG_POSINT: u8 = 0x00;
const TAG_NEGINT: u8 = 0x10;
const TAG_BSTR: u8 = 0x20;
const TAG_TSTR: u8 = 0x30;
const TAG_ARRAY: u8 = 0x40;
const TAG_MAP: u8 = 0x50;
const TAG_FLOAT: u8 = 0x70;
const TAG_FALSE: u8 = TAG_FLOAT | 0x04;
const TAG_TRUE: u8 = TAG_FLOAT | 0x05;
const TAG_NULL: u8 = TAG_FLOAT | 0x06;

/// The doxsync value type.
///
/// Fast to compare, and cheap to clone.
#[derive(Clone)]
pub struct Value {
    /// The hash of the value.
    hash: Hash,
    /// The inner value.
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
    /// A null value.
    Null,
    /// A boolean value.
    Bool { inner: bool },
    /// A floating-point value.
    Float { inner: f64 },
    /// A binary string value.
    BStr { inner: Arc<Vec<u8>> },
    /// A text string value.
    TStr { inner: Arc<String> },
    /// An array value.
    Array { inner: Vec<Value> },
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

    /// Creates a null value.
    pub fn null() -> Result<Self> {
        Ok(Value::inner_null())
    }

    /// Creates a boolean value.
    pub fn bool(value: bool) -> Result<Self> {
        Ok(Value::inner_bool(value))
    }

    /// Creates a 64-bit floating-point value.
    ///
    /// All [`f64`] bit patterns are accepted, including infinities and NaNs.
    pub fn float(value: f64) -> Result<Self> {
        Ok(Value::inner_float(value))
    }

    /// Creates a binary string value.
    pub fn bstr<T>(value: T) -> Result<Self>
    where
        T: Into<Vec<u8>>,
    {
        Ok(Value::inner_bstr(value.into()))
    }

    /// Creates a UTF-8 text string value.
    pub fn tstr<T>(value: T) -> Result<Self>
    where
        T: Into<String>,
    {
        Ok(Value::inner_tstr(value.into()))
    }

    /// Creates an array value.
    pub fn array(value: Vec<Value>) -> Result<Self> {
        Ok(Value::inner_array(value))
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
            ValueInner::Null => ValueKind::Null,
            ValueInner::Bool { .. } => ValueKind::Bool,
            ValueInner::Float { .. } => ValueKind::Float,
            ValueInner::BStr { .. } => ValueKind::BStr,
            ValueInner::TStr { .. } => ValueKind::TStr,
            ValueInner::Array { .. } => ValueKind::Array,
            ValueInner::Map { .. } => ValueKind::Map,
        }
    }
}

impl Value {
    pub fn is_null(&self) -> bool {
        matches!(&*self.inner, ValueInner::Null)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match &*self.inner {
            ValueInner::Bool { inner } => Some(*inner),
            _ => None,
        }
    }

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

    pub fn as_bstr(&self) -> Option<&[u8]> {
        match &*self.inner {
            ValueInner::BStr { inner } => Some(inner.as_slice()),
            _ => None,
        }
    }

    pub fn as_tstr(&self) -> Option<&str> {
        match &*self.inner {
            ValueInner::TStr { inner } => Some(inner.as_str()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match &*self.inner {
            ValueInner::Array { inner } => Some(inner.as_slice()),
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

    pub fn as_bool_and_modify<F>(&self, f: F) -> Result<Self>
    where
        F: FnOnce(&mut bool) -> Result<()>,
    {
        if let ValueInner::Bool { inner } = &*self.inner {
            let mut inner = *inner;
            f(&mut inner)?;
            Value::bool(inner)
        } else {
            Err(Error::new(
                ErrorKind::UnexpectedType,
                "expected a bool value",
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

    pub fn as_bstr_and_modify<F>(&self, f: F) -> Result<Self>
    where
        F: FnOnce(&mut Vec<u8>) -> Result<()>,
    {
        if let ValueInner::BStr { inner } = &*self.inner {
            let mut inner = inner.as_ref().clone();
            f(&mut inner)?;
            Value::bstr(inner)
        } else {
            Err(Error::new(
                ErrorKind::UnexpectedType,
                "expected a bstr value",
            ))
        }
    }

    pub fn as_tstr_and_modify<F>(&self, f: F) -> Result<Self>
    where
        F: FnOnce(&mut String) -> Result<()>,
    {
        if let ValueInner::TStr { inner } = &*self.inner {
            let mut inner = inner.as_ref().clone();
            f(&mut inner)?;
            Value::tstr(inner)
        } else {
            Err(Error::new(
                ErrorKind::UnexpectedType,
                "expected a tstr value",
            ))
        }
    }

    pub fn as_array_and_modify<F>(&self, f: F) -> Result<Self>
    where
        F: FnOnce(&mut Vec<Value>) -> Result<()>,
    {
        if let ValueInner::Array { inner } = &*self.inner {
            let mut inner = inner.clone();
            f(&mut inner)?;
            Value::array(inner)
        } else {
            Err(Error::new(
                ErrorKind::UnexpectedType,
                "expected an array value",
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
    pub(crate) fn hash(&self) -> Hash {
        self.hash
    }

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

    pub(crate) fn inner_null() -> Self {
        let hash = blake3::hash(&[TAG_NULL]);
        Value {
            hash,
            inner: Arc::new(ValueInner::Null),
        }
    }

    pub(crate) fn inner_bool(inner: bool) -> Self {
        let hash = blake3::hash(&[if inner { TAG_TRUE } else { TAG_FALSE }]);
        Value {
            hash,
            inner: Arc::new(ValueInner::Bool { inner }),
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

    pub(crate) fn inner_bstr(inner: Vec<u8>) -> Self {
        let mut hash = blake3::Hasher::new();
        hash.update(&[TAG_BSTR]);
        hash.update(&inner);
        let hash = hash.finalize();
        Value {
            hash,
            inner: Arc::new(ValueInner::BStr {
                inner: Arc::new(inner),
            }),
        }
    }

    pub(crate) fn inner_tstr(inner: String) -> Self {
        let mut hash = blake3::Hasher::new();
        hash.update(&[TAG_TSTR]);
        hash.update(inner.as_bytes());
        let hash = hash.finalize();
        Value {
            hash,
            inner: Arc::new(ValueInner::TStr {
                inner: Arc::new(inner),
            }),
        }
    }

    pub(crate) fn inner_array(inner: Vec<Value>) -> Self {
        let mut hash = blake3::Hasher::new();

        hash.update(&[TAG_ARRAY]);
        for value in &inner {
            hash.update(value.hash.as_bytes());
        }
        let hash = hash.finalize();

        Value {
            hash,
            inner: Arc::new(ValueInner::Array { inner }),
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
            ValueInner::Null => write!(f, "Null"),
            ValueInner::Bool { inner } => write!(f, "Bool({})", inner),
            ValueInner::Float { inner } => write!(f, "Float({})", inner),
            ValueInner::BStr { inner } => write!(f, "BStr({:?})", inner),
            ValueInner::TStr { inner } => write!(f, "TStr({:?})", inner),
            ValueInner::Array { inner } => write!(f, "Array({:?})", inner),
            ValueInner::Map { inner } => write!(f, "Map({:?})", inner),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Int,
    Null,
    Bool,
    Float,
    BStr,
    TStr,
    Array,
    Map,
}
