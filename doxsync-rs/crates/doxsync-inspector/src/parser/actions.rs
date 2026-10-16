use super::limits;
use super::{Parser, Result, annotate};
use crate::Field;
use crate::{Action, ArrayEdit, Value};
const ACTIONS: [&str; 8] = [
    "snapshot",
    "add",
    "delete",
    "copy",
    "replace",
    "stringAppend",
    "stringPrepend",
    "arrayPatch",
];

impl Parser<'_> {
    pub(super) fn actions(&mut self, wire: &mut Vec<Field>) -> Result<Vec<Action>> {
        let count = self.count(wire, "Action count", limits::ACTIONS)?;
        annotate(wire, format!("{count} actions"), 0);
        let mut actions = vec![];
        for i in 0..count {
            let start = self.cursor.pos;
            let mut fields = vec![];
            let opcode = usize::try_from(self.var(&mut fields, "Action opcode", 64, false)?)
                .map_err(|_| self.error("Action opcode overflow"))?;
            let name = *ACTIONS
                .get(opcode)
                .ok_or_else(|| self.error("Unknown action opcode"))?;
            fields[0]
                .explanation
                .push_str(&format!("; {opcode} = {name}"));
            let path = if opcode > 0 {
                self.path_ref(&mut fields, "Target path slot")?
            } else {
                vec![]
            };
            let action = match opcode {
                0 => Action::Snapshot {
                    value: self.value(&mut fields, "Payload document", 0)?,
                },
                1 => Action::Add {
                    path,
                    value: self.value(&mut fields, "Payload document", 0)?,
                },
                2 => Action::Delete { path },
                3 => Action::Copy {
                    path,
                    from: self.path_ref(&mut fields, "Source path slot")?,
                },
                4 => Action::Replace {
                    path,
                    value: self.value(&mut fields, "Payload document", 0)?,
                },
                5 | 6 => {
                    self.require_protocol(3)?;
                    let value = self.value(&mut fields, "String fragment", 0)?;
                    let Value::String(value) = value else {
                        return self.fail("Expected string fragment");
                    };
                    if opcode == 5 {
                        Action::StringAppend { path, value }
                    } else {
                        Action::StringPrepend { path, value }
                    }
                }
                7 => {
                    self.require_protocol(3)?;
                    let n = self.count(&mut fields, "Edit count", self.cursor.bytes.len() / 3)?;
                    let mut edits = vec![];
                    let mut previous_start = None;
                    for j in 0..n {
                        let edit_start = self.cursor.pos;
                        let mut ef = vec![];
                        let start = self.var(&mut ef, "Start index (original array)", 64, false)?;
                        let delete = self.var(&mut ef, "Delete count", 64, false)?;
                        if start + delete > usize::MAX as u128 {
                            return self.fail("Array patch range overflow");
                        }
                        if previous_start.is_some_and(|prev| start >= prev || start + delete > prev)
                        {
                            return self.fail("Array edits overlap or are not strictly descending");
                        }
                        previous_start = Some(start);
                        let n =
                            self.count(&mut ef, "Inserted value count", self.cursor.bytes.len())?;
                        let mut values = vec![];
                        for k in 0..n {
                            values.push(self.value(&mut ef, &format!("Inserted value {k}"), 0)?);
                        }
                        edits.push(ArrayEdit {
                            start: start as u64,
                            delete_count: delete as u64,
                            values,
                        });
                        self.group(
                            &mut fields,
                            edit_start,
                            format!("Edit {j}"),
                            "Indexes refer to the array before this action".into(),
                            ef,
                        )?;
                    }
                    Action::ArrayPatch { path, edits }
                }
                _ => unreachable!(),
            };
            actions.push(action);
            self.group(
                wire,
                start,
                format!("Action {} · {name}", i + 1),
                if opcode == 0 {
                    "Snapshot has one payload document and no target path"
                } else {
                    "Fields appear in wire order"
                }
                .into(),
                fields,
            )?;
            annotate(wire, format!("Action {}: {name}", i + 1), 1);
        }
        Ok(actions)
    }
}
