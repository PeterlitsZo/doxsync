use super::consts::*;
use crate::{Value, ValueInner};

/// Bytes following the tag for an integer or a collection length.
pub(crate) fn payload_width(value: u64, inline: u8) -> usize {
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

pub(crate) fn varuint_len(value: u64) -> usize {
    ((64 - value.leading_zeros()) as usize).max(1).div_ceil(7)
}

/// Encoded base cost.
///
/// Excluding child values, map keys, TStr encodings, actions, paths, and pool
/// patches.
pub(crate) fn value_base_cost(value: &Value) -> usize {
    match value.inner() {
        ValueInner::PosInt { inner } => 1 + payload_width(*inner, posint::INLINE),
        ValueInner::NegInt { inner } => 1 + payload_width(*inner, negint::INLINE),
        ValueInner::Null | ValueInner::Bool { .. } => 1,
        ValueInner::Float { .. } => 1 + size_of::<f64>(),
        ValueInner::BStr { inner } => {
            1 + payload_width(inner.len() as u64, bstr::INLINE) + inner.len()
        }
        ValueInner::TStr { .. } => 0,
        ValueInner::Array { inner } => 1 + payload_width(inner.len() as u64, array::INLINE),
        ValueInner::Map { inner } => 1 + payload_width(inner.len() as u64, map::INLINE),
    }
}

/// Use a pool reference only when it is strictly shorter than the literal.
pub(crate) fn tstr_ref_key(value_len: usize, key: Option<u32>) -> Option<u32> {
    let literal_len = 1 + payload_width(value_len as u64, tstr::INLINE) + value_len;
    key.filter(|key| 1 + payload_width(*key as u64, posint::INLINE) < literal_len)
}

pub(crate) fn tstr_encoded_len(value_len: usize, key: Option<u32>) -> usize {
    match tstr_ref_key(value_len, key) {
        Some(key) => 1 + payload_width(key as u64, posint::INLINE),
        None => 1 + payload_width(value_len as u64, tstr::INLINE) + value_len,
    }
}

/// Requires segments validated and frozen by pool preparation.
pub(crate) fn path_definition_len(path: &[super::PreparedPathSegment]) -> usize {
    use super::PreparedPathSegment;

    varuint_len(path.len() as u64)
        + path
            .iter()
            .map(|segment| match segment {
                PreparedPathSegment::Key(key) => varuint_len((key.len() as u64) << 2) + key.len(),
                PreparedPathSegment::KeyRef(key) => varuint_len((*key as u64) << 2 | 0b10),
                PreparedPathSegment::Index(index) => varuint_len((*index as u64) << 2 | 0b01),
            })
            .sum::<usize>()
}

/// Use a path key reference only when it is strictly shorter than the literal.
pub(crate) fn path_key_ref_key(value_len: usize, key: Option<u32>) -> Option<u32> {
    let literal_len = varuint_len((value_len as u64) << 2) + value_len;
    key.filter(|key| varuint_len((*key as u64) << 2 | 0b10) < literal_len)
}

pub(crate) fn action_tag(action: &crate::patch::Action) -> u64 {
    use crate::patch::Action;

    (match action {
        Action::Snapshot { .. } => ACTION_SNAPSHOT,
        Action::Add { .. } => ACTION_ADD,
        Action::Replace { .. } => ACTION_REPLACE,
        Action::Delete { .. } => ACTION_DELETE,
        Action::Copy { .. } => ACTION_COPY,
    }) as u64
}
