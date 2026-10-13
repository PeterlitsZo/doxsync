use crate::{
    Document, Error, ErrorKind, Message, Result, Value,
    patch::{Action, ArrayPatchEdit, PathSegment},
};

pub(super) struct Applier<'d> {
    document: Option<&'d Document>,
}

impl<'d> Applier<'d> {
    pub(super) fn new(document: Option<&'d Document>) -> Self {
        Self { document }
    }

    pub(super) fn apply(&self, message: &Message) -> Result<Option<Document>> {
        let actions = message.actions();

        // If the document is not yet initialized, the first action must be a
        // snapshot.
        if self.document.is_none() && !matches!(actions.first(), Some(Action::Snapshot { .. })) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "the first message's first action must be a snapshot",
            ));
        }

        // Consume the actions.
        let mut to_updated = self.document.map(|d| d.value());
        for action in actions {
            match action {
                Action::Snapshot { value } => {
                    to_updated = Some(value.clone());
                }
                Action::Add { path, value } => {
                    let current = to_updated.as_ref().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "cannot add to an uninitialized document",
                    ))?;
                    let updated_value = self.apply_add(current, path.segments(), value)?;
                    to_updated = Some(updated_value);
                }
                Action::Replace { path, value } => {
                    let current = to_updated.as_ref().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "cannot replace in an uninitialized document",
                    ))?;
                    to_updated = Some(self.apply_replace(current, path.segments(), value)?);
                }
                Action::TStrAppend { path, value } | Action::TStrPrepend { path, value } => {
                    let current = to_updated.as_ref().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "cannot modify a string in an uninitialized document",
                    ))?;
                    to_updated = Some(self.apply_tstr(
                        current,
                        path.segments(),
                        value,
                        matches!(action, Action::TStrPrepend { .. }),
                    )?);
                }
                Action::ArrayPatch { path, edits } => {
                    let current = to_updated.as_ref().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "cannot patch an uninitialized document",
                    ))?;
                    ArrayPatchEdit::validate(edits)?;
                    to_updated = Some(self.apply_array_patch(current, path.segments(), edits)?);
                }
                Action::Delete { path } => {
                    let value = to_updated.as_ref().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "cannot delete from an uninitialized document",
                    ))?;
                    let updated_value = self.apply_delete(value, path.segments())?;
                    to_updated = Some(updated_value);
                }
                Action::Copy { path, from } => {
                    let current = to_updated.as_ref().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "cannot copy in an uninitialized document",
                    ))?;
                    let updated_value =
                        self.apply_copy(current, path.segments(), from.segments())?;
                    to_updated = Some(updated_value);
                }
            }
        }
        Ok(to_updated.map(Document::new))
    }

    fn apply_add(&self, current: &Value, path: &[PathSegment], value: &Value) -> Result<Value> {
        if path.is_empty() {
            return Ok(value.clone());
        }

        let (segment, remaining_path) = path.split_first().ok_or(Error::new(
            ErrorKind::InvalidData,
            "add path must not be empty",
        ))?;

        match segment {
            PathSegment::Key(key) => current.as_map_and_modify(|map| {
                if remaining_path.is_empty() {
                    map.insert(key.clone(), value.clone());
                } else {
                    let child = map.get(key).ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "add path does not exist",
                    ))?;
                    let updated_child = self.apply_add(child, remaining_path, value)?;
                    map.insert(key.clone(), updated_child);
                }
                Ok(())
            }),
            PathSegment::Index(idx) => current.as_array_and_modify(|arr| {
                if remaining_path.is_empty() {
                    arr.insert(*idx, value.clone());
                } else {
                    let child = arr.get(*idx).ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "add path does not exist",
                    ))?;
                    let updated_child = self.apply_add(child, remaining_path, value)?;
                    arr[*idx] = updated_child;
                }
                Ok(())
            }),
        }
    }

    fn apply_replace(&self, current: &Value, path: &[PathSegment], value: &Value) -> Result<Value> {
        let Some((segment, remaining_path)) = path.split_first() else {
            return Ok(value.clone());
        };

        match segment {
            PathSegment::Key(key) => current.as_map_and_modify(|map| {
                let child = map.get_mut(key).ok_or(Error::new(
                    ErrorKind::InvalidData,
                    "replace path does not exist",
                ))?;
                *child = self.apply_replace(child, remaining_path, value)?;
                Ok(())
            }),
            PathSegment::Index(idx) => current.as_array_and_modify(|arr| {
                let child = arr.get_mut(*idx).ok_or(Error::new(
                    ErrorKind::InvalidData,
                    "replace path does not exist",
                ))?;
                *child = self.apply_replace(child, remaining_path, value)?;
                Ok(())
            }),
        }
    }

    fn apply_array_patch(
        &self,
        current: &Value,
        path: &[PathSegment],
        edits: &[ArrayPatchEdit],
    ) -> Result<Value> {
        let Some((segment, remaining)) = path.split_first() else {
            let original = current.as_array().ok_or(Error::new(
                ErrorKind::UnexpectedType,
                "array patch target must be an array",
            ))?;
            let mut len = original.len();
            for edit in edits {
                if edit.start > original.len() || edit.delete_count > original.len() - edit.start {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "array patch range out of bounds",
                    ));
                }
                len = len
                    .checked_sub(edit.delete_count)
                    .and_then(|len| len.checked_add(edit.values.len()))
                    .ok_or_else(|| {
                        Error::new(ErrorKind::InvalidData, "array patch length overflow")
                    })?;
            }
            if edits.is_empty() {
                return Ok(current.clone());
            }
            let mut result = Vec::new();
            result.try_reserve_exact(len).map_err(|_| {
                Error::new(ErrorKind::InvalidData, "array patch allocation too large")
            })?;
            let mut cursor = 0;
            for edit in edits.iter().rev() {
                result.extend_from_slice(&original[cursor..edit.start]);
                result.extend_from_slice(&edit.values);
                cursor = edit.start + edit.delete_count;
            }
            result.extend_from_slice(&original[cursor..]);
            return Value::array(result);
        };
        match segment {
            PathSegment::Key(key) => current.as_map_and_modify(|map| {
                let child = map.get_mut(key).ok_or(Error::new(
                    ErrorKind::InvalidData,
                    "array patch target does not exist",
                ))?;
                *child = self.apply_array_patch(child, remaining, edits)?;
                Ok(())
            }),
            PathSegment::Index(index) => current.as_array_and_modify(|array| {
                let child = array.get_mut(*index).ok_or(Error::new(
                    ErrorKind::InvalidData,
                    "array patch target does not exist",
                ))?;
                *child = self.apply_array_patch(child, remaining, edits)?;
                Ok(())
            }),
        }
    }

    fn apply_tstr(
        &self,
        current: &Value,
        path: &[PathSegment],
        fragment: &str,
        prepend: bool,
    ) -> Result<Value> {
        let Some((segment, remaining)) = path.split_first() else {
            return current.as_tstr_and_modify(|string| {
                if prepend {
                    string.insert_str(0, fragment);
                } else {
                    string.push_str(fragment);
                }
                Ok(())
            });
        };
        match segment {
            PathSegment::Key(key) => current.as_map_and_modify(|map| {
                let child = map.get_mut(key).ok_or(Error::new(
                    ErrorKind::InvalidData,
                    "string target does not exist",
                ))?;
                *child = self.apply_tstr(child, remaining, fragment, prepend)?;
                Ok(())
            }),
            PathSegment::Index(index) => current.as_array_and_modify(|array| {
                let child = array.get_mut(*index).ok_or(Error::new(
                    ErrorKind::InvalidData,
                    "string target does not exist",
                ))?;
                *child = self.apply_tstr(child, remaining, fragment, prepend)?;
                Ok(())
            }),
        }
    }

    fn apply_copy(
        &self,
        current: &Value,
        path: &[PathSegment],
        from: &[PathSegment],
    ) -> Result<Value> {
        let mut value = &self
            .document
            .ok_or(Error::new(
                ErrorKind::InvalidData,
                "copy source path does not exist",
            ))?
            .value();
        for segment in from {
            value = match segment {
                PathSegment::Key(key) => value
                    .as_map()
                    .ok_or(Error::new(
                        ErrorKind::UnexpectedType,
                        "expected a map value",
                    ))?
                    .get(key)
                    .ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "copy source path does not exist",
                    ))?,
                PathSegment::Index(idx) => value
                    .as_array()
                    .ok_or(Error::new(
                        ErrorKind::UnexpectedType,
                        "expected a array value",
                    ))?
                    .get(*idx)
                    .ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "copy source path does not exist",
                    ))?,
            };
        }
        self.apply_add(current, path, value)
    }

    fn apply_delete(&self, current: &Value, path: &[PathSegment]) -> Result<Value> {
        let (segment, remaining_path) = path.split_first().ok_or(Error::new(
            ErrorKind::InvalidData,
            "delete path must not be empty",
        ))?;

        match segment {
            PathSegment::Key(key) => current.as_map_and_modify(|map| {
                if remaining_path.is_empty() {
                    map.remove(key).ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "delete path does not exist",
                    ))?;
                } else {
                    let child = map.get(key).ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "delete path does not exist",
                    ))?;
                    let updated_child = self.apply_delete(child, remaining_path)?;
                    map.insert(key.clone(), updated_child);
                }
                Ok(())
            }),
            PathSegment::Index(idx) => current.as_array_and_modify(|arr| {
                let child = arr.get(*idx).ok_or(Error::new(
                    ErrorKind::InvalidData,
                    "delete path does not exist",
                ))?;
                if remaining_path.is_empty() {
                    arr.remove(*idx);
                } else {
                    let updated_child = self.apply_delete(child, remaining_path)?;
                    arr[*idx] = updated_child;
                }
                Ok(())
            }),
        }
    }
}
