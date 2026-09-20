use crate::{Document, Error, ErrorKind, Message, Result, Value, message::{Action, PathSegment}};

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

    fn apply_copy(&self, current: &Value, path: &[PathSegment], from: &[PathSegment]) -> Result<Value> {
        let mut value = &self.document
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
