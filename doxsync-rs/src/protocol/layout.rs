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
/// Excluding child values, map keys, actions, paths, and pool patches.
pub(crate) fn value_base_cost(value: &Value) -> usize {
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
        ValueInner::Map { inner } => 1 + payload_width(inner.len() as u64, map::INLINE),
    }
}

/// Requires a path validated by pool preparation.
pub(crate) fn path_definition_len(path: &crate::patch::Path) -> usize {
    use crate::patch::PathSegment;

    varuint_len(path.segments().len() as u64)
        + path
            .segments()
            .iter()
            .map(|segment| match segment {
                PathSegment::Key(key) => varuint_len((key.len() as u64) << 2 | 0b00) + key.len(),
                PathSegment::Index(index) => varuint_len((*index as u64) << 2 | 0b01),
            })
            .sum::<usize>()
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
