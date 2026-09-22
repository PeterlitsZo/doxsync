use std::collections::HashMap;

use crate::{
    Result, Value, ValueInner,
    patch::{Action, Path},
    protocol::{
        PoolPreparation, PoolSavepoint, StringUsage,
        consts::DEFAULT_ACTIONS_LIMIT,
        layout::{action_tag, tstr_encoded_len, value_base_cost, varuint_len},
        visit_strings,
    },
    state::ProducerStateTxn,
};

/// Only state-independent subtree bytes are memoized. String encodings are
/// never cached.
#[derive(Default)]
struct ValueCostCache {
    costs: HashMap<blake3::Hash, usize>,
}

impl ValueCostCache {
    fn base_cost(&mut self, value: &Value) -> usize {
        if let Some(cost) = self.costs.get(&value.hash()) {
            return *cost;
        }
        let mut cost = value_base_cost(value);
        match value.inner() {
            ValueInner::Array { inner } => {
                for value in inner {
                    cost += self.base_cost(value);
                }
            }
            ValueInner::Map { inner } => {
                for value in inner.values() {
                    cost += self.base_cost(value);
                }
            }
            _ => {}
        }
        self.costs.insert(value.hash(), cost);
        cost
    }
}

#[derive(Clone, Copy)]
#[must_use]
pub(super) struct CostSavepoint {
    pools: PoolSavepoint,
    actions: usize,
    body_bytes: usize,
}

/// Exact wire length for an incrementally selected operation prefix. The caller
/// owns commit/rollback of the borrowed transaction.
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
            Action::Snapshot { value } => cost += self.value_cost(value)?,
            Action::Add { path, value } | Action::Replace { path, value } => {
                cost += self.path_cost(path);
                cost += self.value_cost(value)?;
            }
            Action::Delete { path } => cost += self.path_cost(path),
            Action::Copy { path, from } => {
                cost += self.path_cost(path);
                cost += self.path_cost(from);
            }
        }
        self.body_bytes += cost;
        self.actions += 1;
        Ok(())
    }

    fn path_cost(&self, path: &Path) -> usize {
        varuint_len(self.pools.path_key(path) as u64)
    }

    fn value_cost(&mut self, value: &Value) -> Result<usize> {
        let mut cost = self.values.base_cost(value);
        visit_strings(value, &mut |string, usage| {
            let key = self.pools.string_key(string);
            cost += match usage {
                StringUsage::MapKey => match key {
                    Some(key) => varuint_len((key as u64) << 2 | 0b00),
                    None => varuint_len((string.len() as u64) << 2 | 0b01) + string.len(),
                },
                StringUsage::TStr => tstr_encoded_len(string.len(), key),
            };
            Ok(())
        })?;
        Ok(cost)
    }

    pub(super) fn encoded_len(&self) -> usize {
        self.pools.metadata_len() + varuint_len(self.actions as u64) + self.body_bytes
    }
}
