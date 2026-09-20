use crate::{ConsumerState, Document, Message, PackedMessage, Result};

mod applier;

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

    /// Applies a message.
    pub fn consume_diff(&mut self, diff: PackedMessage) -> Result<()> {
        let mut state_txn = self.state.take().expect("consumer state").txn();
        let result = Message::decode(diff, &mut state_txn)
            .and_then(|message| {
                applier::Applier::new(self.document.as_ref()).apply(&message)
            });

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
