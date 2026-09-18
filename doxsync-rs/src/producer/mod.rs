use crate::message::Action;
use crate::{Document, Message, PackedMessage, ProducerState, Result};

mod differ;
use differ::Differ;

pub struct Producer {
    state: Option<ProducerState>,
    current_document: Document,
    last_emited_document: Option<Document>,
}

impl Producer {
    pub fn new(current_document: Document) -> Self {
        Self {
            state: Some(ProducerState::new()),
            current_document,
            last_emited_document: None,
        }
    }

    pub fn replace(&mut self, new_document: Document) {
        self.current_document = new_document;
    }

    pub fn pack_diff(&mut self, diff: Message) -> PackedMessage {
        let mut state = None;
        std::mem::swap(&mut state, &mut self.state);
        let state = state.expect("state is unexpected None");
        let mut state_txn = state.txn();

        let d = diff.packed(&mut state_txn);

        let state = state_txn.commit();
        self.state = Some(state);

        d
    }

    pub fn produce_diff(&mut self) -> Result<Option<PackedMessage>> {
        let diff = self.produce_diff_unpacked()?;
        match diff {
            Some(d) => {
                let packed = self.pack_diff(d);
                Ok(Some(packed))
            }
            None => Ok(None),
        }
    }

    pub fn produce_diff_unpacked(&mut self) -> Result<Option<Message>> {
        match self.last_emited_document {
            Some(ref last) if self.current_document == *last => Ok(None),
            None => {
                let message = Message::new(vec![Action::Snapshot {
                    value: self.current_document.value(),
                }]);
                self.last_emited_document = Some(self.current_document.clone());
                Ok(Some(message))
            }
            Some(ref last) => {
                let mut state = None;
                std::mem::swap(&mut state, &mut self.state);
                let state = state.expect("state is unexpected None");
                let mut state_txn = state.txn();

                let message = Differ::new(&mut state_txn).diff(last, &self.current_document);

                let state = state_txn.rollback();
                self.state = Some(state);

                self.last_emited_document = Some(self.current_document.clone());
                Ok(Some(message))
            }
        }
    }
}
