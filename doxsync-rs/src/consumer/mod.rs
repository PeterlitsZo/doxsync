use crate::message::{Action, PathSegment};
use crate::{ConsumerState, Document, Error, ErrorKind, Message, PackedMessage, Result, Value};

pub struct Consumer {
    state: Option<ConsumerState>,
    document: Option<Document>,
}

impl Consumer {
    pub fn new() -> Self {
        Self {
            state: Some(ConsumerState::new()),
            document: None,
        }
    }

    pub fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    /// Applies a packet.
    pub fn consume_diff(&mut self, diff: PackedMessage) -> Result<()> {
        let mut state_txn = self.state.take().expect("consumer state").txn();
        let result = Message::from_packed(diff, &mut state_txn)
            .and_then(|message| Self::apply_message(self.document.as_ref(), &message));

        match result {
            Ok(document) => {
                self.state = Some(state_txn.commit());
                self.document = document;
                Ok(())
            }
            Err(error) => {
                self.state = Some(state_txn.rollback());
                Err(error)
            }
        }
    }
}

impl Consumer {
    fn apply_message(document: Option<&Document>, message: &Message) -> Result<Option<Document>> {
        let actions = message.actions();

        // If the document is not yet initialized, the first action must be a
        // snapshot.
        if document.is_none() && !matches!(actions.first(), Some(Action::Snapshot { .. })) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "the first message's first action must be a snapshot",
            ));
        }

        // Consume the actions.
        let mut to_updated = document.cloned();
        for action in actions {
            match action {
                Action::Snapshot { value } => {
                    to_updated = Some(Document::new(value.clone()));
                }
                Action::Add { path, value } => {
                    let document = to_updated.as_ref().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "cannot add to an uninitialized document",
                    ))?;
                    let updated_value =
                        Self::add_at_path(&document.value(), path.segments(), value)?;
                    to_updated = Some(Document::new(updated_value));
                }
                Action::Delete { path } => {
                    let document = to_updated.as_ref().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "cannot delete from an uninitialized document",
                    ))?;
                    let updated_value = Self::delete_at_path(&document.value(), path.segments())?;
                    to_updated = Some(Document::new(updated_value));
                }
            }
        }
        Ok(to_updated)
    }

    fn add_at_path(current: &Value, path: &[PathSegment], value: &Value) -> Result<Value> {
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
                    let updated_child = Self::add_at_path(child, remaining_path, value)?;
                    map.insert(key.clone(), updated_child);
                }
                Ok(())
            }),
            PathSegment::Index(_) => Err(Error::new(
                ErrorKind::InvalidData,
                "index path segments are not supported",
            )),
        }
    }

    fn delete_at_path(current: &Value, path: &[PathSegment]) -> Result<Value> {
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
                    let updated_child = Self::delete_at_path(child, remaining_path)?;
                    map.insert(key.clone(), updated_child);
                }
                Ok(())
            }),
            PathSegment::Index(_) => Err(Error::new(
                ErrorKind::InvalidData,
                "index path segments are not supported",
            )),
        }
    }
}
