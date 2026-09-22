mod packed;

pub use packed::PackedMessage;

use crate::patch::Action;

use crate::{
    Result,
    state::{ConsumerStateTxn, ProducerStateTxn},
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

    /// Leaves successful changes in the transaction; errors restore its entry savepoint.
    pub(crate) fn decode(packed: PackedMessage, state_txn: &mut ConsumerStateTxn) -> Result<Self> {
        packed::PackedMessageDecoder::default().decode(packed, state_txn)
    }

    pub(crate) fn encode(&self, state_txn: &mut ProducerStateTxn) -> Result<PackedMessage> {
        packed::PackedMessageEncoder::default().encode(self, state_txn)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ConsumerState, ProducerState,
        patch::{Path, PathSegment},
        value,
    };

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
        let consumer_state = ConsumerState::default();
        let mut consumer_state_txn = consumer_state.txn();

        // Case 1:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: value!(42).unwrap(),
        }]);

        let packed = message.encode(&mut producer_state_txn).unwrap();
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 0c 2a"));

        let unpacked = Message::decode(packed, &mut consumer_state_txn).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: value!(42).unwrap()
            }]
        );

        // Case 2:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: value!(1).unwrap(),
        }]);

        let packed = message.encode(&mut producer_state_txn).unwrap();
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 01"));

        let unpacked = Message::decode(packed, &mut consumer_state_txn).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: value!(1).unwrap()
            },]
        );

        // Case 3:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: value!(0x1FFFFFF).unwrap(),
        }]);

        let packed = message.encode(&mut producer_state_txn).unwrap();
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 0E FF FF FF 01"));

        let unpacked = Message::decode(packed, &mut consumer_state_txn).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: value!(0x1FFFFFF).unwrap()
            }]
        );

        // Case 4:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: value!(-42).unwrap(),
        }]);

        let packed = message.encode(&mut producer_state_txn).unwrap();
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 1c 29"));

        let unpacked = Message::decode(packed, &mut consumer_state_txn).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: value!(-42).unwrap()
            }]
        );

        // Case 5:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: value!(-0x1FFFFFF).unwrap(),
        }]);

        let packed = message.encode(&mut producer_state_txn).unwrap();
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 1E FE FF FF 01"));

        let unpacked = Message::decode(packed, &mut consumer_state_txn).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: value!(-0x1FFFFFF).unwrap()
            }]
        );

        // Case 6:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: value!(3.1415926).unwrap(),
        }]);

        let packed = message.encode(&mut producer_state_txn).unwrap();
        assert_eq!(
            packed.bytes(),
            hex_to_bytes("00 01 00 7b 4a d8 12 4d fb 21 09 40")
        );

        let unpacked = Message::decode(packed, &mut consumer_state_txn).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: value!(3.1415926).unwrap()
            }]
        );

        // Case 7:
        // =====================================================================

        let value = value!({
            "answer": 42,
            "pi": 3.1415926,
        })
        .unwrap();
        let message = Message::new(vec![Action::Snapshot {
            value: value.clone(),
        }]);

        let packed = message.encode(&mut producer_state_txn).unwrap();
        assert_eq!(
            packed.bytes(),
            hex_to_bytes(indoc::indoc! { r#"
                01
                  + 00 02 // string pool patch, two entries
                      + 00 // slot 0 -> "answer"
                      |   + 06 61 6e 73 77 65 72
                      + 01 // slot 1 -> "pi"
                          + 02 70 69
                01
                  + 00
                      + 52
                          + 00 // ref to "answer"
                          + 0c 2a // value int(42)
                          + 04 // ref to "pi"
                          + 7b 4a d8 12 4d fb 21 09 40 // value float(3.1415926)
            "# })
        );

        let unpacked = Message::decode(packed, &mut consumer_state_txn).unwrap();
        assert_eq!(unpacked.actions, &[Action::Snapshot { value }]);

        // Case 8:
        // =====================================================================

        let value = value!({
            "answer": 42,
            "pi": "3.1415926",
        })
        .unwrap();
        let message = Message::new(vec![
            Action::Add {
                path: Path::new(vec![PathSegment::key("foo"), PathSegment::index(42)]),
                value: value,
            },
            Action::Delete {
                path: Path::new(vec![PathSegment::key("bar")]),
            },
        ]);

        let packed = message.encode(&mut producer_state_txn).unwrap();
        assert_eq!(
            packed.bytes(),
            hex_to_bytes(indoc::indoc! { r#"
                02
                  + 00 01 // string pool patch
                  |   + 02 // slot 2 -> "3.1415926"
                  |       + 09 33 2e 31 34 31 35 39 32 36
                  + 01 02 // path pool patch
                      + 00 02 // slot 0 -> path "foo.42"
                      |   + 0c 66 6f 6f
                      |   + a9 01
                      + 01 01 // slot 1 -> path "bar"
                          + 0c 62 61 72
                02
                  + 01 // Add
                  |   + 00 // ref to path "foo.42"
                  |   + 52 // the map value
                  |       + 00
                  |       + 0c 2a
                  |       + 04 // Key (ref to "pi")
                  |       + 82 // TStrRef value (ref to "3.1415926")
                  + 02 // Delete
                      + 01
            "# })
        );

        let unpacked = Message::decode(packed, &mut consumer_state_txn).unwrap();
        assert_eq!(unpacked, message);

        // Case 9:
        // =====================================================================

        let value = value!([null, false, true]).unwrap();
        let message = Message::new(vec![Action::Snapshot {
            value: value.clone(),
        }]);

        let packed = message.encode(&mut producer_state_txn).unwrap();
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 43 76 74 75"));

        let unpacked = Message::decode(packed, &mut consumer_state_txn).unwrap();
        assert_eq!(unpacked.actions, &[Action::Snapshot { value }]);

        // Case 10: COPY reuses a destination path and defines a nested source path.
        // =====================================================================

        let message = Message::new(vec![Action::Copy {
            path: Path::new(vec![PathSegment::key("bar")]),
            from: Path::new(vec![PathSegment::key("baz"), PathSegment::key("qux")]),
        }]);

        let packed = message.encode(&mut producer_state_txn).unwrap();
        assert_eq!(
            packed.bytes(),
            hex_to_bytes(indoc::indoc! { r#"
                01
                  + 01 01 // path pool patch, one entry
                      + 02 02 // slot 2 -> path "baz.qux"
                          + 0c 62 61 7a
                          + 0c 71 75 78
                01
                  + 03 // Copy
                      + 01 // destination: existing path "bar"
                      + 02 // source: new path "baz.qux"
            "# })
        );

        let unpacked = Message::decode(packed, &mut consumer_state_txn).unwrap();
        assert_eq!(unpacked, message);
        let _consumer_state = consumer_state_txn.commit();
    }
}
