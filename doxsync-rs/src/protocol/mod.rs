//! Protocol representation policy, shared wire rules, and pool preparation.

pub(crate) mod consts;
pub(crate) mod decimal;
pub(crate) mod float;
pub(crate) mod layout;
mod preparation;
mod projection;

pub(crate) use preparation::{PoolPreparation, PoolSavepoint, PreparedPathSegment};
pub(crate) use projection::{ProjectedDocument, ProjectedMessage, ProtocolProjection};

use std::sync::Arc;

use crate::{Result, Value, ValueInner};

#[derive(Clone, Copy)]
pub(crate) enum StringUsage {
    MapKey,
    TStr,
}

/// Visits every map key and TStr occurrence in wire order, including nested
/// values.
pub(crate) fn visit_strings(
    value: &Value,
    visit: &mut impl FnMut(&Arc<String>, StringUsage) -> Result<()>,
) -> Result<()> {
    match value.inner() {
        ValueInner::Array { inner } => {
            for value in inner {
                visit_strings(value, visit)?;
            }
        }
        ValueInner::Map { inner } => {
            for (key, value) in inner {
                visit(key, StringUsage::MapKey)?;
                visit_strings(value, visit)?;
            }
        }
        ValueInner::TStr { inner } => visit(inner, StringUsage::TStr)?,
        _ => {}
    }
    Ok(())
}

/// Visits every binary value occurrence, preserving array and map value order.
pub(crate) fn visit_bytes(
    value: &Value,
    visit: &mut impl FnMut(&Arc<Vec<u8>>) -> Result<()>,
) -> Result<()> {
    match value.inner() {
        ValueInner::BStr { inner } => visit(inner)?,
        ValueInner::Array { inner } => {
            for child in inner {
                visit_bytes(child, visit)?;
            }
        }
        ValueInner::Map { inner } => {
            for child in inner.values() {
                visit_bytes(child, visit)?;
            }
        }
        _ => {}
    }
    Ok(())
}
