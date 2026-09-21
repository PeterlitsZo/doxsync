use super::consts::*;
use crate::{Value, ValueInner, state::STRING_POOL_CAPACITY};

/// Bytes following the tag for an integer or a collection length.
pub(super) fn payload_width(value: u64, inline: u8) -> usize {
    if value <= inline as u64 {
        0
    } else if value <= u8::MAX as u64 {
        1
    } else if value <= u16::MAX as u64 {
        2
    } else if value <= u32::MAX as u64 {
        4
    } else {
        8
    }
}

pub(super) fn varuint_len(value: u64) -> usize {
    ((64 - value.leading_zeros()) as usize).max(1).div_ceil(7)
}

/// Encoded cost excluding child values, actions, paths, and pool patches.
/// Map keys use the maximum reference width allowed by the string pool.
pub(crate) fn value_own_cost(value: &Value) -> usize {
    match value.inner() {
        ValueInner::PosInt { inner } => 1 + payload_width(*inner, posint::INLINE),
        ValueInner::NegInt { inner } => 1 + payload_width(*inner, negint::INLINE),
        ValueInner::Null | ValueInner::Bool { .. } => 1,
        ValueInner::Float { .. } => 1 + size_of::<f64>(),
        ValueInner::BStr { inner } => {
            1 + payload_width(inner.len() as u64, bstr::INLINE) + inner.len()
        }
        ValueInner::TStr { inner } => {
            1 + payload_width(inner.len() as u64, tstr::INLINE) + inner.len()
        }
        ValueInner::Array { inner } => 1 + payload_width(inner.len() as u64, array::INLINE),
        ValueInner::Map { inner } => {
            1 + payload_width(inner.len() as u64, map::INLINE)
                + inner.len() * varuint_len((STRING_POOL_CAPACITY - 1) as u64)
        }
    }
}
