use crate::{
    Document, Message, Result, Value, ValueKind,
    message::{Action, Path, PathSegment},
    state::ProducerStateTxn,
};

const COST_SNAPSHOT: usize = 1;
const COST_ADD: usize = 1 + 3;
const COST_REPLACE: usize = 1 + 3;
const COST_DELETE: usize = 1 + 3;
const COST_COPY: usize = 1 + 3 * 2;

pub(super) struct Differ<'s> {
    state_txn: &'s ProducerStateTxn,
}

impl<'s> Differ<'s> {
    pub(super) fn new(state_txn: &'s ProducerStateTxn) -> Self {
        Self { state_txn }
    }

    pub(super) fn diff(&self, old: &Document, new: &Document) -> Result<Message> {
        let internal = DifferInternal::new(self.state_txn, old, new);
        let diff_plan = internal.calaculate_diff_plan();
        Ok(diff_plan.message)
    }
}

struct DifferInternal<'s> {
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

    fn value_cost(&self, value: &Value) -> usize {
        self.new
            .index()
            .get(&value.hash())
            .expect("new value must be indexed")
            .cost
    }

    fn calaculate_diff_plan(&self) -> DiffPlan {
        let old_value = self.old.value();
        let new_value = self.new.value();

        self.common_diff_plan(Path::empty(), Some(&old_value), &new_value)
    }

    fn choose_best_diff_plan(&self, mut plans: Vec<DiffPlan>) -> DiffPlan {
        plans.sort_by_key(|plan| plan.cost);
        plans.first().expect("must have at least one plan").clone()
    }

    fn common_diff_plan(
        &self,
        path: Path,
        old_value: Option<&Value>,
        new_value: &Value,
    ) -> DiffPlan {
        let add_value_plan = || -> DiffPlan {
            let mut plans = vec![];

            // Just use action SNAPSHOT or ADD.
            if path.is_empty() {
                plans.push(DiffPlan {
                    message: Message::new(vec![Action::Snapshot {
                        value: new_value.clone(),
                    }]),
                    cost: COST_SNAPSHOT + self.value_cost(new_value),
                });
            } else {
                plans.push(DiffPlan {
                    message: Message::new(vec![Action::Add {
                        path: path.clone(),
                        value: new_value.clone(),
                    }]),
                    cost: COST_ADD + self.value_cost(new_value),
                });
            };

            // Use action COPY.
            if let Some(entry) = self.old.index().get(&new_value.hash()) {
                debug_assert_eq!(&entry.value, new_value);
                plans.push(DiffPlan {
                    message: Message::new(vec![Action::Copy {
                        path: path.clone(),
                        from: entry.path.clone(),
                    }]),
                    cost: COST_COPY,
                });
            }

            self.choose_best_diff_plan(plans)
        };

        let Some(old_value) = old_value else {
            return add_value_plan();
        };

        if old_value.kind() == ValueKind::Map && new_value.kind() == ValueKind::Map {
            return self.map_diff_plan(path, &old_value, &new_value);
        } else if old_value.kind() == ValueKind::Array && new_value.kind() == ValueKind::Array {
            return self.array_diff_plan(path, old_value, new_value);
        } else {
            return add_value_plan();
        }
    }

    fn array_diff_plan(&self, path: Path, old_value: &Value, new_value: &Value) -> DiffPlan {
        let old_array = old_value.as_array().expect("must be array");
        let new_array = new_value.as_array().expect("must be array");
        let mut actions = vec![];
        let mut cost = 0;

        for (index, new_value) in new_array.iter().enumerate() {
            let old_value = old_array.get(index);
            if old_value == Some(new_value) {
                continue;
            }
            let mut path = path.clone();
            path.push_segment(PathSegment::index(index));

            let plan = match old_value {
                Some(old_value)
                    if old_value.kind() != new_value.kind()
                        || !matches!(new_value.kind(), ValueKind::Map | ValueKind::Array) =>
                {
                    DiffPlan {
                        message: Message::new(vec![Action::Replace {
                            path,
                            value: new_value.clone(),
                        }]),
                        cost: COST_REPLACE + self.value_cost(new_value),
                    }
                }
                _ => self.common_diff_plan(path, old_value, new_value),
            };
            cost += plan.cost;
            actions.extend(plan.message.into_actions());
        }

        // Remove from the end so earlier indices stay valid.
        for index in (new_array.len()..old_array.len()).rev() {
            let mut path = path.clone();
            path.push_segment(PathSegment::index(index));
            cost += COST_DELETE;
            actions.push(Action::Delete { path });
        }

        DiffPlan {
            message: Message::new(actions),
            cost,
        }
    }

    fn map_diff_plan(&self, path: Path, old_value: &Value, new_value: &Value) -> DiffPlan {
        let old_map = old_value.as_map().expect("must be map");
        let new_map = new_value.as_map().expect("must be map");

        let mut actions = vec![];
        let mut cost = 0;
        for (key, new_value) in new_map.iter() {
            let old_value = old_map.get(key);
            if old_value != Some(new_value) {
                let mut path = path.clone();
                path.push_segment(PathSegment::key_arc(key.clone()));

                let plan = self.common_diff_plan(path.clone(), old_value, new_value);
                cost += plan.cost;
                actions.extend(plan.message.into_actions());
            }
        }
        for key in old_map.keys() {
            if !new_map.contains_key(key) {
                let mut path = path.clone();
                path.push_segment(PathSegment::key_arc(key.clone()));

                cost += COST_DELETE;
                actions.push(Action::Delete { path });
            }
        }

        DiffPlan {
            message: Message::new(actions),
            cost,
        }
    }
}

#[derive(Clone)]
struct DiffPlan {
    message: Message,
    cost: usize,
}

#[cfg(test)]
mod tests {
    use crate::{ProducerState, value};

    use super::*;

    #[track_caller]
    fn assert_diff_plan(old: &Document, new: &Document, expected: Message) {
        let state = ProducerState::new();
        let state_txn = state.txn();
        let differ = Differ::new(&state_txn);
        let diff = differ.diff(old, new).unwrap();
        assert_eq!(diff, expected);
    }

    #[test]
    fn test_choose_best_diff_plan() {
        // Case 1:
        // =====================================================================
        let old = Document::new(value!(1).unwrap());
        let new = Document::new(value!(2).unwrap());
        assert_diff_plan(
            &old,
            &new,
            Message::new(vec![Action::Snapshot {
                value: value!(2).unwrap(),
            }]),
        );

        // Case 2: a sparse set of changes in a complex map is cheaper as
        // individual add and delete actions.
        // =====================================================================
        let old = Document::new(
            value!({
                "config": {
                    "region": "us-east-1",
                    "replicas": 2,
                },
                "obsolete": [1, 2],
                "stable": "stable-payload".repeat(8),
                "version": 1,
            })
            .unwrap(),
        );
        let new = Document::new(
            value!({
                "config": {
                    "flags": [true, null],
                    "region": "eu-west-1",
                    "replicas": 3,
                },
                "features": {
                    "audit": true,
                    "search": false,
                },
                "stable": "stable-payload".repeat(8),
                "version": 1,
            })
            .unwrap(),
        );
        assert_diff_plan(
            &old,
            &new,
            Message::new(vec![
                Action::Add {
                    path: Path::parse("config.flags").unwrap(),
                    value: value!([true, null]).unwrap(),
                },
                Action::Add {
                    path: Path::parse("config.region").unwrap(),
                    value: value!("eu-west-1").unwrap(),
                },
                Action::Add {
                    path: Path::parse("config.replicas").unwrap(),
                    value: value!(3).unwrap(),
                },
                Action::Add {
                    path: Path::parse("features").unwrap(),
                    value: value!({
                        "audit": true,
                        "search": false,
                    })
                    .unwrap(),
                },
                Action::Delete {
                    path: Path::parse("obsolete").unwrap(),
                },
            ]),
        );

        // Case 3: replacing most short-key entries is cheaper as one snapshot.
        // =====================================================================
        let old = Document::new(
            value!({
                "a": false,
                "b": 1,
                "c": "old",
                "d": null,
            })
            .unwrap(),
        );
        let new = Document::new(
            value!({
                "a": true,
                "b": 2.0,
                "e": false,
            })
            .unwrap(),
        );
        assert_diff_plan(
            &old,
            &new,
            Message::new(vec![
                Action::add(Path::parse("a").unwrap(), value!(true).unwrap()),
                Action::add(Path::parse("b").unwrap(), value!(2.0).unwrap()),
                Action::add(Path::parse("e").unwrap(), value!(false).unwrap()),
                Action::delete(Path::parse("c").unwrap()),
                Action::delete(Path::parse("d").unwrap()),
            ]),
        );

        // Case 4: when snapshot and map diff costs tie, snapshot wins because
        // it is considered first.
        // =====================================================================
        let old = Document::new(value!({ "id": false }).unwrap());
        let new = Document::new(value!({ "id": true }).unwrap());
        assert_diff_plan(
            &old,
            &new,
            Message::new(vec![Action::add(
                Path::parse("id").unwrap(),
                value!(true).unwrap(),
            )]),
        );

        // Case 5: an unchanged complex map produces no actions.
        // =====================================================================
        let old = Document::new(
            value!({
                "items": [
                    { "id": 1 },
                    { "id": 2 },
                ],
                "metadata": {
                    "enabled": true,
                    "owner": "doxsync",
                },
            })
            .unwrap(),
        );
        let new = Document::new(
            value!({
                "items": [
                    { "id": 1 },
                    { "id": 2 },
                ],
                "metadata": {
                    "enabled": true,
                    "owner": "doxsync",
                },
            })
            .unwrap(),
        );
        assert_diff_plan(&old, &new, Message::new(vec![]));

        // Case 6: Support copy from array.
        // =====================================================================
        let old = Document::new(
            value!({
                "a": [
                    { "id": 1 },
                    { "id": 2 },
                ]
            })
            .unwrap(),
        );
        let new = Document::new(
            value!({
                "a": [
                    { "id": 1 },
                    { "id": 2 },
                ],
                "b": { "id": 1 }
            })
            .unwrap(),
        );
        assert_diff_plan(
            &old,
            &new,
            Message::new(vec![Action::copy(
                Path::parse("b").unwrap(),
                Path::parse("a.0").unwrap(),
            )]),
        );
    }
}
