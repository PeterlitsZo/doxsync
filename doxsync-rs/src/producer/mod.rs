use crate::message::{Action, Path, PathSegment};
use crate::{Document, Message, Result, State, ValueKind};

pub struct Producer {
    state: State,
    current_document: Document,
    last_emited_document: Option<Document>,
}

impl Producer {
    pub fn new(current_document: Document) -> Self {
        Self {
            state: State::new(),
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
                let message = Message::new(vec![Action::Snapshot {
                    value: self.current_document.value(),
                }]);
                self.last_emited_document = Some(self.current_document.clone());
                Ok(Some(message))
            }
            Some(ref last) => {
                let last_value = last.value();
                let curr_value = self.current_document.value();
                if last_value.kind() == curr_value.kind() && curr_value.kind() == ValueKind::Map {
                    let last_map = last_value.as_map().expect("must be map");
                    let curr_map = curr_value.as_map().expect("must be map");
                    let mut actions = vec![];
                    for (key, value) in curr_map.iter() {
                        if let Some(last_value) = last_map.get(key) {
                            if last_value != value {
                                actions.push(Action::Add {
                                    path: Path::new(vec![PathSegment::key_arc(key.clone())]),
                                    value: value.clone(),
                                });
                            }
                        } else {
                            actions.push(Action::Add {
                                path: Path::new(vec![PathSegment::key_arc(key.clone())]),
                                value: value.clone(),
                            });
                        }
                    }
                    for (key, _) in last_map.iter() {
                        if !curr_map.contains_key(key) {
                            actions.push(Action::Delete {
                                path: Path::new(vec![PathSegment::key_arc(key.clone())]),
                            });
                        }
                    }
                    let message = Message::new(actions);
                    self.last_emited_document = Some(self.current_document.clone());
                    return Ok(Some(message));
                }

                let message = Message::new(vec![Action::Snapshot {
                    value: self.current_document.value(),
                }]);
                self.last_emited_document = Some(self.current_document.clone());
                Ok(Some(message))
            }
        }
    }
}
