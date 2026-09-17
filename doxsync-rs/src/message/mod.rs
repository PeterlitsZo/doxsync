mod action;
mod packed;

pub use packed::PackedMessage;

pub(crate) use action::{Action, Path, PathSegment};

use crate::{
    Result,
    state::{ConsumerState, ProducerStateTxn},
};

/// A structured doxsync message.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    actions: Vec<Action>,
}

impl Message {
    pub(crate) fn new(actions: Vec<Action>) -> Self {
        Self { actions }
    }

    pub(crate) fn actions(&self) -> &[Action] {
        &self.actions
    }

    pub(crate) fn from_packed(packed: PackedMessage, state: &mut ConsumerState) -> Result<Self> {
        packed::PackedMessageDecoder::default().decode(packed.bytes(), state)
    }

    pub(crate) fn packed(&self, state_txn: &mut ProducerStateTxn) -> PackedMessage {
        packed::PackedMessageBuilder::default()
            .with_actions(&self.actions)
            .with_state_txn(state_txn)
            .build()
            .expect("packing message failed")
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use crate::{ProducerState, Value};

    use super::*;

    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        let mut hex_post = String::new();
        let hex_chars: Vec<_> = hex.chars().collect();
        let mut i = 0;
        loop {
            let byte = hex_chars[i];
            match byte {
                ' ' | '\n' | '+' | '|' | '-' => {}
                '/' => {
                    // Comment begin with '//'...
                    i += 1;
                    if i >= hex_chars.len() {
                        break;
                    }
                    let next = hex_chars[i];
                    if next != '/' {
                        panic!("expected '//' comment to end at index {}", i);
                    }
                    i += 1;
                    while i < hex_chars.len() {
                        let byte = hex_chars[i];
                        if byte == '\n' {
                            break;
                        }
                        i += 1;
                    }
                    continue;
                }
                _ => hex_post.push(byte),
            }
            i += 1;
            if i >= hex_chars.len() {
                break;
            }
        }
        hex::decode(hex_post).unwrap()
    }

    #[test]
    fn test_pack_and_unpack() {
        let producer_state = ProducerState::default();
        let mut producer_state_txn = producer_state.txn();
        let mut consumer_state = ConsumerState::default();

        // Case 1:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::int(42).unwrap(),
        }]);

        let packed = message.packed(&mut producer_state_txn);
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 0c 2a"));

        let unpacked = Message::from_packed(packed, &mut consumer_state).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::int(42).unwrap()
            },]
        );

        // Case 2:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::int(1).unwrap(),
        }]);

        let packed = message.packed(&mut producer_state_txn);
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 01"));

        let unpacked = Message::from_packed(packed, &mut consumer_state).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::int(1).unwrap()
            },]
        );

        // Case 3:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::int(0x1FFFFFF).unwrap(),
        }]);

        let packed = message.packed(&mut producer_state_txn);
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 0E FF FF FF 01"));

        let unpacked = Message::from_packed(packed, &mut consumer_state).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::int(0x1FFFFFF).unwrap()
            },]
        );

        // Case 4:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::int(-42).unwrap(),
        }]);

        let packed = message.packed(&mut producer_state_txn);
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 1c 29"));

        let unpacked = Message::from_packed(packed, &mut consumer_state).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::int(-42).unwrap()
            },]
        );

        // Case 5:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::int(-0x1FFFFFF).unwrap(),
        }]);

        let packed = message.packed(&mut producer_state_txn);
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 1E FE FF FF 01"));

        let unpacked = Message::from_packed(packed, &mut consumer_state).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::int(-0x1FFFFFF).unwrap()
            },]
        );

        // Case 6:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::float(3.1415926).unwrap(),
        }]);

        let packed = message.packed(&mut producer_state_txn);
        assert_eq!(
            packed.bytes(),
            hex_to_bytes("00 01 00 7b 4a d8 12 4d fb 21 09 40")
        );

        let unpacked = Message::from_packed(packed, &mut consumer_state).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::float(3.1415926).unwrap()
            },]
        );

        // Case 7:
        // =====================================================================

        let mut map = BTreeMap::new();
        map.insert(Arc::new("answer".to_owned()), Value::int(42).unwrap());
        map.insert(Arc::new("pi".to_owned()), Value::float(3.1415926).unwrap());
        let value = Value::map(map).unwrap();
        let message = Message::new(vec![Action::Snapshot {
            value: value.clone(),
        }]);

        let packed = message.packed(&mut producer_state_txn);
        assert_eq!(
            packed.bytes(),
            hex_to_bytes(indoc::indoc! { r#"
                01
                  + 00 // string pool patch
                      + 02
                          + 00 // index 0
                          + 06 61 6e 73 77 65 72 // "answer"
                          + 01 // index 1
                          + 02 70 69 // "pi"
                01
                  + 00
                      + 52
                          + 00 // index of "answer"
                          + 0c 2a // value int(42)
                          + 01 // index of "pi"
                          + 7b 4a d8 12 4d fb 21 09 40 // value float(3.1415926)
            "# })
        );

        let unpacked = Message::from_packed(packed, &mut consumer_state).unwrap();
        assert_eq!(unpacked.actions, &[Action::Snapshot { value }]);

        // Case 8:
        // =====================================================================

        let mut map = BTreeMap::new();
        map.insert(Arc::new("answer".to_owned()), Value::int(42).unwrap());
        map.insert(Arc::new("pi".to_owned()), Value::tstr("3.1415926").unwrap());
        let value = Value::map(map).unwrap();
        let message = Message::new(vec![
            Action::Add {
                path: Path::new(vec![PathSegment::key("foo"), PathSegment::index(42)]),
                value: value,
            },
            Action::Delete {
                path: Path::new(vec![PathSegment::key("bar")]),
            },
        ]);

        let packed = message.packed(&mut producer_state_txn);
        assert_eq!(
            packed.bytes(),
            hex_to_bytes(indoc::indoc! { r#"
                00
                02
                  + 01
                  |   + 02 // path "foo.42"
                  |   |   + 0c 66 6f 6f
                  |   |   + a9 01
                  |   + 52 // value map
                  |       + 00 // index of "answer"
                  |       + 0c 2a // value int(42)
                  |       + 01 // index of "pi"
                  |       + 39 33 2e 31 34 31 35 39 32 36 // value tstr("3.1415926")
                  + 02
                      + 01 // path "bar"
                          + 0c 62 61 72
            "# })
        );

        let unpacked = Message::from_packed(packed, &mut consumer_state).unwrap();
        assert_eq!(unpacked, message);
    }
}
