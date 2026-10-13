//! Protocol 4 column plans. Pool preparation remains in logical row order, so
//! ordinary child payloads cancel exactly when comparing the two layouts.
use std::{collections::BTreeMap, sync::Arc};

use super::{
    consts::*,
    layout::{payload_width, value_base_cost, varuint_len},
};
use crate::{Error, ErrorKind, Result, Value, ValueInner};

pub(crate) const DENSE_VALUES: u8 = 0;
pub(crate) const SPARSE_VALUES: u8 = 1;
pub(crate) const DENSE_INT_DELTA: u8 = 2;
pub(crate) const DENSE_DECIMAL_DELTA: u8 = 3;
pub(crate) const ROWS_LIMIT: usize = 65_536;
pub(crate) const COLUMNS_LIMIT: usize = 4_096;
pub(crate) const ENTRIES_LIMIT: usize = 262_144;
// Bound schema analysis and bitmap construction even for disjoint row schemas.
pub(crate) const CELLS_LIMIT: usize = 1_048_576;
// Zero-column arrays have no per-row payload to bound their expansion.
pub(crate) const EMPTY_ROWS_LIMIT: usize = 256;

pub(crate) struct Column<'a> {
    pub(crate) key: &'a Arc<String>,
    pub(crate) entries: Vec<(usize, &'a Value)>,
    pub(crate) mode: u8,
}

pub(crate) struct Plan<'a> {
    pub(crate) columns: Vec<Column<'a>>,
    pub(crate) savings: usize,
}

pub(crate) fn key_len(key: &Arc<String>, slot: Option<u32>) -> usize {
    match slot {
        Some(slot) => varuint_len(u64::from(slot) << 2),
        None => varuint_len((key.len() as u64) << 2 | 1) + key.len(),
    }
}

/// All scratch storage is proportional to present entries, not the dense matrix.
/// No pool state is changed and no ordinary child value is serialized or copied.
pub(crate) fn plan<'a>(
    rows: &'a [Value],
    mut string_key: impl FnMut(&Arc<String>) -> Option<u32>,
) -> Option<Plan<'a>> {
    if rows.is_empty() || rows.len() > ROWS_LIMIT {
        return None;
    }
    let mut count = 0usize;
    let mut ordinary = 1 + payload_width(rows.len() as u64, array::INLINE);
    for row in rows {
        let ValueInner::Map { inner } = row.inner() else {
            return None;
        };
        count = count.checked_add(inner.len())?;
        if count > ENTRIES_LIMIT {
            return None;
        }
        ordinary += 1 + payload_width(inner.len() as u64, map::INLINE);
    }
    if count == 0 && rows.len() > EMPTY_ROWS_LIMIT {
        return None;
    }
    let mut columns: BTreeMap<&Arc<String>, Column<'a>> = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        let ValueInner::Map { inner } = row.inner() else {
            unreachable!()
        };
        for (key, value) in inner {
            ordinary = ordinary.checked_add(key_len(key, string_key(key)))?;
            let column = columns.entry(key).or_insert_with(|| Column {
                key,
                entries: Vec::new(),
                mode: DENSE_VALUES,
            });
            column.entries.push((index, value));
            if columns.len() > COLUMNS_LIMIT || rows.len().checked_mul(columns.len())? > CELLS_LIMIT
            {
                return None;
            }
        }
    }
    let mut compact = 1 + varuint_len(rows.len() as u64) + varuint_len(columns.len() as u64);
    let mut numeric_savings = 0;
    for column in columns.values_mut() {
        compact = compact.checked_add(key_len(column.key, string_key(column.key)) + 1)?;
        if column.entries.len() != rows.len() {
            column.mode = SPARSE_VALUES;
            compact = compact.checked_add(rows.len().div_ceil(8))?;
        } else if let Some((mode, saved)) = delta_choice(&column.entries) {
            column.mode = mode;
            numeric_savings += saved;
        }
    }
    let savings = ordinary
        .checked_add(numeric_savings)?
        .checked_sub(compact)?;
    (savings > 0).then(|| Plan {
        columns: columns.into_values().collect(),
        savings,
    })
}

fn delta_choice(entries: &[(usize, &Value)]) -> Option<(u8, usize)> {
    let first = entries.first()?.1;
    let (mode, scale, mut previous, mut compact) = match first.inner() {
        ValueInner::PosInt { .. } | ValueInner::NegInt { .. } => (
            DENSE_INT_DELTA,
            None,
            integer(first)?,
            value_base_cost(first, 4),
        ),
        ValueInner::Decimal { inner } => (
            DENSE_DECIMAL_DELTA,
            Some(inner.scale()),
            inner.mantissa(),
            1 + signed_len(inner.mantissa()),
        ),
        _ => return None,
    };
    let mut ordinary = value_base_cost(first, 4);
    for (_, value) in &entries[1..] {
        let next = match (scale, value.inner()) {
            (None, _) => integer(value)?,
            (Some(scale), ValueInner::Decimal { inner }) if scale == inner.scale() => {
                inner.mantissa()
            }
            _ => return None,
        };
        let delta = next.checked_sub(previous)?;
        // Integer deltas use the full signed i64 range. Decimal coefficients and
        // their differences use i128 with bounded 97-bit first / 98-bit delta ZigZag payloads.
        if mode == DENSE_INT_DELTA && i64::try_from(delta).is_err() {
            return None;
        }
        compact += signed_len(delta);
        ordinary += value_base_cost(value, 4);
        previous = next;
    }
    ordinary
        .checked_sub(compact)
        .filter(|saved| *saved > 0)
        .map(|saved| (mode, saved))
}

pub(crate) fn integer(value: &Value) -> Option<i128> {
    match value.inner() {
        ValueInner::PosInt { inner } => Some(i128::from(*inner)),
        ValueInner::NegInt { inner } => Some(-1 - i128::from(*inner)),
        _ => None,
    }
}

pub(crate) fn integer_value(value: i128) -> Result<Value> {
    if value >= 0 {
        u64::try_from(value)
            .map(Value::inner_posint)
            .map_err(|_| invalid("AOO integer overflow"))
    } else {
        u64::try_from(-1 - value)
            .map(Value::inner_negint)
            .map_err(|_| invalid("AOO integer overflow"))
    }
}

fn zigzag(value: i128) -> u128 {
    ((value as u128) << 1) ^ ((value >> 127) as u128)
}

fn signed_len(value: i128) -> usize {
    ((128 - zigzag(value).leading_zeros()) as usize)
        .max(1)
        .div_ceil(7)
}

pub(crate) fn encode_signed(bytes: &mut Vec<u8>, value: i128) {
    let mut value = zigzag(value);
    loop {
        let byte = (value & 127) as u8;
        value >>= 7;
        bytes.push(byte | if value == 0 { 0 } else { 128 });
        if value == 0 {
            break;
        }
    }
}

/// Strict, minimal unsigned LEB128 followed by ZigZag, bounded before shifting.
pub(crate) fn decode_signed(bytes: &mut &[u8], bits: usize) -> Result<i128> {
    let mut value = 0u128;
    for index in 0..bits.div_ceil(7) {
        let byte = take_byte(bytes)?;
        let payload = byte & 127;
        let available = (bits - index * 7).min(7);
        if u16::from(payload) >= (1u16 << available) {
            return Err(invalid("AOO delta overflow"));
        }
        value |= u128::from(payload) << (index * 7);
        if byte & 128 == 0 {
            if index > 0 && payload == 0 {
                return Err(invalid("non-minimal AOO delta"));
            }
            return Ok(((value >> 1) as i128) ^ -((value & 1) as i128));
        }
    }
    Err(invalid("AOO delta overflow"))
}

pub(crate) fn take_byte(bytes: &mut &[u8]) -> Result<u8> {
    let (&byte, rest) = bytes
        .split_first()
        .ok_or_else(|| invalid("truncated AOO"))?;
    *bytes = rest;
    Ok(byte)
}

pub(crate) fn invalid(message: &'static str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
