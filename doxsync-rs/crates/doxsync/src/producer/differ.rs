use crate::{
    Result, Value, ValueKind,
    patch::{Action, Path, PathSegment},
    protocol::ProjectedDocument,
};

use super::cost::CostSession;

pub(super) struct Differ<'d, 'c, 's> {
    old: &'d ProjectedDocument,
    cost: &'c mut CostSession<'s>,
}

impl<'d, 'c, 's> Differ<'d, 'c, 's> {
    pub(super) fn new(old: &'d ProjectedDocument, cost: &'c mut CostSession<'s>) -> Self {
        Self { old, cost }
    }

    pub(super) fn diff(&mut self, new: &ProjectedDocument) -> Result<Vec<Action>> {
        self.diff_value(
            Path::empty(),
            Some(&self.old.document().value()),
            &new.document().value(),
        )
    }

    /// Appends the cheaper encodable action, preferring the direct action on ties.
    fn choose_action(&mut self, direct: Action, copy: Option<Action>) -> Result<Action> {
        let Some(copy) = copy else {
            self.cost.append(&direct)?;
            return Ok(direct);
        };

        // Compare both actions after the same prefix, then append only the winner.
        let point = self.cost.savepoint();
        let direct_cost = self.cost.append(&direct).map(|()| self.cost.encoded_len());
        self.cost.rollback_to(point);
        let copy_cost = self.cost.append(&copy).map(|()| self.cost.encoded_len());
        self.cost.rollback_to(point);

        let action = match (direct_cost, copy_cost) {
            (Ok(direct_len), Ok(copy_len)) if copy_len < direct_len => copy,
            (Ok(_), _) => direct,
            (Err(_), Ok(_)) => copy,
            (Err(error), Err(_)) => return Err(error),
        };
        self.cost.append(&action)?;
        Ok(action)
    }

    fn diff_value(
        &mut self,
        path: Path,
        old_value: Option<&Value>,
        new_value: &Value,
    ) -> Result<Vec<Action>> {
        if let Some(old_value) = old_value {
            if old_value.kind() == ValueKind::Map && new_value.kind() == ValueKind::Map {
                return self.diff_map(path, old_value, new_value);
            } else if old_value.kind() == ValueKind::Array && new_value.kind() == ValueKind::Array {
                return self.diff_array(path, old_value, new_value);
            }
        }

        let action = if path.is_empty() {
            Action::Snapshot {
                value: new_value.clone(),
            }
        } else {
            Action::Add {
                path: path.clone(),
                value: new_value.clone(),
            }
        };
        let copy = self
            .old
            .document()
            .index()
            .get(&new_value.hash())
            .map(|entry| {
                debug_assert_eq!(&entry.value, new_value);
                Action::Copy {
                    path,
                    from: entry.path.clone(),
                }
            });
        Ok(vec![self.choose_action(action, copy)?])
    }

    fn diff_array(
        &mut self,
        path: Path,
        old_value: &Value,
        new_value: &Value,
    ) -> Result<Vec<Action>> {
        let old_array = old_value.as_array().expect("must be array");
        let new_array = new_value.as_array().expect("must be array");
        let mut actions = vec![];

        for (index, new_value) in new_array.iter().enumerate() {
            let old_value = old_array.get(index);
            if old_value == Some(new_value) {
                continue;
            }
            let mut path = path.clone();
            path.push_segment(PathSegment::index(index));
            match old_value {
                Some(old_value)
                    if old_value.kind() != new_value.kind()
                        || !matches!(new_value.kind(), ValueKind::Map | ValueKind::Array) =>
                {
                    let action = Action::Replace {
                        path,
                        value: new_value.clone(),
                    };
                    self.cost.append(&action)?;
                    actions.push(action);
                }
                _ => actions.extend(self.diff_value(path, old_value, new_value)?),
            }
        }
        // Remove from the end so earlier indices stay valid.
        for index in (new_array.len()..old_array.len()).rev() {
            let mut path = path.clone();
            path.push_segment(PathSegment::index(index));
            let action = Action::Delete { path };
            self.cost.append(&action)?;
            actions.push(action);
        }
        Ok(actions)
    }

    fn diff_map(
        &mut self,
        path: Path,
        old_value: &Value,
        new_value: &Value,
    ) -> Result<Vec<Action>> {
        let old_map = old_value.as_map().expect("must be map");
        let new_map = new_value.as_map().expect("must be map");
        let mut actions = vec![];
        for (key, new_value) in new_map.iter() {
            let old_value = old_map.get(key);
            if old_value != Some(new_value) {
                let mut path = path.clone();
                path.push_segment(PathSegment::key_arc(key.clone()));
                actions.extend(self.diff_value(path, old_value, new_value)?);
            }
        }
        for key in old_map.keys() {
            if !new_map.contains_key(key) {
                let mut path = path.clone();
                path.push_segment(PathSegment::key_arc(key.clone()));
                let action = Action::Delete { path };
                self.cost.append(&action)?;
                actions.push(action);
            }
        }
        Ok(actions)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Document, ProducerState, protocol::ProtocolProjection, value};

    use super::*;

    #[track_caller]
    fn assert_diff_plan(old: &Document, new: &Document, expected: Vec<Action>) {
        let state = ProducerState::new();
        let mut state_txn = state.txn();
        let projection = ProtocolProjection::new(state_txn.protocol()).unwrap();
        let old = projection.document(old.clone());
        let new = projection.document(new.clone());
        let mut cost = CostSession::new(&mut state_txn);
        let mut differ = Differ::new(&old, &mut cost);
        let diff = differ.diff(&new).unwrap();
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
            vec![Action::Snapshot {
                value: value!(2).unwrap(),
            }],
        );

        // Case 2: changes in a complex map use individual add and delete actions.
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
            vec![
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
            ],
        );

        // Case 3: maps keep using recursive diffs when many entries change.
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
            vec![
                Action::add(Path::parse("a").unwrap(), value!(true).unwrap()),
                Action::add(Path::parse("b").unwrap(), value!(2.0).unwrap()),
                Action::add(Path::parse("e").unwrap(), value!(false).unwrap()),
                Action::delete(Path::parse("c").unwrap()),
                Action::delete(Path::parse("d").unwrap()),
            ],
        );

        // Case 4: a changed scalar map entry uses an add action.
        // =====================================================================
        let old = Document::new(value!({ "id": false }).unwrap());
        let new = Document::new(value!({ "id": true }).unwrap());
        assert_diff_plan(
            &old,
            &new,
            vec![Action::add(
                Path::parse("id").unwrap(),
                value!(true).unwrap(),
            )],
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
        assert_diff_plan(&old, &new, vec![]);

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
            vec![Action::copy(
                Path::parse("b").unwrap(),
                Path::parse("a.0").unwrap(),
            )],
        );
    }
}
