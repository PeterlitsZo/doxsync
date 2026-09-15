use crate::{Document, Error, ErrorKind, Message, Result, message::Action};

pub struct Consumer {
    document: Option<Document>,
}

impl Consumer {
    pub fn new() -> Self {
        Self { document: None }
    }

    pub fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    pub fn consume_diff(&mut self, diff: Message) -> Result<()> {
        let actions = diff.actions();

        // If the document is not yet initialized, the first action must be a
        // snapshot.
        if self.document.is_none() && !matches!(actions.first(), Some(Action::Snapshot { .. })) {
            return Err(Error::new(ErrorKind::InvalidData, "The first message's first action must be a snapshot"));
        }

        // Consume the actions.
        for action in actions {
            match action {
                Action::Snapshot { value } => {
                    self.document = Some(Document::new(value.clone()));
                }
            }
        }

        Ok(())
    }
}
