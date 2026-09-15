use crate::{Document, Message, Result, message::Action};

pub struct Producer {
    current_document: Document,
    last_emited_document: Option<Document>,
}

impl Producer {
    pub fn new(current_document: Document) -> Self {
        Self {
            current_document,
            last_emited_document: None,
        }
    }

    pub fn replace(&mut self, new_document: Document) {
        self.current_document = new_document;
    }

    pub fn produce_diff(&mut self) -> Result<Option<Message>> {
        match self.last_emited_document {
            Some(ref last) if self.current_document == *last => Ok(None),
            None => {
                let message = Message::new(vec![
                    Action::Snapshot { value: self.current_document.value() }
                ]);
                self.last_emited_document = Some(self.current_document.clone());
                Ok(Some(message))
            }
            Some(_) => {
                let message = Message::new(vec![
                    Action::Snapshot { value: self.current_document.value() }
                ]);
                self.last_emited_document = Some(self.current_document.clone());
                Ok(Some(message))
            }
        }
    }
}
