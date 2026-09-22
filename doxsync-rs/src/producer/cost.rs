use std::collections::HashMap;

use crate::{
    Error, ErrorKind, Result, Value, ValueInner,
    patch::{Action, Path},
    protocol::{
        PoolPreparation, PoolSavepoint,
        consts::DEFAULT_ACTIONS_LIMIT,
        layout::{action_tag, value_base_cost, varuint_len},
        visit_map_keys,
    },
    state::ProducerStateTxn,
};

/// Only state-independent subtree bytes are memoized. Key references are never cached.
#[derive(Default)]
struct ValueCostCache {
    costs: HashMap<blake3::Hash, usize>,
}

impl ValueCostCache {
    fn base_cost(&mut self, value: &Value) -> Result<usize> {
        if let Some(cost) = self.costs.get(&value.hash()) {
            return Ok(*cost);
        }
        let mut cost = value_base_cost(value);
        match value.inner() {
            ValueInner::Array { inner } => {
                for value in inner {
                    cost = add_len(cost, self.base_cost(value)?)?;
                }
            }
            ValueInner::Map { inner } => {
                for value in inner.values() {
                    cost = add_len(cost, self.base_cost(value)?)?;
                }
            }
            _ => {}
        }
        self.costs.insert(value.hash(), cost);
        Ok(cost)
    }
}

#[derive(Clone, Copy)]
#[must_use]
pub(super) struct CostSavepoint {
    pools: PoolSavepoint,
    actions: usize,
    body_bytes: usize,
}

/// Exact wire length for an incrementally selected operation prefix.
/// The caller owns commit/rollback of the borrowed transaction.
pub(super) struct CostSession<'s> {
    pools: PoolPreparation<'s>,
    values: ValueCostCache,
    actions: usize,
    body_bytes: usize,
}

impl<'s> CostSession<'s> {
    pub(super) fn new(txn: &'s mut ProducerStateTxn) -> Self {
        Self {
            pools: PoolPreparation::new(txn, DEFAULT_ACTIONS_LIMIT),
            values: ValueCostCache::default(),
            actions: 0,
            body_bytes: 0,
        }
    }

    pub(super) fn savepoint(&self) -> CostSavepoint {
        CostSavepoint {
            pools: self.pools.savepoint(),
            actions: self.actions,
            body_bytes: self.body_bytes,
        }
    }

    pub(super) fn rollback_to(&mut self, point: CostSavepoint) {
        self.pools.rollback_to(point.pools);
        self.actions = point.actions;
        self.body_bytes = point.body_bytes;
        // The value cache is independent of state and survives speculative branches.
    }

    /// Includes resource preparation and restores the entry savepoint on error.
    pub(super) fn append(&mut self, action: &Action) -> Result<()> {
        let point = self.savepoint();
        let result = self.append_inner(action);
        if result.is_err() {
            self.rollback_to(point);
        }
        result
    }

    fn append_inner(&mut self, action: &Action) -> Result<()> {
        self.pools.append(action)?;
        let mut cost = varuint_len(action_tag(action));
        match action {
            Action::Snapshot { value } => cost = add_len(cost, self.value_cost(value)?)?,
            Action::Add { path, value } | Action::Replace { path, value } => {
                cost = add_len(cost, self.path_cost(path))?;
                cost = add_len(cost, self.value_cost(value)?)?;
            }
            Action::Delete { path } => cost = add_len(cost, self.path_cost(path))?,
            Action::Copy { path, from } => {
                cost = add_len(cost, self.path_cost(path))?;
                cost = add_len(cost, self.path_cost(from))?;
            }
        }
        self.body_bytes = add_len(self.body_bytes, cost)?;
        self.actions += 1;
        // Keep encoded_len infallible even if an operation sequence is too large for usize.
        add_len(
            add_len(self.pools.metadata_len(), varuint_len(self.actions as u64))?,
            self.body_bytes,
        )?;
        Ok(())
    }

    fn path_cost(&self, path: &Path) -> usize {
        varuint_len(self.pools.path_key(path) as u64)
    }

    fn value_cost(&mut self, value: &Value) -> Result<usize> {
        let mut cost = self.values.base_cost(value)?;
        visit_map_keys(value, &mut |key| {
            cost = add_len(cost, varuint_len(self.pools.string_key(key) as u64))?;
            Ok(())
        })?;
        Ok(cost)
    }

    pub(super) fn encoded_len(&self) -> usize {
        self.pools.metadata_len() + varuint_len(self.actions as u64) + self.body_bytes
    }
}

fn add_len(left: usize, right: usize) -> Result<usize> {
    left.checked_add(right)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "encoded length too large"))
}
