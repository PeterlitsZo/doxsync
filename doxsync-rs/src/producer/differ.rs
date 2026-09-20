use crate::{
    Document, ErrorKind, Message, Result, Value, ValueKind,
    message::{Action, Path, PathSegment},
    state::ProducerStateTxn,
};

const COST_ADD: usize = 2;
const COST_DELETE: usize = 2;
const COST_COPY: usize = 3;

pub(super) struct Differ<'s> {
    state_txn: &'s ProducerStateTxn,
}

impl<'s> Differ<'s> {
    pub(super) fn new(state_txn: &'s ProducerStateTxn) -> Self {
        Self { state_txn }
    }

    pub(super) fn diff(&self, old: &Document, new: &Document) -> Result<Message> {
        let internal = DifferInternal::new(self.state_txn, old, new);
        let diff_plan = internal.choose_best_diff_plan()?;
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

    fn choose_best_diff_plan(&self) -> Result<DiffPlan> {
        let old_value = self.old.value();
        let new_value = self.new.value();

        let replace_diff_plan = self.replace_diff_plan();
        let mut plans = vec![replace_diff_plan];

        if old_value.kind() == ValueKind::Map && new_value.kind() == ValueKind::Map {
            plans.push(self.map_diff_plan(&old_value, &new_value, false));
            plans.push(self.map_diff_plan(&old_value, &new_value, true));
        }

        // Stable ordering keeps snapshots first when estimated costs tie.
        // Validate in cost order without encoding or changing the pools.
        plans.sort_by_key(|plan| plan.cost);
        let mut rejected = None;
        for plan in plans {
            match plan.message.validate(self.state_txn) {
                Ok(()) => return Ok(plan),
                Err(error) if error.kind() == ErrorKind::InvalidData => rejected = Some(error),
                Err(error) => return Err(error),
            }
        }
        Err(rejected.expect("at least one candidate"))
    }

    fn replace_diff_plan(&self) -> DiffPlan {
        let new_value = self.new.value();
        let cost = new_value.cost();
        DiffPlan {
            message: Message::new(vec![Action::Snapshot { value: new_value }]),
            cost,
        }
    }

    fn map_diff_plan(&self, old_value: &Value, new_value: &Value, allow_copy: bool) -> DiffPlan {
        let old_map = old_value.as_map().expect("must be map");
        let new_map = new_value.as_map().expect("must be map");
        let copy_sources = allow_copy.then(|| self.old.hash_map());

        let mut actions = vec![];
        let mut cost = 0;
        for (key, value) in new_map.iter() {
            if old_map.get(key) != Some(value) {
                let path = Path::new(vec![PathSegment::key_arc(key.clone())]);
                if COST_COPY < COST_ADD + value.cost() {
                    if let Some((from, _)) = copy_sources
                        .as_ref()
                        .and_then(|sources| sources.get(&value.hash()))
                        .filter(|(from, _)| {
                            // The consumer currently supports only map source paths.
                            from.segments()
                                .iter()
                                .all(|segment| matches!(segment, PathSegment::Key(_)))
                        })
                    {
                        cost += COST_COPY;
                        actions.push(Action::Copy {
                            path,
                            from: from.clone(),
                        });
                        continue;
                    }
                }
                cost += COST_ADD + value.cost();
                actions.push(Action::Add {
                    path,
                    value: value.clone(),
                });
            }
        }
        for key in old_map.keys() {
            if !new_map.contains_key(key) {
                cost += COST_DELETE;
                actions.push(Action::Delete {
                    path: Path::new(vec![PathSegment::key_arc(key.clone())]),
                });
            }
        }

        DiffPlan {
            message: Message::new(actions),
            cost,
        }
    }
}

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
                    path: Path::new(vec![PathSegment::key("config")]),
                    value: value!({
                        "flags": [true, null],
                        "region": "eu-west-1",
                        "replicas": 3,
                    })
                    .unwrap(),
                },
                Action::Add {
                    path: Path::new(vec![PathSegment::key("features")]),
                    value: value!({
                        "audit": true,
                        "search": false,
                    })
                    .unwrap(),
                },
                Action::Delete {
                    path: Path::new(vec![PathSegment::key("obsolete")]),
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
            Message::new(vec![Action::Snapshot {
                value: value!({
                    "a": true,
                    "b": 2.0,
                    "e": false,
                })
                .unwrap(),
            }]),
        );

        // Case 4: when snapshot and map diff costs tie, snapshot wins because
        // it is considered first.
        // =====================================================================
        let old = Document::new(value!({ "id": false }).unwrap());
        let new = Document::new(value!({ "id": true }).unwrap());
        assert_diff_plan(
            &old,
            &new,
            Message::new(vec![Action::Snapshot {
                value: value!({ "id": true }).unwrap(),
            }]),
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
    }
}
