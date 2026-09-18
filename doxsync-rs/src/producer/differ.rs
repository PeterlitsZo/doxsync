use crate::{
    Document, Message, Value, ValueKind,
    message::{Action, Path, PathSegment},
    state::ProducerStateTxn,
};

const COST_ADD: usize = 2;
const COST_DELETE: usize = 2;

pub(super) struct Differ<'s> {
    state_txn: &'s ProducerStateTxn,
}

impl<'s> Differ<'s> {
    pub(super) fn new(state_txn: &'s ProducerStateTxn) -> Self {
        Self { state_txn }
    }

    pub(super) fn diff(&self, old: &Document, new: &Document) -> Message {
        let internal = DifferInternal::new(self.state_txn, old, new);
        let diff_plan = internal.choose_best_diff_plan();
        Message::new(diff_plan.actions)
    }
}

struct DifferInternal<'s> {
    #[allow(dead_code)]
    state_txn: &'s ProducerStateTxn,
    old: &'s Document,
    new: &'s Document,
}

impl<'s> DifferInternal<'s> {
    fn new(state_txn: &'s ProducerStateTxn, old: &'s Document, new: &'s Document) -> Self {
        Self {
            state_txn,
            old,
            new,
        }
    }

    fn choose_best_diff_plan(&self) -> DiffPlan {
        let old_value = self.old.value();
        let new_value = self.new.value();

        let replace_diff_plan = self.replace_diff_plan();
        let mut plans = vec![replace_diff_plan];

        match (old_value.kind(), new_value.kind()) {
            (ValueKind::Map, ValueKind::Map) => {
                let map_diff_plan = self.map_diff_plan(&old_value, &new_value);
                plans.push(map_diff_plan);
            }
            _ => {}
        }

        plans.into_iter().min_by_key(|p| p.cost).unwrap()
    }

    fn replace_diff_plan(&self) -> DiffPlan {
        let new_value = self.new.value();
        let cost = new_value.cost();
        DiffPlan {
            actions: vec![Action::Snapshot { value: new_value }],
            cost,
        }
    }

    fn map_diff_plan(&self, old_value: &Value, new_value: &Value) -> DiffPlan {
        let old_map = old_value.as_map().expect("must be map");
        let new_map = new_value.as_map().expect("must be map");

        let mut actions = vec![];
        let mut cost = 0;
        for (key, value) in new_map.iter() {
            if let Some(old_value) = old_map.get(key) {
                if old_value != value {
                    cost += COST_ADD + value.cost();
                    actions.push(Action::Add {
                        path: Path::new(vec![PathSegment::key_arc(key.clone())]),
                        value: value.clone(),
                    });
                }
            } else {
                cost += COST_ADD + value.cost();
                actions.push(Action::Add {
                    path: Path::new(vec![PathSegment::key_arc(key.clone())]),
                    value: value.clone(),
                });
            }
        }
        for (key, _) in old_map.iter() {
            if !new_map.contains_key(key) {
                cost += COST_DELETE;
                actions.push(Action::Delete {
                    path: Path::new(vec![PathSegment::key_arc(key.clone())]),
                });
            }
        }

        DiffPlan { actions, cost }
    }
}

struct DiffPlan {
    actions: Vec<Action>,
    cost: usize,
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use crate::ProducerState;

    use super::*;

    fn map<const N: usize>(entries: [(&str, Value); N]) -> Value {
        Value::map(
            entries
                .into_iter()
                .map(|(key, value)| (Arc::new(key.to_owned()), value))
                .collect::<BTreeMap<_, _>>(),
        )
        .unwrap()
    }

    #[track_caller]
    fn assert_diff_plan(old: &Document, new: &Document, expected: Message) {
        let state = ProducerState::new();
        let state_txn = state.txn();
        let differ = Differ::new(&state_txn);
        let diff = differ.diff(old, new);
        assert_eq!(diff, expected);
    }

    #[test]
    #[rustfmt::skip]
    fn test_choose_best_diff_plan() {
        // Case 1:
        // =====================================================================
        let old = Document::new(Value::int(1).unwrap());
        let new = Document::new(Value::int(2).unwrap());
        assert_diff_plan(
            &old,
            &new,
            Message::new(vec![Action::Snapshot {
                value: Value::int(2).unwrap(),
            }]),
        );

        // Case 2: a sparse set of changes in a complex map is cheaper as
        // individual add and delete actions.
        // =====================================================================
        let stable = Value::tstr("stable-payload".repeat(8)).unwrap();
        let old_config = map([
            ("region", Value::tstr("us-east-1").unwrap()),
            ("replicas", Value::int(2).unwrap()),
        ]);
        let new_config = map([
            ("flags", Value::array(vec![Value::bool(true).unwrap(), Value::null().unwrap()]).unwrap()),
            ("region", Value::tstr("eu-west-1").unwrap()),
            ("replicas", Value::int(3).unwrap()),
        ]);
        let features = map([
            ("audit", Value::bool(true).unwrap()),
            ("search", Value::bool(false).unwrap()),
        ]);
        let old = Document::new(map([
            ("config", old_config),
            ("obsolete", Value::array(vec![Value::int(1).unwrap(), Value::int(2).unwrap()]).unwrap()),
            ("stable", stable.clone()),
            ("version", Value::int(1).unwrap()),
        ]));
        let new = Document::new(map([
            ("config", new_config.clone()),
            ("features", features.clone()),
            ("stable", stable),
            ("version", Value::int(1).unwrap()),
        ]));
        assert_diff_plan(
            &old,
            &new,
            Message::new(vec![
                Action::Add {
                    path: Path::new(vec![PathSegment::key("config")]),
                    value: new_config,
                },
                Action::Add {
                    path: Path::new(vec![PathSegment::key("features")]),
                    value: features,
                },
                Action::Delete {
                    path: Path::new(vec![PathSegment::key("obsolete")]),
                },
            ]),
        );

        // Case 3: replacing most short-key entries is cheaper as one snapshot.
        // =====================================================================
        let old = Document::new(map([
            ("a", Value::bool(false).unwrap()),
            ("b", Value::int(1).unwrap()),
            ("c", Value::tstr("old").unwrap()),
            ("d", Value::null().unwrap()),
        ]));
        let new_value = map([
            ("a", Value::bool(true).unwrap()),
            ("b", Value::float(2.0).unwrap()),
            ("e", Value::bool(false).unwrap()),
        ]);
        let new = Document::new(new_value.clone());
        assert_diff_plan(
            &old,
            &new,
            Message::new(vec![Action::Snapshot { value: new_value }]),
        );

        // Case 4: when snapshot and map diff costs tie, snapshot wins because
        // it is considered first.
        // =====================================================================
        let old = Document::new(map([("id", Value::bool(false).unwrap())]));
        let new_value = map([("id", Value::bool(true).unwrap())]);
        let new = Document::new(new_value.clone());
        assert_diff_plan(
            &old,
            &new,
            Message::new(vec![Action::Snapshot { value: new_value }]),
        );

        // Case 5: an unchanged complex map produces no actions.
        // =====================================================================
        let value = map([
            (
                "items",
                Value::array(vec![
                    map([("id", Value::int(1).unwrap())]),
                    map([("id", Value::int(2).unwrap())]),
                ])
                .unwrap(),
            ),
            (
                "metadata",
                map([
                    ("enabled", Value::bool(true).unwrap()),
                    ("owner", Value::tstr("doxsync").unwrap()),
                ]),
            ),
        ]);
        let old = Document::new(value.clone());
        let new = Document::new(value);
        assert_diff_plan(&old, &new, Message::new(vec![]));
    }
}
