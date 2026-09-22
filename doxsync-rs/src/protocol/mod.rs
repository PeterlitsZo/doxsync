//! Shared wire rules and pool preparation, independent of messages and diff planning.

pub(crate) mod consts;
pub(crate) mod layout;
mod preparation;

pub(crate) use preparation::{PoolPreparation, PoolSavepoint};

use std::sync::Arc;

use crate::{Result, Value, ValueInner};

/// Visits every map key occurrence in wire order, including nested values.
pub(crate) fn visit_map_keys(
    value: &Value,
    visit: &mut impl FnMut(&Arc<String>) -> Result<()>,
) -> Result<()> {
    match value.inner() {
        ValueInner::Array { inner } => {
            for value in inner {
                visit_map_keys(value, visit)?;
            }
        }
        ValueInner::Map { inner } => {
            for (key, value) in inner {
                visit(key)?;
                visit_map_keys(value, visit)?;
            }
        }
        _ => {}
    }
    Ok(())
}
