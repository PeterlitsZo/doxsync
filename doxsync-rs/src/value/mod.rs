use std::fmt::Debug;

use crate::{Error, ErrorKind, Result};

/// The doxsync value type.
#[derive(Clone, PartialEq)]
pub struct Value {
    inner: ValueInner,
}

#[derive(Clone, PartialEq)]
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
        Value {
            inner: ValueInner::PosInt { inner },
        }
    }

    pub(crate) fn inner_negint(inner: u64) -> Self {
        Value {
            inner: ValueInner::NegInt { inner },
        }
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.inner {
            ValueInner::PosInt { inner } => write!(f, "PosInt({})", inner),
            ValueInner::NegInt { inner } => write!(f, "NegInt({})", -(inner as i128) - 1),
        }
    }
}
