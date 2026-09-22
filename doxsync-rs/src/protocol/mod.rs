//! Shared wire rules and pool preparation, independent of messages and diff planning.

pub(crate) mod consts;
pub(crate) mod layout;
mod preparation;

pub(crate) use preparation::{PoolPreparation, PoolSavepoint};

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
