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

    /// Packs a message.
    ///
    /// Internal state will be updated on success
    pub fn pack_diff(&mut self, diff: Message) -> Result<PackedMessage> {
        let mut txn = self.state.take().expect("producer state").txn();
        let result = diff.packed(&mut txn);
        self.state = Some(if result.is_ok() {
            txn.commit()
        } else {
            txn.rollback()
        });
        result
    }

    /// Packs an encodable change selected by estimated cost. On failure the baseline and pools
    /// remain unchanged, so the same update can be retried.
    pub fn produce_diff(&mut self) -> Result<Option<PackedMessage>> {
        let Some(message) = self.next_message()? else {
            return Ok(None);
        };
        let packed = self.pack_diff(message)?;
        self.last_emited_document = Some(self.current_document.clone());
        Ok(Some(packed))
    }

    /// Produces a structured message and advances the document baseline.
    /// Cost estimation and resource validation leave pools unchanged. Pack and deliver
    /// each returned message before requesting another; retain a clone for retry
    /// if packing fails. Prefer `produce_diff` for atomic baseline advancement.
    pub fn produce_diff_unpacked(&mut self) -> Result<Option<Message>> {
        let message = self.next_message()?;
        if message.is_some() {
            self.last_emited_document = Some(self.current_document.clone());
        }
        Ok(message)
    }

    fn next_message(&mut self) -> Result<Option<Message>> {
        if self.last_emited_document.as_ref() == Some(&self.current_document) {
            return Ok(None);
        }
        let txn = self.state.take().expect("producer state").txn();
        let result = match &self.last_emited_document {
            Some(last) => Differ::new(&txn).diff(last, &self.current_document),
            None => {
                let message = Message::new(vec![Action::Snapshot {
                    value: self.current_document.value(),
                }]);
                message.validate(&txn).map(|_| message)
            }
        };
        self.state = Some(txn.rollback());
        result.map(Some)
    }
}
